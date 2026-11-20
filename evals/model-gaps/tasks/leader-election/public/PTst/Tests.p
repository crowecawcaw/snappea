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

test tcTwoCandidates [main=TwoCandidates]:
  assert ElectionSafety, SomeLeaderElected, CandidaciesConclude in
  { TwoCandidates, Election, Node };
