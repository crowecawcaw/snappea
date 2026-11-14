// The queue contract, stated over events the queue and the sink announce at their
// commit points, plus the jobs the driver enqueues. The specs rebuild each job's
// state from the history; they never read the queue's maps.

// Driver: these jobs are enqueued before any worker starts.
event Enqueued: (job: int);
event Delivered: (job: int, receipt: int);
event VisibilityExpired: (job: int, receipt: int);
event Acked: (job: int, receipt: int);
event AckRejected: (job: int, receipt: int);
// The sink ran a job's effect (at least once per job) ...
event EffectApplied: (job: int, receipt: int);
// ... and recorded it (exactly once per job, keyed by job id).
event EffectRecorded: (job: int);

// Safety:
//  - only enqueued jobs are delivered; a completed job is never delivered again;
//  - each delivery gets a fresh receipt, and a job has at most one current delivery;
//  - a job is completed only by an ack carrying its current receipt, and only after
//    its effect was recorded; an ack is rejected only when its receipt is not current;
//  - each job's effect is recorded exactly once, and only by a delivery of that job.
spec ReceiptsFence observes Enqueued, Delivered, VisibilityExpired, Acked, AckRejected,
    EffectApplied, EffectRecorded {
  var enqueued: set[int];
  var receipts: set[int];
  // job -> receipt of its current delivery
  var current: map[int, int];
  // receipt -> job it was issued for
  var receiptJob: map[int, int];
  var completed: set[int];
  var recorded: set[int];
  // jobs whose effect ran at least once
  var applied: set[int];

  start state Watching {
    on Enqueued do (e: (job: int)) {
      assert !(e.job in enqueued), format("job {0} enqueued twice", e.job);
      enqueued += (e.job);
    }
    on Delivered do (d: (job: int, receipt: int)) {
      assert d.job in enqueued, format("job {0} delivered but never enqueued", d.job);
      assert !(d.job in completed), format("completed job {0} was delivered again", d.job);
      assert !(d.job in current),
        format("job {0} delivered with receipt {1} while receipt {2} is still current", d.job, d.receipt, current[d.job]);
      assert !(d.receipt in receipts), format("receipt {0} issued twice", d.receipt);
      receipts += (d.receipt);
      receiptJob[d.receipt] = d.job;
      current[d.job] = d.receipt;
    }
    on VisibilityExpired do (v: (job: int, receipt: int)) {
      assert v.job in current && current[v.job] == v.receipt,
        format("job {0} became visible again on receipt {1}, which is not its current delivery", v.job, v.receipt);
      current -= (v.job);
    }
    on Acked do (a: (job: int, receipt: int)) {
      assert a.job in current && current[a.job] == a.receipt,
        format("job {0} was completed by an ack with stale receipt {1}", a.job, a.receipt);
      assert a.job in recorded, format("job {0} was completed before its effect was recorded", a.job);
      current -= (a.job);
      completed += (a.job);
    }
    on AckRejected do (a: (job: int, receipt: int)) {
      assert !(a.job in current && current[a.job] == a.receipt),
        format("ack of job {0} with its current receipt {1} was rejected", a.job, a.receipt);
      assert a.receipt in receiptJob && receiptJob[a.receipt] == a.job,
        format("ack of job {0} with receipt {1}, which was never issued for it", a.job, a.receipt);
    }
    on EffectApplied do (e: (job: int, receipt: int)) {
      assert e.receipt in receiptJob && receiptJob[e.receipt] == e.job,
        format("effect of job {0} applied under receipt {1}, which was not issued for it", e.job, e.receipt);
      applied += (e.job);
    }
    on EffectRecorded do (e: (job: int)) {
      assert e.job in applied, format("effect of job {0} recorded but never applied", e.job);
      assert !(e.job in recorded), format("effect of job {0} recorded twice", e.job);
      recorded += (e.job);
    }
  }
}

// A worker sent an ack for this delivery.
event AckSent: (job: int, receipt: int);
// Driver: how many workers it will start, announced before anything else.
event WorkersPlanned: int;
// Driver: a worker started. Worker: it crashed (it stops at that point).
event WorkerStarted: (worker: machine);
event WorkerCrashed: (worker: machine);
// A worker saw that every job is completed and stopped polling.
event WorkerStopped: (worker: machine);

// Progress: every enqueued job is eventually completed (acked), given at most one
// worker crash and a bounded number of early visibility timeouts (the driver's
// budget), with at least one worker that keeps running. Also:
// Every ack a worker sends is answered: accepted or rejected. Every started worker
// that does not crash stops once all jobs are complete.
spec JobsComplete observes WorkersPlanned, Enqueued, Acked, AckSent, AckRejected, WorkerStarted,
    WorkerCrashed, WorkerStopped {
  var pending: set[int];
  // receipt -> job of each ack sent and not yet answered
  var acks: map[int, int];
  // workers still expected to stop
  var running: set[machine];
  // workers planned but not started yet
  var toStart: int;

  start state Idle {
    on WorkersPlanned do (n: int) {
      assert n > 0, "planned no workers";
      toStart = n;
      goto Busy;
    }
    on Enqueued do { assert false, "job enqueued before the run was planned"; }
    on Acked do { assert false, "job acked outside a run"; }
    on AckSent do { assert false, "ack sent outside a run"; }
    on AckRejected do { assert false, "ack rejected outside a run"; }
    on WorkerStarted do { assert false, "worker started outside a run"; }
    // The injector may crash a worker after it already stopped; nothing is owed.
    ignore WorkerCrashed;
    on WorkerStopped do { assert false, "worker stopped outside a run"; }
  }

  hot state Busy {
    on WorkersPlanned do { assert false, "run planned twice"; }
    on Enqueued do (e: (job: int)) { pending += (e.job); }
    on AckSent do (a: (job: int, receipt: int)) {
      assert !(a.receipt in acks), format("receipt {0} of job {1} acked twice", a.receipt, a.job);
      acks[a.receipt] = a.job;
    }
    on Acked do (a: (job: int, receipt: int)) {
      assert a.job in pending, format("job {0} completed twice or never enqueued", a.job);
      pending -= (a.job);
      Answered(a.job, a.receipt);
    }
    on AckRejected do (a: (job: int, receipt: int)) { Answered(a.job, a.receipt); }
    on WorkerStarted do (w: (worker: machine)) {
      running += (w.worker);
      toStart = toStart - 1;
    }
    on WorkerCrashed do (w: (worker: machine)) { running -= (w.worker); Settle(); }
    on WorkerStopped do (w: (worker: machine)) {
      assert sizeof(pending) == 0, format("a worker stopped while {0} jobs were not completed", sizeof(pending));
      running -= (w.worker);
      Settle();
    }
  }

  fun Answered(job: int, receipt: int) {
    assert receipt in acks, format("ack with receipt {0} answered but never sent", receipt);
    assert acks[receipt] == job, "ack response named another job";
    acks -= (receipt);
    Settle();
  }

  fun Settle() {
    if (toStart == 0 && sizeof(pending) == 0 && sizeof(acks) == 0 && sizeof(running) == 0) { goto Idle; }
  }
}
