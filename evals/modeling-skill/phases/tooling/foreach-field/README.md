# Foreach iterator scope: checker crash

On the pinned eval binary (`0391b4493a273b16b6494f570c5debff4bfdeb09722eaaf0972fb155857fe332`):

```sh
snappea observe events.p field.p history.jsonl --spec ForeachScope --strict
snappea observe events.p local.p history.jsonl --spec ForeachScope --strict
```

- Field iterator: exits 101 with an interpreter index-out-of-bounds panic.
- Handler-local iterator: exits 0.
- The map and iteration are identical. This is **not a general failure of `keys(map)` or nested maps**.
- `ForeachNext` writes `item_slot` into the execution frame; a machine-field slot can index outside that frame. The compiler currently uses the variable's slot without accounting for its storage role.
- A smaller field slot can instead update the wrong storage and produce a misleading runtime fault. Neither behavior is a useful diagnostic.
- This directory is a reproducible tooling finding, not a required passing example. No production checker changes were made for these evals.

Original larger calibration fixtures and raw logs are preserved in the evidence bundle. The working local-iterator variant confirms that general map iteration remains available; calibration monitors used equivalent iteration-free calculations before the cause was isolated.
