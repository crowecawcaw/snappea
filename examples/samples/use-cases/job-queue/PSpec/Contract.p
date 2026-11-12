// The queue contract, stated over events the queue and the sink announce at their
// commit points, plus the jobs the driver enqueues. The specs rebuild each job's
// state from the history; they never read the queue's maps.

// Driver: these jobs are enqueued before any worker starts.
event eEnqueued: (job: int);
event eDelivered: (job: int, receipt: int);
event eVisibilityExpired: (job: int, receipt: int);
event eAcked: (job: int, receipt: int);
event eAckRejected: (job: int, receipt: int);
// The sink ran a job's effect (at least once per job) ...
event eEffectApplied: (job: int, receipt: int);
// ... and recorded it (exactly once per job, keyed by job id).
event eEffectRecorded: (job: int);

// Safety:
//  - only enqueued jobs are delivered; a completed job is never delivered again;
//  - each delivery gets a fresh receipt, and a job has at most one current delivery;
//  - a job is completed only by an ack carrying its current receipt, and only after
//    its effect was recorded; an ack is rejected only when its receipt is not current;
//  - each job's effect is recorded exactly once, and only by a delivery of that job.
spec ReceiptsFence observes eEnqueued, eDelivered, eVisibilityExpired, eAcked, eAckRejected,
    eEffectApplied, eEffectRecorded {
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
    on eEnqueued do (e: (job: int)) {
      assert !(e.job in enqueued), format("job {0} enqueued twice", e.job);
      enqueued += (e.job);
    }
    on eDelivered do (d: (job: int, receipt: int)) {
      assert d.job in enqueued, format("job {0} delivered but never enqueued", d.job);
      assert !(d.job in completed), format("completed job {0} was delivered again", d.job);
      assert !(d.job in current),
        format("job {0} delivered with receipt {1} while receipt {2} is still current", d.job, d.receipt, current[d.job]);
      assert !(d.receipt in receipts), format("receipt {0} issued twice", d.receipt);
      receipts += (d.receipt);
      receiptJob[d.receipt] = d.job;
      current[d.job] = d.receipt;
    }
    on eVisibilityExpired do (v: (job: int, receipt: int)) {
      assert v.job in current && current[v.job] == v.receipt,
        format("job {0} became visible again on receipt {1}, which is not its current delivery", v.job, v.receipt);
      current -= (v.job);
    }
    on eAcked do (a: (job: int, receipt: int)) {
      assert a.job in current && current[a.job] == a.receipt,
        format("job {0} was completed by an ack with stale receipt {1}", a.job, a.receipt);
      assert a.job in recorded, format("job {0} was completed before its effect was recorded", a.job);
      current -= (a.job);
      completed += (a.job);
    }
    on eAckRejected do (a: (job: int, receipt: int)) {
      assert !(a.job in current && current[a.job] == a.receipt),
        format("ack of job {0} with its current receipt {1} was rejected", a.job, a.receipt);
      assert a.receipt in receiptJob && receiptJob[a.receipt] == a.job,
        format("ack of job {0} with receipt {1}, which was never issued for it", a.job, a.receipt);
    }
    on eEffectApplied do (e: (job: int, receipt: int)) {
      assert e.receipt in receiptJob && receiptJob[e.receipt] == e.job,
        format("effect of job {0} applied under receipt {1}, which was not issued for it", e.job, e.receipt);
      applied += (e.job);
    }
    on eEffectRecorded do (e: (job: int)) {
      assert e.job in applied, format("effect of job {0} recorded but never applied", e.job);
      assert !(e.job in recorded), format("effect of job {0} recorded twice", e.job);
      recorded += (e.job);
    }
  }
}

// Progress: every enqueued job is eventually completed (acked), given at most one
// worker crash and a bounded number of early visibility timeouts (the driver's
// budget), with at least one worker that keeps running.
spec JobsComplete observes eEnqueued, eAcked {
  var pending: set[int];

  start state Idle {
    on eEnqueued do (e: (job: int)) { Enqueue(e.job); }
    on eAcked do { assert false, "job acked with nothing enqueued"; }
  }

  hot state Busy {
    on eEnqueued do (e: (job: int)) { Enqueue(e.job); }
    // snappea-lint: allow unused-payload-field -- this spec only tracks jobs; ReceiptsFence checks receipts
    on eAcked do (a: (job: int, receipt: int)) {
      assert a.job in pending, format("job {0} completed twice or never enqueued", a.job);
      pending -= (a.job);
      if (sizeof(pending) == 0) { goto Idle; }
    }
  }

  fun Enqueue(job: int) {
    pending += (job);
    goto Busy;
  }
}
