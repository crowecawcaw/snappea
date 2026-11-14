---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do I model an orchestrated saga with compensation in P?
  - What happens when the user cancels while a booking is still in flight?
  - How do I check that a saga never leaves an orphan reservation?
---

# Travel booking saga

An orchestrator books a trip with three services: flight, then hotel, then car. Any
step can fail. On failure it compensates: it releases every booking that succeeded,
newest first. The user can cancel at any moment.

**Contract**

- `NoOrphans`: the saga ends with all three booked (`TripBooked`) or none held
  (`RolledBack`). No orphan reservation survives, including one whose answer was in
  flight when the user cancelled.
- `NoOrphans`: steps run in order, one at a time, and none starts once rollback has
  begun. Only steps that succeeded are released, newest first, and only during
  rollback. The saga rolls back only after a failure or an accepted cancel.
- `NoOrphans`: a cancel is accepted until the trip is booked. After that it is refused
  and nothing is undone (undoing a booked trip is a separate refund flow). A cancel
  after a rollback is accepted and changes nothing.
- `SagaEnds` (hot state): every saga ends, and a user who cancels gets an answer
  matching the decision.

**Atomic primitives**: each service's reserve and release is one step on that
service. The orchestrator's requests, the services' answers and the user's cancel are
separate messages and interleave.

**The in-flight rule**: a cancel that arrives while a reservation is in flight does
not compensate at once. The orchestrator waits for that answer; if it succeeded, the
booking joins the list to release. Compensating immediately and ignoring the late
answer is the classic orphan.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `NoOrphans` (safety) and `SagaEnds` (progress, hot state) |
| [`PSrc/TripSaga.p`](PSrc/TripSaga.p) | The orchestrator |
| [`PSrc/ForgetfulSaga.p`](PSrc/ForgetfulSaga.p) | Negative control: compensates at once, forgets the in-flight step |
| [`PTst/Tests.p`](PTst/Tests.p) | Booking services and the user (environment), drivers, test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcFailureAtEachStep` | The driver picks which service declines (flight, hotel, car or none) | Compensation of exactly the steps that succeeded, newest first. All schedules | Passes |
| `tcCancelRacesSaga` | Nothing fails; the user cancels at any moment | Cancel before, during each step (including the car's success racing with it), after booking. All schedules | Passes |
| `tcCancelRacesFailure` | The car declines; the user cancels at any moment | Cancel during compensation or after rollback. All schedules | Passes |
| `tcCancelAfterBooked` | The user cancels only after the trip is booked | Lifecycle boundary: refused, nothing released. All schedules | Passes |
| `tcBugForgetsInFlightStep` | `ForgetfulSaga`; the user cancels while the car is in flight | The car's late success is never released | Expected bug: `saga ended with the Car reservation still in flight` |

## Run

```sh
snappea check tcFailureAtEachStep tcCancelRacesSaga tcCancelRacesFailure tcCancelAfterBooked --deny mutation
snappea check tcBugForgetsInFlightStep    # passes only when the declared failure is found
```

## Bounds and omissions

One saga, three steps, at most one failing service and one cancel. Services answer
every request and release always succeeds. Not modeled: an orchestrator crash (it
would need its progress in a durable saga log and to resume from it), service
timeouts with unknown outcome (the orchestrator must then release "maybe booked"
steps, so release must be idempotent and accept unknown bookings), retries of
failed releases, and several sagas competing for the same inventory.

## Testing the implementation

Emit `StepStarted`, `CancelAccepted`, `CancelRefused` and `SagaEnded` from the
orchestrator, and `Reserved`, `ReserveFailed`, `Released` from fake services; check
them against `NoOrphans` and `SagaEnds`. To force the in-flight race, make the fake
car service block its reply, send the cancel, and release the reply only after the
orchestrator has handled the cancel.
