#!/usr/bin/env python3
"""Logged, budgeted shell bridge for phased collaboration trials."""
import argparse
import fcntl
import json
from pathlib import Path
import re
import subprocess
import sys
import time

p = argparse.ArgumentParser()
p.add_argument('--run-id', required=True)
a = p.parse_args()
if not re.fullmatch('[a-z0-9-]+', a.run_id):
    raise SystemExit('invalid run id')
run = Path(__file__).resolve().parent/'runs'/a.run_id
cmd = sys.stdin.read()
with (run/'bridge.lock').open('a') as lock:
    fcntl.flock(lock, fcntl.LOCK_EX)
    meta = json.loads((run/'metadata.json').read_text())
    limits = meta['limits']
    path = run/'bridge-state.json'
    state = json.loads(path.read_text()) if path.exists() else {'started': time.time(), 'calls': 0}
    elapsed = time.time()-state['started']
    if state.get('closed') or elapsed >= limits['seconds'] or state['calls'] >= limits['calls']:
        raise SystemExit('Submission frozen or budget exhausted. Stop and return your final response.')
    state['calls'] += 1
    path.write_text(json.dumps(state))
    start = time.time()
    try:
        r = subprocess.run(['docker', 'exec', 'snappea-phase-'+a.run_id, 'bash', '-c', cmd],
                           text=True, capture_output=True,
                           timeout=min(limits['command_seconds'], max(1, limits['seconds']-elapsed)))
        result = {'exit_code': r.returncode, 'stdout': r.stdout, 'stderr': r.stderr}
    except subprocess.TimeoutExpired:
        subprocess.run(['docker', 'kill', 'snappea-phase-'+a.run_id], capture_output=True)
        state['closed'] = True
        path.write_text(json.dumps(state))
        result = {'exit_code': 124, 'stdout': '', 'stderr': 'Command timed out; run aborted.'}
    with (run/'tool-transcript.jsonl').open('a') as out:
        out.write(json.dumps({'call': state['calls'], 'elapsed': elapsed, 'seconds': time.time()-start,
                              'command': cmd, 'result': result})+'\n')
    for field, limit in [('stdout', 18000), ('stderr', 8000)]:
        if len(result[field]) > limit:
            value = result[field]
            result[field] = value[:limit//2]+'\n[truncated; full output retained]\n'+value[-limit//2:]
    print(json.dumps(result))
