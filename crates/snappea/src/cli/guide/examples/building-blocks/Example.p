// Importable P building blocks; no special runtime or language features.
// Export: snappea guide example building-blocks --out vendor
// cd vendor/building-blocks
// Passing: snappea check tcReplies tcNetworkFaults Example.pproj --no-mutation
// Broken: snappea check tcDuplicate Example.pproj --no-mutation
//          must fail with "duplicate terminal reply".
// Broken: snappea check tcMissing Example.pproj --no-mutation
//          must fail in the hot state RepliesPending.
// The library-only BuildingBlocks.pproj can be included by your own project.

event eRequest: (client: machine, id: int);
event eReply: int;

machine Service {
  start state Ready {
    on eRequest do (r: (client: machine, id: int)) { send r.client, eReply, r.id; }
  }
}

// snappea: driver
machine ReplyClient {
  start state Start {
    entry {
      var service: Service;
      service = new Service();
      announce BBPlan, 2;
      announce BBRequested, 1;
      announce BBRequested, 2;
      send service, eRequest, (client = this, id = 1);
      send service, eRequest, (client = this, id = 2);
    }
    on eReply do (id: int) { announce BBReplied, id; }
  }
}

// Negative controls exercise the reusable oracle, not a different hand-written spec.
// snappea: driver
machine DuplicateReply {
  start state Start {
    entry {
      announce BBPlan, 2;
      announce BBRequested, 1;
      announce BBRequested, 2;
      announce BBReplied, 1;
      announce BBReplied, 1;
    }
  }
}

// snappea: driver
machine MissingReply {
  start state Start {
    entry { announce BBPlan, 1; announce BBRequested, 1; }
  }
}

test tcReplies [main=ReplyClient]: assert BBRequestReplies in { ReplyClient, Service };
// snappea-lint: allow no-model-machine -- deliberately broken oracle probe
test tcDuplicate [main=DuplicateReply]: assert BBRequestReplies in { DuplicateReply };
// snappea-lint: allow no-model-machine -- deliberately broken oracle probe
test tcMissing [main=MissingReply]: assert BBRequestReplies in { MissingReply };


// A separate consumer exercises all bounded network behaviors without claiming that
// a lossy transport alone guarantees a reply for every logical request.
event ePacket: int;
machine PacketSink {
  start state Ready { on ePacket do (id: int) { assert id == 1 || id == 2; } }
}
// snappea: driver
machine NetworkProbe {
  start state Start {
    entry {
      var sink: PacketSink;
      var network: BBNetwork;
      sink = new PacketSink();
      network = new BBNetwork((drops = 1, duplicates = 1));
      send network, BBForward, (destination = sink, message = ePacket, payload = 1);
      send network, BBForward, (destination = sink, message = ePacket, payload = 2);
    }
  }
}
spec NetworkBudget observes ePacket {
  var count: map[int, int];
  var total: int;
  start hot state AtLeastOneDelivery {
    entry { count[1] = 0; count[2] = 0; }
    on ePacket goto Delivered with Count;
  }
  cold state Delivered { on ePacket do Count; }
  fun Count(id: int) {
    assert id == 1 || id == 2, "network invented a packet";
    count[id] = count[id] + 1;
    total = total + 1;
    assert count[id] <= 2, "network duplicated one packet more than once";
    assert total <= 3, "network exceeded its shared duplicate budget";
  }
}
test tcNetworkFaults [main=NetworkProbe]: assert NetworkBudget in { NetworkProbe, PacketSink, BBNetwork, BBDelivery };
