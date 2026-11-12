// A plausible broken design, kept as a negative control: confirm checks only that the
// seat is not sold yet, not that this hold still owns it. After a hold expires and
// another customer holds the seat, the first customer's late confirmation still sells
// it. tcBugConfirmIgnoresExpiry shows the spec catching it.
machine UnfencedSeatStore {
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
      // BUG: no check that r.hold is still the hold on this seat.
      if (!(r.seat in sold)) {
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
      if (e.seat in held && held[e.seat] == e.hold) {
        held -= (e.seat);
        announce eExpired, (seat = e.seat, hold = e.hold);
      }
    }
  }
}
