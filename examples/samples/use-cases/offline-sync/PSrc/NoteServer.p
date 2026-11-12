// A notes server that accepts an update only when it is based on the current version.
//
// Real atomic primitive (one conditional write on the note's row, plus the request id
// recorded in the same transaction):
//   update(req, base, content): if req already applied -> its recorded version;
//                               else if base == version -> version + 1;
//                               else conflict with the current copy.
// Everything else (the client's offline edit, the merge, the retry) interleaves.

type tUpdate = (client: machine, clientId: int, req: int, base: int, content: set[int], deleted: bool);

event eUpdate: tUpdate;
event eUpdateOk: (req: int, version: int);
event eUpdateConflict: (req: int, server: tNote);
event eFetch: machine;
event eFetchResp: tNote;

machine NoteServer {
  var note: tNote;
  // request id -> version it produced; makes a retried request idempotent
  var applied: map[int, int];

  start state Serving {
    entry (initial: tNote) {
      note = initial;
    }
    on eUpdate do (u: tUpdate) {
      if (u.req in applied) {
        // The client lost our reply and retried: answer as before, change nothing.
        send u.client, eUpdateOk, (req = u.req, version = applied[u.req]);
        return;
      }
      if (u.base != note.version) {
        announce eConflicted, (client = u.clientId, req = u.req, base = u.base, current = note.version);
        send u.client, eUpdateConflict, (req = u.req, server = note);
        return;
      }
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
