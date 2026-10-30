# Unique usernames

Implement `Registry` in `solution.py`, keeping all state in `runtime.Store` (see
`runtime.py`). Each user holds at most one name, and each name belongs to at most one
user.

```python
Registry(store)
register(user: str, name: str) -> bool
rename(user: str, new_name: str) -> bool
owner(name: str) -> str | None
name_of(user: str) -> str | None
```

- Names are 1 to 16 characters from `a-z`, `0-9` and `-`, compared case-insensitively:
  `Bob` and `bob` are the same name. Store and return names in lower case. Calls with
  an invalid name return `False` (or `None` for `owner`) and change nothing.
- `register` gives `user` the name if the user has no name and nobody holds the name.
  Registering the name the user already holds returns `True`. Otherwise `False`.
- `rename` moves a user who holds a name to `new_name` if nobody else holds it; the
  old name becomes free. Renaming to the name the user already holds returns `True`.
  On `False`, nothing changes.
- `owner` and `name_of` return the current holder and name.
- Many `Registry` objects in many threads may share one `Store`. Calls for the same
  user never overlap; calls for different users may. When no call is in progress,
  `owner(n) == u` exactly when `name_of(u) == n`.

Use P to model the design before implementing it, and to test the implementation.
`snappea` and the `pysnappea` Python package are installed. Keep your P files and
tests in this directory; the tests must run with `python -m pytest`.
