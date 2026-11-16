---
diataxis: reference
personas: [application-developer, coding-agent]
questions:
  - Which questions must a model review answer before I trust a passing check?
  - How do I show each failure mode is modeled or deliberately excluded?
  - Which guide topic explains how to cover each item?
---

# Model review checklist

Answer each item in DESIGN.md: yes / no / not applicable, with the model that
covers it or a reason. Format: question -> evidence -> `snappea guide <topic>`.

## Models

1. Does each model cover one part with its own state and failure modes, and is every
   seam assumption asserted by another model's spec? -> model list with assumptions
   and seams -> modeling

## Actors

2. Is every actor (client, worker, holder, store, timer) a machine, with two
   competing for one resource? -> machine list; a 2-client test -> modeling
3. Is each handler one real atomic primitive, with reads and writes split at
   `send`/`receive`? -> handler-to-primitive map -> modeling

## Network

4. Can messages be lost, duplicated, reordered or arrive after a timeout? -> each
   modeled with a bound, or excluded with a reason -> faults
5. Does every reply carry a request ID, so a late or duplicate reply cannot finish
   other work? -> spec checks IDs -> spec-patterns

## Process

6. Can each node crash at any step, including between two writes? -> driver crash
   with a budget; non-atomic writes in separate handlers -> recovery
7. What survives a restart? -> durable and in-memory state listed -> recovery
8. Is a retry after commit-then-lost-reply safe? -> test: commit, lose reply,
   restart, retry same ID -> recovery

## Time

9. Can a holder crash, or its release be lost, leaving a lock, lease or slot held
   forever? -> test where the holder never returns -> spec-patterns
10. Does every timeout, lease or expiry have a test where it is the only way forward?
    -> removing it fails a liveness spec -> spec-patterns
11. Can a timer fire late, after a cancel, or after the holder resumed? -> timer fires
    at any time; stale writes rejected at the atomic update -> faults, recovery
12. Do retries keep the request ID and continue until success? -> no attempt limit in
    drivers -> spec-patterns

## Specs

13. Is each safety property asserted from driver intent and commits, not design code?
    -> independent spec -> spec-patterns
14. Does each "must eventually" have a hot state that stays hot until the whole plan
    settles? -> named hot state per promise -> spec-patterns
15. Do drivers keep trying, so a stuck state stays visible? -> retry until success, or
    probe after a quiet point -> spec-patterns
16. Does a run with zero work fail? -> spec starts hot, requires a nonzero plan ->
    spec-patterns

## Search

17. Are bounds stated (clients, IDs, values, losses, crashes) and search complete? ->
    `summary.incomplete` is 0 -> modeling
18. Is every scenario reached and all model code covered? -> no coverage gaps ->
    scenarios, scenario-monitors
19. Is every undetected mutant fixed or suppressed with a reason? -> scenarios
20. Can the implementation's tests fail? -> each deliberate break fails a test ->
    testing
