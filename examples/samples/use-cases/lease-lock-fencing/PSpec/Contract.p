// The lease and fencing contract, over events the lock service and storage announce at
// their commit points. The specs rebuild leases, tokens and the storage order from the
// history alone; they never read the services' fields.

event eGranted: (id: int, token: int);
event eBusy: int;
event eExpired: int;
event eReleased: int;
event eWriteAccepted: (id: int, token: int);
event eWriteRejected: (id: int, token: int);

// Safety.
// Leases: at most one live lease; tokens strictly increase; a busy answer needs a live
// lease; a release or expiry ends only the live lease.
// Fencing: storage accepts writes only from the client a token was granted to, in
// token order, so once a later holder has written, no earlier holder's write is ever
// accepted; a write is rejected only when a later token already wrote.
spec FencedWrites observes eGranted, eBusy, eExpired, eReleased, eWriteAccepted, eWriteRejected {
  // token -> client it was granted to
  var grantedTo: map[int, int];
  var lastToken: int;
  // token of the live lease; 0 when free
  var live: int;
  var highestWritten: int;

  start state Watching {
    on eGranted do (p: (id: int, token: int)) {
      assert live == 0,
        format("lease {0} granted to client {1} while lease {2} is live", p.token, p.id, live);
      assert p.token > lastToken,
        format("token {0} granted after token {1}: tokens must increase", p.token, lastToken);
      lastToken = p.token;
      grantedTo[p.token] = p.id;
      live = p.token;
    }
    on eBusy do (id: int) {
      assert live != 0, format("client {0} told the lock is busy, but no lease is live", id);
    }
    on eExpired do (token: int) { End(token, "expired"); }
    on eReleased do (token: int) { End(token, "released"); }
    on eWriteAccepted do (p: (id: int, token: int)) {
      assert p.token in grantedTo && grantedTo[p.token] == p.id,
        format("write by client {0} accepted with token {1}, which was not granted to it", p.id, p.token);
      assert p.token >= highestWritten,
        format("write with token {0} accepted after token {1}: superseded holder wrote", p.token, highestWritten);
      highestWritten = p.token;
    }
    on eWriteRejected do (p: (id: int, token: int)) {
      assert p.token < highestWritten,
        format("write by client {0} with token {1} rejected, but no later token had written", p.id, p.token);
    }
  }

  fun End(token: int, how: string) {
    assert token == live, format("lease {0} {1}, but the live lease is {2}", token, how, live);
    live = 0;
  }
}

// Progress: every client the driver starts ends, with the outcome the lock's and
// storage's decisions justify: Wrote (every write accepted) or Fenced (a write
// rejected) once granted, Crashed only if planned, GaveUp only after maxAttempts
// busy answers. A client that wrote does not finish while its lease is live, and the
// test is not over while any lease is live: a crashed holder's lease must expire.
type tClientPlan = (id: int, behavior: tBehavior, writes: int, maxAttempts: int);
event eClientsPlanned: int;
event eClientStarted: tClientPlan;

spec ClientsFinish observes eClientsPlanned, eClientStarted, eGranted, eBusy, eExpired, eReleased,
    eWriteAccepted, eWriteRejected, eClientDone {
  // client -> its plan; removed when it finishes
  var started: map[int, tClientPlan];
  // client -> its token
  var tokenOf: map[int, int];
  var busy: map[int, int];
  var writesSeen: map[int, int];
  var due: map[int, tOutcome];
  var live: int;
  var unfinished: int;

  start state Idle {
    on eClientsPlanned do (n: int) {
      assert n > 0, "planned no clients";
      unfinished = n;
      goto Busy;
    }
    on eClientStarted do { assert false, "client started without a plan"; }
    on eGranted do { assert false, "grant before any client started"; }
    on eBusy do { assert false, "busy answer before any client started"; }
    on eWriteAccepted do { assert false, "write before any client started"; }
    on eWriteRejected do { assert false, "write before any client started"; }
    on eClientDone do { assert false, "client done before it started"; }
    ignore eExpired, eReleased;
  }

  hot state Busy {
    on eClientsPlanned do { assert false, "clients planned twice"; }
    on eClientStarted do (p: tClientPlan) {
      assert !(p.id in started), format("client {0} started twice", p.id);
      assert p.writes > 0 && p.maxAttempts > 0, format("client {0} planned no writes or no attempts", p.id);
      started[p.id] = p;
      busy[p.id] = 0;
    }
    on eGranted do (p: (id: int, token: int)) {
      Known(p.id);
      assert !(p.id in tokenOf), format("client {0} granted twice", p.id);
      tokenOf[p.id] = p.token;
      live = p.token;
      if (started[p.id].behavior == Crashes) { due[p.id] = Crashed; }
    }
    on eBusy do (id: int) {
      Known(id);
      busy[id] = busy[id] + 1;
      assert busy[id] <= started[id].maxAttempts,
        format("client {0} asked for the lock more than {1} times", id, started[id].maxAttempts);
      if (busy[id] == started[id].maxAttempts) { due[id] = GaveUp; }
    }
    on eExpired do { live = 0; Settle(); }
    on eReleased do { live = 0; Settle(); }
    on eWriteAccepted do (p: (id: int, token: int)) { Wrote(p.id, p.token, Wrote); }
    on eWriteRejected do (p: (id: int, token: int)) { Wrote(p.id, p.token, Fenced); }
    on eClientDone do (p: (id: int, outcome: tOutcome)) {
      assert p.id in started, format("client {0} finished twice or never started", p.id);
      assert p.id in due, format("client {0} finished before the services decided", p.id);
      assert p.outcome == due[p.id],
        format("client {0} reported {1}, the services decided {2}", p.id, p.outcome, due[p.id]);
      assert p.outcome == Crashed || !(p.id in tokenOf) || tokenOf[p.id] != live,
        format("client {0} finished while its lease {1} is still live", p.id, live);
      started -= (p.id);
      unfinished = unfinished - 1;
      Settle();
    }
  }

  state Finished {
    on eClientsPlanned do { assert false, "clients planned twice"; }
    on eClientStarted do { assert false, "client started after all finished"; }
    on eGranted do { assert false, "grant after all clients finished"; }
    on eBusy do { assert false, "busy answer after all clients finished"; }
    on eWriteAccepted do { assert false, "write after all clients finished"; }
    on eWriteRejected do { assert false, "write after all clients finished"; }
    on eClientDone do { assert false, "client done after all finished"; }
    ignore eExpired, eReleased;
  }

  fun Known(id: int) {
    assert id in started, format("client {0} acted but was never started", id);
  }

  fun Wrote(id: int, token: int, outcome: tOutcome) {
    Known(id);
    assert id in tokenOf && tokenOf[id] == token,
      format("client {0} wrote with token {1}, not its own", id, token);
    assert !(id in due) || due[id] == Wrote,
      format("client {0} wrote after it was fenced or crashed", id);
    assert !(id in writesSeen) || writesSeen[id] < started[id].writes,
      format("client {0} wrote more than the {1} writes planned", id, started[id].writes);
    if (!(id in writesSeen)) { writesSeen[id] = 0; }
    writesSeen[id] = writesSeen[id] + 1;
    // Wrote only once every planned write was accepted.
    if (outcome == Fenced || writesSeen[id] == started[id].writes) { due[id] = outcome; }
  }

  fun Settle() {
    if (unfinished == 0 && live == 0) { goto Finished; }
  }
}
