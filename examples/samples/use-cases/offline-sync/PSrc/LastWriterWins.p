// Negative controls, kept as separate machines so their code is not part of the
// passing tests' coverage.

// Last writer wins: the server ignores the base version and overwrites whatever is
// there. Two offline edits of the same base both "succeed" and the first is lost.
machine LastWriterWinsServer {
  var note: Note;
  var applied: map[int, int];

  start state Serving {
    entry (initial: Note) {
      note = initial;
    }
    on Update do (u: UpdateRequest) {
      if (u.req in applied) {
        announce Replayed, (client = u.clientId, req = u.req, base = u.base, version = applied[u.req]);
        send u.client, UpdateOk, (req = u.req, version = applied[u.req]);
        return;
      }
      // BUG: no check of u.base against note.version.
      note = (version = note.version + 1, content = u.content, deleted = u.deleted);
      applied[u.req] = note.version;
      announce Accepted, (client = u.clientId, req = u.req, base = u.base, version = note.version,
                           content = note.content, deleted = note.deleted);
      send u.client, UpdateOk, (req = u.req, version = note.version);
    }
    on Fetch do (c: machine) {
      send c, FetchResp, note;
    }
  }
}

// No request ids: a client whose reply was lost resends the same update, and the
// server, seeing a base one version behind its own write, reports a conflict.
machine ForgetfulServer {
  var note: Note;

  start state Serving {
    entry (initial: Note) {
      note = initial;
    }
    on Update do (u: UpdateRequest) {
      // BUG: no record of applied requests.
      if (u.base != note.version) {
        announce Conflicted, (client = u.clientId, req = u.req, base = u.base, current = note.version);
        send u.client, UpdateConflict, (req = u.req, server = note);
        return;
      }
      note = (version = note.version + 1, content = u.content, deleted = u.deleted);
      announce Accepted, (client = u.clientId, req = u.req, base = u.base, version = note.version,
                           content = note.content, deleted = note.deleted);
      send u.client, UpdateOk, (req = u.req, version = note.version);
    }
    on Fetch do (c: machine) {
      send c, FetchResp, note;
    }
  }
}

// Treats every conflict as fresh: a late conflict for an earlier attempt, arriving
// after the merged retry was sent, makes it merge and push again on that old version.
// Otherwise the same as SyncClient.
machine NaiveSyncClient {
  var owner: machine;
  var server: machine;
  var id: int;
  var note: Note;
  var edit: int;
  var base: int;
  var content: set[int];

  start state Offline {
    entry (input: ClientInit) {
      owner = input.owner;
      server = input.server;
      id = input.id;
      note = input.note;
      edit = input.edit;
      base = note.version;
      content = note.content;
      content += (edit);
      Push();
      goto Pushing;
    }
  }

  state Pushing {
    on UpdateOk do (r: (req: int, version: int)) {
      note = (version = r.version, content = content, deleted = false);
      goto Idle;
    }
    on UpdateConflict do (r: (req: int, server: Note)) {
      var e: int;
      // BUG: no check that the conflict is newer than this attempt's base.
      foreach (e in r.server.content) {
        content += (e);
      }
      base = r.server.version;
      Push();
    }
    on SyncTimeout do {
      Push();
    }
  }

  state Idle {
    entry {
      announce OpDone, (client = id, edit = edit, applied = true, version = note.version);
      send owner, ClientIdle;
    }
    ignore UpdateOk, UpdateConflict;
    on Pull do {
      send server, Fetch, this;
    }
    on FetchResp do (n: Note) {
      note = n;
      announce Synced, (client = id, note = note);
    }
  }

  fun Push() {
    send server, Update, (client = this, clientId = id, req = edit, base = base,
                           content = content, deleted = false);
  }
}
