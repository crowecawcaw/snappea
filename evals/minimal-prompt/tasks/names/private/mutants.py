"""Realistic faulty registries: (name, defect, [(old, new), ...]) edits to reference.py."""

MUTANTS = [
    ("case-sensitive", "Bob and bob are different names",
     [("    name = name.lower()\n", ""), ('re.compile(r"[a-z0-9-]{1,16}")', 're.compile(r"[A-Za-z0-9-]{1,16}")')]),
    ("length-17", "accepts 17-character names", [("{1,16}", "{1,17}")]),
    ("empty-name", "accepts the empty name", [("{1,16}", "{0,16}")]),
    ("claim-ignores-cas", "two users racing for a free name both get it",
     [('            if self.store.compare_swap("name:" + name, version, user):\n                return True',
       '            self.store.compare_swap("name:" + name, version, user)\n            return True')]),
    ("claim-steals", "claiming a name someone else holds succeeds",
     [("                return holder == user", "                return True")]),
    ("free-before-claim", "rename frees the old name before claiming the new one",
     [("        if not self._claim(user, new):\n            return False",
       "        self._free(user, old)\n        if not self._claim(user, new):\n            return False"),
      ("            return False\n        self._free(user, old)\n        return True", "            return False\n        return True")]),
    ("rename-keeps-old", "rename never frees the old name",
     [("            return False\n        self._free(user, old)\n        return True", "            return False\n        return True")]),
    ("register-same-false", "registering the name you hold returns False",
     [("            return current == name", "            return False")]),
    ("rename-same-frees", "renaming to your own name frees it",
     [("        if old == new:\n            return True\n", "")]),
    ("register-second-name", "a user with a name can register another",
     [("        if current is not None:\n            return current == name",
       "        if current == name:\n            return True")]),
    ("rename-without-name", "rename works for a user with no name",
     [("        if old is None:\n            return False\n", ""),
      ("    def _free(self, user, name):\n", "    def _free(self, user, name):\n        if name is None:\n            return\n")]),
]


def apply(source, edits):
    """Apply each (old, new) edit to every occurrence of old; each must occur at least once."""
    for old, new in edits:
        if source.count(old) < 1:
            raise ValueError(f"edit does not apply: {old[:60]!r}")
        source = source.replace(old, new)
    return source
