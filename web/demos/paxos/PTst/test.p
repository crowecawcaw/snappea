// Paxos: proposers seek promises, then acceptance from a majority. Learners observe the
// chosen value; the agreement rule rejects conflicting decisions.
// This test harness chooses the number of actors and starts the workload. The checker
// varies their scheduling and model choices.

test testBasicPaxos3on5 [main = BasicPaxos3on5]:
assert OneValueTaught in (union Paxos, {
  BasicPaxos3on5
}
);

test testBasicPaxos3on3 [main = BasicPaxos3on3]:
assert OneValueTaught in (union Paxos, {
  BasicPaxos3on3
}
);

test testBasicPaxos3on1 [main = BasicPaxos3on1]:
assert OneValueTaught in (union Paxos, {
  BasicPaxos3on1
}
);

test testBasicPaxos2on3 [main = BasicPaxos2on3]:
assert OneValueTaught in (union Paxos, {
  BasicPaxos2on3
}
);

test testBasicPaxos2on2 [main = BasicPaxos2on2]:
assert OneValueTaught in (union Paxos, {
  BasicPaxos2on2
}
);

test testBasicPaxos1on1 [main = BasicPaxos1on1]:
assert OneValueTaught in (union Paxos, {
  BasicPaxos1on1
}
);

type PaxosConfigData = (n_proposers: int, n_acceptors: int, n_learners: int);

event PaxosConfig: (quorum: int);

fun SetupPaxos(config: PaxosConfigData) {
  var i: int;
  var proposers: set[Proposer];
  var acceptors: set[Acceptor];
  var learners: set[Learner];

  var proposerCfg: ProposerConfig;

  announce PaxosConfig, (quorum = config.n_acceptors / 2 + 1,);

  i = 0;
  while (i < config.n_acceptors) {
    i = i + 1;
    acceptors += (new Acceptor());
  }

  i = 0;
  while (i < config.n_learners) {
    i = i + 1;
    learners += (new Learner());
  }

  i = 0;
  while (i < config.n_proposers) {
    i = i + 1;
    proposerCfg = (acceptors = acceptors, learners = learners, valueToPropose = i + 100 + choose(50), proposerId = i + choose(50));
    proposers += (new Proposer(proposerCfg));
  }
}

machine BasicPaxos3on5 {
  start state Init {
    entry {
      var config: PaxosConfigData;
      config = (n_proposers = 3, n_acceptors = 5, n_learners = 1);
      SetupPaxos(config);
    }
  }
}

machine BasicPaxos3on3 {
  start state Init {
    entry {
      var config: PaxosConfigData;
      config = (n_proposers = 3, n_acceptors = 3, n_learners = 1);
      SetupPaxos(config);
    }
  }
}

machine BasicPaxos3on1 {
  start state Init {
    entry {
      var config: PaxosConfigData;
      config = (n_proposers = 3, n_acceptors = 1, n_learners = 1);
      SetupPaxos(config);
    }
  }
}

machine BasicPaxos2on3 {
  start state Init {
    entry {
      var config: PaxosConfigData;
      config = (n_proposers = 2, n_acceptors = 3, n_learners = 1);
      SetupPaxos(config);
    }
  }
}

machine BasicPaxos2on2 {
  start state Init {
    entry {
      var config: PaxosConfigData;
      config = (n_proposers = 2, n_acceptors = 2, n_learners = 1);
      SetupPaxos(config);
    }
  }
}

machine BasicPaxos1on1 {
  start state Init {
    entry {
      var config: PaxosConfigData;
      config = (n_proposers = 1, n_acceptors = 1, n_learners = 1);
      SetupPaxos(config);
    }
  }
}

machine BasicPaxos4on4 {
  start state Init {
    entry {
      var config: PaxosConfigData;
      config = (n_proposers = 4, n_acceptors = 4, n_learners = 1);
      SetupPaxos(config);
    }
  }
}
