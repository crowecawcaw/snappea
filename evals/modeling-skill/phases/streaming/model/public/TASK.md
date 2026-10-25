# Model event-time window aggregation

Read `events.p`. Design a deterministic tumbling-window aggregator and model its public contract.

## Contract

- Window width is 10. An event at timestamp `t` belongs to the half-open window `[s,s+10)`, where `s = floor(t/10)*10`.
- Watermark starts at 0 and can only move forward. Watermark updates are explicit calls; there is no wall clock.
- Allowed lateness is 5. An add to `[s,s+10)` is accepted exactly while the current watermark is less than `s+15`. At or after `s+15` it is rejected with no aggregate or output effect.
- A window exists only after an accepted add. Empty windows never produce output.
- Once the watermark reaches `s+10`, an existing window must have an aggregate output. An add arriving after that boundary but before `s+15` is accepted and immediately emits an updated aggregate.
- Output records contain `start`, `end`, `total`, `revision`, and `final`. The first output has revision 0. Each later output for that window has the next integer revision. Every output carries the current sum of all accepted values for the window.
- When an add updates a window that already emitted, it returns one nonfinal correction. When a watermark advance first reaches a window's `end`, it emits the initial nonfinal aggregate. When one advance jumps directly to or past `end+5` before any output, it emits one final aggregate at revision 0. If a nonfinal output already exists, reaching `end+5` emits a final record at the next revision, even when the sum did not change. Outputs from one advance are ordered by window start.
- An advance may emit records for any existing windows whose boundaries it crosses. A rejected add emits no output. Each method returns all records it emits; `eEndCall` marks the end of that call's output list.
- Timestamps, watermarks, and values are integers; timestamps and watermarks are nonnegative; values may be negative. Ignore malformed inputs.
- Keep the model's timestamp range bounded to `[0,70)` so its finite-state check has a small number of windows; the public service contract is otherwise stated for nonnegative timestamps.

## Deliver

- `model.p`: a stateful aggregator whose service machine computes acceptance, sums, revisions, and outputs from actor-generated add and watermark calls. Do not hardcode the resulting event history as announcements. Use actor-generated calls across more than one window, including a late correction and finalization.
- `monitor.p`: independent `spec WindowContract` using `events.p`; check accepted/rejected adds, totals, revision ordering, output contents, final flags, and output completeness at every `eEndCall`.
- `tests.p`: selects the model and monitor.
- `DESIGN.md`: atomic public-call boundaries, finite model coverage, and omissions.

Events are in public call order. Each call's `eOutput` events follow its input event, then exactly one `eEndCall` closes the return value. The monitor is bounded to timestamps in `[0,70)`; its scope matches the model input bound above.


Run from this directory with the installed Snappea checker:

```sh
snappea check events.p monitor.p model.p tests.p
```
