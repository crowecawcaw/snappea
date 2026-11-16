"""Reference lock service for dlock2: renewed leases measured on each node's own clock.

- A server grants a lease of LEASE ticks on its own clock and extends it when the same
  acquisition id asks again. Entries expire by themselves: no maintenance pass.
- A server refuses new grants for GRACE ticks after it starts, so leases its previous
  incarnation granted (on the same clock) have run out first.
- The client never compares clocks across nodes. A grant for a request it sent at its
  own time t is valid until t + VALID on its clock, where VALID is the lease shrunk by
  the worst rate difference between two clocks.
- While holding, the client renews every RENEW ticks and checks every CHECK ticks that
  a quorum of grants is still valid; if not, it calls `lost` before any lease can end.
- Quorums: write W = N//2 + 1, read R = N - W + 1.
"""
import math

from sim import ClientBase, MAX_DELAY, Node, RHO

LEASE = 60
GRACE = LEASE + 1
VALID = math.floor(LEASE * (1 - RHO) / (1 + RHO)) - 2
RENEW = 10
CHECK = 2
ATTEMPT = math.ceil(2 * MAX_DELAY * (1 + RHO)) + 2


class LockServer(Node):
    def on_start(self):
        self.started = self.now()
        self.table = {}   # resource -> {uid: (client, mode, expires)}

    def _live(self, resource):
        now = self.now()
        entries = self.table.setdefault(resource, {})
        for uid in [u for u, e in entries.items() if e[2] <= now]:
            del entries[uid]
        return entries

    def on_message(self, src, msg):
        kind = msg["type"]
        if kind == "lock":
            res, mode, uid = msg["resource"], msg["mode"], msg["uid"]
            entries = self._live(res)
            if uid in entries:
                ok = True
            elif self.now() < self.started + GRACE:
                ok = False
            elif mode == "write":
                ok = not entries
            else:
                ok = all(e[1] == "read" for e in entries.values())
            if ok:
                entries[uid] = (src, mode, self.now() + LEASE)
            self.send(src, {"type": "reply", "resource": res, "uid": uid, "seq": msg["seq"], "ok": ok})
        elif kind == "release":
            self._live(msg["resource"]).pop(msg["uid"], None)


class LockClient(ClientBase):
    def on_start(self):
        self.counter = 0
        self.seq = 0
        self.sent = {}    # seq -> local send time
        self.want = {}    # resource -> {"mode", "uid", "grants": {server: valid_until}, "refused": set()}
        self.held = {}    # resource -> {"mode", "uid", "grants"}

    def _quorum(self, mode):
        n = len(self.servers)
        w = n // 2 + 1
        return w if mode == "write" else n - w + 1

    def _request(self, resource, mode, uid):
        self.seq += 1
        self.sent[self.seq] = self.now()
        for s in self.servers:
            self.send(s, {"type": "lock", "resource": resource, "mode": mode, "uid": uid, "seq": self.seq})

    def _valid(self, grants, slack=0):
        now = self.now()
        return sum(v > now + slack for v in grants.values())

    def lock(self, resource, mode):
        self.want[resource] = {"mode": mode}
        self._attempt(resource)

    def _attempt(self, resource):
        w = self.want[resource]
        self.counter += 1
        w.update(uid=f"{self.id}/{self.counter}", grants={}, refused=set())
        self._request(resource, w["mode"], w["uid"])
        self.set_timer(ATTEMPT, ("timeout", resource, w["uid"]))

    def _release(self, resource, uid, servers):
        for s in servers:
            self.send(s, {"type": "release", "resource": resource, "uid": uid})

    def on_message(self, src, msg):
        if msg["type"] != "reply":
            return
        res, uid = msg["resource"], msg["uid"]
        until = self.sent.get(msg["seq"], -math.inf) + VALID
        for state in (self.held.get(res), self.want.get(res)):
            if state and state.get("uid") == uid:
                break
        else:
            if msg["ok"]:
                self._release(res, uid, [src])   # a grant for an attempt we gave up
            return
        if msg["ok"]:
            state["grants"][src] = max(state["grants"].get(src, -math.inf), until)
        else:
            state["grants"].pop(src, None)
            state.setdefault("refused", set()).add(src)
        if state is self.held.get(res):
            return
        q = self._quorum(state["mode"])
        if self._valid(state["grants"], CHECK) >= q:
            del self.want[res]
            self.held[res] = {"mode": state["mode"], "uid": uid, "grants": state["grants"]}
            self.acquired(res)
            self.set_timer(RENEW, ("renew", res, uid))
            self.set_timer(CHECK, ("check", res, uid))
        elif len(self.servers) - len(state["refused"]) < q:
            self._abort(res)

    def _abort(self, resource):
        w = self.want[resource]
        self._release(resource, w["uid"], self.servers)
        w["uid"] = None
        self.set_timer(1 + int(self.random() * 4 * MAX_DELAY), ("retry", resource))

    def on_timer(self, tag):
        kind, res = tag[0], tag[1]
        if kind in ("renew", "check"):
            h = self.held.get(res)
            if h is None or h["uid"] != tag[2]:
                return
            if kind == "renew":
                self._request(res, h["mode"], h["uid"])
                self.set_timer(RENEW, tag)
            elif self._valid(h["grants"], CHECK) < self._quorum(h["mode"]):
                del self.held[res]
                self._release(res, h["uid"], self.servers)
                self.lost(res)
            else:
                self.set_timer(CHECK, tag)
            return
        w = self.want.get(res)
        if w is None:
            return
        if kind == "timeout" and w["uid"] == tag[2]:
            self._abort(res)
        elif kind == "retry" and w["uid"] is None:
            self._attempt(res)

    def unlock(self, resource):
        h = self.held.pop(resource)
        self._release(resource, h["uid"], self.servers)
