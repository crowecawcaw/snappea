// The sync contract (DESIGN.md), stated over the events the server and the clients
// announce (declared in PSrc/Sync.p) and the plan the test environment announces. Note
// content is the set of edit ids it contains, so "an edit was lost" is checkable.

// Safety: no edit is silently lost. Each accepted update must be based on the current
// version, so its author saw every earlier edit; an edit's content must keep
// everything in the version it replaces. A delete supersedes the content explicitly.
spec NoLostUpdates observes SyncPlanned, Accepted {
  var current: Note;

  start state Init {
    on SyncPlanned do (p: (clients: int, initial: Note)) {
      current = p.initial;
      goto Watching;
    }
    on Accepted do { assert false, "update accepted before the note existed"; }
  }

  state Watching {
    on SyncPlanned do { assert false, "sync planned twice"; }
    on Accepted do (a: (client: int, req: int, base: int, version: int, content: set[int], deleted: bool)) {
      var e: int;
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
      current = (version = a.version, content = a.content, deleted = a.deleted);
    }
  }
}

// Progress: every user's change is resolved, and after every client stops editing and
// pulls, each holds exactly the server's note.
spec SyncCompletes observes SyncPlanned, OpPlanned, Accepted, OpDone, Synced {
  var current: Note;
  var clients: int;
  // edit -> the client whose user made it, removed when resolved
  var open: map[int, int];
  var synced: set[int];

  start state Init {
    on SyncPlanned do (p: (clients: int, initial: Note)) {
      assert p.clients > 0, "planned no clients";
      clients = p.clients;
      current = p.initial;
      goto Syncing;
    }
    on OpPlanned do { assert false, "op planned before the sync"; }
    on Accepted do { assert false, "update accepted before the sync"; }
    on OpDone do { assert false, "op done before the sync"; }
    on Synced do { assert false, "pull before the sync"; }
  }

  hot state Syncing {
    on SyncPlanned do { assert false, "sync planned twice"; }
    on OpPlanned do (p: (client: int, edit: int, isDelete: bool)) {
      assert !(p.edit in open), format("edit {0} planned twice", p.edit);
      open[p.edit] = p.client;
    }
    on Accepted do (a: (client: int, req: int, base: int, version: int, content: set[int], deleted: bool)) {
      current = (version = a.version, content = a.content, deleted = a.deleted);
    }
    on OpDone do (d: (client: int, edit: int, applied: bool, version: int)) {
      assert d.edit in open, format("edit {0} resolved twice or never planned", d.edit);
      assert open[d.edit] == d.client,
        format("client {0} resolved edit {1} of client {2}", d.client, d.edit, open[d.edit]);
      open -= (d.edit);
    }
    on Synced do (s: (client: int, note: Note)) {
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
    on SyncPlanned do { assert false, "sync planned twice"; }
    on OpPlanned do { assert false, "op planned after convergence"; }
    on Accepted do { assert false, "update accepted after every client converged"; }
    on OpDone do { assert false, "op done after every client converged"; }
    on Synced do { assert false, "pull after every client converged"; }
  }
}
