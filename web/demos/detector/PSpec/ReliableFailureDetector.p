// Failure detection: the detector sends heartbeats and suspects silent nodes. Delayed
// replies can look like crashes; the rule checks reported failures against actual crashes.
// This observer checks a rule; it does not implement the system. Hot states require
// eventual progress.

// ReliableFailureDetector is a liveness property to assert that all nodes that have been
// shutdown by the failure injector will eventually be detected by the failure detector as
// a failed node

spec ReliableFailureDetector observes NotifyNodesDown, ShutDown {
  // set of nodes that are shutdown by the failure injector but
  // have not been detected.
  var nodesShutdownAndNotDetected: set[Node];
  // set of nodes that have been marked as down by the failure detector
  var nodesDownDetected: set[Node];

  // State where all the nodes that are shutdown by failure injector
  // have been detected by the failure detector
  start state AllShutdownNodesAreDetected {
    on NotifyNodesDown do (nodes: set[Node])
    {
      var i: int;
      while(i < sizeof(nodes))
      {
        nodesShutdownAndNotDetected -= (nodes[i]);
        nodesDownDetected += (nodes[i]);
        i = i + 1;
      }
    }

    on ShutDown do (node: machine) {
      if(!((node as Node) in nodesDownDetected)) {
        nodesShutdownAndNotDetected += (node as Node);
        goto NodesShutDownButNotDetected;
      }
    }
  }

  // intermediate "unstable" state where the failure detector has not detected all the nodes
  // that have been shutdown by the failure injector
  hot state NodesShutDownButNotDetected {
    on NotifyNodesDown do (nodes: set[Node])
    {
      var i: int;
      while(i < sizeof(nodes))
      {
        nodesShutdownAndNotDetected -= (nodes[i]);
        nodesDownDetected += (nodes[i]);
        i = i + 1;
      }

      if(sizeof(nodesShutdownAndNotDetected) == 0)
      goto AllShutdownNodesAreDetected; // return to the stable state
    }

    on ShutDown do (node: machine) {
      if(!((node as Node) in nodesDownDetected))
      nodesShutdownAndNotDetected += (node as Node);
    }
  }
}
