// The checkout flow each customer runs: hold, pay, confirm, refund if confirmation is
// refused. Some customers walk away after holding a seat, silently (closed the tab)
// or by cancelling the hold. The payment service is a separate actor, so paying takes real time during
// which the hold can expire.

event Charge: (client: Customer, hold: int);
event ChargeResp: (hold: int);
event Refund: (hold: int);
// Tells whoever started the checkout (the test driver) that it is over.
event CheckoutDone;

machine Payments {
  start state Serving {
    on Charge do (c: (client: Customer, hold: int)) {
      announce Charged, (hold = c.hold,);
      send c.client, ChargeResp, (hold = c.hold,);
    }
    on Refund do (r: (hold: int)) {
      announce Refunded, (hold = r.hold,);
    }
  }
}

machine Customer {
  var store: machine;
  var payments: Payments;
  var seat: int;
  var hold: int;
  var abandons: bool;
  var cancels: bool;
  var owner: machine;

  start state Holding {
    entry (input: (owner: machine, store: machine, payments: Payments, seat: int, hold: int, abandons: bool, cancels: bool)) {
      store = input.store;
      payments = input.payments;
      seat = input.seat;
      hold = input.hold;
      abandons = input.abandons;
      cancels = input.cancels;
      owner = input.owner;
      send store, HoldReq, (client = this, seat = seat, hold = hold);
    }
    on HoldResp do (r: (hold: int, ok: bool)) {
      if (r.ok && abandons) {
        // Closed the tab: only the expiry timer will free the seat.
        goto Done, Abandoned;
      }
      if (r.ok && cancels) {
        // Clicked cancel: free the seat now rather than wait for expiry.
        send store, ReleaseReq, (client = this, seat = seat, hold = hold);
        goto Cancelling;
      }
      if (r.ok) {
        send payments, Charge, (client = this, hold = hold);
        goto Paying;
      }
      goto Done, HoldUnavailable;
    }
  }

  state Cancelling {
    on ReleaseResp do { goto Done, Cancelled; }
  }

  state Paying {
    on ChargeResp goto Confirming with {
      send store, ConfirmReq, (client = this, seat = seat, hold = hold);
    }
  }

  state Confirming {
    on ConfirmResp do (r: (hold: int, ok: bool)) {
      if (!r.ok) {
        send payments, Refund, (hold = hold,);
        goto Done, LostHold;
      }
      goto Done, Bought;
    }
  }

  state Done {
    entry (outcome: CheckoutOutcome) {
      announce CheckoutEnded, (hold = hold, outcome = outcome);
      send owner, CheckoutDone;
    }
  }
}
