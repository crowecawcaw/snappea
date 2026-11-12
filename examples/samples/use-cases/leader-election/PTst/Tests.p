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
    on eRoundLost do {
      waiting = waiting - 1;
      if (waiting == 0) { goto Uncontested; }
    }
  }

  state Uncontested {
    entry { Timeout(this, nodes, choose(3)); }
    ignore eRoundLost;
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

fun ThreeNodes(): seq[machine] {
  var nodes: seq[machine];
  while (sizeof(nodes) < 3) { nodes += (sizeof(nodes), new Node(sizeof(nodes) + 1)); }
  return nodes;
}

fun Indexes2(a: int, b: int): seq[int] {
  var s: seq[int];
  s += (0, a);
  s += (1, b);
  return s;
}

// The core race: nodes 1 and 2 stand at once and node 3, the deciding voter, crashes
// and restarts at any point, including between granting one candidate and seeing the
// other's request. Its persisted vote must keep it from granting twice.
machine TwoCandidatesVoterCrash {
  start state Init {
    entry { new Election((nodes = ThreeNodes(), timeouts = Indexes2(0, 1), crash = 2)); }
  }
}

// Split vote: all three stand in the same round, each votes for itself, nobody wins;
// the uncontested round must still elect a leader. Requests also cross terms here,
// so candidates see higher terms and step down.
machine SplitVote {
  start state Init {
    entry {
      var t: seq[int];
      t = Indexes2(0, 1);
      t += (2, 2);
      new Election((nodes = ThreeNodes(), timeouts = t, crash = -1));
    }
  }
}

// Generated: one or two candidates, chosen from the three, and maybe a crash of a
// node that is not standing. Sampled.
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

machine BuggyTwoCandidatesVoterCrash {
  start state Init {
    entry {
      var nodes: seq[machine];
      while (sizeof(nodes) < 3) { nodes += (sizeof(nodes), new ForgetfulNode(sizeof(nodes) + 1)); }
      new Election((nodes = nodes, timeouts = Indexes2(0, 1), crash = 2));
    }
  }
}

test tcTwoCandidatesVoterCrash [main=TwoCandidatesVoterCrash]:
  assert OneLeaderPerTerm, SomeLeaderElected in
  { TwoCandidatesVoterCrash, Election, Node };

test tcSplitVote [main=SplitVote]:
  assert OneLeaderPerTerm, SomeLeaderElected in
  { SplitVote, Election, Node };

test tcGeneratedElection [main=GeneratedElection]:
  assert OneLeaderPerTerm, SomeLeaderElected in
  { GeneratedElection, Election, Node };

test tcStaleVoteReply [main=StaleVoteReply]:
  assert OneLeaderPerTerm, SomeLeaderElected in
  { StaleVoteReply, Node };

// expect-bug: voted twice in term
test tcBugVoteNotPersisted [main=BuggyTwoCandidatesVoterCrash]:
  assert OneLeaderPerTerm, SomeLeaderElected in
  { BuggyTwoCandidatesVoterCrash, Election, ForgetfulNode };
