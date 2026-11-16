"""Seeded flaws for calibration: each must fail at least one hidden test.

MUTANTS: (name, flaw class, [(old, new), ...]) edits to reference.py.
BASELINES: whole implementations of the original design, as {dest: file in private/}.
"""

MUTANTS = [
    ("no-restart-grace", "restart-amnesia (K4)",
     [("elif self.now() < self.started + LEASE:", "elif False:")]),
    ("release-by-client", "stale-release",
     [('self._live(msg["resource"]).pop(msg["uid"], None)',
       'e = self._live(msg["resource"])\n'
       '            for u in [u for u, v in e.items() if v[0] == src]:\n'
       '                del e[u]')]),
    ("count-duplicate-grants", "duplicate-grant",
     [('w["grants"][src] = msg["expires"]', 'w["grants"][len(w["grants"])] = msg["expires"]')]),
    ("ignore-grant-expiry", "late-grant",
     [('msg["ok"] and msg["expires"] > self.now() + HOLD_MAX + 1', 'msg["ok"]'),
      ("ATTEMPT = 2 * MAX_DELAY + 2", "ATTEMPT = 400"),
      ('if self.now() - msg["sent"] > STALE:', "if False:")]),
    ("even-write-quorum", "quorum-intersection",
     [("w = n // 2 + 1\n        return", "w = (n + 1) // 2\n        return")]),
    ("small-read-quorum", "quorum-intersection",
     [("else n - w + 1", "else n // 2")]),
    ("no-lease", "stale-lock-liveness (K3/K5)",
     [("expires = self.now() + LEASE\n            else:", "expires = 10**9\n            else:"),
      ("expires = self.now() + LEASE\n            if ok", "expires = 10**9\n            if ok")]),
    ("no-retry", "contention-liveness",
     [("        self.set_timer(1 + int(self.random() * 4 * MAX_DELAY), (\"retry\", resource))\n", "")]),
    ("readers-exclusive", "read-sharing",
     [('ok = all(e[1] == "read" for e in entries.values())', "ok = not entries")]),
]

BASELINES = {
    # dsync as built in 2020: stale locks never clear (K3/K5), restart amnesia (K4)
    "dsync-original": {"solution.py": "dsync_original.py"},
    # after upstream's K1/K3/K5 fixes; restart amnesia (K4) remains
    "dsync-fixed": {"solution.py": "dsync_fixed.py", "dsync_original.py": "dsync_original.py"},
}


def apply(reference: str, edits) -> str:
    out = reference
    for old, new in edits:
        if out.count(old) != 1:
            raise ValueError(f"edit target must occur once: {old!r}")
        out = out.replace(old, new)
    return out
