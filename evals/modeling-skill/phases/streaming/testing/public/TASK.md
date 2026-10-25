# Test event-time window aggregation

Read `window.py`, `events.p`, and `contract.p`. Add a saved Python unittest module named `test_window.py` that exercises the public service and validates observed calls and returned output records with the P monitor. Do not modify the supplied files.

Contract and event rules:

- `Service.add(timestamp, value)` returns `(accepted, outputs)`; `Service.advance(watermark)` returns `outputs`.
- Output dictionaries have `start`, `end`, `total`, `revision`, and `final`.
- Record actual results in public call order: one `eAdd` or `eWatermark`, one `eOutput` per returned record, then `eEndCall`.
- Width is 10; allowed lateness is 5. A window is `[floor(timestamp/10)*10, start+10)`.
- Watermarks start at zero and never move backward. Adds are accepted only while `watermark < end+5`.
- Accepted late adds update an emitted sum and return a correction. Empty windows never emit.
- If an advance first reaches `end+5` without an earlier output, it emits one final record at revision 0. Otherwise final uses the next revision, even when the total is unchanged.
- Outputs from one advance are sorted by window start.
- Preserve public call order. `eAdd.timestamp` is the event timestamp and may move backward relative to earlier adds.
- Include `eEndCall` after every method's full returned output list so the monitor can detect omissions.

