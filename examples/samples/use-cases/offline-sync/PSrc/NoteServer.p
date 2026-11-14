// A notes server that accepts an update only when it is based on the current version.
//
// Real atomic primitive (one conditional write on the note's row, plus the request id
// recorded in the same transaction):
//   update(req, base, content): if req already applied -> its recorded version;
//                               else if base == version -> version + 1;
//                               else conflict with the current copy.
// Everything else (the client's offline edit, the merge, the retry) interleaves.

type Update = (client: machine, clientId: int, req: int, base: int, content: set[int], deleted: bool);

event Update: Update;
event UpdateOk: (req: int, version: int);
event UpdateConflict: (req: int, server: Note);
event Fetch: machine;
event FetchResp: Note;

machine NoteServer {
  var note: Note;
  // request id -> version it produced; makes a retried request idempotent
  var applied: map[int, int];

  start state Serving {
    entry (initial: Note) {
      note = initial;
    }
    on Update do (u: Update) {
      if (u.req in applied) {
        announce Replayed, (client = u.clientId, req = u.req, base = u.base, version = applied[u.req]);
        // The client lost our reply and retried: answer as before, change nothing.
        send u.client, UpdateOk, (req = u.req, version = applied[u.req]);
        return;
      }
      if (u.base != note.version) {
        announce Conflicted, (client = u.clientId, req = u.req, base = u.base, current = note.version);
        send u.client, UpdateConflict, (req = u.req, server = note);
        return;
      }
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
