"""Seeded flaws for calibration: each must fail at least one hidden test.

MUTANTS: (name, flaw class, [(old, new), ...]) edits to reference.py.
BASELINES: whole implementations, as {dest: file in private/}.
"""

MUTANTS = [
    ("no-restart-grace", "restart-amnesia (K4)",
     [("elif self.now() < self.started + GRACE:", "elif False:")]),
    ("no-drift-margin", "clock-drift",
     [("VALID = math.floor(LEASE * (1 - RHO) / (1 + RHO)) - 2", "VALID = LEASE - 2")]),
    ("trust-server-clock", "clock-drift",
     [('"seq": msg["seq"], "ok": ok})', '"seq": msg["seq"], "ok": ok, "expires": self.now() + LEASE})'),
      ('until = self.sent.get(msg["seq"], -math.inf) + VALID', 'until = msg.get("expires", 0) - 2')]),
    ("valid-from-reply", "late-grant",
     [('until = self.sent.get(msg["seq"], -math.inf) + VALID', "until = self.now() + VALID")]),
    ("never-step-down", "lost-lease",
     [('elif self._valid(h["grants"], CHECK) < self._quorum(h["mode"]):', "elif False:")]),
    ("no-renew", "lost-lease",
     [('                self._request(res, h["mode"], h["uid"])\n', "")]),
    ("count-duplicate-grants", "duplicate-grant",
     [('state["grants"][src] = max(state["grants"].get(src, -math.inf), until)',
       'state["grants"][(src, msg["seq"], len(state["grants"]))] = until')]),
    ("even-write-quorum", "quorum-intersection",
     [("w = n // 2 + 1\n        return", "w = (n + 1) // 2\n        return")]),
    ("small-read-quorum", "quorum-intersection",
     [("else n - w + 1", "else n // 2")]),
    ("no-lease", "stale-lock-liveness (K3/K5)",
     [("entries[uid] = (src, mode, self.now() + LEASE)", "entries[uid] = (src, mode, 10**12)")]),
    ("readers-exclusive", "read-sharing",
     [('ok = all(e[1] == "read" for e in entries.values())', "ok = not entries")]),
    ("no-retry", "contention-liveness",
     [("        self.set_timer(1 + int(self.random() * 4 * MAX_DELAY), (\"retry\", resource))\n", "")]),
]

BASELINES = {
    # dsync as built in 2020: K1 (now visible with long holds), K3/K5, K4
    "dsync-original": {"solution.py": "dsync_original.py"},
    # after upstream's K1/K3/K5 fixes; K4 remains, and it never steps down
    "dsync-fixed": {"solution.py": "dsync_fixed.py", "dsync_original.py": "dsync_original.py"},
    # dlock's reference: relies on a hold bound and compares clocks across nodes
    "dlock-reference": {"solution.py": "v1_reference.py"},
}


def apply(reference: str, edits) -> str:
    out = reference
    for old, new in edits:
        if out.count(old) != 1:
            raise ValueError(f"edit target must occur once: {old!r}")
        out = out.replace(old, new)
    return out
