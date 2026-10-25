# P modeling skill trials

## First pilot: original skill

All runs used fresh `gpt-6-luna` agents at high thinking, a 900-second/80-call budget, the same offline P/Python environment, and the frozen certified-bundles contract and grader (version 1.1.3, 64 scheduler seeds). Treatment adds one sentence directing the agent to the skill. It does not add modeling, testing, or implementation hints to the task prompt. See README.md for the exact pair of prompts and isolation instructions.

Skill snapshot: `21c2b611827a1342fd9f23948b072d9e765a9db539f390498bf144c6fd646a11`; initial content commit `7654d00`. Checker source is `ccf8440`, binary SHA256 `0391b4493a273b16b6494f570c5debff4bfdeb09722eaaf0972fb155857fe332`. The treatment skill alone was mounted read-only. Public inputs and all frozen submission manifests were verified; see pilot-v1.json for hashes.

| Outcome | P baseline | P + skill v1 |
|---|---:|---:|
| All 20 primary hidden cases passed | 3/3 | 2/3 |
| Primary cases passed | 60/60 | 59/60 |
| Separate wide-input probe passed | 0/3 | 3/3 |
| Saved P model with successful exhaustive check and no coverage bypass | 1/3 | 3/3 |
| Saved implementation-to-monitor observation test | 0/3 | 0/3 |

The wide probe submits a valid logical operation spanning more than one 16,384-byte storage record. Every baseline exceeded the per-record limit. All three treatment implementations passed that probe. This is promising evidence about a particular representation obligation, not a reliable estimate of the skill's overall benefit.

The failed treatment primary case replayed a completed publication over a later update. Its replay/event application logic confused returning an old result with applying an old effect. The model checked a narrower history and did not expose this behavior. Controls passed these primary cases, so the pilot does not establish an overall correctness improvement.

## Independent artifact review

Luna/high auditors reviewed submissions and transcripts without consulting hidden grades or grader sources. See audit-baseline.md and audit-treatment.md for command/source evidence.

Treatment agents read the skill and examples and authored substantive focused models. They used coverage feedback to add scenarios or improve condition coverage. Baseline models were narrower: one useful five-schedule scalar model, one passing publish-only check with coverage bypass and a suppressed empty monitor, and one run that only listed/linted its unsaved model.

All treatment models abstracted the storage visibility operation. That abstraction can be valid for its focused question, but it did not establish that the separate Python reads, staging writes, CAS retries, and restart behavior implement that operation. None of the treatment agents retained a runnable observation-based regression test. Useful inline implementation probes were not saved. No treatment agent followed up all final surviving mutants.

## Revision based on the pilot

The next revision preserves why-oriented guidance while making these omissions concrete:

- Retain a runnable implementation observation test; explain why a one-off probe is not an enduring bridge between code and model.
- Extract real storage boundaries before choosing an atomic abstraction, and distinguish prepared-but-unpublished state from committed state.
- Repeat an old operation after different work advances state; distinguish replaying its original response from replaying its effects.
- Use a later public read to expose state mutations that an announcement-only monitor cannot observe.
- Explain representative model capacities versus actual encoded-limit implementation tests.

The durable-retry example is extended with original bounded positive and negative replay scenarios. Revised skill results must use fresh agents and a new immutable snapshot; they are not pooled with this pilot. Subsequent results are adaptive exploratory evidence, not a held-out confirmation.

## Second exploratory cohort

Fresh treatment runs `skill-v2-01..03` used snapshot `c5f21f5684423da10d43627b6ab40aacc209f6a61da25eb83bf12b9cdc80bed3` (content commit `d25396a`) and the same environment. The original three baseline runs are reused for context; these are not new paired controls. Primary correctness again reached 59/60 cases (2/3 full passes). One implementation repeated the same old-effect replay defect. See pilot-v2.json and audit-v2.md for evidence.

The source-first audit found that all three wrote Python implementation before their P model, despite the design-first framing. None saved an observation-based implementation regression test. Two read implementation guidance before coding, so access alone did not address the omission. They did produce checked bounded models, and two ran useful inline crash/growth probes. The final revision shortens the entry point and states the implementation handoff explicitly near the top: keep a saved runnable observation test, explain why it protects future changes, and distinguish emitted events from wrapper-captured real inputs/results.

The second cohort passed the supplemental wide-input probe in all three submissions. This repeats the first pilot's representation result but is still adaptive evidence on the same task.

## Final entry revision: explicit implementation handoff

Fresh runs `skill-v3-01..03` used snapshot `132d5175e77953171ad56f040a1881ebc6e668d3d6f7ecae86ce108eee15d0f9`, committed in `11c8f2f`. The task prompt was unchanged. The entry was shortened and the saved implementation observation test was moved near the top, with an explanation of its maintenance value and explicit event-provenance limits. The durable-retry README also received a portability-only command fix. This revision was reviewed before the trials.

| Outcome | Original P baseline (reused) | P + final skill |
|---|---:|---:|
| All 20 primary hidden cases passed | 3/3 | 2/3 |
| Primary cases passed | 60/60 | 58/60 |
| Separate wide-input probe passed | 0/3 | 2/3 |
| Saved implementation-to-monitor tests, independently rerun successfully | 0/3 | 3/3 |

All three retained observation tests that call the actual Service and feed real results or emitted events to a monitor. They were independently rerun in fresh offline containers with the exact trial binary and public inputs: run 01 passed five tests, run 02 five, and run 03 eight (each count includes four supplied public tests). See `recheck_observation.py` for the reproduction path and audit-v3.md for the properties and limitations. These are genuine wrapper-based observations, not proof of internal commits or every possible history.

Run 01 fails two primary cases involving replaying the original submit response after crash/revision, and its record layout still exceeds the storage limit for a wide valid update. Its saved observer focuses on publication and does not cover submit-response identity. Runs 02 and 03 pass all primary cases and the wide probe. Thus saved observation tests improve reproducibility and validation coverage, but can still miss requirements that they never observe or assert.

## What the evidence supports

The skill is runnable, self-contained, reviewed, and backed by original positive/negative examples. In these pilots, it steered agents toward substantive bounded models and representation-aware implementations; the final entry also steered all three agents to retain real implementation observation tests. The baseline had no such saved checks, and all baseline submissions failed the supplemental storage-size probe.

The trials do **not** establish an overall correctness win: controls passed all primary cases, while every treatment cohort had one imperfect implementation. Model/implementation correspondence remains incomplete, many mutants survive, and a saved observer only checks the properties and histories it actually sees. The task has a primary-score ceiling for the three controls, and cohorts of three are too small for reliable effect estimates. Revisions were tuned on this task; the final cohort is an adaptive pilot, not a held-out confirmation. New tasks and larger prespecified cohorts are needed before claiming general robustness gains.

No product CLI changes or runtime instrumentation semantics changed. The skill stays primarily a design aid, with implementation checking as the second stage; its limitations are explicit rather than hidden behind coverage counts.

## Validation and evidence

All skill links and correct/broken example cases passed `skills/p-modeling/validate.py`; the reservation example's saved Python tests include a deliberately defective implementation. Snapshot/prompt parity and tamper-detection tests passed. Workspace build, tests, formatting, Clippy, and emit-only build passed. Frozen submission hashes and public inputs were verified for all three cohorts, each with the same checker binary, P image, public-file hashes, grader version, and 64-seed setting.

Ignored `evals/certified-bundles/runs/` retains exact prompts, tool transcripts, submissions/manifests, content-addressed skill snapshots, grading and wide-probe results, and final observation reruns. The downloadable evidence archive contains the twelve actual solver trials and the three skill snapshots used by those trials; setup-only snapshots and scripted protocol smoke are not counted as agent results.

The final source-first audit also confirmed that all three v3 agents still wrote implementation before the P model. The next focused revision adds one explicit design-first sentence at the top of the skill, with its reason: resolving counterexamples before committing to code. Two fresh exploratory trials test this ordering issue and retention of the observation bridge; they are reported separately from the three-run cohorts.
