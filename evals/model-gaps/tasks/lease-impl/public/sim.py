"""Deterministic simulator for the lease lock in DESIGN.md.

`lease.py` defines `LockService`, `Storage` and `Client` as subclasses of `Node`. The
simulator creates the nodes, delivers their messages, fires their timers, injects
faults and writes the event log.

Time is an integer tick shared by every node. Everything is single-threaded: one
handler runs at a time and runs to completion; a crash happens between handlers.

Example:

    from sim import Sim
    from lease import LockService, Storage, Client

    s = Sim(LockService, Storage, Client, name="two_clients", seed=1)
    s.add_client(1)
    s.add_client(2, max_attempts=3, at=2)
    result = s.run()
    print(result.outcomes)      # {1: "Wrote", 2: "Wrote"}
    print(result.log_path)      # logs/two_clients.jsonl
"""
from __future__ import annotations

import copy
import heapq
import json
import random
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable

TTL = 30         # a lease lasts this many ticks from its grant, unless released first
RETRY = 5        # a client repeats an unanswered request after this many ticks
BACKOFF = 10     # a client told "busy" waits this many ticks before asking again
MAX_DELAY = 3    # a message is normally delivered within 1..MAX_DELAY ticks
HORIZON = 3000   # a run ends at this tick
MAX_EVENTS = 300_000

LOCK = "lock"          # the lock service's node id
STORAGE = "storage"    # the storage service's node id

BEHAVIORS = ("Normal", "Pauses", "Crashes")
OUTCOMES = ("Wrote", "Fenced", "GaveUp", "Crashed")

# Contract events (model/PSpec/Contract.p): who may emit each, and its payload fields.
_EMITTERS = {
    "Granted": (LOCK, ("id", "token")),
    "Busy": (LOCK, None),
    "Expired": (LOCK, None),
    "Released": (LOCK, None),
    "WriteAccepted": (STORAGE, ("id", "token")),
    "WriteRejected": (STORAGE, ("id", "token")),
    "ClientDone": ("client", ("id", "outcome")),
}


class SimError(Exception):
    """The simulator was used wrongly, or the run did not stay within its limits."""


def client_node(client_id: int) -> str:
    """The node id of client `client_id`: 1 -> "c1"."""
    return f"c{client_id}"


class Node:
    """Base class for LockService, Storage and Client. Do not override these methods."""

    def __init__(self, sim: "Sim", node_id: str):
        self._sim = sim
        self.id = node_id                      # "lock", "storage", "c1", "c2", ...
        # Survives a crash and restart of this node. Everything else on the object is
        # lost: a restarted node is a new object. Keep plain data in it.
        self.disk: dict = sim._disks.setdefault(node_id, {})

    def now(self) -> int:
        """The current tick."""
        return self._sim.time

    def send(self, dst: str, msg: Any) -> None:
        """Send a message to another node ("lock", "storage", "c1", ...). The message
        is copied; the network may lose, duplicate, reorder or delay it."""
        self._sim._send(self.id, dst, msg)

    def set_timer(self, delay: int, tag: Any = None) -> None:
        """Call `self.on_timer(tag)` after `delay` (>= 1) ticks, unless this node crashes
        first. Timers cannot be cancelled; use the tag to ignore stale ones."""
        self._sim._timer(self.id, max(1, int(delay)), tag)

    def emit(self, event: str, payload: Any) -> None:
        """Record a contract event in the run's log, e.g.
        `self.emit("Granted", {"id": 1, "token": 3})` or `self.emit("Busy", 2)`.
        Payloads are as declared in model/PSpec/Contract.p and model/PSrc/Client.p."""
        self._sim._emit(self.id, event, payload)

    # Override these.
    def on_start(self) -> None:
        """Called once when the node starts, and again (on a new object) after each
        restart."""

    def on_message(self, src: str, msg: Any) -> None:
        """Called when a message arrives. `src` is the sender's node id."""

    def on_timer(self, tag: Any) -> None:
        """Called when a timer set with `set_timer` fires."""


class ClientBase(Node):
    """Client's base: the plan the driver gave this client."""

    def __init__(self, sim: "Sim", node_id: str, client_id: int, writes: int, max_attempts: int):
        super().__init__(sim, node_id)
        self.client_id = client_id          # the `id` in every contract event about this client
        self.writes = writes                # how many writes to make once granted (>= 1)
        self.max_attempts = max_attempts    # how many times to ask for the lock (>= 1)

    def done(self, outcome: str) -> None:
        """Report how this client ended: "Wrote", "Fenced" or "GaveUp". Emits ClientDone."""
        self.emit("ClientDone", {"id": self.client_id, "outcome": outcome})


@dataclass
class Envelope:
    """A message in flight, as seen by a network policy."""
    src: str
    dst: str
    msg: Any
    sent_at: int


@dataclass(order=True)
class _Event:
    time: int
    seq: int
    kind: str = field(compare=False)
    node: str = field(compare=False)
    data: Any = field(compare=False, default=None)
    incarnation: int = field(compare=False, default=0)


@dataclass
class Result:
    name: str
    log_path: str
    outcomes: dict            # client id -> outcome, for clients that ended
    unfinished: list          # client ids that never reported an outcome
    events: list              # the logged events, in order: (tick, event, payload)


Policy = Callable[["Sim", Envelope], list]


def healthy(sim: "Sim", env: Envelope) -> list:
    """Default network: each message delivered once after 1..MAX_DELAY ticks."""
    return [sim.rng.randint(1, MAX_DELAY)]


def lossy(loss=0.2, dup=0.2, slow=0.1, slow_max=60) -> Policy:
    """A faulty network: drops, duplicates and sometimes delays messages a lot (which
    also reorders them)."""
    def policy(sim: "Sim", env: Envelope) -> list:
        if sim.rng.random() < loss:
            return []
        delays = [sim.rng.randint(1, MAX_DELAY)]
        if sim.rng.random() < slow:
            delays = [sim.rng.randint(MAX_DELAY, slow_max)]
        if sim.rng.random() < dup:
            delays.append(sim.rng.randint(1, slow_max))
        return delays
    return policy


class Sim:
    """Runs one scenario.

    Sim(lock_cls, storage_cls, client_cls, name, seed=0, policy=healthy, log_dir="logs")

    Before `run`:
    - `add_client(id, writes=1, max_attempts=1, behavior="Normal", at=0, pause=0)`:
      start client `id` at tick `at`. Behavior "Pauses" freezes the client for `pause`
      ticks from the moment it is granted the lock (a GC pause: its messages and timers
      wait until it resumes); "Crashes" kills it at that moment, for good.
    - `crash(node, at)`, `restart(node, at)`: for "lock" and "storage". A crashed node
      gets no messages and no timers; a restarted one is a new object with the old
      `disk`.
    - `pause(node, at, ticks)`: freeze a client for a while.
    - `set_policy(policy, at=0)`: change the network. A policy is a function
      `(sim, envelope) -> [delay, ...]`, one delivery per delay; `[]` loses the message.
      See `healthy` and `lossy`. It may read `sim.time` and `sim.events`.
    - `at(tick, fn)`: call `fn(sim)` at a tick.

    `run(until=HORIZON)` runs the scenario, writes `<log_dir>/<name>.jsonl` and returns
    a `Result`.
    """

    def __init__(self, lock_cls, storage_cls, client_cls, name: str, seed: int = 0,
                 policy: Policy = healthy, log_dir: str | Path = "logs"):
        self.name = name
        self.seed = seed
        self.rng = random.Random(f"net/{seed}")
        self.time = 0
        self._classes = {LOCK: lock_cls, STORAGE: storage_cls}
        self._client_cls = client_cls
        self.policy = policy
        self.log_dir = Path(log_dir)
        self.nodes: dict[str, Node] = {}
        self.plans: dict[int, dict] = {}
        self.events: list[tuple] = []          # (tick, event, payload) logged so far
        self.outcomes: dict[int, str] = {}
        self._disks: dict[str, dict] = {}
        self._incarnations: dict[str, int] = {}
        self._paused: dict[str, int] = {}      # node -> tick it resumes
        self._dead: set[str] = set()           # clients crashed for good
        self._queue: list[_Event] = []
        self._seq = 0
        self._ran = False

    # ---- scenario controls
    def add_client(self, client_id: int, writes: int = 1, max_attempts: int = 1,
                   behavior: str = "Normal", at: int = 0, pause: int = 0):
        if self._ran:
            raise SimError("add clients before run()")
        if behavior not in BEHAVIORS or writes < 1 or max_attempts < 1 or client_id in self.plans:
            raise SimError(f"bad plan for client {client_id}")
        self.plans[client_id] = {"id": client_id, "behavior": behavior, "writes": writes,
                                 "maxAttempts": max_attempts, "pause": pause}
        self._push(at, "start_client", client_node(client_id), client_id)

    def crash(self, node: str, at: int):
        self._server(node)
        self._push(at, "crash", node)

    def restart(self, node: str, at: int):
        self._server(node)
        self._push(at, "restart", node)

    def pause(self, node: str, at: int, ticks: int):
        self._push(at, "pause", node, ticks)

    def set_policy(self, policy: Policy, at: int = 0):
        self._push(at, "policy", "", policy)

    def at(self, t: int, fn: Callable[["Sim"], None]):
        self._push(t, "call", "", fn)

    def up(self, node: str) -> bool:
        return node in self.nodes

    def run(self, until: int = HORIZON) -> Result:
        if self._ran:
            raise SimError("run() can be called once")
        if not self.plans:
            raise SimError("add at least one client")
        self._ran = True
        self._log("ClientsPlanned", len(self.plans))
        for nid in (LOCK, STORAGE):
            self._boot(nid)
        handled = 0
        while self._queue and self._queue[0].time <= until:
            ev = heapq.heappop(self._queue)
            self.time = ev.time
            self._dispatch(ev)
            handled += 1
            if handled > MAX_EVENTS:
                raise SimError(f"more than {MAX_EVENTS} simulator events before tick {until}: "
                               "something sends messages or sets timers without end")
        self.time = until
        path = self._write_log()
        unfinished = sorted(c for c in self.plans if c not in self.outcomes)
        return Result(self.name, str(path), dict(self.outcomes), unfinished, list(self.events))

    # ---- internals
    def _server(self, node):
        if node not in (LOCK, STORAGE):
            raise SimError(f"only {LOCK!r} and {STORAGE!r} crash and restart, not {node!r}")

    def _push(self, t, kind, node, data=None, incarnation=0):
        if t < self.time:
            raise SimError("cannot schedule in the past")
        self._seq += 1
        heapq.heappush(self._queue, _Event(t, self._seq, kind, node, data, incarnation))

    def _boot(self, nid, client_id=None):
        self._incarnations[nid] = self._incarnations.get(nid, -1) + 1
        if client_id is None:
            node = self._classes[nid](self, nid)
        else:
            plan = self.plans[client_id]
            node = self._client_cls(self, nid, client_id, plan["writes"], plan["maxAttempts"])
        self.nodes[nid] = node
        node.on_start()

    def _send(self, src, dst, msg):
        if dst not in (LOCK, STORAGE) and dst not in {client_node(c) for c in self.plans}:
            raise SimError(f"unknown destination {dst!r}")
        env = Envelope(src, dst, copy.deepcopy(msg), self.time)
        for d in self.policy(self, env):
            self._push(self.time + max(1, int(d)), "deliver", dst, (src, copy.deepcopy(env.msg)))

    def _timer(self, nid, delay, tag):
        self._push(self.time + delay, "timer", nid, tag, self._incarnations[nid])

    def _log(self, event, payload):
        self.events.append((self.time, event, copy.deepcopy(payload)))

    def _emit(self, nid, event, payload):
        if event not in _EMITTERS:
            raise SimError(f"{nid} emitted {event!r}: not a contract event ({', '.join(_EMITTERS)})")
        who, fields = _EMITTERS[event]
        if who == "client":
            ok_emitter = isinstance(payload, dict) and nid == client_node(payload.get("id"))
        else:
            ok_emitter = nid == who
        if not ok_emitter:
            raise SimError(f"{nid} must not emit {event} {payload!r}")
        if fields is None:
            shape = type(payload) is int
        else:
            shape = isinstance(payload, dict) and tuple(sorted(payload)) == tuple(sorted(fields))
        if not shape:
            want = "an int" if fields is None else "a dict with " + ", ".join(fields)
            raise SimError(f"{event} payload must be {want}, got {payload!r}")
        if event == "ClientDone":
            if payload["outcome"] not in ("Wrote", "Fenced", "GaveUp"):
                raise SimError(f"client outcome must be Wrote, Fenced or GaveUp, got {payload['outcome']!r}")
            self.outcomes.setdefault(payload["id"], payload["outcome"])
        self._log(event, payload)
        if event == "Granted":
            self._granted(payload["id"])

    def _granted(self, client_id):
        """Play the client's planned behavior at its grant."""
        plan = self.plans.get(client_id)
        nid = client_node(client_id)
        if plan is None or nid not in self.nodes:
            return
        if plan["behavior"] == "Crashes":
            del self.nodes[nid]
            self._dead.add(nid)
            self.outcomes[client_id] = "Crashed"
            # Test bookkeeping, as in the model: the driver records that the client died.
            self._log("ClientDone", {"id": client_id, "outcome": "Crashed"})
        elif plan["behavior"] == "Pauses" and plan["pause"] > 0:
            self._paused[nid] = max(self._paused.get(nid, 0), self.time + plan["pause"])

    def _dispatch(self, ev: _Event):
        if ev.kind in ("deliver", "timer"):
            resume = self._paused.get(ev.node, 0)
            if resume > self.time:
                # A frozen node sees nothing until it resumes; order is kept.
                self._push(resume, ev.kind, ev.node, ev.data, ev.incarnation)
                return
            node = self.nodes.get(ev.node)
            if node is None:
                return
            if ev.kind == "deliver":
                node.on_message(*ev.data)
            elif self._incarnations[ev.node] == ev.incarnation:
                node.on_timer(ev.data)
        elif ev.kind == "start_client":
            plan = self.plans[ev.data]
            self._log("ClientStarted", {k: plan[k] for k in ("id", "behavior", "writes", "maxAttempts")})
            self._boot(ev.node, ev.data)
        elif ev.kind == "crash":
            self.nodes.pop(ev.node, None)
        elif ev.kind == "restart":
            if ev.node not in self.nodes:
                self._boot(ev.node)
        elif ev.kind == "pause":
            if ev.node in self.nodes:
                self._paused[ev.node] = max(self._paused.get(ev.node, 0), self.time + ev.data)
        elif ev.kind == "policy":
            self.policy = ev.data
        elif ev.kind == "call":
            ev.data(self)

    def _write_log(self) -> Path:
        self.log_dir.mkdir(parents=True, exist_ok=True)
        path = self.log_dir / f"{self.name}.jsonl"
        with open(path, "w") as f:
            for tick, event, payload in self.events:
                # One JSON object per line. `key` names the run; `timestamp` is the tick.
                f.write(json.dumps({"key": self.name, "timestamp": tick, "event": event,
                                    "payload": payload}) + "\n")
        return path
