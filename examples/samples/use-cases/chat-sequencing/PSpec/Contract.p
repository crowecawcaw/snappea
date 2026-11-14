// The chat contract, stated over what senders post, what the server sequences, and
// what each client displays. The specs rebuild the room log from the history.

event Posted: (msg: int);
event Sequenced: (sn: int, msg: int);
event Displayed: (client: int, sn: int, msg: int);
// Driver: how many messages will be posted and how many clients watch the room.
event Planned: (msgs: int, clients: int);

// Safety:
//  - the server numbers posted messages 1, 2, 3, ... with no gaps, each message once;
//  - each client displays the room's messages in sequence order, with no gaps and no
//    duplicates, each with the message the server assigned to that seq.
spec InOrderNoGaps observes Posted, Sequenced, Displayed {
  var posted: set[int];
  var log: seq[int];
  // client -> last seq it displayed
  var shown: map[int, int];

  start state Watching {
    on Posted do (p: (msg: int)) {
      assert !(p.msg in posted), format("message {0} posted twice", p.msg);
      posted += (p.msg);
    }
    on Sequenced do (s: (sn: int, msg: int)) {
      assert s.msg in posted, format("message {0} sequenced but never posted", s.msg);
      assert !(s.msg in log), format("message {0} sequenced twice", s.msg);
      assert s.sn == sizeof(log) + 1, format("seq {0} assigned after seq {1}", s.sn, sizeof(log));
      log += (sizeof(log), s.msg);
    }
    on Displayed do (d: (client: int, sn: int, msg: int)) {
      if (!(d.client in shown)) { shown[d.client] = 0; }
      assert d.sn == shown[d.client] + 1,
        format("client {0} displayed seq {1} after seq {2}: a gap or a duplicate", d.client, d.sn, shown[d.client]);
      assert d.sn <= sizeof(log) && log[d.sn - 1] == d.msg,
        format("client {0} displayed message {1} as seq {2}, which the server did not assign", d.client, d.msg, d.sn);
      shown[d.client] = d.sn;
    }
  }
}

// Progress: once the senders stop, every client eventually displays every message,
// even after a dropped connection (it reconnects and catches up).
spec EveryoneCatchesUp observes Planned, Sequenced, Displayed {
  var msgs: int;
  var clients: int;
  var sequenced: int;
  // client -> messages displayed
  var shown: map[int, int];

  start state Idle {
    on Planned do (p: (msgs: int, clients: int)) {
      assert p.msgs > 0 && p.clients > 0, "planned no messages or no clients";
      msgs = p.msgs;
      clients = p.clients;
      goto CatchingUp;
    }
    on Sequenced do { assert false, "message sequenced before the run was planned"; }
    on Displayed do { assert false, "message displayed before the run was planned"; }
  }

  hot state CatchingUp {
    on Planned do { assert false, "run planned twice"; }

    // snappea-lint: allow unused-payload-field -- InOrderNoGaps checks seq and message; this spec counts
    on Sequenced do (s: (sn: int, msg: int)) {
      sequenced = sequenced + 1;
      assert sequenced <= msgs, format("more messages sequenced than the {0} planned", msgs);
      Settle();
    }

    // snappea-lint: allow unused-payload-field -- InOrderNoGaps checks seq and message; this spec counts
    on Displayed do (d: (client: int, sn: int, msg: int)) {
      assert d.client >= 1 && d.client <= clients, format("client {0} was not planned", d.client);
      if (!(d.client in shown)) { shown[d.client] = 0; }
      shown[d.client] = shown[d.client] + 1;
      Settle();
    }
  }

  state Done {
    on Planned do { assert false, "run planned twice"; }
    on Sequenced do { assert false, "message sequenced after every client caught up"; }
    on Displayed do { assert false, "message displayed after every client caught up"; }
  }

  fun Settle() {
    var c: int;
    if (sequenced < msgs || sizeof(shown) < clients) { return; }
    foreach (c in keys(shown)) {
      if (shown[c] < msgs) { return; }
    }
    goto Done;
  }
}
