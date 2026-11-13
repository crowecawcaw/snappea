// Negative controls, kept as separate machines so their code is not part of the
// passing tests' coverage.

// Last writer wins: the server ignores the base version and overwrites whatever is
// there. Two offline edits of the same base both "succeed" and the first is lost.
machine LastWriterWinsServer {
  var note: tNote;
  var applied: map[int, int];

  start state Serving {
    entry (initial: tNote) {
      note = initial;
    }
    on eUpdate do (u: tUpdate) {
      if (u.req in applied) {
        send u.client, eUpdateOk, (req = u.req, version = applied[u.req]);
        return;
      }
      // BUG: no check of u.base against note.version.
      note = (version = note.version + 1, content = u.content, deleted = u.deleted);
      applied[u.req] = note.version;
      announce eAccepted, (client = u.clientId, req = u.req, base = u.base, version = note.version,
                           content = note.content, deleted = note.deleted);
      send u.client, eUpdateOk, (req = u.req, version = note.version);
    }
    on eFetch do (c: machine) {
      send c, eFetchResp, note;
    }
  }
}

// No request ids: a client whose reply was lost resends the same update, and the
// server, seeing a base one version behind its own write, reports a conflict.
machine ForgetfulServer {
  var note: tNote;

  start state Serving {
    entry (initial: tNote) {
      note = initial;
    }
    on eUpdate do (u: tUpdate) {
      // BUG: no record of applied requests.
      if (u.base != note.version) {
        announce eConflicted, (client = u.clientId, req = u.req, base = u.base, current = note.version);
        send u.client, eUpdateConflict, (req = u.req, server = note);
        return;
      }
      note = (version = note.version + 1, content = u.content, deleted = u.deleted);
      announce eAccepted, (client = u.clientId, req = u.req, base = u.base, version = note.version,
                           content = note.content, deleted = note.deleted);
      send u.client, eUpdateOk, (req = u.req, version = note.version);
    }
    on eFetch do (c: machine) {
      send c, eFetchResp, note;
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
  var note: tNote;
  var edit: int;
  var base: int;
  var content: set[int];

  start state Offline {
    entry (input: tClientInit) {
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
    on eUpdateOk do (r: (req: int, version: int)) {
      note = (version = r.version, content = content, deleted = false);
      goto Idle;
    }
    on eUpdateConflict do (r: (req: int, server: tNote)) {
      var e: int;
      // BUG: no check that the conflict is newer than this attempt's base.
      foreach (e in r.server.content) {
        content += (e);
      }
      base = r.server.version;
      Push();
    }
    on eSyncTimeout do {
      Push();
    }
  }

  state Idle {
    entry {
      announce eOpDone, (client = id, edit = edit, applied = true, version = note.version);
      send owner, eClientIdle;
    }
    ignore eUpdateOk, eUpdateConflict;
    on ePull do {
      send server, eFetch, this;
    }
    on eFetchResp do (n: tNote) {
      note = n;
      announce eSynced, (client = id, note = note);
    }
  }

  fun Push() {
    send server, eUpdate, (client = this, clientId = id, req = edit, base = base,
                           content = content, deleted = false);
  }
}
