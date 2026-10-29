# Bounded histories: inputs, interleavings, state, completion

## Contract and invariants

- Initial stock is 2. Purchase succeeds iff its positive quantity fits; successful purchases subtract it. Replenishment succeeds and adds its quantity. Rejection changes no stock.
- Each command has a **fresh correlation ID**. IDs do not imply retry/idempotency semantics here; see [durable retry](../durable-retry/README.md).
- `CapacityOracle` records driver inputs, checks each decision against its own stock, matches each returned result by ID, then compares an actual public snapshot against that stock. It never reads `Store.remaining`.
- A correct rejection alone is insufficient: `tcRejectedWriteChangesState` returns the correct rejection while silently adding stock. The subsequent public inspection exposes it.
- Every client waits for each command's reply and inspection before its next command. Completion requires the declared, nonzero command count, one decision/checked reply/inspection per ID, and every client finishing. A hot monitor detects missing work or replies.

## Why these boundaries?

- Store check/update is one non-yielding region modeling an **actual atomic storage primitive**. This example tests history generation, not an implementation's split read/CAS algorithm. If those steps can interfere, model them separately as in [versioned publication](../versioned-publication/README.md).
- Decision and snapshot observations occur at the Store, before delayed replies. Client receipt order is not used as a linearization witness.
- Other clients may commit between a command and its inspection. The oracle includes those intervening decisions; it does not incorrectly demand the stock immediately after the earlier command.
- Reads after rejection prevent later work from masking its effect in sequential histories. The concurrent case additionally varies where other clients intervene; it does not guarantee isolation between rejection and inspection.

## Drivers

All reuse `HistoryClient`; each preserves the same request/reply/inspection protocol.

| Test | Inputs | Scheduling |
| --- | --- | --- |
| `tcFixedInputHistory` | One client, three one-unit purchases | Sequential; third purchase must reject |
| `tcSchedulerChoosesInterleaving` | Three clients, one one-unit purchase each | Inputs fixed, independent clients compete |
| `tcGeneratedHistory` | One client, three choices of purchase/replenish and quantity 1/2 | Sequential generated histories |
| `tcConcurrentGeneratedHistory` | Two clients, each chooses purchase/replenish of 2 units | Generated inputs **and** competing clients |

Small bounds preserve depletion, exact-fit, excess, replenishment, and conflicting purchases. Concurrent quantities stay fixed at 2 to keep complete exploration quick; sequential generation covers quantity variation. These are deliberate partitions, not an argument that longer histories are unnecessary.

## Run the positive suite

```sh
snappea check bounded-history.p tcFixedInputHistory tcSchedulerChoosesInterleaving tcGeneratedHistory tcConcurrentGeneratedHistory --no-mutation --timeout 5s
```

All four searches complete and the suite covers required statements/branches/handlers. Run together: each intentionally narrow test need not cover every branch of shared driver/oracle code. The checker reports those individual coverage gaps rather than silently suppressing them. `--no-mutation` keeps this input/scheduling lesson focused; ordinary checks can additionally inspect mutation feedback.

## Negative controls

```sh
snappea check bounded-history.p tcBrokenInputHistory --no-mutation
snappea check bounded-history.p tcBrokenGeneratedHistory --no-mutation
snappea check bounded-history.p tcRejectedWriteChangesState --no-mutation
```

- First two exit 1 because a purchase was accepted without enough stock.
- Third exits 1 with `public stock 1, expected 0`: rejection was reported correctly but changed state.
- Fault parameters are only negative controls to demonstrate that assertions distinguish incorrect behavior.

## Independent monitor probes

These are hand-calculated histories, **not** captured implementation evidence.

```sh
snappea observe bounded-history.p valid.jsonl --spec CapacityOracle --strict --check-hot
snappea observe bounded-history.p valid-concurrent.jsonl --spec CapacityOracle --strict --check-hot
snappea observe bounded-history.p invalid.jsonl --spec CapacityOracle --strict --check-hot
snappea observe bounded-history.p invalid-state.jsonl --spec CapacityOracle --strict --check-hot
snappea observe bounded-history.p invalid-reply.jsonl --spec CapacityOracle --strict --check-hot
snappea observe bounded-history.p incomplete.jsonl --spec CapacityOracle --strict --check-hot
snappea observe bounded-history.p zero-work.jsonl --spec CapacityOracle --strict --check-hot
```

- Valid serial/concurrent traces exit 0. Concurrent requests can both be outstanding; later public reads include all earlier committed decisions.
- Invalid decision/state/reply traces exit 1 for the intended capacity/snapshot/returned-result assertion.
- Incomplete trace omits client completion and exits 1 for the remaining hot state; zero-work trace fails the explicit nonzero-work assertion.
- Implementation validation needs real recorded calls/results and a justified observation order; see [reservation's Python tests](../reservation/README.md).

## Assumptions and omissions

- Finite history lengths above; one process-local Store; positive quantities; fresh command IDs; reliable delivery and clients that continue running.
- No crashes, persistence, request retries, conflicting ID reuse, storage failures, or unbounded recovery/fairness claims.
- A completed finite search establishes only these modeled bounds. Broaden relationships and properties deliberately; schedule count is not semantic coverage.
