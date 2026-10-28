#!/usr/bin/env python3
"""Verify the eight frozen pilot trials and exact within-task prompt ablation."""
import hashlib
import json
from pathlib import Path
from experiment import SENTENCE
from recheck_model_sources import corrected_events
ROOT=Path(__file__).resolve().parent

def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def load(p):return json.loads(p.read_text())

def main():
    rows=load(ROOT/'runs/map.json');assert len(rows)==8
    for task in ['flow','vcs']:
        cohort=[r for r in rows if r['task']==task];assert len(cohort)==4
        metas=[];normalized=[]
        for r in cohort:
            root=Path(r['run']);m=load(root/'metadata.json');metas.append(m)
            assert m['status']!='running'
            assert m['model']=='gpt-6-luna' and m['reasoning_effort']=='high'
            assert m['experiment_source_sha256']==sha(ROOT/'experiment.py')
            assert m['reference_manifest_sha256']==sha(ROOT/'runs/reference-manifest.json')
            assert sha(Path(r['prompt']))==m.get('delegation_prompt_sha256',m['prompt_sha256'])
            prompt=Path(r['prompt']).read_text();original=(root/'original-delegation.txt').read_text()
            assert prompt==((SENTENCE+'\n\n') if r['prethought'] else '')+original
            normalized.append(original.replace(r['run_id'],'RUN_ID'))
            for name,want in load(root/'submission-manifest.json').items():assert sha(root/'submission'/name)==want
            for name,want in m['public_sha256'].items():assert sha(root/'public'/name)==want
            if task=='flow':
                for key,folder in [('grader_sha256','private'),('harness_sha256','harness')]:
                    for name,want in m[key].items():assert sha(root/folder/name)==want
                corrected_events(root/'public/events.p',root/'submission/events.p')
            else:
                for name,want in m['archived_inputs_sha256'].items():assert sha(root/name)==want
            assert sha(Path(m['cli_binary']))==m['cli_binary_sha256']
            assert sha(Path(m['python_backend_binary']))==m['python_backend_sha256']
            assert load(root/'bridge-state.json')['closed']
        assert len(set(normalized))==1
        for key in ['public_sha256','limits','image','cli_binary_sha256','python_backend_sha256']:
            assert all(m[key]==metas[0][key] for m in metas),key
        key='grader_sha256' if task=='flow' else 'archived_inputs_sha256'
        assert all(m[key]==metas[0][key] for m in metas)
    repo=ROOT.parents[1]
    for name,want in load(ROOT/'runs/reference-manifest.json').items():assert sha(repo/name)==want
    for p in ROOT.glob('*hashes.json'):
        d=load(p)
        for name,want in d.get('files',d.get('sources',{})).items():
            if isinstance(want,str):assert sha(Path(name))==want
    print('Verified eight frozen trials, exact prompt delta, matching task inputs/tools/budgets, references and audit hashes.')

if __name__=='__main__':main()
