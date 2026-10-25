#!/usr/bin/env python3
"""Summarize frozen phase trials without pooling skill revisions."""
import argparse
import json
from pathlib import Path
from runner import ROOT,manifest


def summarize(ids):
    records=[]
    for name in ids:
        run=ROOT/'runs'/name
        meta=json.loads((run/'metadata.json').read_text())
        if meta['status']=='running':
            raise ValueError(f'{name} is not frozen')
        if manifest(run/'submission')!=json.loads((run/'submission-manifest.json').read_text()):
            raise ValueError(f'{name} submission changed')
        report=json.loads((run/'grading/grade.json').read_text())
        record={k:meta.get(k) for k in ['run_id','phase','condition','model','reasoning_effort',
                  'skill_sha256','binary_sha256','image','public_sha256','grader_sha256','harness_sha256',
                  'status','seconds','calls','public_unchanged']}
        if meta['phase']=='modeling':
            keys=['model_check_passed','selected_test_count','all_search_complete','schedules',
                  'valid_accepted','valid_total','invalid_rejected','invalid_total']
            record['result']={k:report[k] for k in keys}
        else:
            record['result']=report['summary']
        supplemental=run/'supplemental-v2/grade.json'
        if supplemental.exists():
            recheck=json.loads(supplemental.read_text())
            record['supplemental_v2']={'version':recheck.get('version'),
                                       'result':recheck['summary'],
                                       'provenance':json.loads((run/'supplemental-v2/provenance.json').read_text())}
        source_recheck=run/'supplemental-monitor-sources/grade.json'
        if source_recheck.exists():
            corrected=json.loads(source_recheck.read_text())
            record['supplemental_monitor_sources']={'version':corrected['version'],
                'result':corrected['summary'],'monitor_source_files':corrected['monitor_source_files'],
                'provenance':json.loads((source_recheck.parent/'provenance.json').read_text())}
        record['archived_grader']= (run/'private').is_dir()
        records.append(record)
    return {'trials':records,'limits':'Adaptive small development pilots. Keep revisions separate; no causal or population-level claims.'}


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('run_ids',nargs='+')
    p.add_argument('--output',type=Path)
    a=p.parse_args()
    data=summarize(a.run_ids)
    content=json.dumps(data,indent=2)+'\n'
    if a.output:
        a.output.write_text(content)
    else:
        print(content,end='')
