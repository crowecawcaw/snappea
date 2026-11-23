# Snap Pea for Python

Check [P](https://p-org.github.io/P/) specs against events your code emits during its
ordinary tests. The package brings the `snappea` binary, so `pip install pysnappea` is the whole
setup.

```python
from snappea import emit

def grant(self, client, lock):
    ...
    emit("eEnter", key=lock, client=client)
```

```toml
# pyproject.toml
[tool.snappea]
spec_files = ["specs/*.p"]
```

Every pytest test now also checks every spec in `specs/`. Or check selected tests
explicitly:

```python
from snappea import check_spec

def test_lease_expiry():
    with check_spec("MutualExclusion"):
        ...
```

In production, `snappea.set_sink("/var/log/app/events.jsonl")` sends the same events to a
log that `snappea observe` checks. See
the [Python guide](https://snappea.dev/docs/python.html) and
[Testing and production](https://github.com/crowecawcaw/snappea/blob/main/docs/production.md).
