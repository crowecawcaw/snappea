// The mobile client: one offline change, pushed with the version the user edited.

machine SyncClient {
  var owner: machine;
  var server: machine;
  var id: int;
  // the local copy the user last saw
  var note: Note;
  var edit: int;
  var isDelete: bool;
  // the attempt in flight
  var base: int;
  var content: set[int];

  start state Offline {
    entry (input: ClientInit) {
      owner = input.owner;
      server = input.server;
      id = input.id;
      note = input.note;
      edit = input.edit;
      isDelete = input.isDelete;
      // The user's offline change, on top of the local copy.
      base = note.version;
      if (!isDelete) {
        content = note.content;
        content += (edit);
      }
      Push();
      goto Pushing;
    }
  }

  state Pushing {
    on UpdateOk do (r: (req: int, version: int)) {
      note = (version = r.version, content = content, deleted = isDelete);
      goto Idle, true;
    }
    on UpdateConflict do (r: (req: int, server: Note)) {
      var e: int;
      // A late reply to an earlier attempt: this attempt already uses its version.
      if (r.server.version <= base) { return; }
      if (isDelete) {
        // The note changed after the user last saw it: drop the delete, keep the copy.
        note = r.server;
        goto Idle, false;
      }
      // Merge: the server copy plus our own content, on top of the server's version.
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
    entry (applied: bool) {
      announce OpDone, (client = id, edit = edit, applied = applied, version = note.version);
      send owner, ClientIdle;
    }
    // Late replies to earlier attempts: the change is already resolved.
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
                           content = content, deleted = isDelete);
  }
}
