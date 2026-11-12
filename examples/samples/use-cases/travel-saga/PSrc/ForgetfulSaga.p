// Negative control, kept apart so its code is not part of the passing tests: on cancel
// it releases the steps already confirmed and ends at once, forgetting the
// reservation still in flight. When that reservation succeeds, nobody releases it.
// tcBugForgetsInFlightStep shows the contract catching the orphan.
machine ForgetfulSaga {
  var owner: machine;
  var services: map[tStep, machine];
  var held: seq[tStep];

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
      if (!r.ok) { goto Compensating; }
      if (r.step == Car) { goto Done, TripBooked; }
      goto Booking, NextStep(r.step);
    }
    on eUserCancel do (user: machine) {
      // BUG: compensates only confirmed steps; the in-flight one is forgotten.
      announce eCancelAccepted;
      send user, eCancelResp, true;
      goto Compensating;
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
    ignore eReserveResp;
  }

  state Done {
    entry (outcome: tOutcome) {
      announce eSagaEnded, outcome;
      send owner, eSagaDone, outcome;
    }
    ignore eReserveResp, eUserCancel;
  }
}
