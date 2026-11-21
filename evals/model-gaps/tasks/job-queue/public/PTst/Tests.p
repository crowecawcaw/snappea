// The environment: the visibility-timer service, and the test cases. Workers, queue
// and sink are the design.

// Visibility timers. Workers ack well within the visibility timeout, so no timer
// fires.
machine Timers {
  start state Running {
    ignore StartTimer;
  }
}

// Enqueues jobs 1..n.
fun JobSet(n: int): set[int] {
  var jobs: set[int];
  var j: int;
  j = 1;
  while (j <= n) {
    announce Enqueued, (job = j,);
    jobs += (j);
    j = j + 1;
  }
  return jobs;
}

fun Run(jobs: int, workers: int) {
  var timers: machine;
  var queue: machine;
  var sink: Sink;
  var w: machine;
  var i: int;
  announce WorkersPlanned, workers;
  timers = new Timers();
  queue = new Queue((timers = timers, jobs = JobSet(jobs)));
  sink = new Sink();
  while (i < workers) {
    w = new Worker((queue = queue, sink = sink));
    announce WorkerStarted, (worker = w,);
    i = i + 1;
  }
}

// One worker drains the queue.
machine OneWorker {
  start state Init {
    entry { Run(2, 1); }
  }
}

// Two workers share two jobs.
machine TwoWorkers {
  start state Init {
    entry { Run(2, 2); }
  }
}

test tcOneWorker [main=OneWorker]:
  assert ReceiptsFence, JobsComplete in { OneWorker, Timers, Queue, Sink, Worker };

test tcTwoWorkers [main=TwoWorkers]:
  assert ReceiptsFence, JobsComplete in { TwoWorkers, Timers, Queue, Sink, Worker };
