---
diataxis: reference
personas: [application-developer, coding-agent]
questions:
  - Which sample should I copy for the design I am modeling?
  - Where is a small example of one P concept?
  - What do common distributed-design bugs look like as failing checks?
  - How are the samples checked?
---

# P model samples

Reference models to read and copy. Each folder is one P project with a README that
states the contract, the tests and why they matter, the bounds, and how the specs
carry over to tests of a real implementation.

- **[concepts/](concepts/)**: one P concept each, small. Start here if P is new.
- **[use-cases/](use-cases/)**: realistic designs with several clients, safety and
  progress specs, and tests for the edge cases that matter.
- **[flawed/](flawed/)**: a plausible design with a deliberate flaw, the failing
  test that finds it, and the fix.

Every sample passes lint, coverage and mutation testing, and every test marked
`// expect-bug:` fails with the named message. CI runs
[`check_samples.py`](check_samples.py); see [AUTHORING.md](AUTHORING.md) to add one.

```sh
cargo build -p snappea --bin snappea
python3 examples/samples/check_samples.py use-cases/seat-hold
```

## Concepts

| Sample | Teaches |
| --- | --- |
| [01-first-model](concepts/01-first-model/) | Machines, events, a spec and a test case |
| [02-safety-spec](concepts/02-safety-spec/) | Specs that rebuild the expected state from events |
| [03-liveness](concepts/03-liveness/) | Hot states: every request is eventually answered |
| [04-nondeterminism](concepts/04-nondeterminism/) | `$` and `choose`: let the checker pick |
| [05-multiple-clients](concepts/05-multiple-clients/) | Contention on a shared server |
| [06-tests-and-modules](concepts/06-tests-and-modules/) | Test declarations, modules, drivers |
| [07-receive](concepts/07-receive/) | Blocking `receive` versus state handlers |
| [08-defer-and-ignore](concepts/08-defer-and-ignore/) | `defer`, `ignore`, unhandled events |
| [09-timers](concepts/09-timers/) | A timeout racing the reply |
| [10-crash-and-restart](concepts/10-crash-and-restart/) | What survives a crash |
| [11-unreliable-network](concepts/11-unreliable-network/) | Loss, duplicates and reordering |
| [12-scenarios](concepts/12-scenarios/) | Scenario monitors prove a test reached the race |
| [13-data-types](concepts/13-data-types/) | Tuples, maps, sets, sequences, enums |
| [14-control-flow](concepts/14-control-flow/) | States, `goto`, entry and exit |
| [15-mutation-driven-specs](concepts/15-mutation-driven-specs/) | Using undetected mutants to strengthen a spec |
| [16-observe-traces](concepts/16-observe-traces/) | One spec for the model and for real logs |
| [17-time-budgets](concepts/17-time-budgets/) | Retries inside a deadline |

## Use cases

| Sample | Design | Race or edge case it checks |
| --- | --- | --- |
| [seat-hold](use-cases/seat-hold/) | Seat holds that expire during payment | Expiry between payment and confirm; stale timer or cancel |
| [rate-limiter](use-cases/rate-limiter/) | Fixed-window limiter with CAS | Lost CAS, exactly at capacity, window rollover |
| [lease-lock-fencing](use-cases/lease-lock-fencing/) | Lease lock with fencing tokens | Paused holder writes after its lease expired |
| [idempotent-payments](use-cases/idempotent-payments/) | Payment API with idempotency keys | Concurrent retries of one key |
| [travel-saga](use-cases/travel-saga/) | Booking saga with compensation | Cancel while a step is in flight |
| [transactional-outbox](use-cases/transactional-outbox/) | Outbox relay | Relay crash and replacement; missed wake-up |
| [offline-sync](use-cases/offline-sync/) | Offline note sync with base versions | Concurrent edits from two devices |
| [circuit-breaker](use-cases/circuit-breaker/) | Circuit breaker | Concurrent probes in half-open |
| [elevator](use-cases/elevator/) | Elevator controller, door and motor | Never move with the door open; obstructed door |
| [config-rollout](use-cases/config-rollout/) | Staged rollout with canary | Rollback during the canary |
| [worker-drain](use-cases/worker-drain/) | Worker drain on scale-in | Work arriving while draining |
| [cache-invalidation](use-cases/cache-invalidation/) | Look-aside cache with lease fills | Stale fill after an invalidation |
| [leader-election](use-cases/leader-election/) | Term-based election | Two candidates in one term |
| [job-queue](use-cases/job-queue/) | Queue with visibility timeouts | Slow worker finishes after redelivery |
| [quorum-kv](use-cases/quorum-kv/) | Quorum-replicated key-value store | Overlapping reads and writes |
| [chat-sequencing](use-cases/chat-sequencing/) | Chat message ordering | Per-room order across senders |
| [order-cancel-vs-ship](use-cases/order-cancel-vs-ship/) | Order cancel versus warehouse ship | Cancel between pick and ship; capture retry |
| [online-migration](use-cases/online-migration/) | Dual-write, backfill, cutover | Backfill racing writers and deletes |
| [consumer-group-rebalance](use-cases/consumer-group-rebalance/) | Consumer group with generations | Zombie consumer commits after rebalance |
| [token-refresh](use-cases/token-refresh/) | Rotating refresh tokens across tabs | Both tabs refresh; lost refresh response |

## Flawed designs

| Sample | Flaw | Difficulty |
| --- | --- | --- |
| [lost-update](flawed/lost-update/) | Read-modify-write of a shared balance | Easy |
| [in-flight-duplicate](flawed/in-flight-duplicate/) | Idempotency key recorded after the side effect | Easy |
| [lock-order-deadlock](flawed/lock-order-deadlock/) | Locks taken in argument order | Easy |
| [lease-without-fencing](flawed/lease-without-fencing/) | Old leader writes after its lease expired | Medium |
| [minority-quorum](flawed/minority-quorum/) | `n/2` votes elect two leaders | Medium |
| [stale-replica-read](flawed/stale-replica-read/) | Reads from asynchronous replicas | Medium |
| [dual-write-crash](flawed/dual-write-crash/) | Crash between commit and publish | Medium |
| [lost-wakeup](flawed/lost-wakeup/) | Check-then-sleep in a blocking queue | Hard |
| [aba-cas](flawed/aba-cas/) | ABA in a lock-free stack | Hard |
| [write-skew](flawed/write-skew/) | Write skew under snapshot isolation | Hard |
