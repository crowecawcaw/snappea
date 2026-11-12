// The saga contract, over what the services did (announced by the services, which are
// the environment in PTst) and what the orchestrator decided. The specs rebuild which
// bookings are held from the services' events; they never read the orchestrator.

// Services: a reservation committed, was declined, or a release was requested.
event eReserved: tStep;
event eReserveFailed: tStep;
event eReleased: tStep;
// Orchestrator decisions.
event eStepStarted: tStep;
event eCancelAccepted;
event eCancelRefused;
event eSagaEnded: tOutcome;
// Driver: the user asked to cancel; the user got this answer.
event eCancelRequested;
event eCancelAnswered: bool;
event eSagaPlanned: bool;

// Safety.
// - Steps run in order (flight, hotel, car), each only after the previous one booked,
//   and none after the saga started rolling back.
// - A release is only for a booking that is held, and always the newest one held
//   (reverse order). Nothing is released unless the saga is rolling back.
// - The saga rolls back only after a failure or an accepted cancel; it ends with all
//   three booked (TripBooked) or none held (RolledBack): no orphan reservation.
// - A cancel is refused only once the trip is booked; once refused, nothing is undone.
spec NoOrphans observes eStepStarted, eReserved, eReserveFailed, eReleased, eCancelRequested,
    eCancelAccepted, eCancelRefused, eSagaEnded {
  var started: seq[tStep];
  // in booking order
  var held: seq[tStep];
  var inFlight: bool;
  var rollingBack: bool;
  var cancelRequested: bool;
  var ended: bool;
  var outcome: tOutcome;

  start state Watching {
    on eStepStarted do (s: tStep) {
      assert !ended && !rollingBack, format("{0} started after the saga began rolling back or ended", Name(s));
      assert !inFlight, format("{0} started while another reservation was in flight", Name(s));
      assert sizeof(started) == sizeof(held) && Order(s) == sizeof(started),
        format("{0} started out of order or before the previous step was booked", Name(s));
      started += (sizeof(started), s);
      inFlight = true;
    }
    on eReserved do (s: tStep) {
      Answered(s);
      held += (sizeof(held), s);
    }
    on eReserveFailed do (s: tStep) {
      Answered(s);
      rollingBack = true;
    }
    on eReleased do (s: tStep) {
      assert sizeof(held) > 0 && held[sizeof(held) - 1] == s,
        format("released {0}, which is not the newest booking held", Name(s));
      assert rollingBack && !ended, format("released {0} although the saga was not rolling back", Name(s));
      held -= (sizeof(held) - 1);
    }
    on eCancelRequested do { cancelRequested = true; }
    on eCancelAccepted do {
      assert cancelRequested, "cancel accepted that nobody requested";
      assert !(ended && outcome == TripBooked), "cancel accepted after the trip was booked";
      if (!ended) { rollingBack = true; }
    }
    on eCancelRefused do {
      assert cancelRequested, "cancel refused that nobody requested";
      assert ended && outcome == TripBooked, "cancel refused while the saga could still roll back";
    }
    on eSagaEnded do (o: tOutcome) {
      assert !ended, "saga ended twice";
      assert !inFlight, format("saga ended with the {0} reservation still in flight", Name(started[sizeof(started) - 1]));
      if (o == TripBooked) {
        assert sizeof(held) == 3 && !rollingBack,
          format("saga ended TripBooked holding {0} of 3 bookings, rolling back: {1}", sizeof(held), rollingBack);
      } else {
        assert sizeof(held) == 0, format("saga ended RolledBack but the {0} booking is still held", Name(held[0]));
        assert rollingBack, "saga rolled back with no failure and no cancel";
      }
      ended = true;
      outcome = o;
    }
  }

  fun Answered(s: tStep) {
    assert inFlight && started[sizeof(started) - 1] == s, format("{0} answered but was not in flight", Name(s));
    inFlight = false;
  }

  // Enum values format as numbers; name them for readable failures.
  // snappea-lint: allow coverage -- runs only inside assertion messages, i.e. when a check fails
  fun Name(s: tStep): string {
    if (s == Flight) { return "flight"; }
    if (s == Hotel) { return "hotel"; }
    return "car";
  }

  fun Order(s: tStep): int {
    if (s == Flight) { return 0; }
    if (s == Hotel) { return 1; }
    return 2;
  }
}

// Progress: every saga ends, and a user who asked to cancel gets an answer that
// matches the orchestrator's decision. Hot until both happen.
spec SagaEnds observes eSagaPlanned, eSagaEnded, eCancelAccepted, eCancelRefused, eCancelAnswered {
  var cancelling: bool;
  var ended: bool;
  var decided: bool;
  var accepted: bool;
  var answered: bool;

  start state Idle {
    on eSagaPlanned do (withCancel: bool) {
      cancelling = withCancel;
      goto Running;
    }
    on eSagaEnded do { assert false, "saga ended before it was planned"; }
    on eCancelAccepted do { assert false, "cancel decided before the saga was planned"; }
    on eCancelRefused do { assert false, "cancel decided before the saga was planned"; }
    on eCancelAnswered do { assert false, "cancel answered before the saga was planned"; }
  }

  hot state Running {
    on eSagaPlanned do { assert false, "saga planned twice"; }
    // snappea-lint: allow unused-payload-field -- NoOrphans checks the outcome
    on eSagaEnded do (o: tOutcome) {
      ended = true;
      Check();
    }
    on eCancelAccepted do { Decide(true); }
    on eCancelRefused do { Decide(false); }
    on eCancelAnswered do (a: bool) {
      assert decided && !answered, "cancel answered before it was decided, or twice";
      assert a == accepted, format("user told cancel accepted={0}, but the saga decided {1}", a, accepted);
      answered = true;
      Check();
    }
  }

  state Finished {
    on eSagaPlanned do { assert false, "saga planned twice"; }
    on eSagaEnded do { assert false, "saga ended twice"; }
    on eCancelAccepted do { assert false, "cancel decided after everything finished"; }
    on eCancelRefused do { assert false, "cancel decided after everything finished"; }
    on eCancelAnswered do { assert false, "cancel answered after everything finished"; }
  }

  fun Decide(a: bool) {
    assert cancelling && !decided, "cancel decided but not planned, or decided twice";
    decided = true;
    accepted = a;
  }

  fun Check() {
    if (ended && (!cancelling || answered)) { goto Finished; }
  }
}
