---
diataxis: how-to
personas: [application-developer, coding-agent]
questions:
  - Which state survives a crash, and how do I model a restart?
  - Which retry and stale-message histories should I check?
  - How do I keep stale holders and stale evidence from writing?
---

# Crashes, restarts and retries

## Model a crash

- Durable state (effects, outcome records) survives a restart; volatile state
  (memory, pending replies, flags) does not, unless you model storage loss.
- Keep durable state in a storage machine. The crashed machine drops its fields, or
  a new machine replaces it (`raise halt` in the old one) and reloads from storage.
- A crash is a driver event (`eCrash`); it lands between handlers, never inside one.
  Writes that do not commit together go in separate handlers (send yourself a
  continue event after the first).
- The driver chooses when (`$` before each step, budget of one) and announces the
  crash so specs know which effects may be missing. Recipe: `snappea guide faults`.
- Give each incarnation an epoch carried by requests and replies; drop replies from
  earlier epochs.

## Histories to check

- Commit, lose the reply, restart, retry the same ID. A missing reply does not mean
  a missing commit.
- Complete A, let B change the same state, retry A: if promised, return A's original
  result without reapplying it. Track original replies apart from current state.
- Retries keep the request ID; a new ID changes the idempotency contract.
- Effect and outcome record are one step only if a real primitive commits them
  together.
- A holder that crashes holding a lock: `snappea guide spec-patterns`.
- Runnable: `snappea guide example durable-retry`,
  `snappea guide example blob-publication`.

## Stale holders and evidence

- A version, lease or fencing check protects a write only if the atomic update
  enforces it. A holder whose lease expired can still act.
- After a CAS conflict, reread, recompute and revalidate; changing only the expected
  version can publish stale evidence (`snappea guide example versioned-publication`).

Start with one request, effect and restart. State omitted crash points, expiry,
network behavior and storage failures.
