# Durable retry after a lost response

- **Contract:** apply each request ID once; return its original result on retry without reverting newer state.
- **Correct:** `DurableService` co-commits `durableTotal` and `durableOutcomes` in non-yielding assignments, representing one actual transaction/CAS. Retains both across restart.
- **Broken:** `SplitCommitService` persists the effect but keeps the outcome memo volatile and updates it later; restart permits duplicate application. `ReapplyReplayService` returns the old response but applies the effect again.
- **Oracle:** `RetryContract` tracks distinct requested IDs and first committed results independently; rejects repeated commits and checks responses/current total separately. Hot-state checking requires outcomes before scenario completion.

## Histories

- `RetryDriver`: ID 7 commits → reply withheld → restart → retry 7. Internal `eCommitted` identifies the crash boundary; it is not a client reply.
- `CompletedReplayDriver`: ID 7 returns 1 → ID 8 commits, reply lost → restart/retry 8 → restart/retry 7. Retry 7 returns 1 while current total remains 2.

## Run

With `snappea` on `PATH`, from this directory:

```sh
snappea check tcCompletedReplay
snappea check tcDurableRetry tcCompletedReplay
snappea check tcReappliedReplay
snappea check tcSplitCommitRetry
```

- First two commands pass required coverage and mutation checks. Validated checker: replay alone 164 schedules, 47 statements, 8 branches, 11 handlers, 13/13 mutants; combined cases 86/165 schedules.
- Last two exit 1 with `one request caused more than one effect`.
- Unfiltered `snappea check` includes broken variants and should fail.
- Source build: `cargo build --release -p snappea`; add repository `target/release` to `PATH`.

## Boundaries

- One service/client, two IDs, incrementing effects, one lost response, two restarts in the replay case.
- Adjacent updates model co-commit only if the implementation supplies one atomic durable primitive; communication can yield within handlers.
- Omits concurrent clients, conflicting payloads per ID, expiration, arbitrary duplication/reordering, additional crash points, storage failures, and engine/isolation details.
- Finite hot-state checks do not establish production fairness or unbounded recovery correctness.
