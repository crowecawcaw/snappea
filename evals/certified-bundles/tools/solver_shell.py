#!/usr/bin/env python3
"""Logged, budgeted shell bridge for collaboration subagents. Commands arrive on stdin."""
import argparse
import fcntl
import json
from pathlib import Path
import re
import sys
import time
import run_sonnet as runner
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser()
p.add_argument('--run-id',required=True)
a=p.parse_args()
if not re.fullmatch(r'[a-z0-9-]+',a.run_id): raise SystemExit('invalid run ID')
run=ROOT/'runs'/a.run_id
cmd=sys.stdin.read()
with (run/'bridge.lock').open('a') as lock:
    fcntl.flock(lock,fcntl.LOCK_EX)
    state_path=run/'bridge-state.json'
    state=json.loads(state_path.read_text()) if state_path.exists() else {'started':time.time(),'calls':0,'closed':False}
    elapsed=time.time()-state['started']
    if state['closed'] or elapsed>=900 or state['calls']>=80:
        raise SystemExit('Solver budget exhausted or submission frozen. Stop and return your final response.')
    state['calls']+=1
    state_path.write_text(json.dumps(state,indent=2))
    start=time.time()
    try:
        metadata=json.loads((run/'metadata.json').read_text())
        per_command=metadata.get('command_timeout_seconds',40)
        result=runner.shell('snappea-eval-'+a.run_id,cmd,timeout=min(per_command,max(1,int(900-elapsed))))
    except Exception as exc:
        result={'exit_code':124,'stdout':'','stderr':f'{type(exc).__name__}: {exc}'}
        state['closed']=True
        state_path.write_text(json.dumps(state,indent=2))
    record={'call':state['calls'],'elapsed':round(elapsed,3),'seconds':round(time.time()-start,3),'command':cmd,'result':result}
    with (run/'tool-transcript.jsonl').open('a') as out: out.write(json.dumps(record)+'\n')
    visible=dict(result)
    for field,limit in [('stdout',16000),('stderr',8000)]:
        raw=visible[field]
        visible[field+'_total_bytes']=len(raw.encode())
        if len(raw)>limit:
            visible[field]=raw[:limit//2]+'\n[output truncated; full stream retained in experiment log]\n'+raw[-limit//2:]
            visible[field+'_truncated']=True
    print(json.dumps(visible))
