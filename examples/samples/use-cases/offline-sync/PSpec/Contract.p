// The sync contract, stated over events the server announces when it commits or
// refuses an update, and events the clients and the driver announce. Note content is
// modeled as the set of edit ids it contains, so "an edit was lost" is checkable: a
// later version that does not contain it, without the user having seen it.

type tNote = (version: int, content: set[int], deleted: bool);

// Server, at its commit point: update `req` from `client`, based on `base`, became
// version `version` with this content.
event eAccepted: (client: int, req: int, base: int, version: int, content: set[int], deleted: bool);
// Server, when it refuses an update because its base is not the current version.
event eConflicted: (client: int, req: int, base: int, current: int);

// Driver: the note every client starts from, and how many clients will sync.
event eSyncPlanned: (clients: int, initial: tNote);
// Driver: what each client's user did offline. `edit` names the change.
event eOpPlanned: (client: int, edit: int, isDelete: bool);
// Client: its user's change is resolved. applied=false only for a delete the client
// dropped because the note changed after the user last saw it.
event eOpDone: (client: int, edit: int, applied: bool, version: int);
// Test network: the reply to this client's last update was lost; it will resend.
event eReplyLost: (client: int);
// Client: its local copy after a final pull.
event eSynced: (client: int, note: tNote);

// Safety: no edit is silently lost, and conflicts are real.
// Each accepted update must be based on the current version, so its author saw every
// earlier edit; an edit's content must keep everything in the version it replaces. A
// delete supersedes the content explicitly, and only the version its user saw. A
// conflict is legitimate only when the base really is stale, and never for a request
// the server already applied (a retried request must not conflict with itself).
spec NoLostUpdates observes eSyncPlanned, eAccepted, eConflicted {
  var current: tNote;
  var applied: set[int];

  start state Init {
    // snappea-lint: allow unused-payload-field -- SyncCompletes checks the client count
    on eSyncPlanned do (p: (clients: int, initial: tNote)) {
      current = p.initial;
      goto Watching;
    }
    on eAccepted do { assert false, "update accepted before the note existed"; }
    on eConflicted do { assert false, "conflict before the note existed"; }
  }

  state Watching {
    on eSyncPlanned do { assert false, "sync planned twice"; }
    on eAccepted do (a: (client: int, req: int, base: int, version: int, content: set[int], deleted: bool)) {
      var e: int;
      assert !(a.req in applied), format("request {0} applied twice", a.req);
      assert a.base == current.version,
        format("update {0} from client {1} based on version {2} overwrote version {3}",
               a.req, a.client, a.base, current.version);
      assert a.version == current.version + 1,
        format("update {0} became version {1}, expected {2}", a.req, a.version, current.version + 1);
      if (a.deleted) {
        assert sizeof(a.content) == 0, format("delete {0} kept content", a.req);
      } else {
        foreach (e in current.content) {
          assert e in a.content,
            format("update {0} from client {1} lost edit {2}", a.req, a.client, e);
        }
      }
      applied += (a.req);
      current = (version = a.version, content = a.content, deleted = a.deleted);
    }
    on eConflicted do (c: (client: int, req: int, base: int, current: int)) {
      assert !(c.req in applied),
        format("request {0} conflicted with itself: it was already applied", c.req);
      assert c.base != current.version,
        format("update {0} from client {1} refused although its base {2} is current", c.req, c.client, c.base);
      assert c.current == current.version,
        format("conflict for update {0} reported version {1}, current is {2}", c.req, c.current, current.version);
    }
  }
}

// Progress: every user's change is resolved with the outcome its updates justify, and
// after every client stops editing and pulls, each holds exactly the server's note.
// It also pins down what each client sends: its first push is based on the version
// the user edited, and after a conflict it is based on the version that conflict
// reported. Only a resend after a lost reply may repeat a refused base.
spec SyncCompletes observes eSyncPlanned, eOpPlanned, eAccepted, eConflicted, eOpDone, eSynced, eReplyLost {
  var current: tNote;
  var clients: int;
  var initialVersion: int;
  // edit -> what the user did, removed when resolved
  var open: map[int, (client: int, isDelete: bool)];
  // client -> the base its next update must carry
  var expectedBase: map[int, int];
  // client -> the base of its last refused update, which a resend may repeat
  var refusedBase: map[int, int];
  // clients whose last reply was lost, so their next update may be a resend
  var resending: set[int];
  // client -> its accepted update
  var accepted: map[int, tNote];
  var synced: set[int];

  start state Init {
    on eSyncPlanned do (p: (clients: int, initial: tNote)) {
      assert p.clients > 0, "planned no clients";
      clients = p.clients;
      current = p.initial;
      initialVersion = p.initial.version;
      goto Syncing;
    }
    on eOpPlanned do { assert false, "op planned before the sync"; }
    on eAccepted do { assert false, "update accepted before the sync"; }
    on eConflicted do { assert false, "conflict before the sync"; }
    on eOpDone do { assert false, "op done before the sync"; }
    on eSynced do { assert false, "pull before the sync"; }
    on eReplyLost do { assert false, "reply lost before the sync"; }
  }

  hot state Syncing {
    on eSyncPlanned do { assert false, "sync planned twice"; }
    on eOpPlanned do (p: (client: int, edit: int, isDelete: bool)) {
      assert !(p.edit in open), format("edit {0} planned twice", p.edit);
      open[p.edit] = (client = p.client, isDelete = p.isDelete);
      // Every user edits the copy they synced last: the initial note.
      expectedBase[p.client] = initialVersion;
    }
    // snappea-lint: allow unused-payload-field -- NoLostUpdates checks request ids
    on eAccepted do (a: (client: int, req: int, base: int, version: int, content: set[int], deleted: bool)) {
      CheckBase(a.client, a.base);
      current = (version = a.version, content = a.content, deleted = a.deleted);
      accepted[a.client] = current;
    }
    // snappea-lint: allow unused-payload-field -- NoLostUpdates checks request ids
    on eConflicted do (c: (client: int, req: int, base: int, current: int)) {
      CheckBase(c.client, c.base);
      refusedBase[c.client] = c.base;
      expectedBase[c.client] = c.current;
    }
    on eReplyLost do (l: (client: int)) {
      resending += (l.client);
    }
    on eOpDone do (d: (client: int, edit: int, applied: bool, version: int)) {
      var mine: (client: int, isDelete: bool);
      assert d.edit in open, format("edit {0} resolved twice or never planned", d.edit);
      mine = open[d.edit];
      assert mine.client == d.client,
        format("client {0} resolved edit {1} of client {2}", d.client, d.edit, mine.client);
      if (d.applied) {
        assert d.client in accepted,
          format("client {0} reported edit {1} applied, but no update of it was accepted", d.client, d.edit);
        assert d.version == accepted[d.client].version,
          format("client {0} reported version {1}, its update became version {2}",
                 d.client, d.version, accepted[d.client].version);
        assert accepted[d.client].deleted == mine.isDelete,
          format("client {0} applied the wrong kind of change for edit {1}", d.client, d.edit);
        assert mine.isDelete || d.edit in accepted[d.client].content,
          format("client {0} applied an update without its edit {1}", d.client, d.edit);
      } else {
        assert mine.isDelete, format("client {0} dropped edit {1}", d.client, d.edit);
        assert d.client in refusedBase, format("client {0} dropped its delete without a conflict", d.client);
        assert !(d.client in accepted), format("client {0} dropped a delete that was applied", d.client);
        // Dropping the delete means the user now sees the copy the conflict returned.
        assert d.version == expectedBase[d.client],
          format("client {0} dropped its delete but kept version {1}, the server sent {2}",
                 d.client, d.version, expectedBase[d.client]);
      }
      open -= (d.edit);
    }
    on eSynced do (s: (client: int, note: tNote)) {
      assert sizeof(open) == 0, format("client {0} pulled before every change was resolved", s.client);
      assert !(s.client in synced), format("client {0} pulled twice", s.client);
      assert s.note == current,
        format("client {0} converged to version {1}, the server holds version {2}",
               s.client, s.note.version, current.version);
      synced += (s.client);
      if (sizeof(synced) == clients) { goto Converged; }
    }
  }

  state Converged {
    on eSyncPlanned do { assert false, "sync planned twice"; }
    on eOpPlanned do { assert false, "op planned after convergence"; }
    on eAccepted do { assert false, "update accepted after every client converged"; }
    on eConflicted do { assert false, "conflict after every client converged"; }
    on eOpDone do { assert false, "op done after every client converged"; }
    on eSynced do { assert false, "pull after every client converged"; }
    on eReplyLost do { assert false, "reply lost after every client converged"; }
  }

  fun CheckBase(client: int, base: int) {
    assert client in expectedBase, format("client {0} pushed without a planned change", client);
    if (client in resending && client in refusedBase && base == refusedBase[client]) {
      // The resend of a request whose conflict reply was lost.
      resending -= (client);
      return;
    }
    assert base == expectedBase[client],
      format("client {0} pushed base {1}, but the last version it saw is {2}", client, base, expectedBase[client]);
  }
}
