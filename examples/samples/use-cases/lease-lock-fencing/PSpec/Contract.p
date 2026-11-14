// The lease and fencing contract, over events the lock service and storage announce at
// their commit points. The specs rebuild leases, tokens and the storage order from the
// history alone; they never read the services' fields.

event Granted: (id: int, token: int);
event Busy: int;
event Expired: int;
event Released: int;
event WriteAccepted: (id: int, token: int);
event WriteRejected: (id: int, token: int);

// Safety.
// Leases: at most one live lease; tokens strictly increase; a busy answer needs a live
// lease; a release or expiry ends only the live lease.
// Fencing: storage accepts writes only from the client a token was granted to, in
// token order, so once a later holder has written, no earlier holder's write is ever
// accepted; a write is rejected only when a later token already wrote.
spec FencedWrites observes Granted, Busy, Expired, Released, WriteAccepted, WriteRejected {
  // token -> client it was granted to
  var grantedTo: map[int, int];
  var lastToken: int;
  // token of the live lease; 0 when free
  var live: int;
  var highestWritten: int;

  start state Watching {
    on Granted do (p: (id: int, token: int)) {
      assert live == 0,
        format("lease {0} granted to client {1} while lease {2} is live", p.token, p.id, live);
      assert p.token > lastToken,
        format("token {0} granted after token {1}: tokens must increase", p.token, lastToken);
      lastToken = p.token;
      grantedTo[p.token] = p.id;
      live = p.token;
    }
    on Busy do (id: int) {
      assert live != 0, format("client {0} told the lock is busy, but no lease is live", id);
    }
    on Expired do (token: int) { End(token, "expired"); }
    on Released do (token: int) { End(token, "released"); }
    on WriteAccepted do (p: (id: int, token: int)) {
      assert p.token in grantedTo && grantedTo[p.token] == p.id,
        format("write by client {0} accepted with token {1}, which was not granted to it", p.id, p.token);
      assert p.token >= highestWritten,
        format("write with token {0} accepted after token {1}: superseded holder wrote", p.token, highestWritten);
      highestWritten = p.token;
    }
    on WriteRejected do (p: (id: int, token: int)) {
      assert p.token in grantedTo && grantedTo[p.token] == p.id,
        "rejected write used another client's token";
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
type ClientPlan = (id: int, behavior: Behavior, writes: int, maxAttempts: int);
event ClientsPlanned: int;
event ClientStarted: ClientPlan;

spec ClientsFinish observes ClientsPlanned, ClientStarted, Granted, Busy, Expired, Released,
    WriteAccepted, WriteRejected, ClientDone {
  // client -> its plan; removed when it finishes
  var started: map[int, ClientPlan];
  // client -> its token
  var tokenOf: map[int, int];
  var busy: map[int, int];
  var writesSeen: map[int, int];
  var due: map[int, Outcome];
  var live: int;
  var unfinished: int;

  start state Idle {
    on ClientsPlanned do (n: int) {
      assert n > 0, "planned no clients";
      unfinished = n;
      goto Running;
    }
    on ClientStarted do { assert false, "client started without a plan"; }
    on Granted do { assert false, "grant before any client started"; }
    on Busy do { assert false, "busy answer before any client started"; }
    on WriteAccepted do { assert false, "write before any client started"; }
    on WriteRejected do { assert false, "write before any client started"; }
    on ClientDone do { assert false, "client done before it started"; }
    ignore Expired, Released;
  }

  hot state Running {
    on ClientsPlanned do { assert false, "clients planned twice"; }
    on ClientStarted do (p: ClientPlan) {
      assert !(p.id in started), format("client {0} started twice", p.id);
      assert p.writes > 0 && p.maxAttempts > 0, format("client {0} planned no writes or no attempts", p.id);
      started[p.id] = p;
      busy[p.id] = 0;
    }
    on Granted do (p: (id: int, token: int)) {
      Known(p.id);
      assert !(p.id in tokenOf), format("client {0} granted twice", p.id);
      tokenOf[p.id] = p.token;
      live = p.token;
      if (started[p.id].behavior == Crashes) { due[p.id] = Crashed; }
    }
    on Busy do (id: int) {
      Known(id);
      busy[id] = busy[id] + 1;
      assert busy[id] <= started[id].maxAttempts,
        format("client {0} asked for the lock more than {1} times", id, started[id].maxAttempts);
      if (busy[id] == started[id].maxAttempts) { due[id] = GaveUp; }
    }
    on Expired do { live = 0; Settle(); }
    on Released do { live = 0; Settle(); }
    on WriteAccepted do (p: (id: int, token: int)) { NoteWrite(p.id, p.token, Wrote); }
    on WriteRejected do (p: (id: int, token: int)) { NoteWrite(p.id, p.token, Fenced); }
    on ClientDone do (p: (id: int, outcome: Outcome)) {
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
    on ClientsPlanned do { assert false, "clients planned twice"; }
    on ClientStarted do { assert false, "client started after all finished"; }
    on Granted do { assert false, "grant after all clients finished"; }
    on Busy do { assert false, "busy answer after all clients finished"; }
    on WriteAccepted do { assert false, "write after all clients finished"; }
    on WriteRejected do { assert false, "write after all clients finished"; }
    on ClientDone do { assert false, "client done after all finished"; }
    ignore Expired, Released;
  }

  fun Known(id: int) {
    assert id in started, format("client {0} acted but was never started", id);
  }

  fun NoteWrite(id: int, token: int, outcome: Outcome) {
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
