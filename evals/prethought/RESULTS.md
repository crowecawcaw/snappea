# Written failure scenarios: better early notes, no reliable quality gain

Eight fresh Luna/high agents, two tasks × two prompts × two repetitions. Native
Snappea, no skill, identical within-task contracts, tools and budgets. Treatment
adds only:

> Before modeling, write a short design note listing the main failure modes and
> concrete scenarios that would expose them, including concurrency. Use it to
> guide the model and tests.

Treatment runs show earlier, more scenario-oriented written notes, but this pilot
does not show a consistent improvement in correctness or model quality. Some treatment models
have richer scenarios; others omit selected properties and planned interference.
The original implementation evals again have a ceiling. Two runs per task/arm
cannot establish either an effect or the absence of one.

- [Frozen protocol](PROTOCOL.md), [reproduction](README.md), [verified records](results.json).
- Source-first reviews: [FIFO](AUDIT-FLOW-source.md), [VCS](AUDIT-VCS-source.md).
- Then-unblinded transcript reviews: [FIFO](AUDIT-FLOW-transcripts.md), [VCS](AUDIT-VCS-transcripts.md).
- Separate source/transcript hash manifests accompany both reviews.

## Executed outcomes

Each cell describes two submissions. Different axes are not one combined score.

| Outcome | Planning sentence | Control |
| --- | --- | --- |
| FIFO implementation passes all 8 hidden cases | 2/2 | 2/2 |
| VCS implementation passes all 20 hidden cases | 2/2 | 2/2 |
| VCS existing supplemental wide-bundle check passes | 1/2 | 1/2 |
| FIFO saved Python suite passes | 1/2 | 2/2 |
| FIFO P tests select QueueContract | 0/2 | 2/2 |
| FIFO models separate load/version-check phases | 0/2 | 2/2 |
| FIFO bounded model recheck completes and passes | 1/2 | 2/2 |
| FIFO monitor accepts 4 valid / rejects 4 invalid independent histories | 2/2 | 2/2 |

The uniform FIFO recheck uses the original 5-second checker and 5-second mutation
budgets with the source-assembly correction below. A passing check with an
unselected contract monitor is not equivalent to checking that contract. Explicit
machine assertions remain, but the treatment models' assertions are much narrower.
The control load/CAS abstractions also differ: A02 passes only a version to clients
and computes effects inside Store, rather than modeling full stale candidate state.

One treatment FIFO run (`flow-b02`) batches unittests and a 90-second checker in a
single command under a 90-second command cap. The bridge aborts at 90 seconds,
before the full 900-second run budget. It remains a scored trial, not an excluded
infrastructure replacement. Independent grading runs its saved tests and finds
wrong expectations: a nonempty take is expected to return None, and a memoized
rejected put is expected to succeed after space opens.

Only FIFO planning A01's saved tests accept the private reference: they reject
2/3 seeded faults. Both controls additionally require constructor-capacity mismatch
rejection, which the public contract does not require the private reference to
implement. B02's suite has further invalid assertions. Those reference-incompatible
suites cannot receive meaningful portable-mutation scores; do not interpret them
as zero detections or compare 2/3 against zero. All candidate implementations still
pass the independent hidden valid-input implementation cases.

## Did the note influence the work?

All four treatment agents write a concrete scenario note before their first P
source. Both FIFO controls also write DESIGN before P, but after writing Python
within the same shell command; their notes mainly explain the algorithm. Neither
VCS control leaves an early written note. This is a comparison of an explicit early
failure-scenario request against the existing design-document requirement, not
planning against an absence of all planning.

Concrete follow-through is mixed:

- **FIFO planning A01:** its note explicitly says to expose load/CAS contention.
  The final model makes whole queue operations atomic and suppresses the unselected
  monitor lint. Python assertions do add useful rejection-after-space and
  empty-take-after-refill witnesses. The saved suite writes observations but does
  not replay them automatically; the agent runs strict replay separately.
- **FIFO planning B02:** its note proposes meaningful delayed retries and competing
  operations. Independent clients and useful duplicate-call tests appear, but
  the version counter never controls CAS, the P contract monitor is unselected,
  and later saved assertions contradict the memoized-rejection rule in the note.
  Client receipt order is also unsuitable as the model's linearization witness.
- **VCS planning B01:** the early same-parent conflict scenario becomes two clients
  publishing to an atomic ledger. Real inline runtime probes exercise competing
  publishes, crash retry and chunked large updates. Storage-level interference and
  most listed lifecycle rules remain outside P.
- **VCS planning A02:** two writers first read the same root before competing CAS
  attempts; a separate model checks revision reset/stale evidence. This adds an explicit same-root read/CAS
  race absent from both controls' P models. However the CAS loser is dropped rather
  than retried, only A has a simplified success-retry path, and encoding/chunking
  stays unmodeled. Despite listing the record-size hazard, its implementation
  assumes a 64-branch event fits and fails the existing wide probe.
- **Controls also do substantive work:** both FIFO controls retain the selected
  contract and separate load/CAS phases. VCS control B02 develops revision/CI
  scenarios, a selected at-most-once application monitor, inline crash/concurrency
  probes and a default mutation run without the added sentence.

None of the VCS agents saves a custom Python test file, although three execute
inline runtime probes. The VCS task does not explicitly require saved custom tests;
this is an observability/reproducibility distinction, not an instruction violation.

## Model-first instructions still do not reliably control workflow

The FIFO task explicitly says to model the contract before implementation. All four
agents write Python before their first P source, including both treatment agents
that wrote the requested note first. A note is not a substitute for design in P.

Only VCS planning B01 successfully checks its service model before writing Python.
A01 writes Python before any P source. A02 and B02 attempt P snippets first but
obtain successful service-model checks after implementation. No private reasoning
is available: these statements concern visible files and commands, not intent.

## Concrete tooling and measurement findings

1. **Module/test discovery consumes many repair attempts.** All VCS agents read
   CLI help but repeatedly guess module/test grammar. A particularly actionable
   diagnostic loop suggests test `tcCounter` as a repair for missing module
   `Counter`, then suggests machine `Counter` when that also fails. Neither is a
   module. Suggest only symbols of the required kind, and show a minimal correct
   module/test declaration when no useful module candidate exists.
2. **A spec in module membership gets misleading repair advice.** Two VCS runs
   receive “could not find interface Safety/Safe” for putting a spec in the module
   machine set. The useful repair is to remove it from membership and select it in
   the test's `assert`, rather than declare an interface. Test/module confusion
   also precedes both FIFO treatment agents dropping monitor selection.
3. **Exercised guards do not imply independently checked requirements.** VCS
   control B02 reaches full baseline coverage, yet its final mutation report has
   37/43 mutants undetected, including removing certification resets or all
   successful publication. These mutant searches are bounded and incomplete, not
   proofs of survival/equivalence. The agent returns after receiving the report.
   This is feedback actually available but not acted upon, not absent tooling.
4. **The grader assumed a source-file layout.** FIFO control A02 preserves all
   three supplied event declarations verbatim and appends internal event types to
   events.p. The old grader restores original events.p and falsely reports an
   undeclared eLoad. Public TASK does not prohibit this organization. A source-first
   reviewer confirms that it preserves the public schema.

Original grades remain untouched. `recheck_model_sources.py` uniformly checks all
four frozen FIFO submissions, retaining only nonconflicting appended event
declarations and using the same original checker settings. Only A02's source
assembly changes; its three selected tests complete and pass. The results are
separate under `supplemental-model-source-v2`, with raw JSON, hashes and provenance;
`results.json` keeps original and supplemental axes distinct. The reused container
helper's raw `status` is a unittest classifier; model verdicts derive from checker
JSON, selected tests, search completion and exit code, as in the original grader.
No checker, library, prompt, fixture or skill was changed during the trials.

## What to try next

Keep the note as a cheap design aid if useful, but do not rely on it to improve
models. This cohort suggests more direct opportunities:

- Fix the concrete module/spec diagnostic loops and make one valid test + monitor
  skeleton readily discoverable. They diverted both arms into syntax repair and
  preceded monitor opt-outs.
- Make selected properties conspicuous in success output, so “passes” with no
  selected contract is harder to mistake for contract validation.
- Test generated bounded drivers separately. They can remove actor, scheduling,
  identity and completion plumbing, while independent properties and model fidelity
  still require their own validation.

## Validation and evidence

Five narrow source-assembly tests pass (unchanged schema, valid additions, altered
schema, duplicate declaration and non-event code). All eight prompts have the
same single-prefix treatment delta after normalizing run ID; paired public/private
inputs, binary/image and budgets match. Original archived harness/source hashes,
submission manifests, public-schema preservation and reviewer hashes are verified.
A02's appended internal events are recorded, not hidden as byte-identical inputs.
Original + supplemental grades, complete tool transcripts, initial notes embedded
in writes, final sources, final agent responses, pinned native binary and mounted
reference sources are archived at:

`/workspace/library-files/snappea-prethought-evals-evidence.tar.gz`
