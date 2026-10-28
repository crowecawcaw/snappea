#!/usr/bin/env python3
"""Matched written-prethought prompt trial; reuse the frozen task harnesses."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[1]
PHASES = REPO/'evals/modeling-skill/phases'
VCS = REPO/'evals/certified-bundles'
BINARY = REPO/'target/driver-eval/release/snappea'
IMAGE = 'sha256:00dbf3678a1e6fc660c1cc87fe05f9a8d2cdaeda2daa73fedd9941bb55ce5431'
SENTENCE = ('Before modeling, write a short design note listing the main failure modes '
            'and concrete scenarios that would expose them, including concurrency. '
            'Use it to guide the model and tests.')

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def write(path, obj):
    path.write_text(json.dumps(obj, indent=2)+'\n')

def call(args):
    env = os.environ.copy()
    env.update(SNAPPEA_EVAL_BINARY=str(BINARY), SNAPPEA_EVAL_SOURCE=str(REPO),
               SNAPPEA_EVAL_P_IMAGE=IMAGE)
    env.pop('SNAPPEA_EVAL_PROMPT_VARIANT', None)
    r = subprocess.run([sys.executable, *map(str,args)], env=env, text=True, capture_output=True)
    if r.returncode:
        raise RuntimeError(r.stdout+r.stderr)
    print(r.stdout.strip(), flush=True)

def start():
    assert sha(BINARY) == 'b7ea15b06ee99225277adaeeee4dda33b2d2e6a6484b22ec3c13d9973d051fb2'
    out = ROOT/'runs'
    out.mkdir(exist_ok=False)
    refs = {str(p.relative_to(REPO)): sha(p) for base in [REPO/'docs',REPO/'examples/model']
            for p in sorted(base.rglob('*')) if p.is_file() and not p.is_symlink()}
    write(out/'reference-manifest.json', refs)
    rows = []
    for task in ['flow','vcs']:
        for rep in [1,2]:
            for label in ['a','b']:
                planning = (label == ('a' if (task == 'flow') == (rep == 1) else 'b'))
                run_id = f'prethought-{task}-{label}{rep:02d}'
                if task == 'flow':
                    call([PHASES/'runner.py','start','--run-id',run_id,'--phase','flow',
                          '--condition','baseline','--model','gpt-6-luna','--reasoning-effort','high',
                          '--binary',BINARY,'--tool','snappea'])
                    run = PHASES/'runs'/run_id
                    delegation = run/'prompt.txt'
                else:
                    call([VCS/'tools/collaboration_run.py','start','--run-id',run_id,'--arm','p',
                          '--condition','baseline','--model','gpt-6-luna','--reasoning-effort','high',
                          '--tool','snappea'])
                    run = VCS/'runs'/run_id
                    delegation = run/'delegation.txt'
                meta = json.loads((run/'metadata.json').read_text())
                original = delegation.read_text()
                (run/'original-delegation.txt').write_text(original)
                if planning:
                    updated = SENTENCE+'\n\n'+original
                    delegation.write_text(updated)
                    if task == 'vcs':
                        (run/'original-prompt.txt').write_text((run/'prompt.txt').read_text())
                        (run/'prompt.txt').write_text(SENTENCE+'\n\n'+(run/'prompt.txt').read_text())
                meta.update(prethought=planning, experiment='written-prethought-v1',
                            experiment_source_sha256=sha(Path(__file__)),
                            reference_manifest_sha256=sha(out/'reference-manifest.json'),
                            original_delegation_sha256=hashlib.sha256(original.encode()).hexdigest())
                meta['prompt_sha256'] = sha(run/'prompt.txt')
                if task == 'vcs':
                    meta['delegation_prompt_sha256'] = sha(delegation)
                write(run/'metadata.json',meta)
                rows.append(dict(run_id=run_id, task=task, repetition=rep,
                                 prethought=planning, run=str(run), prompt=str(delegation)))
                write(out/'map.json',rows)
    # Verify exact single-prefix delta and matched task/grader/tools/budgets.
    for row in rows:
        run = Path(row['run'])
        original = (run/'original-delegation.txt').read_text()
        assert Path(row['prompt']).read_text() == ((SENTENCE+'\n\n') if row['prethought'] else '')+original
    for task in ['flow','vcs']:
        cohort=[json.loads((Path(x['run'])/'metadata.json').read_text()) for x in rows if x['task']==task]
        keys=['public_sha256','limits','model','reasoning_effort','cli_binary_sha256','python_backend_sha256']
        if task == 'flow': keys += ['grader_sha256','harness_sha256','image']
        else: keys += ['archived_inputs_sha256','image','command_timeout_seconds']
        for key in keys:
            assert all(m[key] == cohort[0][key] for m in cohort), key
    print('All eight prompts and paired inputs verified.')

def finish(run_id):
    rows=json.loads((ROOT/'runs/map.json').read_text())
    row=next(r for r in rows if r['run_id']==run_id)
    if row['task']=='flow':
        call([PHASES/'runner.py','finish','--run-id',run_id])
        # A nonzero grader exit denotes a scored invalid submission, not harness failure.
        env=os.environ.copy()
        r=subprocess.run([sys.executable,str(PHASES/'grade.py'),'--run-id',run_id],env=env)
        report=Path(row['run'])/'grading/grade.json'
        if not report.is_file(): raise RuntimeError(f'grading failed: {r.returncode}')
    else:
        call([VCS/'tools/collaboration_run.py','finish','--run-id',run_id,'--seeds','64'])

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('action', choices=['start','finish'])
    p.add_argument('--run-id')
    a=p.parse_args()
    start() if a.action=='start' else finish(a.run_id)
