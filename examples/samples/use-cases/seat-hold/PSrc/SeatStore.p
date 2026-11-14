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

type HoldRequest = (client: Customer, seat: int, hold: int);
type HoldReference = (client: Customer, seat: int, hold: int);

event HoldReq: HoldRequest;
event HoldResp: (hold: int, ok: bool);
event ConfirmReq: HoldReference;
event ConfirmResp: (hold: int, ok: bool);
// From the hold's expiry timer.
event Expire: (seat: int, hold: int);
// The customer cancels its own hold; the store answers once the cancel is done.
event ReleaseReq: (client: Customer, seat: int, hold: int);
event ReleaseResp;

// The fixed design: confirm succeeds only while this hold still holds the seat.
machine SeatStore {
  // seat -> hold currently holding it
  var held: map[int, int];
  var sold: set[int];

  start state Serving {
    on HoldReq do (r: HoldRequest) {
      if (r.seat in held || r.seat in sold) {
        announce HoldRejected, (seat = r.seat, hold = r.hold);
        send r.client, HoldResp, (hold = r.hold, ok = false);
        return;
      }
      held[r.seat] = r.hold;
      announce Held, (seat = r.seat, hold = r.hold);
      new ExpiryTimer((store = this, seat = r.seat, hold = r.hold));
      send r.client, HoldResp, (hold = r.hold, ok = true);
    }
    on ConfirmReq do (r: HoldReference) {
      if (r.seat in held && held[r.seat] == r.hold) {
        held -= (r.seat);
        sold += (r.seat);
        announce Sold, (seat = r.seat, hold = r.hold);
        send r.client, ConfirmResp, (hold = r.hold, ok = true);
      } else {
        announce ConfirmRejected, (seat = r.seat, hold = r.hold);
        send r.client, ConfirmResp, (hold = r.hold, ok = false);
      }
    }
    on Expire do (e: (seat: int, hold: int)) {
      // The timer of a hold that was cancelled fires late, possibly while a later
      // hold has the seat: compare the hold id, not just the seat.
      if (e.seat in held && held[e.seat] == e.hold) {
        held -= (e.seat);
        announce Expired, (seat = e.seat, hold = e.hold);
      }
    }
    on ReleaseReq do (r: (client: Customer, seat: int, hold: int)) {
      // A cancel that arrives after the hold expired, possibly while a later hold has
      // the seat, must not free it either.
      if (r.seat in held && held[r.seat] == r.hold) {
        held -= (r.seat);
        announce HoldReleased, (seat = r.seat, hold = r.hold);
      }
      send r.client, ReleaseResp;
    }
  }
}

// One expiry timer per hold. It fires once, at a moment the scheduler picks: before the
// customer pays, between payment and confirmation, or after the sale.
machine ExpiryTimer {
  start state Armed {
    entry (t: (store: machine, seat: int, hold: int)) {
      send t.store, Expire, (seat = t.seat, hold = t.hold);
    }
  }
}
