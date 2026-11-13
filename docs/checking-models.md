---
diataxis: reference
personas: ["application-developer", "coding-agent"]
questions:
  - "How do checks, exploration, coverage and mutation testing behave?"
  - "What makes a check fail?"
  - "What does a bug report say, and how do I read a runtime error, liveness or deadlock report?"
  - "What do the scenario coverage terms mean, and when does a scenario fail a check?"
---

# Checking P models

Run `snappea` in a folder with a `.pproj` file:

```sh
snappea tests                                   # list the test cases
snappea check                                   # check every test, 10 seconds in all
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
the exhaustive search and continues with an adaptive scheduling portfolio. Narrow frontiers
(up to 256 untried alternatives) get the bounded probe to reach deeper recovery stages. The 16,384-choice stall budget renews while cache pruning keeps up with new states;
workers share renewals and unused budget, since they share the state cache.
Memory is limited to 100,000 cached states and 1,000,000 retained choice
alternatives; hitting a step bound also starts the portfolio.

The portfolio keeps separate guided-random and deliberate-delay corpora,
plus PCT trials with 0, 2, and 10 priority-change points. Delays temporarily hold
one runnable actor while others execute; they release it if nothing else can run.
Their checkpoints distinguish the delayed actor, starting point, duration, ordering,
and whether the delay has ended. Delayed schedules and PCT schedules are unfair;
only the fair guided-random arm performs liveness temperature checks (exhaustive search
checks cycles instead; see below).

After actor contention, a bounded early pilot also explores these schedulers before
resuming a converging exhaustive search. Tiny finite searches finish before this
pilot starts. Several senders targeting one receiver favor priority ordering and a
larger pilot, capped at 512 schedules. Bounded coverage/event-progress novelty and
new failure sites adjust later allocation, with execution work charged to each arm.
Every arm retains an exploration floor when those signals are uninformative.
Selection samples among scores instead of giving all exploitation work to one
slightly higher score; the initial send-target hint fades with experience.
Feedback also retains fresh sampling so it can escape unproductive seeds.

The portfolio continues sampling alongside a converging DFS search rather than
letting state-cache pruning monopolize the entire run. In full exploration, once
a concurrent execution fails, finding different failures becomes the priority:
three portfolio schedules run between DFS schedules, while DFS remains active.
Without a failure, the continuing allocation is one portfolio schedule per three
DFS schedules. Tiny finite searches finish before this interleaving starts.

Automatic guided arms also retain prefixes at short-circuit condition edges,
before unrelated suffix choices can destroy useful progress. These bounded
checkpoints complement event-count prefixes. Explicit feedback and upstream
compatibility retain their existing mutation policies.

These heuristics do not guarantee that auto will outperform an explicit strategy
on every model or seed. Library callers can inspect allocation and novelty
scores in `CheckResult::scheduler_stats`.

Exhaustive search and the portfolio keep separate coverage novelty histories.
Revisiting either phase restores its history; replaying already-covered code does
not earn fresh novelty. The coverage report includes every schedule from both phases. Work budgets count controlled
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

A completed exhaustive search ends the run early. Timed models are searched the same
way: the state cache compares clocks too, so a timed model that ends can finish its
search. One that loops never returns to an explored state, since clocks only grow; it
is sampled, and its test line says `timed model: sampled`
(`"incomplete_reason": "timed_model_sampled"` in JSON). Models with foreign functions
start directly with feedback, unless their implementation declares itself searchable
(`ForeignFunctions::searchable` in the [Rust library](library.md)). Stalling
changes strategy; it never certifies a model as safe.

Exhaustive search checks liveness on cycles. When a schedule returns to a state already
on its own path while a liveness monitor is hot, the steps since that state form a
cycle the model can repeat forever. If the monitor stayed out of cold states through
the cycle, and the cycle is fair (every machine that could run at every step of it ran,
and every `$$` choice made in it took both values), the check fails at once:

```
Answered detected liveness bug in hot state 'Waiting': the last 6 steps return to an
earlier state, so the model can repeat them forever without the monitor entering a
cold state.
```

A retry loop that never succeeds is then found on the first schedule that closes it,
as cheaply as a deadlock. Plain `$` and `choose` choices need not vary: a network that
may lose every retry is a real livelock. The saved schedule ends with the cycle and
names its start in a `--snappea-lasso:<step>` header; `snappea replay` checks that the
end returns to that state. Other cycles are pruned. A search that pruned a cycle
through a state where a liveness monitor was not cold (an unfair cycle, or one through
a cold state) has not verified liveness, since another cycle through that state could
be a bug: it continues with fair feedback search. With several workers, any pruned
cycle counts, since the shared state cache can hide a cycle from the worker that closes
it.

By default the selected test cases **share 10 seconds**: each gets the time left divided
by the test cases left. A test case stops early, marked `stopped: coverage stopped
growing` (`coverage_stopped_growing` in JSON), once no schedule in the second half of
its schedules reached new coverage, after at least 200 schedules and a quarter of its
share; the time it leaves goes to the test cases after it. This keeps a check of a large
model fast enough to run after every change, but explores less: the Raft tutorial's
known bug is found in about 6 of 10 such runs, and every time with `--timeout 10s`.
`--timeout` gives each test case its own fixed limit instead, with no early stop; set
it before trusting a pass on a large model. A test case whose exhaustive search is
still running never stops early for coverage: finishing the search proves more than
sampling finds.

A passing test case that did not cover every schedule says why, with
`incomplete_reason` in JSON:

- `exhaustive_search_unfinished`: the exhaustive search was still running when time
  ran out (`untried_alternatives` counts those left on its current path). The hint
  suggests a longer `--timeout`. How much longer cannot be projected reliably: the
  state cache makes later branches much cheaper than earlier ones.
- `sampled`: auto handed off to sampling, or a sampling strategy was selected.
- `timed_model_sampled`: as above, for a timed model.

When auto abandons exhaustive search, text and `auto_handoffs` in JSON report the
reason and point of the handoff for each worker: its completed schedules (including
interleaved samples) and exhaustive choices. Reasons distinguish resource limits,
a large frontier that is not converging, a depth bound, non-repeatable execution,
and unverified liveness. Giving a parked search more time runs more samples; it
does not resume exhaustive search. Temporary sampling probes are not handoffs.

For these, the output shows where schedules branch, averaged per schedule: the
scheduling points with a choice, the machines ready to run at them, and the
`$`/`choose` choices each machine made (`branching` in JSON; `-v` prints it for every
test case). It also notes when most schedules reach the same few states
(`distinct_states` in JSON). Reduce the top machines first; see `snappea guide
modeling`. A timeout or early stop means the search is
incomplete, even if it found no bugs. A missing required coverage
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

Schedule counts depend on the order the search tries alternatives, which the seed
sets. Without `--seed` each run picks a new seed, so `all N schedules` differs between
runs of the same model; the number of distinct states it reached (`distinct_states` in
JSON) does not, for a given worker count. With `--seed` and `-j 1`, a search that ends
before its time limit repeats its counts exactly. A time limit, the shared budget and
several workers make counts depend on timing; under the shared budget, which test
cases run together also changes each one's share. Paste `distinct_states`, or pass
`--seed` and `-j 1`, when a document needs a count readers can reproduce.
`-v` shows progress and statistics and uses one worker; its
`verbose.trace.json` keeps the first 500 schedules.

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
  = help: give it a value of the expected type, convert it (`e to int`, `n to float`, or `v as T` for an `any` value), or change the declared type
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
with every uncovered entry, and the mutation testing results under `mutation`. The
top-level `summary` says first what happened: `status` is `tests_failed`,
`coverage_failed`, `mutation_failed` (with `--deny mutation`), `search_incomplete` or
`passed`, with counts for each outcome. `summary.uncovered` lists up to five required
sites that never ran (`uncovered_more` counts the rest). `mutants_undetected` and
`mutants_not_tested` count mutants no test case caught, and mutants left untested when
mutation testing reached its time limit, which does not change `status`. `skipped`
lists checks a pass did not include (`--no-mutation`, `--warn coverage`), and `note`
says in a sentence what a pass did not cover and what to run next.
`--format text|json` or `--json` chooses, as does `SNAPPEA_FORMAT=text|json`, and
`--json-report FILE` also writes the JSON to `FILE`. Every `snappea` command takes
these options, before or after the command (for example, `snappea --json check`).
Argument errors, including unknown options and missing values, also produce a JSON
error result and write `--json-report` when requested. JSON keys retain insertion order.

### Bug reports

A bug's report says what failed. `snappea-compat` prints upstream's text instead.

* An assertion: `Assertion Failed: <file:line:col> <message>`. `format`, `print`, assertion
  messages and the payloads in `bug.txt` show enum values by name (`Done`, not `2`).
* A runtime error, such as reading a missing map key, an index out of range, or sending
  to a null target: what went wrong in P terms, the machine and state, the source with
  the failing expression underlined, and a fix:

  ```
  error: map 'carts' has no key 1 (in Store(2), state 'Open')
    --> PSrc/Store.p:25:9
     |
  25 |   total = carts[cart] + 1;
     |           ^^^^^^^^^^^
     |
     = help: test `cart in carts` first, or add the key before reading it
  ```

* A liveness bug: the spec still in a hot state when the execution ended, then its
  fields, which show what it was still waiting for:

  ```
  Lifecycle detected liveness bug in hot state 'Running' at the end of program execution.
  Lifecycle's fields: open = {3}, closed = 0
  ```

* A deadlock: the machines waiting to receive, and each spec in a hot state with its
  fields. Match tests on the spec rather than on machine ids, which depend on creation
  order:

  ```
  Deadlock detected. RacyConsumer(3) is waiting to receive an event, but no other controlled
  tasks are enabled, while spec TakeBlocksOnlyWhenEmpty is in hot state 'ItemsOutstanding'
  (wants = {1: 1}, queued = [10], sleeping = {1}, outstanding = 1).
  ```

The JSON result has the same text in each failed test case's `bug.report`.

## Coverage

The [coverage guide](https://snappea.dev/docs/coverage.html) explains the types of
coverage and how to use the gaps. In addition to statements, branches and handlers,
checks measure atomic condition outcomes in `if`/`while` decisions and consecutive
state-transition pairs on a single implementation-machine instance (not spec monitors).
Text output lists required gaps first, then informational condition gaps. Missing
transition pairs are summarized in one line; use `-v` for details or `--json` for
the complete list. These informational gaps do not fail the coverage gate. Structural transition pairs may be
unreachable because of guards or data constraints. Loaded components outside the
selected tests are listed separately, excluded from totals.

JSON adds `conditions`, `transition_pairs`, and `excluded_components` under the
suite's `coverage`; each test and observed spec has the first two counts as well.

Coverage counts what the explored schedules ran, in the machines and monitors each test
case includes: statements, both outcomes of each `if`, and the `do` and `goto` event
handlers of each state. Code that starts with `assert false` must never run, so it is
not counted, nor are functions only assertion messages call (such as a helper that names
a value for a message), which run only when an assertion fails. A bug stops a test case early, so its coverage is marked partial in
the console and with `coverage.partial: true` in JSON. The combined total includes
these results and is also marked partial when any test case finds a bug.

Scenario monitors (`scenario` declarations) appear in the console as a unified summary,
including scenarios that were never satisfied:

```text
Unified scenario coverage across 1 test case:
  1/2 scenarios covered in >=1 test case; 1 coverage gap.
  [covered] ConflictThenRetry: covered in 1/1 test cases (tcTwoClients); 53 total triggers, 6 satisfying timelines (summed per test case)
  [  GAP  ] RefusedTwice: never covered in any of 1 test case; best progress anywhere 2/3 states
```

- **total triggers**: explored schedules that satisfied the scenario (entered a `cold`
  state other than its start), summed over the test cases.
- **satisfying timelines**: distinct timelines among those schedules, summed over the
  test cases. A timeline abstracts a schedule by the order in which each machine received
  its events (`--timeline-repr`), so schedules that differ only in unrelated
  interleavings count once.
- **best progress anywhere N/M states**: the most distinct states of the scenario that
  one schedule entered, out of its M states.

Scenarios watch every test case without being selected with `assert ... in`, so they
are not listed as outside the selected tests. In `snappea check` a scenario ignores an
observed event its state does not handle; `snappea-compat` fails as upstream P does.
A scenario no selected test case satisfies is a required coverage gap: it is listed
under `Not covered`, counted in `summary.coverage_gaps` (and `summary.scenario_gaps`),
and fails the check. Accept an intended one with
`// snappea-lint: allow coverage -- reason` before the scenario. `snappea guide
scenario-monitors` shows how to write them.

JSON includes the scenario artifacts under `scenarios`
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
validated with the other scoped lint directives. A suppression before an `if` or `while`
also covers its condition outcomes. They remain in the raw coverage totals,
while `coverage.required` in JSON shows post-suppression counts.

Suppressed sites that never ran are listed under their own heading, each with its
reason, not as plain uncovered code:

```text
Not covered, suppressed:
  PTst/Tests.p:129  SwapHolder.Forwarding, on eLoadTop: never handled (suppressed: used only by tcBugPointerOnlyCas)
```

In JSON they are under `coverage.uncovered_suppressed`; `coverage.uncovered` holds the rest.

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
Mutation testing: 19 of 39 mutants detected (1.8s)
Not detected by any test case (every test case still passes with the change):
  PSrc/Inventory.p:24  Inventory.Init entry: removed `stock = units;`
  PSrc/Inventory.p:32  Inventory.Serving, on eReserveReq: `req.units <= stock` → `req.units < stock`
  PSrc/Inventory.p:35  Inventory.Serving, on eReserveReq: `false` → `true`
  PSrc/Inventory.p:49  Inventory.Serving, on eReleaseReq: removed the whole `if (req.order in reservations)` (and 4 more undetected inside it)
  ...
```

Here the specs catch overselling and an order that never gets an answer, but nothing
else: an inventory that starts with no stock, refuses an order that fits exactly,
reports a refusal as a success, or loses stock on release passes every test case. Each
line is a choice: add a spec or an assertion that the change would break (an assertion
that a refusal is justified, or that an order's answer matches the reservation), add a
test case that drives the code into the situation, or decide the code does not matter.

A mutant changes one place in the code of the machines under test:

| Mutation | Example |
| --- | --- |
| `remove-statement` | removes an assignment, collection update, `send`, `raise`, `announce`, `goto`, `new` or function call, or a whole `if`, `while` or `foreach` |
| `empty-body` | removes every statement of a function or handler body but its `return`s |
| `condition-true`, `condition-false` | replaces an `if` condition by `true` or `false` |
| `swap-operator` | `<` and `<=`, `>` and `>=`, `==` and `!=`, `+` and `-`, `&&` and `\|\|` |
| `drop-conjunct` | `a && b` becomes `a` or `b`, in separate mutants |
| `drop-disjunct` | `a \|\| b` becomes `a` or `b`, in separate mutants |
| `remove-not` | `!e` becomes `e` |
| `flip-boolean` | `true` becomes `false` and back |

Global functions declared under `PTst/` are test scaffolding and are not mutated,
even when design code calls them. Production helpers they call outside `PTst/`
remain eligible for mutation. Directory matching is case-insensitive.

Spec monitors and the [drivers](#driver-limits) are the tests, so they are not mutated,
and neither are assertions, `print` or format strings. Conditions on a choice (`$`,
`$$`, `choose`) are not forced, swapped or negated: the checker already explores every
choice, so that only relabels the branches. Only code a passing test case ran
is mutated (coverage lists the rest), and an `if` is forced only to the outcome a test
case did not take: forcing a condition that was always true changes nothing.

Each mutant runs only the test cases that ran the changed code, in turns, smallest
first: the test cases whose unchanged search was complete in the fewest schedules, then
the rest. First every mutant gets a turn of 10 schedules on each of its test cases:
most mutants a test case detects at all, it detects within a few schedules, so short
first turns find the test case that detects them sooner. A test case whose unchanged
search was complete in at most 1000 schedules instead gets one turn of up to 1000,
enough for the mutant's search to complete too, so its verdict there is exact. Then the
undetected ones get more turns, round robin over their test cases, each three times
longer than the last on that test case (at most 100 schedules), up to a budget per test
case learned from the first turns: 3 times the schedules that 95% of the detected
mutants needed, at least 100 and at most 1000 (1000 when fewer than 20 were detected).
Each test case gets the whole budget, so a large test case does not starve a small one
that would detect the mutant. When a turn's exhaustive search pruned a cycle while a
liveness monitor was hot without settling whether the cycle can repeat forever, the
mutant's later turns on that test case run fair schedules, which check hot monitors'
temperature, so a mutant that loops without progress is found in a short turn. JSON gives it as `schedules_per_mutant`. `snappea check
mutation` then keeps giving undetected mutants turns, up to 1000 schedules per test
case, until its time limit: a deeper search for a CI gate, where `snappea check` stops
at the budget. When every undetected mutant's search was incomplete, the console says
so with a hint to run `snappea check mutation` before adding assertions or
suppressions, and JSON has `deeper_search_may_detect: true`.

Removing a whole `if`, `while`, `foreach` or body is a *coarse* mutant: the other
mutants inside it change part of what it removes. A mutant inside one gets its first
turns with the others, but the rest of its budget only after every mutant outside an
undetected coarse mutant had its own. When no test case detects the coarse mutant, no
test case notices that code removed at all, which is the gap to close first, so the
undetected mutants inside it are listed with it: the console shows
`removed the whole body (3 statements) (and 2 more undetected inside it; -v lists them)`,
`-v` lists them under it, and JSON lists them under the coarse mutant's
`undetected_inside`, each with its `file` and `line`. Given the time, every mutant
gets its full budget; when the time limit stops the run, what it cut first is the
confirmation of mutants inside a gap already found. The
seed is fixed, so a mutant gets the same verdict on every run and machine. A mutant can survive
because no test case reached the failing schedule within its schedules;
`search_complete: true` in JSON means every one of its test cases explored all of its
schedules, so no test case can detect it as written. Some mutants cannot be detected by
any test: `x < n` and `x <= n` behave the same if `x` never equals `n`.

Some changes do nothing in most states, so a mutant runs only the test cases where it
would change a value. While the test cases check the unchanged model, they record, for
each such mutant, whether its change would have changed a value: for `<`, `<=`, `>` and
`>=`, whether the operands were ever equal; for `+` and `-`, whether the right operand
was ever nonzero; for a removed assignment, whether it ever stored a value other than
the one already there (unless computing the value or where it goes has an effect: a
`new`, a choice, or a call to a foreign function or to a function that sends, raises,
announces, changes state, creates a machine, writes a field or makes a choice; removing
that removes the effect, so the mutant always runs); for a removed `+=` or `-=` on a set or map, whether the element
was ever missing or present. A mutant whose change never would is not run: it is
undetected, with 0 schedules, and the console says why, for example
`` `c > 5` → `c >= 5` (never changes a value in the test cases: the operands are never
equal) ``. In JSON it has `changes_a_value: false`. Like coverage, this covers the
schedules the test cases explored, not every schedule: add a scenario that reaches
such a state, and the mutant runs. On Raft, about one surviving mutant in ten never
changes a value.

A mutant whose change only writes a variable no check depends on (the
[`unchecked-state`](linting.md#unchecked-state-and-events) analysis: no assertion,
condition, spec or sent event reads it, directly or through other variables) is not
run either: no test case can notice it. The console says `(only writes 'log', which no
check depends on: see the unchecked-state lint)` and JSON names the variable as
`unchecked_variable`. Only changes inside a plain assignment, append or map write whose
values involve no call, `new`, choice or operation that can fail are skipped.

Some mutants no test case can detect, as the code shows. They are not run: the console
counts them as `equivalent (not run)`, and JSON lists them under `mutation.equivalent`
with a `reason`:

- removing an `if` with no `else` whose condition is a bare `$` or `choose()` (or its
  negation): the checker already explores the run that skips the branch, as in fault
  injection like `if ($) { return; }`;
- swapping `>` and `>=` (or `<` and `<=`) in a guard whose branch only assigns one
  compared value to the other, `if (a > b) { b = a; }`: at equality it assigns the value
  already there;
- swapping `+` and `-` in the one increment `f = f + 1` of an `int` field whose values
  are only copied (into variables, named tuple fields, parameters and event payloads),
  compared with `==` or `!=` against each other or 0, and reset to 0: a version, tag or
  epoch. Counting down gives values as new as counting up.

One more kind runs, and if no test case detects it, it is reported as likely equivalent
instead of undetected (`Likely equivalent, not counted as undetected:` in the console,
`mutation.likely_equivalent` in JSON), so `--deny mutation` does not fail on it. The
rule: the mutant empties a `do` handler, removes its one statement, or empties a
function that is only ever the one statement of such handlers; the function runs nowhere
else; and every event the handler handles is optional in every test case that ran the
mutant. An event is optional in a test case when every `send` or `raise` of it in the
test case's machines is the whole branch of an `if` on a bare `$` or `choose()`, such
as a crash injected with `if ($) { send server, eCrash; }`. A handler that does nothing
is then the history where the fault never happened. A test case that always sends the
event, a crash in every run, does not qualify.

When an undetected mutant changes code under a `$` or `choose()` condition, the console
adds a hint: code like that is often fault injection in a machine that stands in for the
environment, which should be marked `// snappea: driver` so it is not mutated. `snappea
guide faults` has recipes for timers, crashes and lossy networks as drivers.

A mutant that keeps the model from finishing is stopped early. Before the mutants run,
each test case runs 100 schedules of the unchanged model. When at most 40% of those
reach the step bound and a mutant's do far more often, the mutant stops and counts as
detected by the step bound: at least 90% of at least 10 schedules, or sooner, its first
schedules all at the bound, as many in a row as the unchanged model would reach less
than 1% of the time (2 when it never does). Such a change, like an increment
turned into a decrement in a retry loop, breaks the model too badly to be a useful
finding, and running it out costs more than every other mutant. The console adds one
line, `3 of them because nearly every schedule ran to the step bound: the change made
the model loop without finishing.`, and JSON has the count as `detected_by_step_bound`.
A liveness spec (a hot state the loop never leaves) reports such a loop as a bug
instead, often on the first schedule that closes the loop. A model whose schedules mostly reach the
bound anyway, such as a server that never stops, has none. Those 100 schedules also set
each test case's step bound for its mutants: 10 times the longest of them that ended,
with the liveness threshold scaled to match, when the unchanged model still passes with
them. A mutant that loops then stops at that
bound in milliseconds instead of running to the full bound. The same 100 schedules
bound what a mutant's schedule may do in other ways, each at 10 times the most the
unchanged model did: the work of one step (a `while` whose counter no longer moves),
the machines created (a loop of `new` that never ends) and the events queued for one
machine (a loop of `send` that never ends). A mutant past one of them fails, so a test
case detects it, in milliseconds rather than the seconds or minutes each of its
schedules would take as the machines or events pile up.

`snappea check mutation [SOURCE...] [TEST...]` runs mutation testing alone: it runs
the test cases, then the mutants, and reports neither coverage nor scenarios. It has a
5 minute time limit by default and exits with 1 when a mutant goes undetected (or a
test case fails), so CI can require every mutant to be detected or suppressed. A test
case named `lint` or `mutation` is selected with a glob (`snappea check 'mutation*'`).

In `snappea check`, mutation testing stops at its time limit, 30 seconds by default
(`--mutation-timeout 2m` changes it, `--no-mutation` skips it). Because the schedules
go round robin, a run the time limit stops has given every mutant about the same share:
on a large model, each may get only its first turns. Undetected mutants with fewer
schedules than the budget are marked `(only 60 schedules)` in the console and have a smaller
`schedules` in JSON; raise the limit for a stronger result. Mutation testing is skipped
when a test case fails: fix the bug first. Undetected mutants do not change the exit
status unless `--deny mutation` is given. The JSON result has every undetected mutant under `mutation.undetected`, with
its `file`, `line`, `context`, `mutation`, `change`, `test_cases`, `schedules`,
`search_complete`, `changes_a_value`, `unchecked_variable` and, for a coarse mutant, the undetected mutants inside it
(`undetected_inside`); `undetected_inside_undetected` counts those.

### Suppressing a mutant

A suppression applies only to mutations directly on the following statement or
declaration. Mutations in nested statements remain active. Opt into a nested scope
with `allow mutation-nested:<kind>` (or blanket `mutation-nested`), and give a reason.
Use `--list-suppressions` to inspect every suppressed mutant and stale directive;
JSON includes `suppressed_details` and `stale_suppressions` alongside the count.


When a change cannot or need not be detected, say so with the
[lint suppression comment](linting.md#suppress-intentional-findings), on its own line
before the statement, handler, function or machine:

```p
// snappea-lint: allow mutation:swap-operator -- units never equals stock in any test
if (req.units <= stock && !(req.order in reservations)) {
```

`mutation` allows every mutant in the code that follows; `mutation:<kind>` allows one
kind (the IDs in the table above), and `mutation:<kind>,<kind>` several, as in
`allow mutation:remove-statement,condition-false -- reason`.

Suppression is the right answer for best-effort operations: cancelling a timer, warming
a cache, sending a hint. The design must be correct with or without them, so no test
case can detect their removal, and the reason says so. Suppressed mutants do not run and are counted as
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
| `order` | `no Client sends eAcquire after eRelease`; `User sends eRegister once, before everything else; no schedule runs without it` |
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

A passing test case whose search explored every schedule, and whose schedules reach
few states beyond one run (at most 3 distinct states per step of the longest
schedule), gets a note:

```
ok    tcServe (all 5 schedules, 0.00s)
      note: every schedule reaches nearly the same states, so this test checks little more than one run would
      hint: give the model something to get wrong: a second client, concurrent requests, a failure, or a choice
```

The model has nearly one behavior under this test, so the search has little to find:
one client sending one request is a unit test, not a concurrency check. JSON has
`little_to_explore: true` on the test case. The note never changes the exit status.

For installation and a first model, see the [command line guide](https://snappea.dev/docs/cli.html).
For checking application events and logs, see [snappea observe](observe.md).

### Inspect a failing execution

Add `--inspect-trace` to `snappea check` to include
source statements, machine fields, active-frame locals, and recorded writes in
the failing execution's `bug.trace.json`. For example:

```sh
snappea check Model.p tcMain --inspect-trace
```

The Playground uses these captures for its walkthrough and variable history.
Captures are taken while replaying the first failing schedule, so exploration
uses the same model and scheduler. The normal trace format is unchanged without
this option. Captures stop after 10,000 snapshots; long value previews are marked
with an ellipsis. These snapshots describe the model execution, not application
runtime state.
