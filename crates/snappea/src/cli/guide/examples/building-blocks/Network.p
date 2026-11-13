// Building block v1: bounded loss/duplication and independent delivery order.
// Include BuildingBlocks.pproj; see README.md for the delivery contract.
type BBMessage = (destination: machine, message: event, payload: any);
event BBForward: BBMessage;

// snappea: driver
machine BBNetwork {
  var dropsLeft: int;
  var duplicatesLeft: int;
  start state Ready {
    entry (budget: (drops: int, duplicates: int)) {
      assert budget.drops >= 0 && budget.duplicates >= 0, "network budgets must be nonnegative";
      dropsLeft = budget.drops;
      duplicatesLeft = budget.duplicates;
    }
    on BBForward do (packet: BBMessage) {
      if (dropsLeft > 0 && $) {
        dropsLeft = dropsLeft - 1;
        return;
      }
      new BBDelivery(packet);
      if (duplicatesLeft > 0 && $) {
        duplicatesLeft = duplicatesLeft - 1;
        new BBDelivery(packet);
      }
    }
  }
}

// Each delivery has its own sender identity. The scheduler can deliver a later
// packet first, or deliver a reply after the client has handled a timeout.
// snappea: driver
machine BBDelivery {
  start state Deliver {
    entry (packet: BBMessage) {
      send packet.destination, packet.message, packet.payload;
    }
  }
}
