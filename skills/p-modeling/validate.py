#!/usr/bin/env python3
"""Validate skill links and run positive/negative controls in temporary copies.

Usage: python skills/p-modeling/validate.py --binary /path/to/snappea
Requires the Python distribution pysnappea (import snappea).
"""
import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent

def validate(binary):
    for page in ROOT.rglob('*.md'):
        for link in re.findall(r'\]\(([^)]+)\)', page.read_text()):
            if '://' in link or link.startswith('#'):
                continue
            target = (page.parent / link.split('#')[0]).resolve()
            if not target.is_relative_to(ROOT) or not target.exists():
                raise RuntimeError(f'{page.relative_to(ROOT)}: broken or external local link: {link}')
    print('Local links: PASS')
    checks = [
        ('reservation', ['check', 'Atomic.pproj', '--timeout', '5s'], None),
        ('reservation', ['check', 'StaleWrite.pproj', '--timeout', '5s'], 'reservations exceed available capacity'),
        ('durable-retry', ['check', 'tcDurableRetry', 'tcCompletedReplay'], None),
        ('durable-retry', ['check', 'tcSplitCommitRetry'], 'one request caused more than one effect'),
        ('durable-retry', ['check', 'tcReappliedReplay'], 'one request caused more than one effect'),
        ('versioned-publication', ['check', 'Revalidated.p'], None),
        ('versioned-publication', ['check', 'StaleRetry.p'], 'was validated for version'),
    ]
    env = dict(os.environ, SNAPPEA_BIN=str(binary))
    with tempfile.TemporaryDirectory(prefix='snappea-skill-') as tmp:
        examples = Path(tmp) / 'examples'
        shutil.copytree(ROOT / 'examples', examples, ignore=shutil.ignore_patterns('PCheckerOutput', '__pycache__', '*.log'))
        for name, args, expected_error in checks:
            r = subprocess.run([str(binary), *args], cwd=examples/name, env=env,
                               capture_output=True, text=True, timeout=120)
            output = r.stdout + r.stderr
            expected_code = 1 if expected_error else 0
            if r.returncode != expected_code or (expected_error and expected_error not in output):
                raise RuntimeError(f'{name} {args}: expected {expected_code}/{expected_error}\n{output}')
            print(f'{name} {args}: PASS ({"expected counterexample" if expected_error else "correct model"})')
        r = subprocess.run([sys.executable, '-m', 'unittest', '-v'], cwd=examples/'reservation',
                           env=env, capture_output=True, text=True, timeout=120)
        if r.returncode:
            raise RuntimeError(r.stdout + r.stderr)
        print('Implementation observation: PASS (positive and negative controls)')

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', default=shutil.which('snappea'))
    args = parser.parse_args()
    if not args.binary:
        parser.error('set --binary or install snappea on PATH')
    validate(Path(args.binary).resolve())
