# Single-file examples and Sol bundle trial

Frozen before solver delegation; twelve fresh agents, no parent history or hidden feedback.

- Format trial: four `gpt-6-luna` / high agents, FIFO and VCS once each with the prior reviewed multifile skill or its single-file version. Both use native Snappea and the identical skill sentence. This exploratory trial measures actual source reading and qualitative adoption (requested reads versus returned source visible within truncation; driver/monitor ranges separately), not a causal correctness estimate.
- Bundle trial: eight `gpt-6.1-sol` / medium agents, FIFO and VCS twice each, compatibility CLI without skill versus native Snappea with the new single-file skill. This changes both CLI and skill; differences cannot be attributed separately.

Skill sentence: “Use the P modeling skill at /opt/p-modeling/SKILL.md.” No other added design instructions. Every solver is forked with no history and uses only its isolated bridge. Same public task contracts and hidden graders as the preceding example trial, 900 seconds and 80 calls, FIFO 90 second command budget, VCS 120. No replacement trials for modeling failures. Mount immutable skill snapshots, no network, same docs/reference content and Python observation backend. CLI commands and FIFO TASK command syntax adapt only to the chosen CLI.

Pinned native binary b7ea15b06ee99225277adaeeee4dda33b2d2e6a6484b22ec3c13d9973d051fb2;
image sha256:00dbf3678a1e6fc660c1cc87fe05f9a8d2cdaeda2daa73fedd9941bb55ce5431. Compat binary f969f66ae7dc03a39823113fff33c0a94e4da61918253be173070d73024f8092, matching the prior Sol CLI cohort. Multifile skill snapshot 827079afa5ecc92094b1aef194cbcf1392cc8cfe358a8cb27b9af60e1a7d2fec. Single-file snapshot 4de6d74d7e1d9c3aee3fae5587eb9879e5f7a7649708edd9d5a71e551330e506 was frozen after full validation and independent review (source commit 1c4a357).

Outcomes: original hidden implementation scores, independent FIFO monitor histories, saved tests and portable seeded-fault probes where tests accept a correct reference; source quality (monitor selection, independent oracles, real load/candidate/CAS/retry, completion, actual Python-to-P capture); transcript evidence of syntax/example reads, first P check and first Python write timing, compiler/checker repair and scope reduction. Source-first review before grades when practical; unblind afterward. Do not infer private reasoning.

Preserve original grading. Uniformly apply the existing supplemental FIFO model-source assembly check (legal internal event declarations may otherwise be stripped) and separate VCS wide-bundle probe with a 10 second async deadline. These do not replace primary scores. Primary cases have known ceilings; do not summarize perfect case scores as proof of robust models. Report incomplete search, coverage bypasses, execution/resource failures and rejected valid histories explicitly.

Postfreeze grading-method correction: source-first review found one FIFO submission
with legal record-type aliases prepended before the unchanged public event block.
The previous supplemental assembler supports only appended events and would drop
these declarations. Preserve original grades and prior assembler; this cohort's
local supplemental assembler retains the exact public declaration block once,
permits only nonconflicting extra type/event declarations before/after it, and
rejects executable code, schema changes and suppressions. Six regression tests
cover valid aliases/nested types, unchanged/missing candidates, conflicting names,
changed/repeated schema, executable code/suppressions and malformed type syntax.
Apply uniformly to all six FIFO submissions with identical native 5 second checker
and 5 second mutation budgets. This corrects source assembly, not model behavior or
thresholds; keep original and supplemental outcomes separate.

The same restoration bug also affects original saved-test execution, monitor
histories and portable-test reference checks. Apply the same declaration-only
assembly uniformly to those three axes for all six FIFO candidates using archived
grader functions, cases, seeded variants and their original 90-second execution
limit. A temporary PUBLIC directory changes only events.p assembly; runtime.py
remains the original. Preserve original results and distinguish setup failures
from assertion rejections. Do not rerun or replace the pure Python implementation
contract scores. Record helper/source/schema hashes for these separate outcomes.
