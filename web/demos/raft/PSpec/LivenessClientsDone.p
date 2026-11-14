// Raft: servers elect a leader and replicate its log despite delayed or dropped messages.
// Progress: every client the test starts eventually finishes its workload.

// The View announces how many clients it starts before starting any of them.
event ClientsPlanned: int;

spec LivenessClientsDone observes ClientsPlanned, ClientFinishedMonitor {
  var planned: int;
  var finishedClients: set[Client];

  start cold state NoClients {
    on ClientsPlanned do (n: int) {
      assert n > 0, "a test must start at least one client";
      planned = n;
      goto ClientsActive;
    }
    on ClientFinishedMonitor do (c: Client) {
      assert false, format("client {0} finished, but no clients were planned", c);
    }
  }

  hot state ClientsActive {
    on ClientFinishedMonitor do (c: Client) {
      assert !(c in finishedClients), format("client {0} finished twice", c);
      finishedClients += (c);
      if (sizeof(finishedClients) == planned) {
        goto AllClientsDone;
      }
    }
  }

  cold state AllClientsDone {
    on ClientFinishedMonitor do (c: Client) {
      assert false, format("client {0} finished after all {1} planned clients were done", c, planned);
    }
  }
}
