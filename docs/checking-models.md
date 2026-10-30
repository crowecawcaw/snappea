# Checking P models

Run `snappea` in a folder with a `.pproj` file:

```sh
snappea tests                                   # list the test cases
snappea check                                   # check every test, up to 10 seconds each
snappea check tcMyTest                          # one test case
snappea check 'tcMy*' --timeout 5m               # matching tests, up to 5 minutes each
snappea check model.p tcMyTest                  # explicit sources
snappea replay PCheckerOutput/tcMyTest/bug.schedule
```

`snappea check` type-checks and [lints](linting.md) the sources before exploring
schedules. Unsuppressed lint findings stop the check with exit status 2; run
`snappea check lint` to inspect them without exploration. A project that lists
foreign (`.cs`, `.java`) files also fails, since snappea cannot run them and the check
would leave that code out; pass `--ignore-foreign-files` to check the P model anyway.
It selects search strategies automatically.
It starts with exhaustive search so small finite models can finish; with several
workers, each explores its own part of the schedules.
After each 1,024 controlled choices it checks whether the unexplored frontier is
small, state-cache pruning is keeping up with new states, or a shrinking frontier
is projected to finish within another 4,096 choices. If none applies, it releases
the exhaustive search and continues with coverage-guided feedback. Narrow frontiers
(up to 256 untried alternatives) get the bounded probe to reach deeper recovery stages. The 16,384-choice stall budget renews while cache pruning keeps up with new states;
workers share renewals and unused budget, since they share the state cache.
Memory is limited to 100,000 cached states and 1,000,000 retained choice
alternatives; hitting a step bound also starts feedback.
There are no separate random or PCT arms in automatic mode. Feedback retains its
own fresh sampling so it can escape unproductive seeds.

The handoff gives feedback a fresh coverage novelty history; the coverage report
still includes every schedule from both phases. Work budgets count controlled
scheduling and data choices, including replayed DFS prefixes, rather than elapsed
time or schedule counts.

The guided corpus keeps schedules that add statement/branch coverage, Boolean
expression outcomes, consecutive transition pairs, event-order features, scenario
progress, or a distinct failure site. Boolean expression edges also guide assignments
and returns outside the reported condition coverage of `if`/`while` guards. Reported
atomic condition outcomes overlap these expression and branch features; feedback
retains them in coverage reports without treating them as a second source of seed
novelty.
It mutates productive seeds immediately, mixing small input changes with resampled
continuations. Seeds of distinct failure sites remain available when searching for
other bugs. Stalled guidance increases fresh sampling while retaining a share for
mutations. The corpus and event-feature history are bounded.

Feedback also preserves the executed input and scheduling choices when an event
handler reaches a new repetition count. Half of subsequent attempts replay a
saved prefix and sample a fresh continuation; the other half use the coverage
corpus and fresh sampling above. Prefixes with no remaining choices stop being
sampled. Up to 256 checkpoints retain at most 262,144 random choices. This is
enabled for native random feedback, including automatic search after its
exhaustive phase. Compatibility mode retains its original behavior.

A completed exhaustive search ends the run early. Timed models start directly
with feedback because exhaustive time tracking is not supported, and so do models
with foreign functions, unless their implementation declares itself searchable
(`ForeignFunctions::searchable` in the [Rust library](library.md)). Cyclic hot
monitors also continue through fair feedback: closing a repeated state is not a
liveness proof. Stalling changes strategy; it never certifies a model as safe.

The default is a fixed **10-second limit per test**, with no automatic extensions.
`--timeout` overrides it. A timeout
means the search is incomplete, even if it found no bugs. A missing required coverage
point also fails the check, with the same status as a model bug; the report says when
coverage was not reached within the run budget. Exit status is 0 when no bug or
required coverage gap was found, 1 for a bug or coverage gap, and 2 on an error. The
result distinguishes exhaustive completion from an incomplete search.

### What fails a check

Besides bugs, `snappea check` reports three kinds of findings. Each one either fails
the check or is reported as a warning:

| Check | Default | Silence one in the source |
|---|---|---|
| `lint` | fails (exit 2, before exploring) | `// snappea-lint: allow <rule> -- reason` |
| `coverage` | fails (exit 1) | `// snappea-lint: allow coverage -- reason` |
| `mutation` | warns (fails in `check mutation`) | `// snappea-lint: allow mutation[:<kind>] -- reason` |

The source comment is the way to accept an intended finding: it names one rule, covers
one declaration or statement, and carries its reason into code review. To change what
fails for a single run, use `--warn <check>` or `--deny <check>` (repeatable; `all`
names every check; a single check overrides `all`):

```sh
snappea check --warn lint        # look for bugs in a model before addressing its lints
snappea check --warn coverage    # explore while the test suite is still being built
snappea check --deny mutation    # CI: also fail on undetected mutants
```

A warned finding is still printed, labelled `warning`, and the final line says
`with warnings: ...`; the JSON result counts them under `warnings`. Fix or suppress
warnings before relying on a passing result. The [playground](../web/README.md) warns on all three
unless "Strict checks" is ticked.

Workers run in parallel, one per available CPU core by default; `-j N` limits resource
use. Workers split exhaustive search between them, each taking its own part of the
schedules, and share the states already explored. The first bug, or every worker
completing its part, stops the run. `--seed` sets their starting seeds, but worker
timing can change which bug wins and how many schedules run. Bug schedules record the
choices and replay exactly, whatever the worker count.
`-v` shows progress and statistics and uses one worker.

While a check runs, `snappea check` shows how far it has got on stderr. On a terminal
that is a status line, redrawn in place and cleared when each test case ends:

```text
Checking tcBank (2/5): 12,400 schedules, 3.1s of 10s
```

In CI (`CI` set) the same line prints every 30 seconds, so the log shows the run is
alive. Otherwise, as when an agent or script captures the output, nothing extra is
printed, and with `-v` the schedule log replaces it. `--progress off` turns it off.
`--progress json` writes each update as one line of JSON on stderr, for a harness or
agent watching a long check:

```json
{"phase":"check","test_case":"tcBank","test":2,"tests":5,"schedules":12400,"schedule_limit":null,"bugs":0,"workers":8,"elapsed":3.1,"time_limit":10.0,"done":false}
```

Mutation testing reports `{"phase":"mutation","detected":3,"mutants":12,...}`. The
last update of each phase has `"done":true` and the counts the result reports. Progress
never estimates time left: the fraction shown is against `--timeout` or the schedule
limit. The [playground](../web/README.md) and the [Rust library](library.md) get the
same updates.

Each test case's files go to `PCheckerOutput/<test>/`, replacing the previous run's:
`bug.txt`, `bug.schedule`, `bug.trace.json` and, for
scenario monitors, `scenario_coverage.json`. A failure includes its report and a
`snappea replay` command. `-o DIR` selects another output folder.

The native CLI no longer accepts schedule counts, strategy selection, exhaustive
mode selection, step bounds, or feedback-tuning options. `snappea-compat` retains
upstream's CLI, defaults, step bounds, and seeded streams, including the original
feedback mutation behavior. Existing compatibility traces still replay; the native
`replay` command accepts step options needed by older traces.

Time tracking (`--latency <file>[:<profile>]`, `--timed`) checks timeouts and latency
budgets; see [design/time](../design/time/RetryBudget/README.md).

`snappea check` type-checks the sources itself. Each error shows its location, the
source lines around it and a suggested fix:

```
error: got type: string, expected: int
  --> a.p:6:11
  |
4 |   start state Init {
5 |     entry {
6 |       x = "hello";
  |           ^^^^^^^
7 |     }
  |
  = help: give it a value of the expected type, convert it (e.g. `x as int`), or change the declared type
```

`snappea-compat` prints them as `p compile` does. `snappea check` prints one line per test case with its coverage and, for a bug, its report and the
`snappea replay` command that reproduces it. Then it lists the code no test case ran:

```
$ snappea check
Checking Inventory: 2 test cases
ok    tcAtomicReserve (1000 schedules, 4 workers, 0.07s)
      Coverage: 41/49 statements, 6/10 branches, 6/9 event handlers
FAIL  tcCheckThenReserve (bug found after 4 schedules, 4 workers, 0.00s)
      Assertion Failed: PSpec/inventory.p:27:7 reservation exceeds available stock
      Replay: snappea replay PCheckerOutput/tcCheckThenReserve/bug.schedule
      Coverage: 43/49 statements, 5/10 branches, 8/9 event handlers (partial: stopped at first bug)
Coverage of all test cases: 49/50 statements, 8/10 branches, 9/9 event handlers
Not covered by any test case:
  PSrc/Inventory.p:46  Inventory.Serving, on eReleaseReq: if (req.order in reservations) never false
  PSrc/Inventory.p:85  OrderClient.Checking, on eAvailableResp: if (resp.units < units) never true
Coverage includes partial results from test cases stopped at their first bug.
Mutation testing skipped: a test case failed.
1 of 2 test cases failed in 0.08s, peak memory 10 MB. Output: PCheckerOutput/
```

A test case that finds a bug writes its files to `PCheckerOutput/<test>/`, replacing
the previous run's: `bug.txt` (the buggy schedule's log), `bug.schedule` and
`bug.trace.json`. Scenario monitors also write `scenario_coverage.json` there. `-v` adds
upstream's progress and statistics. It exits with 0 when no bug is found, 1 when one
is, and 2 on any other error. `snappea <command> --help` lists the options.

On a terminal or in CI (`CI` set) the output is the text above; otherwise (agents,
scripts) it is JSON: per test case its result, schedules, bug report and `replay`
command, coverage and driver limits (`driver`); then the coverage of all test cases
with every uncovered entry, and the mutation testing results under `mutation`.
`--format text|json` or `--json` chooses, as does `SNAPPEA_FORMAT=text|json`, and
`--json-report FILE` also writes the JSON to `FILE`. Every `snappea` command takes
these options, before or after the command (for example, `snappea --json check`).
Argument errors, including unknown options and missing values, also produce a JSON
error result and write `--json-report` when requested. JSON keys retain insertion order.

## Coverage

The [coverage guide](https://snappea.dev/docs/coverage.html) explains the types of
coverage and how to use the gaps. In addition to statements, branches and handlers,
checks measure atomic condition outcomes in `if`/`while` decisions and consecutive
state-transition pairs on a single instance. Structural transition pairs may be
unreachable because of guards or data constraints. Loaded components outside the
selected tests are listed separately, excluded from totals.

JSON adds `conditions`, `transition_pairs`, and `excluded_components` under the
suite's `coverage`; each test and observed spec has the first two counts as well.

Coverage counts what the explored schedules ran, in the machines and monitors each test
case includes: statements, both outcomes of each `if`, and the `do` and `goto` event
handlers of each state. Code that starts with `assert false` must never run, so it is
not counted. A bug stops a test case early, so its coverage is marked partial in
the console and with `coverage.partial: true` in JSON. The combined total includes
these results and is also marked partial when any test case finds a bug.

Scenario monitors appear in the console as a unified summary, including scenarios
that were never satisfied. JSON includes the scenario artifacts under `scenarios`
for the suite and for each test case with scenarios. The same artifacts are still
written to `scenario_coverage.json` for merging across runs.

The default gate checks aggregate coverage across all selected test cases. It requires
all executable statements (including ordinary assertions), event handlers, and both
outcomes of each `if` branch. Condition-expression outcomes and consecutive transition
pairs remain advisory, as do monitor states (a spec or scenario state no schedule
entered is listed, but its handlers are what the gate counts). `assert false` paths keep their existing exclusion because they
must never run. Components outside the selected tests do not enter the totals. To accept
an intentional gap, place `// snappea-lint: allow coverage -- reason` before its
statement, handler, function, or machine; coverage suppressions need a reason and are
validated with the other scoped lint directives. They remain in the raw coverage totals,
while `coverage.required` in JSON shows post-suppression counts.

The list after the test cases names, for each piece of code that never ran, the
outermost one: a handler that was never triggered rather than each of its statements.
It shows the first 20 entries; the JSON result has them all.

Code that never runs is code the model was not checked on. Usually a test case is
missing (no test sends the event, or none reaches the state), or a test driver never
chooses the value that takes the branch; sometimes the code cannot run, which is worth
knowing too. Uncovered code in a spec monitor means the property it checks was never
exercised.

## Mutation testing

Coverage shows what ran; it does not show whether anything checked it. When every test
case passes, `snappea check` runs them again against mutants: copies of the model with
one small change. A mutant that every test case still passes marks code whose effect no
spec or assertion depends on:

```
$ snappea check tcAtomicReserve
...
Mutation testing: 13 of 34 mutants detected (0.1s)
Not detected by any test case (every test case still passes with the change):
  PSrc/Inventory.p:24  Inventory.Init entry: removed `stock = units;`
  PSrc/Inventory.p:32  Inventory.Serving, on eReserveReq: `req.units <= stock` → `req.units < stock`
  PSrc/Inventory.p:35  Inventory.Serving, on eReserveReq: removed `send req.client, eReserveResp, (order = req.order, ok = f...`
  PSrc/Inventory.p:48  Inventory.Serving, on eReleaseReq: `stock + units` → `stock - units`
  ...
```

Here the spec catches overselling but nothing else: an inventory that starts with no
stock, refuses an order that fits exactly, never answers a refused order, or loses stock
on release passes every test case. Each line is a choice: add a spec or an assertion
that the change would break (a liveness spec that every order gets an answer, an
assertion that a refusal is justified), add a test case that drives the code into the
situation, or decide the code does not matter.

A mutant changes one place in the code of the machines under test:

| Mutation | Example |
| --- | --- |
| `remove-statement` | removes an assignment, collection update, `send`, `raise`, `announce`, `goto`, `new` or function call |
| `condition-true`, `condition-false` | replaces an `if` condition by `true` or `false` |
| `swap-operator` | `<` and `<=`, `>` and `>=`, `==` and `!=`, `+` and `-`, `&&` and `\|\|` |
| `remove-not` | `!e` becomes `e` |
| `flip-boolean` | `true` becomes `false` and back |

Spec monitors and the [drivers](#driver-limits) are the tests, so they are not mutated,
and neither are assertions, `print` or format strings. Conditions on a choice (`$`,
`$$`, `choose`) are not forced, swapped or negated: the checker already explores every
choice, so that only relabels the branches. Only code a passing test case ran
is mutated (coverage lists the rest), and an `if` is forced only to the outcome a test
case did not take: forcing a condition that was always true changes nothing.

Each mutant runs only the test cases that ran the changed code. They share 1000
schedules, in turns of 100: each round gives every mutant not yet detected one turn on
the next of its test cases (and every test case gets at least one turn). The seed is
fixed, so a mutant gets the same verdict on every run and machine. A mutant can survive
because no test case reached the failing schedule within its schedules;
`search_complete: true` in JSON means every one of its test cases explored all of its
schedules, so no test case can detect it as written. Some mutants cannot be detected by
any test: `x < n` and `x <= n` behave the same if `x` never equals `n`.

`snappea check mutation [SOURCE...] [TEST...]` runs mutation testing alone: it runs
the test cases, then the mutants, and reports neither coverage nor scenarios. It has a
5 minute time limit by default and exits with 1 when a mutant goes undetected (or a
test case fails), so CI can require every mutant to be detected or suppressed. A test
case named `lint` or `mutation` is selected with a glob (`snappea check 'mutation*'`).

In `snappea check`, mutation testing stops at its time limit, 30 seconds by default
(`--mutation-timeout 2m` changes it, `--no-mutation` skips it). Because the schedules
go round robin, a run the time limit stops has given every mutant about the same share:
on a large model, each may get only its first turn. Undetected mutants with fewer than
1000 schedules are marked `(only 100 schedules)` in the console and have a smaller
`schedules` in JSON; raise the limit for a stronger result. Mutation testing is skipped
when a test case fails: fix the bug first. Undetected mutants do not change the exit
status unless `--deny mutation` is given. The JSON result has every undetected mutant under `mutation.undetected`, with
its `file`, `line`, `context`, `mutation`, `change`, `test_cases`, `schedules` and
`search_complete`.

### Suppressing a mutant

When a change cannot or need not be detected, say so with the
[lint suppression comment](linting.md#suppress-intentional-findings), on its own line
before the statement, handler, function or machine:

```p
// snappea-lint: allow mutation:swap-operator -- units never equals stock in any test
if (req.units <= stock && !(req.order in reservations)) {
```

`mutation` allows every mutant in the code that follows; `mutation:<kind>` allows one
kind (the IDs in the table above). Suppressed mutants do not run and are counted as
suppressed. A misspelled kind fails linting, so it cannot silently allow nothing.

## Driver limits

A test can only find bugs in what its driver makes happen. A passing test case lists
the limits its driver kept in every schedule it explored:

```
ok    tcTwoClients (all 226 schedules, 4 workers, 0.01s)
      Coverage: 19/19 statements, 4/4 branches, 5/5 event handlers, 4/4 condition outcomes
      Driver limits (held in all 226 schedules):
        each Client sends eAcquire, then eRelease, once each
        creates 1 LockServer and 2 Client, never more
      hint: if the real environment can do more, extend the driver to try it
```

Here every client acquires the lock once and releases it once. A lock server that breaks
when a client acquires it again passes this test with full coverage. Each limit is a
choice: if real clients can do it, extend the driver (a loop, more clients, a `$` or
`choose`); if they cannot, the limit is part of the design, and worth knowing.

The driver is the test case's main machine, the machines declared under a `PTst`
folder, and machines marked with a comment on the line before them:

```p
// snappea: driver
machine Client { ... }
```

The limits are of these kinds, most telling first:

| Kind | Example |
| --- | --- |
| `sequence` | `each Client sends eAcquire, then eRelease, once each` |
| `order` | `no Client sends eAcquire after eRelease`; `User sends eRegister once, before everything else` |
| `instances` | `creates 1 LockServer and 2 Client, never more` |
| `count` | `each Client sends eWithdraw at most twice` (only when it is at most twice: every driver stops somewhere, and higher limits rarely matter) |
| `values` | `Proposer is only created with proposerId = 1 or 2`; `eWithdraw is always sent with amount = 10` |

The console shows the first 5; JSON has every one under `driver.facts`, each with its
`kind`, a stable `id` and a `hint`, and the machines treated as the driver under
`driver.machines`. Only events a driver sends to a machine that is not a driver count,
and only `bool`, `int`, enum and `string` payload values. A search that stops before
the end says "explored schedules": the limits held in what it ran. When the search
found a cycle or hit the step bound, the driver may repeat without limit: no counts or
orders are listed then, and `driver.repeats` is true. Driver limits are not listed for a
test case that failed, and never change the exit status. `--no-driver-facts` turns them
off.

For installation and a first model, see the [command line guide](https://snappea.dev/docs/cli.html).
For checking application events and logs, see [snappea observe](observe.md).
