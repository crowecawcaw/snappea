// A work queue with visibility timeouts (SQS-like).
//
// Real atomic primitives, one handler each on the queue:
//   receive:  take a visible job, mint a new receipt handle, make the job invisible
//             and start its visibility timer
//   timeout:  if the timer's receipt is still the job's current one, make it visible
//   ack:      if the receipt is the job's current one, delete the job (completed)
// A worker's receive -> apply -> ack spans three of them plus the sink, so a timeout,
// a redelivery to another worker, or a crash can land in between.

event eReceive: (worker: machine);
event eJob: (job: int, receipt: int);
// Every job is completed: the worker can stop polling.
event eNoMoreJobs;
event eAck: (worker: machine, job: int, receipt: int);
// The worker carries on polling either way, so the reply carries nothing.
event eAckResp;
// To the timer service, and back when the visibility timeout expires.
event eStartTimer: (queue: machine, job: int, receipt: int, worker: machine);
event eVisibilityTimeout: (job: int, receipt: int);

type tInFlight = (receipt: int, worker: machine);

machine Queue {
  var timers: machine;
  var visible: set[int];
  var inFlight: map[int, tInFlight];
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
    on eReceive do (r: (worker: machine)) {
      if (sizeof(visible) > 0) {
        Deliver(r.worker);
      } else if (remaining == 0) {
        send r.worker, eNoMoreJobs;
      } else {
        waiting += (sizeof(waiting), r.worker);
      }
    }
    on eVisibilityTimeout do (t: (job: int, receipt: int)) {
      // A timer for a delivery that was acked, or superseded, changes nothing.
      if (t.job in inFlight && inFlight[t.job].receipt == t.receipt) {
        inFlight -= (t.job);
        visible += (t.job);
        announce eVisibilityExpired, (job = t.job, receipt = t.receipt);
        if (sizeof(waiting) > 0) {
          Deliver(waiting[0]);
          waiting -= (0);
        }
      }
    }
    on eAck do (a: (worker: machine, job: int, receipt: int)) {
      // The receipt, not the job id, proves the caller holds the current delivery.
      if (a.job in inFlight && inFlight[a.job].receipt == a.receipt) {
        inFlight -= (a.job);
        remaining = remaining - 1;
        announce eAcked, (job = a.job, receipt = a.receipt);
        send a.worker, eAckResp;
        if (remaining == 0) { ReleaseWaiting(); }
      } else {
        announce eAckRejected, (job = a.job, receipt = a.receipt);
        send a.worker, eAckResp;
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
    announce eDelivered, (job = job, receipt = lastReceipt);
    send timers, eStartTimer, (queue = this, job = job, receipt = lastReceipt, worker = worker);
    send worker, eJob, (job = job, receipt = lastReceipt);
  }

  fun ReleaseWaiting() {
    var w: machine;
    foreach (w in waiting) { send w, eNoMoreJobs; }
    waiting = default(seq[machine]);
  }
}
