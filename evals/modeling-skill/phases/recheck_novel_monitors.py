#!/usr/bin/env python3
"""Labeled posthoc correction for monitors using shared model declarations.

Preserves original model-check evidence; reruns only independent monitor histories.
Original grade.json and archived harness remain untouched.
"""
import argparse
import json
from pathlib import Path
import shutil
from unittest.mock import patch
import grade_novel
from runner import ROOT,manifest,digest


def recheck(run):
    meta=json.loads((run/'metadata.json').read_text())
    if meta['status']=='running' or not meta['phase'].endswith('/model'):
        raise ValueError('expected frozen novel model trial')
    for folder,expected in [('submission',json.loads((run/'submission-manifest.json').read_text())),
                            ('public',meta['public_sha256']),('private',meta['grader_sha256']),
                            ('harness',meta['harness_sha256'])]:
        if manifest(run/folder)!=expected:raise ValueError(f'changed {folder}')
    if digest(Path(meta['binary']))!=meta['binary_sha256']:raise ValueError('changed binary')
    original=json.loads((run/'grading/grade.json').read_text())
    actual_execute=grade_novel.execute
    def preserve_native(work,metadata,args,timeout=60):
        return original['native_check'] if args[1]=='check' else actual_execute(work,metadata,args,timeout)
    output=run/'supplemental-monitor-sources'
    output.mkdir(exist_ok=False)
    scenario=meta['phase'].split('/')[0]
    with patch.object(grade_novel,'execute',side_effect=preserve_native):
        report=grade_novel.model_grade(run,meta,grade_novel.CONFIG[scenario],output)
    (output/'grade.json').write_text(json.dumps(report,indent=2)+'\n')
    (output/'harness').mkdir()
    for name in ['grade_novel.py','recheck_novel_monitors.py','runner.py']:
        shutil.copy2(ROOT/name,output/'harness'/name)
    provenance={'posthoc':True,'native_check_preserved':True,
                'reason':'Allow shared P type/helper declarations; observe does not run model machines.',
                'original_grade_sha256':digest(run/'grading/grade.json'),
                'submission_manifest_sha256':digest(run/'submission-manifest.json'),
                'harness_sha256':manifest(output/'harness'),
                'public_sha256':meta['public_sha256'],'grader_sha256':meta['grader_sha256'],
                'image':meta['image'],'binary_sha256':meta['binary_sha256']}
    (output/'provenance.json').write_text(json.dumps(provenance,indent=2)+'\n')
    print(json.dumps({'run':meta['run_id'],**report['summary']}),flush=True)


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('run_ids',nargs='+')
    a=p.parse_args()
    for name in a.run_ids:recheck(ROOT/'runs'/name)
