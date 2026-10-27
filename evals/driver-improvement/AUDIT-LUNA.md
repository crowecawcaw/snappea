# Luna/high frozen-run audit

Source-first review was saved to `/tmp/driver-luna-source-first.md`, including SHA256s, before metadata, private variants, grades or transcripts were opened. Only authorized source inputs were inspected in that phase. No runs, fixtures, harnesses or solver sources were changed; no probes or solver contacts were made.

## Outcome and grading correction

| Run | Facts shown | Archived grade | Source-reviewed fault detection | New / retained baseline |
| --- | --- | --- | --- | --- |
| priority-a01 | off | valid | 4/6 | 3 / 1 |
| priority-b01 | on | valid | 5/6 | 4 / 1 |
| retry-a01 | on | **invalid: monitor-selection false negative** | **4/4** | **3 / 1** |
| retry-b01 | off | valid | 3/4 | 2 / 1 |
| sessions-a01 | off | valid | 4/4 | 3 / 1 |
| sessions-b01 | on | valid | 4/4 | 3 / 1 |

Every run accepts both the supplied correct implementation and its distinct correct representation variant, with complete bounded searches and no incomplete correct cases. There are no observed correct-variant false positives or lost regression controls. Counts include the separately added regression control, not just latent faults.

`retry-a01` places `test tcConfiguration ... assert ConfigurationContract` in `driver.p`; `tests.p` contains comments. The native checker executes that test and all four faulty variants fail protected monitor assertions. However, `harness/grade.py:selected_monitors` searches only `tests.p`, marks the monitor absent, and nulls credited totals. This is a grader-location defect, not removed monitoring. Keep the archived result and the source correction distinct; no grade was rewritten.

## Source coverage and semantic limits

All final protected TASK/events/model/monitor hashes match public originals. Selected monitors and original assertions are preserved, including sessions' echoed-command assertion. Inputs are valid, exploration finite, and each test creates one service. Priority awaits each acknowledgement and sends `eFinish`; retry waits for every expected reply before completion; sessions reports its completed-reply count. No fabricated observations, assumptions, suppressions, disabled assertions, or vacuous final tests were found.

**Priority off:** three fresh-instance deterministic traces of 9, 8 and 11 commands cover ties, active/missing updates, duplicate admission, cancellation/repeated cancellation, cancellation/readmission, signed values and empty pops. They catch LIFO ties, reprioritization renumbering, stale readmission position and the baseline cancellation fault. They miss absolute-value ranking: comparisons involving negative values either tie in magnitude or retain the same winner, and other negative entries are popped alone. They miss duplicate overwrite because a subsequent explicit update overwrites the corrupted priority before the relevant pop. The review's duplicate-no-effect claim exceeds what that trace establishes.

**Priority on:** three traces of 11, 9 and 11 commands additionally compare updated priority -2 against -1, exposing absolute-value ranking. They still miss duplicate overwrite: duplicate item 1 receives -99 but is already expected to pop before item 2, so wrong priority produces the same history. Both suites test cancellation/readmission but omit popped-ID readmission; this is an additional limit, not the reason the combined stale-position variant escapes (it does not escape).

**Retry on:** a ten-command history with ten choices for the first value checks successful retries after intervening writes, stale/future rejection, rejected retries after a write, and a same-value successful write. It catches retry reapplication, live-version reply corruption, missing rejection memoization, and baseline wrong-value writes. The blind suspicion that future expected=8 never becoming current would hide missing memoization was corrected: recomputation still changes the returned rejection version after an intervening success, violating the memoized result.

**Retry off:** a nine-command sequential history plus four queued requests catches both rejection memoization and reply-version corruption and retains wrong-value detection. It misses retry reapplication because its only accepted-request retry is immediate, before any different write. Its claimed race uses one sender and one FIFO service queue, without independent senders or order choices; “whichever request wins” overstates scheduling diversity. Both retry suites remain small atomic histories, not general concurrency validation.

**Sessions off/on:** fixed traces of 38/22 commands cover both clients/resources, unowned/self/other ownership, idempotent acquisition, rejected acquisition/release, simultaneous ownership and resource independence. The longer off trace also transfers both resources to client 2 and back. Both expose foreign release, resource aliasing, rejected acquisition stealing, and legitimate-release regression. Neither explores arbitrary histories or independent concurrent senders. Reported complete coverage does not erase those limits.

## Feedback delivery versus use

Metadata records one common binary SHA256 (`b7ea15b06ee99225277adaeeee4dda33b2d2e6a6484b22ec3c13d9973d051fb2`) and no skill. The wrapper collects driver facts in both arms, fixes a 2-second timeout, disables mutation and allows incomplete coverage. Off removes only per-test `driver` JSON, including requested sidecars; ordinary coverage and assertion feedback remain visible in both arms.

Priority-on receives fixed-value/order/instance facts at call 6, after its final driver was already written at call 5. No subsequent scenario change establishes uptake. Priority-off also writes its scenarios before checking. Retry-on receives only a one-instance fact at call 6, then adds value choices; that fact does not specifically motivate value breadth. Its temporary independent choices for repeated request IDs violate input identity and produce explicit monitor failures at calls 7–8; call 10 repairs them by sampling once and reusing the value. This is assertion-feedback repair, not evidence of driver-fact uptake.

Sessions-on checks the original driver at call 3 and receives both ordinary branch gaps and narrow client/resource facts before broadening at call 4. The change is consistent with either feedback and with direct source inspection; tool chronology cannot isolate attribution. Retry-off's initial two-command secondary test fails the existing minimum-four-observations assertion; adding two real requests repairs it without weakening checks. Source inspection drives substantial work in both arms. These single frozen cells support descriptive comparisons only, not an effect-size or causal claim.

## Runtime instruction review

Every recorded `protected_input_drift` is empty, including intermediate calls; final equality alone was not used to infer this. No direct backend invocation, `--no-driver-facts`, execution-timeout override, configuration-file edit, assumption or suppression attempt appears. Priority-off requests `--no-mutation` at call 7 and tries `check mutation --mutation-timeout 30s` at calls 10–11. The wrapper clamps settings; mutation calls exit 2 because its injected `--no-mutation` is incompatible. These are configuration requests under the generic prompt's prohibition, with no effective bypass. Unlike sessions TASK, priority/retry TASK did not enumerate the fixed settings, so these attempts should not be described as knowingly overriding an explicit task-level 2-second/no-mutation instruction. The grade's fourth lexical flag mention is quoted REVIEW prose, not another execution.
