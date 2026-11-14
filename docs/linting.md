---
diataxis: reference
personas: ["application-developer", "coding-agent"]
questions:
  - "Which modeling mistakes do lints detect?"
  - "How do I fix or suppress a finding?"
  - "Which model state and events can no test notice?"
  - "Why does a test need a hot spec?"
  - "Why must a driver announce its plan before later stages?"
---

# Linting P models

`snappea check` type-checks and lints all loaded sources before exploring schedules.
An unsuppressed blocking lint finding stops the command with exit status **2**, before it
creates checker output. A model bug found during exploration still exits with 1.
`snappea check --warn lint` reports the findings as warnings and checks anyway, e.g.
to see whether a model written for upstream P finds bugs before you address its lints
(see [what fails a check](checking-models.md#what-fails-a-check)).

To run linting alone:

```sh
snappea check lint                   # discover local .pproj or .p sources
snappea check lint Model.pproj
snappea check lint events.p model.p tests.p
snappea check lint --list-rules
```

`check lint` loads and type-checks sources, prints every unsuppressed finding with its
file, line, column, and rule ID, and exits with 0 when there are no blocking findings, or 2 on blocking findings or
source errors. It does not explore schedules or write checker artifacts. Test-name
filters are not accepted: selection checks need the complete loaded project.
`check` also lints the whole loaded project, even when you select one test to run.

`snappea observe` and the test libraries (Rust `snappea::test`, and Python, Go and
Java through `snappea observe`) enforce the spec rule `unobserved-handler` too, with
the same suppression comments; the other rules concern model tests.

Some rules identify a risk rather than a demonstrated modeling mistake. These are
**advisory**: they print `warning[rule-id]` and do not block `check lint` or `check`,
even with `--deny lint`. `--warn lint` continues to downgrade the blocking rules.
`--list-rules` JSON includes each rule's `severity` (`error` or `advisory`). Successful
`check lint` JSON includes advisory entries in `findings`, with source locations;
`check` counts them under `warnings.lint`. Scoped suppressions apply to both kinds.

Linting identifies common weak checks. A clean result does not establish that the
model covers every requirement or faithfully represents the implementation.
`snappea-compat` retains upstream behavior and does not apply these lint rules.

## Rules

| Rule ID | Finding | Suggested fix |
| --- | --- | --- |
| `permissive-monitor` | A spec accepts every sequence of its observed events through empty handlers, ignores, or unconditional state transitions, with no hot-state obligation. | Check an invariant, reject invalid event ordering, or express a progress obligation. |
| `unselected-spec` | A spec is selected by no test or runnable implementation in the loaded project. | Add `assert MySpec in ...` to a test/module, or suppress for an observation-only spec. |
| `no-model-machine` | A runnable test configuration contains no selected implementation machine besides its entry machine. | Put modeled behavior in a separate implementation machine and select it alongside the entry driver. |
| `tautological-assert` | A scalar assertion is guaranteed true by its expression or preceding assignments. | Compare against independently observed state or an independent expected result. |
| `announce-and-send` | Adjacent `announce` and `send` expose the same event and payload to a spec twice. | Remove the redundant `announce`, or suppress an intentional duplicate. |
| `unused-payload-field` | An event handler in a spec declares a named payload field but never reads it outside assertion messages. Formatting a field in an error message does not check it. | Validate or use the field, or suppress an intentional abstraction. |
| `unobserved-handler` | A spec or scenario handles an event its `observes` list leaves out, so the handler never runs. | Add the event to `observes`, or remove the handler. |
| `scenario-never-accepts` | A scenario has no accepting (`cold`) state, so it always reports 0 coverage. | Mark the state the scenario should reach `cold`. |
| `scenario-cold-start` | A scenario starts in an accepting (`cold`) state, so it accepts before any event. | Mark the start state `hot` (or leave it unmarked) and the accepting state `cold`. |
| `enum-value-shadows-machine` | An enum value has the name of a machine, spec or scenario, so the name means the enum value in expressions and the machine in types. | Rename the enum value, e.g. with a prefix. |
| `unchecked-state` | A model or spec field is written, but no assertion, condition, spec or sent event depends on it. | Assert on it in a spec or the driver, send it in an observed event, or remove it. |
| `unchecked-event` | The model sends an event that no spec observes and whose handlers change nothing a check depends on. | Observe it in a spec, or act on it where it is received; remove the send if nothing should. |
| `no-progress-spec` | A test's driver, or a model machine, waits for the model's responses, but none of the test's specs has a hot state. | Add a spec that is `hot` while a request is pending, and select it in the test. |
| `unplanned-driver-stage` | A driver starts more model work only after a model notification, and no driver of the test announces a plan a spec observes. | Announce the planned operations before starting any, and keep a spec `hot` until they all finished. |
| `shared-spec-helper` | Advisory: a selected spec and design machine call the same global helper, directly or transitively. | Compute expected results independently, or suppress intentional shared utilities. |
| `unreachable-statement` | A statement follows a `goto`, a `raise`, or a call to a function that always does one, so it never runs. | Remove the statement, or move it before the call. |

### Permissive monitors

This spec observes activity but cannot reject a trace:

```p
spec BundleTrace observes ePublish, eResult {
  start state Active {
    on ePublish goto Active;
    on eResult goto Active;
  }
}
```

The rule checks reachable states and follows named event handlers. It does not
flag a spec merely because it has no assertions: missing event handlers can reject
invalid traces, and hot states can express liveness. Its initial analysis recognizes
simple passive bodies; arbitrary helper calls are not proven permissive.

### Unselected specs

Defining a spec does not select it. The implicit `Main`/`DefaultImpl` model does
not automatically enable all specs. Write a test such as:

```p
test tcPublish [main=Main]: assert AtomicCommit in { Main, Store, Publisher };
```

The rule resolves named modules across files, including compositions, wrappers,
and runnable `implementation` declarations. Load the test files along with the
spec; linting the spec file alone will report it as unselected. A spec used only
by Python tests or `snappea observe` needs an explicit suppression.

### No modeled machine

Each runnable `test` or `implementation` is checked separately. Its entry machine
is the ordinary implementation machine that starts the scenario, so putting all
behavior in that machine leaves the checker with no separate model behavior to
exercise. Add another `machine` and include it in the test module, for example:

```p
machine Main {
  start state Start {
    entry {
      var worker: Worker;
      worker = new Worker();
    }
  }
}

machine Worker {
  start state Waiting { }
}

test tcWorker [main=Main]: { Main, Worker };
```

Module aliases, compositions, and wrappers are followed when identifying the
machines in each selected configuration. Specs and scenarios do not count, and
an implementation declared elsewhere in the loaded project does not count.
When there are no explicit tests or implementations, the checker's implicit
`DefaultImpl` configuration is checked if a machine named `Main` exists. Files
without a runnable configuration, such as observation-only specs, are not
reported. For an intentional fixed-trace test, put a narrow suppression before
that test or implementation declaration:

```p
// snappea-lint: allow no-model-machine -- this test checks a fixed trace only
test tcTrace [main=Main]: { Main };
```

For the implicit `DefaultImpl` configuration, put the suppression before the
entry machine declaration instead:

```p
// snappea-lint: allow no-model-machine -- this observation model has no separate driver
machine Main { start state S { } }
```

### Tautological assertions

This assertion checks assignments made by the monitor itself:

```p
x = p.newX;
y = p.newY;
assert x == p.newX && y == p.newY, "partial bundle update";
```

The initial rule tracks local scalar copies and literals for integers, booleans,
and strings. It forgets uncertain facts at branch joins, loops, receives, and
calls with possible side effects. It does not assume float self-equality, prove
arbitrary arithmetic identities, or perform whole-program symbolic execution.

### Announcing and sending the same event

Specs observe `send` as well as `announce`. This reports completion twice:

```p
announce eClientDone, id;
send owner, eClientDone, id;
```

Keep the `send`; a separate announcement is unnecessary. If the announcement means
something different, give it a distinct event name that expresses that fact.

`announce-and-send` checks adjacent statements in either order, inside a function
or handler, for an event observed by a spec. It requires identical, repeatable
payload expressions and a send target without calls or nondeterministic choices.
It does not infer equivalence across assignments, branches, or helper calls. This
conservative check reports concrete duplicate observation, not a guessed
request/reply relationship. Intentional duplicates can use a scoped suppression on
the second statement:

```p
announce eClientDone, id;
// snappea-lint: allow announce-and-send -- this test deliberately reports completion twice
send owner, eClientDone, id;
```

### Unused payload fields

A handler that receives `(revision, digest)` but checks only `revision` does not
validate digest binding. The rule checks anonymous and named event handlers,
resolves tuple type aliases, and reports unused fields separately. A direct write
to a field does not count as reading it. Passing or copying the whole payload is
conservatively treated as using every field; this is not interprocedural dataflow
analysis. Reading a field also does not prove it contributes to a useful property.
An index on the left of an assignment is a read: `requested[r.id] = r.amount;` reads
both fields. A handler that unconditionally reaches `assert false` (with an optional message
and preceding literal or formatted diagnostic prints without helper calls) reports an event that must not happen, so it need not read every field. P has no
partial or `_` binding for payloads; to track one field of an event another spec
checks fully, suppress the finding with a reason.

### Unobserved handlers

A monitor sees only the events in its `observes` list. Here the `eRefund` handler
never runs, so the check passes without ever testing refunds:

```p
spec Balance observes ePay {
  start state Open {
    on ePay do (n: int) { assert n > 0; }
    on eRefund do (n: int) { assert n <= paid; }   // error[unobserved-handler]
  }
}
```

Write `spec Balance observes ePay, eRefund`, or delete the handler. `on ... goto` and
`ignore` items are checked too. `snappea-compat` keeps upstream's `[Warning:]` line
instead. For a handler kept on purpose, for example for an event not emitted yet:

```p
    // snappea-lint: allow unobserved-handler -- eRefund is not emitted until v2
    on eRefund do (n: int) { assert n <= paid; }
```

### Scenario states

A scenario counts as covered when it reaches an accepting (`cold`) state.
`scenario-never-accepts` reports a scenario without one: it can never be covered.
`scenario-cold-start` reports one that starts in a `cold` state: it accepts before
anything happens, and counts as covered only after events re-enter an accepting
state. Mark the start state `hot` (or leave it unmarked) and the goal state `cold`:

```p
scenario RefundAfterPay observes ePay, eRefund {
  start state Waiting { on ePay goto Paid; }
  state Paid { on eRefund goto Done; }
  cold state Done {}
}
```

### Unchecked state and events

A test fails only through a check: an assertion (in the model, a driver or a spec), a
condition that decides what runs next, a send, or an operation that can fail. The
type-checked model is analyzed for what each of those depends on, through assignments,
function calls and returns, `new` payloads and event payloads into any machine. A
field nothing depends on can hold any value and every test passes:

```p
machine Server {
  var served: int;
  var log: seq[int];              // error[unchecked-state]: nothing reads it
  start state S {
    on eReq do (c: Client) {
      served = served + 1;
      log += (sizeof(log), served);
      announce eServed, served;   // a spec checks `served`
      send c, eAck;               // error[unchecked-event]: no spec observes eAck,
    }                             // and the client's handler does nothing
  }
}
```

`unchecked-state` reports fields of model machines and specs that are written but
that no check depends on, at the field's declaration. `unchecked-event` reports
an event the model sends or announces that no spec observes and that no machine acts
on: its handlers (in every machine) neither assert, change state, create machines,
nor write or send anything a check depends on. When only a scenario observes the event,
the finding says so: scenarios measure coverage and check nothing. An event a machine handles in some states
but not others counts as checked, since it can fail as unhandled; so does `halt`.
Driver machines (see [mark driver machines](#mark-driver-machines)) and a test's main
machine are not reported: they are the checks. The analysis over-approximates:
collections count as one value and every handler of an event receives every payload
sent with it, so a field it reports is unchecked in every test, and one it does not
report may still be checked only weakly. A field kept for debugging or printing is a
fine reason to suppress:

```p
  // snappea-lint: allow unchecked-state -- trace for debugging only
  var log: seq[int];
```

Mutation testing skips the changes these findings explain (see
[mutation testing](checking-models.md#mutation-testing)).

### No progress spec

A safety spec checks what the model does, not that it does anything. A test whose
driver waits for the model's responses (handles or `receive`s an event a model machine
sends) passes when the model never answers, unless one of its specs has a `hot` state:

```p
spec EveryRequestAnswered observes eReq, eResp {
  var pending: int;
  start cold state Idle { on eReq do { pending = pending + 1; goto Waiting; } }
  hot state Waiting {
    on eReq do { pending = pending + 1; }
    on eResp do { pending = pending - 1; if (pending == 0) { goto Idle; } }
  }
}

test tcServe [main=Driver]: assert Safety, EveryRequestAnswered in { Server, Driver };
```

The same holds when the waiting client is part of the design rather than the driver:
a machine that sends a request and then waits for the reply, in a `receive` or in a
state that handles only replies, passes when the reply never comes. A reply is an event
another machine of the test sends while handling one of this machine's requests (in the
handler, the functions it calls, or the entry of the state it goes to). This finding is
reported once for all tests, at each waiting state or `receive`. A suppression before
the machine covers all of them, for example for a design that does not promise progress:

```p
// snappea-lint: allow no-progress-spec -- Paxos does not promise progress with competing proposers
machine Proposer { ... }
```

`no-progress-spec` reports such a test at its declaration. A test meant to check only
safety, for example alongside another test that checks progress, can suppress it:

```p
// snappea-lint: allow no-progress-spec -- tcServeLiveness checks progress
test tcServe [main=Driver]: assert Safety in { Server, Driver };
```

### Unplanned driver stages

A driver that starts more work only after the model tells it to (the second doctor
starts when the first reports done) depends on that notification. If the model never
sends it, the later stage never starts, nothing is pending, and a spec that is `hot`
only while a request is open sees nothing wrong:

```p
machine Main {
  start state First {
    entry { new Doctor(this); }
    on eDone goto Second;
  }
  state Second {
    entry { new Doctor(this); }    // error[unplanned-driver-stage]
  }
}
```

The fix is for the driver to announce its plan before starting any work, and for a spec
to stay `hot` until every planned operation finished:

```p
    entry { announce eDoctorsPlanned, 2; new Doctor(this); }
```

The rule reports the first `new` of a model machine, or `send` of an event a model
machine handles, that runs only after a driver handles an event a model machine sends:
in that handler, or in a state it goes to that the driver cannot reach otherwise.
Replies to the driver's own requests do not count (`no-progress-spec` covers those), and
neither does forwarding the event received, as a test proxy does. Any driver of the test
announcing an event one of its specs observes counts as a plan.

### Unreachable statements

A `goto` or `raise` leaves the state, so the rest of the handler does not run. A call
to a function that always does one leaves it too:

```p
on eTimeout do {
  Resolve(false);
  return;          // error[unreachable-statement]: Resolve always does `goto Done`
}

fun Resolve(ok: bool) { announce eOutcome, ok; goto Done; }
```

A function always leaves the state when every path through it reaches a `goto` or
`raise` before any `return`, `break` or `continue`. Coverage reports the same statement
as never reached; the lint reports it before checking.

## Suppress intentional findings

Put a standalone comment immediately before the declaration or statement:

```p
// snappea-lint: allow unselected-spec -- checked by Python check_spec tests
spec AtomicCommit observes eCommitted {
  start state Ready {
    // snappea-lint: allow unused-payload-field -- this abstraction checks counts
    on eCommitted do (p: (bundle: string, changed: int, expected: int)) {
      assert p.changed == p.expected;
    }
  }
}
```

The comment allows only the named rule within the following machine, test,
implementation, state, handler, function, or statement, including its nested code
for lint and coverage rules. Mutation suppressions use the narrower scope below. It does not suppress
sibling declarations or statements. Multiple rule IDs are comma-separated:

```p
// snappea-lint: allow permissive-monitor, unselected-spec -- passive trace fixture
```

The same comment excludes code from `snappea check`'s mutation testing:
`allow mutation` for every mutant directly on the following statement or declaration,
or `allow mutation:<kind>`
(`remove-statement`, `condition-true`, `condition-false`, `swap-operator`, `remove-not`,
`flip-boolean`, `empty-body`, `drop-conjunct`, `drop-disjunct`, `remove-defer`,
`defer-to-ignore`, `remove-ignore`) for one kind; after a `mutation:<kind>`, a bare kind
names another, so `allow mutation:remove-statement,condition-false` allows both. See
[mutation testing](checking-models.md#suppressing-a-mutant).

Mutation suppressions do not capture nested statements. A suppression on an `if`
covers removing the `if` and mutating its condition, but not assignments or sends in
its branches. Use `mutation-nested` or `mutation-nested:<kind>` when the whole nested
scope is deliberately exempt; the comma shorthand works with that prefix too.
Prefer named kinds over a blanket suppression. `snappea check --list-suppressions`
(or `-v`) lists suppressed changes and their reasons, plus directives matching no
mutant in the selected model. JSON retains the `mutation.suppressed` count and adds
`suppressed_details` and `stale_suppressions` lists.

`allow coverage` accepts deliberate missing statement, handler, or branch coverage in
`snappea check`. Coverage suppressions require a nonempty reason after `--`:

```p
// snappea-lint: allow coverage -- this handler is exercised by the Python integration suite
on eObserved do { validate(payload); }
```

The coverage report lists the point under `Not covered, suppressed:` with its reason
(`coverage.uncovered_suppressed` in JSON); the required coverage counts exclude its
scope. Before an `if` or `while`, the suppression also covers its condition outcomes. A misspelled coverage suppression ID is an `invalid-suppression` finding.

The `-- reason` suffix is optional, but explains the abstraction to reviewers.
Comments must be on their own line. Unknown IDs, malformed directives, and
directives without an eligible following node produce an `invalid-suppression`
finding; a misspelling cannot silently disable a check. Text inside strings and
block comments is not a directive. There is no global command-line lint bypass.

## Mark driver machines

`// snappea: driver` on its own line before a machine marks it as test code that drives
the model, like the machines under `PTst`. `snappea check` then lists the
[limits it kept](checking-models.md#driver-limits) and does not mutate it. Another
`// snappea:` directive, or one that is not directly before a machine, fails linting
with `invalid-directive`.

## Add a rule

The `snappea-lint` crate has no I/O or scheduling dependency. Implement its `Rule`
trait (`id`, `description`, and `check`), emit `Finding` values with source spans,
and register the rule in `Registry::builtins`. Custom registries can register
additional rules without changing the execution or suppression pipeline. Rule IDs
must be unique. Diagnostics are sorted by source position and rule ID.

The parser lives in `snappea-syntax`, re-exported as `snappea::syntax` for existing
callers. Lint rules share the same parser and source positions as the checker.

Run validation with:

```sh
cargo test -p snappea-lint
cargo test -p snappea --test lint_cli
```

Tests include frozen agent-produced models, clean models, named handlers, module
selection across files, local dataflow boundaries, and suppression scope.


Test intent can be declared with `// snappea: expect-bug "failure text"` or
`// snappea: boundary -- reason` immediately before a test. Malformed or duplicate
directives are `invalid-directive` findings. Expected-bug tests skip progress
obligation lints; boundary tests do not skip lints. See
[expected bugs and boundary tests](checking-models.md#expected-bugs-and-boundary-tests)
for matching, coverage, and selection behavior.


### Shared helpers between design and spec

`shared-spec-helper` reports a global helper whose body is reachable from both a
selected design machine and a spec selected in the same test. It names the helper,
the machines/specs, and the test cases, and points at the helper declaration. Calls
through wrappers and receive cases count. Driver-only helpers, scenarios, foreign
functions without bodies, and sharing that occurs only across unrelated tests do
not produce this advisory.

If the implementation computes a value with `Compute(x)` and its spec checks that
value against `Compute(x)`, a mistake in that helper can change both sides without
failing the check. Prefer an independent calculation or a separately stated invariant
in the spec. This is a call-graph warning, not proof that the spec is weak: shared
representation and formatting utilities may be appropriate. The analysis includes
methods even if this run never calls them and does not establish that a shared call
feeds an assertion. Review the use and suppress an intentional common utility at
its declaration:

```p
// snappea-lint: allow shared-spec-helper -- reviewed representation conversion; invariant is independent
fun RowKey(row: (id: int, value: int)): int { return row.id; }
```
