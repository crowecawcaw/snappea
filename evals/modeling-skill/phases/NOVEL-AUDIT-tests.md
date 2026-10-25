# Frozen test submission audit

Scope: the six frozen `submission/test_*.py` files named below, read against their matching public `TASK.md`, `events.p`, and `contract.p`. The task asks for tests of public methods and monitor-checked observations; gaps below mean **not covered by these histories**, not that the submitted implementation is incorrect. The migration task explicitly says the service has no instrumentation and requires wrappers to capture its calls and results (migration `public/TASK.md:7-14`).

## Source and monitor use

| Submission | Public-call provenance | P monitor setup |
|---|---|---|
| `stream-test-skill-01/submission/test_window.py` | `add`/`advance` call `Service` once, then capture the actual accepted/output results and arguments; one output event per returned dict; `eEndCall` follows the full list (`:34-57`). | Batched `check_events` over `events.p` and `contract.p`, with `strict=True`, `check_hot=True`, and unknown events as errors (`:59-69`). |
| `stream-test-skill-02/submission/test_window.py` | Same direct call/result capture; helper preserves returned output order (`:18-47`). | Batched `check_events` over the supplied files with the same explicit strict/hot/unknown-event settings (`:50-58`). |
| `stream-test-base-01/submission/test_window.py` | `add`/`advance` call the service, emit their returned results, and append `eEndCall` after all outputs (`:12-39`). | `check_spec` context wraps each public history (`:43-45`, `:61-64`, `:78-80`, `:92-94`). It omits `strict=True`; the named-spec default is `strict=True`, but missing relevant events currently warn unless strict is explicit (`python/src/snappea/__init__.py:62-65`). The exercised histories do emit events. |
| `migration-test-skill-01/submission/test_migration.py` | Each wrapper records direct method arguments and the method's actual return/state result, including reads, `inspect_new`, and `ready` (`:14-64`). | Batched `check_events` with strict, hot-state, and unknown-event checks (`:66-75`). |
| `migration-test-skill-02/submission/test_migration.py` | Same call/result wrapper pattern for every event in the supplied schema (`:14-64`). | Same explicit strict/hot/unknown-event checks (`:66-75`). |
| `migration-test-base-01/submission/test_migration.py` | Each wrapper captures a direct public call and its returned value (`:8-58`). | `check_events` checks the named contract but explicitly sets `strict=False` and omits `check_hot`/`unknown_events` (`:60-68`). The histories do contain captured events; the setting weakens the guard against a future empty or partially instrumented history. |

The stream task requires public call order, backward event timestamps to remain in call order, and an end marker after each call (`stream-test-skill-01/public/TASK.md:7-16`; same wording in the other stream public tasks). The stream P contract recomputes acceptance, totals, revisions, and ordered pending outputs, then checks for unexpected/missing outputs and unclosed calls (`stream-test-skill-01/public/contract.p:27-49,52-88`). The wrappers preserve that source/result split: event arguments come from the public inputs, acceptance/output payloads come from the service return, and P independently checks those records.

## Stream histories

In this table, Skill 01/02 and Base 01 refer to the corresponding `stream-test-*/submission/test_window.py` paths in the source table above.

| Contract case | Skill 01 | Skill 02 | Base 01 |
|---|---|---|---|
| Existing emitted window gets a late correction, then final revision | Yes: correction after watermark 20 and final at 25 (`:71-90`). | Yes: correction after watermark 10 and final at 15 (`:62-84`). | Yes: correction after watermark 10 and final at 15 (`:61-76`). |
| First output only at lateness cutoff is final revision 0 | Yes: `advance(35)` (`:113-121`). | Yes: `advance(15)` (`:110-120`). | Yes: `advance(35)` (`:78-90`). |
| First late arrival after window end but before cutoff emits immediately at revision 0, distinct from correction | Yes: watermark 10 precedes `add(7, 6)`; its immediate nonfinal revision-0 output is then finalized (`:130-143`). | Not isolated. The backwards-timestamp add occurs before the first watermark (`:66-72`); the cutoff-first case is covered separately. | Not covered; the tests have corrections and cutoff-first finalization, but no post-window-end/pre-cutoff first arrival (`:61-90`). |
| Exact `end + lateness` rejection | Yes: reaches 25 for `[10,20)`, then rejects `add(18,100)` (`:92-111`). | Yes: reaches 25 for `[10,20)`, then rejects `add(19,100)` (`:85-93`). | Yes: reaches 35 for `[20,30)`, then rejects `add(20,8)` (`:78-89`). |
| Accepted event exactly at a width boundary asserts its bucket | Not covered (`:72-143`). | Not covered: `add(0,1)` is rejected, so it does not assert a bucket (`:110-120`). | Not covered: `add(20,8)` is rejected, so it does not assert a bucket (`:78-89`). The suite does exercise a watermark exactly at `20` (`:49-55`). |
| Negative values | Not covered (`:71-143`). | Not covered (`:62-120`). | Not covered (`:43-105`). |
| Zero-valued add / no-op correction | No zero-valued add (`:71-143`). | No zero-valued add (`:62-120`). | Yes: adding zero to an emitted sum returns an unchanged total at the next revision (`:91-106`). |
| Equal/no-op watermark | Yes, indirectly: after `advance(100)`, a rejected add and another `advance(100)` produce no output (`:123-128`). No explicit `advance(0)`. | No repeated watermark (`:62-120`). | No repeated watermark (`:43-105`). |
| Multi-window output order and empty gaps | Yes: windows 0 and 20 sorted with gap 10 omitted (`:113-121`). | Yes: same shape (`:95-108`). | Yes: windows 0 and 10 emitted sorted on one advance (`:43-59`). |

Skill 01 has the most distinct stream branches, notably the late-first-arrival case. Skill 02 combines many edge cases in three histories but its backward timestamp demonstrates call-order preservation, not first arrival after a window has already ended. The base submission adds the useful zero-value revision case and a same-advance pair at the exact window-end watermark. Across all three, negative values and an accepted event whose bucket is asserted at an exact width boundary remain untested; the public contract gives values as `int` and defines windows as half-open `[floor(timestamp/10)*10, start+10)` (`public/TASK.md:7-13`), so these are reasonable additional semantic probes, not extra requirements.

## Migration histories

In this table, Skill 01/02 and Base 01 refer to the corresponding `migration-test-*/submission/test_migration.py` paths in the source table above.

| Contract case | Skill 01 | Skill 02 | Base 01 |
|---|---|---|---|
| Stale snapshot cannot regress a newer live write | Yes: write `b` after prepare, then apply (`:122-131`). | Yes: write `a` after prepare; stale snapshot fills `b` while preserving `a` (`:97-109`). | Yes: write/delete after prepare, then apply (`:101-118`). |
| Stale snapshot cannot erase a tombstone | Yes: delete `b` after prepare, apply, inspect tombstone, read absent (`:133-145`). | Yes: prepare, delete `a`, replay snapshot with older live `a` (`:139-153`). | Yes: same stale-delete shape for `b` (`:101-118`). |
| Delete then recreate gets a fresh revision | Yes: delete then write `a` and assert larger revision (`:147-158`). | Yes: delete `b`, then write it after cutover and assert larger revision (`:132-163`). | Yes: delete `b`, then write it after cutover and assert larger revision (`:106-124`). |
| New key and readiness | Yes: writes `c`, inspects it, and checks readiness before/after snapshot apply (`:160-174`). This exercises new-key projection, but the pre-apply `False` readiness is already explained by uncopied `a`/`b`, so it does not isolate `c` as the readiness cause. | No new key (`:78-165`). | No new key (`:75-135`). |
| Rejected cutover remains read-visible | Yes: unready cutover is false, then legacy read and readiness are checked (`:98-104`). | Yes: reads are checked immediately before and after rejected cutover (`:81-92`). | Partial: rejected cutover is asserted, but no read follows it before prepare/apply and eventual successful cutover (`:75-99`). |
| Snapshot token replacement / unknown token / retry | Yes: superseding prepare, stale and unknown rejection, current retry (`:106-120`; retry also `:85-90`). | Yes: unknown and superseded rejection, current retry (`:93-130`). | Yes: unknown and superseded rejection, repeat current apply (`:83-99`, `:127-135`). |

The skill submissions use stronger monitor options than the migration base submission, but the base still records and checks the exercised histories. Skill 02 is the broadest single migration history: it includes an unknown token, retry, superseding snapshot, stale write and tombstone, cutovers, reads, and post-cutover mutation (`:78-165`). Skill 01 is more separately focused and is the only one of the three to exercise an arbitrary new key (`c`).

The migration monitor's `eReady` and `eCutover` compare only `a` and `b` (`migration-test-skill-01/public/contract.p:74-83,103-112`). Under the public invariant, these are the only rows initially missing from shadow; every later write or delete atomically updates both authoritative and shadow rows, including arbitrary keys (`migration-test-skill-01/public/TASK.md:5`; monitor `eWrite`/`eDelete`: `migration-test-skill-01/public/contract.p:57-71`). Thus checking that initial `a` and `b` match is sufficient to predict readiness under the contract; the fixed `a`/`b` check is not a readiness-monitor gap. Skill 01 additionally exercises a new key's public projected row and its ready path (`:160-174`).
