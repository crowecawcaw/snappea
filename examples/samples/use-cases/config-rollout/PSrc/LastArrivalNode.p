// Negative control, a separate machine so its code is excluded from the passing tests'
// coverage: the node applies whatever push arrives last. When the network reorders
// the pushes of two versions, the node goes back to the older one.
machine LastArrivalNode {
  var id: int;
  var version: int;
  var content: int;

  start state Running {
    entry (input: (id: int, version: int, content: int)) {
      id = input.id;
      version = input.version;
      content = input.content;
    }
    on ePush do (p: tPush) {
      // BUG: no comparison with the version already running.
      if (p.version != version) {
        version = p.version;
        content = p.content;
        announce eApplied, (node = id, version = version, content = content);
      }
      send p.service, eAck, (node = id, version = p.version);
    }
  }
}
