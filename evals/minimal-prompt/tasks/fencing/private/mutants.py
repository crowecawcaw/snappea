"""Realistic faulty lock services: (name, defect, [(old, new), ...]) edits to reference.py."""

WRITE_LOOP = '''        while True:
            version, record = self.store.load(self.key)
            fence = record["fence"] if record else 0
            if token < fence:
                return False
            if self.store.compare_swap(self.key, version, {"fence": token, "value": value}):
                return True'''

MUTANTS = [
    ("token-reset-on-release", "release deletes the lock record, so tokens restart at 1",
     [('            record["holder"] = None\n',
       '            record = {"issued": 0, "holder": None, "token": 0, "expires": 0}\n')]),
    ("expiry-inclusive", "a lease is still held at exactly now == expiry",
     [('held = record["holder"] is not None and now < record["expires"]',
       'held = record["holder"] is not None and now <= record["expires"]')]),
    ("reacquire-new-token", "the holder re-acquiring gets a new token",
     [('return record["token"] if record["holder"] == client else None',
       'if record["holder"] != client:\n                    return None')]),
    ("expired-holder-keeps-token", "a client whose lease expired gets its old token back",
     [('held = record["holder"] is not None and now < record["expires"]',
       'held = record["holder"] is not None and (now < record["expires"] or record["holder"] == client)')]),
    ("acquire-ignores-cas", "two racing acquires can both win",
     [('            if self.store.compare_swap(LOCK, version, record):\n                return record["token"]',
       '            self.store.compare_swap(LOCK, version, record)\n            return record["token"]')]),
    ("release-ignores-token", "release with an old token ends the current lease",
     [(' and record["token"] == token and now < record["expires"]', ' and now < record["expires"]')]),
    ("release-ignores-expiry", "release succeeds after the lease expired",
     [(' and record["token"] == token and now < record["expires"]', ' and record["token"] == token')]),
    ("fence-rejects-equal", "a second write with the same token is rejected",
     [("if token < fence:", "if token <= fence:")]),
    ("no-issued-check", "write accepts a token that was never issued",
     [("if not 1 <= token <= issued:", "if token < 1:")]),
    ("fence-is-current-holder", "write accepts only the current lease's token",
     [("if not 1 <= token <= issued:",
       "if not lock or lock[\"holder\"] is None or token != lock[\"token\"]:")]),
    ("write-ignores-cas", "a write that loses a compare-and-swap still reports success",
     [('            if self.store.compare_swap(self.key, version, {"fence": token, "value": value}):\n                return True',
       '            self.store.compare_swap(self.key, version, {"fence": token, "value": value})\n            return True')]),
    ("write-stale-retry", "after a lost compare-and-swap, write retries without re-reading the fence",
     [(WRITE_LOOP, '''        version, record = self.store.load(self.key)
        while True:
            fence = record["fence"] if record else 0
            if token < fence:
                return False
            if self.store.compare_swap(self.key, version, {"fence": token, "value": value}):
                return True
            version = self.store.load(self.key)[0]''')]),
    ("shared-fence", "all resources share one fence",
     [('self.key = "resource:" + name', 'self.key = "resource"')]),
]


def apply(source, edits):
    for old, new in edits:
        if source.count(old) < 1:
            raise ValueError(f"edit does not apply: {old[:60]!r}")
        source = source.replace(old, new)
    return source
