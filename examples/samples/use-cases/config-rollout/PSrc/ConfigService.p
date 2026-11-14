// A config service rolls versioned configs out to nodes in two stages: first the
// canary (node 0), then, once the canary acknowledges, every other node. A new version
// issued mid-rollout (including a rollback, issued as a higher version with the old
// content) abandons the current rollout and starts over at the canary.
//
// Real atomic steps: the service handles one message at a time (one leader); a node
// applies a config atomically. Each push travels on its own connection (a BBDelivery),
// so pushes can be delayed and two pushes to one node can arrive in either order.

type ConfigPush = (service: machine, version: int, content: int);

event Issue: (version: int, content: int);
event PushConfig: ConfigPush;
event Ack: (node: int, version: int);


machine ConfigService {
  // node index -> node; node 0 is the canary
  var nodes: seq[machine];
  var version: int;
  var content: int;
  var acked: set[int];

  start state Starting {
    entry (ns: seq[machine]) {
      nodes = ns;
      goto Idle;
    }
  }

  // Every node acknowledged the current version (or nothing was issued yet).
  state Idle {
    on Issue do (i: (version: int, content: int)) { Start(i.version, i.content); }
    // An ack for a version this service already moved past.
    ignore Ack;
  }

  state Canary {
    on Issue do (i: (version: int, content: int)) { Start(i.version, i.content); }
    on Ack do (a: (node: int, version: int)) {
      var n: int;
      if (a.version != version) {
        // Late ack for a superseded version: it says nothing about this one. (Only
        // the canary has been sent the current version, so a current ack is its.)
        return;
      }
      n = 1;
      while (n < sizeof(nodes)) {
        Push(n);
        n = n + 1;
      }
      goto Rest;
    }
  }

  state Rest {
    on Issue do (i: (version: int, content: int)) { Start(i.version, i.content); }
    on Ack do (a: (node: int, version: int)) {
      if (a.version != version) { return; }
      acked += (a.node);
      if (sizeof(acked) == sizeof(nodes) - 1) {
        announce RolloutDone, (version = version,);
        // snappea-lint: allow mutation:remove-statement -- equivalent: each node acks each version once, so no ack for this version is still coming; Rest and Idle handle Issue and older acks alike
        goto Idle;
      }
    }
  }

  fun Start(v: int, c: int) {
    version = v;
    content = c;
    acked = default(set[int]);
    announce Issued, (version = v, content = c);
    Push(0);
    goto Canary;
  }

  fun Push(n: int) {
    announce PushSent, (node = n, version = version);
    new BBDelivery((destination = nodes[n], message = PushConfig,
      payload = (service = this, version = version, content = content)));
  }
}

// A node applies a pushed config only if its version is newer than the one it runs,
// and acknowledges every push it receives with that push's version.
machine Node {
  var id: int;
  var version: int;
  var content: int;

  start state Running {
    entry (input: (id: int, version: int, content: int)) {
      id = input.id;
      version = input.version;
      content = input.content;
      announce NodeStarted, (node = id, version = version, content = content);
    }
    on PushConfig do (p: ConfigPush) {
      // snappea-lint: allow mutation:swap-operator -- each version is pushed to each node once, so a push never equals the running version
      if (p.version > version) {
        version = p.version;
        content = p.content;
        announce Applied, (node = id, version = version, content = content);
      }
      send p.service, Ack, (node = id, version = p.version);
    }
  }
}
