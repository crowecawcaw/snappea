# Answer key: minio-dsync @ 10fd53d6 (2020-03-14)

Never show this file to a solver. Each issue exists in the snapshot and was later fixed
upstream or documented by MinIO as a known deficiency. A report finds an issue when
it describes the mechanism (the cause below) and its consequence; naming the
symptom alone is partial.

## K1: Stale-lock maintenance purges a live lock on one peer's word

- **Code**: `cmd/lock-rest-server.go` `lockMaintenance`; `Expired` in
  `cmd/local-locker.go` / `ExpiredHandler`.
- **Mechanism**: for each long-lived lock entry, maintenance asks every other lock
  server `Expired(uid)`. If *any single* server answers "expired" (it has no entry for
  that UID), the entry is removed locally. Servers that never granted the lock (lost
  the race, were down or slow during acquisition, or dropped it on restart) always
  answer "expired". There is no quorum.
- **Consequence**: a lock held by a live client is purged from enough servers that a
  second writer gets quorum: two writers (or a writer and readers) at once.
- **Fix**: c9212819 "lock maintenance should honor quorum": purge only when fewer than
  a quorum of servers vouch for the lock.

## K2: Maintenance needs a distributed lock to clean up stale locks

- **Code**: `lockMaintenance` takes `objAPI.NewNSLock(ctx, "system",
  "lock-maintenance-ops").GetLock(...)` before doing anything.
- **Mechanism**: the cleanup of stale locks is itself gated on acquiring a dsync
  write lock. If that lock's entries are stale (a maintenance run's owner crashed
  mid-run or its unlock was lost) or too many servers are down/stale, no server can
  run maintenance, so nothing removes the stale entries that block it. Only one server
  runs maintenance at a time.
- **Consequence**: stale locks, including the maintenance lock itself, can stay
  forever: liveness failure / permanent unavailability.
- **Fix**: c9212819 runs maintenance on every server without a distributed lock.

## K3: Unlock is fire-and-forget; no release quorum

- **Code**: `pkg/dsync/drwmutex.go` `Unlock`, `RUnlock`, `unlock`, `sendRelease`,
  `releaseAll`; also release of late grants and of partial grants after a failed attempt.
- **Mechanism**: the client clears its own state and sends release messages without
  checking that they arrived; errors are only logged. A lost release leaves the entry
  on that server. A server that never got the release keeps blocking writers (and,
  for a lost write unlock, readers).
- **Consequence**: after enough lost releases a resource cannot be locked until (if
  ever) maintenance removes the entries; combined with K1/K2/K5 this becomes permanent.
  An unlock with an even number of lockers where only half the releases land leaves
  read locks active.
- **Fix**: 849fcf01 "block unlocks if there are quorum failures" (retry until a
  quorum of releases succeed); effe1310 (split-brain tolerance for RUnlock).
- **Accept**: any report that unlock/release delivery failures are ignored and leave
  locks held on some servers, blocking later lockers.

## K4: A restarted lock server forgets its grants

- **Code**: `localLocker.lockMap` is in memory only (`newLocker`); `drwmutex.go`
  counts grants once and never re-validates them.
- **Mechanism**: a server that granted a write lock crashes and restarts with an empty
  table. A second client can now collect a quorum that overlaps the first only on
  restarted servers. Example (MinIO's README, 8 nodes): 3 nodes down, client A locks
  on nodes 1-5; nodes 4 and 5 crash; nodes 4-8 restart; client B locks on nodes 4-8.
- **Consequence**: two concurrent write locks.
- **Source**: dsync README "Known deficiencies: More than one write lock"; also the
  restriction "If a down node comes up, it will not try to (re)acquire any locks".
- **Accept**: any report that a server restart (losing in-memory lock state) lets a
  second writer reach quorum while the first still holds the lock.

## K5: Expiry checks peers, not the owner's liveness

- **Code**: `lockMaintenance` and `Expired`: the check asks lock servers whether they
  still hold the UID; nobody asks whether the owning client/server is alive (the
  comment says "checks back with the original server", the code does not).
- **Mechanism**: if the lock owner crashes after acquiring, every server that granted
  the lock still has the entry, so each one answers "not expired" for the others.
  The stale entries vouch for each other.
- **Consequence**: a crashed owner's lock is never cleaned up: the resource is
  permanently unlockable (until restarts).
- **Fix**: a4f67058 "expire stale locks when owner is down", eafa7759 "add lock
  ownership to expire locks", d9db7f33.
- **Accept**: any report that a crashed client's lock is never released because the
  staleness check does not detect a dead owner.

## Not issues at this snapshot

- Quorum sizes: write quorum is N/2+1 and read quorum is ceil(N/2), so a write quorum
  intersects every other read or write quorum. A report claiming the quorum
  arithmetic itself allows two writers with all servers up and no restarts is wrong.
- `lock()` in `drwmutex.go` cannot send on a closed channel: every sender goroutine is
  counted in `wg`, the channel is buffered to `len(restClnts)`, and `lock()` returns
  (running `defer close(ch)`) only after `wg.Wait()`. Late grants are released by the
  receiver goroutine. A claim of a send-on-closed-channel panic is invalid.
- `cmd/namespace-lock.go` refcounting holds `lockMapMutex` around every ref change at
  this commit (the atomic refcount race, 6de410a0, came later).

## Valid extras (not scored as known issues)

Classify these as "valid" other claims.

- `localLocker.RLock`/`RUnlock` use only `args.Resources[0]`, so a multi-resource read
  lock registers on the first resource only and a writer on another resource is granted.
  Latent at this commit: the only multi-resource caller (multi-object delete in
  `cmd/xl-zones.go`) takes a write lock. Reworked in 9cdd981c.
- A grant whose reply is lost (RPC error after the server granted) is counted as not
  granted and never released, leaving an orphan entry. Same family as K3 when the report
  also covers lost releases; on its own, grade K3 "partial".
