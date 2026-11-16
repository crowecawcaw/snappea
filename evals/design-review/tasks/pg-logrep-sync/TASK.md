# Design review: PostgreSQL logical replication workers

`/work/src` holds the logical replication subscriber of PostgreSQL at a historical
commit. A launcher process starts one apply worker per enabled subscription. The
apply worker streams committed transactions from the publisher and applies them.
For each table that still needs its initial copy, the apply worker starts a
table-sync worker that copies the table and then hands the table back to the apply
worker. The two coordinate through a shared-memory slot array (`launcher.c`) and the
per-table state catalog `pg_subscription_rel`. `CREATE/ALTER/DROP SUBSCRIPTION`
(`subscriptioncmds.c`) change the set of subscriptions and tables while workers run.

Facts about code not included:

- `SetSubscriptionRelState` (`pg_subscription.c`) updates the table's row in
  `pg_subscription_rel`, inserting it if missing. Catalog writes become visible to
  other processes when the writer's transaction commits.
- The apply worker opens a target table with `logicalrep_rel_open` (`relation.c`),
  which re-reads the table's state from `pg_subscription_rel` whenever its cached
  state is not READY.
- After a restart, the publisher streams transactions whose commit record starts at
  or after the requested position.

Find design bugs in this subsystem: ways replicated changes can be lost or applied
twice, and ways workers or DDL can get stuck forever or leave orphaned workers.
Assume any worker can exit or crash at any point, the server can restart, and DDL
can run concurrently with the workers.

Use P to model the design and check it. {tool_line}

Deliverables:

- `/work/model/`: your P model.
- `/work/REPORT.md`: one section per design bug you found, with a title, the
  sequence of events that triggers it, the code involved, and whether the P
  checker produced a counterexample for it. Report only bugs you can explain.
