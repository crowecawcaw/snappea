// A model of the inventory service for one product, and of the orders that use it.
// The service announces the spec's events after each stock change, where the
// applications call emit.

type tReserveReq = (client: OrderClient, order: string, units: int);

// The atomic design: one request checks stock and reserves it.
event eReserveReq: tReserveReq;
event eReserveResp: (order: string, ok: bool);
event eReleaseReq: (order: string);

// The check-then-reserve design: an order reads the available stock first, then
// commits a reservation the service no longer checks.
event eAvailableReq: (client: OrderClient);
event eAvailableResp: (units: int);
event eCommitReq: tReserveReq;

machine Inventory {
  var stock: int;
  var reservations: map[string, int];

  start state Init {
    entry (units: int) {
      stock = units;
      announce eStocked, (units = units,);
      goto Serving;
    }
  }

  state Serving {
    on eReserveReq do (req: tReserveReq) {
      if (req.units <= stock && !(req.order in reservations)) {
        Reserve(req);
      } else {
        send req.client, eReserveResp, (order = req.order, ok = false);
      }
    }
    // snappea-lint: allow coverage -- exercised by tcCheckThenReserve, not the atomic design test
    on eAvailableReq do (req: (client: OrderClient)) {
      send req.client, eAvailableResp, (units = stock,);
    }
    // snappea-lint: allow coverage -- exercised by tcCheckThenReserve, not the atomic design test
    on eCommitReq do (req: tReserveReq) {
      Reserve(req);
    }
    on eReleaseReq do (req: (order: string)) {
      var units: int;
      // snappea-lint: allow coverage -- clients release only successful reservations
      if (req.order in reservations) {
        units = reservations[req.order];
        stock = stock + units;
        reservations -= req.order;
        announce eReleased, (order = req.order, units = units);
      }
    }
  }

  fun Reserve(req: tReserveReq) {
    stock = stock - req.units;
    reservations[req.order] = req.units;
    announce eReserved, (order = req.order, units = req.units);
    send req.client, eReserveResp, (order = req.order, ok = true);
  }
}

// snappea-lint: allow no-progress-spec -- one order reserves its units, then may cancel; this example checks safety only
machine OrderClient {
  var inventory: Inventory;
  var order: string;
  var units: int;

  start state Init {
    entry (input: (inventory: Inventory, order: string, units: int, checkFirst: bool)) {
      inventory = input.inventory;
      order = input.order;
      units = input.units;
      // snappea-lint: allow coverage -- exercised by tcCheckThenReserve, not the atomic design test
      if (input.checkFirst) {
        send inventory, eAvailableReq, (client = this,);
        goto Checking;
      }
      send inventory, eReserveReq, (client = this, order = order, units = units);
      goto Reserving;
    }
  }

  // snappea-lint: allow coverage -- exercised by tcCheckThenReserve, not the atomic design test
  state Checking {
    on eAvailableResp do (resp: (units: int)) {
      if (resp.units < units) {
        goto Done;
      }
      send inventory, eCommitReq, (client = this, order = order, units = units);
      goto Reserving;
    }
  }

  state Reserving {
    on eReserveResp do (resp: (order: string, ok: bool)) {
      if (resp.ok && $) {
        send inventory, eReleaseReq, (order = order,);
      }
      goto Done;
    }
  }

  state Done {}
}
