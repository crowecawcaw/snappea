# Source and transcript audit

Read frozen source before prompt assignment or hidden results. Cite files/lines.
No numerical quality score: describe concrete behaviors and omissions.

| Dimension | Evidence to seek |
| --- | --- |
| Concurrency fidelity | Independent clients; distinct load/propose/CAS steps; failed CAS recomputes; which interference can actually occur |
| Oracle | Selected independent monitor; checks public outcomes against contract; no copied implementation decisions or self-validating assertions |
| Scenarios | Delayed retries across changes; negative outcomes retained; identities/arguments preserved; lifecycle transitions; inputs make faults observable |
| Implementation link | Same atomicity/recovery as P; real observed calls; valid concurrent-history ordering |
| Validation | Nonvacuous finite work; selected monitors; actual executed tests; complete vs incomplete searches |

Then inspect tool transcripts and assignment:
- First written note (filename/call) vs first P source write/check and Python implementation write.
- Did the note identify a concrete witness or just name a category?
- Trace named scenarios into model/test code. Label planned-but-absent cases.
- Distinguish existing spontaneous planning in controls from treatment adherence.
- Record feedback-driven changes separately from planning-driven choices.
- Only infer visible behavior; tool transcripts do not expose private reasoning.

Finally unblind hidden results. Explain mismatches between prose claims, source
coverage and executed outcomes. Do not count compiler errors as defect detection.
