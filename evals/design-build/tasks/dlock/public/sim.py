"""Deterministic simulator for the distributed lock service in TASK.md.

Your `solution.py` defines `LockServer` and `LockClient` as subclasses of `Node`. The
simulator creates the nodes, delivers their messages, fires their timers, injects
faults, plays the application (calls `lock` and `unlock`) and checks safety.

Time is an integer tick shared by every node (clocks are perfectly synchronized).
Everything is single-threaded: one handler runs at a time and runs to completion.

Example:

    from sim import Sim
    from solution import LockServer, LockClient

    s = Sim(LockServer, LockClient, servers=3, clients=2, seed=1)
    s.lock("c0", "r", "write", hold=10)
    s.lock("c1", "r", "write", hold=10, at=2)
    s.run_until(300)
    assert s.violations == []
    assert len(s.acquisitions("r")) == 2
"""
from __future__ import annotations

import copy
import heapq
import random
from dataclasses import dataclass, field
from typing import Any, Callable

MAX_DELAY = 5      # a message is normally delivered within 1..MAX_DELAY ticks
HOLD_MAX = 20      # the application unlocks at most HOLD_MAX ticks after `acquired`
LIVENESS = 150     # see TASK.md: once the network is healthy, how soon a free lock is granted


class Node:
    """Base class for LockServer and LockClient. Do not override these methods."""

    def __init__(self, sim: "Sim", node_id: str):
        self._sim = sim
        self.id = node_id                       # "s0", "s1", ... or "c0", "c1", ...
        self.servers = list(sim.server_ids)     # every lock server's id
        self._rng = random.Random(f"{sim.seed}/{node_id}/{sim.incarnation(node_id)}")

    def now(self) -> int:
        """The current tick."""
        return self._sim.time

    def send(self, dst: str, msg: Any) -> None:
        """Send a message to another node. The message is copied; it may be lost,
        duplicated, reordered or delayed (see TASK.md)."""
        self._sim._send(self.id, dst, msg)

    def set_timer(self, delay: int, tag: Any = None) -> None:
        """Call `self.on_timer(tag)` after `delay` (>= 1) ticks, unless this node crashes
        first. Timers cannot be cancelled; use the tag to ignore stale ones."""
        self._sim._timer(self.id, max(1, int(delay)), tag)

    def random(self) -> float:
        """A deterministic pseudo-random float in [0, 1), private to this node."""
        return self._rng.random()

    # Override these.
    def on_start(self) -> None:
        """Called once when the node starts (for a server, also after each restart:
        a restarted server is a new object with no state)."""

    def on_message(self, src: str, msg: Any) -> None:
        """Called when a message arrives."""

    def on_timer(self, tag: Any) -> None:
        """Called when a timer set with `set_timer` fires."""


class ClientBase(Node):
    """LockClient's base. The application calls `lock` and `unlock`; the client reports
    a grant by calling `self.acquired(resource)`."""

    def lock(self, resource: str, mode: str) -> None:
        """The application asks for `resource` in `mode` ("write" or "read"). Called only
        when this client neither holds nor is requesting `resource`."""
        raise NotImplementedError

    def unlock(self, resource: str) -> None:
        """The application releases `resource`, which this client holds."""
        raise NotImplementedError

    def acquired(self, resource: str) -> None:
        """Call this once the lock asked for by `lock(resource, mode)` is held. From this
        call until the application calls `unlock`, this client holds the lock."""
        self._sim._acquired(self.id, resource)


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
class Hold:
    client: str
    resource: str
    mode: str
    start: int
    end: int | None = None   # None while held


Policy = Callable[["Sim", Envelope], list]


def healthy(sim: "Sim", env: Envelope) -> list:
    """Default network: each message delivered once after 1..MAX_DELAY ticks."""
    return [sim.rng.randint(1, MAX_DELAY)]


def lossy(loss=0.1, dup=0.05, slow=0.05, slow_max=400) -> Policy:
    """A faulty network: drops, duplicates and occasionally delays messages a lot."""
    def policy(sim: "Sim", env: Envelope) -> list:
        r = sim.rng.random()
        if r < loss:
            return []
        delays = [sim.rng.randint(1, MAX_DELAY)]
        if sim.rng.random() < slow:
            delays = [sim.rng.randint(MAX_DELAY, slow_max)]
        if sim.rng.random() < dup:
            delays.append(sim.rng.randint(1, slow_max))
        return delays
    return policy


class Sim:
    """Runs LockServer and LockClient nodes.

    Sim(server_cls, client_cls, servers=3, clients=2, seed=0, policy=healthy)

    - `lock(client, resource, mode, hold, at=None)`: at tick `at` (default now) the
      application calls `client.lock(resource, mode)`, and unlocks `hold` ticks after
      the client calls `acquired`.
    - `crash(node, at=None)`: the node stops; messages and timers for it are dropped.
      A crashed client stays down. `restart(server, at=None)` starts a fresh server.
    - `set_policy(policy, at=None)`: change the network (see `healthy`, `lossy`).
    - `run_until(t)`: run events up to tick t.
    - `violations`: safety violations found so far (two conflicting holders).
    - `holds`, `acquisitions(resource)`, `pending()`: what happened.
    """

    def __init__(self, server_cls, client_cls, servers: int = 3, clients: int = 2,
                 seed: int = 0, policy: Policy = healthy):
        self.seed = seed
        self.rng = random.Random(f"net/{seed}")
        self.time = 0
        self.server_cls, self.client_cls = server_cls, client_cls
        self.server_ids = [f"s{i}" for i in range(servers)]
        self.client_ids = [f"c{i}" for i in range(clients)]
        self.policy = policy
        self.nodes: dict[str, Node] = {}
        self._incarnations: dict[str, int] = {}
        self._queue: list[_Event] = []
        self._seq = 0
        self.holds: list[Hold] = []
        self.requests: dict[tuple, dict] = {}   # (client, resource) -> {"mode", "hold", "at"}
        self.violations: list[str] = []
        self.log: list[str] = []
        for nid in self.server_ids + self.client_ids:
            self._boot(nid)

    # ---- scenario controls
    def lock(self, client: str, resource: str, mode: str, hold: int, at: int | None = None):
        assert mode in ("read", "write") and 0 <= hold <= HOLD_MAX
        self._push(self.time if at is None else at, "app_lock", client, (resource, mode, hold))

    def crash(self, node: str, at: int | None = None):
        self._push(self.time if at is None else at, "crash", node)

    def restart(self, server: str, at: int | None = None):
        assert server in self.server_ids
        self._push(self.time if at is None else at, "restart", server)

    def set_policy(self, policy: Policy, at: int | None = None):
        self._push(self.time if at is None else at, "policy", "", policy)

    def at(self, t: int, fn: Callable[["Sim"], None]):
        """Run fn(sim) at tick t (for custom scenario steps)."""
        self._push(t, "call", "", fn)

    def run_until(self, t: int):
        while self._queue and self._queue[0].time <= t:
            ev = heapq.heappop(self._queue)
            self.time = ev.time
            self._dispatch(ev)
        self.time = max(self.time, t)

    # ---- results
    def acquisitions(self, resource: str | None = None) -> list[Hold]:
        return [h for h in self.holds if resource is None or h.resource == resource]

    def pending(self) -> list[tuple]:
        """(client, resource) requests not yet acquired, from clients that are up."""
        return [k for k in self.requests if k[0] in self.nodes]

    def up(self, node: str) -> bool:
        return node in self.nodes

    def incarnation(self, node: str) -> int:
        return self._incarnations.get(node, 0)

    # ---- internals
    def _push(self, t, kind, node, data=None, incarnation=0):
        assert t >= self.time, "cannot schedule in the past"
        self._seq += 1
        heapq.heappush(self._queue, _Event(t, self._seq, kind, node, data, incarnation))

    def _boot(self, nid):
        self._incarnations[nid] = self._incarnations.get(nid, -1) + 1
        cls = self.server_cls if nid in self.server_ids else self.client_cls
        node = cls(self, nid)
        self.nodes[nid] = node
        self.log.append(f"{self.time} start {nid}")
        node.on_start()

    def _alive(self, nid, incarnation):
        return nid in self.nodes and self._incarnations[nid] == incarnation

    def _send(self, src, dst, msg):
        if dst not in self.server_ids and dst not in self.client_ids:
            raise ValueError(f"unknown destination {dst!r}")
        env = Envelope(src, dst, copy.deepcopy(msg), self.time)
        for d in self.policy(self, env):
            self._push(self.time + max(1, int(d)), "deliver", dst, (src, copy.deepcopy(env.msg)),
                       incarnation=-1)

    def _timer(self, nid, delay, tag):
        self._push(self.time + delay, "timer", nid, tag, self._incarnations[nid])

    def _acquired(self, client, resource):
        key = (client, resource)
        if key not in self.requests:
            self.violations.append(f"{self.time}: {client} called acquired({resource!r}) with no pending lock()")
            return
        req = self.requests.pop(key)
        hold = Hold(client, resource, req["mode"], self.time)
        for other in self.holds:
            if other.resource == resource and other.end is None and other.client != client:
                if hold.mode == "write" or other.mode == "write":
                    self.violations.append(
                        f"{self.time}: {client} acquired {resource!r} ({hold.mode}) while {other.client} "
                        f"holds it ({other.mode}, since {other.start})")
        self.holds.append(hold)
        self.log.append(f"{self.time} {client} acquired {resource} {hold.mode}")
        self._push(self.time + req["hold"], "app_unlock", client, (resource, hold), self._incarnations[client])

    def _dispatch(self, ev: _Event):
        if ev.kind == "deliver":
            src, msg = ev.data
            node = self.nodes.get(ev.node)
            if node is not None:
                node.on_message(src, msg)
        elif ev.kind == "timer":
            if self._alive(ev.node, ev.incarnation):
                self.nodes[ev.node].on_timer(ev.data)
        elif ev.kind == "app_lock":
            resource, mode, hold = ev.data
            node = self.nodes.get(ev.node)
            if node is not None:
                busy = (ev.node, resource) in self.requests or any(
                    h.client == ev.node and h.resource == resource and h.end is None for h in self.holds)
                if busy:
                    raise RuntimeError(f"scenario error at {self.time}: {ev.node} already holds or is "
                                       f"requesting {resource!r}; the application never does that")
                self.requests[(ev.node, resource)] = {"mode": mode, "hold": hold, "at": self.time}
                self.log.append(f"{self.time} {ev.node} lock {resource} {mode}")
                node.lock(resource, mode)
        elif ev.kind == "app_unlock":
            resource, hold = ev.data
            if self._alive(ev.node, ev.incarnation) and hold.end is None:
                hold.end = self.time
                self.log.append(f"{self.time} {ev.node} unlock {resource}")
                self.nodes[ev.node].unlock(resource)
        elif ev.kind == "crash":
            if ev.node in self.nodes:
                del self.nodes[ev.node]
                self.log.append(f"{self.time} crash {ev.node}")
                for h in self.holds:
                    if h.client == ev.node and h.end is None:
                        h.end = self.time
        elif ev.kind == "restart":
            if ev.node not in self.nodes:
                self._boot(ev.node)
        elif ev.kind == "policy":
            self.policy = ev.data
        elif ev.kind == "call":
            ev.data(self)
