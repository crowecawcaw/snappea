// Drivers: the election timers and the failure injector. Nodes are the design.

// timeouts: indexes of nodes whose timers fire together in the contested round.
// crash: index of a node that crashes once and restarts, or -1.
type ElectionSetup = (nodes: seq[machine], timeouts: seq[int], crash: int);

// Fires the contested round's timeouts and crashes one voter at a moment the scheduler
// picks. When every candidate has heard back from the whole cluster and none won, one
// node's timer fires alone: the "eventually one candidate runs uncontested" assumption
// that Raft's randomized timeouts provide.
machine Election {
  var nodes: seq[machine];
  var waiting: int;
  // index of the node whose timer fires alone
  var lone: int;

  start state Contested {
    entry (e: ElectionSetup) {
      var i: int;
      nodes = e.nodes;
      announce ElectionsStarted;
      foreach (i in e.timeouts) {
        Timeout(this, nodes, i);
      }
      // Sent last, but the candidates' requests race with it, so the crash can land
      // before, between or after the requests in the victim's queue.
      if (e.crash >= 0) {
        send nodes[e.crash], Crash, this;
      }
      waiting = sizeof(e.timeouts);
    }
    ignore Restarted;
    // A winner never reports, so this waits forever once someone has won.
    on RoundLost do (r: (id: int, term: int)) {
      announce LossReported, r;
      waiting = waiting - 1;
      if (waiting == 0) { goto Uncontested; }
    }
  }

  // Any node may be the one whose timer fires alone. If it still loses (it was behind
  // on terms), its replies told it the highest term in the cluster, so the same node
  // stands again above every term and wins.
  state Uncontested {
    entry {
      lone = choose(sizeof(nodes));
      Timeout(this, nodes, lone);
    }
    on RoundLost do (r: (id: int, term: int)) {
      announce LossReported, r;
      Timeout(this, nodes, lone);
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
  send nodes[i], ElectionTimeout, (peers = peers, driver = driver);
}

// Starts a node from durable state, telling the specs what that state is.
fun NewNode(d: DurableState): machine {
  announce Disk, d;
  return new Node(d);
}

fun ThreeNodes(): seq[machine] {
  var nodes: seq[machine];
  announce ClusterSize, 3;
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
      announce ClusterSize, 3;
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
      announce ClusterSize, 3;
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
    ignore VoteReq;
  }
}

// Majority with one node down: node 1 stands with node 2 up and node 3 unreachable.
// Its own vote plus node 2's is a majority, so it must win without node 3.
machine OneNodeDown {
  start state Init {
    entry {
      var nodes: seq[machine];
      announce ClusterSize, 3;
      nodes += (0, NewNode((id = 1, term = 0, votedTerm = 0)));
      nodes += (1, NewNode((id = 2, term = 0, votedTerm = 0)));
      nodes += (2, new Unreachable());
      announce ElectionsStarted;
      Timeout(this, nodes, 0);
    }
    ignore RoundLost;
  }
}

// Exactly a majority of an even cluster: four nodes, node 4 unreachable, node 1
// stands. Two votes of four (its own and one grant) are half, not a majority, and must
// not win; the second grant (three of four) must.
machine FourNodesOneDown {
  start state Init {
    entry {
      var nodes: seq[machine];
      announce ClusterSize, 4;
      nodes += (0, NewNode((id = 1, term = 0, votedTerm = 0)));
      nodes += (1, NewNode((id = 2, term = 0, votedTerm = 0)));
      nodes += (2, NewNode((id = 3, term = 0, votedTerm = 0)));
      nodes += (3, new Unreachable());
      announce ElectionsStarted;
      Timeout(this, nodes, 0);
    }
    ignore RoundLost;
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
      announce ElectionsStarted;
      Timeout(this, nodes, 0);
      Timeout(this, nodes, 0);
    }
    ignore RoundLost;
  }
}

// A client that sends a node one request at a moment the scheduler picks.
machine Prober {
  start state Probing {
    entry (node: machine) {
      announce ProbeSent;
      send node, ClientReq;
    }
  }
}

// Deposed leader: node 1 stands in term 1 while node 3, restarted at term 5, stands in
// term 6, and a client sends node 1 a request at any moment. Node 1 can win
// term 1 with node 2's vote and then see a higher term in node 3's request or in its
// reply; from then on it must refuse client requests. A leader that missed the
// step-down accepts them in a term it did not win. Node 3 always wins term 6.
machine DeposedLeader {
  start state Init {
    entry {
      var nodes: seq[machine];
      announce ClusterSize, 3;
      nodes += (0, NewNode((id = 1, term = 0, votedTerm = 0)));
      nodes += (1, NewNode((id = 2, term = 0, votedTerm = 0)));
      nodes += (2, NewNode((id = 3, term = 5, votedTerm = 0)));
      announce ElectionsStarted;
      Timeout(this, nodes, 0);
      Timeout(this, nodes, 2);
      new Prober(nodes[0]);
    }
    // Node 1 loses when node 3's term reaches it first; no further round is needed.
    ignore RoundLost;
  }
}

// The timeout and crash are sent by the same driver, so the node starts a
// campaign before restarting. Votes may arrive on either side of the crash; it may
// even win first. After restart it must discard the old tally and campaign afresh.
machine CandidateRestart {
  var nodes: seq[machine];
  start state Running {
    entry {
      nodes = ThreeNodes();
      announce ElectionsStarted;
      Timeout(this, nodes, 0);
      announce RecoveryPlanned, 1;
      send nodes[0], Crash, this;
    }
    on Restarted do {
      // This request must be refused, including when the node won before crashing.
      announce ProbeSent;
      send nodes[0], ClientReq;
      Timeout(this, nodes, 0);
    }
    ignore RoundLost;
  }
}

// Both nodes vote for themselves in term 1. After node 1 loses and restarts,
// node 2's delayed RequestVote must not make it vote again in that same term.
// Only then does the timer start a fresh term that can succeed.
machine SelfVoteSurvivesRestart {
  var node: machine;
  start state Competing {
    entry {
      var peers: seq[machine];
      announce ClusterSize, 2;
      announce Disk, (id = 2, term = 0, votedTerm = 0);
      node = NewNode((id = 1, term = 0, votedTerm = 0));
      peers += (0, this);
      announce ElectionsStarted;
      send node, ElectionTimeout, (peers = peers, driver = this);
    }
    on VoteReq do (r: (term: int, candidate: machine, cid: int)) {
      assert r.term == 1 && r.cid == 1, "first candidacy must be node 1 in term 1";
      announce ElectionStarted, (id = 2, term = 1);
      announce VoteAnswered, (voter = 2, candidate = r.cid, reqTerm = r.term,
        term = 1, granted = false);
      send r.candidate, VoteResp, (reqTerm = r.term, term = 1, granted = false);
    }
    on RoundLost do (r: (id: int, term: int)) {
      announce LossReported, r;
      announce RecoveryPlanned, 1;
      send node, Crash, this;
      goto Restarting;
    }
  }
  state Restarting {
    on Restarted do {
      send node, VoteReq, (term = 1, candidate = this, cid = 2);
      goto AwaitingVote;
    }
  }
  state AwaitingVote {
    on VoteResp do (r: (reqTerm: int, term: int, granted: bool)) {
      var peers: seq[machine];
      assert r.reqTerm == 1 && r.term == 1 && !r.granted,
        "restart must preserve the self-vote in the current term";
      announce HeardAll, (id = 2, term = 1);
      peers += (0, this);
      send node, ElectionTimeout, (peers = peers, driver = this);
      goto FreshElection;
    }
  }
  state FreshElection {
    on VoteReq do (r: (term: int, candidate: machine, cid: int)) {
      assert r.term == 2 && r.cid == 1, "recovery must start a fresh term";
      announce VoteAnswered, (voter = 2, candidate = r.cid, reqTerm = r.term,
        term = 2, granted = true);
      send r.candidate, VoteResp, (reqTerm = r.term, term = 2, granted = true);
    }
  }
}

// A controlled peer isolates the two higher-term step-down paths. It persists
// term 5 before the test; in the request case it also votes for itself and asks the
// target for a vote. There is no packet loss or duplicated reply.
machine StepDownRestart {
  var node: machine;
  var requestFirst: bool;
  var peerTerm: int;
  start state Setup {
    entry (byRequest: bool) {
      var peers: seq[machine];
      requestFirst = byRequest;
      peerTerm = 5;
      announce ClusterSize, 2;
      announce Disk, (id = 2, term = 5, votedTerm = 0);
      node = NewNode((id = 1, term = 0, votedTerm = 0));
      peers += (0, this);
      announce ElectionsStarted;
      send node, ElectionTimeout, (peers = peers, driver = this);
      if (byRequest) {
        // Peer starts a newer election. Its request is queued before its reply to
        // node 1, so it is definitely the request that first makes node 1 step down.
        peerTerm = 6;
        announce ElectionStarted, (id = 2, term = peerTerm);
        send node, VoteReq, (term = peerTerm, candidate = this, cid = 2);
      }
      goto BeforeCrash;
    }
  }
  state BeforeCrash {
    on VoteReq do (r: (term: int, candidate: machine, cid: int)) { Reply(r); }
    on VoteResp do (p: (reqTerm: int, term: int, granted: bool)) {
      assert requestFirst && p.reqTerm == peerTerm && p.term == peerTerm && p.granted,
        "target must acknowledge the higher-term request before the crash";
      announce HeardAll, (id = 2, term = peerTerm);
      CrashTarget();
    }
    on RoundLost do (r: (id: int, term: int)) {
      announce LossReported, r;
      CrashTarget();
    }
  }
  state AfterCrash {
    on VoteReq do (r: (term: int, candidate: machine, cid: int)) { Reply(r); }
    on Restarted do {
      var peers: seq[machine];
      announce ProbeSent;
      send node, ClientReq;
      peers += (0, this);
      send node, ElectionTimeout, (peers = peers, driver = this);
    }
    // A loss notification already sent before the crash can arrive afterwards.
    on RoundLost do (r: (id: int, term: int)) { announce LossReported, r; }
  }
  fun CrashTarget() {
    announce RecoveryPlanned, 1;
    send node, Crash, this;
    goto AfterCrash;
  }
  fun Reply(r: (term: int, candidate: machine, cid: int)) {
    var grant: bool;
    grant = r.term > peerTerm;
    if (grant) { peerTerm = r.term; }
    announce VoteAnswered, (voter = 2, candidate = r.cid, reqTerm = r.term,
      term = peerTerm, granted = grant);
    send r.candidate, VoteResp, (reqTerm = r.term, term = peerTerm, granted = grant);
  }
}

machine RequestStepDownRestart {
  start state Init { entry { new StepDownRestart(true); } }
}
machine ReplyStepDownRestart {
  start state Init { entry { new StepDownRestart(false); } }
}

scenario CrashBeforeWinning observes ElectionStarted, BecameLeader, Restarted {
  start hot state Waiting {
    on ElectionStarted do (s: (id: int, term: int)) {
      if (s.id == 1 && s.term == 1) { goto Campaigning; }
    }
  }
  hot state Campaigning {
    on BecameLeader goto WonFirst;
    on Restarted do (r: (id: int, term: int)) {
      if (r.id == 1 && r.term == 1) { goto Covered; }
    }
  }
  hot state WonFirst { }
  cold state Covered { }
}

scenario RestartAfterWinning observes BecameLeader, Restarted, ClientAnswered {
  start hot state Waiting {
    on BecameLeader do (l: (id: int, term: int)) {
      if (l.id == 1 && l.term == 1) { goto Leading; }
    }
  }
  hot state Leading {
    on Restarted do (r: (id: int, term: int)) {
      if (r.id == 1 && r.term == 1) { goto RestartedLeader; }
    }
  }
  hot state RestartedLeader {
    on ClientAnswered do (a: (id: int, term: int, accepted: bool)) {
      if (a.id == 1 && a.term == 1 && !a.accepted) { goto Covered; }
    }
  }
  cold state Covered { }
}

// The negative control's race: node 3 crashes and restarts at a moment the scheduler
// picks while nodes 1 and 2 stand.
machine BuggyTwoCandidatesVoterCrash {
  start state Init {
    entry {
      var nodes: seq[machine];
      announce ClusterSize, 3;
      while (sizeof(nodes) < 3) {
        announce Disk, (id = sizeof(nodes) + 1, term = 0, votedTerm = 0);
        nodes += (sizeof(nodes), new ForgetfulNode((id = sizeof(nodes) + 1, term = 0, votedTerm = 0)));
      }
      new Election((nodes = nodes, timeouts = Indexes2(0, 1), crash = 2));
    }
  }
}

test tcTwoCandidates [main=TwoCandidates]:
  assert ElectionSafety, SomeLeaderElected, CandidaciesConclude in
  { TwoCandidates, Election, Node };

test tcDeposedLeader [main=DeposedLeader]:
  assert ElectionSafety, SomeLeaderElected, CandidaciesConclude, ProbeAnswered in
  { DeposedLeader, Prober, Node };

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

// Node 4 never answers, so CandidaciesConclude does not apply.
test tcFourNodesOneDown [main=FourNodesOneDown]:
  assert ElectionSafety, SomeLeaderElected in
  { FourNodesOneDown, Unreachable, Node };

test tcGeneratedElection [main=GeneratedElection]:
  assert ElectionSafety, SomeLeaderElected, CandidaciesConclude in
  { GeneratedElection, Election, Node };

test tcStaleVoteReply [main=StaleVoteReply]:
  assert ElectionSafety, SomeLeaderElected, CandidaciesConclude in
  { StaleVoteReply, Node };

// snappea: expect-bug "node 3 voted twice in term 1"
test tcBugVoteNotPersisted [main=BuggyTwoCandidatesVoterCrash]:
  assert ElectionSafety, SomeLeaderElected, CandidaciesConclude in
  { BuggyTwoCandidatesVoterCrash, Election, ForgetfulNode };

test tcCandidateRestart [main=CandidateRestart]:
  assert ElectionSafety, SomeLeaderElected, CandidaciesConclude, ProbeAnswered,
    LeaderAfterRestart in { CandidateRestart, Node };

test tcRequestStepDownRestart [main=RequestStepDownRestart]:
  assert ElectionSafety, SomeLeaderElected, CandidaciesConclude, ProbeAnswered,
    LeaderAfterRestart in { RequestStepDownRestart, StepDownRestart, Node };

test tcReplyStepDownRestart [main=ReplyStepDownRestart]:
  assert ElectionSafety, SomeLeaderElected, CandidaciesConclude, ProbeAnswered,
    LeaderAfterRestart in { ReplyStepDownRestart, StepDownRestart, Node };

// snappea: boundary -- a delayed competing request checks the self-vote at the restart boundary
test tcSelfVoteSurvivesRestart [main=SelfVoteSurvivesRestart]:
  assert ElectionSafety, SomeLeaderElected, CandidaciesConclude, LeaderAfterRestart in
    { SelfVoteSurvivesRestart, Node };
