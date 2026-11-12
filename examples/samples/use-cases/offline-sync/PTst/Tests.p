// Drivers, a scenario, and the test cases. Clients run the sync algorithm and are part
// of the design under test; the machines here only set up the run and announce intent.

// One user's offline change. `edit` names it; client `client` makes it.
type tOp = (client: int, edit: int, isDelete: bool, losesFirstReply: bool);

fun InitialNote(): tNote {
  var c: set[int];
  c += (0);
  return (version = 1, content = c, deleted = false);
}

// The network between one client and the server. It forwards requests and replies, and
// when asked, loses the first update reply: the client then sees a timeout instead.
machine LossyLink {
  var server: machine;
  var client: machine;
  var clientId: int;
  var loseNext: bool;

  start state Forwarding {
    entry (input: (server: machine, loseFirstReply: bool)) {
      server = input.server;
      loseNext = input.loseFirstReply;
    }
    on eUpdate do (u: tUpdate) {
      client = u.client;
      clientId = u.clientId;
      send server, eUpdate, (client = this, clientId = u.clientId, req = u.req, base = u.base,
                             content = u.content, deleted = u.deleted);
    }
    on eUpdateOk do (r: (req: int, version: int)) {
      if (Lose()) { return; }
      send client, eUpdateOk, r;
    }
    on eUpdateConflict do (r: (req: int, server: tNote)) {
      if (Lose()) { return; }
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

  fun Lose(): bool {
    if (loseNext) {
      loseNext = false;
      announce eReplyLost, (client = clientId,);
      send client, eSyncTimeout;
      return true;
    }
    return false;
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
        // Only a client whose reply will be lost talks through a link, which keeps the
        // other schedules small.
        server = input.server;
        if (op.losesFirstReply) {
          server = new LossyLink((server = input.server, loseFirstReply = true));
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
        (client = 1, edit = 11, isDelete = false, losesFirstReply = false),
        (client = 2, edit = 21, isDelete = false, losesFirstReply = false))));
    }
  }
}

// Client 1's reply is lost and it resends the same request, while client 2 may write
// in between. The retry must be recognised by its id, not judged by its now-stale base.
machine LostReplyRetry {
  start state Init {
    entry {
      new SyncDriver((server = new NoteServer(InitialNote()), ops = Ops2(
        (client = 1, edit = 11, isDelete = false, losesFirstReply = true),
        (client = 2, edit = 21, isDelete = false, losesFirstReply = false))));
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
        (client = 1, edit = 11, isDelete = true, losesFirstReply = false),
        (client = 2, edit = 21, isDelete = false, losesFirstReply = false))));
    }
  }
}

// Three users, each makes an edit or a delete, and any of them may lose a reply.
machine GeneratedSync {
  start state Init {
    entry {
      var ops: seq[tOp];
      var i: int;
      while (i < 3) {
        ops += (i, (client = i + 1, edit = (i + 1) * 10 + 1, isDelete = $, losesFirstReply = $));
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
        (client = 1, edit = 11, isDelete = false, losesFirstReply = false),
        (client = 2, edit = 21, isDelete = false, losesFirstReply = false))));
    }
  }
}

machine BuggyNoRequestIds {
  start state Init {
    entry {
      var ops: seq[tOp];
      ops += (0, (client = 1, edit = 11, isDelete = false, losesFirstReply = true));
      new SyncDriver((server = new ForgetfulServer(InitialNote()), ops = ops));
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
  assert NoLostUpdates, SyncCompletes in { TwoEditorsSameBase, SyncDriver, LossyLink, NoteServer, SyncClient };

test tcLostReplyRetry [main=LostReplyRetry]:
  assert NoLostUpdates, SyncCompletes in { LostReplyRetry, SyncDriver, LossyLink, NoteServer, SyncClient };

test tcEditVsDelete [main=EditVsDelete]:
  assert NoLostUpdates, SyncCompletes in { EditVsDelete, SyncDriver, LossyLink, NoteServer, SyncClient };

test tcGeneratedSync [main=GeneratedSync]:
  assert NoLostUpdates, SyncCompletes in { GeneratedSync, SyncDriver, LossyLink, NoteServer, SyncClient };

// expect-bug: based on version 1 overwrote version 2
test tcBugLastWriterWins [main=BuggyLastWriterWins]:
  assert NoLostUpdates, SyncCompletes in { BuggyLastWriterWins, SyncDriver, LossyLink, LastWriterWinsServer, SyncClient };

// expect-bug: conflicted with itself
test tcBugNoRequestIds [main=BuggyNoRequestIds]:
  assert NoLostUpdates, SyncCompletes in { BuggyNoRequestIds, SyncDriver, LossyLink, ForgetfulServer, SyncClient };
