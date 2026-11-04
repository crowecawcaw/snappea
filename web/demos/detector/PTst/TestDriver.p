// Failure detection: the detector sends heartbeats and suspects silent nodes. Delayed
// replies can look like crashes; the rule checks reported failures against actual crashes.
// This test harness chooses the number of actors and starts the workload. The checker
// varies their scheduling and model choices.

type SystemConfig = (
numNodes: int,
numClients: int
);

// Setup a system with 3 nodes and 2 clients
machine TestMultipleClients {
  start state Init {
    entry {
      var config: SystemConfig;
      config = (numNodes = 3, numClients = 2);
      SetupSystemWithFailureInjector(config);
    }
  }
}

param numNodes: int;
param numClients: int;

machine TestWithConfig {
  start state Init {
    entry {
      var config: SystemConfig;
      config = (numNodes = numNodes, numClients = numClients);
      SetupSystemWithFailureInjector(config);
    }
  }
}

// setup the system for failure detection
fun SetupSystemWithFailureInjector(config: SystemConfig)
{
  var i : int;
  var nodes: set[Node];
  var clients: set[Client];
  // create Nodes
  while(i < config.numNodes) {
    nodes += (new Node());
    i = i + 1;
  }

  i = 0;
  // create clients
  while(i < config.numClients) {
    clients += (new Client(nodes));
    i = i + 1;
  }

  // create the failure detector
  new FailureDetector((nodes = nodes, clients = clients));

  // create the failure injector
  new FailureInjector((nodes = nodes, nFailures = sizeof(nodes)/2 + 1));
}
