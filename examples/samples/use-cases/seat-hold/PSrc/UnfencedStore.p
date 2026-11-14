// A plausible broken design, kept as a negative control: confirm checks only that the
// seat is not sold yet, not that this hold still owns it. After a hold expires and
// another customer holds the seat, the first customer's late confirmation still sells
// it. tcBugConfirmIgnoresExpiry shows the spec catching it.
machine UnfencedSeatStore {
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
      // BUG: no check that r.hold is still the hold on this seat.
      if (!(r.seat in sold)) {
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
      if (e.seat in held && held[e.seat] == e.hold) {
        held -= (e.seat);
        announce Expired, (seat = e.seat, hold = e.hold);
      }
    }
  }
}
