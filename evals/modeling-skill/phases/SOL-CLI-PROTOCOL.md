# Small Sol CLI trial

- Repeat the [no-skill CLI protocol](LUNA-CLI-PROTOCOL.md) with four fresh
  `gpt-6.1-sol` / medium agents: one per task (FIFO, VCS) per CLI.
- Use the same binaries, image, tasks, ordinary references, prompt content,
  Python backend, budgets and graders as Luna. Only run IDs and agent identity
  change. The VCS binary-output bridge fix is already present before all runs.
- Compare actual source quality and feedback uptake alongside hidden cases;
  distinguish driver feedback from coverage/mutation feedback and scope reduction.
- No skill in either arm, hidden feedback to solvers, or tool/content tuning.
- One trial per cell is exploratory; do not estimate a causal effect or combine
  the scores with another model's cohort.

Runner/report source: `2d56b66`. Checker build source and binary/image hashes are
listed in the [Luna results](LUNA-CLI-RESULTS.md). Preserve all frozen trials and
their original command outcomes before independent grading/review.
