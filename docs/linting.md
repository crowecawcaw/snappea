# Linting P models

`snappea check` type-checks and lints all loaded sources before exploring schedules.
An unsuppressed lint finding stops the command with exit status **2**, before it
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
file, line, column, and rule ID, and exits with 0 when clean or 2 on findings or
source errors. It does not explore schedules or write checker artifacts. Test-name
filters are not accepted: selection checks need the complete loaded project.
`check` also lints the whole loaded project, even when you select one test to run.

`snappea observe` and the test libraries (Rust `snappea::test`, and Python, Go and
Java through `snappea observe`) enforce the spec rule `unobserved-handler` too, with
the same suppression comments; the other rules concern model tests.

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
| `unused-payload-field` | An event handler in a spec declares a named payload field but never reads it. | Validate or use the field, or suppress an intentional abstraction. |
| `unobserved-handler` | A spec or scenario handles an event its `observes` list leaves out, so the handler never runs. | Add the event to `observes`, or remove the handler. |
| `scenario-never-accepts` | A scenario has no accepting (`cold`) state, so it always reports 0 coverage. | Mark the state the scenario should reach `cold`. |
| `scenario-cold-start` | A scenario starts in an accepting (`cold`) state, so it accepts before any event. | Mark the start state `hot` (or leave it unmarked) and the accepting state `cold`. |

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

### Unused payload fields

A handler that receives `(revision, digest)` but checks only `revision` does not
validate digest binding. The rule checks anonymous and named event handlers,
resolves tuple type aliases, and reports unused fields separately. A direct write
to a field does not count as reading it. Passing or copying the whole payload is
conservatively treated as using every field; this is not interprocedural dataflow
analysis. Reading a field also does not prove it contributes to a useful property.

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
implementation, state, handler, function, or statement, including its nested code. It does not suppress
sibling declarations or statements. Multiple rule IDs are comma-separated:

```p
// snappea-lint: allow permissive-monitor, unselected-spec -- passive trace fixture
```

The same comment excludes code from `snappea check`'s mutation testing:
`allow mutation` for every mutant of the following code, or `allow mutation:<kind>`
(`remove-statement`, `condition-true`, `condition-false`, `swap-operator`, `remove-not`,
`flip-boolean`) for one kind. See
[mutation testing](checking-models.md#suppressing-a-mutant).

`allow coverage` accepts deliberate missing statement, handler, or branch coverage in
`snappea check`. Coverage suppressions require a nonempty reason after `--`:

```p
// snappea-lint: allow coverage -- this handler is exercised by the Python integration suite
on eObserved do { validate(payload); }
```

The raw coverage report still lists the point; the required coverage counts exclude its
scope. A misspelled coverage suppression ID is an `invalid-suppression` finding.

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
