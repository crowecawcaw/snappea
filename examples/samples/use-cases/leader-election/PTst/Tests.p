// Drivers: the election timers and the failure injector. Nodes are the design.

// timeouts: indexes of nodes whose timers fire together in the contested round.
// crash: index of a node that crashes once and restarts, or -1.
type tElection = (nodes: seq[machine], timeouts: seq[int], crash: int);

// Fires the contested round's timeouts and crashes one voter at a moment the scheduler
// picks. When every candidate has heard back from the whole cluster and none won, one node's timer fires alone: the "eventually one candidate
// runs uncontested" assumption that Raft's randomized timeouts provide.
machine Election {
  var nodes: seq[machine];
  var waiting: int;

  start state Contested {
    entry (e: tElection) {
      var i: int;
      nodes = e.nodes;
      announce eElectionsStarted;
      foreach (i in e.timeouts) {
        Timeout(this, nodes, i);
      }
      // Sent last, but the candidates' requests race with it, so the crash can land
      // before, between or after the requests in the victim's queue.
      if (e.crash >= 0) {
        send nodes[e.crash], eCrash;
      }
      waiting = sizeof(e.timeouts);
    }
    // A winner never reports, so this waits forever once someone has won.
    on eRoundLost do (r: (id: int, term: int)) {
      announce eLossReported, r;
      waiting = waiting - 1;
      if (waiting == 0) { goto Uncontested; }
    }
  }

  // If the lone candidate still loses (it was behind on terms), it has now learned
  // the highest term, and so has everyone else: fire one timer alone again.
  state Uncontested {
    entry { Timeout(this, nodes, choose(3)); }
    on eRoundLost do (r: (id: int, term: int)) {
      announce eLossReported, r;
      goto Uncontested;
    }
  }
}

// Fires node i's election timer, handing it its peers.
fun Timeout(driver: machine, nodes: seq[machine], i: int) {
  var j: int;
  var peers: seq[machine];
  while (j < sizeof(nodes)) {
    if (j != i) { peers += (sizeof(peers), nodes[j]); }
    j = j + 1;
  }
  send nodes[i], eTimeout, (peers = peers, driver = driver);
}

// Starts a node from durable state, telling the specs what that state is.
fun NewNode(d: tDisk): machine {
  announce eDisk, d;
  return new Node(d);
}

fun ThreeNodes(): seq[machine] {
  var nodes: seq[machine];
  while (sizeof(nodes) < 3) { nodes += (sizeof(nodes), NewNode((id = sizeof(nodes) + 1, term = 0, votedTerm = 0))); }
  return nodes;
}

fun Indexes2(a: int, b: int): seq[int] {
  var s: seq[int];
  s += (0, a);
  s += (1, b);
  return s;
}

// The core race: nodes 1 and 2 stand at once and node 3 decides. Either candidate
// may see the other's request before its own timer fires and stand in a higher term,
// so candidates receive higher terms, step down, and get replies from older terms.
machine TwoCandidates {
  start state Init {
    entry { new Election((nodes = ThreeNodes(), timeouts = Indexes2(0, 1), crash = -1)); }
  }
}

// Split vote: node 3 already voted for itself in term 1 (its requests were lost) and
// nodes 1 and 2 stand in term 1 together. Every term-1 request is refused, nobody wins
// term 1, both candidates report the loss, and the uncontested round must still elect
// a leader. Requests also cross terms here, so candidates see higher terms and step down.
machine SplitVote {
  start state Init {
    entry {
      var nodes: seq[machine];
      nodes += (0, NewNode((id = 1, term = 0, votedTerm = 0)));
      nodes += (1, NewNode((id = 2, term = 0, votedTerm = 0)));
      nodes += (2, NewNode((id = 3, term = 1, votedTerm = 1)));
      new Election((nodes = nodes, timeouts = Indexes2(0, 1), crash = -1));
    }
  }
}

// Candidate receives a higher term in a reply: node 3 (and in some runs node 2)
// restarts from disk at term 5, so its answer to node 1's term-1 request carries
// term 5. Node 1 must step down, or, if node 2's grant came first, it already leads
// term 1. When both answers carry term 5 node 1 loses, and its next candidacy must
// be above term 5.
machine HigherTermReply {
  start state Init {
    entry {
      var nodes: seq[machine];
      var t: seq[int];
      var term2: int;
      if ($) { term2 = 5; }
      nodes += (0, NewNode((id = 1, term = 0, votedTerm = 0)));
      nodes += (1, NewNode((id = 2, term = term2, votedTerm = 0)));
      nodes += (2, NewNode((id = 3, term = 5, votedTerm = 0)));
      t += (0, 0);
      new Election((nodes = nodes, timeouts = t, crash = -1));
    }
  }
}

// A peer that is down for good: requests to it are never answered.
machine Unreachable {
  start state Down {
    ignore eVoteReq;
  }
}

// Majority with one node down: node 1 stands with node 2 up and node 3 unreachable.
// Its own vote plus node 2's is a majority, so it must win without node 3.
machine OneNodeDown {
  start state Init {
    entry {
      var nodes: seq[machine];
      nodes += (0, NewNode((id = 1, term = 0, votedTerm = 0)));
      nodes += (1, NewNode((id = 2, term = 0, votedTerm = 0)));
      nodes += (2, new Unreachable());
      announce eElectionsStarted;
      Timeout(this, nodes, 0);
    }
    ignore eRoundLost;
  }
}

// Generated: one or two candidates, chosen from the three, and maybe a crash and
// restart of a node that is not standing, at any moment: between granting one
// candidate and seeing the other's request, its persisted vote must keep it from
// granting twice. Sampled.
machine GeneratedElection {
  start state Init {
    entry {
      var t: seq[int];
      var a: int;
      var b: int;
      var crash: int;
      a = choose(3);
      t += (0, a);
      crash = -1;
      b = (a + 1 + choose(2)) % 3;
      if ($) {
        t += (1, b);
        if ($) { crash = 3 - a - b; }
      } else {
        if ($) { crash = b; }
      }
      new Election((nodes = ThreeNodes(), timeouts = t, crash = crash));
    }
  }
}

// Stale vote reply: node 1's timer fires twice in a row, so replies to its term-1
// requests can arrive while it campaigns in term 2. They must not count toward term 2.
// No other node stands, so node 1 must win term 1 or term 2.
machine StaleVoteReply {
  start state Init {
    entry {
      var nodes: seq[machine];
      nodes = ThreeNodes();
      announce eElectionsStarted;
      Timeout(this, nodes, 0);
      Timeout(this, nodes, 0);
    }
    ignore eRoundLost;
  }
}

// The negative control's race: node 3 crashes and restarts at a moment the scheduler
// picks while nodes 1 and 2 stand.
machine BuggyTwoCandidatesVoterCrash {
  start state Init {
    entry {
      var nodes: seq[machine];
      while (sizeof(nodes) < 3) {
        announce eDisk, (id = sizeof(nodes) + 1, term = 0, votedTerm = 0);
        nodes += (sizeof(nodes), new ForgetfulNode((id = sizeof(nodes) + 1, term = 0, votedTerm = 0)));
      }
      new Election((nodes = nodes, timeouts = Indexes2(0, 1), crash = 2));
    }
  }
}

test tcTwoCandidates [main=TwoCandidates]:
  assert ElectionSafety, SomeLeaderElected, CandidaciesConclude in
  { TwoCandidates, Election, Node };

test tcSplitVote [main=SplitVote]:
  assert ElectionSafety, SomeLeaderElected, CandidaciesConclude in
  { SplitVote, Election, Node };

test tcHigherTermReply [main=HigherTermReply]:
  assert ElectionSafety, SomeLeaderElected, CandidaciesConclude in
  { HigherTermReply, Election, Node };

// Node 3 never answers, so CandidaciesConclude does not apply.
test tcOneNodeDown [main=OneNodeDown]:
  assert ElectionSafety, SomeLeaderElected in
  { OneNodeDown, Unreachable, Node };

test tcGeneratedElection [main=GeneratedElection]:
  assert ElectionSafety, SomeLeaderElected, CandidaciesConclude in
  { GeneratedElection, Election, Node };

test tcStaleVoteReply [main=StaleVoteReply]:
  assert ElectionSafety, SomeLeaderElected, CandidaciesConclude in
  { StaleVoteReply, Node };

// expect-bug: voted twice in term
test tcBugVoteNotPersisted [main=BuggyTwoCandidatesVoterCrash]:
  assert ElectionSafety, SomeLeaderElected, CandidaciesConclude in
  { BuggyTwoCandidatesVoterCrash, Election, ForgetfulNode };
