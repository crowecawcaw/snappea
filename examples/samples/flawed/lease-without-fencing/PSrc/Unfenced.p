// THE FLAW. The node checks that its lease is still valid, then writes. Storage
// accepts any write. If the node pauses after the check, its lease can expire and a
// new leader can be granted and write; when the old leader resumes, it still
// believes the check it made, and its write overwrites the new leader's.
//
// Real primitives: storage write (accept and store).

event eWrite: (node: machine, id: int, value: int);

machine UnfencedStorage {
  var value: int;
  start state Serving {
    on eWrite do (w: (node: machine, id: int, value: int)) {
      value = w.value;
      announce eWriteAccepted, (node = w.id, value = value);
      send w.node, eWriteResp, (ok = true,);
    }
  }
}

machine UnfencedNode {
  var config: tNodeConfig;
  var token: int;
  var written: int;

  start state Acquiring {
    entry (c: tNodeConfig) {
      config = c;
      send config.locks, eAcquire, (node = this, id = config.id);
    }
    on eGranted do (g: (token: int)) {
      token = g.token;
      goto Writing;
    }
  }

  state Writing {
    entry {
      // The lease check passes here (the lease is fresh) ... then the node pauses.
      send this, eResume;
    }
    on eResume do {
      // BUG: nothing stops this write if the lease expired during the pause.
      send config.storage, eWrite, (node = this, id = config.id, value = config.plan[written]);
    }
    on eWriteResp do {
      written = written + 1;
      if (written < sizeof(config.plan)) { goto Writing; }
      send config.locks, eRelease, (token = token,);
      announce eNodeDone, (node = config.id, fenced = false);
    }
  }
}
