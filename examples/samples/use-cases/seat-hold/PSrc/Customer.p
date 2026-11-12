// The checkout flow each customer runs: hold, pay, confirm, refund if confirmation is
// refused. Some customers walk away after holding a seat, silently (closed the tab)
// or by cancelling the hold. The payment service is a separate actor, so paying takes real time during
// which the hold can expire.

event eCharge: (client: Customer, hold: int);
event eChargeResp: (hold: int);
event eRefund: (hold: int);
// Tells whoever started the checkout (the test driver) that it is over.
event eCheckoutDone;

machine Payments {
  start state Serving {
    on eCharge do (c: (client: Customer, hold: int)) {
      announce eCharged, (hold = c.hold,);
      send c.client, eChargeResp, (hold = c.hold,);
    }
    on eRefund do (r: (hold: int)) {
      announce eRefunded, (hold = r.hold,);
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
      send store, eHoldReq, (client = this, seat = seat, hold = hold);
    }
    on eHoldResp do (r: (hold: int, ok: bool)) {
      if (r.ok && abandons) {
        // Closed the tab: only the expiry timer will free the seat.
        goto Done, Abandoned;
      }
      if (r.ok && cancels) {
        // Clicked cancel: free the seat now rather than wait for expiry.
        send store, eReleaseReq, (client = this, seat = seat, hold = hold);
        goto Cancelling;
      }
      if (r.ok) {
        send payments, eCharge, (client = this, hold = hold);
        goto Paying;
      }
      goto Done, HoldRejected;
    }
  }

  state Cancelling {
    on eReleaseResp do { goto Done, Cancelled; }
  }

  state Paying {
    on eChargeResp goto Confirming with {
      send store, eConfirmReq, (client = this, seat = seat, hold = hold);
    }
  }

  state Confirming {
    on eConfirmResp do (r: (hold: int, ok: bool)) {
      if (!r.ok) {
        send payments, eRefund, (hold = hold,);
        goto Done, LostHold;
      }
      goto Done, Bought;
    }
  }

  state Done {
    entry (outcome: tOutcome) {
      announce eCheckoutEnded, (hold = hold, outcome = outcome);
      send owner, eCheckoutDone;
    }
  }
}
