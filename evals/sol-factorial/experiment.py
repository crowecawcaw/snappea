#!/usr/bin/env python3
"""Missing Sol factorial cells using the existing isolated task harnesses."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT=Path(__file__).resolve().parent
REPO=ROOT.parents[1]
PHASES=REPO/'evals/modeling-skill/phases'
VCS=REPO/'evals/certified-bundles'
BINARY=REPO/'target/driver-eval/release/snappea'
IMAGE='sha256:00dbf3678a1e6fc660c1cc87fe05f9a8d2cdaeda2daa73fedd9941bb55ce5431'
SENTENCE='Use the P modeling skill at /opt/p-modeling/SKILL.md.'
ENV=dict(os.environ,SNAPPEA_EVAL_BINARY=str(BINARY),SNAPPEA_EVAL_SOURCE=str(REPO),SNAPPEA_EVAL_P_IMAGE=IMAGE)
ENV.pop('SNAPPEA_EVAL_PROMPT_VARIANT',None)

def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def load(p):return json.loads(Path(p).read_text())
def write(p,d):Path(p).write_text(json.dumps(d,indent=2)+'\n')
def command(args,env=None):
 r=subprocess.run(list(map(str,args)),env=env,text=True,capture_output=True)
 if r.returncode:raise RuntimeError(r.stdout+r.stderr)
 return r

def start():
 assert sha(BINARY)=='b7ea15b06ee99225277adaeeee4dda33b2d2e6a6484b22ec3c13d9973d051fb2'
 assert sha(BINARY.with_name('snappea-compat'))=='f969f66ae7dc03a39823113fff33c0a94e4da61918253be173070d73024f8092'
 out=ROOT/'runs';snapshots=load(out/'snapshots.json')
 assert not (out/'map.json').exists()
 sys.path.insert(0,str(PHASES));import runner as phase
 sys.path.insert(0,str(VCS/'tools'));import run_sonnet as vcs
 from skill_snapshot import verify_snapshot
 vcs.P_IMAGE=IMAGE
 # The VCS container helper selects source paths dynamically from these variables.
 os.environ.update({k:ENV[k] for k in ['SNAPPEA_EVAL_BINARY','SNAPPEA_EVAL_SOURCE']})
 refs={str(p.relative_to(REPO)):sha(p) for base in [REPO/'docs',REPO/'examples/model']
       for p in sorted(base.rglob('*')) if p.is_file() and not p.is_symlink()}
 write(out/'reference-manifest.json',refs)
 assignments=[('flow','compat-skill',1),('vcs','native-no-skill',1),('flow','native-no-skill',1),('vcs','compat-skill',1),('flow','native-no-skill',2),('vcs','compat-skill',2),('flow','compat-skill',2),('vcs','native-no-skill',2)]
 rows=[]
 for task,condition,rep in assignments:
    family='sol'
    rid=f'factorial-sol-{task}-{condition}-{rep:02d}'
    model='gpt-6.1-sol'
    effort='medium'
    tool='snappea-compat' if condition=='compat-skill' else 'snappea'
    if task=='flow':
     args=[PHASES/'runner.py','start','--run-id',rid,'--phase','flow',
           '--condition','baseline','--model',model,'--reasoning-effort',effort,
           '--binary',BINARY,'--tool',tool]
     root=PHASES/'runs'/rid;delegation=root/'prompt.txt';container='snappea-phase-'+rid
    else:
     args=[VCS/'tools/collaboration_run.py','start','--run-id',rid,'--arm','p',
           '--condition','baseline','--model',model,'--reasoning-effort',effort,'--tool',tool]
     root=VCS/'runs'/rid;delegation=root/'delegation.txt';container='snappea-eval-'+rid
    print(command([sys.executable,*args],ENV).stdout.strip(),flush=True)
    meta=load(root/'metadata.json');original=delegation.read_text()
    (root/'original-delegation.txt').write_text(original)
    write(root/'original-metadata.json',meta)
    snapshot=None;skill=None
    if condition=='compat-skill':
     snapshot=Path(snapshots['skill']);skill=verify_snapshot(snapshot)
     # No solver has used the bridge: reprovision identical container plus the
     # read-only skill mount, then freeze the actual prompt/metadata before launch.
     assert not (root/'bridge-state.json').exists()
     command(['docker','rm','-f',container])
     if task=='flow':
      docker=phase.container_args((root/'work').resolve(),image=meta['image'],
        binary=BINARY,name=container,tool=tool,skill=snapshot)+['sleep','infinity']
     else:
      docker=vcs.container_args(container,root/'work','p','skill',snapshot,tool,BINARY.with_name(tool))
     command(docker)
     command(['docker','exec',container,'bash','-c',f'test -r /opt/p-modeling/SKILL.md && {tool} version && ! command -v snappea'])
     delegation.write_text(SENTENCE+'\n\n'+original)
     if task=='vcs':
      (root/'original-prompt.txt').write_text((root/'prompt.txt').read_text())
      (root/'prompt.txt').write_text(SENTENCE+'\n\n'+(root/'prompt.txt').read_text())
    meta.update(condition='skill' if condition=='compat-skill' else 'baseline',
      experiment='sol-factorial-v1',execution_block='missing-cells',model_family=family,experiment_condition=condition,
      experiment_source_sha256=sha(Path(__file__)),
      reference_manifest_sha256=sha(out/'reference-manifest.json'),
      skill_sha256=skill['tree_sha256'] if skill else None,
      skill_snapshot=str(snapshot) if snapshot else None,
      skill_manifest=skill,
      original_delegation_sha256=hashlib.sha256(original.encode()).hexdigest())
    meta['prompt_sha256']=sha(root/'prompt.txt')
    if task=='vcs':meta['delegation_prompt_sha256']=sha(delegation)
    mounted=command(['docker','exec',container,'sha256sum','/opt/'+tool,'/opt/python-snappea']).stdout.splitlines()
    assert mounted[0].split()[0]==meta['cli_binary_sha256']
    assert mounted[1].split()[0]==meta['python_backend_sha256']
    meta['mounted_tool_hashes_preflight']=mounted
    write(root/'metadata.json',meta)
    rows.append(dict(run_id=rid,task=task,repetition=rep,condition=condition,
                     run=str(root),prompt=str(delegation),model_family=family,tool=tool))
    write(out/'map.json',rows)
 for task in ['flow','vcs']:
  cohort=[r for r in rows if r['task']==task];metas=[load(Path(r['run'])/'metadata.json') for r in cohort]
  normalized=[(Path(r['run'])/'original-delegation.txt').read_text().replace(r['run_id'],'RUN_ID').replace('snappea-compat','snappea') for r in cohort]
  assert len(set(normalized))==1
  for key in ['limits','image','python_backend_sha256']:
   assert all(m[key]==metas[0][key] for m in metas),key
  if task=='flow':
   native=(PHASES/'flow/public/TASK.md').read_text()
   for r in cohort:
    text=(Path(r['run'])/'public/TASK.md').read_text()
    want=native.replace('snappea check events.p monitor.p model.p tests.p','snappea-compat check --pfiles events.p monitor.p model.p tests.p') if r['tool']=='snappea-compat' else native
    assert text==want
    assert 'Use '+r['tool']+' for P model checks' in Path(r['prompt']).read_text()
 verify_parity(rows)
 print('Eight prompts frozen; parity with prior eight Sol trials verified.',flush=True)

def verify_parity(new_rows):
 prior=[r for r in load(REPO/'evals/single-file-examples/runs/map.json') if r['model_family']=='sol']
 assert len(prior)==8 and len(new_rows)==8
 write(ROOT/'runs/prior-map.json',prior)
 for task in ['flow','vcs']:
  selected=[r for r in prior+new_rows if r['task']==task]
  metas=[load(Path(r['run'])/'metadata.json') for r in selected]
  normalized=[]
  for row,meta in zip(selected,metas):
   assert meta['model']=='gpt-6.1-sol' and meta['reasoning_effort']=='medium'
   original=(Path(row['run'])/'original-delegation.txt').read_text()
   normalized.append(original.replace(row['run_id'],'RUN_ID').replace('snappea-compat','snappea'))
  assert len(set(normalized))==1
  for key in ['limits','image','python_backend_sha256']:
   assert all(m[key]==metas[0][key] for m in metas),key
  key='grader_sha256' if task=='flow' else 'archived_inputs_sha256'
  assert all(m[key]==metas[0][key] for m in metas),key
  for tool in ['snappea','snappea-compat']:
   hashes={m['cli_binary_sha256'] for m in metas if m['tool']==tool}
   assert len(hashes)==1
 prior_refs=load(REPO/'evals/single-file-examples/runs/reference-manifest.json')
 assert load(ROOT/'runs/reference-manifest.json')==prior_refs


def finish(rid):
 row=next(r for r in load(ROOT/'runs/map.json') if r['run_id']==rid);root=Path(row['run'])
 state=load(root/'bridge-state.json');logs=[json.loads(l) for l in (root/'tool-transcript.jsonl').read_text().splitlines()]
 status='command_timeout' if state.get('closed') and logs[-1]['result']['exit_code']==124 else 'agent_finished'
 if row['task']=='flow':
  print(command([sys.executable,PHASES/'runner.py','finish','--run-id',rid,'--status',status],ENV).stdout,flush=True)
  r=subprocess.run([sys.executable,str(PHASES/'grade.py'),'--run-id',rid],env=ENV,text=True,capture_output=True)
  (root/'grader-console.log').write_text(r.stdout+r.stderr)
  if not (root/'grading/grade.json').is_file():raise RuntimeError(r.stdout+r.stderr)
 else:
  print(command([sys.executable,VCS/'tools/collaboration_run.py','finish','--run-id',rid,'--status',status,'--seeds','64'],ENV).stdout,flush=True)

if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('action',choices=['start','finish']);p.add_argument('--run-id');a=p.parse_args()
 start() if a.action=='start' else finish(a.run_id)
