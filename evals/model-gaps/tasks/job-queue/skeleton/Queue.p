// A work queue with visibility timeouts (SQS-like). See DESIGN.md.
//
// The queue announces the events PSpec/Contract.p observes (Delivered,
// VisibilityExpired, Acked, AckRejected) at its commit points.

event Receive: (worker: machine);
event Job: (job: int, receipt: int);
// Every job is completed: the worker can stop polling.
event NoMoreJobs;
event Ack: (worker: machine, job: int, receipt: int);
// The worker carries on polling either way, so the reply carries nothing.
event AckResp;
// To the timer service, and back when the visibility timeout expires. The queue starts
// one timer per delivery.
event StartTimer: (queue: machine, job: int, receipt: int, worker: machine);
event VisibilityTimeout: (job: int, receipt: int);

machine Queue {
  start state Serving {
    entry (input: (timers: machine, jobs: set[int])) {
      // TODO
    }
  }
}
