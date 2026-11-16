# Answer key: postgres logical-replication sync @ b38006ef6d (2017-04-04)

Never show this to a solver. Every issue below exists at the snapshot and was fixed
later. The solver got the files in `task.json`, not `pg_subscription.c` or
`relation.c`; TASK.md states the upsert and re-read behaviour. Line numbers refer to
the snapshot. A report finds an issue when it explains the mechanism and the consequence;
naming the symptom alone is partial.

## K1: Worker slot "free" test races with concurrent launchers

- **Code**: `launcher.c` `logicalrep_worker_launch` (L233-322): a free slot is one with
  `!workers[slot].proc` (L260). The launcher fills in `subid/relid/dbid` under
  `LogicalRepWorkerLock` and releases the lock **before** the new background worker
  attaches. `proc` is set only later, in `logicalrep_worker_attach` (L446-464), in the
  child process.
- **Mechanism**: several processes launch workers concurrently: the launcher (apply
  workers) and every apply worker (sync workers, `process_syncing_tables_for_apply`
  L398). A slot that one launcher has claimed but whose worker has not attached yet
  still has `proc == NULL`, so a second launcher picks the same slot and overwrites its
  identity. The comment in `WaitForReplicationWorkerAttach` (L174-178, "only launcher
  ever starts the workers, so nobody can steal the worker slot") is false at this commit.
- **Consequence**: two background workers are given the same slot index. One of them
  fails with "slot already used". Or a process starts under the other launcher's
  identity (for example, an apply worker for subscription B runs as a sync worker for
  a table of subscription A), because `ApplyWorkerMain` reads its subid/relid from the
  slot. The intended worker is lost, or a duplicate exists, and waiters watch the wrong
  worker.
- **Fix**: `de43897122` (2017-04-26) "Fix various concurrency issues in logical
  replication worker launching": adds an `in_use` flag that is set under the lock when
  the slot is claimed, and a `generation` counter. The commit says the code assumed the
  launcher was the only process starting workers, "However that hasn't been true since
  commit 7c4f52409".

## K2: A worker that never attaches leaves its slot claimed forever

- **Code**: `logicalrep_worker_launch`: when `RegisterDynamicBackgroundWorker` fails
  (L311-318), it returns without clearing the slot. The result of
  `WaitForReplicationWorkerAttach` (L321) is ignored. When the worker exits before
  attaching (`BGWH_STOPPED`, L182), nothing clears the slot.
  `logicalrep_worker_find(..., only_running=false)` (L206-227) and
  `logicalrep_sync_worker_count` (L509-525) match by `subid/relid` alone. Only
  `logicalrep_worker_detach`, which runs in a worker that did attach, clears `subid`.
- **Mechanism**: a slot is left with `subid/relid` set and `proc == NULL`. Everyone
  treats it as a live worker that is "starting up".
- **Consequence** (liveness):
  - The launcher main loop (L691) finds a "worker" for the subscription and never
    starts the apply worker again.
  - The apply worker finds a "sync worker" for the table, so the table never syncs. The
    stuck slot also counts against `max_sync_workers_per_subscription`.
  - `logicalrep_worker_stop` (L348-383) loops forever. It waits for `proc` to be set or
    for `subid` to be cleared, and neither happens, so DROP SUBSCRIPTION hangs.
- **Fix**: `de43897122` (cleans up on `BGWH_STOPPED`; garbage-collects slots that are
  `in_use` but never attached after `wal_receiver_timeout`). `3e1683d37e` (2017-09-18)
  "Fix, or at least ameliorate, bugs in logicalrep_worker_launch()": the slot was still
  not released when background-worker registration failed, "This led to an indefinite
  hang in subscription startup".
- **Accept**: any report that a failed or aborted worker start leaves a slot that looks
  like a starting worker, and that this blocks restarts, syncs or stop.

## K3: Sync worker waits on itself, not on the apply worker (orphaned sync worker)

- **Code**: `tablesync.c` `wait_for_sync_status_change(relid, origstate)` (L141-179).
  The sync worker calls it with its own relid (L816) after it sets SYNCWAIT (L807-810).
  The loop looks up `logicalrep_worker_find(subid, relid)`, which is the sync worker's
  own slot, and checks whether that slot still exists and whether its state changed.
- **Mechanism**: the sync worker's only exit conditions are its own slot disappearing
  (it never does) or its state changing (only the apply worker changes it). It never
  checks that the apply worker is alive. If the apply worker exits, the sync worker
  waits in SYNCWAIT forever. The apply worker can exit because the subscription is
  disabled (`reread_subscription` exits) or dropped (`DropSubscription` stops only
  relid=0, L793; see K4), or because it errors and the launcher does not restart it.
- **Consequence**: an orphaned sync worker. It keeps its worker slot, an open local
  transaction with the uncommitted copied data, a RowExclusiveLock on the table, and a
  temporary slot plus walsender connection on the publisher. The publisher's temporary
  slot holds back WAL and catalog xmin. After DROP SUBSCRIPTION this lasts forever
  (only a manual SIGTERM ends it).
- **Fix**: `3c9bc2157a` (2017-06-03) "Make tablesync worker exit when apply dies while
  it was waiting for it ... caused by a thinko in wait_for_sync_status_change": the sync
  worker now looks up the apply worker (relid InvalidOid).

## K4: Subscription DDL ignores running sync workers; workers recreate removed state

- **Code**:
  - `subscriptioncmds.c` `AlterSubscription_refresh` (L440-537) calls
    `RemoveSubscriptionRel` (L530) for tables that left the publication and stops no
    worker.
  - `DropSubscription` stops only the apply worker:
    `logicalrep_worker_stop(subid, InvalidOid)` (L793).
  - `pg_subscription.c` `SetSubscriptionRelState` (L219-) is an upsert: it inserts the
    row if missing (L240-259).
  - Workers call it with new states: `tablesync.c` L219, L743, L821; apply-side READY
    at L312.
  - The sync worker treats the whole copy and catch-up as one remote transaction, so it
    never processes invalidations and never re-reads the subscription:
    `AcceptInvalidationMessages`/`reread_subscription` run only when
    `!in_remote_transaction` (`worker.c` L1101-1109), and the wait loop at L141-179
    checks nothing.
- **Mechanism**: the catalog row is deleted, but the worker that owns the table keeps
  running on stale knowledge. When it advances state, the upsert re-inserts the deleted
  row.
- **Consequence**:
  - A table removed by `ALTER SUBSCRIPTION ... REFRESH PUBLICATION` comes back into the
    subscription.
  - `pg_subscription_rel` rows are created for a subscription that DROP has already
    deleted.
  - Sync workers keep copying data for a table or subscription that no longer exists,
    or sit orphaned in SYNCWAIT (with K3).
- **Fixes**:
  - `644ea35fc1` (2017-06-07) "Fix updating of pg_subscription_rel from workers": workers
    may only update, not insert, "so that there are no races if a concurrent refresh
    removes rows".
  - `ddd7b22b22` (2017-06-09) "Stop table sync workers when subscription relation entry
    is removed" ("the worker could be left orphaned").
  - `66b84fa82f` (2017-06-03) "Receive invalidation messages correctly in tablesync
    worker" ("the sync worker didn't learn about subscription changes/drop until it has
    finished").
  - `7e174fa793` (2017-08-04) makes DROP SUBSCRIPTION stop all of the subscription's
    workers (`logicalrep_workers_find`).
- **Accept**: any one of these: refresh or drop leaves sync workers running, workers
  re-insert deleted state rows, or sync workers never notice the subscription
  changed. Full credit needs the mechanism (no stop, upsert, or no invalidation
  processing) and a consequence (orphan or resurrected row).

## K5: Apply resumes on a shared-memory state change before the sync worker commits (lost changes)

- **Code**:
  - `tablesync.c` `process_syncing_tables_for_apply` (L346-388): on SYNCWAIT, the apply
    worker sets CATCHUP in shared memory, then calls
    `wait_for_sync_status_change(relid, CATCHUP)`, which returns as soon as the
    **shared-memory** `relstate` differs.
  - `process_syncing_tables_for_sync` (L200-229): the sync worker first writes
    READY/SYNCDONE into shared memory (L211-215). Only after that does it call
    `SetSubscriptionRelState`, then `finish_sync_worker`, which commits the local
    transaction holding both the copied rows and the catalog update (L117-119).
  - The apply worker decides per relation through `should_apply_changes_for_rel`
    (`worker.c` L135-143), which reads the state from the catalog (`relation.c`
    `logicalrep_rel_open`). Changes are skipped unless the state is READY, or SYNCDONE
    with lsn <= the transaction's LSN.
- **Mechanism**: a window opens after the sync worker publishes READY/SYNCDONE in shared
  memory and before its transaction commits. If the apply worker's wait loop runs in
  that window, it returns. That happens on the 10s timeout, or on any latch set:
  another sync worker's `finish_sync_worker` wakes the apply worker (L125). The apply
  worker resumes streaming. For the next transactions that touch the table, the
  catalog still shows DATASYNC (uncommitted), so the apply worker skips them. The sync
  worker has already stopped at its catch-up LSN and will not apply them either.
- **Consequence**: changes to that table made after the handover point are skipped by
  both workers. The subscriber's copy permanently diverges (lost updates).
- **Fix**: `e3a815d2fa` (2017-06-06) "Improve handover logic between sync and apply
  workers: Make apply busy wait check the catalog instead of shmem state to ensure that
  next transaction will see the expected table synchronization state." It also changes
  the protocol to always go SYNCWAIT -> CATCHUP -> SYNCDONE (in the catalog) -> READY.
  Related: `1f201a818a` reworked the same loop.
- **Accept**: any report that the apply worker's decision to continue is based on
  shared-memory state that runs ahead of the committed catalog state and data, so
  changes are skipped or applied against an inconsistent view.

## K6: Wakeup through a worker pointer after the lock is released

- **Code**:
  - `launcher.c` `logicalrep_worker_wakeup` (L421-431) finds the worker under
    `LogicalRepWorkerLock`, releases the lock, then calls `logicalrep_worker_wakeup_ptr`.
    That function does `SetLatch(&worker->proc->procLatch)` (L437-440).
  - `tablesync.c` `process_syncing_tables_for_apply` releases the lock (L340) and later
    calls `logicalrep_worker_wakeup_ptr(syncworker)` (L382).
- **Mechanism**: after the lock is released, the target can exit. Detach sets
  `proc = NULL` (L470-481), and the slot can then be reused by another worker. The
  sync worker exits on SIGTERM, error or finish, so this is the normal state machine,
  not a corner case.
- **Consequence**: a NULL dereference crashes the apply worker or the exiting sync
  worker. A backend crash makes the postmaster restart the whole server. Otherwise the
  wakeup goes to an unrelated process that now holds the slot, and the intended waiter
  misses the wakeup. The same staleness affects `logicalrep_worker_stop`'s wait loop
  (L390-414), which can end up watching a different worker that reused the slot. The
  generation fix in `de43897122` addresses that part.
- **Fix**: `1f201a818a` (2017-06-30, Tom Lane) "Fix race conditions and missed wakeups
  in syncrep worker signaling": "we can *not* call logicalrep_worker_wakeup_ptr() once
  we've released the LogicalRepWorkerLock, because worker->proc might've been reset to
  NULL ... the LogicalRepWorker slot even belongs to the same worker anymore".

## K7: Origin progress records the commit's start LSN, so a crash replays the last transaction

- **Code**: `worker.c` `apply_handle_begin` sets
  `replorigin_session_origin_lsn = begin_data.final_lsn` (L425). `final_lsn` is the
  start of the remote commit record; `apply_handle_commit` asserts
  `commit_lsn == replorigin_session_origin_lsn` (L446). Local commit stores that value
  as the origin's progress. On restart, `ApplyWorkerMain` streams from
  `replorigin_session_get_progress` (L1540, L1569-1575). Flush feedback to the
  publisher uses `end_lsn` (L456), but it is sent asynchronously.
- **Mechanism**: the durable restart point of a subscriber is the origin progress,
  committed atomically with each applied transaction. Here it points at the start of
  the last applied transaction's commit record, not its end. The subscriber can crash
  after a local commit but before the publisher's slot has confirmed that end LSN
  (feedback is periodic). After the restart it asks for changes from `commit_lsn`, and
  the publisher sends that transaction again.
- **Consequence**: the last transaction is applied twice after a subscriber crash.
  That produces duplicate rows on tables without a key. On keyed tables a
  unique-violation error makes the apply worker fail and restart in a loop, which
  stalls replication.
- **Fix**: `fe7bbc4ddb` (2017-04-04, the commit right after the snapshot) "Fix remote
  position tracking in logical replication": "We need to set the origin remote position
  to end_lsn, not commit_lsn ... If we'd use commit_lsn, we could request data that we
  already received from the remote server after a crash of a downstream server."
- **Grading note**: a solver must know that the requested start position is inclusive
  of a transaction whose commit begins there. Accept a report that says progress is
  recorded at the commit's start, so a crash and restart re-requests the transaction.

## K8: SIGTERM is only a flag, so DROP SUBSCRIPTION can wait forever on a blocked worker

- **Code**:
  - `launcher.c` `logicalrep_worker_sigterm` (L495-502) only sets `got_SIGTERM` and the
    latch, installed at `worker.c` L1443. The flag is checked only at the top of
    `LogicalRepApplyLoop` (L1004), in `copy_read_data`, and in
    `wait_for_sync_status_change`.
  - `DropSubscription` (L691-) takes AccessExclusiveLock on `pg_subscription` (L722) and
    on the subscription object (L758). Both are held to the end of the transaction.
    It then calls `logicalrep_worker_stop` (L793), which busy-waits on a latch
    (L390-414) until the worker's `proc` is cleared.
  - DROP without DROP SLOT is allowed inside a transaction block (only DROP SLOT calls
    `PreventTransactionChain`, L715).
- **Mechanism**: a worker blocked on a heavyweight lock, or in a blocking catalog or
  network call, does not react to SIGTERM. The DROP SUBSCRIPTION transaction may hold
  the lock the worker is waiting for. Two examples:
  - the transaction earlier locked or modified a replicated table;
  - the worker needs `pg_subscription` after a catalog cache miss or invalidation
    (`reread_subscription` -> `GetSubscription`).
  The DROP waits for the worker's exit through a latch polling loop, which the lock
  manager's deadlock detector cannot see.
- **Consequence**: an undetected deadlock: DROP SUBSCRIPTION hangs forever while holding
  AccessExclusiveLock on `pg_subscription`, which also blocks the launcher's catalog
  scan. Even without a cycle, DROP hangs for as long as the worker is blocked.
- **Fix**: `9fcf670c2e` (2017-06-02) "Fix signal handling in logical replication
  workers: ... use the normal die() handler for SIGTERM and CHECK_FOR_INTERRUPTS() ...
  the apply worker would not exit promptly when a subscription was dropped, which could
  lead to deadlocks." A related later symptom of the same "DDL waits for workers inside
  its transaction" design: `8edacab209` (2017-09-17) "Fix DROP SUBSCRIPTION hang"
  (ALTER ... DISABLE plus DROP in one transaction), whose triggering code came in
  `7e174fa793` after the snapshot, so it is not scored.
- **Accept**: any report that stopping a worker relies on a polled flag, so a worker
  blocked on a lock held by the DROP transaction (or by anything else) is never
  stopped and DROP waits forever.

## Not issues at this snapshot

- **SYNCDONE `<=` comparison** in `should_apply_changes_for_rel` (`worker.c` L139-142):
  it is intentional, and the comment above it explains why. The SYNCDONE LSN can equal
  the start of the next commit record. A report of an off-by-one that loses or doubles
  one transaction at the SYNCDONE boundary is wrong.
- **Sync worker crash mid-copy or mid-catch-up does not leave partial data**: the copy
  and catch-up run in one local transaction that is committed only in
  `finish_sync_worker`. DATASYNC is committed separately (L742-747), so a restarted
  sync worker repeats the copy from scratch, and the publisher-side slot is temporary.
  Claims of duplicated or half-copied rows after a sync-worker crash are invalid.
  Lost changes from the handover window are K5, which is a different mechanism.
- **Too many sync workers for one subscription**: only that subscription's apply worker
  launches its sync workers. It counts them under the lock (L339) and launches
  synchronously. Excess sync workers through the count alone is not an issue; slot
  stealing between different launchers is K1.

## Valid extras (not scored)

- `8bbc618b48` (2017-04-20) "Don't call the function that may raise an error while
  holding spinlock": `LogicalRepSyncTableStart` reads the catalog with
  `GetSubscriptionRelState(..., missing_ok=false)` while holding the slot spinlock
  `relmutex` (`tablesync.c` L700-706). If the table was removed from the subscription
  before the worker started, the call raises an error with the spinlock held
  (`8dc7c33812`, 2017-06-09, "Improve tablesync behavior with concurrent changes"). The
  apply worker or the slot's next user then spins on `relmutex` until a "stuck
  spinlock" PANIC. This is valid, but more an implementation rule violation than a
  design issue. Grade "valid", or partial K4 if the report frames it as the
  removed-table race.
- `e3cf708016` (2017-04-27) "Wait between tablesync worker restarts": the apply worker
  restarts a failing sync worker on every loop iteration with no backoff (tight
  restart loop).
- `9f11fcec66` (2017-04-28): `last_start_time` in `ApplyLauncherMain` is declared
  inside the loop (L645), so the retry throttling is dead code.
- `9414e41ea7` (2017-05-01): `on_commit_launcher_wakeup` is never reset
  (`AtCommit_ApplyLauncher`), so the launcher is woken at every later commit, and
  after aborts too.
- DROP SUBSCRIPTION stops the apply worker before its transaction commits. If the
  DROP aborts, workers were killed for nothing; the launcher restarts them, so this is
  mild (see `7e174fa793`).
- Plausible, but no fix found: while the apply worker busy-waits in
  `wait_for_sync_status_change` for a slow catch-up, it reads no stream and sends no
  feedback, so the publisher's `wal_sender_timeout` can drop its connection.

