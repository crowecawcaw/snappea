#!/usr/bin/env python3
"""Budgeted shell bridge. Command text arrives on stdin; full output is retained."""
import argparse
import fcntl
import json
from pathlib import Path
import re
import sys
import time
import subprocess
from common import command, write_json, protected_input_drift

p = argparse.ArgumentParser()
p.add_argument('--run-id', required=True)
a = p.parse_args()
if not re.fullmatch('[a-z0-9-]+', a.run_id):
    raise SystemExit('invalid run ID')
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
    write_json(path, state)
    start = time.time()
    try:
        r = command(['docker', 'exec', 'snappea-driver-'+a.run_id, 'bash', '-c', cmd],
                    timeout=min(limits['command_seconds'], max(1, limits['seconds']-elapsed)))
        result = {'exit_code': r.returncode, 'stdout': r.stdout, 'stderr': r.stderr}
    except subprocess.TimeoutExpired:
        command(['docker', 'kill', 'snappea-driver-'+a.run_id])
        state['closed'] = True
        write_json(path, state)
        result = {'exit_code': 124, 'stdout': '', 'stderr': 'Command timed out; run aborted.'}
    with (run/'tool-transcript.jsonl').open('a') as out:
        out.write(json.dumps({'call': state['calls'], 'elapsed': elapsed, 'seconds': time.time()-start,
                              'command': cmd, 'result': result,
                              'protected_input_drift': protected_input_drift(run/'work', meta['public_sha256'])})+'\n')
    visible = dict(result)
    for field, limit in [('stdout', 18000), ('stderr', 8000)]:
        if len(visible[field]) > limit:
            value = visible[field]
            visible[field] = value[:limit//2]+'\n[truncated; full output retained]\n'+value[-limit//2:]
    print(json.dumps(visible))
