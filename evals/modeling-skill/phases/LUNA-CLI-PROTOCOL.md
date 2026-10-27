# Luna/high: Snappea versus compat, no skill

Frozen comparison planned before solver runs:

- Model: `gpt-6-luna`, high thinking, fresh isolated agents.
- Tasks: existing shared FIFO flow and certified-bundles VCS design/build.
- Two trials per task per CLI: eight submissions. No skill in either arm.
- Source: `41b89c3`, merging main `a07b926` (driver-limit reports).
- Same public contracts, private cases, image, budgets and Python observation
  library/backend in both arms. Only selected CLI is on PATH. The native
  Python backend is library plumbing, not a second CLI for the solver.
- CLI commands use their actual names and accepted syntax; no option translation
  wrapper. Ordinary reference material is identical within each task.
- Native Snappea's defaults and compat's upstream defaults remain intact. This
  compares the whole CLI experience, not the isolated driver-report feature.
- Existing graders use the same native checker for both frozen submissions.
  Save the agent's own command outcomes separately from independent grading.
- Budgets: 900 seconds/80 calls; FIFO commands 90 seconds, VCS commands 120 seconds.
- Do not feed hidden results back to solvers or tune tooling during the cohort.

Assess primary hidden implementation cases, independent monitor histories where
available, saved tests, and fixed-budget native model checks. Audit source and
transcripts for contract fidelity, independent oracles, actual implementation
observations, varied generated operations, retry/lifecycle/progress omissions,
and concrete changes made after feedback. Specifically distinguish reading a
driver report from changing the driver in response to it. Search completion and
coverage are not proxies for contract completeness. Do not infer a causal effect
size from two trials per cell.

Any new source-driven probes are exploratory and reported separately from the
existing hidden cases. Preserve prompts, commands, outputs, tool/input hashes,
frozen submissions and grading artifacts.
