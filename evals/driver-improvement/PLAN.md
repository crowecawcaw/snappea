# Driver improvement pilot

Question: does showing driver limitations help an agent strengthen an existing
model's tests? Also inspect tooling blind spots and instruction adherence.

## Initial design

- Three original fixtures: stable priority ordering, durable configuration
  request outcomes, and two-client/two-resource ownership.
- Each supplies a correct implementation, independent protected monitor, weak
  passing driver, and explicit interface/contract. Agents edit only driver/tests
  and write a short review. No modeling skill is supplied.
- Fault inventories come from the contract before driver-report inspection.
  Each fixture also has one fault already caught by its initial driver, added
  during preflight review to measure regressions, and a distinct correct variant.
- Private reference drivers must detect every fault and accept both correct
  implementations. Missing completion and invalid inputs must fail correct cases.
- Initial scores: priority 1/6 (five latent faults), retry 1/4 (three latent),
  ownership 1/4 (three latent). Reference scores: 6/6, 4/4, 4/4.

## Report ablation

- Same pinned native Snappea binary and image as the preceding CLI trials.
- Same CLI arguments and collection in both arms; the common wrapper hides only
  the driver facts in the report-off arm, including JSON sidecars.
- Common two-second search, mutation disabled, coverage gate bypassed for this
  review exercise. Coverage is still reported in both arms. Priority and retry
  initial drivers also pass the strict coverage gate; ownership has branch gaps.
- Both JSON outputs pass through the same wrapper. No native tool changes.
- Budget: 480 seconds, 40 commands, 30 seconds per command; fresh isolated agents.

## Trials and analysis

- First wave: six Sol 6.1 / medium agents, one per fixture/report condition.
- No hidden results or hints are returned to solvers. Freeze before grading.
- Grading restores the original algorithm/monitor/interface for every variant.
  Credit actual assertion failures, not compile, lint, coverage or harness errors.
- Report new latent faults detected, previously detected faults lost, false
  positives on both correct variants, incomplete searches and instruction evidence.
- Review source and transcripts: report received versus used; scenario changes
  versus weaker assertions; invalid tests versus tooling failures.
- Choose any follow-up after inspecting this wave. Preserve it as a separate
  exploratory cohort; do not pool adaptive revisions or claim effect sizes.

The fixtures may be easy for Sol. A ceiling is a result; do not remove successful
cases or choose faults afterward to manufacture a report advantage. Report-blind
fixtures remain in the study to expose tool limitations.
