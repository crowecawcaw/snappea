# Model a design with P, then test the code against it

P finds bugs that only appear in some orderings: two clients read the same version,
a retry arrives after the state changed, a step fails between read and write. Use it
for shared state, retries, concurrency and recovery. Pure transformations need only
ordinary tests.

The model is worth what it changes in the code and tests. Follow these steps in order.
More: `snappea guide testing` (testing the implementation), `snappea guide syntax`
(P syntax and a complete small model).

## 1. List the risks (before any code)

Write down, in DESIGN.md:

- the contract in checkable sentences ("a retried request returns its first result and
  has no further effect");
- the real atomic primitives (one compare-and-swap on one key, a lock, a transaction).
  Everything else can interleave;
- 5-10 concrete scenarios that could break the contract. Always consider: two clients
  that read the same version; a retry after another request changed the state
  (including retries of rejected or empty results); two objects sharing one store;
  each precondition false on its own; values exactly at each boundary.

## 2. Model the algorithm, not the API

- One machine for the store, holding exactly what the real store holds. Its handlers
  are the real primitives: `load` replies with (version, value); `cas` succeeds only at
  the current version.
- Client machines run your algorithm: send load, receive, decide, send cas, retry on
  failure. Every `send`/`receive` is a point where another client can run. If the
  whole operation is one handler, the model assumes the race away.
- A `spec` monitor computes the expected results from the contract and the history of
  requests, independently of the algorithm, and asserts each actual result. Announce
  events at the store's commit, not after a reply, or the monitor sees them out of order.
- Drive it with 2-3 clients and small `choose(n)` inputs, plus fixed tests for the
  scenarios from step 1.

## 3. Check until it passes for real

    snappea check            # every test case, then coverage and mutation checks

- Exit 1 is work to do: a bug (fix the design; keep the scenario), a coverage gap (add
  a scenario), or a surviving mutant (an assertion is missing). Read each one.
- Do not finish with --no-mutation or --allow-incomplete-coverage. Use them only while
  iterating. If search is incomplete, shrink the bounds or raise --timeout.
- A bug found here is the payoff: write it down, fix the design, then implement.

## 4. Implement, then test what the model explored

For each scenario and each bug the model found, write a test that makes it happen on
the real code. Random thread stress rarely hits a race; force it (`snappea guide
testing` shows how):

- wrap the store so a competing call runs between this call's load and its
  compare-and-swap, or so two calls read the same version before either writes;
- retry a request after another request changed the state, through a second object.

Add an observation test: run real calls, record their actual arguments and results as
events, and check them against your P spec. In Python: `pip install pysnappea`, then
`from snappea import check_events`.

## 5. Prove the tests can fail

Break your implementation on purpose, one change at a time (ignore the
compare-and-swap result, drop a retry memo, flip a boundary `<` to `<=`, keep state in
the object instead of the store). Run the tests each time; every break must fail a
test. Add a test for any that pass. Restore the code.

## Done means

- `snappea check` exits 0, with complete search or stated bounds.
- The implementation's tests pass, including forced-interleaving and observation tests.
- Every deliberate break from step 5 fails a test.
- DESIGN.md states the contract, primitives, scenarios, model bounds, what the model
  found, and what is not covered.
