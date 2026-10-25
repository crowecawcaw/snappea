#!/usr/bin/env python3
"""Grade a frozen phase trial only against its original public inputs/grader."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
from runner import ROOT, digest, manifest


def verify(run):
    meta=json.loads((run/'metadata.json').read_text())
    if meta['status']=='running':
        raise ValueError('freeze submission first')
    if manifest(run/'submission')!=json.loads((run/'submission-manifest.json').read_text()):
        raise ValueError('frozen submission changed')
    if digest(Path(meta['binary']))!=meta['binary_sha256']:
        raise ValueError('binary changed')
    if meta.get('harness_sha256'):
        for name,want in meta['harness_sha256'].items():
            if digest(ROOT/name)!=want:
                raise ValueError('grading harness changed since provisioning')
    phase=ROOT/meta['phase']
    grader=run/'private' if (run/'private').exists() else phase/'private'
    if manifest(grader)!=meta['grader_sha256']:
        raise ValueError('grader changed since provisioning; use original revision, or label regrading supplemental')
    if not (run/'private').exists() and manifest(phase/'public')!=meta['public_sha256']:
        raise ValueError('public task changed since provisioning; use original revision')
    if manifest(run/'public')!=meta['public_sha256']:
        raise ValueError('frozen public inputs changed')
    return meta


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--run-id',required=True)
    a=p.parse_args()
    run=ROOT/'runs'/a.run_id
    meta=verify(run)
    if '/' in meta['phase']:
        args=[sys.executable,str(ROOT/'grade_novel.py'),'--run-id',a.run_id]
    elif meta['phase']=='modeling':
        args=[sys.executable,str(ROOT/'grade_model.py'),'--run-id',a.run_id]
    else:
        grader=(run/'private/grade.py') if (run/'private').exists() else ROOT/meta['phase']/'private/grade.py'
        args=[sys.executable,str(grader),
              '--submission',str(run/'submission'),'--output',str(run/'grading/grade.json'),
              '--image',meta['image'],'--binary',meta['binary']]
    raise SystemExit(subprocess.run(args).returncode)
