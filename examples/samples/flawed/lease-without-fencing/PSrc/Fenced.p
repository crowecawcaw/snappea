// THE FIX. Fencing tokens. Every write carries the token of the lease it was made
// under, and storage remembers the largest token it has seen. A write with a smaller
// token comes from a leader whose lease has been replaced, so storage rejects it and
// the node steps down. The check happens where the data is, so a pause anywhere in
// the node cannot get around it.
//
// Real primitives: storage write-if-token (one conditional write that compares the
// token with the stored maximum and updates both).

event FencedWrite: (node: machine, id: int, token: int, value: int);

machine FencedStorage {
  var value: int;
  var maxToken: int;
  start state Serving {
    on FencedWrite do (w: (node: machine, id: int, token: int, value: int)) {
      if (w.token < maxToken) {
        announce WriteRejected, (node = w.id, value = w.value);
        send w.node, WriteResp, (ok = false,);
        return;
      }
      maxToken = w.token;
      value = w.value;
      announce WriteAccepted, (node = w.id, value = value);
      send w.node, WriteResp, (ok = true,);
    }
  }
}

machine FencedNode {
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
      send this, Resume;
    }
    on Resume do {
      send config.storage, FencedWrite, (node = this, id = config.id, token = token, value = config.plan[written]);
    }
    on WriteResp do (r: (ok: bool)) {
      if (!r.ok) {
        // A newer leader has written: stop acting as leader.
        Stop(true);
        return;
      }
      written = written + 1;
      if (written < sizeof(config.plan)) { goto Writing; }
      // snappea-lint: allow mutation:remove-statement -- releasing only ends the lease before its timer would; the model has no time, so the two are indistinguishable
      send config.locks, Release, (token = token,);
      Stop(false);
    }
  }

  fun Stop(fenced: bool) {
    announce NodeDone, (node = config.id, fenced = fenced);
  }
}
