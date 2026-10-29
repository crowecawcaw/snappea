// Drivers wait for actual replies before declaring completion. RetryContract
// observes their real send payloads, before reordered client receipt.
// The original crash case drops the response after request 7 commits, then
// restarts and retries ID 7.
machine RetryDriver {
  var service: DurableService;

  start state Init {
    entry {
      service = new DurableService();
      announce eRequest, 7;
      send service, eInvoke, (client = this, id = 7, dropReply = true);
    }
    on eCommitted do (id: int) {
      send service, eCrash;
      send service, eRestart;
      announce eRequest, 7;
      send service, eInvoke, (client = this, id = 7, dropReply = false);
      goto WaitingForReply;
    }
  }

  state WaitingForReply {
    on eReply do (reply: (id: int, result: int, currentTotal: int)) {
      assert reply.id == 7, "reply ID does not match the pending request";
      announce eScenarioDone;
      goto Done;
    }
  }

  state Done {}
}

machine SplitRetryDriver {
  var service: SplitCommitService;

  start state Init {
    entry {
      service = new SplitCommitService();
      announce eRequest, 7;
      send service, eInvoke, (client = this, id = 7, dropReply = true);
    }
    on eCommitted do (id: int) {
      send service, eCrash;
      send service, eRestart;
      announce eRequest, 7;
      send service, eInvoke, (client = this, id = 7, dropReply = false);
      goto WaitingForRetryCommit;
    }
  }

  state WaitingForRetryCommit {
    on eCommitted do (id: int) {
      send service, eRemember, (client = this, id = id);
      goto WaitingForReply;
    }
  }

  state WaitingForReply {
    on eReply do (reply: (id: int, result: int, currentTotal: int)) {
      assert reply.id == 7, "reply ID does not match the pending request";
      announce eScenarioDone;
      goto Done;
    }
  }

  state Done {}
}

// This history completes ID 7, commits ID 8 without its reply, retries ID 8
// after restart, and then retries old ID 7. Its original result must stay 1
// while the current total stays 2.
machine CompletedReplayDriver {
  var service: DurableService;

  start state Init {
    entry {
      service = new DurableService();
      announce eRequest, 7;
      send service, eInvoke, (client = this, id = 7, dropReply = false);
    }
    on eReply do (reply: (id: int, result: int, currentTotal: int)) {
      assert reply.id == 7, "reply ID does not match the pending request";
      announce eRequest, 8;
      send service, eInvoke, (client = this, id = 8, dropReply = true);
      goto WaitingForSecondCommit;
    }
  }

  state WaitingForSecondCommit {
    on eCommitted do (id: int) {
      send service, eCrash;
      send service, eRestart;
      announce eRequest, 8;
      send service, eInvoke, (client = this, id = 8, dropReply = false);
      goto WaitingForSecondReply;
    }
  }

  state WaitingForSecondReply {
    on eReply do (reply: (id: int, result: int, currentTotal: int)) {
      assert reply.id == 8, "reply ID does not match the pending request";
      send service, eCrash;
      send service, eRestart;
      announce eRequest, 7;
      send service, eInvoke, (client = this, id = 7, dropReply = false);
      goto WaitingForOldReplay;
    }
  }

  state WaitingForOldReplay {
    on eReply do (reply: (id: int, result: int, currentTotal: int)) {
      assert reply.id == 7, "reply ID does not match the pending request";
      announce eScenarioDone;
      goto Done;
    }
  }

  state Done {}
}

machine ReappliedReplayDriver {
  var service: ReapplyReplayService;

  start state Init {
    entry {
      service = new ReapplyReplayService();
      announce eRequest, 7;
      send service, eInvoke, (client = this, id = 7, dropReply = false);
    }
    on eReply do (reply: (id: int, result: int, currentTotal: int)) {
      assert reply.id == 7, "reply ID does not match the pending request";
      announce eRequest, 8;
      send service, eInvoke, (client = this, id = 8, dropReply = true);
      goto WaitingForSecondCommit;
    }
  }

  state WaitingForSecondCommit {
    on eCommitted do (id: int) {
      send service, eCrash;
      send service, eRestart;
      announce eRequest, 8;
      send service, eInvoke, (client = this, id = 8, dropReply = false);
      goto WaitingForSecondReply;
    }
  }

  state WaitingForSecondReply {
    on eReply do (reply: (id: int, result: int, currentTotal: int)) {
      assert reply.id == 8, "reply ID does not match the pending request";
      send service, eCrash;
      send service, eRestart;
      announce eRequest, 7;
      send service, eInvoke, (client = this, id = 7, dropReply = false);
      goto WaitingForOldReplay;
    }
  }

  state WaitingForOldReplay {
    on eReply do (reply: (id: int, result: int, currentTotal: int)) {
      assert reply.id == 7, "reply ID does not match the pending request";
      announce eScenarioDone;
      goto Done;
    }
  }

  state Done {}
}

// Each caller has a stable logical ID; its retry never becomes a new request.
// Independent callers permit both requests to be pending before either commits.
event eCallerFinished;
machine ConcurrentCaller {
  start state Run {
    entry (p: (service: DurableService, coordinator: machine, id: int)) {
      announce eRequest, p.id;
      send p.service, eInvoke, (client = this, id = p.id, dropReply = false);
      receive { case eReply: (first: (id: int, result: int, currentTotal: int)) {
        assert first.id == p.id, "reply ID does not match the pending request";
      } }
      announce eRequest, p.id;
      send p.service, eInvoke, (client = this, id = p.id, dropReply = false);
      receive { case eReply: (retry: (id: int, result: int, currentTotal: int)) {
        assert retry.id == p.id, "retry ID does not match the pending request";
      } }
      send p.coordinator, eCallerFinished;
    }
  }
}
machine ConcurrentRetryDriver {
  var completed: int;
  start state Run {
    entry {
      var service: DurableService;
      service = new DurableService();
      new ConcurrentCaller((service = service, coordinator = this, id = 7));
      new ConcurrentCaller((service = service, coordinator = this, id = 8));
    }
    on eCallerFinished do {
      completed = completed + 1;
      if (completed == 2) { announce eScenarioDone; goto Done; }
    }
  }
  state Done { }
}
