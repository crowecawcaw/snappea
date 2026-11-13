# Model the risky boundary

## Scope

- Start with a falsifiable contract: “two clients cannot reserve the last unit” or “retry cannot apply an effect twice.”
- Name independent actors, owned/shared state, real storage primitives, and relevant identities/versions.
- Start small: two competing clients, one resource, a few values. Retain distinctions that permit the defect; omit unrelated detail.
- State atomicity, message delay/duplication, persistence, bounds, and omissions. Ask whether each omission could change the property.

## Preserve interference

- An atomic API specification describes allowed behavior; it does not validate the algorithm implementing it. Include the actual read/CAS/retry protocol when assessing that design.

- Snappea schedules at operations such as `send`, `receive`, and machine creation. Ordinary assignments and `announce` do not yield.
- A scheduling point comes *before* the operation takes effect. At `send`, specs see the event at once, then other machines may run, then the event is queued. At `new`, machines created earlier may run before the new one exists. A dequeued event, and a `receive` that found its event, may wait while others run before the handler or case body starts.
- So `announce eDone; send c, eReply` lets others act between the two; `send c, eReply; announce eQueued` announces once the reply is queued. Announce plans before the `new` that starts machines acting on them.
- A non-yielding group may represent **one real atomic primitive**: a transaction, lock-protected update, or CAS. A whole handler is not automatically atomic.
- Split read → decide → write across messages/phases when another actor can intervene. Otherwise the model assumes away the race.
- Keep preparation, dependency writes, validation, and publication separate when those boundaries matter. Prepared-but-unpublished records are not committed state.
- A per-resource model cannot check a shared global limit unless it represents the competing resources.
- See reservation (`snappea guide example reservation`) for atomic conditional update versus stale read/write.
- The store's isolation level is part of the model. A transaction written as one handler is serializable: it assumes away write skew and many lost updates. If the real store is weaker, split the transaction's reads and writes.
- A direct `send` is queued before the sender's next step, so it arrives in causal order: a later message routed through another machine cannot overtake it. To model replication lag or a slow link, put a forwarding machine on that path.

## Keep the search small

- Each scheduling point where several machines can run multiplies the schedules. Every extra concurrent actor, forwarding hop, retry loop or fault budget can multiply them 10-100x.
- Merge machines whose hop adds no order the contract cares about, such as reliable FIFO forwarders.
- Fix inputs before starting workers: choose values (`choose`, `$`) in the driver, then create the workers.
- Split fault classes (loss, duplication, crash) into separate tests unless the bug needs them together.
- To force one rare order, add a test-only holder machine that keeps one message until another actor finishes, then releases it. That test runs a few schedules instead of thousands.
- When a search does not finish, `snappea check` says where schedules branch: the machines ready at scheduling points, and the `$`/`choose` choices made, per schedule. Reduce the top ones first. `snappea check -v` prints this for every test case.

## Compare with implementation

- Map each modeled read, write, version check, retry, and restart to its actual primitive.
- An atomic commit-pointer model alone does not validate candidate construction or CAS retry logic; model those stages if risky.
- For bounded records and larger logical operations, represent splitting plus publication. Separately test actual encoded sizes in the implementation.
- Replay and reduce counterexamples before adding realism. A passing finite search covers only represented transitions and bounds.
