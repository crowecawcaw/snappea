#!/usr/bin/env python3
"""Run the pre-existing wide-bundle probe on every frozen VCS submission."""
import argparse
import json
import os
from pathlib import Path
import subprocess
ROOT=Path(__file__).resolve().parent


def run(row):
    root=Path(row['run']);meta=json.loads((root/'metadata.json').read_text())
    assert meta['status']!='running'
    out=root/'supplemental-wide';out.mkdir(exist_ok=False)
    args=['docker','run','--rm','--network','none','--cap-drop','ALL',
        '--security-opt','no-new-privileges','--user',f'{os.getuid()}:{os.getgid()}',
        '--memory','768m','--cpus','1','--pids-limit','128','--read-only',
        '--tmpfs','/tmp:rw,size=128m']
    for src,dst,ro in [(root/'private','/grader/private',True),
                       (root/'public','/grader/public',True),
                       (root/'submission','/submission',True),(out,'/output',False)]:
        args+=['--mount',f'type=bind,src={src},dst={dst}'+(',readonly' if ro else '')]
    args += [meta['image'],'python','/grader/private/stress_wide.py',
             '/submission/solution.py','--output','/output/result.json']
    r=subprocess.run(args,text=True,capture_output=True,timeout=120)
    (out/'execution.json').write_text(json.dumps({'argv':args,'exit_code':r.returncode,
        'stdout':r.stdout,'stderr':r.stderr},indent=2)+'\n')
    assert (out/'result.json').is_file(),r.stderr
    return {'run_id':row['run_id'],**json.loads((out/'result.json').read_text())}

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--run-id');a=p.parse_args()
    results=[]
    for row in json.loads((ROOT/'runs/map.json').read_text()):
        if row['task']=='vcs' and (not a.run_id or row['run_id']==a.run_id): results.append(run(row))
    print(json.dumps(results,indent=2))
