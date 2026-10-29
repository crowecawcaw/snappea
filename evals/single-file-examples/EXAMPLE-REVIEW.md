# Independent single-file example review

Reviewed by `/root/review_singlefile` before the eval skill snapshot was frozen.
Compared the pending refactor against commit `8faab39` and read the resulting
models, headers, project files, scripts, guides and direct skill links.

- **Behavior preserved:** independently stripped comments/whitespace and compared
  concatenated P token sequences. CAS, durable retry, bounded history and both
  versioned-publication files are unchanged. Reservation changes only the broken
  variant's `Store`/`Client` names to `StaleStore`/`StaleClient` so both variants can
  share a file.
- **Paths and selection:** project/input paths, Python capture spec path, temporary
  fault controls and validator references match the consolidated sources. Header
  commands explicitly select positive tests where a file also includes deliberate
  negatives. Skill and guide links now open executable P directly.
- **Guidance:** headers explain invariants, independent expected state, actual
  response/state observations, scheduling and storage boundaries, workload bounds,
  progress scope, and how to run positive/negative controls. No stronger correctness
  claim than the actual models support.
- **Correction:** reservation initially said the monitor “starts hot,” although
  `BeforeOpen` is cold. Corrected to “stays hot after opening until the first
  reservation.” This was comment-only.
- **Validation evidence:** reviewed `/tmp/single-file-example-validation.log` from
  the author's full validator run: positive models, intended assertion failures,
  manual valid/invalid oracle histories, completion and actual-reply regressions,
  and Python actual-capture tests all pass. No repeated full run was needed for the
  comment-only correction.

No blocking findings remain. Sources are 159–464 lines; header maps identify oracle,
algorithm, driver and test names, but a partial read can still omit a driver. The
eval should distinguish requesting an example from actually reading its relevant
sections. Cleared the author and eval coordinator to freeze the skill.
