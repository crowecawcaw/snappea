# Phased development pilots

Luna/high, fresh agents, isolated offline containers. Raw prompts, source, transcripts, hashes, and grades are under ignored `runs/`; [pilot.json](pilot.json) contains original outcomes and explicitly labeled supplemental rechecks. These are adaptive development runs, not a statistical estimate of skill benefit.

## Model construction

| Revision/run | Native check | Monitor: valid accepted | Invalid rejected | Source finding |
|---|---|---:|---:|---|
| v1-01 | Coverage failure; 4 complete searches | 2/5 | 13/13 | Genuine read/CAS/retry; 11 coverage gaps; monitor requires consecutive tokens |
| v1-02 | Pass; 1 complete search | 2/5 | 13/13 | Sequential API model; omits actual read/CAS algorithm; consecutive-token constraint |
| v2-01 | Pass; 3 complete searches | 5/5 | 13/13 | Competing clients, actual read/CAS/retry, independent monitor |
| v2-02 | Pass; 2 complete searches | 5/5 | 13/13 | Actual read/CAS/retry; extra CAS-based clock mechanism |

- V2 skill distinguishes an atomic API specification from a model of the implementing algorithm; monitors assert external promises rather than incidental choices.
- V2 task also clarifies linearization-order events, serialized test-control time, and permitted token gaps. Improvements cannot be attributed solely to skill wording.
- Both v2 models are reasonable design artifacts. Contention remains concentrated on acquire/acquire; neither covers every operation-specific race. V2-02 models time advance through CAS rather than the specified direct atomic control operation, though its scenarios serialize it.
- Complete required coverage did not establish scenario adequacy. Mutation analysis hit budgets and is not exhaustive proof.
- Sources: [v1 audit](audit-v1.md), [v2 audit](audit-model-v2.md), [independent final audit](audit-final-independent.md).

## Adding implementation tests

The original six faults were too forgiving: both v1 submissions rejected all six while still asserting incidental token numbers and allowing later successful writes to hide rejected-write side effects. Before v2, the grader added a permitted token-allocation variant and three faults targeting state effects. This nine-fault set stayed unchanged through v5.

| Skill revision | Valid variants accepted, per run | Faults rejected, per run | Material lesson |
|---|---|---|---|
| v1, original grader | 1/1, 1/1 | 6/6, 6/6 | Real saved observation tests, but weak discrimination |
| v1, supplemental nine-fault recheck | 1/2, 1/2 | 8/9, 7/9 | Brittle token assertions and hidden rejected effects |
| v2 | 2/2, 2/2 | 8/9, 8/9 | Allow contract-valid alternatives; read after rejection |
| v3 | 2/2, 2/2 | 8/9, 8/9 | Isolate guards; test stale calls against successor state |
| v4 | 2/2, 2/2 | 9/9, 8/9 | Explicitly include lifecycle conditions, not only arguments |
| v5 | 2/2, 2/2 | **9/9, 9/9** | Put immediate public reads after rejection in the entry point |

- V2 task also explicitly permits different token-allocation policies. V3–v5 use the same task/grader; only skill content changes.
- All ten test agents saved runnable tests using actual `Service` calls and `check_events`, with real inputs/results. No fabricated commits or lossy value collapsing.
- V3 exercised wrong owner/token but missed expiry with otherwise-valid credentials: a different guard masked the missing check.
- V4's weaker run checked rejected returns, then overwrote the value successfully before reading it. The monitor never received the evidence needed to reject the defect.
- Both v5 suites observe rejected-write effects immediately, isolate expiry before reacquisition, and try stale release against a successor's live lease. Both accept nonconsecutive token allocation.
- These results measure test-suite sensitivity to selected faults. Direct Python assertions also detect faults; this is not evidence that P alone caused detection.
- Sources: [v2 audit](audit-test-v2.md), [v3 audit](audit-test-v3.md), [v5 audit](audit-test-v5.md), [independent final audit](audit-final-independent.md).

## Combined flow: new queue contract

After the isolated capabilities worked, freeze a different task and run two skill agents plus one no-skill control. All use the same contract, checker, image, grader, and 900-second/80-call budget. Control differs only by the skill sentence/mount. Prompt/environment equality was checked after substituting run IDs.

Corrected v2 grading of the same frozen submissions:

| Condition/run | Implementation cases | Own saved tests | Native model check | Monitor valid accepted | Invalid rejected | Calls |
|---|---:|---:|---|---:|---:|---:|
| No skill, base-01 | 8/8 | 5/5 | Pass | 4/4 | 3/4 | 80 |
| Skill-01 | 8/8 | 4/4 | Coverage failure; recheck search incomplete | 3/4 | 4/4 | 41 |
| Skill-02 | 8/8 | 4/4 | Pass | 3/4 | 3/4 | 33 |

- **Implementation correctness transferred; faithful design modeling did not.** All three Python implementations use real read/CAS retries. All three final P models collapse public operations into atomic Store handlers, so none validates the implementing CAS algorithm. Skill models contain competing actors; control has one client.
- **Valid histories matter.** Both skill monitors reject a required stable empty-take replay after a later insertion. A monitor rejecting every bad trace can still reject correct behavior. Skill-02 also fails to observe snapshots; one extra effect is invisible to it. The control's snapshot assertions omit sequence length and accept one invalid history.
- **Model-first remains unreliable.** Skill-01 wrote Python first; skill-02 started P first but obtained its first passing final model only after implementation. Control also implemented first. See command/source audits for exact chronology.
- Both skill suites check real implementation behavior with helpers. Skill-01 records calls through a wrapper; skill-02 captures implementation `emit` calls and checks source provenance. Control also saves a genuine observation path. Source review is required; lexical helper-name checks alone do not establish this.
- The lower tool-call counts with skill suggest reduced tool friction in this pilot; one control and two treatments do not establish an effect size. Neither condition passes the full model/monitor audit.
- Sources: [control audit](audit-flow-base-01.md), [skill-01 audit](audit-flow-skill-01.md), [skill-02 audit](audit-flow-skill-02.md).

### Grader correction

The original combined grader swapped in a reference implementation to score saved tests. This is unsuitable as a primary requirement when an agent owns its implementation: the reference emits no events, and legitimate implementation-specific tests can enforce additional behavior outside the task's preconditions. Both skill suites also check their chosen capacity-mismatch validation; the control has its own reference incompatibility. A failed reference means mutant failures are uninterpretable.

- V2 independently runs saved tests against their own submitted implementation.
- Cross-implementation probes run only after reference compatibility is established, and remain diagnostic. None of these three suites qualified, so no mutation-detection score is claimed for the combined flow.
- Original grades and grader source remain archived; v2 reports are explicitly posthoc `supplemental-v2`, with source/provenance manifests. Hidden implementation and monitor cases did not change.
- A regression test ensures reference setup/compatibility failures do not become mutant-detection credit.

**Conclusion:** focused tasks now discriminate useful models and stronger observation tests. Recombining them still produces weaker models and incomplete oracles; the skill is not yet a reliable end-to-end design workflow. The phased eval exposes that regression rather than hiding it behind passing Python tests or a passing bounded P check.

## Skill changes retained

Short bullets rather than additional workflow machinery:

- Model the real algorithm, including read/conditional-write/retry boundaries.
- Assert promised constraints; allow permitted implementation choices.
- Test each acceptance precondition independently, including lifecycle boundaries.
- Exercise old operations against newer live state.
- Read public state immediately after rejection, before later work hides side effects.

The entry point is about 350 words including metadata. Detailed guides remain bullet-based; complete examples supply syntax and runnable controls.

## Calibration and limitations

- Model monitor controls: known-good monitor accepts 5/5 valid histories, rejects 13/13 invalid histories. Model fidelity still needs source review.
- Lease test calibration: two valid implementations accepted; all nine faulty variants rejected by a private reference suite.
- Queue calibration: 8/8 implementation cases, 4/4 valid monitor histories accepted, 4/4 invalid histories rejected, and all three observation-suite faults detected by the reference suite. No reference queue P algorithm was calibrated.
- Guard against empty test discovery, compilation/setup errors earning mutation credit, and changed frozen inputs. Protocol tests cover those cases.
- Initial test-v1 grader sources were hashed but not archived. Current-nine-fault rechecks are explicitly supplemental. Later graders are archived with trials.
- Small samples, adaptive tuning on one family, no isolated-phase controls. Do not pool revisions or infer a general effect size.
