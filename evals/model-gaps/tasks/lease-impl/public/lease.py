"""The lease lock: lock service, storage and client. See TASK.md and DESIGN.md.

Complete the three classes. The message format between them is yours to choose.
Each class gets its clock, network, timers, disk and `emit` from `sim.Node`.
"""
from sim import BACKOFF, LOCK, RETRY, STORAGE, TTL, ClientBase, Node


class LockService(Node):
    """Grants leases with fencing tokens. Emits Granted, Busy, Released, Expired."""

    def on_start(self):
        pass

    def on_message(self, src, msg):
        pass

    def on_timer(self, tag):
        pass


class Storage(Node):
    """Accepts or rejects writes by fencing token. Emits WriteAccepted, WriteRejected."""

    def on_start(self):
        pass

    def on_message(self, src, msg):
        pass

    def on_timer(self, tag):
        pass


class Client(ClientBase):
    """Acquires the lock (self.max_attempts tries), writes self.writes times, releases,
    then calls self.done("Wrote" | "Fenced" | "GaveUp")."""

    def on_start(self):
        pass

    def on_message(self, src, msg):
        pass

    def on_timer(self, tag):
        pass
