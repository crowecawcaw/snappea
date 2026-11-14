---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do I model a controller that commands physical devices?
  - How do I check that the car never moves with its doors open?
  - How do I state "every requested floor is eventually served" as a spec?
---

# Elevator controller, door and motor

A single elevator serves three floors. Passengers press hall and car buttons. The
controller keeps the set of requested floors and commands two devices: the motor moves
the car one floor and reports arrival; the door opens and closes and reports when it
is done, or reports an obstruction (the doors stay open). The controller moves the
motor only after the door has acknowledged it is closed, and opens the door only after
the motor reports the car level with a floor. It keeps its direction while requests
lie ahead, then turns.

**Contract**

- Safety (`DoorsAndMotion`): the car never starts moving with its doors open; the doors
  open only while the car is stopped; the car stays in the shaft. Stated from what the
  motor and door physically do, not from the controller's state.
- Progress (`RequestsServed`, hot state): every press reaches the controller, and every
  registered floor is eventually served: the doors open there after the registration.
- Scheduling (`RequestsServed`): every move goes toward a registered request, and the
  car reverses only when nothing registered lies ahead. Judged when the controller
  commands the move (`MoveCommanded`), against what it knew then.

**Atomic steps**: each controller handler is one step of its control loop. Button
presses, motor arrival and door acknowledgements interleave with them; the motor's
movement and the door's closing take time (other machines run in between).

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `DoorsAndMotion` (safety), `RequestsServed` (progress and scheduling) |
| [`PSrc/Controller.p`](PSrc/Controller.p) | The controller |
| [`PSrc/EagerController.p`](PSrc/EagerController.p) | Negative control: commands the motor before the door's closed acknowledgement |
| [`PTst/Tests.p`](PTst/Tests.p) | Motor, door (with an obstruction budget), passengers, a scenario, tests |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcTwoPassengers` | Car at 0; presses for 2 then 0, and for 1 | Two passengers, a reversal, a floor passed on the way. All schedules | Passes |
| `tcCallWhileClosing` | Car at 0; both press 0 | The second press lands while the doors open, dwell or close: it needs its own opening. All schedules | Passes |
| `tcCallWhilePassing` | Car at 2 called to 0; the other presses 1 at any moment | Press before the car reaches 1 (stop there) or after it passed (come back). All schedules | Passes |
| `tcObstructedDoor` | Doors blocked twice before closing | The car must wait, then still serve the next floor. All schedules | Passes |
| `tcGeneratedRides` | Any start floor, three presses chosen from three floors, 0 or 1 obstruction | Mixed histories. Sampled | Passes |
| `tcBugMoveBeforeClosed` | `EagerController` | The motor starts before the door confirms it is closed | Expected bug: `car moved from floor 0 with its doors open` |

The `ReopenedForLatePress` scenario confirms the checks reach a press for the current
floor after the doors opened there, followed by another opening there before the car
moves.

## What building it taught

- **Judge decisions where they are made.** The first scheduling check ran on the motor's
  `MotorStarted`, and failed falsely: a press registered between the controller's
  decision and the motor starting made a correct move look like a reversal. The
  controller now announces `MoveCommanded`, and the spec also requires every motor
  start to match one, so the announcement cannot be dropped.
- **Count service from registration, not from the press.** A press that lands while
  the doors are already opening at that floor reaches the controller late, and the
  controller (correctly) opens once more. Measuring "served" from the press made that
  extra opening look like a move after everything was served.
- **Progress alone does not pin down scheduling.** With only "eventually served",
  mutating the direction choice survived: any order serves three floors. The
  scheduling rule (never move away from every request; reverse only when nothing lies
  ahead) is what bounds a passenger's wait, and it is what detects those mutants.
- The door and motor are drivers in `PTst/`, so obstruction (a fault) is not mutated.

## Run

```sh
snappea check tcTwoPassengers tcCallWhileClosing tcCallWhilePassing tcObstructedDoor tcGeneratedRides --deny mutation
snappea check tcBugMoveBeforeClosed     # passes only when the declared failure is found
```

## Bounds and omissions

Three floors, two passengers, at most four presses, at most two obstructions per test.
The motor moves one floor per command; the controller decides at every floor. Not
modeled: door open/close timers (dwell is immediate), several cars, load limits,
emergency stop, a door that never stops being obstructed (the bound is what makes
"eventually served" hold), direction-specific hall buttons.

Two suppressions, both equivalent mutants: `>`/`>=` and `<`/`<=` in `NextDirection`. `Dispatch` serves the
current floor before choosing a direction, so no request equals the current floor
there and the operators agree.

## Testing the implementation

Emit `MotorStarted`/`MotorStopped` from the motor driver and `DoorOpened`/
`DoorClosed` from the door driver (or their simulators), and `CallRegistered`/
`MoveCommanded` from the controller. Against hardware simulators, delay the door's
closed acknowledgement and check the motor stays idle; inject obstruction on close;
press the current floor's button during the closing delay; press a floor while the
car is between floors.
