// The sync contract, stated over events the server announces when it commits or
// refuses an update, and events the clients and the driver announce. Note content is
// modeled as the set of edit ids it contains, so "an edit was lost" is checkable: a
// later version that does not contain it, without the user having seen it.

type Note = (version: int, content: set[int], deleted: bool);

// Server, at its commit point: update `req` from `client`, based on `base`, became
// version `version` with this content.
event Accepted: (client: int, req: int, base: int, version: int, content: set[int], deleted: bool);
// Server, when it answers a request it already applied with the version it produced.
event Replayed: (client: int, req: int, base: int, version: int);
// Server, when it refuses an update because its base is not the current version.
event Conflicted: (client: int, req: int, base: int, current: int);

// Driver: the note every client starts from, and how many clients will sync.
event SyncPlanned: (clients: int, initial: Note);
// Driver: what each client's user did offline. `edit` names the change.
event OpPlanned: (client: int, edit: int, isDelete: bool);
// Client: its user's change is resolved. applied=false only for a delete the client
// dropped because the note changed after the user last saw it.
event OpDone: (client: int, edit: int, applied: bool, version: int);
// Test network: this client's update timed out, so it will resend once. The reply
// may be lost or only late.
event RequestTimedOut: (client: int);
// Client: its local copy after a final pull.
event Synced: (client: int, note: Note);

// Safety: no edit is silently lost, and conflicts are real.
// Each accepted update must be based on the current version, so its author saw every
// earlier edit; an edit's content must keep everything in the version it replaces. A
// delete supersedes the content explicitly, and only the version its user saw. A
// conflict is legitimate only when the base really is stale, and never for a request
// the server already applied (a retried request must not conflict with itself).
spec NoLostUpdates observes SyncPlanned, Accepted, Conflicted, Replayed {
  var current: Note;
  // request -> the version it produced
  var applied: map[int, int];

  start state Init {
    on SyncPlanned do (p: (clients: int, initial: Note)) {
      current = p.initial;
      goto Watching;
    }
    on Accepted do { assert false, "update accepted before the note existed"; }
    on Conflicted do { assert false, "conflict before the note existed"; }
    on Replayed do { assert false, "replay before the note existed"; }
  }

  state Watching {
    on SyncPlanned do { assert false, "sync planned twice"; }
    on Accepted do (a: (client: int, req: int, base: int, version: int, content: set[int], deleted: bool)) {
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
      applied[a.req] = a.version;
      current = (version = a.version, content = a.content, deleted = a.deleted);
    }
    on Replayed do (r: (client: int, req: int, base: int, version: int)) {
      assert r.req in applied, format("request {0} replayed but never applied", r.req);
      assert r.version == applied[r.req],
        format("request {0} replayed as version {1}, it produced version {2}", r.req, r.version, applied[r.req]);
    }
    on Conflicted do (c: (client: int, req: int, base: int, current: int)) {
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
// the user edited, and every later push either resends after a timeout or moves to a
// newer version a conflict reported to it. A reply can arrive after its timeout, so a
// client may see several conflicts; a push on a version it already pushed, or on an
// older one, means it took a late reply as fresh.
spec SyncCompletes observes SyncPlanned, OpPlanned, Accepted, Conflicted, Replayed, OpDone, Synced, RequestTimedOut {
  var current: Note;
  var clients: int;
  var initialVersion: int;
  // edit -> what the user did, removed when resolved
  var open: map[int, (client: int, isDelete: bool)];
  // client -> versions conflicts reported to it
  var reported: map[int, set[int]];
  // client -> the base of its last push
  var lastBase: map[int, int];
  // client -> timeouts not yet answered by a resend
  var resends: map[int, int];
  // client -> its accepted update
  var accepted: map[int, Note];
  var synced: set[int];

  start state Init {
    on SyncPlanned do (p: (clients: int, initial: Note)) {
      assert p.clients > 0, "planned no clients";
      clients = p.clients;
      current = p.initial;
      initialVersion = p.initial.version;
      goto Syncing;
    }
    on OpPlanned do { assert false, "op planned before the sync"; }
    on Accepted do { assert false, "update accepted before the sync"; }
    on Conflicted do { assert false, "conflict before the sync"; }
    on OpDone do { assert false, "op done before the sync"; }
    on Synced do { assert false, "pull before the sync"; }
    on RequestTimedOut do { assert false, "timeout before the sync"; }
    on Replayed do { assert false, "replay before the sync"; }
  }

  hot state Syncing {
    on SyncPlanned do { assert false, "sync planned twice"; }
    on OpPlanned do (p: (client: int, edit: int, isDelete: bool)) {
      assert !(p.edit in open), format("edit {0} planned twice", p.edit);
      open[p.edit] = (client = p.client, isDelete = p.isDelete);
      reported[p.client] = default(set[int]);
      resends[p.client] = 0;
    }
    on Accepted do (a: (client: int, req: int, base: int, version: int, content: set[int], deleted: bool)) {
      CheckBase(a.client, a.base);
      current = (version = a.version, content = a.content, deleted = a.deleted);
      accepted[a.client] = current;
    }
    on Conflicted do (c: (client: int, req: int, base: int, current: int)) {
      CheckBase(c.client, c.base);
      reported[c.client] += (c.current);
    }
    on Replayed do (r: (client: int, req: int, base: int, version: int)) {
      CheckBase(r.client, r.base);
    }
    on RequestTimedOut do (t: (client: int)) {
      resends[t.client] = resends[t.client] + 1;
    }
    on OpDone do (d: (client: int, edit: int, applied: bool, version: int)) {
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
        assert sizeof(reported[d.client]) > 0, format("client {0} dropped its delete without a conflict", d.client);
        assert !(d.client in accepted), format("client {0} dropped a delete that was applied", d.client);
        // Dropping the delete means the user now sees the copy a conflict returned.
        assert d.version in reported[d.client],
          format("client {0} dropped its delete but kept version {1}, which no conflict sent it",
                 d.client, d.version);
      }
      open -= (d.edit);
    }
    on Synced do (s: (client: int, note: Note)) {
      assert sizeof(open) == 0, format("client {0} pulled before every change was resolved", s.client);
      assert !(s.client in synced), format("client {0} pulled twice", s.client);
      // The pull follows the resend on the same path, so the server has answered it.
      assert resends[s.client] == 0,
        format("client {0} pulled, but the server never reported its resend after a timeout", s.client);
      assert s.note == current,
        format("client {0} converged to version {1}, the server holds version {2}",
               s.client, s.note.version, current.version);
      synced += (s.client);
      if (sizeof(synced) == clients) { goto Converged; }
    }
  }

  state Converged {
    on SyncPlanned do { assert false, "sync planned twice"; }
    on OpPlanned do { assert false, "op planned after convergence"; }
    on Accepted do { assert false, "update accepted after every client converged"; }
    on Conflicted do { assert false, "conflict after every client converged"; }
    on OpDone do { assert false, "op done after every client converged"; }
    on Synced do { assert false, "pull after every client converged"; }
    on RequestTimedOut do { assert false, "timeout after every client converged"; }
    on Replayed do { assert false, "replay after every client converged"; }
  }

  fun CheckBase(client: int, base: int) {
    assert client in reported, format("client {0} pushed without a planned change", client);
    if (!(client in lastBase)) {
      // Every user edits the copy they synced last: the initial note.
      assert base == initialVersion,
        format("client {0} first pushed base {1}, but its user edited version {2}", client, base, initialVersion);
    } else if (base == lastBase[client] && resends[client] > 0) {
      // The resend after a timeout.
      resends[client] = resends[client] - 1;
    } else {
      assert base in reported[client],
        format("client {0} pushed base {1}, which no conflict reported to it", client, base);
      assert base > lastBase[client],
        format("client {0} pushed base {1}, not newer than its last push on base {2}: it took a late reply as fresh",
               client, base, lastBase[client]);
    }
    lastBase[client] = base;
  }
}
