// The saga orchestrator. It books flight, then hotel, then car, one at a time. If a
// step fails, or the user cancels before the trip is booked, it releases every step
// that succeeded, newest first, waiting for each release. A cancel that arrives
// while a reservation is in flight waits for that reservation's answer: if it
// succeeded, it is released too. A cancel after the trip is booked is refused.
//
// Each service's reserve and release is one atomic step on that service; everything
// between them (the orchestrator's decisions, the user's cancel) interleaves.

enum BookingStep { Flight, Hotel, Car }
enum SagaOutcome { TripBooked, RolledBack }

event Reserve: (saga: machine, step: BookingStep);
event ReserveResp: (step: BookingStep, ok: bool);
event Release: (saga: machine, step: BookingStep);
event ReleaseResp;
event UserCancel: machine;
event CancelResp: bool;
// To whoever started the saga.
event SagaDone: SagaOutcome;

fun NextStep(step: BookingStep): BookingStep {
  if (step == Flight) { return Hotel; }
  return Car;
}

machine TripSaga {
  var owner: machine;
  var services: map[BookingStep, machine];
  // Steps that succeeded and are not yet released, in booking order.
  var held: seq[BookingStep];
  var cancelled: bool;

  start state Init {
    entry (input: (owner: machine, services: map[BookingStep, machine])) {
      owner = input.owner;
      services = input.services;
      goto Booking, Flight;
    }
  }

  state Booking {
    entry (step: BookingStep) {
      announce StepStarted, step;
      send services[step], Reserve, (saga = this, step = step);
    }
    on ReserveResp do (r: (step: BookingStep, ok: bool)) {
      if (r.ok) { held += (sizeof(held), r.step); }
      if (!r.ok || cancelled) { goto Compensating; }
      if (r.step == Car) { goto Done, TripBooked; }
      goto Booking, NextStep(r.step);
    }
    on UserCancel do (user: machine) {
      // Do not compensate yet: the reservation in flight may still succeed.
      cancelled = true;
      Accept(user);
    }
  }

  state Compensating {
    entry {
      if (sizeof(held) == 0) { goto Done, RolledBack; }
      send services[held[sizeof(held) - 1]], Release, (saga = this, step = held[sizeof(held) - 1]);
    }
    on ReleaseResp do {
      held -= (sizeof(held) - 1);
      goto Compensating;
    }
    on UserCancel do (user: machine) { Accept(user); }
  }

  state Done {
    entry (outcome: SagaOutcome) {
      announce SagaEnded, outcome;
      send owner, SagaDone, outcome;
    }
    on UserCancel do (user: machine) {
      if (sizeof(held) == 0) {
        // Already rolled back: nothing left to undo.
        Accept(user);
        return;
      }
      // The trip is booked. Undoing it is a separate refund flow, not this saga.
      announce CancelRefused;
      send user, CancelResp, false;
    }
  }

  fun Accept(user: machine) {
    announce CancelAccepted;
    send user, CancelResp, true;
  }
}
