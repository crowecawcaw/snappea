// The environment: the three booking services and the user. They are not the design
// under test, so they live here and are not mutated; what they announce is ground
// truth for the specs.

// One booking service (flight, hotel or car). It declines if told to fail.
machine BookingService {
  var fails: bool;
  start state Serving {
    entry (f: bool) { fails = f; }
    on Reserve do (r: (saga: machine, step: BookingStep)) {
      if (fails) {
        announce ReserveFailed, r.step;
        send r.saga, ReserveResp, (step = r.step, ok = false);
        return;
      }
      announce Reserved, r.step;
      send r.saga, ReserveResp, (step = r.step, ok = true);
    }
    on Release do (r: (saga: machine, step: BookingStep)) {
      announce Released, r.step;
      send r.saga, ReleaseResp;
    }
  }
}

// The user's cancel button: one request, one answer.
machine CancellingUser {
  start state Asking {
    entry (saga: machine) {
      announce CancelRequested;
      send saga, UserCancel, this;
    }
    on CancelResp do (accepted: bool) { announce CancelAnswered, accepted; }
  }
}

// failAt: the step whose service declines (none if failAt is 3).
fun StartSaga(owner: machine, failAt: int, withCancel: bool): TripSaga {
  var services: map[BookingStep, machine];
  announce SagaPlanned, withCancel;
  services[Flight] = new BookingService(failAt == 0);
  services[Hotel] = new BookingService(failAt == 1);
  services[Car] = new BookingService(failAt == 2);
  return new TripSaga((owner = owner, services = services));
}

// Each step can fail (or none does): the steps that succeeded are released newest
// first, and the failed one is never released.
machine FailureAtEachStep {
  start state Run {
    entry { StartSaga(this, choose(4), false); }
    ignore SagaDone;
  }
}

// Nothing fails, and the user cancels at a moment the scheduler picks: before the
// flight, while any reservation is in flight (including the car, the last step), or
// after the trip is booked.
machine CancelRacesSaga {
  start state Run {
    entry { new CancellingUser(StartSaga(this, 3, true)); }
    ignore SagaDone;
  }
}

// The car fails and the user cancels at any moment: a cancel during compensation or
// after the rollback is accepted and changes nothing.
machine CancelRacesFailure {
  start state Run {
    entry { new CancellingUser(StartSaga(this, 2, true)); }
    ignore SagaDone;
  }
}

// Lifecycle boundary: the trip is booked, and only then does the user cancel. The
// cancel is refused and nothing is released.
machine CancelAfterBooked {
  var saga: TripSaga;
  start state Run {
    entry { saga = StartSaga(this, 3, true); }
    on SagaDone do { new CancellingUser(saga); }
  }
}

// For the negative control: a car service that holds the reservation request until
// the driver lets it commit, so the cancel deterministically arrives while the car,
// the last step, is in flight.
event CarRequested;
event CarGo;
machine SlowCarService {
  var driver: machine;
  start state Waiting {
    entry (d: machine) { driver = d; }
    on Reserve do (r: (saga: machine, step: BookingStep)) {
      send driver, CarRequested;
      receive { case CarGo: { } }
      announce Reserved, r.step;
      send r.saga, ReserveResp, (step = r.step, ok = true);
    }
  }
}

// Cancel while the car reservation is in flight; the car commits only after the saga
// has ended. A saga that forgets the in-flight step ends while it is outstanding.
machine BugForgetsInFlightStep {
  var saga: machine;
  var car: machine;
  start state Run {
    entry {
      var services: map[BookingStep, machine];
      announce SagaPlanned, true;
      car = new SlowCarService(this);
      services[Flight] = new BookingService(false);
      services[Hotel] = new BookingService(false);
      services[Car] = car;
      saga = new ForgetfulSaga((owner = this, services = services));
    }
    on CarRequested do { new CancellingUser(saga); }
    on SagaDone do { send car, CarGo; }
  }
}

module Saga = { TripSaga, BookingService, CancellingUser };

test tcFailureAtEachStep [main=FailureAtEachStep]:
  assert NoOrphans, SagaEnds in (union Saga, { FailureAtEachStep });

test tcCancelRacesSaga [main=CancelRacesSaga]:
  assert NoOrphans, SagaEnds in (union Saga, { CancelRacesSaga });

test tcCancelRacesFailure [main=CancelRacesFailure]:
  assert NoOrphans, SagaEnds in (union Saga, { CancelRacesFailure });

test tcCancelAfterBooked [main=CancelAfterBooked]:
  assert NoOrphans, SagaEnds in (union Saga, { CancelAfterBooked });

// snappea: expect-bug "saga ended with the Car reservation still in flight"
test tcBugForgetsInFlightStep [main=BugForgetsInFlightStep]:
  assert NoOrphans, SagaEnds in { BugForgetsInFlightStep, ForgetfulSaga, BookingService, SlowCarService, CancellingUser };
