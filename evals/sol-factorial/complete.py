#!/usr/bin/env python3
"""Preserve freeze/grader console and Docker lifecycle events, including failures."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import time
ROOT=Path(__file__).resolve().parent
p=argparse.ArgumentParser(description=__doc__);p.add_argument('run_id');args=p.parse_args()
row=next(r for r in json.loads((ROOT/'runs/map.json').read_text()) if r['run_id']==args.run_id)
run=Path(row['run']);started=time.time()
r=subprocess.run([sys.executable,str(ROOT/'experiment.py'),'finish','--run-id',args.run_id],text=True,capture_output=True)
finished=time.time();(run/'freeze-grader-console.log').write_text(r.stdout+r.stderr)
events=subprocess.run(['docker','events','--since',str(started),'--until',str(finished),'--format','{{json .}}'],text=True,capture_output=True)
(run/'grading-docker-events.json').write_text(json.dumps([json.loads(l) for l in events.stdout.splitlines()],indent=2)+'\n')
(run/'freeze-grader-execution.json').write_text(json.dumps({'exit_code':r.returncode,'started_unix':started,'finished_unix':finished,'event_capture_exit_code':events.returncode},indent=2)+'\n')
print(r.stdout+r.stderr,end='');sys.exit(r.returncode)
