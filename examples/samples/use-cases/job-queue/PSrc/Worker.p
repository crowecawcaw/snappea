// A worker: receive a job, apply its effect to the sink, ack with the receipt. The
// sink records each job's effect once, keyed by job id, so a redelivered job that was
// already applied is harmless. A worker can crash at any point (eCrash from the test's
// failure injector) and never comes back.

event eApply: (worker: machine, job: int, receipt: int);
event eApplied;
event eCrash;

machine Sink {
  var recorded: set[int];

  start state Serving {
    on eApply do (a: (worker: machine, job: int, receipt: int)) {
      // One transaction: insert the effect unless a row for this job id exists.
      announce eEffectApplied, (job = a.job, receipt = a.receipt);
      if (!(a.job in recorded)) {
        recorded += (a.job);
        announce eEffectRecorded, (job = a.job,);
      }
      send a.worker, eApplied;
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
    entry { send queue, eReceive, (worker = this,); }
    on eJob goto Applying with (j: (job: int, receipt: int)) {
      job = j.job;
      receipt = j.receipt;
    }
    on eNoMoreJobs goto Stopped;
    on eCrash goto Crashed;
  }

  state Applying {
    entry { send sink, eApply, (worker = this, job = job, receipt = receipt); }
    on eApplied goto Acking;
    on eCrash goto Crashed;
  }

  state Acking {
    entry {
      announce eAckSent, (job = job, receipt = receipt);
      send queue, eAck, (worker = this, job = job, receipt = receipt);
    }
    // A rejected ack means the job was redelivered: whoever holds it now will ack it.
    on eAckResp goto Receiving;
    on eCrash goto Crashed;
  }

  state Stopped {
    entry { announce eWorkerStopped, (worker = this,); }
    ignore eCrash;
  }

  // Crashed for good: replies to the dead process are dropped. The announcement
  // marks the moment of the crash for the specs (a supervisor would see it).
  state Crashed {
    entry { announce eWorkerCrashed, (worker = this,); }
    ignore eJob, eNoMoreJobs, eApplied, eAckResp, eCrash;
  }
}
