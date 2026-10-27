#!/usr/local/bin/python
"""Matched checker execution; only display of driver facts differs between arms."""
import json
import os
from pathlib import Path
import subprocess
import sys

requested = sys.argv[1:]
args = list(requested)
sidecars = []
is_check = 'check' in args and not any(a in ['--help', '-h'] for a in args)
if is_check:
    # Experiment settings remain fixed. Collect facts in BOTH arms; this is a
    # display ablation, not an instrumentation/search-throughput comparison.
    clean = []
    i = 0
    while i < len(args):
        a = args[i]
        if a in ['--timeout', '--mutation-timeout', '--format']:
            i += 2
            continue
        if a == '--json-report' and i+1 < len(args):
            sidecars.append(args[i+1])
            i += 2
            continue
        if a.startswith('--json-report='):
            sidecars.append(a.split('=', 1)[1])
            i += 1
            continue
        if a.startswith(('--timeout=', '--mutation-timeout=', '--format=')) or a in ['--no-driver-facts', '--no-mutation', '--allow-incomplete-coverage', '--json']:
            i += 1
            continue
        clean.append(a)
        i += 1
    args = clean+['--no-mutation', '--allow-incomplete-coverage', '--timeout', '2s', '--json']

result = subprocess.run(['/opt/checker-native', *args], capture_output=True, text=True, errors='replace')
output = result.stdout
parsed = None
if is_check:
    try:
        parsed = json.loads(output)
    except ValueError:
        pass
    if isinstance(parsed, dict):
        if os.environ.get('DRIVER_REPORTS') == 'off':
            for case in parsed.get('test_cases', []):
                case.pop('driver', None)
        # Serialize BOTH arms identically; do not let formatting reveal the arm.
        output = json.dumps(parsed, indent=2)+'\n'
        for filename in sidecars:
            try:
                Path(filename).write_text(output)
            except OSError as exc:
                sys.stderr.write(f'could not write JSON report {filename}: {exc}\n')
                sys.exit(2)
try:
    with open('/work/.check-invocations.jsonl', 'a') as out:
        out.write(json.dumps({'requested': requested, 'effective': args,
                              'reports': os.environ.get('DRIVER_REPORTS', 'on'),
                              'facts_collected_in_both_arms': True,
                              'display_filtered_only': os.environ.get('DRIVER_REPORTS') == 'off'})+'\n')
except OSError:
    pass
sys.stdout.write(output)
sys.stderr.write(result.stderr)
sys.exit(result.returncode)
