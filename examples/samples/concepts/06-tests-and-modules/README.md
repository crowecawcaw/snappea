---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I write several test cases over one design?
  - What are modules, union and bindings (A -> B) for?
  - How do I choose which specs a test case checks?
  - How does coverage combine across the test cases I select?
---

# Test cases and modules: an id allocator under contention and at its limit

One driver rarely reaches every case. A design usually needs a few **test cases**: one
for contention, one per boundary, one per broken variant. Here an allocator hands out
unique ids from 0 to `limit - 1` and refuses once they are used up; two callers race
for ids, one caller runs past the limit, and a broken allocator is swapped in with a
module binding.

**Contract:**

- No id is handed out twice or out of range (`UniqueIds`).
- A refusal happens only when every id has been handed out (`UniqueIds`).
- Every request gets an id or a refusal (`RequestsAnswered`).

**Atomic primitives:** the allocator's `Alloc` handler reads and advances its counter
in one step, like an atomic increment or a sequence in a database.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/Allocator.p`](PSrc/Allocator.p) | `Allocator`, the broken `WrappingAllocator`, and the `Caller` driver |
| [`PSpec/Ids.p`](PSpec/Ids.p) | `UniqueIds` (safety) and `RequestsAnswered` (progress) |
| [`PTst/Tests.p`](PTst/Tests.p) | Modules, two drivers, three test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcTwoCallers` | 2 callers, 1 request each, 3 ids | Contention: concurrent requests get different ids | Passes |
| `tcOneMoreThanLimit` | 1 caller, 4 requests, 3 ids | Boundary: the last id, then a refusal | Passes |
| `tcBugWrapsAround` | Same as above, `WrappingAllocator` bound in | Negative control: reusing ids instead of refusing | Expected bug: `id 0 given twice` |

## The key lines

Each test case names its driver (`[main=...]`), the specs it checks (`assert ...`)
and the machines it may create (`in ...`). **Modules** name groups of machines so test
cases can share them. A module can **bind** one machine to another name: wherever the
model says `new Allocator(...)`, `WrappingAllocation` creates a `WrappingAllocator`
instead. The drivers stay unchanged.

```p
module Allocation = { Allocator };
module Callers = { Caller };
module WrappingAllocation = { WrappingAllocator -> Allocator };
```

**Test cases** combine modules with `union`, plus the driver. Each picks its specs:

```p
test tcTwoCallers [main=TwoCallers]:
  assert UniqueIds, RequestsAnswered in (union Allocation, Callers, { TwoCallers });

// snappea: boundary -- exhaust the id range and request one more
test tcOneMoreThanLimit [main=OneMoreThanLimit]:
  assert UniqueIds, RequestsAnswered in (union Allocation, Callers, { OneMoreThanLimit });

// snappea: expect-bug "id 0 given twice"
test tcBugWrapsAround [main=OneMoreThanLimit]:
  assert UniqueIds in (union WrappingAllocation, Callers, { OneMoreThanLimit });
```

The negative control checks only the spec its bug is about, so it cannot fail for an
unrelated reason. The check succeeds only when the failure message matches, and keeps
the replay. The `// snappea: boundary -- ...` directive on `tcOneMoreThanLimit` says
why a single caller is enough there and suppresses only the low-diversity note; all
checks still apply.

**Drivers outside `PTst`.** `Caller` models the design's users and lives next to the
design in `PSrc`. The comment on the line before it marks it as a driver, like the
machines under `PTst` and each test's `main`:

```p
// snappea: driver
machine Caller { ... }
```

Drivers are not mutated (they are the test, not the design), and the checker lists
the limits they kept ("each Caller sends Alloc exactly once").

## Coverage adds up across the selected test cases

The coverage gate counts what **any selected test case** ran. Together the two
passing tests cover everything:

```text
ok    tcTwoCallers (all 95 schedules, 2 workers, 0.03s)
      Coverage: 4/4 states, 32/37 statements, 4/6 branches, 7/10 event handlers, 6/8 condition outcomes
ok    tcOneMoreThanLimit (all 25 schedules, 2 workers, 0.01s)
      Coverage: 4/4 states, 36/37 statements, 5/6 branches, 9/10 event handlers, 7/8 condition outcomes
Coverage of all test cases: 4/4 states, 38/38 statements, 6/6 branches, 10/10 event handlers, 8/8 condition outcomes
Outside the selected tests (excluded from coverage totals):
  PSrc/Allocator.p:29  WrappingAllocator: not included in selected test cases
Mutation testing: 9 of 9 mutants detected (0.1s)
```

Select one test case and the gate applies to it alone. `snappea check tcTwoCallers`
fails, because two callers never run out of ids:

```text
Not covered by the test case:
  PSpec/Ids.p:23       UniqueIds.Checking, on NoIdsLeft: never handled
  PSpec/Ids.p:41       RequestsAnswered.Waiting, on NoIdsLeft: never handled
  PSrc/Allocator.p:17  Allocator.Serving, on Alloc: if (next < limit) never false
  PSrc/Allocator.p:62  Caller.Asking, on NoIdsLeft: never handled
```

Run all test cases together in CI. Matching expected bugs contribute driver coverage;
their design and spec coverage is reported separately and cannot fill passing-test
gaps. Mutation testing also runs each mutant against every selected test case that ran
the changed code: `tcOneMoreThanLimit` is what catches `next < limit` turned into
`next <= limit`.

## Common mistake: listing a spec in the machine set

The part after `in` holds machines and modules, never specs; specs go after `assert`.
Putting `UniqueIds` in the set gives:

```text
error: could not find interface 'UniqueIds'
   --> PTst/Tests.p:33:72
```

("interface" is P's word for a machine name.) A test case with no `assert` checks
nothing but runtime errors.

## Try this

Bind the broken allocator into the contention test too:
`(union WrappingAllocation, Callers, { TwoCallers })` in `tcTwoCallers`. No
assertion fails, since two requests never exhaust three ids; the check fails on
coverage instead: `WrappingAllocator.Serving, on Alloc: if (next == limit) never
true`. A broken variant needs a test that reaches the case it gets wrong.

## Run

```sh
cd examples/samples/concepts/06-tests-and-modules
snappea tests                     # list the test cases
snappea check --deny mutation     # all three; tcBugWrapsAround passes when its bug matches
```

## Bounds and omissions

At most two callers and four requests, three ids. Ids are never returned to the
allocator.

## Testing the implementation

Emit `IdResponse` (with the id) and `NoIdsLeft` from the allocator where it commits the
counter, and `IdRequested` from callers. One set of specs serves every test: a
contention test with concurrent callers, a boundary test that exhausts the ids, and a
test with a fault you inject on purpose.
