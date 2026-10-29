# Independent example review

Read `/workspace/snappea-skill/AGENTS.md`. Reviewed all example families and `reference/p-patterns.md`; no source edits and no eval agents/private cases. Used only the prebuilt `/workspace/snappea-skill/target/driver-eval/release/snappea`.

## Findings (both closed after independent recheck)

### Closed P2: Durable retry validates a duplicate audit payload, not the actual returned result

Sources: `examples/durable-retry/PSrc/RetryModel.p:20-22,31-33`; `PSpec/Contract.p:32-45`; `PTst/Driver.p:171-174` (and the sequential eReply handlers).

`eOutcome` contains the correct memo result, but the separately constructed actual `eReply` payload is discarded by every driver. Thus the documented contract "return its original result on retry" is not checked against the returned value. A temp-copy probe replaced only the two DurableService eReply result expressions with `999`, retaining the eOutcome announcements. All three positive tests still exited 0, completed their searches, and passed aggregate required coverage. This is a narrow, realistic response-wiring defect, not an omitted environmental feature.

Concrete fix: observe `eReply` at send (as versioned-publication does) and validate that actual payload against independently saved commit results/current total, replacing redundant eOutcome audit observations; or announce `eReturned` at receipt and validate immutable id/result there while preserving the service-side currentTotal observation. Do not compare arbitrary client receipt-time currentTotal to the latest global total. Add a named wrong-reply control.

### Closed P2: Durable retry's independent oracle accepts explicit zero-work completion

Source: `examples/durable-retry/PSpec/Contract.p:48-52`.

The only completion guards are `expectedTotal == requestedIds` and `returnedIds == requestedIds`. Both hold at zero. A one-event JSONL containing only `{"event":"eScenarioDone"}` exits 0 under `observe ... --strict --check-hot`. This contradicts the README's useful-success/completion teaching when the monitor is reused for histories. The current fixed drivers do execute real work, so this is an oracle robustness gap rather than a failure of their base searches.

Concrete fix: assert `requestedIds > 0` at completion and save the one-event trace as a negative oracle probe. A scenario-specific declared expected request count is stronger if desired, but is not needed to close this demonstrated hole.

## Verified strengths

- CAS retry is the clearest flagship: true client snapshot/candidate construction, separate load/CAS scheduling points, whole-record CAS over balance+memo, failed-CAS reload/recomputation, stable request definitions, and a barrier placing retries after both first calls. Oracle derives acceptance and balance independently, immediately checks rejection/retry state, and checks actual client returns. `check_examples.py` passed the correct complete suite, all four assertion-specific fault controls (lost update, replay reapplication, rejection mutation, corrupt reply), and valid/invalid hand traces. Negative controls distinguish property failures from compiler/coverage/timeouts.
- Durable retry's concurrency fix correctly separates requested obligations from unique committed effects; pending second requests do not inflate expectedTotal. All three positive searches completed with aggregate required coverage. Both advertised broken variants failed `one request caused more than one effect`, rather than setup or coverage failure. Documented crash/interleaving bounds are honest.
- Versioned publication keeps evidence/base record/candidate separate from CAS expectation and recomputes all on conflict. Two distinct started/completed clients plus two commits prevent abandoning a loser from passing. Positive search completed (604 schedules), and the saved missing-completion control passed by requiring an actual hot-state liveness bug.
- Reservation Python tests passed all four cases. The race barrier forces both snapshots before either write; implementation uses a lock for consistent reads and conditional stock+record commits. Real capture records implementation source locations. Rejection tests isolate stale-version/capacity/duplicate guards against otherwise-valid requests and check state before useful follow-up success. Broken stale writes fail both committed-event capacity and independently recomputed inventory invariants. Post-lock event ordering is explicitly limited to an order-independent invariant.
- The stronger syntax-reference fenced model was copied verbatim into a temporary `.p` file and checked successfully with complete search and required coverage. Independent requested additions, actual sent replies, explicit fresh IDs, two useful successes, and hot completion make it a meaningful starter example. Its sequential scope is explicit, avoiding receipt-order misuse.
- Bounded history now distinguishes fixed inputs/scheduled interleavings from generated sequential/concurrent inputs, declares nonzero work, matches decisions and client replies by stable correlation ID, and separately observes actual Store snapshots after commands. It accurately explains that other clients can intervene before concurrent inspection and that its atomic API abstraction does not model a split client CAS algorithm. No additional material defect found in inspected version.

## Limits / artifacts

No source files changed. Several early base checks were accidentally run in example source directories rather than temp copies; they only produced checker output artifacts (`PCheckerOutput`, ignored inside example dirs). Subsequent mutation/reference probes were isolated temp copies. No binary rebuild, no private eval files/results accessed.

Final bounded-history targeted verification used an isolated temp copy: all four positive cases complete and aggregate required coverage passes (105298 fixed-input concurrent schedules; 24 fixed sequential; 242 generated sequential; 1311 generated concurrent). All three broken cases fail intended acceptance/snapshot assertions. Both valid traces pass; invalid decision/state/reply, incomplete, and zero-work traces fail named property/hot-state assertions. No additional material bounded-history or syntax-reference findings.

## Final review after durable fixes

Both findings are closed. Independently inspected the final sources: `PSpec/Contract.p:6,9,32` now declares and observes actual `eReply`, with original-result/current-total checks at send; there is no parallel eOutcome payload. Sequential and concurrent drivers check received IDs against pending logical requests. `PSpec/Contract.p:49` explicitly requires nonzero requested work.

Rechecked entirely in an isolated temporary copy using the same prebuilt binary:

- All three positive durable workloads exit 0, all searches complete, aggregate required coverage passes with no gaps.
- Saved `python -m unittest -v` guard passes: corrupting only actual reply results to 999 makes every positive workload fail the original-result assertion.
- Valid concurrent history exits 0. Duplicate, unfinished, premature completion, reverted-current-total, and zero-work histories each exit 1 for their intended named assertion/hot state. In particular zero-work now fails `scenario completed without any requests`.
- Both broken durable designs still fail `one request caused more than one effect`.

Final assessment: no unresolved material findings in the reviewed examples or syntax reference. The final examples meet their stated bounded scope and are ready for the parent's eval freeze. This is a review of the models/examples and saved regression controls, not implementation refinement or unbounded correctness evidence.
