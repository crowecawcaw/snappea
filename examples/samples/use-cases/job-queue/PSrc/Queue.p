// A work queue with visibility timeouts (SQS-like).
//
// Real atomic primitives, one handler each on the queue:
//   receive:  take a visible job, mint a new receipt handle, make the job invisible
//             and start its visibility timer
//   timeout:  if the timer's receipt is still the job's current one, make it visible
//   ack:      if the receipt is the job's current one, delete the job (completed)
// A worker's receive -> apply -> ack spans three of them plus the sink, so a timeout,
// a redelivery to another worker, or a crash can land in between.

event Receive: (worker: machine);
event Job: (job: int, receipt: int);
// Every job is completed: the worker can stop polling.
event NoMoreJobs;
event Ack: (worker: machine, job: int, receipt: int);
// The worker carries on polling either way, so the reply carries nothing.
event AckResp;
// To the timer service, and back when the visibility timeout expires.
event StartTimer: (queue: machine, job: int, receipt: int, worker: machine);
event VisibilityTimeout: (job: int, receipt: int);

type InFlight = (receipt: int, worker: machine);

machine Queue {
  var timers: machine;
  var visible: set[int];
  var inFlight: map[int, InFlight];
  var remaining: int;
  var lastReceipt: int;
  // Long-polling receives waiting for a job to become visible.
  var waiting: seq[machine];

  start state Serving {
    entry (input: (timers: machine, jobs: set[int])) {
      timers = input.timers;
      visible = input.jobs;
      remaining = sizeof(input.jobs);
    }
    on Receive do (r: (worker: machine)) {
      if (sizeof(visible) > 0) {
        Deliver(r.worker);
      } else if (remaining == 0) {
        send r.worker, NoMoreJobs;
      } else {
        waiting += (sizeof(waiting), r.worker);
      }
    }
    on VisibilityTimeout do (t: (job: int, receipt: int)) {
      // A timer for a delivery that was acked, or superseded, changes nothing.
      if (t.job in inFlight && inFlight[t.job].receipt == t.receipt) {
        inFlight -= (t.job);
        visible += (t.job);
        announce VisibilityExpired, (job = t.job, receipt = t.receipt);
        if (sizeof(waiting) > 0) {
          Deliver(waiting[0]);
          waiting -= (0);
        }
      }
    }
    on Ack do (a: (worker: machine, job: int, receipt: int)) {
      // The receipt, not the job id, proves the caller holds the current delivery.
      if (a.job in inFlight && inFlight[a.job].receipt == a.receipt) {
        inFlight -= (a.job);
        remaining = remaining - 1;
        announce Acked, (job = a.job, receipt = a.receipt);
        send a.worker, AckResp;
        if (remaining == 0) { ReleaseWaiting(); }
      } else {
        announce AckRejected, (job = a.job, receipt = a.receipt);
        send a.worker, AckResp;
      }
    }
  }

  fun Deliver(worker: machine) {
    var job: int;
    var j: int;
    // Any visible job will do.
    foreach (j in visible) { job = j; }
    // snappea-lint: allow mutation:swap-operator -- receipts only need to be distinct; counting down keeps them so
    lastReceipt = lastReceipt + 1;
    visible -= (job);
    inFlight[job] = (receipt = lastReceipt, worker = worker);
    announce Delivered, (job = job, receipt = lastReceipt);
    send timers, StartTimer, (queue = this, job = job, receipt = lastReceipt, worker = worker);
    send worker, Job, (job = job, receipt = lastReceipt);
  }

  // Every job is complete, which is final: no job will be delivered again, so the
  // released workers can stay in `waiting`.
  fun ReleaseWaiting() {
    var w: machine;
    foreach (w in waiting) { send w, NoMoreJobs; }
  }
}
