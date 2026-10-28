#!/usr/bin/env python3
"""Verify frozen runs and emit primary outcomes without conflating quality axes."""
import hashlib
import json
from pathlib import Path

ROOT=Path(__file__).resolve().parent

def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def load(p): return json.loads(p.read_text())

def main():
    records=[]
    for row in load(ROOT/'runs/map.json'):
        run=Path(row['run']); meta=load(run/'metadata.json')
        assert meta['status']!='running'
        assert meta['prethought']==row['prethought']
        assert sha(run/'prompt.txt')==meta['prompt_sha256']
        if row['task']=='vcs': assert sha(run/'delegation.txt')==meta['delegation_prompt_sha256']
        for name,want in load(run/'submission-manifest.json').items():
            assert sha(run/'submission'/name)==want, name
        g=load(run/'grading/grade.json')
        record={**row, 'metadata_sha256':sha(run/'metadata.json'),
                'submission_manifest_sha256':sha(run/'submission-manifest.json'),
                'grade_sha256':sha(run/'grading/grade.json'),
                'calls':meta.get('calls',meta.get('tool_calls')),'seconds':meta['seconds'],
                'run_status':meta['status'],
                'public_unchanged':meta.get('public_unchanged',meta.get('public_files_unchanged'))}
        if row['task']=='flow':
            record['primary']=g['summary']
            record['implementation_status']=g['implementation']['status']
            record['implementation_output']=g['implementation']['output']
            record['model_status']=g['model']['status']
            record['portable_probes']=g['portable_probes']
            supplemental=run/'supplemental-model-source-v2/result.json'
            correction=load(supplemental)
            raw=correction['result']
            parsed=json.loads(raw['output'])
            cases=parsed.get('test_cases',[])
            complete=bool(cases) and all(c.get('search_complete',False) for c in cases)
            # run_container's status is a unittest classifier. Model verdicts,
            # like the archived run_model, use checker JSON and exit status.
            record['supplemental_model']={
                'result_sha256':sha(supplemental),
                'source_assembly_changed':correction['source_assembly_changed'],
                'model_check_passed':raw['exit_code']==0 and bool(cases) and complete,
                'model_search_complete':complete,'selected_tests':len(cases),
                'checker_exit_code':raw['exit_code']}
        else:
            record['primary']={'passed':g['passed'],'total':g['total'],
                'failed_cases':[c['case'] for c in g['cases'] if not c['passed']]}
            wide=run/'supplemental-wide/result.json'
            record['supplemental_wide']={**load(wide),'result_sha256':sha(wide)}
        records.append(record)
    (ROOT/'results.json').write_text(json.dumps(records,indent=2)+'\n')
    for r in records:
        print(r['run_id'],'planning' if r['prethought'] else 'control',r['primary'])

if __name__=='__main__': main()
