// The driver announces the messages before the sender starts, so the spec knows what
// must arrive even if the sender never sends.
fun Plan(): seq[int] {
  var ids: seq[int];
  var planned: set[int];
  ids += (0, 1);
  ids += (1, 2);
  planned += (1);
  planned += (2);
  announce eMessagesPlanned, planned;
  return ids;
}

fun StartPair(net: Network) {
  var receiver: Receiver;
  receiver = new Receiver(net);
  new Sender((net = net, receiver = receiver, ids = Plan()));
}

// Two messages; the network loses up to two packets, messages or acknowledgements.
machine LostPackets {
  start state Init {
    entry { StartPair(new Network((drops = 2, duplicates = 0))); }
  }
}

// Two messages; the network delivers one packet twice. The copy can arrive after the
// next message, so it tests deduplication and reordering together.
machine DuplicatedPacket {
  start state Init {
    entry { StartPair(new Network((drops = 0, duplicates = 1))); }
  }
}

machine DuplicatedPacketNoDedup {
  start state Init {
    entry {
      var net: Network;
      net = new Network((drops = 0, duplicates = 1));
      new Sender((net = net, receiver = new NoDedupReceiver(net), ids = Plan()));
    }
  }
}

test tcLostPackets [main=LostPackets]:
  assert ExactlyOnce in { LostPackets, Network, Sender, Receiver };

test tcDuplicatedPacket [main=DuplicatedPacket]:
  assert ExactlyOnce in { DuplicatedPacket, Network, Sender, Receiver };

// expect-bug: delivered twice
test tcBugNoDedup [main=DuplicatedPacketNoDedup]:
  assert ExactlyOnce in { DuplicatedPacketNoDedup, Network, Sender, NoDedupReceiver };
