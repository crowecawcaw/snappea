// The environment: the visibility-timer service and the failure injector, as one
// driver machine with budgets. Workers, queue and sink are the design.

// Visibility timers. A timer fires only when the environment allows it: always for a
// crashed worker's delivery (it never acks), and for at most `slow` other deliveries
// (a worker slower than the visibility timeout). Every other delivery is acked well
// within its timeout, so its timer never fires. At most `crashes` workers crash, each
// at a moment the scheduler picks after one of its deliveries.
machine Timers {
  var slow: int;
  var crashes: int;
  var crashed: set[machine];

  start state Running {
    entry (budget: (slow: int, crashes: int)) {
      slow = budget.slow;
      crashes = budget.crashes;
    }
    on eStartTimer do (t: (queue: machine, job: int, receipt: int, worker: machine)) {
      if (t.worker in crashed) {
        send t.queue, eVisibilityTimeout, (job = t.job, receipt = t.receipt);
      } else if (crashes > 0 && $) {
        crashes = crashes - 1;
        crashed += (t.worker);
        send t.worker, eCrash;
        send t.queue, eVisibilityTimeout, (job = t.job, receipt = t.receipt);
      } else if (slow > 0 && $) {
        slow = slow - 1;
        send t.queue, eVisibilityTimeout, (job = t.job, receipt = t.receipt);
        // Timer callbacks are at-least-once: a duplicate can arrive after the job was
        // redelivered under a new receipt.
        if ($) { send t.queue, eVisibilityTimeout, (job = t.job, receipt = t.receipt); }
      }
    }
  }
}

type tSetup = (jobs: int, workers: int, slow: int, crashes: int);

// Enqueues jobs 1..n and starts the workers against `queue`'s kind of queue.
fun JobSet(n: int): set[int] {
  var jobs: set[int];
  var j: int;
  j = 1;
  while (j <= n) {
    announce eEnqueued, (job = j,);
    jobs += (j);
    j = j + 1;
  }
  return jobs;
}

fun StartWorkers(queue: machine, n: int) {
  var sink: Sink;
  var i: int;
  sink = new Sink();
  while (i < n) {
    new Worker((queue = queue, sink = sink));
    i = i + 1;
  }
}

fun Run(s: tSetup) {
  var timers: machine;
  var queue: machine;
  timers = new Timers((slow = s.slow, crashes = s.crashes));
  queue = new Queue((timers = timers, jobs = JobSet(s.jobs)));
  StartWorkers(queue, s.workers);
}

// The stale-ack race: one job, two workers, and one delivery may outlive its
// visibility timeout. The job is redelivered to the other worker while the first is
// still working; the first worker's ack then carries a stale receipt and must be
// rejected, and the job must still be completed exactly once.
machine SlowWorker {
  start state Init {
    entry { Run((jobs = 1, workers = 2, slow = 1, crashes = 0)); }
  }
}

// A worker crashes after receiving the job: before applying it, after applying it
// but before acking, or after acking. The job must be redelivered and completed by
// the other worker, and its effect recorded once.
machine WorkerCrash {
  start state Init {
    entry { Run((jobs = 1, workers = 2, slow = 0, crashes = 1)); }
  }
}

// Two workers, two jobs, one slow delivery: redelivery of one job while the other is
// in flight, and long-polling workers that must be woken by a visibility timeout or
// released when the last job completes.
machine TwoJobsTwoWorkers {
  start state Init {
    entry { Run((jobs = 2, workers = 2, slow = 1, crashes = 0)); }
  }
}

// Generated: two jobs, two workers, one slow delivery and one crash allowed. Sampled.
machine Generated {
  start state Init {
    entry { Run((jobs = 2, workers = 2, slow = 1, crashes = 1)); }
  }
}

test tcSlowWorker [main=SlowWorker]:
  assert ReceiptsFence, JobsComplete in { SlowWorker, Timers, Queue, Sink, Worker };

test tcWorkerCrash [main=WorkerCrash]:
  assert ReceiptsFence, JobsComplete in { WorkerCrash, Timers, Queue, Sink, Worker };

test tcTwoJobsTwoWorkers [main=TwoJobsTwoWorkers]:
  assert ReceiptsFence, JobsComplete in { TwoJobsTwoWorkers, Timers, Queue, Sink, Worker };

test tcGenerated [main=Generated]:
  assert ReceiptsFence, JobsComplete in { Generated, Timers, Queue, Sink, Worker };
