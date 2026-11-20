// The ticketing contract, stated over events the store and the payment service announce
// at their commit points. The spec rebuilds what must be true from the history of
// events alone; it does not read the store's fields.

event Held: (seat: int, hold: int);
event HoldRejected: (seat: int, hold: int);
event Expired: (seat: int, hold: int);
event HoldReleased: (seat: int, hold: int);
event Sold: (seat: int, hold: int);
event ConfirmRejected: (seat: int, hold: int);
event Charged: (hold: int);
event Refunded: (hold: int);

// Safety: a seat is held by at most one hold at a time and is sold at most once.
spec SeatsSoldOnce observes Held, Expired, HoldReleased, Sold {
  // seats that are held now
  var held: set[int];
  var sold: set[int];

  start state Watching {
    on Held do (p: (seat: int, hold: int)) {
      assert !(p.seat in held), format("seat {0} held twice", p.seat);
      assert !(p.seat in sold), format("seat {0} held after it was sold", p.seat);
      held += (p.seat);
    }
    on Expired do (p: (seat: int, hold: int)) {
      held -= (p.seat);
    }
    on HoldReleased do (p: (seat: int, hold: int)) {
      held -= (p.seat);
    }
    on Sold do (p: (seat: int, hold: int)) {
      assert !(p.seat in sold), format("seat {0} sold twice", p.seat);
      held -= (p.seat);
      sold += (p.seat);
    }
  }
}

// How a checkout ended, as its customer reports it.
enum CheckoutOutcome { HoldUnavailable, Bought, LostHold, Abandoned, Cancelled }
// abandons: walks away after holding (only expiry frees the seat).
// cancels: cancels the hold right after getting it.
type Checkout = (hold: int, seat: int, abandons: bool, cancels: bool);
event CheckoutEnded: (hold: int, outcome: CheckoutOutcome);
