// The saga contract, over what the services did (announced by the services, which are
// the environment in PTst) and what the orchestrator decided. The specs rebuild which
// bookings are held from the services' events; they never read the orchestrator.

// Services: a reservation committed, was declined, or a release was requested.
event Reserved: BookingStep;
event ReserveFailed: BookingStep;
event Released: BookingStep;
// Orchestrator decisions.
event StepStarted: BookingStep;
event CancelAccepted;
event CancelRefused;
event SagaEnded: SagaOutcome;
// Driver: the user asked to cancel; the user got this answer.
event CancelRequested;
event CancelAnswered: bool;
event SagaPlanned: bool;

// Safety.
// - Steps run in order (flight, hotel, car), each only after the previous one booked,
//   and none after the saga started rolling back.
// - A release is only for a booking that is held, and always the newest one held
//   (reverse order). Nothing is released unless the saga is rolling back.
// - The saga rolls back only after a failure or an accepted cancel; it ends with all
//   three booked (TripBooked) or none held (RolledBack): no orphan reservation.
// - A cancel is refused only once the trip is booked; once refused, nothing is undone.
spec NoOrphans observes StepStarted, Reserved, ReserveFailed, Released, CancelRequested,
    CancelAccepted, CancelRefused, SagaEnded {
  var started: seq[BookingStep];
  // in booking order
  var held: seq[BookingStep];
  var inFlight: bool;
  var rollingBack: bool;
  var cancelRequested: bool;
  var ended: bool;
  var outcome: SagaOutcome;

  start state Watching {
    on StepStarted do (s: BookingStep) {
      assert !ended && !rollingBack, format("{0} started after the saga began rolling back or ended", s);
      assert !inFlight, format("{0} started while another reservation was in flight", s);
      assert sizeof(started) == sizeof(held) && Order(s) == sizeof(started),
        format("{0} started out of order or before the previous step was booked", s);
      started += (sizeof(started), s);
      inFlight = true;
    }
    on Reserved do (s: BookingStep) {
      Answered(s);
      held += (sizeof(held), s);
    }
    on ReserveFailed do (s: BookingStep) {
      Answered(s);
      rollingBack = true;
    }
    on Released do (s: BookingStep) {
      assert sizeof(held) > 0 && held[sizeof(held) - 1] == s,
        format("released {0}, which is not the newest booking held", s);
      assert rollingBack && !ended, format("released {0} although the saga was not rolling back", s);
      held -= (sizeof(held) - 1);
    }
    on CancelRequested do { cancelRequested = true; }
    on CancelAccepted do {
      assert cancelRequested, "cancel accepted that nobody requested";
      assert !(ended && outcome == TripBooked), "cancel accepted after the trip was booked";
      if (!ended) { rollingBack = true; }
    }
    on CancelRefused do {
      assert cancelRequested, "cancel refused that nobody requested";
      assert ended && outcome == TripBooked, "cancel refused while the saga could still roll back";
    }
    on SagaEnded do (o: SagaOutcome) {
      assert !ended, "saga ended twice";
      assert !inFlight, format("saga ended with the {0} reservation still in flight", started[sizeof(started) - 1]);
      if (o == TripBooked) {
        assert sizeof(held) == 3 && !rollingBack,
          format("saga ended TripBooked holding {0} of 3 bookings, rolling back: {1}", sizeof(held), rollingBack);
      } else {
        assert sizeof(held) == 0, format("saga ended RolledBack but the {0} booking is still held", held[0]);
        assert rollingBack, "saga rolled back with no failure and no cancel";
      }
      ended = true;
      outcome = o;
    }
  }

  fun Answered(s: BookingStep) {
    assert inFlight && started[sizeof(started) - 1] == s, format("{0} answered but was not in flight", s);
    inFlight = false;
  }

  fun Order(s: BookingStep): int {
    if (s == Flight) { return 0; }
    if (s == Hotel) { return 1; }
    return 2;
  }
}

// Progress: every saga ends, and a user who asked to cancel gets an answer that
// matches the orchestrator's decision. Hot until both happen.
spec SagaEnds observes SagaPlanned, SagaEnded, CancelAccepted, CancelRefused, CancelAnswered {
  var cancelling: bool;
  var ended: bool;
  var decided: bool;
  var accepted: bool;
  var answered: bool;

  start state Idle {
    on SagaPlanned do (withCancel: bool) {
      cancelling = withCancel;
      goto Running;
    }
    on SagaEnded do { assert false, "saga ended before it was planned"; }
    on CancelAccepted do { assert false, "cancel decided before the saga was planned"; }
    on CancelRefused do { assert false, "cancel decided before the saga was planned"; }
    on CancelAnswered do { assert false, "cancel answered before the saga was planned"; }
  }

  hot state Running {
    on SagaPlanned do { assert false, "saga planned twice"; }
    on SagaEnded do (o: SagaOutcome) {
      ended = true;
      Check();
    }
    on CancelAccepted do { Decide(true); }
    on CancelRefused do { Decide(false); }
    on CancelAnswered do (a: bool) {
      assert decided && !answered, "cancel answered before it was decided, or twice";
      assert a == accepted, format("user told cancel accepted={0}, but the saga decided {1}", a, accepted);
      answered = true;
      Check();
    }
  }

  state Finished {
    on SagaPlanned do { assert false, "saga planned twice"; }
    on SagaEnded do { assert false, "saga ended twice"; }
    on CancelAccepted do { assert false, "cancel decided after everything finished"; }
    on CancelRefused do { assert false, "cancel decided after everything finished"; }
    on CancelAnswered do { assert false, "cancel answered after everything finished"; }
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
