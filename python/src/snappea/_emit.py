"""`emit`: the one function application code imports.

It has no dependencies beyond the standard library. Where an event goes:

1. a test in this process is recording: to its recorders, in memory;
2. `SNAPPEA_EVENTS_DIR` is set (a process a recording test started): to this process's own
   file in that folder;
3. a production sink is set with :func:`set_sink`: to that sink, from a background
   thread, never raising or blocking;
4. otherwise nowhere, at the cost of a list check and an environment lookup.
"""

from __future__ import annotations

import atexit
import enum
import itertools
import json
import os
import queue
import re
import sys
import threading
import time
from dataclasses import dataclass, field
from datetime import date, datetime, timezone
from typing import Any, Callable, Optional, Union

EVENTS_DIR_ENV = "SNAPPEA_EVENTS_DIR"

_NO_PAYLOAD: Any = object()


@dataclass
class Event:
    """One emitted event."""

    event: str
    payload: Any
    key: str
    #: Nanoseconds since the epoch (or the logical time passed as `at`).
    time_ns: int
    #: `True` when the time came from `at=` rather than the clock.
    at: bool = False
    #: `file:line` of the `emit` call (or `path:line` of a parsed log line).
    src: Optional[str] = None
    #: Order of emission within this process.
    seq: int = 0
    #: Process that emitted it.
    pid: int = field(default_factory=os.getpid)

    def to_json(self) -> str:
        """The JSON line `snappea observe` reads, plus fields used for reports."""
        d: dict = {
            "key": self.key,
            "timestamp": iso_timestamp(self.time_ns),
            "event": self.event,
            "payload": _plain(self.payload),
            "src": self.src,
            "seq": self.seq,
            "pid": self.pid,
        }
        if self.at:
            d["at"] = True
        return json.dumps(d, default=_json_default, separators=(",", ":"))

    @staticmethod
    def from_json(d: dict) -> "Event":
        return Event(
            event=d.get("event", ""),
            payload=d.get("payload"),
            key=_key_str(d.get("key")),
            time_ns=0,
            at=bool(d.get("at")),
            src=d.get("src"),
            seq=d.get("seq", 0) or 0,
            pid=d.get("pid", 0) or 0,
        )


# Recorders currently collecting events in this process, innermost last. A plain list,
# not a contextvar, so that events from threads started inside a test are kept.
_recorders: list = []
_lock = threading.Lock()
_seq = itertools.count(1)
_last_ns = 0
_file = None
_file_pid = 0
_file_dir = None
_sink: Optional["Sink"] = None
#: Events a process started by a recording test could not write to `SNAPPEA_EVENTS_DIR`
#: (a payload that cannot be encoded, or a failed write). `emit` drops them rather than
#: raising into application code.
dir_dropped = 0


def _after_fork_in_child() -> None:
    # A forked child (multiprocessing on Linux) inherits the parent's recorders, but its
    # events would stay in its own copy of them: it writes to SNAPPEA_EVENTS_DIR instead.
    global _file, _lock
    _recorders.clear()
    _file = None
    _lock = threading.Lock()  # another thread may have held it at the fork
    if _sink is not None:
        _sink._lock = threading.Lock()


if hasattr(os, "register_at_fork"):
    os.register_at_fork(after_in_child=_after_fork_in_child)


def emit(event: str, payload: Any = _NO_PAYLOAD, /, *, key: Any = None, at: Union[datetime, int, None] = None, **fields: Any) -> None:
    """Record that `event` happened, for P specs to check.

    The payload is either the keyword arguments (``emit("eEnter", client="c1")``) or one
    positional value, for events whose payload is not a named tuple or whose field names
    clash with ``key``/``at`` (``emit("eClose", 7)``, ``emit("ePut", {"key": "k"})``).

    ``key`` splits events into independent streams, each checked by its own spec
    instance. ``at`` overrides the event time: a ``datetime``, or an integer of epoch
    nanoseconds (any increasing counter works). Without a recording test or a sink set
    with :func:`set_sink` this does nothing.
    """
    if not _recorders:
        events_dir = os.environ.get(EVENTS_DIR_ENV)
        if not events_dir:
            if _sink is not None:
                _sink._emit(event, payload, key, at, fields)
            return
    else:
        events_dir = None
    if payload is _NO_PAYLOAD:
        payload = fields if fields else None
    elif fields:
        raise TypeError("emit() takes either a positional payload or keyword fields, not both")
    ev = _make_event(event, payload, key, at, sys._getframe(1))
    if events_dir is None:
        for r in tuple(_recorders):
            r._add(ev)
    else:
        _write_to_dir(events_dir, ev)


def _make_event(event: str, payload: Any, key: Any, at: Any, frame: Any) -> Event:
    global _last_ns
    if not isinstance(event, str):
        raise TypeError(f"emit(): the event name must be a string, not {type(event).__name__}")
    src = f"{_short_path(frame.f_code.co_filename)}:{frame.f_lineno}" if frame is not None else None
    with _lock:
        if at is None:
            # never earlier than the previous event, so a clock step back keeps emit order
            now = max(time.time_ns(), _last_ns)
            _last_ns = now
            t, overridden = now, False
        else:
            t, overridden = to_ns(at), True
        return Event(event, payload, _key_str(key), t, overridden, src, next(_seq))


def process_name() -> str:
    """This program's name for event file names: the script, module or package run."""
    argv0 = (getattr(sys, "argv", None) or [""])[0]
    if not argv0 or argv0 == "-c":
        return "python"
    base, name = os.path.split(argv0)
    name = os.path.splitext(name)[0]
    if name == "__main__":  # python -m package
        name = os.path.basename(base)
    return re.sub(r"[^A-Za-z0-9_.-]", "_", name)[:40] or "python"


def events_file_name(pid: Optional[int] = None) -> str:
    """`<name>-<pid>-<random>.jsonl`: unique even across containers sharing pid 1."""
    return f"{process_name()}-{pid or os.getpid()}-{os.urandom(3).hex()}.jsonl"


def _write_to_dir(events_dir: str, ev: Event) -> None:
    """Appends to this process's own file in `events_dir`, so writes never interleave.

    Best-effort: this runs inside application code, so an event that cannot be encoded
    or written is dropped and counted in :data:`dir_dropped`, never raised."""
    global _file, _file_pid, _file_dir, dir_dropped
    try:
        line = ev.to_json() + "\n"
        with _lock:
            pid = os.getpid()
            if _file is None or _file_pid != pid or _file_dir != events_dir:
                if _file is not None and _file_pid == pid:
                    _file.close()
                    _file = None
                os.makedirs(events_dir, exist_ok=True)
                _file = open(os.path.join(events_dir, events_file_name(pid)), "a", encoding="utf-8")
                _file_pid, _file_dir = pid, events_dir
            _file.write(line)
            _file.flush()
    except Exception:
        dir_dropped += 1


# ---- production sink -------------------------------------------------------------------

SinkTarget = Union[str, "os.PathLike[str]", Callable[[str], Any], Any, None]


class Sink:
    """Where :func:`emit` writes events outside tests. Made by :func:`set_sink`.

    Lines are queued and written by a background thread, so ``emit`` never blocks or
    raises. When the queue is full or a write fails, the event is dropped and counted
    in :attr:`dropped`.
    """

    #: Lines waiting to be written before new events are dropped.
    QUEUE_SIZE = 10_000

    def __init__(self, target: SinkTarget):
        self.target = target
        #: Events dropped: queue full, a write failed, or a payload could not be encoded.
        self.dropped = 0
        self._write = self._writer(target)
        self._lock = threading.Lock()
        self._pid = -1
        self._closed = False
        self._queue: "queue.Queue[Optional[str]]" = queue.Queue(self.QUEUE_SIZE)

    @staticmethod
    def _writer(target: SinkTarget) -> Callable[[str], None]:
        if target == "-":
            target = sys.stdout
        if isinstance(target, (str, os.PathLike)):
            path = os.fspath(target)
            parent = os.path.dirname(path)
            if parent:
                os.makedirs(parent, exist_ok=True)
            # one write() per line on an O_APPEND file: lines never interleave
            fd = os.open(path, os.O_WRONLY | os.O_APPEND | os.O_CREAT, 0o644)

            def write(line: str) -> None:
                os.write(fd, (line + "\n").encode("utf-8"))

            return write
        if hasattr(target, "write"):
            stream = target

            def write(line: str) -> None:
                stream.write(line + "\n")
                flush = getattr(stream, "flush", None)
                if flush is not None:
                    flush()

            return write
        if callable(target):
            return target  # type: ignore[return-value]
        raise TypeError(f"set_sink() takes a path, a text stream, a callable or None, not {type(target).__name__}")

    def _emit(self, event: str, payload: Any, key: Any, at: Any, fields: dict) -> None:
        try:
            if payload is _NO_PAYLOAD:
                payload = fields if fields else None
            elif fields:
                raise TypeError("emit() takes either a positional payload or keyword fields, not both")
            with self._lock:
                if self._closed:
                    return
                if self._pid != os.getpid():  # first event, or a forked child: start a writer
                    self._start()
                line = _make_event(event, payload, key, at, sys._getframe(2)).to_json()
                self._queue.put_nowait(line)
        except Exception:
            self.dropped += 1

    def _start(self) -> None:
        self._pid = os.getpid()
        self._queue = queue.Queue(self.QUEUE_SIZE)
        threading.Thread(target=self._run, args=(self._queue,), name="snappea-sink", daemon=True).start()

    def _run(self, q: "queue.Queue[Optional[str]]") -> None:
        while True:
            line = q.get()
            try:
                if line is None:
                    return
                self._write(line)
            except Exception:
                self.dropped += 1
            finally:
                q.task_done()

    def flush(self, timeout: float = 5.0) -> bool:
        """Waits up to `timeout` seconds for queued events to be written. Returns
        whether they were."""
        deadline = time.monotonic() + timeout
        while self._queue.unfinished_tasks and self._pid == os.getpid():
            if time.monotonic() >= deadline:
                return False
            time.sleep(0.005)
        return True

    def close(self, timeout: float = 5.0) -> None:
        """Writes what is queued and stops the writer. Later events are ignored."""
        with self._lock:
            if self._closed:
                return
            self._closed = True
            if self._pid == os.getpid():
                try:
                    self._queue.put_nowait(None)
                except queue.Full:
                    pass
        self.flush(timeout)


def set_sink(target: SinkTarget) -> Optional[Sink]:
    """Sends events to `target` when no test is recording, as the JSON lines
    ``snappea observe`` reads. `target` is a file path (appended to), ``"-"`` or a text
    stream such as ``sys.stdout``, a callable taking one line (no newline), or ``None``
    to turn the sink off. Returns the new :class:`Sink` (``None`` for off).

    Inside a recording test, or in a process a test started, events go to the test
    instead. ``emit`` never raises or blocks because of the sink.
    """
    global _sink
    old, _sink = _sink, None
    if old is not None:
        old.close()
    if target is not None:
        _sink = Sink(target)
    return _sink


@atexit.register
def _flush_sink_at_exit() -> None:
    if _sink is not None:
        _sink.close(timeout=2.0)


# ---- times and values ------------------------------------------------------------------


def to_ns(at: Union[datetime, date, int, float]) -> int:
    """Epoch nanoseconds of an `at=` value. Naive datetimes are local time, as in Python."""
    if isinstance(at, bool):
        raise TypeError("emit(at=...) takes a datetime or an integer, not a bool")
    if isinstance(at, int):
        return at
    if isinstance(at, datetime):
        if at.tzinfo is None:
            at = at.astimezone()
        delta = at - datetime(1970, 1, 1, tzinfo=timezone.utc)
        return (delta.days * 86_400 + delta.seconds) * 1_000_000_000 + delta.microseconds * 1_000
    if isinstance(at, float):
        raise TypeError("emit(at=...) takes a datetime or integer nanoseconds; got a float (seconds?)")
    raise TypeError(f"emit(at=...) takes a datetime or an integer, not {type(at).__name__}")


def iso_timestamp(ns: int) -> str:
    """ISO-8601 with nanoseconds, which snappea orders exactly (bare integers read as ms)."""
    secs, nanos = divmod(ns, 1_000_000_000)
    return time.strftime("%Y-%m-%dT%H:%M:%S", time.gmtime(secs)) + f".{nanos:09d}Z"


def _key_str(key: Any) -> str:
    if key is None:
        return ""
    return key if isinstance(key, str) else str(key)


def _plain(v: Any) -> Any:
    """`v` with enums as their names and sets and tuples as lists. Done before encoding
    because `json` writes `str` and `int` enums by value without asking `default`."""
    if isinstance(v, enum.Enum):
        return v.name
    if isinstance(v, dict):
        return {(k.name if isinstance(k, enum.Enum) else k): _plain(x) for k, x in v.items()}
    if isinstance(v, (list, tuple, set, frozenset)):
        return [_plain(x) for x in v]
    return v


def _json_default(o: Any) -> Any:
    if isinstance(o, (datetime, date)):
        return o.isoformat()
    if hasattr(o, "__dataclass_fields__"):
        return _plain({k: getattr(o, k) for k in o.__dataclass_fields__})
    return str(o)


_cwd = os.getcwd()


def _short_path(path: str) -> str:
    try:
        rel = os.path.relpath(path, _cwd)
    except ValueError:  # another drive on Windows
        return path
    return path if rel.startswith("..") else rel
