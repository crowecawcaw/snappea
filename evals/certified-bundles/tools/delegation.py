"""Current collaboration prompts. Completed runs retain their original delegation.txt."""
from pathlib import Path
from prompts import task_prompt

def prompt(run_id, arm, condition='legacy'):
    bridge = Path(__file__).resolve().parent / 'solver_shell.py'
    return task_prompt(arm, condition) + f"""
Run ID: {run_id}. Work independently; do not spawn or contact other agents.
Use only this shell bridge for commands and file access; do not inspect host files,
other runs, or the bridge itself. Commands run in your isolated /work container:

python {bridge} --run-id {run_id} <<'SOLVER_COMMAND'
cat TASK.md contract.py
SOLVER_COMMAND

The bridge returns stdout, stderr and exit_code. Budget: 900 seconds from the first
call and 80 calls. Return when finished; files in /work are your submission.
"""
