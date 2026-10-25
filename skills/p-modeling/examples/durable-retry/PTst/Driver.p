// Each driver expresses the same client history against a different service.
// It drops the first response, restarts the process, and retries request 7.
machine RetryDriver {
  var service: DurableService;

  start state Init {
    entry {
      service = new DurableService();
      announce eRequest, 7;
      send service, eInvoke, (client = this, id = 7);
    }
    on eCommitted do (id: int) {
      send service, eCrash;
      send service, eRestart;
      announce eRequest, 7;
      send service, eInvoke, (client = this, id = 7);
      goto WaitingForReply;
    }
  }

  state WaitingForReply {
    on eReply do (reply: (id: int, total: int)) {
      announce eOutcome, (id = reply.id, total = reply.total);
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
      send service, eInvoke, (client = this, id = 7);
    }
    on eCommitted do (id: int) {
      send service, eCrash;
      send service, eRestart;
      announce eRequest, 7;
      send service, eInvoke, (client = this, id = 7);
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
    on eReply do (reply: (id: int, total: int)) {
      announce eOutcome, (id = reply.id, total = reply.total);
      goto Done;
    }
  }

  state Done {}
}
