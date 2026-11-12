// The environment: the visibility-timer service and the failure injector, as one
// driver machine with budgets. Workers, queue and sink are the design.

// The environment's crash request: crash this worker at a moment the scheduler picks.
event eCrashWorker: (worker: machine);

// Visibility timers. A timer fires only when the environment allows it: for every
// delivery to a crashed worker (it never acks), and for at most `slow` other
// deliveries (a worker slower than the visibility timeout), whose timer may also fire
// twice. Every other delivery is acked well within its timeout, so its timer never
// fires. This machine is also the failure injector, so it knows which worker crashed.
machine Timers {
  var slow: int;
  var crashed: set[machine];
  // receipt -> the delivery its timer belongs to
  var deliveries: map[int, (queue: machine, job: int, worker: machine)];

  start state Running {
    entry (budget: int) { slow = budget; }
    on eStartTimer do (t: (queue: machine, job: int, receipt: int, worker: machine)) {
      deliveries[t.receipt] = (queue = t.queue, job = t.job, worker = t.worker);
      if (t.worker in crashed) {
        Fire(t.receipt);
      } else if (slow > 0 && $) {
        slow = slow - 1;
        Fire(t.receipt);
        // Timer callbacks are at-least-once: a duplicate can arrive after the job was
        // redelivered under a new receipt.
        if ($) { Fire(t.receipt); }
      }
    }
    on eCrashWorker do (c: (worker: machine)) {
      var r: int;
      announce eWorkerCrashed, c;
      crashed += (c.worker);
      send c.worker, eCrash;
      // The crashed worker's deliveries will all time out.
      foreach (r in keys(deliveries)) {
        if (deliveries[r].worker == c.worker) { Fire(r); }
      }
    }
  }

  fun Fire(receipt: int) {
    send deliveries[receipt].queue, eVisibilityTimeout, (job = deliveries[receipt].job, receipt = receipt);
  }
}

// crash: whether the failure injector crashes the first worker.
type tSetup = (jobs: int, workers: int, slow: int, crash: bool);

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

fun StartWorkers(queue: machine, timers: machine, s: tSetup) {
  var sink: Sink;
  var w: machine;
  var i: int;
  sink = new Sink();
  while (i < s.workers) {
    w = new Worker((queue = queue, sink = sink));
    announce eWorkerStarted, (worker = w,);
    if (i == 0 && s.crash) { send timers, eCrashWorker, (worker = w,); }
    i = i + 1;
  }
}

fun Run(s: tSetup) {
  var timers: machine;
  var queue: machine;
  announce eWorkersPlanned, s.workers;
  timers = new Timers(s.slow);
  queue = new Queue((timers = timers, jobs = JobSet(s.jobs)));
  StartWorkers(queue, timers, s);
}

// The stale-ack race: one job, two workers, and one delivery may outlive its
// visibility timeout. The job is redelivered to the other worker while the first is
// still working; the first worker's ack then carries a stale receipt and must be
// rejected, and the job must still be completed exactly once.
machine SlowWorker {
  start state Init {
    entry { Run((jobs = 1, workers = 2, slow = 1, crash = false)); }
  }
}

// A worker crashes at any point: before receiving, before applying, after applying
// but before acking, or after acking. The job must be redelivered and completed by
// the other worker, and its effect recorded once.
machine WorkerCrash {
  start state Init {
    entry { Run((jobs = 1, workers = 2, slow = 0, crash = true)); }
  }
}

// Two workers, two jobs, one slow delivery: redelivery of one job while the other is
// in flight, and long-polling workers that must be woken by a visibility timeout or
// released when the last job completes.
machine TwoJobsTwoWorkers {
  start state Init {
    entry { Run((jobs = 2, workers = 2, slow = 1, crash = false)); }
  }
}

// Generated: two jobs, two workers, one slow delivery, and maybe a crash. Sampled.
machine Generated {
  start state Init {
    entry { Run((jobs = 2, workers = 2, slow = 1, crash = $)); }
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
