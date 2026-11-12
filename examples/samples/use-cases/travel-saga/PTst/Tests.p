// The environment: the three booking services and the user. They are not the design
// under test, so they live here and are not mutated; what they announce is ground
// truth for the specs.

// One booking service (flight, hotel or car). It declines if told to fail.
machine BookingService {
  var fails: bool;
  start state Serving {
    entry (f: bool) { fails = f; }
    on eReserve do (r: (saga: machine, step: tStep)) {
      if (fails) {
        announce eReserveFailed, r.step;
        send r.saga, eReserveResp, (step = r.step, ok = false);
        return;
      }
      announce eReserved, r.step;
      send r.saga, eReserveResp, (step = r.step, ok = true);
    }
    on eRelease do (r: (saga: machine, step: tStep)) {
      announce eReleased, r.step;
      send r.saga, eReleaseResp;
    }
  }
}

// The user's cancel button: one request, one answer.
machine CancellingUser {
  start state Asking {
    entry (saga: machine) {
      announce eCancelRequested;
      send saga, eUserCancel, this;
    }
    on eCancelResp do (accepted: bool) { announce eCancelAnswered, accepted; }
  }
}

// failAt: the step whose service declines (none if failAt is 3).
fun StartSaga(owner: machine, failAt: int, withCancel: bool): TripSaga {
  var services: map[tStep, machine];
  announce eSagaPlanned, withCancel;
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
    ignore eSagaDone;
  }
}

// Nothing fails, and the user cancels at a moment the scheduler picks: before the
// flight, while any reservation is in flight (including the car, the last step), or
// after the trip is booked.
machine CancelRacesSaga {
  start state Run {
    entry { new CancellingUser(StartSaga(this, 3, true)); }
    ignore eSagaDone;
  }
}

// The car fails and the user cancels at any moment: a cancel during compensation or
// after the rollback is accepted and changes nothing.
machine CancelRacesFailure {
  start state Run {
    entry { new CancellingUser(StartSaga(this, 2, true)); }
    ignore eSagaDone;
  }
}

// Lifecycle boundary: the trip is booked, and only then does the user cancel. The
// cancel is refused and nothing is released.
machine CancelAfterBooked {
  var saga: TripSaga;
  start state Run {
    entry { saga = StartSaga(this, 3, true); }
    on eSagaDone do { new CancellingUser(saga); }
  }
}

// For the negative control: a car service that holds the reservation request until
// the driver lets it commit, so the cancel deterministically arrives while the car,
// the last step, is in flight.
event eCarRequested;
event eCarGo;
machine SlowCarService {
  var driver: machine;
  start state Waiting {
    entry (d: machine) { driver = d; }
    on eReserve do (r: (saga: machine, step: tStep)) {
      send driver, eCarRequested;
      receive { case eCarGo: { } }
      announce eReserved, r.step;
      send r.saga, eReserveResp, (step = r.step, ok = true);
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
      var services: map[tStep, machine];
      announce eSagaPlanned, true;
      car = new SlowCarService(this);
      services[Flight] = new BookingService(false);
      services[Hotel] = new BookingService(false);
      services[Car] = car;
      saga = new ForgetfulSaga((owner = this, services = services));
    }
    on eCarRequested do { new CancellingUser(saga); }
    on eSagaDone do { send car, eCarGo; }
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

// expect-bug: saga ended with the car reservation still in flight
test tcBugForgetsInFlightStep [main=BugForgetsInFlightStep]:
  assert NoOrphans, SagaEnds in { BugForgetsInFlightStep, ForgetfulSaga, BookingService, SlowCarService, CancellingUser };
