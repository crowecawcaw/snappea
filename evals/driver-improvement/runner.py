#!/usr/bin/env python3
"""Provision/freeze an isolated, transcript-preserving driver-improvement trial."""
import argparse
import fcntl
import json
from pathlib import Path
import re
import shutil
import time
from common import IMAGE, command, container_args, digest, manifest, write_json

ROOT = Path(__file__).resolve().parent
BINARY = ROOT.parents[1]/'target/driver-eval/release/snappea'
HARNESS = ['common.py', 'runner.py', 'bridge.py', 'grade.py', 'cli_wrapper.py']


def start(args):
    if not re.fullmatch('[a-z0-9-]+', args.run_id):
        raise ValueError('invalid run ID')
    fixture = ROOT/'fixtures'/args.fixture
    binary = args.binary.resolve()
    run = ROOT/'runs'/args.run_id
    run.mkdir(parents=True, exist_ok=False)
    for name in ['public', 'private']:
        shutil.copytree(fixture/name, run/name, ignore=shutil.ignore_patterns('__pycache__', 'PCheckerOutput'))
    shutil.copytree(run/'public', run/'work')
    (run/'harness').mkdir()
    for name in HARNESS:
        shutil.copy2(ROOT/name, run/'harness'/name)
    image = command(['docker', 'image', 'inspect', args.image, '--format', '{{.Id}}'])
    if image.returncode:
        raise RuntimeError(image.stderr)
    image_id = image.stdout.strip()
    prompt = f'''Review this model and improve its tests.
Read TASK.md and deliver the requested files.

Run ID: {args.run_id}. Work independently; do not spawn or contact other agents.
Use only this shell bridge for commands and file access. Do not inspect host files,
other runs, or bridge/harness sources. Commands run in an isolated /work container:

python {ROOT/'bridge.py'} --run-id {args.run_id} <<'SOLVER_COMMAND'
cat TASK.md
SOLVER_COMMAND

Use the installed snappea command. /opt/checker-native is experiment plumbing;
do not invoke it directly or change experiment configuration.
Budget: 480 seconds from first call, 40 calls, 30 seconds per command.
Return when finished; files in /work are your submission.
'''
    meta = {'run_id': args.run_id, 'fixture': args.fixture, 'reports': args.reports,
            'model': args.model, 'reasoning_effort': args.reasoning_effort, 'skill': None,
            'binary': str(binary), 'binary_sha256': digest(binary), 'image': image_id,
            'public_sha256': manifest(run/'public'), 'private_sha256': manifest(run/'private'),
            'harness_sha256': manifest(run/'harness'), 'prompt_sha256': __import__('hashlib').sha256(prompt.encode()).hexdigest(),
            'limits': {'seconds': 480, 'calls': 40, 'command_seconds': 30},
            'status': 'running', 'provisioned_unix': time.time()}
    (run/'prompt.txt').write_text(prompt)
    write_json(run/'metadata.json', meta)
    r = command(container_args(run/'work', binary, run/'harness/cli_wrapper.py', image_id,
                               args.reports, 'snappea-driver-'+args.run_id, detached=True)+['sleep', 'infinity'])
    if r.returncode:
        raise RuntimeError(r.stderr)
    preflight = command(['docker', 'exec', 'snappea-driver-'+args.run_id, 'snappea', 'version'])
    if preflight.returncode:
        command(['docker', 'rm', '-f', 'snappea-driver-'+args.run_id])
        raise RuntimeError(preflight.stdout+preflight.stderr)
    print(json.dumps({'run': args.run_id, 'prompt': str(run/'prompt.txt')}))


def finish(args):
    run = ROOT/'runs'/args.run_id
    meta = json.loads((run/'metadata.json').read_text())
    if digest(meta['binary']) != meta['binary_sha256']:
        raise ValueError('binary changed')
    with (run/'bridge.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        path = run/'bridge-state.json'
        state = json.loads(path.read_text()) if path.exists() else {'calls': 0, 'started': time.time()}
        state['closed'] = True
        write_json(path, state)
        command(['docker', 'rm', '-f', 'snappea-driver-'+args.run_id])
        shutil.copytree(run/'work', run/'submission', symlinks=True,
                        ignore=shutil.ignore_patterns('__pycache__', 'PCheckerOutput', '.pytest_cache'))
        meta.update(status=args.status, calls=state['calls'], seconds=time.time()-state['started'])
        write_json(run/'submission-manifest.json', manifest(run/'submission'))
        write_json(run/'metadata.json', meta)
    print(json.dumps({'run': args.run_id, 'status': args.status}))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('action', choices=['start', 'finish'])
    p.add_argument('--run-id', required=True)
    p.add_argument('--fixture', choices=['priority', 'retry', 'sessions'])
    p.add_argument('--reports', choices=['on', 'off'], default='on')
    p.add_argument('--binary', type=Path, default=BINARY)
    p.add_argument('--image', default=IMAGE)
    p.add_argument('--model', default='gpt-6.1-sol')
    p.add_argument('--reasoning-effort', default='medium')
    p.add_argument('--status', default='agent_finished')
    a = p.parse_args()
    if a.action == 'start':
        if not a.fixture:
            p.error('--fixture required')
        start(a)
    else:
        finish(a)
