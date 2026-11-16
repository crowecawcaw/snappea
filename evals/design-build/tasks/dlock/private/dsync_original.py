"""The design in TASK.md as MinIO dsync built it (2020): K1, K3, K4, K5 present.

- Entries have no lease. Maintenance runs every MAINT ticks on entries older than
  STALE_AGE: it asks every other server whether it still has the uid, and drops the entry
  if any one says no (K1). Nobody asks the owner (K5).
- Unlock is fire-and-forget (K3). A restarted server starts empty and grants at once (K4).
"""
from sim import ClientBase, MAX_DELAY, Node

MAINT = 50
STALE_AGE = 60
WAIT = 2 * MAX_DELAY + 2


class LockServer(Node):
    def on_start(self):
        self.table = {}      # res -> {uid: (client, mode, since)}
        self.checks = {}     # check id -> (res, uid)
        self.n = 0
        self.set_timer(MAINT, "maint")

    def on_message(self, src, msg):
        t = msg["type"]
        if t == "lock":
            res, uid, mode = msg["resource"], msg["uid"], msg["mode"]
            e = self.table.setdefault(res, {})
            if uid in e:
                ok = True
            elif mode == "write":
                ok = not e
            else:
                ok = all(v[1] == "read" for v in e.values())
            if ok:
                e.setdefault(uid, (src, mode, self.now()))
            self.send(src, {"type": "reply", "resource": res, "uid": uid, "ok": ok})
        elif t == "release":
            self.table.get(msg["resource"], {}).pop(msg["uid"], None)
        elif t == "expired?":
            has = msg["uid"] in self.table.get(msg["resource"], {})
            self.send(src, {"type": "expired", "id": msg["id"], "expired": not has})
        elif t == "expired":
            c = self.checks.pop(msg["id"], None)
            if c and msg["expired"]:
                self.table.get(c[0], {}).pop(c[1], None)

    def on_timer(self, tag):
        self.checks.clear()
        for res, e in self.table.items():
            for uid, (_, _, since) in e.items():
                if self.now() - since >= STALE_AGE:
                    for peer in self.servers:
                        if peer != self.id:
                            self.n += 1
                            self.checks[self.n] = (res, uid)
                            self.send(peer, {"type": "expired?", "id": self.n, "resource": res, "uid": uid})
        self.set_timer(MAINT, "maint")


class LockClient(ClientBase):
    def on_start(self):
        self.counter = 0
        self.want = {}
        self.held = {}

    def _quorum(self, mode):
        n = len(self.servers)
        return n // 2 + 1 if mode == "write" else (n + 1) // 2

    def lock(self, resource, mode):
        self.want[resource] = {"mode": mode}
        self._attempt(resource)

    def _attempt(self, resource):
        w = self.want[resource]
        self.counter += 1
        w.update(uid=f"{self.id}/{self.counter}", grants=set(), replies=set())
        for s in self.servers:
            self.send(s, {"type": "lock", "resource": resource, "mode": w["mode"], "uid": w["uid"]})
        self.set_timer(WAIT, ("timeout", resource, w["uid"]))

    def _release(self, res, uid, servers):
        for s in servers:
            self.send(s, {"type": "release", "resource": res, "uid": uid})

    def on_message(self, src, msg):
        if msg["type"] != "reply":
            return
        res, uid = msg["resource"], msg["uid"]
        w = self.want.get(res)
        if w is None or w["uid"] != uid:
            if msg["ok"] and self.held.get(res) != uid:
                self._release(res, uid, [src])
            return
        w["replies"].add(src)
        if msg["ok"]:
            w["grants"].add(src)
        if len(w["grants"]) >= self._quorum(w["mode"]):
            del self.want[res]
            self.held[res] = uid
            self.acquired(res)
        elif len(self.servers) - (len(w["replies"]) - len(w["grants"])) < self._quorum(w["mode"]):
            self._fail(res)

    def _fail(self, res):
        w = self.want[res]
        self._release(res, w["uid"], self.servers)
        w["uid"] = None
        self.set_timer(1 + int(self.random() * 4 * MAX_DELAY), ("retry", res))

    def on_timer(self, tag):
        w = self.want.get(tag[1])
        if w is None:
            return
        if tag[0] == "timeout" and w["uid"] == tag[2]:
            self._fail(tag[1])
        elif tag[0] == "retry" and w["uid"] is None:
            self._attempt(tag[1])

    def unlock(self, resource):
        self._release(resource, self.held.pop(resource), self.servers)
