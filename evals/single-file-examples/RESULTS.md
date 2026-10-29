# Single-file examples and Sol package comparison

Single-file examples brought agents to actual P source earlier, but did not increase
observed reading frequency: both Luna agents in each format eventually read source.
Sol with native Snappea + skill produced stronger model validation and completion
checks than compat without skill in these trials. Implementation scores did not
improve. This is an exploratory twelve-run comparison, not an estimated effect size.

## Comparisons and controls

- **Format:** four fresh `gpt-6-luna` / high agents: FIFO and VCS once per format,
  native Snappea and the same skill sentence in both arms. Multifile snapshot
  `827079afa5ecc92094b1aef194cbcf1392cc8cfe358a8cb27b9af60e1a7d2fec` versus single-file
  `4de6d74d7e1d9c3aee3fae5587eb9879e5f7a7649708edd9d5a71e551330e506`.
  Header comments and direct source links changed together; modeling behavior was
  independently checked for equivalence before the skill was frozen.
- **Package:** eight fresh `gpt-6.1-sol` / medium agents: two FIFO and two VCS per
  arm, compat without skill versus native Snappea with the new skill. This changes
  CLI and skill together; it cannot isolate their contributions.
- Existing tasks/hidden graders, identical Python backend, pinned binaries/image,
  isolated bridges, no network or parent history, 900 seconds/80 calls each.
  All twelve prompts/inputs frozen before delegation. No hints, replacements or
  tool/content tuning during runs. No solver infrastructure failures.
- Sources reviewed before external assignments/transcripts/grades were disclosed.
  Some DESIGN files themselves name the CLI, so this was source-first with external
  assignments withheld, not guaranteed condition blinding.

[Protocol](PROTOCOL.md), [verified results](results.json), [reference visibility](uptake.json),
[FIFO source audit](AUDIT-FIFO-source.md), [FIFO transcript audit](AUDIT-FIFO-transcripts.md),
[VCS source audit](AUDIT-VCS-source.md), [VCS transcript audit](AUDIT-VCS-transcripts.md).

## Did the new examples get read?

| Luna task | Multifile skill | Single-file skill |
| --- | --- | --- |
| FIFO | README, wrong filename guess, actual protocol/Store at call 7; never reads CAS driver | Direct Balance.p request at call 3; batching clips its middle/driver, but bounded-history driver is visible in the tail |
| VCS | README, wrong filename guess, Store/driver at 13 and protocol at 14, after Python | Whole Revalidated.p visible at call 3, before modeling/Python |

Both formats: **2/2 agents eventually see actual example P source**. This trial supports easier
access, not a measured increase in reading probability. Both new-format Luna agents
attempt P checks before Python; both old-format agents write Python first. A first
check attempt does not mean the design/model was already complete.

Visibility matters: Luna FIFO requested 29,787 characters together, but the bridge
returns only the first/last 9,000. Balance's later CAS handling, retry barrier and
generated/targeted tests never reached it. All 291 Balance lines did reach both Sol
FIFO agents at call 3. Sol VCS trial 1 also saw the full file; trial 2's batch clipped
some middle client code while retaining Scenario/test sections. Reading delivered
text is not proof of attention, understanding or copying.

Both Sol FIFO skill agents still guessed nonexistent `reservation/test_observation.py`
instead of `test_storage.py`, then introspected library helpers. Single-file P
packaging does not solve every reference-discovery issue. Luna FIFO compile/setup
friction also did not improve: seven checker-ending exit-code-2 calls with the new format
versus three with the old. Do not claim a syntax-success effect.

## Scores and model evidence

Every FIFO implementation passes **8/8** hidden cases; every VCS implementation
passes **20/20**. Every saved FIFO suite passes its own implementation once legal
source declarations are preserved. All four Sol VCS implementations pass the
separate wide-bundle probe. The Luna single-file VCS fails it with a 16 KiB record
error; multifile passes. Source reading alone did not fix valid large-operation
handling.

| FIFO run suffix | Condition | Monitor histories | Uniform native 5s model check |
| --- | --- | --- | --- |
| luna-flow-multi-01 | Multifile skill | 3/4 valid + 4/4 invalid | Full required coverage; incomplete search |
| luna-flow-single-01 | Single-file skill | 4/4 valid + 4/4 invalid | Both selected tests complete; full coverage |
| sol-flow-compat-01 | Compat/no skill | 4/4 valid + 4/4 invalid | Six uncovered memo-hit points; incomplete search |
| sol-flow-compat-02 | Compat/no skill | 4/4 valid + 4/4 invalid | Full coverage; incomplete search |
| sol-flow-single-01 | Native/skill | 4/4 valid + 4/4 invalid | Two focused tests complete; competing test incomplete; full coverage |
| sol-flow-single-02 | Native/skill | 4/4 valid + 4/4 invalid | Complete; full coverage |

Run IDs prefix each suffix with `onefile-`. The short uniform check is supplemental,
using native tooling for all frozen FIFO models, not feedback compat agents saw.
Sol native FIFO trial 1 completed all three tests in its own 30s check. Short-budget
incompletion is not a counterexample. Luna multifile's valid-history rejection is a
real monitor overconstraint: it requires zero in the irrelevant item field of an
empty take. It is not an implementation failure.

## Qualitative Sol difference

- **Useful completion:** native/skill 4/4 retain hot/count obligations; compat 0/4.
  Native agents require actual nonzero work/replies and inspect bounded mutation
  survivors. Both native FIFO agents strengthen operation/snapshot counts after
  omitted-work mutations survive; trial 1 also counts actual client replies. Their
  bounded detections change
  52/64→59/64 and 58/68→64/68 on evolving models; these are feedback-uptake examples,
  not normalized scores or proofs that remaining mutants are equivalent.
- **VCS:** native/skill 2/2 use independent selected state/result monitors, delayed
  competing replay, completion and saved real Python-to-P histories. Compat 2/2
  model real read/candidate/CAS/reload and have good ordinary concurrency/crash/
  payload tests, but no real P capture or completion. Their internal transition
  invariants are useful; one also compares candidates with the same Apply function
  that builds them, weakening independence. Neither native model covers the full
  evidence/crash/storage implementation; one publish ID per bundle also leaves a
  fresh request against an older published bundle outside its modeled workload.
- **FIFO:** actual Python capture is strong in both Sol arms, as TASK requests it.
  Compat agents independently build interval-aware serialization and force real
  CAS conflicts; one has the strongest saved negative controls for that Python
  oracle. There is no consistent native advantage in ordinary concurrency tests.
  Native models add actual response checking and useful completion. First P check
  precedes Python in native 4/4 versus compat 2/4 across both tasks.
- **Luna format:** the new FIFO finishes two smaller scenarios, but reduces
  inspections/concurrent workload while reaching completion. Both FIFO models
  still fail to independently check actual replay reply payloads. Both VCS models
  use atomic service abstractions rather than concrete client read/CAS retry loops.
  Whole-file reading did not guarantee faithful adoption of the example protocol.
  Old Luna VCS really ran extra inline growth/crash/concurrency probes even though
  it saved no extra test file; absence of a saved file is not absence of testing.

The package looks helpful for Sol's model/oracle discipline here. These artifacts
do not establish better implementation correctness, and the format trial is mixed.

## Grading correction and limits

The original FIFO grader restores events.p wholesale, discarding legal internal
events and two prepended record aliases in the Luna submissions. That causes model,
saved-test and monitor-history setup errors. Original grades remain unchanged.
Uniform supplemental assembly retains the exact original schema once and permits
only nonconflicting type/event declarations before/after it; executable code,
suppressions, changed schemas and malformed types are rejected by six regression
tests. The same repair was applied to all six FIFO models and their saved tests,
histories and portable-reference checks, using archived cases/limits. The pure Python hidden
contract scores were not rerun. Helper/schema/source hashes record the correction.

Only multifile Luna FIFO accepts the portable reference and rejects 2/3 seeded faults;
it misses the empty-take memo fault, matching its saved delayed-retry gap. Other
suites depend on real application emission or expect additional API validation /
constructor-race behavior absent from the reference. Their portable mutation
results are **not interpretable**, not zero detections; useful own tests were not
weakened to make a reference score. Per-model mutation populations and incomplete
survivor searches likewise cannot be compared as a common fault benchmark.

Primary correctness cases have a known ceiling; one/two trials per cell cannot
estimate a causal effect. Keep source fidelity, reference visibility, completion,
coverage, bounded search, real capture and ordinary implementation tests separate.
