# Written scenarios before P modeling

Question: does a short request for written failure scenarios improve the models
and tests produced by Luna/high?

- [Results](RESULTS.md) and [verified trial records](results.json)
- [Protocol and exact prompt addition](PROTOCOL.md)
- [Source/transcript review criteria](AUDIT-RUBRIC.md)
- `experiment.py`: reuse the FIFO and VCS harnesses, adding only the planning
  sentence in the treatment arm; verify matched inputs before delegation.
- `summarize.py`: verify frozen artifacts and extract primary outcomes.
- `supplement.py`: run the existing wide-bundle stress separately on frozen VCS
  submissions. It does not change the original 20-case primary score.

Start with `python evals/prethought/experiment.py start`. Each generated prompt
must go to a fresh `gpt-6-luna` agent with high reasoning and no inherited eval
context. Do not give agents the protocol, graders, other runs, or hidden results.
Finish each run only after its agent stops:

```sh
python evals/prethought/experiment.py finish --run-id ID
```

If a FIFO command times out, retain the trial and freeze with its actual status:

```sh
python evals/modeling-skill/phases/runner.py finish --run-id ID --status command_timeout
python evals/modeling-skill/phases/grade.py --run-id ID
```

Grader nonzero exits can mean a scored invalid submission; inspect the generated
report rather than dropping or rerunning the trial. Audit source before unblinding
assignment and grades. Use transcripts to verify when the note was written and
which planned scenarios reached executable tests; do not infer private reasoning.
After all runs are frozen:

```sh
python evals/prethought/recheck_model_sources.py
python evals/prethought/supplement.py
python evals/prethought/summarize.py
python -m unittest discover -s evals/prethought -p test_protocol.py -v
```

The model-only recheck fixes source assembly for nonconflicting internal event
extensions, uniformly for all four FIFO submissions. It preserves original grades.
Its reused container helper has a unittest-specific raw status; interpret model
verdicts from checker JSON and exit status, as summarize.py does.

Raw runs and provisioning maps are ignored by git and should be archived intact.
