#!/usr/bin/env python3
"""Provision/freeze/grade a collaboration-agent trial without sharing hidden tests."""
import argparse
import fcntl
import hashlib
import json
from pathlib import Path
import time
import shutil
import run_sonnet as r
from delegation import prompt as delegation_prompt
from prompts import VERSION as PROMPT_VERSION, prompt_variant
from skill_snapshot import verify_snapshot
import tool_variant
ROOT=Path(__file__).resolve().parents[1]

def start(run_id,arm,condition='legacy',skill_snapshot=None,model='gpt-6-luna',reasoning_effort='high',tool=None,tool_binary=None):
    if tool and (condition != 'baseline' or arm != 'p'):
        raise ValueError('--tool requires --arm p --condition baseline')
    if condition in ('baseline','skill') and arm != 'p':
        raise ValueError('baseline and skill conditions require --arm p')
    if condition == 'skill':
        if not skill_snapshot:
            raise ValueError('skill condition requires --skill-snapshot')
        skill_manifest=verify_snapshot(skill_snapshot)
    else:
        skill_manifest=None
    run,work,prompt=r.prepare(run_id,arm,condition)
    if tool:
        prompt += tool_variant.instruction(tool)
        (run/'prompt.txt').write_text(prompt)
    for directory in ('public','private','tools'):
        shutil.copytree(ROOT/directory,run/directory,ignore=shutil.ignore_patterns('__pycache__'))
    delegation=delegation_prompt(run_id,arm,condition)
    if tool:
        delegation = delegation.replace(r.task_prompt(arm, condition), prompt, 1)
    (run/'delegation.txt').write_text(delegation)
    result=r.command(r.container_args('snappea-eval-'+run_id,work,arm,condition,skill_snapshot,tool,tool_binary))
    if result.returncode: raise RuntimeError(result.stderr)
    preflight=None
    if arm=='p':
        check=r.command(['docker','exec','snappea-eval-'+run_id,'bash','-c',
            f'command -v {tool or "snappea"} && {tool or "snappea"} version && python -m pytest --version && python -c "import snappea; from importlib.metadata import version; print(version(\'pysnappea\'))"'])
        preflight={'exit_code':check.returncode,'stdout':check.stdout,'stderr':check.stderr}
        if condition == 'skill' and check.returncode == 0:
            skill_check=r.command(['docker','exec','snappea-eval-'+run_id,'bash','-c',
                'test -r /opt/p-modeling/SKILL.md && find /opt/p-modeling/examples -type f -print -quit | grep -q .'])
            preflight['skill_mount']={'exit_code':skill_check.returncode,'stdout':skill_check.stdout,'stderr':skill_check.stderr}
            if skill_check.returncode:
                r.command(['docker','rm','-f','snappea-eval-'+run_id])
                raise RuntimeError('P modeling skill mount preflight failed: '+skill_check.stderr)
        if check.returncode:
            r.command(['docker','rm','-f','snappea-eval-'+run_id])
            raise RuntimeError('Snappea preflight failed: '+check.stderr)
    meta={'run_id':run_id,'arm':arm,'condition':condition,'prompt_version':r.prompt_version(),'prompt_variant':prompt_variant(arm,condition),'model':model,'reasoning_effort':reasoning_effort,
        'backend':'collaboration','tool_preflight':preflight,'environment_revision':'pysnappea-tests','snappea_on_path':arm=='p','tool_bridge_version':2,'delegation_prompt_sha256':hashlib.sha256(delegation.encode()).hexdigest(),'image':r.image_for_arm(arm),'limits':{'seconds':900,'tool_calls':80,'generated_tokens':None},
        'command_timeout_seconds':120 if condition in ('baseline','skill') else 40,
        'public_sha256':{n:hashlib.sha256((ROOT/'public'/n).read_bytes()).hexdigest() for n in r.PUBLIC},
        'prompt_sha256':hashlib.sha256(prompt.encode()).hexdigest(),'smoke':False,
        'status':'running','seconds':None,'generated_tokens':None,'input_tokens':None,'tool_calls':0,
        'provisioned_unix':time.time()}
    meta['archived_inputs_sha256']={str(p.relative_to(run)):hashlib.sha256(p.read_bytes()).hexdigest()
        for directory in ('public','private','tools') for p in sorted((run/directory).rglob('*'))
        if p.is_file() and not p.is_symlink()}
    if skill_manifest:
        meta['skill_snapshot']={'tree_sha256':skill_manifest['tree_sha256'],'files':skill_manifest['files']}
    if arm=='p':
        binary, source = r.tool_paths()
        revision = r.command(['git','-C',str(source),'rev-parse','HEAD'])
        meta['snappea_binary_sha256'] = hashlib.sha256(binary.read_bytes()).hexdigest()
        if tool:
            meta.update(tool_variant.metadata(binary, tool, tool_binary))
            meta['snappea_on_path'] = tool == 'snappea'
        meta['docs_source_revision'] = revision.stdout.strip() if revision.returncode == 0 else None
    (run/'metadata.json').write_text(json.dumps(meta,indent=2))
    print(json.dumps({'run':run_id,'arm':arm,'container':'snappea-eval-'+run_id}))

def finish(run_id,status,seeds):
    run=ROOT/'runs'/run_id
    tool_variant.verify(json.loads((run/'metadata.json').read_text()))
    with (run/'bridge.lock').open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        state_path=run/'bridge-state.json'
        state=json.loads(state_path.read_text()) if state_path.exists() else {'started':time.time(),'calls':0}
        state['closed']=True
        state_path.write_text(json.dumps(state,indent=2))
        r.command(['docker','rm','-f','snappea-eval-'+run_id])
        r.frozen_submission(run,run/'work')
        meta=json.loads((run/'metadata.json').read_text())
        meta.update(status=status,seconds=round(time.time()-state['started'],3),tool_calls=state['calls'],
            public_files_unchanged=all(hashlib.sha256((run/'work'/n).read_bytes()).hexdigest()==meta['public_sha256'][n] for n in r.PUBLIC))
        (run/'metadata.json').write_text(json.dumps(meta,indent=2))
    report=r.grade(run,seeds)
    print(json.dumps({'run':run_id,'passed':report['passed'],'total':report['total']}))

if __name__=='__main__':
    p=argparse.ArgumentParser()
    p.add_argument('action',choices=['start','finish'])
    p.add_argument('--run-id',required=True)
    p.add_argument('--arm',choices=['p','python'])
    p.add_argument('--condition',choices=['legacy','baseline','skill'],default='legacy')
    p.add_argument('--tool', choices=tool_variant.TOOLS, help='explicit no-skill CLI comparison')
    p.add_argument('--tool-binary', type=Path, help='chosen CLI binary (defaults beside native checker)')
    p.add_argument('--skill-snapshot',help='content-addressed snapshot from tools/snapshot_skill.py')
    p.add_argument('--model',default='gpt-6-luna')
    p.add_argument('--reasoning-effort',default='high',choices=['low','medium','high','xhigh','max','ultra'])
    p.add_argument('--status',default='agent_finished')
    p.add_argument('--seeds',type=int,default=64)
    a=p.parse_args()
    if a.action=='start': start(a.run_id,a.arm,a.condition,a.skill_snapshot,a.model,a.reasoning_effort,a.tool,a.tool_binary)
    else: finish(a.run_id,a.status,a.seeds)
