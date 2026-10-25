# Frozen testing-phase v3 audit

Scope: source-first review of frozen `phase-test-v3-01` and `phase-test-v3-02` submissions and their tool transcripts only. No grading files were read. The v2 model runs still had no frozen `submission/` directories when checked, so no model-v2 claim is included.

## Did v3 improve the targeted tests?

Yes. Both suites now follow the added guidance to isolate guards and to test stale operations against a live successor.

- **Isolated write guards:** Each first establishes an active Alice lease, then separately tries Bob with Alice’s valid token and Alice with a wrong token. In both suites, a public `read()` follows each rejected write, before a successful write can overwrite evidence (`v3-01/test_lease.py:107-123`; `v3-02/test_lease.py:97-108`). This would detect a false-returning rejected write that nevertheless changes the integer value.
- **Isolated release guards:** Each separately tests wrong-owner and wrong-token releases while Alice’s lease is active, then checks `peek()` still shows Alice’s lease after each rejection (`v3-01/test_lease.py:125-136`; `v3-02/test_lease.py:110-121`).
- **Stale operations against a successor:** Each expires Alice’s token, lets Bob acquire a new live lease, then sends Alice’s old token to both `write` and `release`. V3-01 immediately reads the unchanged value after Alice’s rejected write, checks Bob’s lease after the stale release, and then confirms Bob can still write (`v3-01/test_lease.py:138-157`). V3-02 does the same (`v3-02/test_lease.py:123-140`). These observations would catch the two targeted false-returning side effects: changing the value on stale write and clearing/changing the successor lease on stale release. The probes occur before later successful work could conceal either effect.

Both wrappers still capture actual public inputs/results and actual public `read`/`peek` returns, then pass those histories to `check_events` with strict handling and unknown-event errors. Token checks accept any positive strictly increasing allocation, consistent with the task clarification. The test histories remain deterministic and serial; they validate sequential API behavior and event provenance, not concurrent implementation schedules.

The transcripts show both agents read the updated skill. Each initially omitted `events.p` from `spec_files`, got a monitor setup error, added the event declarations, and reran the complete unit suite successfully (v3-01 calls 9-10; v3-02 calls 5-6). The frozen submissions contain the correction.

## Remaining scenario limits

The new successor test is strong for stale write/release results and visible effects. It exercises the stale credential with the old owner, which is the key distinction missing in v2. It does not separately test stale `acquire` behavior after an old owner’s lease has expired, but that is not the rejection-guard/stale-operation gap the v3 skill additions target. Neither phase provides concurrency evidence because the supplied service is deterministic and single-threaded.

## Skill prose review

The current `SKILL.md` is about 328 words in its body (345 including front matter), so it is compact for an entry point and the new generic statements earn their space: “Isolate rejection conditions” and “Test stale operations against newer live state” directly address the v2 misses, and the frozen v3 tests reflect both. I do not see a substantial block to cut. If shortening the guides further, there is mild overlap between “Check later public reads when an announcement alone cannot establish that state” in `scenarios-and-oracles.md` and “Check state after rejected/stale operations before later successful work can overwrite evidence”; they could be combined as one rule about observing resulting public state before subsequent work. The surrounding detail explains distinct event-provenance, bound, and progress limits and is not obviously redundant.
