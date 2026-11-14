// Raft: servers elect a leader and replicate its log despite delayed or dropped messages.
// Safety rules compare leaders, log entries, and applied commands.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.
// $ is a model choice: the checker explores both outcomes across schedules.

// A View service that manages the view of the cluster. It is responsible for maintaining
// roles of, injecting failures to and shutting down the nodes.

// Shutdown message will be sent to the cluster when
// client notifies the view service that it has finished all requests AND got all responses
event Shutdown;
// A node can notify the view service about its log entires
// This is used for choosing servers that should receive an election timeout
event NotifyLog: (server: Server, log: seq[ServerLog]);

// snappea: driver
// Test environment: starts the cluster and clients, injects timeouts and crashes.
machine View {
  // the cluster nodes
  var servers: set[Server];
  // the probability that a node got an election timeout [0, 100]
  // this is used to model network failure (i.e. a follower does not receive heartbeat)
  var timeoutRate: int;
  // the probability that a node crashes [0, 100]
  var crashRate: int;
  // the total number failures that can be injected
  var numFailures: int;
  // election timeouts still to force, one per round, each to a server the checker chooses,
  // whether or not there is a leader: timers that fire while an election is under way
  var forcedElections: int;
  // the timer for the view service to perform an action
  var triggerTimer: Timer;
  // the set of clients that have finished all requests
  var clientsDone: set[machine];
  // the number of clients
  var numClients: int;

  // the set of followers, leaders and candidates
  var followers: set[Server];
  var leaders: set[Server];
  var candidates: set[Server];
  // the set of servers that have not sent a `RequestVote` message
  // this set will be exahusted when the view service
  // cannot detect any leader in the cluster
  var requestVotePendingSet: set[Server];
  // the logs of nodes
  var serverLogs: map[Server, seq[ServerLog]];
  // number of rounds of actions where the view service cannot detect any leader
  var noLeaderRounds: int;

  start state Init {
    entry (setup: (numServers: int, numClients: int, timeoutRate: int, crashRate: int, numFailures: int, forcedElections: int)) {
      var i: int;
      var server: Server;
      timeoutRate = setup.timeoutRate;
      numClients = setup.numClients;
      crashRate = setup.crashRate;
      numFailures = setup.numFailures;
      forcedElections = setup.forcedElections;
      followers = default(set[Server]);
      // at a moment, there can be multiple leaders, e.g. partitioned network
      leaders = default(set[Server]);
      candidates = default(set[Server]);
      requestVotePendingSet = default(set[Server]);
      serverLogs = default(map[Server, seq[ServerLog]]);
      noLeaderRounds = 0;

      while (i < setup.numServers) {
        server = new Server();
        servers += (server);
        serverLogs += (server, default(seq[ServerLog]));
        i = i + 1;
      }

      i = 0;
      foreach (server in servers) {
        send server, ServerInit, (myId=i, cluster=servers, viewServer=this);
        followers += (server);
        i = i + 1;
      }

      triggerTimer = new Timer((user=this, timeoutEvent=HeartbeatTimeout));
      clientsDone = default(set[machine]);
      // the plan comes first, so the liveness spec waits for every client
      announce ClientsPlanned, numClients;
      i = 0;
      while (i < numClients) {
        new Client((viewService=this, servers=servers, requests=randomWorkload(3)));
        i = i + 1;
      }

      goto Monitoring;
    }
  }

  state Monitoring {
    entry {
      var server: Server;
      startTimer(triggerTimer);
      server = choose(servers);
      candidates += (server);
      send server, ElectionTimeout;
    }

    on NotifyLog do (payload: (server: Server, log: seq[ServerLog])) {
      // one server's reports arrive in the order it sent them, so the last is the newest
      serverLogs[payload.server] = payload.log;
    }

    on ViewChangedLeader do (server: Server) {
      // server change its role to a leader
      noLeaderRounds = 0;
      leaders += (server);
      followers -= (server);
      candidates -= (server);
    }

    on ViewChangedFollower do (server: Server) {
      // server change its role to a follower
      followers += (server);
      candidates -= (server);
      leaders -= (server);
    }

    on ViewChangedCandidate do (server: Server) {
      // server change its role to a candidate
      candidates += (server);
      followers -= (server);
      leaders -= (server);
    }

    on HeartbeatTimeout do {
      var server: Server;
      if (forcedElections > 0) {
        server = choose(servers);
        send server, ElectionTimeout;
        forcedElections = forcedElections - 1;
      }

      // print format("Current view: leaders={0} followers={1} candidates={2}", leaders, followers, candidates);
      if (sizeof(leaders) == 0) {
        if (noLeaderRounds % 25 == 0 && sizeof(requestVotePendingSet) > 0) {
          // non-deterministically choose a server to trigger an election
          // either the current most up-to-date server or a random server
          if ($) {
            server = mostUpToDateServer(requestVotePendingSet);
          }
          else {
            server = choose(requestVotePendingSet);
          }

          requestVotePendingSet -= (server);
          // print format("NoLeader rounds exceeded, trigger election on {0}", server);
          send server, ElectionTimeout;
          candidates += (server);
          followers -= (server);
          noLeaderRounds = 0;
        }

        if (sizeof(requestVotePendingSet) == 0) {
          requestVotePendingSet = servers;
        }

        noLeaderRounds = noLeaderRounds + 1;
      }
      else {
        foreach (server in servers) {
          send server, HeartbeatTimeout;
        }

        foreach (server in followers) {
          if (choose(100) < timeoutRate && numFailures > 0) {
            // print format("Failure Injection: timeout a follower {0}", server);
            send server, ElectionTimeout;
            numFailures = numFailures - 1;
            startTimer(triggerTimer);
            return;
          }
        }

        foreach (server in leaders) {
          if (choose(100) < crashRate && numFailures > 0) {
            // print format("Failure Injection: crash a leader {0}", server);
            send server, Reset;
            numFailures = numFailures - 1;
            startTimer(triggerTimer);
            return;
          }
        }

        foreach (server in candidates) {
          if (choose(100) < crashRate && numFailures > 0) {
            // print format("Failure Injection: crash a candidate {0}", server);
            send server, Reset;
            numFailures = numFailures - 1;
            startTimer(triggerTimer);
            return;
          }
        }
      }

      startTimer(triggerTimer);
    }

    on ClientFinished do (client: machine) {
      var i: int;
      clientsDone += (client);
      if (sizeof(clientsDone) == numClients) {
        while (i < sizeof(servers)) {
          send servers[i], Shutdown;
          i = i + 1;
        }

        goto ViewServiceEnd;
      }
    }
  }

  state ViewServiceEnd {
    entry {
      send triggerTimer, Shutdown;
    }

    ignore ClientFinished, HeartbeatTimeout, ViewChangedCandidate, NotifyLog, ViewChangedLeader, ViewChangedFollower;
  }

  fun mostUpToDateServer(choices: set[Server]): Server {
    // get the server with the most up-to-date logs (in the view of the view service)
    // it might not be the real most up-to-date server in the cluster because of the message delay
    var server: Server;
    var candidate: Server;
    var term: int;
    var length: int;
    // print format("Choose candidate: server |-> logs: {0}", serverLogs);
    foreach (server in servers) {
      if (candidate == default(Server)) {
        candidate = server;
        term = lastLogTerm(serverLogs[server]);
        length = sizeof(serverLogs[server]);
      }
      else {
        if (sizeof(serverLogs[server]) > 0) {
          if (term < lastLogTerm(serverLogs[server])) {
            term = lastLogTerm(serverLogs[server]);
            length = sizeof(serverLogs[server]);
            candidate = server;
          }
          else if (term == lastLogTerm(serverLogs[server])) {
            if (length < sizeof(serverLogs[server])) {
              length = sizeof(serverLogs[server]);
              candidate = server;
            }
          }
        }
      }
    }

    return candidate;
  }
}
