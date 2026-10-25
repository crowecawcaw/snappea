# Durable retry after a lost response

This small P project checks what a retry means after commit. It keeps the saved result for each completed request separate from the service's current effect state. That difference matters when another operation changes the current state before an older request is replayed.

## Run it

From this directory, with `snappea` installed and on `PATH`, run:

```sh
snappea check tcCompletedReplay
snappea check tcDurableRetry tcCompletedReplay
snappea check tcReappliedReplay
snappea check tcSplitCommitRetry
```

`tcCompletedReplay` passes lint, complete bounded schedule exploration, required coverage, and default mutation checks on its own. The current checker explores 164 schedules, covers all 47 statements, all 8 branches, and all 11 handlers, and detects all 13 mutants. The second command checks the original crash-and-retry case together with the replay case; the pair passes full required coverage and mutation checks, exploring 86 and 165 schedules respectively.

The last two commands are deliberately expected to exit with status 1. `tcReappliedReplay` should report `one request caused more than one effect`: its service applies ID `7` again after ID `8` changed the current total. `tcSplitCommitRetry` should report the same assertion because its durable effect and volatile outcome memo are split across transitions. These are deliberate property failures. A combined `snappea check` with no test name includes both broken variants and should fail for these reasons.

If building from source, run `cargo build --release -p snappea` from the repository root and add its `target/release` directory to `PATH`.

## What the model preserves

The drivers are separate from the service models. In the original `RetryDriver` history, ID `7` commits, its client reply is withheld, the process restarts, and the client retries ID `7`. The internal `eCommitted` message only tells the test driver where to inject the restart; it is not a client response. This represents an ambiguous commit: the durable operation succeeded, while the client cannot know its outcome.

`CompletedReplayDriver` makes the related state distinction explicit. It completes ID `7` with result `1`, commits distinct ID `8` but loses that reply, restarts and retries ID `8`, then restarts and retries old ID `7`. The fixed service returns ID `7`'s original result `1` while the current total remains `2`. The saved response describes the earlier operation; it is not a copy of current service state.

`DurableService` keeps `durableTotal` and `durableOutcomes` across restart. A new request increments the effect and stores that request's result in adjacent local assignments before the next communication scheduling point. That non-yielding update region stands for one actual atomic storage primitive, such as a transaction or a compare-and-swap on one durable record containing both values. P handlers can yield at communication operations such as `send`, `receive`, or `create`; the whole handler is not modeled as atomic. The implementation must provide the same co-commit boundary for this abstraction to apply. A retry reads its saved result and reports the separately maintained current total without applying the effect again.

`RetryContract` does not inspect service variables. It derives expected operation count from distinct `eRequest` IDs, records each ID's first committed result, rejects a second `eCommit` for that ID, and checks each reported response against the saved result for that ID. Separately, it checks the reported current total against the number of distinct requested operations. Thus a correct replay can return an old result while current state includes later work. Its hot state also requires each finite scenario to report outcomes for all distinct requested IDs before it announces completion.

`ReapplyReplayService` deliberately returns the old saved response while incrementing and committing the effect again on replay. The monitor catches the repeated commit. `SplitCommitService` demonstrates the earlier failure: it commits the effect, but keeps the replay memo in volatile state and writes it in a later transition. The process crashes before that transition; retry applies the effect again.

## Boundaries and assumptions

The replay case has one service, one client, two request IDs, incrementing effects, one lost response, two restarts, and retries of IDs `8` and `7`. The checker explores all schedules for these bounded machines. Enabled messages are assumed eventually to be handled so the finite history can reach quiescence; the hot-state check is evidence about these modeled histories, not a fairness guarantee for an operating system or production service.

The model omits concurrent clients, more IDs, conflicting payloads under one ID, memo expiration, storage or restart failures, arbitrary network duplication or reordering, and crash points beyond those shown. It also omits implementation-specific transaction isolation, persistence configuration, and the storage engine. Passing checks show that the represented atomic design preserves each saved result and the represented current total in these bounded histories. They do not prove a generic crash-proof implementation or unbounded correctness.

The key implementation mapping is the storage boundary: the fixed variant's adjacent local updates are valid only when one actual primitive co-commits the effect and durable outcome. If an implementation writes them separately, expose that gap in the model, as `SplitCommitService` does.
