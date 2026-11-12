// The saga orchestrator. It books flight, then hotel, then car, one at a time. If a
// step fails, or the user cancels before the trip is booked, it releases every step
// that succeeded, newest first, waiting for each release. A cancel that arrives
// while a reservation is in flight waits for that reservation's answer: if it
// succeeded, it is released too. A cancel after the trip is booked is refused.
//
// Each service's reserve and release is one atomic step on that service; everything
// between them (the orchestrator's decisions, the user's cancel) interleaves.

enum tStep { Flight, Hotel, Car }
enum tOutcome { TripBooked, RolledBack }

event eReserve: (saga: machine, step: tStep);
event eReserveResp: (step: tStep, ok: bool);
event eRelease: (saga: machine, step: tStep);
event eReleaseResp;
event eUserCancel: machine;
event eCancelResp: bool;
// To whoever started the saga.
event eSagaDone: tOutcome;

fun NextStep(step: tStep): tStep {
  if (step == Flight) { return Hotel; }
  return Car;
}

machine TripSaga {
  var owner: machine;
  var services: map[tStep, machine];
  // Steps that succeeded and are not yet released, in booking order.
  var held: seq[tStep];
  var cancelled: bool;

  start state Init {
    entry (input: (owner: machine, services: map[tStep, machine])) {
      owner = input.owner;
      services = input.services;
      goto Booking, Flight;
    }
  }

  state Booking {
    entry (step: tStep) {
      announce eStepStarted, step;
      send services[step], eReserve, (saga = this, step = step);
    }
    on eReserveResp do (r: (step: tStep, ok: bool)) {
      if (r.ok) { held += (sizeof(held), r.step); }
      if (!r.ok || cancelled) { goto Compensating; }
      if (r.step == Car) { goto Done, TripBooked; }
      goto Booking, NextStep(r.step);
    }
    on eUserCancel do (user: machine) {
      // Do not compensate yet: the reservation in flight may still succeed.
      cancelled = true;
      Accept(user);
    }
  }

  state Compensating {
    entry {
      if (sizeof(held) == 0) { goto Done, RolledBack; }
      send services[held[sizeof(held) - 1]], eRelease, (saga = this, step = held[sizeof(held) - 1]);
    }
    on eReleaseResp do {
      held -= (sizeof(held) - 1);
      goto Compensating;
    }
    on eUserCancel do (user: machine) { Accept(user); }
  }

  state Done {
    entry (outcome: tOutcome) {
      announce eSagaEnded, outcome;
      send owner, eSagaDone, outcome;
    }
    on eUserCancel do (user: machine) {
      if (sizeof(held) == 0) {
        // Already rolled back: nothing left to undo.
        Accept(user);
        return;
      }
      // The trip is booked. Undoing it is a separate refund flow, not this saga.
      announce eCancelRefused;
      send user, eCancelResp, false;
    }
  }

  fun Accept(user: machine) {
    announce eCancelAccepted;
    send user, eCancelResp, true;
  }
}
