---
diataxis: reference
personas: [application-developer, coding-agent]
questions:
  - Which questions must a model review answer before I trust a passing check?
  - How do I show each failure mode is modeled or deliberately excluded?
  - Which guide topic explains how to cover each item?
---

# Model review checklist

Answer each item in DESIGN.md: yes / no / not applicable, with a reason.
Format: question -> evidence -> `snappea guide <topic>`.

## Actors

1. Is every actor (client, worker, holder, store, timer) a machine, with two
   competing for one resource? -> machine list; a 2-client test -> modeling
2. Is each handler one real atomic primitive, with reads and writes split at
   `send`/`receive`? -> handler-to-primitive map -> modeling

## Network

3. Can messages be lost, duplicated, reordered or arrive after a timeout? -> each
   modeled with a bound, or excluded with a reason -> faults
4. Does every reply carry a request ID, so a late or duplicate reply cannot finish
   other work? -> spec checks IDs -> spec-patterns

## Process

5. Can each node crash at any step, including between two writes? -> driver crash
   with a budget; non-atomic writes in separate handlers -> recovery
6. What survives a restart? -> durable and in-memory state listed -> recovery
7. Is a retry after commit-then-lost-reply safe? -> test: commit, lose reply,
   restart, retry same ID -> recovery

## Time

8. Can a holder crash, or its release be lost, leaving a lock, lease or slot held
   forever? -> test where the holder never returns -> spec-patterns
9. Does every timeout, lease or expiry have a test where it is the only way forward?
   -> removing it fails a liveness spec -> spec-patterns
10. Can a timer fire late, after a cancel, or after the holder resumed? -> timer fires
    at any time; stale writes rejected at the atomic update -> faults, recovery
11. Do retries keep the request ID and continue until success? -> no attempt limit in
    drivers -> spec-patterns

## Specs

12. Is each safety property asserted from driver intent and commits, not design code?
    -> independent spec -> spec-patterns
13. Does each "must eventually" have a hot state that stays hot until the whole plan
    settles? -> named hot state per promise -> spec-patterns
14. Do drivers keep trying, so a stuck state stays visible? -> retry until success, or
    probe after a quiet point -> spec-patterns
15. Does a run with zero work fail? -> spec starts hot, requires a nonzero plan ->
    spec-patterns

## Search

16. Are bounds stated (clients, IDs, values, losses, crashes) and search complete? ->
    `summary.incomplete` is 0 -> modeling
17. Is every scenario reached and all model code covered? -> no coverage gaps ->
    scenarios, scenario-monitors
18. Is every undetected mutant fixed or suppressed with a reason? -> scenarios
19. Can the implementation's tests fail? -> each deliberate break fails a test ->
    testing
