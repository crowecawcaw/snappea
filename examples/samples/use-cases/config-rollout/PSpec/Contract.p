// The rollout contract, over events the config service announces when it issues a
// version and sends a push, and the nodes announce when they apply one. The specs
// rebuild per-node state from those events and never read the service's stage.
//
// Versions only go up. A rollback is issued as a new, higher version that carries the
// old content: nodes then need one rule ("apply only newer versions"), and a delayed
// push of the bad version can never be mistaken for the rollback.

// Driver: `nodes` nodes all run version `version` with `content`; node 0 is the
// canary; the operator will issue `issues` new versions.
event RolloutPlanned: (nodes: int, version: int, content: int, issues: int);
// Service: accepted a new version from the operator.
event Issued: (version: int, content: int);
// Service: sent a push of `version` to `node`.
event PushSent: (node: int, version: int);
// Node: applied `version` with `content`.
event Applied: (node: int, version: int, content: int);
// Node: started, running `version` with `content`.
event NodeStarted: (node: int, version: int, content: int);
// Service: every node acknowledged `version`; the rollout is complete.
event RolloutDone: (version: int);

// Safety: a node applies only versions newer than what it runs, with the content that
// version was issued with; the service pushes only the latest issued version (after a
// rollback nothing pushes the bad version again), and pushes a version beyond the
// canary only after the canary applied it. A node applies only what was pushed to it,
// and the service declares a rollout done only when every node runs that version.
spec RolloutSafety observes RolloutPlanned, NodeStarted, Issued, PushSent, Applied, RolloutDone {
  var running: map[int, int];
  var initial: (version: int, content: int);
  // (node, version) pairs pushed
  var pushed: set[(node: int, version: int)];
  var started: set[int];
  // version -> content it was issued with
  var issued: map[int, int];
  var latest: int;

  start state Init {
    // snappea-lint: allow unused-payload-field -- RolloutCompletes counts the issues
    on RolloutPlanned do (p: (nodes: int, version: int, content: int, issues: int)) {
      var n: int;
      while (n < p.nodes) {
        running[n] = p.version;
        n = n + 1;
      }
      issued[p.version] = p.content;
      latest = p.version;
      initial = (version = p.version, content = p.content);
      goto Watching;
    }
    on NodeStarted do { assert false, "node started before the rollout was planned"; }
    on Issued do { assert false, "version issued before the rollout was planned"; }
    on PushSent do { assert false, "push before the rollout was planned"; }
    on Applied do { assert false, "apply before the rollout was planned"; }
    on RolloutDone do { assert false, "rollout done before it was planned"; }
  }

  state Watching {
    on RolloutPlanned do { assert false, "rollout planned twice"; }
    on NodeStarted do (n: (node: int, version: int, content: int)) {
      assert n.node in running, format("unknown node {0} started", n.node);
      assert n.version == initial.version && n.content == initial.content,
        format("node {0} started with version {1} content {2}, the plan says version {3} content {4}",
               n.node, n.version, n.content, initial.version, initial.content);
      started += (n.node);
    }
    on RolloutDone do (d: (version: int)) {
      var n: int;
      assert d.version == latest, format("rollout of version {0} declared done, but {1} superseded it", d.version, latest);
      foreach (n in keys(running)) {
        assert running[n] >= d.version,
          format("rollout of version {0} declared done while node {1} runs version {2}", d.version, n, running[n]);
      }
    }
    on Issued do (i: (version: int, content: int)) {
      assert i.version > latest, format("version {0} issued after version {1}", i.version, latest);
      issued[i.version] = i.content;
      latest = i.version;
    }
    on PushSent do (p: (node: int, version: int)) {
      assert p.node in running, format("push to unknown node {0}", p.node);
      assert p.version == latest,
        format("pushed version {0} to node {1}, but version {2} superseded it", p.version, p.node, latest);
      assert p.node == 0 || running[0] >= p.version,
        format("pushed version {0} to node {1} before the canary applied it", p.version, p.node);
      pushed += ((node = p.node, version = p.version));
    }
    on Applied do (a: (node: int, version: int, content: int)) {
      assert a.node in running, format("unknown node {0} applied a version", a.node);
      assert a.node in started, format("node {0} applied a version before it reported starting", a.node);
      assert (node = a.node, version = a.version) in pushed,
        format("node {0} applied version {1}, which was never pushed to it", a.node, a.version);
      assert a.version > running[a.node],
        format("node {0} applied version {1} after version {2}", a.node, a.version, running[a.node]);
      assert a.version in issued && issued[a.version] == a.content,
        format("node {0} applied version {1} with content {2}, which was not issued", a.node, a.version, a.content);
      running[a.node] = a.version;
    }
  }
}

// Progress: once the operator has issued every planned version, every node
// eventually runs the latest one, and the service declares that rollout done.
spec RolloutCompletes observes RolloutPlanned, Issued, Applied, RolloutDone {
  var nodes: int;
  var issuesLeft: int;
  var latest: int;
  // nodes running `latest`
  var current: set[int];
  var done: bool;

  start state Init {
    // snappea-lint: allow unused-payload-field -- RolloutSafety checks contents
    on RolloutPlanned do (p: (nodes: int, version: int, content: int, issues: int)) {
      assert p.issues > 0, "planned no new versions";
      nodes = p.nodes;
      issuesLeft = p.issues;
      latest = p.version;
      goto Rolling;
    }
    on Issued do { assert false, "version issued before the rollout was planned"; }
    on Applied do { assert false, "apply before the rollout was planned"; }
    on RolloutDone do { assert false, "rollout done before it was planned"; }
  }

  hot state Rolling {
    on RolloutPlanned do { assert false, "rollout planned twice"; }

    // snappea-lint: allow unused-payload-field -- RolloutSafety checks contents
    on Issued do (i: (version: int, content: int)) {
      assert issuesLeft > 0, "more versions issued than planned";
      issuesLeft = issuesLeft - 1;
      latest = i.version;
      current = default(set[int]);
      done = false;
    }
    on RolloutDone do (d: (version: int)) {
      assert d.version == latest, format("rollout of superseded version {0} declared done", d.version);
      done = true;
      Check();
    }

    // snappea-lint: allow unused-payload-field -- RolloutSafety checks contents
    on Applied do (a: (node: int, version: int, content: int)) {
      if (a.version == latest) { current += (a.node); }
      Check();
    }
  }

  state Converged {
    on RolloutPlanned do { assert false, "rollout planned twice"; }
    on Issued do { assert false, "version issued after the rollout converged"; }
    on Applied do { assert false, "a node applied a version after every node ran the latest"; }
    on RolloutDone do { assert false, "rollout declared done twice"; }
  }

  fun Check() {
    if (issuesLeft == 0 && done) {
      assert sizeof(current) == nodes,
        format("rollout declared done with {0} of {1} nodes on version {2}", sizeof(current), nodes, latest);
      goto Converged;
    }
  }
}
