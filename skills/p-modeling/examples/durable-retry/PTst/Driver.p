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
      announce eOutcome, (id = reply.id, result = reply.result,
        currentTotal = reply.currentTotal);
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
      announce eOutcome, (id = reply.id, result = reply.result,
        currentTotal = reply.currentTotal);
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
      announce eOutcome, (id = reply.id, result = reply.result,
        currentTotal = reply.currentTotal);
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
      announce eOutcome, (id = reply.id, result = reply.result,
        currentTotal = reply.currentTotal);
      send service, eCrash;
      send service, eRestart;
      announce eRequest, 7;
      send service, eInvoke, (client = this, id = 7, dropReply = false);
      goto WaitingForOldReplay;
    }
  }

  state WaitingForOldReplay {
    on eReply do (reply: (id: int, result: int, currentTotal: int)) {
      announce eOutcome, (id = reply.id, result = reply.result,
        currentTotal = reply.currentTotal);
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
      announce eOutcome, (id = reply.id, result = reply.result,
        currentTotal = reply.currentTotal);
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
      announce eOutcome, (id = reply.id, result = reply.result,
        currentTotal = reply.currentTotal);
      send service, eCrash;
      send service, eRestart;
      announce eRequest, 7;
      send service, eInvoke, (client = this, id = 7, dropReply = false);
      goto WaitingForOldReplay;
    }
  }

  state WaitingForOldReplay {
    on eReply do (reply: (id: int, result: int, currentTotal: int)) {
      announce eOutcome, (id = reply.id, result = reply.result,
        currentTotal = reply.currentTotal);
      announce eScenarioDone;
      goto Done;
    }
  }

  state Done {}
}
