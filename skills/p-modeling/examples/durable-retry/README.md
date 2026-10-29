# Durable retry after a lost response

- **Contract:** apply each request ID once; return its original result on retry without reverting newer state.
- **Correct:** `DurableService` co-commits `durableTotal` and `durableOutcomes` in non-yielding assignments, representing one actual transaction/CAS. Retains both across restart.
- **Broken:** `SplitCommitService` persists the effect but keeps the outcome memo volatile and updates it later; restart permits duplicate application. `ReapplyReplayService` returns the old response but applies the effect again.
- **Oracle:** requests create completion obligations; unique commits advance an independent expected total. A request is not assumed committed merely because it was invoked. Repeated commits fail; retries must preserve the original result while reporting the current total.
- **Ordering:** the selected monitor observes actual `eReply` sends at reply creation, before client delivery; it does not trust a separate service-supplied response audit. Across clients, reply receipt order is not a legal replacement for that observation order. The driver waits for actual replies before announcing scenario completion.
- **Progress:** the monitor starts hot; every requested ID must commit and return before `eScenarioDone` reaches cold `Complete`. Useful success is required, not just absence of duplicate effects.

## Histories

- `RetryDriver`: ID 7 commits → reply withheld → restart → retry 7. Internal `eCommitted` identifies the crash boundary; it is not a client reply.
- `CompletedReplayDriver`: ID 7 returns 1 → ID 8 commits, reply lost → restart/retry 8 → restart/retry 7. Retry 7 returns 1 while current total remains 2.
- `ConcurrentRetryDriver`: two independent clients invoke stable IDs 7 and 8 and each retries its own ID. The checker varies their interleavings; coordinator completion requires both clients to receive both replies.

## Run

With `snappea` on `PATH`, from this directory:

```sh
snappea check tcCompletedReplay
snappea check tcDurableRetry tcCompletedReplay tcConcurrentRetries
snappea check tcReappliedReplay
snappea check tcSplitCommitRetry
```

- First two commands pass required coverage; inspect default mutation feedback separately. Fixed crash scenarios and independent concurrent callers cover different obligations, so retain both.
- Last two exit 1 with `one request caused more than one effect`.
- Unfiltered `snappea check` includes broken variants and should fail.
- Source build: `cargo build --release -p snappea`; add repository `target/release` to `PATH`.

## Boundaries

- One service, at most two clients/IDs, incrementing effects. Crash histories have one lost response and at most two restarts; the concurrent history has no crashes.
- Adjacent updates model co-commit only if the implementation supplies one atomic durable primitive; communication can yield within handlers.
- Omits concurrent crash/recovery, conflicting payloads per ID, expiration, arbitrary duplication/reordering, additional crash points, storage failures, and engine/isolation details.
- Finite hot-state checks do not establish production fairness or unbounded recovery correctness.

## Validate the oracle independently

These hand-calculated traces test the monitor, not an implementation. The valid
history has two pending requests before the first commit, then retries the older
ID after the second commit. It must not confuse pending requests with effects.

```sh
snappea observe PSpec/Contract.p valid-concurrent.jsonl --spec RetryContract --strict --check-hot
snappea observe PSpec/Contract.p invalid-duplicate.jsonl --spec RetryContract --strict --check-hot
snappea observe PSpec/Contract.p invalid-incomplete.jsonl --spec RetryContract --strict --check-hot
snappea observe PSpec/Contract.p invalid-premature-done.jsonl --spec RetryContract --strict --check-hot
snappea observe PSpec/Contract.p invalid-reverted-total.jsonl --spec RetryContract --strict --check-hot
snappea observe PSpec/Contract.p invalid-zero-work.jsonl --spec RetryContract --strict --check-hot
```

The first exits 0; all others exit 1 for duplicate effect, unfinished work,
premature completion, a reply-created current-state regression, and zero work respectively.
The last trace would not be a valid oracle probe if its eReply events meant
arbitrarily delayed client receipt; preserve the documented observation boundary.

Saved response-integrity regression:

```sh
python -m unittest -v
```

It corrupts only actual reply payloads in a temporary copy, keeping commit effects
unchanged. All three correct-driver workloads must reject the wrong original
result. This prevents a separate audit event from hiding a bad public reply.
