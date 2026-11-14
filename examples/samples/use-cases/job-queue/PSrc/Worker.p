// A worker: receive a job, apply its effect to the sink, ack with the receipt. The
// sink records each job's effect once, keyed by job id, so a redelivered job that was
// already applied is harmless. A worker can crash at any point (Crash from the test's
// failure injector) and never comes back.

event Apply: (worker: machine, job: int, receipt: int);
event Applied;
event Crash;

machine Sink {
  var recorded: set[int];

  start state Serving {
    on Apply do (a: (worker: machine, job: int, receipt: int)) {
      // One transaction: insert the effect unless a row for this job id exists.
      announce EffectApplied, (job = a.job, receipt = a.receipt);
      if (!(a.job in recorded)) {
        recorded += (a.job);
        announce EffectRecorded, (job = a.job,);
      }
      send a.worker, Applied;
    }
  }
}

machine Worker {
  var queue: machine;
  var sink: Sink;
  var job: int;
  var receipt: int;

  start state Polling {
    entry (input: (queue: machine, sink: Sink)) {
      queue = input.queue;
      sink = input.sink;
      goto Receiving;
    }
  }

  state Receiving {
    entry { send queue, Receive, (worker = this,); }
    on Job goto Applying with (j: (job: int, receipt: int)) {
      job = j.job;
      receipt = j.receipt;
    }
    on NoMoreJobs goto Stopped;
    on Crash goto Crashed;
  }

  state Applying {
    entry { send sink, Apply, (worker = this, job = job, receipt = receipt); }
    on Applied goto Acking;
    on Crash goto Crashed;
  }

  state Acking {
    entry {
      announce AckSent, (job = job, receipt = receipt);
      send queue, Ack, (worker = this, job = job, receipt = receipt);
    }
    // A rejected ack means the job was redelivered: whoever holds it now will ack it.
    on AckResp goto Receiving;
    on Crash goto Crashed;
  }

  state Stopped {
    entry { announce WorkerStopped, (worker = this,); }
    ignore Crash;
  }

  // Crashed for good: replies to the dead process are dropped. The announcement
  // marks the moment of the crash for the specs (a supervisor would see it).
  state Crashed {
    entry { announce WorkerCrashed, (worker = this,); }
    ignore Job, NoMoreJobs, Applied, AckResp, Crash;
  }
}
