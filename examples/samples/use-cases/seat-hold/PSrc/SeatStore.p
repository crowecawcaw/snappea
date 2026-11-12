// A ticketing service: a customer holds a seat, pays, then confirms the hold. A hold
// expires after a while so abandoned checkouts free their seats. Payment is slow, so
// expiry can race with confirmation.
//
// Real atomic primitives (one database transaction each, on the seat's row):
//   hold:    free -> held(hold)
//   confirm: held(hold) -> sold(hold)
//   expire:  held(hold) -> free
//   release: held(hold) -> free (the customer clicked cancel)
// Everything between them (the payment, the timer) can interleave.

type tHoldReq = (client: Customer, seat: int, hold: int);
type tHoldRef = (client: Customer, seat: int, hold: int);

event eHoldReq: tHoldReq;
event eHoldResp: (hold: int, ok: bool);
event eConfirmReq: tHoldRef;
event eConfirmResp: (hold: int, ok: bool);
// From the hold's expiry timer.
event eExpire: (seat: int, hold: int);
// The customer cancels its own hold.
event eReleaseReq: (seat: int, hold: int);

// The fixed design: confirm succeeds only while this hold still holds the seat.
machine SeatStore {
  // seat -> hold currently holding it
  var held: map[int, int];
  var sold: set[int];

  start state Serving {
    on eHoldReq do (r: tHoldReq) {
      if (r.seat in held || r.seat in sold) {
        announce eHoldRejected, (seat = r.seat, hold = r.hold);
        send r.client, eHoldResp, (hold = r.hold, ok = false);
        return;
      }
      held[r.seat] = r.hold;
      announce eHeld, (seat = r.seat, hold = r.hold);
      new ExpiryTimer((store = this, seat = r.seat, hold = r.hold));
      send r.client, eHoldResp, (hold = r.hold, ok = true);
    }
    on eConfirmReq do (r: tHoldRef) {
      if (r.seat in held && held[r.seat] == r.hold) {
        held -= (r.seat);
        sold += (r.seat);
        announce eSold, (seat = r.seat, hold = r.hold);
        send r.client, eConfirmResp, (hold = r.hold, ok = true);
      } else {
        announce eConfirmRejected, (seat = r.seat, hold = r.hold);
        send r.client, eConfirmResp, (hold = r.hold, ok = false);
      }
    }
    on eExpire do (e: (seat: int, hold: int)) {
      // The timer of a hold that was cancelled fires late, possibly while a later
      // hold has the seat: compare the hold id, not just the seat.
      if (e.seat in held && held[e.seat] == e.hold) {
        held -= (e.seat);
        announce eExpired, (seat = e.seat, hold = e.hold);
      }
    }
    on eReleaseReq do (r: (seat: int, hold: int)) {
      // A cancel that arrives after the hold expired, possibly while a later hold has
      // the seat, must not free it either.
      if (r.seat in held && held[r.seat] == r.hold) {
        held -= (r.seat);
        announce eHoldReleased, (seat = r.seat, hold = r.hold);
      }
    }
  }
}

// One expiry timer per hold. It fires once, at a moment the scheduler picks: before the
// customer pays, between payment and confirmation, or after the sale.
machine ExpiryTimer {
  start state Armed {
    entry (t: (store: machine, seat: int, hold: int)) {
      send t.store, eExpire, (seat = t.seat, hold = t.hold);
    }
  }
}
