---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do I model a work queue with visibility timeouts and receipt handles?
  - Why must an ack carry the receipt rather than the job id?
  - How do I bound timeouts and crashes so "every job completes" is checkable?
---

# Work queue with visibility timeouts

An SQS-like queue. A worker receives a job with a receipt handle; the job is invisible
until the visibility timeout expires, then it can be delivered again under a new
receipt. The worker applies the job's effect to a sink and acks with its receipt. An ack
with a stale receipt (from before a redelivery) is rejected. Workers can crash. The sink
records each effect keyed by job id, so at-least-once delivery gives exactly-once
effects.

**Contract**

- Safety (`ReceiptsFence`): only enqueued jobs are delivered, and a completed job never
  again; each delivery has a fresh receipt and a job has at most one current delivery;
  a job is completed only by an ack with its current receipt and only after its effect
  was recorded; an ack is rejected only when its receipt is not current; each effect is
  applied at least once and recorded exactly once.
- Progress (`JobsComplete`): every enqueued job is completed, every ack is answered,
  and every worker that does not crash stops once all jobs are complete. Assumes at
  most one crash and at most one delivery slower than the visibility timeout.

**Atomic primitives**: queue receive (mint receipt, hide job, start timer), visibility
timeout (re-show the job if the timer's receipt is still current), ack (delete if the
receipt is current); sink apply (insert the effect unless the job id exists). Receive,
apply and ack are separate steps, so timeouts, redeliveries and crashes interleave.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `ReceiptsFence` (safety), `JobsComplete` (progress, hot state) |
| [`PSrc/Queue.p`](PSrc/Queue.p) | The queue with receipts and long-polling receives |
| [`PSrc/Worker.p`](PSrc/Worker.p) | The worker and the idempotent sink |
| [`PSrc/AckByIdQueue.p`](PSrc/AckByIdQueue.p) | Negative control: ack by job id |
| [`PTst/Tests.p`](PTst/Tests.p) | `Timers` (visibility timers + failure injector, with budgets), drivers, test cases |

## Test cases

| Test | Drives | Why it matters |
| --- | --- | --- |
| `tcSlowWorker` | One job, two workers, one delivery outlives its timeout | The stale-ack race: redelivery while the first worker still works; its ack must be rejected |
| `tcWorkerCrash` | One job, two workers, one worker crashes at any point | Crash before apply, between apply and ack, or after ack: the job is redelivered, effect recorded once |
| `tcTwoJobsTwoWorkers` | Two jobs, one slow delivery whose timer may fire twice | A duplicate timer after redelivery must not re-show the job; waiting workers woken and released |
| `tcMoreWorkersThanJobs` | Three workers, two jobs | A waiting worker is released only when the last job completes |
| `tcGenerated` | Two jobs, slow delivery, maybe a crash | Combinations; sampled |
| `tcBugAckByJobId` | `AckByIdQueue` in the slow-worker race | **Expected to fail**: `job 1 was completed by an ack with stale receipt 1` |

The consequence in the negative control: the first worker's late ack deletes the job
while the second worker holds the current delivery. If that second attempt then failed,
nobody would retry it, and the queue reports success for work it cannot vouch for.

## Run

```sh
snappea check tcSlowWorker tcWorkerCrash tcTwoJobsTwoWorkers tcMoreWorkersThanJobs tcGenerated --deny mutation
snappea check tcBugAckByJobId      # exits 1 with the stale ack above
```

`tcSlowWorker` (8.5k schedules) and `tcWorkerCrash` (19k) explore every schedule; the
two-job and three-worker tests are sampled.

## Bounds and omissions

One or two jobs, two or three workers, at most one slow delivery and one crash, and a
crashed worker never restarts. The environment (`Timers`) decides which timers fire:
a crashed worker's always, a slow delivery's once or twice, every other delivery's never
(it was acked in time). Without that bound a timer could beat every ack forever. Not
modeled: dead-letter queues and max receive counts, visibility extension, batching,
sink failures, and a job whose effect is not idempotent.

## What building it taught

- The first version had no obligation for workers to stop, so dropping the
  "no more jobs" reply or the long-poll wake-up went unnoticed. `JobsComplete` now
  requires every live worker to stop, and every ack to be answered (`eAckSent`).
- Crashing a worker only via the timer service made "the queue never starts a timer"
  invisible: no timer, no crash. The injector now crashes a worker independently and
  fires the timers of everything it held.
- One mutant (release on every ack instead of the last) needs more workers than jobs;
  hence `tcMoreWorkersThanJobs`.

## Testing the implementation

Emit `eDelivered`, `eVisibilityExpired`, `eAcked` and `eAckRejected` from the queue (or
a wrapper around its client) and `eEffectApplied`/`eEffectRecorded` from the sink's
transaction. To force the stale ack: set a short visibility timeout, block worker A
after it applies the job until worker B has received the job, then let A ack. A's ack
must fail and B's must succeed.
