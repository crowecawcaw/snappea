// Drivers, a scenario, and the test cases. Clients run the sync algorithm and are part
// of the design under test; the machines here only set up the run and announce intent.

// What the network does to the reply to a client's first update.
//   Reliable:  delivers it.
//   LoseReply: drops it; the client times out and resends.
//   SlowReply: delivers it late; the client times out, resends, and gets both replies.
enum tFault { Reliable, LoseReply, SlowReply }

// One user's offline change. `edit` names it; client `client` makes it.
type tOp = (client: int, edit: int, isDelete: bool, fault: tFault);

fun InitialNote(): tNote {
  var c: set[int];
  c += (0);
  return (version = 1, content = c, deleted = false);
}

// The network between one client and the server. It forwards requests and replies, and
// applies its fault to the first update reply: the client then sees a timeout, and the
// reply is lost or handed to a Courier that delivers it later.
machine LossyLink {
  var server: machine;
  var client: machine;
  var clientId: int;
  var fault: tFault;

  start state Forwarding {
    entry (input: (server: machine, fault: tFault)) {
      server = input.server;
      fault = input.fault;
    }
    on eUpdate do (u: tUpdate) {
      client = u.client;
      clientId = u.clientId;
      send server, eUpdate, (client = this, clientId = u.clientId, req = u.req, base = u.base,
                             content = u.content, deleted = u.deleted);
    }
    on eUpdateOk do (r: (req: int, version: int)) {
      if (TimedOut()) {
        if (fault == SlowReply) { new Courier((to = client, ok = true, okReply = r, conflictReply = default((req: int, server: tNote)))); }
        fault = Reliable;
        return;
      }
      send client, eUpdateOk, r;
    }
    on eUpdateConflict do (r: (req: int, server: tNote)) {
      if (TimedOut()) {
        if (fault == SlowReply) { new Courier((to = client, ok = false, okReply = default((req: int, version: int)), conflictReply = r)); }
        fault = Reliable;
        return;
      }
      send client, eUpdateConflict, r;
    }
    on eFetch do (c: machine) {
      client = c;
      send server, eFetch, this;
    }
    on eFetchResp do (n: tNote) {
      send client, eFetchResp, n;
    }
  }

  fun TimedOut(): bool {
    if (fault == Reliable) { return false; }
    announce eTimedOut, (client = clientId,);
    send client, eSyncTimeout;
    return true;
  }
}

// Delivers one late reply. A separate sender, so the reply can arrive at any point
// after the timeout, before or after anything the link sends later.
machine Courier {
  start state Carrying {
    entry (d: (to: machine, ok: bool, okReply: (req: int, version: int), conflictReply: (req: int, server: tNote))) {
      if (d.ok) {
        send d.to, eUpdateOk, d.okReply;
      } else {
        send d.to, eUpdateConflict, d.conflictReply;
      }
    }
  }
}

// Starts one client per op, all from the same synced copy. Once every client has
// resolved its change and stopped editing, asks each to pull the server's copy.
machine SyncDriver {
  var clients: seq[SyncClient];
  var idle: int;

  start state Running {
    entry (input: (server: machine, ops: seq[tOp])) {
      var op: tOp;
      var initial: tNote;
      var server: machine;
      initial = InitialNote();
      announce eSyncPlanned, (clients = sizeof(input.ops), initial = initial);
      foreach (op in input.ops) {
        announce eOpPlanned, (client = op.client, edit = op.edit, isDelete = op.isDelete);
        // Only a client with a faulty reply talks through a link, which keeps the other
        // schedules small.
        server = input.server;
        if (op.fault != Reliable) {
          server = new LossyLink((server = input.server, fault = op.fault));
        }
        clients += (sizeof(clients), new SyncClient((owner = this, server = server, id = op.client,
          note = initial, edit = op.edit, isDelete = op.isDelete)));
      }
    }
    on eClientIdle do {
      var c: SyncClient;
      idle = idle + 1;
      if (idle == sizeof(clients)) {
        foreach (c in clients) {
          send c, ePull;
        }
      }
    }
  }
}

fun Ops2(a: tOp, b: tOp): seq[tOp] {
  var ops: seq[tOp];
  ops += (0, a);
  ops += (1, b);
  return ops;
}

// Both users edited version 1 offline. Whoever syncs second gets a conflict, merges,
// and retries; no order may lose either edit.
machine TwoEditorsSameBase {
  start state Init {
    entry {
      new SyncDriver((server = new NoteServer(InitialNote()), ops = Ops2(
        (client = 1, edit = 11, isDelete = false, fault = Reliable),
        (client = 2, edit = 21, isDelete = false, fault = Reliable))));
    }
  }
}

// Client 1's reply is lost and it resends the same request, while client 2 may write
// in between. The retry must be recognised by its id, not judged by its now-stale base.
machine LostReplyRetry {
  start state Init {
    entry {
      new SyncDriver((server = new NoteServer(InitialNote()), ops = Ops2(
        (client = 1, edit = 11, isDelete = false, fault = LoseReply),
        (client = 2, edit = 21, isDelete = false, fault = Reliable))));
    }
  }
}

// One user deletes the note, the other edits it, both from version 1. If the edit
// lands first the delete is dropped; if the delete lands first the edit restores the
// note with the user's content.
machine EditVsDelete {
  start state Init {
    entry {
      new SyncDriver((server = new NoteServer(InitialNote()), ops = Ops2(
        (client = 1, edit = 11, isDelete = true, fault = Reliable),
        (client = 2, edit = 21, isDelete = false, fault = Reliable))));
    }
  }
}

// Client 1's first reply is slow: it times out and resends, and then gets both
// replies, in either order, possibly after its merged retry. Client 2 may write before,
// between or after. The client must act only on replies that are news to it.
machine SlowReply {
  start state Init {
    entry {
      new SyncDriver((server = new NoteServer(InitialNote()), ops = Ops2(
        (client = 1, edit = 11, isDelete = false, fault = SlowReply),
        (client = 2, edit = 21, isDelete = false, fault = Reliable))));
    }
  }
}

// Three users, each makes an edit or a delete, and any of them may lose a reply or
// get it late.
machine GeneratedSync {
  start state Init {
    entry {
      var ops: seq[tOp];
      var i: int;
      var fault: tFault;
      while (i < 3) {
        fault = Reliable;
        if ($) { fault = LoseReply; } else if ($) { fault = SlowReply; }
        ops += (i, (client = i + 1, edit = (i + 1) * 10 + 1, isDelete = $, fault = fault));
        i = i + 1;
      }
      new SyncDriver((server = new NoteServer(InitialNote()), ops = ops));
    }
  }
}

machine BuggyLastWriterWins {
  start state Init {
    entry {
      new SyncDriver((server = new LastWriterWinsServer(InitialNote()), ops = Ops2(
        (client = 1, edit = 11, isDelete = false, fault = Reliable),
        (client = 2, edit = 21, isDelete = false, fault = Reliable))));
    }
  }
}

machine BuggyNoRequestIds {
  start state Init {
    entry {
      var ops: seq[tOp];
      ops += (0, (client = 1, edit = 11, isDelete = false, fault = LoseReply));
      new SyncDriver((server = new ForgetfulServer(InitialNote()), ops = ops));
    }
  }
}

// The SlowReply scenario with a client that treats every conflict as fresh. It is its
// own driver, so SyncDriver stays free of branches only this test would take.
machine BuggyStaleReply {
  var idle: int;
  var clients: seq[machine];
  start state Running {
    entry {
      var initial: tNote;
      var server: machine;
      initial = InitialNote();
      server = new NoteServer(initial);
      announce eSyncPlanned, (clients = 2, initial = initial);
      announce eOpPlanned, (client = 1, edit = 11, isDelete = false);
      announce eOpPlanned, (client = 2, edit = 21, isDelete = false);
      clients += (0, new NaiveSyncClient((owner = this, server = new LossyLink((server = server, fault = SlowReply)),
        id = 1, note = initial, edit = 11, isDelete = false)));
      clients += (1, new SyncClient((owner = this, server = server, id = 2, note = initial, edit = 21, isDelete = false)));
    }
    on eClientIdle do {
      var c: machine;
      idle = idle + 1;
      if (idle == 2) {
        foreach (c in clients) { send c, ePull; }
      }
    }
  }
}

// Covered: a delete landed first and a concurrent edit brought the note back.
scenario EditRestoresDeletedNote observes eAccepted {
  start hot state WaitDelete {
    on eAccepted do (a: (client: int, req: int, base: int, version: int, content: set[int], deleted: bool)) {
      if (a.deleted) { goto WaitRestore; }
    }
  }
  hot state WaitRestore {
    on eAccepted do (a: (client: int, req: int, base: int, version: int, content: set[int], deleted: bool)) {
      if (!a.deleted) { goto Covered; }
    }
  }
  cold state Covered {
    ignore eAccepted;
  }
}

test tcTwoEditorsSameBase [main=TwoEditorsSameBase]:
  assert NoLostUpdates, SyncCompletes in { TwoEditorsSameBase, SyncDriver, LossyLink, Courier, NoteServer, SyncClient };

test tcLostReplyRetry [main=LostReplyRetry]:
  assert NoLostUpdates, SyncCompletes in { LostReplyRetry, SyncDriver, LossyLink, Courier, NoteServer, SyncClient };

test tcSlowReply [main=SlowReply]:
  assert NoLostUpdates, SyncCompletes in { SlowReply, SyncDriver, LossyLink, Courier, NoteServer, SyncClient };

test tcEditVsDelete [main=EditVsDelete]:
  assert NoLostUpdates, SyncCompletes in { EditVsDelete, SyncDriver, LossyLink, Courier, NoteServer, SyncClient };

test tcGeneratedSync [main=GeneratedSync]:
  assert NoLostUpdates, SyncCompletes in { GeneratedSync, SyncDriver, LossyLink, Courier, NoteServer, SyncClient };

// expect-bug: based on version 1 overwrote version 2
test tcBugLastWriterWins [main=BuggyLastWriterWins]:
  assert NoLostUpdates, SyncCompletes in { BuggyLastWriterWins, SyncDriver, LossyLink, LastWriterWinsServer, SyncClient };

// expect-bug: conflicted with itself
test tcBugNoRequestIds [main=BuggyNoRequestIds]:
  assert NoLostUpdates, SyncCompletes in { BuggyNoRequestIds, SyncDriver, LossyLink, ForgetfulServer, SyncClient };

// expect-bug: took a late reply as fresh
test tcBugStaleReply [main=BuggyStaleReply]:
  assert NoLostUpdates, SyncCompletes in { BuggyStaleReply, LossyLink, Courier, NoteServer, SyncClient, NaiveSyncClient };
