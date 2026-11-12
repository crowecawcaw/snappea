// The network as a machine. Packets wait in a bag (a set, so a retransmitted copy of a
// packet still in flight is the same packet) and are delivered one at a time in any
// order. A bounded number of packets is lost or duplicated, so the search stays finite.
//
// The network is the environment, not the design under test: its faults are test input.

type tPacket = (dest: machine, ev: event, msg: tMsg);
event eNetSend: tPacket;
// From the sender: where to deliver timeouts.
event eSetTimer: machine;
// Sent to the sender when a packet was lost and nothing is in flight: the
// retransmission timer fires.
event eTimeout;
// Internal: deliver the next packet.
event eStep;

// snappea: driver
machine Network {
  var bag: set[tPacket];
  var timerOwner: machine;
  var dropsLeft: int;
  var duplicatesLeft: int;
  var stepping: bool;
  // A packet was lost since the last timeout.
  var lost: bool;

  start state Running {
    entry (config: (drops: int, duplicates: int)) {
      dropsLeft = config.drops;
      duplicatesLeft = config.duplicates;
    }
    on eNetSend do (p: tPacket) {
      bag += (p);
      if (!stepping) {
        stepping = true;
        send this, eStep;
      }
    }
    on eStep do { DeliverOne(); }
    on eSetTimer do (owner: machine) { timerOwner = owner; }
  }

  fun DeliverOne() {
    var p: tPacket;
    // Any packet in flight can go next: this is reordering.
    p = choose(bag);
    if (dropsLeft > 0 && $) {
      dropsLeft = dropsLeft - 1;
      lost = true;
      bag -= (p);
    } else if (duplicatesLeft > 0 && $) {
      // Deliver a copy and keep the packet in flight: it arrives twice.
      duplicatesLeft = duplicatesLeft - 1;
      send p.dest, p.ev, p.msg;
    } else {
      bag -= (p);
      send p.dest, p.ev, p.msg;
    }
    if (sizeof(bag) > 0) {
      send this, eStep;
      return;
    }
    stepping = false;
    // Nothing in flight and something was lost: the sender's timeout expires. Timeouts
    // are assumed longer than any delivery, so they never fire while packets are in flight.
    if (lost) {
      lost = false;
      send timerOwner, eTimeout;
    }
  }
}
