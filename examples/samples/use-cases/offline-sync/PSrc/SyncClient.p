// The mobile client. Its user made one change offline (an edit or a delete) on top of
// the copy it last synced. When it comes online it pushes the change with that base
// version. On a conflict it resolves against the server copy and retries:
//   - an edit merges: keep everything in the server copy and the user's own content,
//     based on the server's version (this also restores a note deleted meanwhile, so
//     the user's work is kept);
//   - a delete is dropped: the note changed since the user last saw it, and deleting
//     content the user never saw would lose it.
// Every attempt carries the edit's id, so a retry after a lost reply is recognised.

event ePull;
// The client's request timed out (its reply was lost); it resends the same request.
event eSyncTimeout;
// Tells the driver this client has resolved its change and stopped editing.
event eClientIdle;

type tClientInit = (owner: machine, server: machine, id: int, note: tNote, edit: int, isDelete: bool);

machine SyncClient {
  var owner: machine;
  var server: machine;
  var id: int;
  // the local copy the user last saw
  var note: tNote;
  var edit: int;
  var isDelete: bool;
  // the attempt in flight
  var base: int;
  var content: set[int];

  start state Offline {
    entry (input: tClientInit) {
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
    on eUpdateOk do (r: (req: int, version: int)) {
      note = (version = r.version, content = content, deleted = isDelete);
      goto Idle, true;
    }
    on eUpdateConflict do (r: (req: int, server: tNote)) {
      var e: int;
      if (isDelete) {
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
    on eSyncTimeout do {
      Push();
    }
  }

  state Idle {
    entry (applied: bool) {
      announce eOpDone, (client = id, edit = edit, applied = applied, version = note.version);
      send owner, eClientIdle;
    }
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
                           content = content, deleted = isDelete);
  }
}
