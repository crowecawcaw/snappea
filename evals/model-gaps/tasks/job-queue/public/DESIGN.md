# Work queue with visibility timeouts

An SQS-like queue. A worker receives a job with a receipt handle; the job is invisible
until its visibility timeout expires, then it can be delivered again under a new
receipt. The worker applies the job's effect to a sink and acks with its receipt. The
sink records each effect keyed by job id, so at-least-once delivery gives exactly-once
effects. A receive with no visible job waits (long poll) until a job becomes visible or
every job is complete.

## Contract

- Only enqueued jobs are delivered, and a completed job never again. Each delivery has
  a fresh receipt, and a job has at most one current delivery.
- A job is completed only by an ack with its current receipt, and only after its effect
  was recorded. An ack is rejected only when its receipt is not current.
- Each effect is applied at least once and recorded exactly once.
- Every enqueued job is completed, every ack is answered, and every worker that does
  not crash stops once all jobs are complete, and not before.

## Atomic steps

Queue receive (mint a receipt, hide the job, start its timer), visibility timeout
(show the job again), ack (delete the job), sink apply (insert the effect unless the
job id exists). A worker's receive, apply and ack are separate steps.

## What can happen

The queue serves any number of workers and jobs. A worker can crash at any point and
never comes back; at least one worker keeps running. A worker can be slower than the
visibility timeout, so its job is redelivered while it still works. The timer service
delivers each timeout at least once: a timeout can arrive twice, and late. The queue
and the sink do not crash, and messages are not lost.

Not in scope: dead-letter queues, visibility extension, batching, sink failures.
