"""The lease lock: lock service, storage and client. Reference implementation."""
from sim import BACKOFF, LOCK, RETRY, STORAGE, TTL, ClientBase, Node


class LockService(Node):
    """Grants leases with fencing tokens. Emits Granted, Busy, Released, Expired."""

    def on_start(self):
        d = self.disk
        d.setdefault("last", 0)       # highest token granted
        d.setdefault("holder", 0)     # token of the live lease, 0 when free
        d.setdefault("expires", 0)    # tick the live lease ends
        self.answers = {}
        if d["holder"]:
            # Timers died with the old process: arm the live lease's expiry again.
            self.set_timer(d["expires"] - self.now(), d["holder"])

    def on_message(self, src, msg):
        d = self.disk
        if msg["t"] == "acquire":
            cid, n = msg["id"], msg["n"]
            prev = self.answers.get(cid)
            if prev is not None and prev["n"] > n:
                return                      # a late copy of an older attempt
            if prev is not None and prev["n"] == n:
                self.send(src, prev)        # the same attempt again: same answer
                return
            if d["holder"]:
                answer = {"t": "acquire_resp", "n": n, "granted": False, "token": 0}
                self.answers[cid] = answer
                self.emit("Busy", cid)
            else:
                d["last"] += 1
                d["holder"] = d["last"]
                d["expires"] = self.now() + TTL
                answer = {"t": "acquire_resp", "n": n, "granted": True, "token": d["holder"]}
                self.answers[cid] = answer
                self.emit("Granted", {"id": cid, "token": d["holder"]})
                self.set_timer(TTL, d["holder"])
            self.send(src, answer)
        elif msg["t"] == "release":
            if msg["token"] == d["holder"]:
                d["holder"] = 0
                self.emit("Released", msg["token"])
            self.send(src, {"t": "release_resp", "token": msg["token"]})

    def on_timer(self, tag):
        d = self.disk
        if tag == d["holder"] and self.now() >= d["expires"]:
            d["holder"] = 0
            self.emit("Expired", tag)


class Storage(Node):
    """Accepts or rejects writes by fencing token. Emits WriteAccepted, WriteRejected."""

    def on_start(self):
        self.disk.setdefault("highest", 0)
        self.disk.setdefault("writes", {})   # client id -> [seq, ok] of its latest write

    def on_message(self, src, msg):
        d = self.disk
        if msg["t"] != "write":
            return
        cid, seq, token = msg["id"], msg["seq"], msg["token"]
        prev = d["writes"].get(cid)
        if prev is not None and prev[0] > seq:
            return
        if prev is not None and prev[0] == seq:
            self.send(src, {"t": "write_resp", "seq": seq, "ok": prev[1]})
            return
        ok = token >= d["highest"]
        if ok:
            d["highest"] = token
        d["writes"][cid] = [seq, ok]
        self.emit("WriteAccepted" if ok else "WriteRejected", {"id": cid, "token": token})
        self.send(src, {"t": "write_resp", "seq": seq, "ok": ok})


class Client(ClientBase):
    """Acquires the lock (self.max_attempts tries), writes self.writes times, releases,
    then calls self.done("Wrote" | "Fenced" | "GaveUp")."""

    def on_start(self):
        self.phase = "acquire"
        self.attempt = 1
        self.seq = 1
        self.token = 0
        self.ok = True
        self.step = 0          # bumped on every new request, so old timers are ignored
        self._request()

    def _request(self):
        self.step += 1
        self._transmit()

    def _transmit(self):
        if self.phase == "acquire":
            self.send(LOCK, {"t": "acquire", "id": self.client_id, "n": self.attempt})
        elif self.phase == "write":
            self.send(STORAGE, {"t": "write", "id": self.client_id, "seq": self.seq, "token": self.token})
        elif self.phase == "release":
            self.send(LOCK, {"t": "release", "id": self.client_id, "token": self.token})
        self.set_timer(RETRY, ("retry", self.step))

    def on_timer(self, tag):
        kind, step = tag
        if step != self.step or self.phase == "done":
            return
        if kind == "retry" and self.phase != "backoff":
            self._transmit()
        elif kind == "next" and self.phase == "backoff":
            self.phase = "acquire"
            self.attempt += 1
            self._request()

    def on_message(self, src, msg):
        t = msg["t"]
        if t == "acquire_resp" and self.phase == "acquire" and msg["n"] == self.attempt:
            if msg["granted"]:
                self.token = msg["token"]
                self.phase = "write"
                self._request()
            elif self.attempt < self.max_attempts:
                self.phase = "backoff"
                self.step += 1
                self.set_timer(BACKOFF, ("next", self.step))
            else:
                self.phase = "done"
                self.done("GaveUp")
        elif t == "write_resp" and self.phase == "write" and msg["seq"] == self.seq:
            self.ok = msg["ok"]
            if self.ok and self.seq < self.writes:
                self.seq += 1
            else:
                # Release even when fenced: the lease may still be ours.
                self.phase = "release"
            self._request()
        elif t == "release_resp" and self.phase == "release" and msg["token"] == self.token:
            self.phase = "done"
            self.done("Wrote" if self.ok else "Fenced")
