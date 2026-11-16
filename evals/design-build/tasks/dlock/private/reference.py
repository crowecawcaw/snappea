"""Reference lock service: leases instead of maintenance.

- A grant is a lease: the server keeps the entry until `expires` (server time) unless
  released by the same acquisition id. No maintenance pass, no peer queries.
- A server refuses every grant for LEASE ticks after it starts, so a restarted server
  cannot hand out a lock that its previous incarnation granted and forgot.
- The client counts distinct servers whose grant is still valid for HOLD_MAX more ticks;
  a late grant is useless. Quorums: write W = N//2 + 1, read R = N - W + 1 (R + W > N).
- An attempt that cannot reach quorum quickly is aborted: the client releases what it
  got and retries after a random backoff with a new acquisition id.
- Releases name the acquisition id, so a delayed release never drops a later grant.
"""
from sim import ClientBase, HOLD_MAX, MAX_DELAY, Node

LEASE = 60
STALE = 2 * MAX_DELAY          # requests older than this are ignored: their grant would be useless
ATTEMPT = 2 * MAX_DELAY + 2    # how long an attempt waits for a quorum


class LockServer(Node):
    def on_start(self):
        self.started = self.now()
        self.table = {}   # resource -> {uid: (client, mode, expires)}

    def _live(self, resource):
        now = self.now()
        entries = self.table.get(resource, {})
        for uid in [u for u, e in entries.items() if e[2] <= now]:
            del entries[uid]
        return entries

    def on_message(self, src, msg):
        kind = msg["type"]
        if kind == "lock":
            res, mode, uid = msg["resource"], msg["mode"], msg["uid"]
            if self.now() - msg["sent"] > STALE:
                return
            entries = self._live(res)
            if uid in entries:
                ok, expires = True, entries[uid][2]
            elif self.now() < self.started + LEASE:
                ok, expires = False, 0
            elif mode == "write":
                ok = not entries
                expires = self.now() + LEASE
            else:
                ok = all(e[1] == "read" for e in entries.values())
                expires = self.now() + LEASE
            if ok and uid not in entries:
                self.table.setdefault(res, {})[uid] = (src, mode, expires)
            self.send(src, {"type": "reply", "resource": res, "uid": uid, "ok": ok, "expires": expires})
        elif kind == "release":
            self._live(msg["resource"]).pop(msg["uid"], None)


class LockClient(ClientBase):
    def on_start(self):
        self.counter = 0
        self.want = {}    # resource -> {"mode", "uid", "grants": {server: expires}, "refused": set()}
        self.held = {}    # resource -> uid

    def _quorum(self, mode):
        n = len(self.servers)
        w = n // 2 + 1
        return w if mode == "write" else n - w + 1

    def lock(self, resource, mode):
        self.want[resource] = {"mode": mode}
        self._attempt(resource)

    def _attempt(self, resource):
        w = self.want[resource]
        self.counter += 1
        w.update(uid=f"{self.id}/{self.counter}", grants={}, refused=set())
        for s in self.servers:
            self.send(s, {"type": "lock", "resource": resource, "mode": w["mode"],
                          "uid": w["uid"], "sent": self.now()})
        self.set_timer(ATTEMPT, ("timeout", resource, w["uid"]))

    def _release(self, resource, uid, servers):
        for s in servers:
            self.send(s, {"type": "release", "resource": resource, "uid": uid})

    def on_message(self, src, msg):
        if msg["type"] != "reply":
            return
        res, uid = msg["resource"], msg["uid"]
        w = self.want.get(res)
        if w is None or w["uid"] != uid:
            if msg["ok"] and self.held.get(res) != uid:
                self._release(res, uid, [src])   # a late grant for an abandoned attempt
            return
        if msg["ok"] and msg["expires"] > self.now() + HOLD_MAX + 1:
            w["grants"][src] = msg["expires"]
        else:
            w["refused"].add(src)
        q = self._quorum(w["mode"])
        if len(w["grants"]) >= q:
            del self.want[res]
            self.held[res] = uid
            self.acquired(res)
        elif len(self.servers) - len(w["refused"]) < q:
            self._abort(res)

    def _abort(self, resource):
        w = self.want[resource]
        self._release(resource, w["uid"], self.servers)
        w["uid"] = None
        self.set_timer(1 + int(self.random() * 4 * MAX_DELAY), ("retry", resource))

    def on_timer(self, tag):
        kind, res = tag[0], tag[1]
        w = self.want.get(res)
        if w is None:
            return
        if kind == "timeout" and w["uid"] == tag[2]:
            self._abort(res)
        elif kind == "retry" and w["uid"] is None:
            self._attempt(res)

    def unlock(self, resource):
        uid = self.held.pop(resource)
        self._release(resource, uid, self.servers)
