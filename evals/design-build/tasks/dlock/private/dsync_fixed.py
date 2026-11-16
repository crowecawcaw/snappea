"""dsync after upstream's fixes for K1 and K5 (quorum-vouched maintenance that also
asks the owner) and K3 (unlock retried until a quorum acknowledges). K4 remains: a
restarted server grants at once. This is the plausible "careful" solution that still
misses restart amnesia.
"""
from sim import MAX_DELAY
import dsync_original as base

MAINT, STALE_AGE = base.MAINT, base.STALE_AGE


class LockServer(base.LockServer):
    def on_message(self, src, msg):
        t = msg["type"]
        if t == "release":
            self.table.get(msg["resource"], {}).pop(msg["uid"], None)
            self.send(src, {"type": "released", "resource": msg["resource"], "uid": msg["uid"]})
        elif t == "expired":
            c = self.checks.get(msg["id"])
            if c:
                c["answers"] += 1
                c["vouch"] += 0 if msg["expired"] else 1
        elif t == "holding":
            c = self.checks.get(msg["id"])
            if c:
                c["owner"] = msg["holding"]
        else:
            super().on_message(src, msg)

    def on_timer(self, tag):
        if tag == "decide":
            q = len(self.servers) // 2 + 1
            for c in self.checks.values():
                # purge if the owner does not confirm (no answer counts as down), or if
                # fewer than a quorum vouch (self included)
                if c["owner"] is not True or c["vouch"] + 1 < q:
                    self.table.get(c["res"], {}).pop(c["uid"], None)
            self.checks = {}
            return
        self.checks = {}
        for res, e in self.table.items():
            for uid, (owner, _, since) in e.items():
                if self.now() - since >= STALE_AGE:
                    self.n += 1
                    self.checks[self.n] = {"res": res, "uid": uid, "vouch": 0, "answers": 0, "owner": None}
                    for peer in self.servers:
                        if peer != self.id:
                            self.send(peer, {"type": "expired?", "id": self.n, "resource": res, "uid": uid})
                    self.send(owner, {"type": "holding?", "id": self.n, "resource": res, "uid": uid})
        self.set_timer(2 * MAX_DELAY + 2, "decide")
        self.set_timer(MAINT, "maint")


class LockClient(base.LockClient):
    def on_start(self):
        super().on_start()
        self.unacked = {}   # (res, uid) -> set of servers not yet acked

    def on_message(self, src, msg):
        t = msg["type"]
        if t == "holding?":
            want = self.want.get(msg["resource"])
            mine = self.held.get(msg["resource"]) == msg["uid"] or (want and want.get("uid") == msg["uid"])
            self.send(src, {"type": "holding", "id": msg["id"], "holding": bool(mine)})
        elif t == "released":
            self.unacked.get((msg["resource"], msg["uid"]), set()).discard(src)
        else:
            super().on_message(src, msg)

    def unlock(self, resource):
        uid = self.held.pop(resource)
        self.unacked[(resource, uid)] = set(self.servers)
        self._resend(resource, uid)

    def _resend(self, res, uid):
        left = self.unacked.get((res, uid))
        if left and len(self.servers) - len(left) < len(self.servers) // 2 + 1:
            self._release(res, uid, left)
            self.set_timer(2 * MAX_DELAY + 2, ("unlock", res, uid))
        else:
            self.unacked.pop((res, uid), None)

    def on_timer(self, tag):
        if tag[0] == "unlock":
            self._resend(tag[1], tag[2])
        else:
            super().on_timer(tag)
