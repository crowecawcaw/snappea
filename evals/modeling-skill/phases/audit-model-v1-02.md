# Frozen model-phase audit: phase-model-v1-02

Scope: source-first review of `submission/{TASK.md,events.p,model.p,monitor.p,tests.p,DESIGN.md}` and `tool-transcript.jsonl` only. No grading files or hidden grader cases were read. Transcript call numbers below are one-based.

## Finding

This is a useful bounded sequential contract model and a strong example of an event-derived state monitor, but it does not model the competing storage-call behavior requested by the task. The author states that limitation clearly in `DESIGN.md`; the artifact should therefore receive credit for contract modeling and honest scope, not for validating the CAS concurrency boundary.

## Model meaning and concurrency

`model.p:8-30` has one `Main` driver that sends a fixed series of requests to one `LeaseAPI` machine. The apparent clients A, B, and C are request labels; there are no independent client machines. Every public operation is handled as one non-yielding function in `LeaseAPI` (`model.p:32-90`), where the complete check and state update happen together. The model contains no storage read/version result, CAS attempt, CAS failure, stale snapshot, or retry. It therefore serializes whole public calls by construction.

The schedule count does not demonstrate a race. In call 23 the checker reports 226 schedules and a complete search, but the model has only the one scripted request stream and one state owner. The schedules explore scheduling around message delivery, not two clients making independently scheduled decisions from the same record version. `DESIGN.md:5` accurately says the model omits the read/CAS gap and racing stale clients. That disclosure is good; it does not satisfy the task’s request to model calls that may interleave at the given storage boundary.

The design note’s proposed one-record CAS loop is plausible as an implementation refinement: placing the value, lease, clock, and token allocator in one versioned record gives one commit point, and retrying after failed CAS can serialize conflicting updates. The P model does not establish that protocol, however. There is no evidence in it that a failed CAS forces a reread and recomputation before retry, or that every operation’s result corresponds to the record version it commits.

## Monitor quality and contract fidelity

The monitor is meaningfully stateful and separate from `LeaseAPI`: it reconstructs clock, value, owner, expiry, and token state from public events and checks reported acquire/write/release results plus later `read`/`peek` observations (`monitor.p:1-50`). The model emits those public outcomes, and the fixed trace includes successful and rejected calls, exact expiry, a later acquisition, and public state reads (`model.p:13-27`). This is substantially better than a P-only assertion over internal state or a monitor that trusts an implementation-supplied validity flag.

There is one material overconstraint. The contract permits any positive token greater than all previously issued tokens (`TASK.md:8`), but the monitor initializes `nextToken` to 1 and requires each successful acquisition to return exactly the next consecutive integer (`monitor.p:10,22-29`). A correct implementation could issue 41 and later 90; the monitor would reject that valid history. The model’s consecutive-token choice is a valid refinement, but the standalone contract monitor must check positivity and strict increase, not a particular allocation sequence. This matters because the task explicitly requires the monitor to work on event histories without the model.

The monitor’s otherwise similar `Active`/`ClearExpired` logic is independently stateful, but it mirrors the model’s formulas closely (`model.p:53-57`, `monitor.p:18-21`). That makes review of shared assumptions important: a mistaken expiry convention copied into both could pass model-generated traces. Public `read` and `peek` checks help expose effect errors, while the written contract and a small external-history suite should remain the source of truth.

The script deliberately sends `ttl = 0` (`model.p:16`) and the design claims to check “invalid TTL” (`DESIGN.md:7`). The task states TTLs are positive preconditions (`TASK.md:7`), so zero-TTL behavior is outside the required input domain. Returning `-1` is a reasonable defensive extension, but it should be labeled as such rather than counted as contract coverage. The other exercised branches align with the stated contract: owner/token validation, rejection without value change, exact expiry, token nonreuse in the chosen allocation policy, and release preserving the integer value.

## Guidance likely to change the approach

The useful general lesson is to make the model’s atomic transition match a named real primitive. A full public handler may be represented as one step only when the implementation has a transaction, lock, or equivalent operation that makes the whole handler atomic. When storage exposes a read followed by conditional write, represent those as separate schedulable interactions; give competing clients independent control flow over the same shared record; model CAS success and failure; and let a failed attempt return to a fresh read before retry. A model that has one driver send a fixed request queue to one state owner is a serial history check, even if it has many schedules or several client names.

The entry guidance should also separate two claims: an event monitor can check public contract histories, while a storage-level model must show that the proposed primitive sequence preserves those histories under interference. State which claim each artifact supports, and do not treat coverage, schedule count, or a passing sequential scenario as evidence for the second claim.

## Assessment

- **Contract monitor:** good independent event-state reconstruction, with a strict-increasing-token bug that rejects valid implementations.
- **Sequential model:** useful concrete bounded API history with state-owning machine and visible result events.
- **Concurrent design model:** absent; no independent clients or modeled read/CAS/retry interleavings.
- **Reporting:** the author correctly documents this gap and the bounded scope. The reported 226 complete schedules should be described as schedules of this fixed serial model, not CAS-race exploration.
