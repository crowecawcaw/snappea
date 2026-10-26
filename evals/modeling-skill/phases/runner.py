#!/usr/bin/env python3
"""Frozen, isolated phased skill trials. Solvers use bridge.py, never this file."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent
TOOLS = ROOT.parents[1] / 'certified-bundles' / 'tools'
sys.path.insert(0, str(TOOLS))
from skill_snapshot import create_snapshot, verify_snapshot

IMAGE = 'snappea-eval:py312-pytest842-pysnappea-source-0.1.0'
BINARY = ROOT.parents[2] / 'target/release/snappea'
SOURCE = ROOT.parents[2]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def manifest(root):
    return {str(p.relative_to(root)): digest(p) for p in sorted(root.rglob('*'))
            if p.is_file() and not p.is_symlink() and '__pycache__' not in p.parts
            and 'PCheckerOutput' not in p.parts}


def command(args, **kwargs):
    return subprocess.run(args, text=True, capture_output=True, **kwargs)


def container_args(work, image=IMAGE, binary=BINARY, name=None, skill=None):
    args = ['docker', 'run', '--network', 'none', '--cap-drop', 'ALL',
            '--security-opt', 'no-new-privileges', '--user', f'{os.getuid()}:{os.getgid()}',
            '--memory', '768m', '--cpus', '1', '--pids-limit', '128', '--read-only',
            '--tmpfs', '/tmp:rw,exec,size=128m', '--mount', f'type=bind,src={work},dst=/work',
            '--mount', f'type=bind,src={binary},dst=/opt/snappea,readonly',
            '--env', 'PATH=/opt:/usr/local/bin:/usr/local/sbin:/usr/sbin:/usr/bin:/sbin:/bin',
            '--env', 'SNAPPEA_BIN=/opt/snappea', '-w', '/work']
    if name:
        args += ['-d', '--name', name]
    else:
        args += ['--rm']
    if skill:
        args += ['--mount', f'type=bind,src={skill},dst=/opt/p-modeling,readonly']
    return args + [image]


def start(args):
    if not re.fullmatch('[a-z0-9-]+', args.run_id):
        raise ValueError('invalid run id')
    binary = args.binary.resolve()
    public = ROOT / args.phase / 'public'
    run = ROOT / 'runs' / args.run_id
    run.mkdir(parents=True, exist_ok=False)
    shutil.copytree(public, run/'work')
    shutil.copytree(public, run/'public')
    if args.condition=='skill':
        snapshot, skill = create_snapshot(SOURCE/'skills/p-modeling')
    else:
        snapshot, skill = None, None
    image = command(['docker', 'image', 'inspect', IMAGE, '--format', '{{.Id}}'])
    if image.returncode:
        raise RuntimeError(image.stderr)
    image_id = image.stdout.strip()
    private = ROOT/args.phase/'private'
    shutil.copytree(private,run/'private',ignore=shutil.ignore_patterns('__pycache__'))
    harness=['runner.py','bridge.py','grade.py','grade_novel.py']
    (run/'harness').mkdir()
    for name in harness:shutil.copy2(ROOT/name,run/'harness'/name)
    prompt = ('Read TASK.md.' + (' Use the P modeling skill at /opt/p-modeling/SKILL.md.'
              if args.condition=='skill' else '') + '\nComplete the requested deliverables.\n')
    bridge = ROOT/'bridge.py'
    seconds, calls = (900,80) if args.phase=='flow' else (600,50)
    delegation = prompt + f'''
Run ID: {args.run_id}. Work independently; do not spawn/contact agents.
Use only this shell bridge for file access and commands. Do not inspect host files,
other runs, or bridge/harness sources. Commands run in an isolated /work container:

python {bridge} --run-id {args.run_id} <<'SOLVER_COMMAND'
cat TASK.md
SOLVER_COMMAND

Budget: {seconds} seconds from first call, {calls} calls, 90 seconds per command.
Return when finished; /work files are your submission.
'''
    meta = {'run_id': args.run_id, 'phase': args.phase, 'condition': args.condition,
            'model': 'gpt-6-luna', 'reasoning_effort': 'high',
            'image': image_id, 'binary_sha256': digest(binary), 'binary': str(binary),
            'skill_sha256': skill['tree_sha256'] if skill else None, 'skill_snapshot': str(snapshot) if snapshot else None,
            'public_sha256': manifest(public), 'grader_sha256': manifest(private),
            'prompt_sha256': hashlib.sha256(delegation.encode()).hexdigest(),
            'harness_sha256': manifest(run/'harness'),
            'limits': {'seconds': seconds, 'calls': calls, 'command_seconds': 90},
            'status': 'running', 'provisioned_unix': time.time()}
    (run/'prompt.txt').write_text(delegation)
    (run/'metadata.json').write_text(json.dumps(meta, indent=2)+'\n')
    result = command(container_args((run/'work').resolve(), image=image_id,
                                   name='snappea-phase-'+args.run_id, skill=snapshot, binary=binary) + ['sleep', 'infinity'])
    if result.returncode:
        raise RuntimeError(result.stderr)
    check = command(['docker', 'exec', 'snappea-phase-'+args.run_id, 'bash', '-c',
                     'snappea version && python -c "import snappea"'+(' && test -r /opt/p-modeling/SKILL.md' if skill else '')])
    if check.returncode:
        command(['docker', 'rm', '-f', 'snappea-phase-'+args.run_id])
        raise RuntimeError(check.stdout+check.stderr)
    print(json.dumps({'run': args.run_id, 'prompt': str(run/'prompt.txt'), 'skill': skill['tree_sha256'] if skill else None}))


def finish(args):
    import fcntl
    run = ROOT/'runs'/args.run_id
    with (run/'bridge.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        state_path = run/'bridge-state.json'
        state = json.loads(state_path.read_text()) if state_path.exists() else {'calls': 0, 'started': time.time()}
        state['closed'] = True
        state_path.write_text(json.dumps(state))
        command(['docker', 'rm', '-f', 'snappea-phase-'+args.run_id])
        meta = json.loads((run/'metadata.json').read_text())
        if (run/'submission').exists():
            raise ValueError('already frozen')
        shutil.copytree(run/'work', run/'submission', symlinks=True,
                        ignore=shutil.ignore_patterns('__pycache__', 'PCheckerOutput', '.pytest_cache'))
        meta.update(status=args.status, calls=state['calls'], seconds=time.time()-state['started'])
        meta['public_unchanged'] = all((run/'work'/n).is_file() and digest(run/'work'/n)==h
                                       for n,h in meta['public_sha256'].items())
        (run/'submission-manifest.json').write_text(json.dumps(manifest(run/'submission'), indent=2)+'\n')
        (run/'metadata.json').write_text(json.dumps(meta, indent=2)+'\n')
    print(json.dumps({'run': args.run_id, 'status': args.status, 'public_unchanged': meta['public_unchanged']}))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('action', choices=['start', 'finish'])
    p.add_argument('--run-id', required=True)
    p.add_argument('--phase', choices=['modeling', 'testing', 'flow', 'migration/model', 'migration/testing', 'streaming/model', 'streaming/testing', 'priority/testing'])
    p.add_argument('--binary', type=Path, default=BINARY, help='checker binary to freeze for this trial')
    p.add_argument('--condition',choices=['skill','baseline'],default='skill')
    p.add_argument('--status', default='agent_finished')
    a = p.parse_args()
    if a.action == 'start':
        if not a.phase:
            p.error('--phase required for start')
        start(a)
    else:
        finish(a)
