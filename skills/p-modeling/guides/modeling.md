# Model the risky boundary

## Scope

- Start with a falsifiable contract: “two clients cannot reserve the last unit” or “retry cannot apply an effect twice.”
- Name independent actors, owned/shared state, real storage primitives, and relevant identities/versions.
- Start small: two competing clients, one resource, a few values. Retain distinctions that permit the defect; omit unrelated detail.
- State atomicity, message delay/duplication, persistence, bounds, and omissions. Ask whether each omission could change the property.

## Preserve interference

- An atomic API specification describes allowed behavior; it does not validate the algorithm implementing it. Include the actual read/CAS/retry protocol when assessing that design.

- Snappea schedules at operations such as `send`, `receive`, and machine creation. Ordinary assignments do not yield.
- A non-yielding group may represent **one real atomic primitive**: a transaction, lock-protected update, or CAS. A whole handler is not automatically atomic.
- Split read → decide → write across messages/phases when another actor can intervene. Otherwise the model assumes away the race.
- Keep preparation, dependency writes, validation, and publication separate when those boundaries matter. Prepared-but-unpublished records are not committed state.
- A per-resource model cannot check a shared global limit unless it represents the competing resources.
- See [reservation](../examples/reservation/README.md) for atomic conditional update versus stale read/write.

## Compare with implementation

- Map each modeled read, write, version check, retry, and restart to its actual primitive.
- An atomic commit-pointer model alone does not validate candidate construction or CAS retry logic; model those stages if risky.
- For bounded records and larger logical operations, represent splitting plus publication. Separately test actual encoded sizes in the implementation.
- Replay and reduce counterexamples before adding realism. A passing finite search covers only represented transitions and bounds.
