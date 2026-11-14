---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I start several clients with ids in a test driver?
  - How does a spec track each client separately?
  - How many clients does a test need?
---

# Multiple clients: more clients than permits

Most bugs worth modeling need two actors that touch the same state at the same time.
Here a permit server (a semaphore) lets at most `capacity` clients hold a permit at
once; each client asks for one and, if it gets it, gives it back. With one more client
than permits, someone is refused unless a holder released first, and the spec has to
follow each client separately while their requests interleave.

**Contract:**

- At most `capacity` clients hold a permit at once (`PermitsRespected`).
- A client is refused only when all permits are held (`PermitsRespected`).
- Only a holder releases (`PermitsRespected`).
- Every client finishes, says truthfully whether it got a permit, and gives back any
  permit it got (`PermitsRespected`, hot until then).

**Atomic primitives:** the server's `Acquire` handler checks and updates its holder set
in one step (one critical section), and so does `Release`.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/Permits.p`](PSrc/Permits.p) | The design: `PermitServer` and `Client` |
| [`PSpec/PermitsRespected.p`](PSpec/PermitsRespected.p) | The contract, with a `map[int, Phase]` of clients |
| [`PTst/Tests.p`](PTst/Tests.p) | `StartClients(capacity, clients)` and two test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcThreeClientsTwoPermits` | 3 clients, 2 permits | Refusal when full; a permit released before the third client asks must be reusable | Passes |
| `tcTwoClientsOnePermit` | 2 clients, 1 permit | Boundary: the capacity-1 case, mutual exclusion | Passes |

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

The spec keeps **per-client state in a map keyed by the client id**. With interleaved
clients, one shared "current request" variable would be overwritten by the next
client. A shared counter (`holding`) carries the cross-client rule:

```p
var phase: map[int, Phase];   // Asking, Holding, Returned, Refused
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

Use the **smallest number that crosses each limit**: one more client than permits. Two
permits need three clients for anyone to be refused; a lock (one permit) needs two.
Each extra client multiplies the orders the checker must try. On one machine, the
three-client test explores a few thousand schedules in well under a second; a fourth
client takes it to about forty thousand and several seconds, and only repeats
situations three clients already reach (full, refused, freed and reused). Add clients
only when the design has a new limit to cross, not for "more realism".

## Common mistake: forgetting the end of each client's story

A first version of the spec left its hot state once every client had *finished*, and
ignored releases after that. A server that never frees a permit, and a client that
never gives one back, both passed:

```text
Mutation testing: 21 of 25 mutants detected (0.8s)
Not detected by any test case (every test case still passes with the change):
  PSrc/Permits.p:27  PermitServer.Serving, on Release: removed the whole body (2 statements)
  PSrc/Permits.p:45  Client.Acquiring, on AcquireResponse: removed the whole `if (r.ok)` (and 2 more undetected inside it; -v lists them)
```

The fix is in `Settle()`: stay hot until every client finished **and** `holding == 0`.
For per-client specs, check that each client reaches an end state of its protocol,
not only that it reported something.

## Try this

Off by one in the server: change `sizeof(holders) < capacity` to `<=`. Both tests fail
on their first schedule, for example:

```text
Assertion Failed: PSpec/PermitsRespected.p:39:7 client 3 got a permit: 3 clients now hold one, capacity 2
```

## Run

```sh
cd examples/samples/concepts/05-multiple-clients
snappea check --deny mutation
```

## Bounds and omissions

Up to three clients, one acquire per client, no retries after a refusal, no client
crashes while holding a permit (a real semaphore needs leases or timeouts for that).

## Testing the implementation

Emit `Granted`, `Denied` and `Released` from the server where it updates its holder
set (inside the lock or transaction), with the client id. Start the clients in a test
with ids, and force contention by holding the first two permits until the third
client has been refused.
