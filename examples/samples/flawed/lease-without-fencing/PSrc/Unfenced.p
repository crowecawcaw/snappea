// THE FLAW. The node checks that its lease is still valid, then writes. Storage
// accepts any write. If the node pauses after the check, its lease can expire and a
// new leader can be granted and write; when the old leader resumes, it still
// believes the check it made, and its write overwrites the new leader's.
//
// Real primitives: storage write (accept and store).

event Write: (node: machine, id: int, value: int);

machine UnfencedStorage {
  var value: int;
  start state Serving {
    on Write do (w: (node: machine, id: int, value: int)) {
      value = w.value;
      announce WriteAccepted, (node = w.id, value = value);
      send w.node, WriteResp, (ok = true,);
    }
  }
}

machine UnfencedNode {
  var config: NodeConfig;
  var token: int;
  var written: int;

  start state Acquiring {
    entry (c: NodeConfig) {
      config = c;
      send config.locks, Acquire, (node = this, id = config.id);
    }
    on Granted do (g: (token: int)) {
      token = g.token;
      goto Writing;
    }
  }

  state Writing {
    entry {
      // The lease check passes here (the lease is fresh) ... then the node pauses.
      send this, Resume;
    }
    on Resume do {
      // BUG: nothing stops this write if the lease expired during the pause.
      send config.storage, Write, (node = this, id = config.id, value = config.plan[written]);
    }
    on WriteResp do {
      written = written + 1;
      if (written < sizeof(config.plan)) { goto Writing; }
      send config.locks, Release, (token = token,);
      announce NodeDone, (node = config.id, fenced = false);
    }
  }
}
