// Contract: at most `capacity` clients hold a permit at once; a client is refused only
// when all permits are held; only a holder releases; every client finishes, says
// truthfully whether it got a permit, and gives back any permit it got.

enum Phase { Asking, Holding, Returned, Refused }

event ClientsPlanned: (capacity: int, clients: int);
event ClientStarted: (id: int);
event Granted: (id: int);
event Denied: (id: int);
event Released: (id: int);
event ClientFinished: (id: int, gotPermit: bool);

spec PermitsRespected observes ClientsPlanned, ClientStarted, Granted, Denied,
    Released, ClientFinished {
  var capacity: int;
  var unfinished: int;
  // Per-client state, keyed by client id: where each client is in its protocol.
  var phase: map[int, Phase];
  var holding: int;

  start state Setup {
    on ClientsPlanned do (p: (capacity: int, clients: int)) {
      capacity = p.capacity;
      unfinished = p.clients;
      goto Running;
    }
  }

  // Hot until every client has finished and every permit is back.
  hot state Running {
    on ClientStarted do (c: (id: int)) {
      assert !(c.id in phase), format("client {0} started twice", c.id);
      phase[c.id] = Asking;
    }
    on Granted do (g: (id: int)) {
      InPhase(g.id, Asking);
      holding = holding + 1;
      assert holding <= capacity,
        format("client {0} got a permit: {1} clients now hold one, capacity {2}", g.id, holding, capacity);
      phase[g.id] = Holding;
    }
    on Denied do (d: (id: int)) {
      InPhase(d.id, Asking);
      assert holding == capacity,
        format("client {0} was refused while only {1} of {2} permits were held", d.id, holding, capacity);
      phase[d.id] = Refused;
    }
    on Released do (r: (id: int)) {
      InPhase(r.id, Holding);
      holding = holding - 1;
      phase[r.id] = Returned;
      Settle();
    }
    on ClientFinished do (f: (id: int, gotPermit: bool)) {
      assert f.id in phase, format("unknown client {0} finished", f.id);
      assert f.gotPermit == (phase[f.id] != Refused),
        format("client {0} says gotPermit = {1}, but the server's decision was {2}", f.id, f.gotPermit, phase[f.id]);
      unfinished = unfinished - 1;
      Settle();
    }
  }

  // Every client finished and every permit is back. Nothing else may happen.
  state Finished {}

  fun Settle() {
    if (unfinished == 0 && holding == 0) {
      goto Finished;
    }
  }

  fun InPhase(id: int, want: Phase) {
    assert id in phase, format("client {0} was never started", id);
    assert phase[id] == want, format("client {0} is {1}, expected {2}", id, phase[id], want);
  }
}
