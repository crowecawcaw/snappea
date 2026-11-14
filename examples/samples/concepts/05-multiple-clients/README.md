---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I start several clients with ids in a test driver?
  - How does a spec track each client separately?
  - How many clients does a test need?
---

# Multiple clients: contention on a shared server

Most bugs worth modeling need two actors that touch the same state at the same time.
Start several clients in a loop, give each an **id**, and let the spec keep
**per-client state in a map keyed by that id**: with interleaved clients, one shared
"current request" variable in the spec would be overwritten by the next client.

This sample is a permit server (a semaphore): at most `capacity` clients hold a permit
at once. Each client asks for one, and if it gets it, gives it back.

**Contract:** at most `capacity` clients hold a permit at once; a client is refused
only when all permits are held; only a holder releases; every client finishes, says
truthfully whether it got a permit, and gives back any permit it got.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/Permits.p`](PSrc/Permits.p) | The design: `PermitServer` and `Client` |
| [`PSpec/PermitsRespected.p`](PSpec/PermitsRespected.p) | The contract, with a `map[int, Phase]` of clients |
| [`PTst/Tests.p`](PTst/Tests.p) | `StartClients(capacity, clients)` and two test cases |

## The key lines

The driver starts clients in a loop. It announces the plan and each client's id
before creating it, so the spec knows who should finish, even if the client code is
broken:

```p
fun StartClients(capacity: int, clients: int) {
  var server: PermitServer;
  var id: int;
  server = new PermitServer(capacity);
  announce ClientsPlanned, (capacity = capacity, clients = clients);
  id = 1;
  while (id <= clients) {
    announce ClientStarted, (id = id,);
    new Client((server = server, id = id));
    id = id + 1;
  }
}
```

The spec keeps one entry per client and checks each event against that client's own
phase. A shared counter (`holding`) carries the cross-client rule:

```p
var phase: map[int, Phase];   // Asking, Holding, Released, Refused
...
on Granted do (g: (id: int)) {
  InPhase(g.id, Asking);
  holding = holding + 1;
  assert holding <= capacity,
    format("client {0} got a permit: {1} clients now hold one, capacity {2}", g.id, holding, capacity);
  phase[g.id] = Holding;
}
on Denied do (d: (id: int)) {
  InPhase(d.id, Asking);
  assert holding == capacity,
    format("client {0} was refused while only {1} of {2} permits were held", d.id, holding, capacity);
  phase[d.id] = Refused;
}
```

The refusal check matters as much as the limit: a server that refuses everyone never
exceeds capacity.

## How many clients?

Use the **smallest number that crosses each limit, plus nothing**: one more client than
permits. Two permits need three clients for anyone to be refused; a lock (one permit)
needs two. Each extra client multiplies the orders the checker must try:

| Test | Clients | Schedules (exhaustive) |
| --- | --- | --- |
| `tcTwoClientsOnePermit` | 2 | 75 |
| `tcThreeClientsTwoPermits` | 3 | 2,493 |
| same, with a 4th client | 4 | 44,585 (12 s) |

Here the 4th client only repeats situations three clients already reach (full,
refused, freed and reused), at 18 times the cost. Add clients only when the design has a new
limit to cross, not for "more realism".

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcThreeClientsTwoPermits` | 3 clients, 2 permits | Refusal when full; a permit released before the third client asks must be reusable | passes, all 2,493 schedules |
| `tcTwoClientsOnePermit` | 2 clients, 1 permit | Boundary: the capacity-1 case, mutual exclusion | passes, all 75 schedules |

Schedule counts, timings and report excerpts below are illustrative. Use the current
run's completion status to decide whether it explored all schedules.

## Run it

```sh
cd examples/samples/concepts/05-multiple-clients
snappea check --deny mutation
```

```text
ok    tcThreeClientsTwoPermits (all 2493 schedules, 0.57s)
      Driver limits (held in all 2493 schedules):
        creates 1 PermitServer and 3 Client, never more
        PermitServer is always created with 2
        Client is only created with id = 1, 2 or 3
ok    tcTwoClientsOnePermit (all 75 schedules, 0.01s)
Coverage of all test cases: 3/3 states, 48/48 statements, 6/6 branches, 9/9 event handlers, ...
Mutation testing: 25 of 25 mutants detected (0.1s)
```

## Common mistake: forgetting the end of each client's story

The first version of the spec left its hot state once every client had *finished*,
and ignored releases after that. A server that never frees a permit, and a client
that never gives one back, both passed:

```text
Mutation testing: 21 of 25 mutants detected (0.9s)
Not detected by any test case (every test case still passes with the change):
  PSrc/Permits.p:27  PermitServer.Serving, on Release: removed the whole body (2 statements)
  PSrc/Permits.p:45  Client.Acquiring, on AcquireResponse: removed the whole `if (r.ok)` ...
```

The fix is in `Settle()`: stay hot until every client finished **and** `holding == 0`.
For per-client specs, check that each client reaches an end state of its protocol,
not only that it reported something.

## Try this

Off by one in the server: change `sizeof(holders) < capacity` to `<=`. Both tests fail
on their first schedule:

```text
FAIL  tcThreeClientsTwoPermits (bug found after 1 schedule, 0.00s)
      Assertion Failed: PSpec/PermitsRespected.p:39:7 client 2 got a permit: 3 clients now hold one, capacity 2
```

## Bounds

Up to three clients, one acquire per client, no retries after a refusal, no client
crashes while holding a permit (a real semaphore needs leases or timeouts for that).

## In your implementation

Emit `Granted`, `Denied` and `Released` from the server where it updates its holder
set (inside the lock or transaction), with the client id. Start the clients in a test
with ids, and force contention by holding the first two permits until the third
client has been refused.
