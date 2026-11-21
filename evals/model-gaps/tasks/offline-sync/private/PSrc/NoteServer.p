// The notes server. One conditional write on the note's row, with the request id
// recorded in the same transaction:
//   update(req, base, content): req already applied -> the version it became;
//                               base is current     -> version + 1;
//                               otherwise           -> conflict with the current copy.

machine NoteServer {
  var note: Note;
  // request id -> the version it became; makes a resent request idempotent
  var applied: map[int, int];

  start state Serving {
    entry (initial: Note) {
      note = initial;
    }
    on Update do (u: UpdateRequest) {
      // A known request is answered as before, whatever its base is by now.
      if (u.req in applied) { Replay(u); return; }
      if (u.base != note.version) { Refuse(u); return; }
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

  fun Replay(u: UpdateRequest) {
    announce Replayed, (client = u.clientId, req = u.req, base = u.base, version = applied[u.req]);
    send u.client, UpdateOk, (req = u.req, version = applied[u.req]);
  }

  fun Refuse(u: UpdateRequest) {
    announce Conflicted, (client = u.clientId, req = u.req, base = u.base, current = note.version);
    send u.client, UpdateConflict, (req = u.req, server = note);
  }
}
