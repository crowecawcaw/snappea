---
diataxis: reference
personas: [contributor, coding-agent]
questions:
  - How should I name machines, events, states, functions and tests in a sample?
  - Which sections does a sample README have, in what order?
  - Which modeling shortcuts and suppression reasons are not allowed in samples?
  - What should a spec check, and what should it never do?
---

# Sample style guide

Samples are copied by agents and developers, so every sample follows the same
conventions. [AUTHORING.md](AUTHORING.md) says how to add a sample and what it must
pass; this page states the exact style. When a rule here and an existing sample
disagree, the sample is wrong.

## Naming

| Thing | Case | Example | Notes |
| --- | --- | --- | --- |
| Machine | PascalCase noun | `SeatStore`, `PaymentService` | Name the role, not the algorithm step |
| Event | PascalCase, no `e` prefix | `HoldRequested`, `Held`, `HoldRejected` | Past tense for facts (`Sold`), noun or imperative for requests (`Hold`, `Confirm`) |
| Spec | PascalCase claim | `SeatsSoldOnce`, `CheckoutsEnd` | Reads as the property it checks |
| Scenario monitor | PascalCase situation | `ExpiredWhilePaying` | Names the history it proves was reached |
| State | PascalCase adjective or gerund | `Idle`, `Holding`, `Owing` | |
| Type, enum | PascalCase noun | `Checkout`, `CheckoutOutcome` | Enum values PascalCase and unique across the project (`Bought`, `LostHold`) |
| Function | PascalCase verb | `Charge`, `Resolve`, `SendPay` | |
| Variable, field, parameter | camelCase | `heldBy`, `soldTo`, `p.hold` | Plural for collections (`holds`, `pending`) |
| Test case | `tc` + PascalCase | `tcTwoCustomersOneSeat` | `tcBug*` for expected bugs, `tcFixed*` for the fix in flawed samples |
| Project | PascalCase, matches the folder | `SeatHold.pproj` | |

One name, one meaning:

- Never reuse a name for different kinds of thing. A function, type, state and event
  must not share a name (`type Receipt` plus `event Receipt` plus `fun Receipt`).
  Rename the helper: `SendPay`, `NotePayRequest`, `ReceiptLine`.
- Use the same name for the same thing in source, specs, README and implementation
  log examples.
- Keep names consistent across samples that show the same design. A CAS counter
  uses `Get`/`Got`/`Cas`/`CasResult` everywhere, not different names per sample.
- No abbreviations that the domain does not use (`req`, `resp`, `mgr`). Short
  standard words are fine: `id`, `seq`, `ack`.

## Code layout

- One concern per file: `PSrc/` for the design, `PSpec/` for specs and the events they
  observe, `PTst/` for drivers, fault injectors, scenarios and test declarations.
- Open each file with one to three comment lines saying what it holds.
- Comment the why, not the what. Mark the commit point (`// commit point: the row update`)
  and each atomic primitive.
- Two-space indentation, braces on the same line, one statement per line.
- Use tuple keys and tuple payloads, never packed integers. Write
  `map[(int, int), ...]` or `(term: int, node: int)`, not `term * 10 + id`; packing
  breaks silently when a value passes 9.
- Use `choose(collection)` when the design may pick any element. A `foreach` that keeps
  the last element is deterministic, so the checker never tries the others.
- Make handlers idempotent where the real system retries (joins, acks, commits), or
  state in Bounds why a retry cannot happen.
- A server that blocks in `receive` serves one request at a time. Either that is the
  real design and Bounds says so, or the server uses state handlers. When replies can
  be late or out of order, tag them with a request id and check it.

## Specs

- Rebuild the expected state from observed events, then assert the actual results.
  Never trust a flag or decision computed by the model.
- Observe the event that carries the outcome to its consumer (the reply, or an
  announce at the commit point), not a side announcement that only exists for the spec.
- A check must be able to fail. If an assertion only compares the spec's own
  bookkeeping with itself, remove it.
- Check only what the real system can observe. Do not infer inbox order from
  "announced right after send"; network latency breaks that in real logs. Check that
  an outcome is justified by the history instead (`ok` implies a matching `Served`).
- Add a hot-state progress spec for every promise of the form "every request gets an
  answer" or "every job finishes".
- Assertion messages say what went wrong with the ids involved, using `format`:
  `format("seat {0} sold to hold {1}, which does not hold it", p.seat, p.hold)`.
  These messages are what `expect-bug` pins, so make them specific.
- Reply-address fields (`client: machine`) do not need a spec read.

## Tests

- One focused test per race or boundary named in the README, plus at most one broader
  generated workload.
- Negative controls are separate machines with a `tcBug*` test and an `expect-bug`
  that pins the distinctive message, including ids or the hot state name. A message
  that any hang or any failure would match (`"is waiting to receive an event"`) is
  not acceptable.
- Describe a test as exhaustive only if its report says `search_complete`.

## Suppressions

A suppression is a claim that the tool is wrong here. Each one needs a reason a
reviewer can verify, and is rare.

Allowed reasons:

- The mutant is equivalent: no execution of the real system can tell it apart. Say
  why (`the counter is only compared for equality, so starting at 1 is the same`).
- An intentional abstraction stated in Bounds.

Not allowed:

- "Observation only" or "dropping it removes a check, not behavior" on an announce or
  send the contract depends on. Observe the reply instead, or add the missing obligation.
- "Only bandwidth", "only duplicates", "only memory" when the README presents that
  mechanism as a feature. If the feature matters, check it.
- Suppressing a lint finding to keep a test green when the lint points at a real
  missing check.

## README

Front matter first (`diataxis: tutorial` for concepts, `explanation` for use cases and
flawed samples; personas; three to five questions the sample answers). Then these
sections, in this order, with these headings:

1. `# Title` in sentence case, naming the situation (`Seat holds that expire during payment`).
2. Two to four sentences: the system and why it is hard. Flawed samples add
   **The flaw in one sentence.**
3. **Contract**: bullets, each a property a spec checks. Name the spec.
4. **Atomic primitives**: what one handler models in the real system.
5. `## Files`: a table of file and what it holds.
6. `## Test cases`: a table with columns Test, Drives, Why it matters, Expected.
   Expected is `Passes` or `Expected bug: <exact message prefix>`.
7. Concept-specific sections (`## What building it taught`, `## The counterexample`,
   `## The fix`).
8. `## Run`: the commands, copy-pasteable from the sample folder.
9. `## Bounds and omissions`: machine counts, faults, what is not modeled.
10. `## Testing the implementation`: which events the real code emits, where, and how
    to force each race in an integration test.

Rules for README text:

- Plain, short sentences. Say what the reader can do.
- No exact schedule counts or timings in tables; they go stale. Say "all schedules"
  or "sampled", and give rough numbers in prose only where they teach something.
- Quote error messages exactly as `snappea check` prints them, without the
  `PImplementation.` prefix.
- Every claim about checks, exhaustiveness and mutants must match a current run.
- Difficulty, when given, must match the index in [README.md](README.md).

## Avoid

- Checks that cannot fail, and code comments that teach a rule the model cannot break.
  Add a negative control that breaks it, or remove the claim.
- Two concept samples teaching the same design under different names.
- Primitives no real system offers (a database that pushes results to waiters).
  Model the real mechanism, or name the abstraction honestly in Bounds.
- `e`-prefixed events, Hungarian notation, and names that differ only in case.
- `ignore` or `defer` used to silence an unhandled-event error without a comment
  saying why the event is safe to drop or delay there.
