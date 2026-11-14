---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do I model a look-aside cache racing a writer's invalidation?
  - How do lease tokens stop a stale fill after a delete?
  - How do I state "the cache is never stale" so a checker can test it?
---

# Look-aside cache with lease-protected fills

Readers check the cache; on a miss they read the database and fill the cache. Writers
commit to the database and then delete the cache key. The classic race: a reader reads
the old value, the writer commits and deletes, then the reader fills the cache with the
old value, which stays there until the next write. The fix here is memcache-style
leases: a miss hands out a lease, a delete revokes it, and a fill is stored only with
the outstanding lease.

**Contract** (writes commit increasing values, so "older" means "smaller")

- A cache hit returns a value the database committed at some point not before the last
  completed invalidation, and that value is what the cache was last filled with.
- Read-after-write: a read that starts after a write completed (database write and
  cache delete both acknowledged) returns that write's value or a newer one.
- At quiescence (no client running) the cache is empty or equal to the database, and a
  read-through fills it: the probe's second read is served from the cache.
- Progress: every planned read and write returns.

**Atomic primitives**: database read and write; cache get (hit, or miss plus a new
lease that replaces any earlier one), set-with-lease, delete (also revokes the lease).
A read-through is three of them, so a write and delete can land in between.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `CacheFresh` (safety, including the quiescence probe), `OperationsFinish` (progress, hot state) |
| [`PSrc/Stores.p`](PSrc/Stores.p) | `Database` and the lease-checking `LeaseCache` |
| [`PSrc/Clients.p`](PSrc/Clients.p) | `Reader` (read-through) and `Writer` (write, then delete) |
| [`PSrc/NaiveCache.p`](PSrc/NaiveCache.p) | Negative control: a cache that stores every set |
| [`PTst/Tests.p`](PTst/Tests.p) | The `Workload` driver and the test cases |

## Test cases

| Test | Drives | Why it matters |
| --- | --- | --- |
| `tcStaleFillRace` | One reader (one read) and one writer, then the probe | The core race: a fill computed before the write arrives after the delete |
| `tcTwoReadersOneWriter` | Two readers and one writer, then the probe | A second miss replaces the first lease; hits on a filled cache while a write is in flight |
| `tcGeneratedWorkload` | Readers read once or twice, the writer writes once or twice | Repeated invalidation, leases revoked by either write |
| `tcReadWriteRead` | Fill, then a completed write, then a read, in order | Lifecycle boundary: the entry filled before the write must not be served after it |
| `tcBugNaiveCache` | The core race against `NaiveCache` | **Expected bug**: `cache holds stale value 0 at quiescence; database has 1` |

After the clients finish, the driver announces `Quiesced` and runs a probe reader that
reads twice. That turns "stale indefinitely" into a finite check: a stale entry is
served to the probe, and a cache that refuses every fill never serves the probe's
second read.

## Run

```sh
snappea check tcStaleFillRace tcTwoReadersOneWriter tcGeneratedWorkload tcReadWriteRead --deny mutation
snappea check tcBugNaiveCache      # passes only when the declared failure is found
```

The small stale-fill and lifecycle tests isolate the core races. The generated
and multi-reader tests explore a larger space and may be sampled. Check each
run's `search_complete` field; a passing sampled run is not an exhaustive result.

## Bounds and omissions

One key, up to two readers, one writer with up to two writes, values 1 and 2. Messages
are not lost or reordered between a pair of machines; nothing crashes. A miss always
issues a new lease (memcache instead tells a second concurrent miss to wait); this is
simpler and still safe. Not modeled: cache eviction, lease expiry, failed deletes (the
writer would have to retry), several writers (a reader's lease can still be
revoked by any of them), and multiple keys.

## What building it taught

- Safety alone missed a reader that ignored hits: it read the database every time and
  every check passed. Announcing `fromCache` with each read result, and requiring the
  probe's second read to come from the cache, made "the cache is used" checkable.
- Mirroring the cache's contents in the spec (fills, invalidations, hits, misses)
  caught a dropped `Invalidated` announcement and a miss reported while filled.
- A broad `allow mutation` on the lease counter hid a real mutant (removing the
  increment makes every lease 0, so a revoked lease matches again). Scope suppressions
  to the kind: `allow mutation:swap-operator`.

## Testing the implementation

Emit `DatabaseCommitted` after the database commit, `CacheHit`, `CacheMiss`,
`CacheFilled` and `Invalidated` from the cache client wrapper after each call
returns, and `ReadStarted`/`ReadReturned`/`WriteDone` from the application. To force
the race, block the reader between its database read and its cache set until the
writer's delete has returned, then release it: the set must be refused, and a
following read must miss.
