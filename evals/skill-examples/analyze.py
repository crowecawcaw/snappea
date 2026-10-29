#!/usr/bin/env python3
"""Verify, supplement and summarize frozen old/revised/no-skill trials."""
import argparse
import hashlib
import importlib.util
import json
import os
import subprocess
import time
from pathlib import Path
import sys

ROOT=Path(__file__).resolve().parent
REPO=ROOT.parents[1]
TOOLS=REPO/'evals/certified-bundles/tools'
sys.path.insert(0,str(TOOLS))
from skill_snapshot import verify_snapshot
sys.path.insert(0,str(ROOT))
from experiment import SENTENCE

def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def load(p):return json.loads(Path(p).read_text())
def write(p,d):Path(p).write_text(json.dumps(d,indent=2)+'\n')
def rows():return load(ROOT/'runs/map.json')
def module(name,file):
 s=importlib.util.spec_from_file_location(name,file);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m

def verify():
 cohort=rows();assert len(cohort)==12
 for task in ['flow','vcs']:
  selected=[r for r in cohort if r['task']==task];assert len(selected)==6
  normalized=[];metas=[]
  for row in selected:
   root=Path(row['run']);m=load(root/'metadata.json');metas.append(m)
   assert m['status']!='running'
   assert m['model']=='gpt-6-luna' and m['reasoning_effort']=='high'
   assert m['experiment_source_sha256']==sha(ROOT/'experiment.py')
   assert m['reference_manifest_sha256']==sha(ROOT/'runs/reference-manifest.json')
   assert m['experiment_condition']==row['condition']
   original=(root/'original-delegation.txt').read_text()
   assert Path(row['prompt']).read_text()==('' if row['condition']=='none' else SENTENCE+'\n\n')+original
   normalized.append(original.replace(row['run_id'],'RUN_ID'))
   assert sha(root/'prompt.txt')==m['prompt_sha256']
   if task=='vcs':assert sha(root/'delegation.txt')==m['delegation_prompt_sha256']
   if row['condition']=='none':assert not m['skill_snapshot'] and not m['skill_sha256']
   else:
    manifest=verify_snapshot(m['skill_snapshot'])
    assert manifest==m['skill_manifest'] and manifest['tree_sha256']==m['skill_sha256']
   for name,want in load(root/'submission-manifest.json').items():assert sha(root/'submission'/name)==want
   for name,want in m['public_sha256'].items():assert sha(root/'public'/name)==want
   if task=='flow':
    for key,folder in [('grader_sha256','private'),('harness_sha256','harness')]:
     for name,want in m[key].items():assert sha(root/folder/name)==want
   else:
    for name,want in m['archived_inputs_sha256'].items():assert sha(root/name)==want
   for path,key in [('cli_binary','cli_binary_sha256'),('python_backend_binary','python_backend_sha256')]:assert sha(m[path])==m[key]
   assert load(root/'bridge-state.json')['closed']
  assert len(set(normalized))==1
  for key in ['public_sha256','limits','image','cli_binary_sha256','python_backend_sha256']:
   assert all(m[key]==metas[0][key] for m in metas),key
  key='grader_sha256' if task=='flow' else 'archived_inputs_sha256'
  assert all(m[key]==metas[0][key] for m in metas)
 for name,want in load(ROOT/'runs/reference-manifest.json').items():assert sha(REPO/name)==want
 print('Verified12frozen trials, exact prompt delta, snapshots, inputs, tools and budgets.')

def supplement():
 verify()
 # Same source assembly correction and settings as the prior written-planning
 # cohort. Neither raw scores nor frozen solver sources are modified.
 m=module('model_source_correction',REPO/'evals/prethought/recheck_model_sources.py')
 m.ROOT=ROOT;m.main()
 results=[wide(r) for r in rows() if r['task']=='vcs']
 write(ROOT/'runs/wide-results.json',results)

def wide(row):
 root=Path(row['run']);m=load(root/'metadata.json');out=root/'supplemental-wide';out.mkdir(exist_ok=False)
 container='skill-wide-'+row['run_id']
 argv=['docker','run','--rm','--name',container,'--network','none','--cap-drop','ALL',
       '--security-opt','no-new-privileges','--user',f'{os.getuid()}:{os.getgid()}',
       '--memory','768m','--cpus','1','--pids-limit','128','--read-only','--tmpfs','/tmp:rw,size=128m']
 for src,dst,ro in [(root/'private','/grader/private',True),(root/'public','/grader/public',True),
                    (root/'submission','/submission',True),(out,'/output',False),
                    (ROOT/'bounded_wide.py','/checker/bounded_wide.py',True)]:
  argv+=['--mount',f'type=bind,src={src},dst={dst}'+(',readonly' if ro else '')]
 argv += [m['image'],'python','/checker/bounded_wide.py','/submission/solution.py',
          '--output','/output/result.json','--timeout','10']
 started=time.time()
 try:
  done=subprocess.run(argv,text=True,capture_output=True,timeout=30)
  execution={'argv':argv,'exit_code':done.returncode,'stdout':done.stdout,'stderr':done.stderr}
 except subprocess.TimeoutExpired:
  subprocess.run(['docker','rm','-f',container],capture_output=True)
  execution={'argv':argv,'exit_code':124,'stdout':'','stderr':'container execution deadline exceeded'}
 execution.update(started_unix=started,seconds=time.time()-started,wrapper_sha256=sha(ROOT/'bounded_wide.py'))
 write(out/'execution.json',execution)
 if not (out/'result.json').exists():
  write(out/'result.json',{'supplemental':True,'passed':False,'execution_status':'execution_failure',
                         'error':f'container exited {execution["exit_code"]} without a result'})
 return {'run_id':row['run_id'],**load(out/'result.json')}

def summarize():
 verify();records=[]
 for row in rows():
  root=Path(row['run']);m=load(root/'metadata.json');grade=root/'grading/grade.json';g=load(grade) if grade.exists() else None
  logs=[json.loads(l) for l in (root/'tool-transcript.jsonl').read_text().splitlines()]
  r={**row,'run_status':m['status'],'calls':m.get('calls',m.get('tool_calls')),
     'seconds':m['seconds'],'metadata_sha256':sha(root/'metadata.json'),
     'submission_manifest_sha256':sha(root/'submission-manifest.json'),
     'grade_sha256':sha(grade) if grade.exists() else None,'skill_sha256':m['skill_sha256'],
     'public_unchanged':m.get('public_unchanged',m.get('public_files_unchanged')),
     'references_requested':[{k:x[k] for k in ['call','command','elapsed']} for x in logs if '/opt/p-modeling' in x['command']],
     'checker_exit2_calls':[x['call'] for x in logs if x['result']['exit_code']==2 and 'snappea' in x['command']]}
  if row['task']=='flow':
   r['primary']=g['summary'];r['implementation_output']=g['implementation']['output']
   r['portable_probes']=g['portable_probes']
   source=root/'supplemental-model-source-v2/result.json';c=load(source);raw=c['result'];parsed=json.loads(raw['output'])
   cases=parsed.get('test_cases',[]);complete=bool(cases) and all(x.get('search_complete',False) for x in cases)
   r['supplemental_model']={'result_sha256':sha(source),'source_assembly_changed':c['source_assembly_changed'],
       'checker_exit_code':raw['exit_code'],'selected_tests':len(cases),'model_search_complete':complete,
       'model_check_passed':raw['exit_code']==0 and complete}
  else:
   if g is None:
    events=load(root/'original-grading-docker-events.json')
    assert any(e.get('Action')=='oom' for e in events),'missing grade without recorded cause'
    r['primary']={'passed':None,'total':20,'status':'grader_resource_failure',
        'reason':'candidate quiescent-write loop exhausted grader memory; no case score produced',
        'docker_events_sha256':sha(root/'original-grading-docker-events.json')}
   else:
    r['primary']={'passed':g['passed'],'total':g['total'],'status':'completed',
      'failed_cases':[c['case'] for c in g['cases'] if not c['passed']]}
   w=root/'supplemental-wide/result.json';r['supplemental_wide']={**load(w),'result_sha256':sha(w)}
  records.append(r)
 write(ROOT/'results.json',records)
 for r in records:print(r['run_id'],r['condition'],r['primary'])

if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('action',choices=['verify','supplement','summarize']);a=p.parse_args()
 {'verify':verify,'supplement':supplement,'summarize':summarize}[a.action]()
