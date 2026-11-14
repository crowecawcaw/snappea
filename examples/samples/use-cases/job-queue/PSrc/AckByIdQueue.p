// A plausible broken design, kept as a negative control: identical to Queue except
// that ack checks the job id instead of the receipt. A slow worker whose delivery timed
// out acks after the job was redelivered: the queue deletes the job while the second
// worker is still processing it, so a failure of that second attempt would go unseen,
// and the first worker's possibly half-done work is taken as the completion.
// tcBugAckByJobId shows the spec catching it.
machine AckByIdQueue {
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
      // BUG: any ack for an in-flight job completes it, whatever its receipt.
      if (a.job in inFlight) {
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
