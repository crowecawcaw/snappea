# Durable retry after a lost response

This small P project checks one recovery history for one operation. A client sends request ID `7`; the service commits the effect, but the client receives no response before the process restarts. The client retries ID `7`. The contract requires one effect and one successful final outcome. The internal `eCommitted` message only tells the test driver where to inject the restart; it is not a client response.

## Run it

From this directory, with `snappea` installed and on `PATH`, run:

```sh
snappea check tcDurableRetry
snappea check tcSplitCommitRetry
```

The first command passes. It runs lint, explores the complete bounded schedule space, and runs the default mutation checks. With the current checker it explores 84 schedules, covers all 30 statements, all 4 branches, and all 8 handlers, and detects all 10 mutants.

The second command is deliberately expected to exit with status 1. The report should identify `one request caused more than one effect` in `RetryContract`. This is the property failure the example is meant to expose. Do not treat the combined `snappea check` with no test name as a clean run: it includes this deliberately broken case and should fail for that reason.

If building from source, run `cargo build --release -p snappea` from the repository root and add `target/release` to `PATH`.

## What the model preserves

`RetryDriver` is separate from the service model. It announces a request, receives the internal commit notification, then crashes and restarts the service without receiving a client reply. That boundary represents an ambiguous commit: the durable operation succeeded, while the client cannot know its outcome. The driver then retries the same ID, representing the same logical operation rather than a new one.

`DurableService` keeps `durableTotal` and `durableOutcomes` across restart. On a first request, adjacent local assignments increment the effect and record the replayable result before the next communication scheduling point. That non-yielding update region stands for one real atomic storage primitive, such as a database transaction or a compare-and-swap that updates one durable record containing both values. P handlers can yield at communication operations such as `send`, `receive`, or `create`; this example does not treat an entire handler as atomic. The implementation must provide the same co-commit boundary for the model to apply.

`SplitCommitService` is the broken design. It increments the durable effect, then relies on a separate `eRemember` transition to populate `volatileMemo`. The driver crashes after the first commit, before that later transition, so restart clears the memo. The retry increments the effect again. The monitor catches the second `eCommit` for ID `7` before the driver can report an outcome.

`RetryContract` is independent of the service's state. It counts distinct request IDs from `eRequest`, checks each `eCommit` against that expected work, rejects a second commit for an ID, and checks the final reported total against the number of unique requests. Its hot `AwaitingOutcome` state also checks that the modeled driver eventually receives a result in this finite scenario.

## Boundaries and assumptions

The model includes one service, one client, one request ID, one incrementing effect, one lost-response point, one restart, and one retry. For the passing case, the checker explores all schedules for these bounded machines. The run assumes enabled messages are eventually handled so the finite history can reach quiescence; the hot-state check is evidence about that modeled history, not a fairness guarantee for an operating system or a production service.

It omits concurrent clients, multiple IDs, conflicting payloads under one ID, memo expiration, storage or restart failures, network duplication and reordering, and all crash points except the one after commit and before the response. It also omits implementation-specific transaction isolation, persistence configuration, and the storage engine itself. The result demonstrates that the represented split design duplicates the effect and that the represented atomic design handles this one retry history. It does not prove a generic crash-proof implementation or unbounded correctness.

The P model's key implementation mapping is the storage boundary: the fixed variant's adjacent local updates are valid only when the actual storage primitive co-commits effect and durable outcome. If an implementation writes them separately, its model should expose that gap, as `SplitCommitService` does.
