#!/usr/bin/env python3
"""Evaluate a frozen model and its monitor separately; does not establish fidelity."""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import uuid
from runner import ROOT, BINARY, container_args, digest, manifest


def load(path, name):
    spec = importlib.util.spec_from_file_location(name,path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def evaluate(submission, public, output, image, binary=BINARY, cases_file=None):
    output.mkdir(parents=True,exist_ok=True)
    results = []
    with tempfile.TemporaryDirectory(prefix='snappea-model-grade-') as tmp:
        work = Path(tmp)
        # Only requested deliverables; public events override any submitted edits.
        for name in ['monitor.p','model.p','tests.p','DESIGN.md']:
            if (submission/name).is_file() and not (submission/name).is_symlink():
                shutil.copy2(submission/name,work/name)
        shutil.copy2(public/'events.p',work/'events.p')
        def run(args,timeout=90):
            try:
                name = 'snappea-model-grade-'+uuid.uuid4().hex[:12]
                base = container_args(work,image=image,binary=binary)
                base[2:2] = ['--name',name]
                r = subprocess.run(base+args,
                                   text=True,capture_output=True,timeout=timeout)
                return {'exit_code':r.returncode,'stdout':r.stdout,'stderr':r.stderr,'timeout':False}
            except subprocess.TimeoutExpired as e:
                return {'exit_code':124,'stdout':str(e.stdout or ''),'stderr':str(e.stderr or ''),'timeout':True}
            finally:
                subprocess.run(['docker','rm','-f',name],capture_output=True)
        model = run(['snappea','check','events.p','monitor.p','model.p','tests.p',
                     '--timeout','5s','--mutation-timeout','5s','--json'],timeout=75)
        (output/'model-check.json').write_text(json.dumps(model,indent=2)+'\n')
        try:
            model_json=json.loads(model['stdout'])
        except ValueError:
            model_json=None
        suite = load(cases_file or ROOT/'modeling/private/cases.py','hidden_model_cases')
        for name, valid, events in suite.cases():
            (work/'history.jsonl').write_text(''.join(json.dumps(e)+'\n' for e in events))
            result = run(['snappea','observe','events.p','monitor.p','history.jsonl',
                          '--spec','LeaseContract','--strict','--unknown-events','error','--json'],timeout=20)
            # Compiler/parse/infra failures never count as detecting invalid behavior.
            passed = not result['timeout'] and result['exit_code']==(0 if valid else 1)
            result.update(name=name,expected_valid=valid,passed=passed)
            results.append(result)
    selected = model_json.get('test_cases',[]) if isinstance(model_json,dict) else []
    report = {'model_check_passed':model['exit_code']==0 and bool(selected),
              'selected_test_count':len(selected),
              'all_search_complete':bool(selected) and all(t.get('search_complete',False) for t in selected),
              'schedules':sum(t.get('schedules',0) for t in selected), 'model_check':model_json,
              'valid_accepted':sum(r['passed'] for r in results if r['expected_valid']),
              'valid_total':sum(r['expected_valid'] for r in results),
              'invalid_rejected':sum(r['passed'] for r in results if not r['expected_valid']),
              'invalid_total':sum(not r['expected_valid'] for r in results),
              'cases':results,
              'limit':'Monitor trace checks do not establish model fidelity; source audit required.'}
    (output/'grade.json').write_text(json.dumps(report,indent=2)+'\n')
    return report


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--run-id',required=True)
    a=p.parse_args()
    run=ROOT/'runs'/a.run_id
    meta=json.loads((run/'metadata.json').read_text())
    assert meta['status']!='running', 'freeze first'
    assert manifest(run/'submission')==json.loads((run/'submission-manifest.json').read_text())
    assert digest(Path(meta['binary']))==meta['binary_sha256']
    private=run/'private' if (run/'private').exists() else ROOT/'modeling/private'
    assert manifest(private)==meta['grader_sha256'], 'grader changed since start'
    grade=evaluate(run/'submission',run/'public',run/'grading',meta['image'],Path(meta['binary']),
                   cases_file=private/'cases.py')
    print(json.dumps({k:v for k,v in grade.items() if k not in ('cases','model_check')}))
