---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I write several test cases over one design?
  - What are modules, union and bindings (A -> B) for?
  - How do I choose which specs a test case checks?
  - How does coverage combine across the test cases I select?
---

# Test cases and modules

One driver rarely reaches every case. A design usually needs a few **test cases**:
one for contention, one per boundary, one per broken variant. Each test case names
its driver (`[main=...]`), the specs it checks (`assert ...`) and the machines it may
create (`in ...`). **Modules** name groups of machines so test cases can share them.

This sample is an id allocator: it hands out unique ids from 0 to `limit - 1` and
refuses once they are used up.

**Contract:** no id is handed out twice or out of range; a refusal happens only when
every id has been handed out; every request gets an id or a refusal.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/Allocator.p`](PSrc/Allocator.p) | `Allocator`, the broken `WrappingAllocator`, and the `Caller` driver |
| [`PSpec/Ids.p`](PSpec/Ids.p) | `UniqueIds` (safety) and `RequestsAnswered` (progress) |
| [`PTst/Tests.p`](PTst/Tests.p) | Modules, two drivers, three test cases |

## The key lines

**Modules** group machines. A module can **bind** one machine to another name:
wherever the model says `new Allocator(...)`, `WrappingAllocation` creates a
`WrappingAllocator` instead. The drivers stay unchanged.

```p
module Allocation = { Allocator };
module Callers = { Caller };
module WrappingAllocation = { WrappingAllocator -> Allocator };
```

**Test cases** combine modules with `union`, plus the driver. Each picks its specs:

```p
test tcTwoCallers [main=TwoCallers]:
  assert UniqueIds, RequestsAnswered in (union Allocation, Callers, { TwoCallers });

test tcOneMoreThanLimit [main=OneMoreThanLimit]:
  assert UniqueIds, RequestsAnswered in (union Allocation, Callers, { OneMoreThanLimit });

// snappea-lint: allow no-progress-spec -- negative control for UniqueIds; ...
// expect-bug: id 0 given twice
test tcBugWrapsAround [main=OneMoreThanLimit]:
  assert UniqueIds in (union WrappingAllocation, Callers, { OneMoreThanLimit });
```

The negative control checks only the spec its bug is about, so it cannot fail for an
unrelated reason. The linter normally requires a progress spec in a test whose driver
waits for replies; the suppression says why this one has none.

**Drivers outside `PTst`.** `Caller` models the design's users and lives next to the
design in `PSrc`. The comment on the line before it marks it as a driver, like the
machines under `PTst` and each test's `main`:

```p
// snappea: driver
machine Caller { ... }
```

Drivers are not mutated (they are the test, not the design), and the checker lists
the limits they kept ("each Caller sends eAlloc exactly once").

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcTwoCallers` | 2 callers, 1 request each, 3 ids | Contention: concurrent requests get different ids | passes, all 53 schedules |
| `tcOneMoreThanLimit` | 1 caller, 4 requests, 3 ids | Boundary: the last id, then a refusal | passes, all 4 schedules |
| `tcBugWrapsAround` | Same as above, `WrappingAllocator` bound in | Negative control: reusing ids instead of refusing | **fails**: `id 0 given twice` |

`tcOneMoreThanLimit` is sequential, so the checker notes that its schedules all reach
nearly the same states. That is expected for a boundary test: its job is to reach the
boundary, while `tcTwoCallers` explores the interleavings.

## Run it

```sh
cd examples/samples/concepts/06-tests-and-modules
snappea tests                                   # list the test cases
snappea check tcTwoCallers tcOneMoreThanLimit --deny mutation
snappea check tcBugWrapsAround                  # exits 1
```

## Coverage adds up across the selected test cases

The coverage gate counts what **any selected test case** ran. Together the two
passing tests cover everything:

```text
ok    tcTwoCallers (all 53 schedules, 0.03s)
      Coverage: 4/4 states, 32/37 statements, 4/6 branches, 7/10 event handlers, ...
ok    tcOneMoreThanLimit (all 4 schedules, 0.00s)
      Coverage: 4/4 states, 36/37 statements, 5/6 branches, 9/10 event handlers, ...
Coverage of all test cases: 4/4 states, 38/38 statements, 6/6 branches, 10/10 event handlers, ...
Outside the selected tests (excluded from coverage totals):
  PSrc/Allocator.p:29  WrappingAllocator: not included in selected test cases
Mutation testing: 9 of 9 mutants detected (0.0s)
```

Select one test case and the gate applies to it alone. `snappea check tcTwoCallers`
fails, because two callers never run out of ids:

```text
Not covered by the test case:
  PSpec/Ids.p:23       UniqueIds.Checking, on eNoIdsLeft: never handled
  PSrc/Allocator.p:17  Allocator.Serving, on eAlloc: if (next < limit) never false
  ...
Coverage requirements were not met for 10 points.
```

So run the passing test cases together in CI, and run a broken variant's test on its
own (it is meant to fail, and machines only it uses are left out of the totals).
Mutation testing also runs each mutant against every selected test case that ran the
changed code: `tcOneMoreThanLimit` is what catches `next < limit` turned into
`next <= limit`.

## Common mistake: listing a spec in the machine set

The part after `in` holds machines and modules, never specs; specs go after `assert`.
Putting `UniqueIds` in the set gives:

```text
error: could not find interface 'UniqueIds'
   --> PTst/Tests.p:33:83
```

("interface" is P's word for a machine name.) A test case with no `assert` checks
nothing but runtime errors.

## Try this

Bind the broken allocator into the contention test too:
`(union WrappingAllocation, Callers, { TwoCallers })` in `tcTwoCallers`. No
assertion fails, since two requests never exhaust three ids; the check fails on
coverage instead: `WrappingAllocator.Serving, on eAlloc: if (next == limit) never
true`. A broken variant needs a test that reaches the case it gets wrong.

## Bounds

At most two callers and four requests, three ids. Ids are never returned to the
allocator.

## In your implementation

Emit `eIdResp` (with the id) and `eNoIdsLeft` from the allocator where it commits the
counter, and `eIdRequested` from callers. One set of specs serves every test: a
contention test, a boundary test that exhausts the ids, and a test with a fault you
inject on purpose.
