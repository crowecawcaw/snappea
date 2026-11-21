# Seat holds that expire during payment

A ticketing service lets a customer hold a seat, pay, then confirm. Holds expire so
abandoned checkouts free their seats. Payment is slow, so the expiry timer can fire
between payment and confirmation, and another customer can hold the seat meanwhile.

## Contract

- A seat is held by at most one hold at a time and sold at most once, only to the hold
  that currently has it. A hold or a confirmation is rejected only when the seat's
  state justifies it.
- A late expiry timer or a late cancel never frees a seat that a later hold has. A
  cancel is answered only once its hold no longer has the seat.
- Every charge ends in exactly one of a sale or a refund.
- Every checkout ends, and every hold is eventually sold, expired or cancelled, even
  when the customer walks away.

## Atomic steps

One transaction on the seat's row each: hold (free to held), confirm (held by this
hold to sold), expire and cancel (held by this hold to free). Payment and the timer
are separate actors, so everything between them interleaves.

## What can happen

Any number of customers want any seats, at any time, also after a seat was sold. A
customer can walk away after holding a seat: silently (only expiry frees the seat) or
by cancelling the hold. Each hold's timer fires once, at any moment: before payment,
between payment and confirmation, after the sale, or after the hold was cancelled. A
cancel can arrive after its hold expired. The store and the payment service do not
crash, and messages are not lost.

Not in scope: payment failures, partial refunds, seat maps.
