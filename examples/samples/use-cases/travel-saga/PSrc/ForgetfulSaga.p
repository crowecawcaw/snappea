// Negative control, kept apart so its code is not part of the passing tests: on cancel
// it releases the steps already confirmed and ends at once, forgetting the
// reservation still in flight. When that reservation succeeds, nobody releases it.
// tcBugForgetsInFlightStep shows the contract catching the orphan.
machine ForgetfulSaga {
  var owner: machine;
  var services: map[BookingStep, machine];
  var held: seq[BookingStep];

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
      if (!r.ok) { goto Compensating; }
      if (r.step == Car) { goto Done, TripBooked; }
      goto Booking, NextStep(r.step);
    }
    on UserCancel do (user: machine) {
      // BUG: compensates only confirmed steps; the in-flight one is forgotten.
      announce CancelAccepted;
      send user, CancelResp, true;
      goto Compensating;
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
    ignore ReserveResp;
  }

  state Done {
    entry (outcome: SagaOutcome) {
      announce SagaEnded, outcome;
      send owner, SagaDone, outcome;
    }
    ignore ReserveResp, UserCancel;
  }
}
