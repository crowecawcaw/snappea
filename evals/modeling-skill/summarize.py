#!/usr/bin/env python3
"""Summarize explicit frozen run IDs; verify inputs and never pool skill versions."""
import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / 'certified-bundles'

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def summarize(names):
    rows = []
    for name in names:
        run = ROOT / 'runs' / name
        meta = json.loads((run/'metadata.json').read_text())
        manifest = json.loads((run/'submission-manifest.json').read_text())
        assert all(sha(run/'submission'/path) == digest for path, digest in manifest.items()), name
        assert meta['public_files_unchanged'], name
        assert all(sha(run/'work'/path) == digest for path, digest in meta['public_sha256'].items()), name
        grade = json.loads((run/'grading/grade.json').read_text())
        wide_path = run/'grading/wide.json'
        wide = json.loads(wide_path.read_text()) if wide_path.exists() else None
        rows.append(dict(run=name, condition=meta['condition'], passed=grade['passed'], total=grade['total'],
                         all_passed=grade['all_passed'], wide_passed=wide['passed'] if wide else None,
                         model=meta['model'], reasoning_effort=meta['reasoning_effort'],
                         skill_sha256=meta.get('skill_snapshot',{}).get('tree_sha256'),
                         binary_sha256=meta.get('snappea_binary_sha256'), image=meta['image'],
                         public_sha256=meta['public_sha256'], manifest_entries=len(manifest),
                         manifest_verified=True, public_files_unchanged=True,
                         grader_version=grade['harness_version'], grader_source_sha256=grade['source_sha256'],
                         grader_seeds=grade['seeds']))
    assert len({r['binary_sha256'] for r in rows}) == 1
    assert len({r['image'] for r in rows}) == 1
    assert len({json.dumps(r['public_sha256'], sort_keys=True) for r in rows}) == 1
    assert len({r['skill_sha256'] for r in rows if r['condition']=='skill'}) <= 1, 'Do not pool skill versions'
    return rows

if __name__ == '__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('runs', nargs='+')
    p.add_argument('--output', type=Path, required=True)
    a=p.parse_args()
    a.output.write_text(json.dumps(summarize(a.runs),indent=2)+'\n')
