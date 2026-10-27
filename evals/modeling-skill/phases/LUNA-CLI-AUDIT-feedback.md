# Luna/high CLI comparison: feedback audit

Eight frozen runs, no modeling skill in either arm. This audit reads saved commands,
outputs and final source; it does not see private model reasoning. Call numbers are
from each run's `tool-transcript.jsonl`. See [protocol](LUNA-CLI-PROTOCOL.md).

## What the new report changed

- All four native runs received complete, visible driver facts. Relevant outputs
  fit the bridge limits: FIFO 18,000 stdout characters, VCS 16,000. Receipt alone
  is not evidence of deliberate uptake.
- **VCS native01:** call19 combines coverage failures with facts saying approve/CI
  never follow publish. Call21 adds approve and CI after publish, fixing coverage
  and eliminating those order facts. This is a useful expansion following **mixed
  coverage and driver feedback**; the transcript cannot isolate which caused it.
- **FIFO native01:** after a passing report57, call58 gives previously inert client
  actors real work. This is real driver rework after feedback, but its motive is
  not stated. Incomplete searches59/60 lead to a smaller competing-client workload61.
  Fixed operation/ID/value lists remain; no generated history choices are added.
- **VCS native02:** feedback10 lists unexercised published-retry and success branches
  plus `mustWin=false` constructor values. Call11 removes the branches/assertion
  rather than expanding inputs. The final constructor-value limits remain.
- **FIFO native02:** actors expand36 after a model assertion failure35, before the
  first driver report37. No further driver expansion follows the visible limits.
- No run turns finite-value/order facts into `$`/`choose` operation generators.
  No cohort-wide driver-report benefit is established. Other native validation
  features also differ from compat, so this is not a report-only experiment.

## FIFO command/source histories

### native01

- Help3/4. P source6 precedes Python32; first accepted type-check31, first model
  search33. A failed skill-file lookup5 obtains no content.
- Calls7–31 are mostly P syntax/type/module repairs, not CLI-option guessing.
  Calls35–45 repeatedly guess sequence construction; several replacements narrow
  snapshots to empty results instead of fixing general construction.
- Coverage33/38/49 prompts added scenarios and snapshot work. Bug53 is an invalid
  sequence-index operation, replayed54 and read55. Call56 restricts snapshots to
  empty and suppresses populated-monitor coverage;57 passes. Calls65/66 try removing
  the suppressions/adding snapshots, but another index bug70/71 leads72 to the same
  empty-only model plus explicit coverage suppressions. These are declared omissions,
  not proof of populated-snapshot model coverage.
- Driver rework58/61 follows the visible report57; final73 completes 11,516 schedules.
  Facts still say no Main put after take/snapshot and list fixed IDs/values.
- Saved tests use actual serial Python results and run `snappea observe` against
  the same monitor, including populated snapshots. Concurrency tests check capacity
  with ordinary Python assertions, not interval-respecting observation replay.
- Final75 runs mutation checking:35/50 detected; the15 undetected have incomplete
  mutant searches. No subsequent repair:76 only reads final files. Do not treat
  survivors as proved equivalent or this as a complete mutation score.

### native02

- Help3/4/6; Python7 precedes P8. Calls8–31 repeatedly guess sequence and test/module
  grammar. Deleting the test34 triggers `unselected-spec`;35 suppresses it for
  observation-only use. The model check consequently has no QueueContract monitor.
- Model failure35 is `put retry changed item`: clients reuse one ID with different
  values. Calls36/37 assign per-client IDs, add retries and a third client. This
  repairs scenario validity/expands coverage before driver facts first appear37.
- Reports37/38/43/44/49/50 are explicitly `search_incomplete`, with fixed three-client
  constructor values. Later model edit43 adds an atomicity comment only; no generated
  choices or additional operation sequences follow those reports.
- Python test39 exposes a wrong expectation for retrying a previously rejected put;
 40 corrects it and adds a fresh request. Observation setup40 fails because flat
  fields do not supply the expected payload;41 writes keyed nested payloads and passes.
- Saved tests really check Python results through `snappea observe` and QueueContract.
  They do not use the Python library; the model monitor stays intentionally unselected.
  Every model search uses `--no-mutation`.

### compat01

- Help3/4; P5, Python14, successful model check27. P syntax/type repairs6–11/16/18–25
  include replacing the split read/CAS attempt with an atomic handler model16.
- Test grammar guesses21–24 end by deleting the test declaration24. Successful
  listing26 is `DefaultImpl`; QueueContract is never selected. Snapshot handler is
  removed26 after payload typing trouble. Successful checks27/29/31/32 explore one
  schedule. Adding a third put32 is manual, without driver feedback.
- Saved tests record actual Python calls and compare expected values, but never
  execute the P monitor. Neither coverage nor monitor selection is validated by
  the reported zero-bug compat result.

### compat02

- Help3/4/5; Python6 precedes P/monitor7 and stateful model16. Test/module syntax
  recovery8–12 eventually selects QueueContract correctly.
- Python history order/count assertions13/14 fail;15 fixes the capture/expected list.
  P snapshot/client syntax trouble16–21 leads to deleting the snapshot handler19
  and repairing client/test declarations. Checks22–24 pass; final27 uses20 schedules.
- Saved Python events preserve real calls but only compare expected dictionaries;
  they never execute P observation validation. No driver/coverage/mutation feedback
  is emitted by these compat checks.

## VCS command/source histories

### native01

- Help4/6; Python7 precedes P9. Calls9–18 repair test/module grammar. Deleting the
  test12 causes native `no test case`13, and a selected test is finally accepted18.
- Native19 fails required coverage and shows missing approve/CI-after-publish order
  facts. Call21 adds both operations after publish, resolving the two covered-branch
  gaps and three order facts. Source stays a fixed single-BundleStore scenario.
- Checks21/23 pass with `--no-mutation`. Remaining one-instance/send-count limits
  are visible, without later expansion. No saved custom Python tests or P observations.

### native02

- Help4/7; Python6 precedes P10. It discovers/reads the ordinary inventory example8/9;
 10 already compiles and selects a monitor.
- Check10 reports unused published-retry and `mustWin` branches, plus constructor
  facts with `mustWin=false` only. Call11 removes the published memo/retry branch
  and the success assertion. Call12 removes leftover constructor arguments and
  passes, without expanding the scenarios. The scope reduction is concrete.
- The final monitor checks allowed two-head snapshots. It cannot validate full
  bundle lifecycle or fresh-request replay behavior. No saved custom Python tests.
  Both actual model searches use `--no-mutation`.

### compat02

- Help4/5. P6; missing events fixed7, before Python8. Compat7 runs20 seconds,
 8,997,283 schedules but **one timeline**, two scheduling points: `Main` initializes
  but receives no work. A separate `Test` raises events only to itself. Relevant
  Main handlers/assertions are unexercised. Final16 checks1,000 schedules of that
  same idle `DefaultImpl`; zero bugs is not meaningful protocol validation.
- Calls10–15 are Python storage-cost probes and layout changes, not model feedback.
  No saved custom tests, P observations, or independent property monitor.

### compat03

- Help4/5/18; Python7 and public tests8 precede P9. Calls9–25 repeatedly guess
  testcase/test/module/scenario syntax, eventually reducing the attempt to
  `assert true`; no valid selected model check is reached.
- Genuine CLI guesses26/28 use `check .`;29 comma-joins `--pfiles` and30 supplies
  multiple files as one space-containing string. These differ from P grammar errors.
- Call33 deletes both P files. Final DESIGN still describes a P store protocol
  and invariants, but no P source remains. No saved custom Python tests.

## Boundaries and provenance

- No scored run invokes `/opt/python-snappea` directly. No saved suite imports
  `snappea`; only native FIFO tests invoke the native observation CLI. Python
  library/backend availability is equal but is not the observed mechanism here.
- No modeling skill content was mounted or successfully read. Only VCS native02
  visibly reads the shared ordinary P example; both VCS arms had equal access.
- Excluded VCS compat01 froze after3 calls because binary stdout raised a UTF-8
  decoding exception. The preserved infrastructure failure is not a model failure.
  The bridge then adopted replacement decoding; compat02/03 are fresh scored runs.
- Source was read only after each metadata status changed from `running`. This
  is a transcript/source audit, separate from hidden correctness grading.
- Every final file listed in each submission manifest was SHA-256 verified. Full
  transcript and submission-manifest hashes below identify the exact command/source
  evidence; the manifest contains hashes of every source, test and design file.

| Run (`luna-cli-` prefix) | Transcript SHA-256 | Submission-manifest SHA-256 |
|---|---|---|
| `flow-compat-01` | `fc1c8e087c4bb2c33126990cf13af95f4ae2adfeab76c1087f744da500b343d4` | `6fa5cef42b20809fa1ea058cfb8822d521579a96196e21a15f38c3d9eb8c992e` |
| `flow-compat-02` | `bee7ccc5c03ae8546be410f0f50f4fdd7693269da76b69fe50115d510d3987a2` | `3378b4fbd613a106cdd5098782fea5e4c3602420d74df90d6b9f947349374fbc` |
| `flow-native-01` | `306e21cd4bcd42a5a12b5f251d105b512b28e81db68a3ebe34fc33a81d89a41a` | `61d4ff324764abe661207aa1a0ee5fdcfb1515051147d1740d8a7dedaf3cb2a9` |
| `flow-native-02` | `60138718dec33cb8a1e44c8f43f031b670cc74bd8caa04db3fe52a52f9d73439` | `27e95fe620bb3a8c7add55bebb86b7704be657968768a1096fb521b232e0b477` |
| `vcs-compat-02` | `1ea29fd1c4a889ae47d76bf1a6658c88a0f76ddcafa9e35ea6f6480476632ec0` | `815e41cc5e1d4b1d929639bf6390bd2efb4d98e247201af5cd2d28f919f471fc` |
| `vcs-compat-03` | `04fa3a17e0db584192d83d480b55215eb1b02f30cceb4b5092835caed84125e0` | `ee8756a5c508e8cd1bd8ad3cf02f2d04561f7d1278fd955b61c9155453cf2400` |
| `vcs-native-01` | `50bf3e95ff6c52c5de337375614dc6c31c5b3b28d32c631340325cf2437087ba` | `ce2818497683564c80917e09b13af07f1eda46b7cec45be27ae1c110d49410ca` |
| `vcs-native-02` | `128aa88fde0729ec3008a5ebd7f5bb533f60b96063f2ef0f294e9b51e6d2015b` | `33e6804a982fe6c77f35d2ba2a31d4ea383d861341d7d2eb1fe8332995945a24` |
