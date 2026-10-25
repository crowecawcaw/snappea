#!/usr/bin/env python3
"""Isolated, transcript-preserving Sonnet agent runner (Anthropic Messages API)."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
IMAGE = os.environ.get('SNAPPEA_EVAL_IMAGE', 'python:3.12-slim@sha256:cb33f7f771ecec2ef1225cf39b159591a040391bd73e4c01f00fdfc4c47611d2')
P_IMAGE = os.environ.get('SNAPPEA_EVAL_P_IMAGE', IMAGE)
PUBLIC = ['TASK.md','contract.py','runtime.py','test_public.py']
from prompts import COMMON, P, CONTROL, VERSION as PROMPT_VERSION, task_prompt, prompt_version, prompt_variant
from skill_snapshot import verify_snapshot


def tool_paths():
    binary = Path(os.environ.get('SNAPPEA_EVAL_BINARY', ROOT/'vendor/bin/snappea-v0.1.0-x86_64-unknown-linux-gnu/snappea')).resolve()
    source = Path(os.environ.get('SNAPPEA_EVAL_SOURCE', ROOT/'vendor/source/snappea-main')).resolve()
    return binary, source


def image_for_arm(arm):
    return P_IMAGE if arm == 'p' else IMAGE


def grading_image(run):
    return json.loads((run/'metadata.json').read_text()).get('image', IMAGE)


def command(argv, **kwargs):
    return subprocess.run(argv,capture_output=True,text=True,**kwargs)


def prepare(run_id, arm, condition='legacy'):
    run = ROOT/'runs'/run_id
    work = run/'work'
    if work.exists(): raise SystemExit(f'Run already exists: {run_id}')
    work.mkdir(parents=True)
    for name in PUBLIC: shutil.copy2(ROOT/'public'/name,work/name)
    (work/'solution.py').write_text('from contract import Service\n')
    prompt = task_prompt(arm, condition)
    (run/'prompt.txt').write_text(prompt)
    return run,work,prompt


def container_args(name,work,arm,condition='legacy',skill_snapshot=None):
    args = ['docker','run','-d','--name',name,'--network','none','--cap-drop','ALL',
        '--security-opt','no-new-privileges','--user',f'{os.getuid()}:{os.getgid()}','--memory','768m','--cpus','1',
        '--pids-limit','128','--read-only','--tmpfs','/tmp:rw,size=128m',
        '--mount',f'type=bind,src={work},dst=/work','-w','/work']
    if arm == 'p':
        binary, source = tool_paths()
        docs = source/'docs'
        inventory = source/'examples/model'
        args += ['--env','PATH=/opt:/usr/local/bin:/usr/local/sbin:/usr/sbin:/usr/bin:/sbin:/bin',
                 '--mount',f'type=bind,src={binary},dst=/opt/snappea,readonly',
                 '--mount',f'type=bind,src={docs},dst=/opt/pdocs,readonly',
                 '--mount',f'type=bind,src={inventory},dst=/opt/inventory,readonly']
        if condition == 'skill':
            if not skill_snapshot:
                raise ValueError('skill condition requires --skill-snapshot')
            verify_snapshot(skill_snapshot)
            args += ['--mount',f'type=bind,src={Path(skill_snapshot).resolve()},dst=/opt/p-modeling,readonly']
    return args + [image_for_arm(arm),'sleep','infinity']


def shell(name,cmd,timeout=40):
    try:
        r=command(['docker','exec',name,'bash','-c',cmd],timeout=timeout)
        return {'exit_code':r.returncode,'stdout':r.stdout,'stderr':r.stderr}
    except subprocess.TimeoutExpired as e:
        # Kill all processes in this tool invocation's container on timeout. Abort
        # the run rather than allowing an unobserved background writer to persist.
        command(['docker','kill',name])
        raise RuntimeError('tool command exceeded time limit; run aborted') from e


def request(model,messages,max_tokens,timeout=120):
    key=os.environ.get('ANTHROPIC_API_KEY')
    if not key: raise RuntimeError('ANTHROPIC_API_KEY is not configured')
    payload={'model':model,'max_tokens':max_tokens,'messages':messages,
        'tools':[{'name':'shell','description':'Run a shell command in your isolated workspace.',
          'input_schema':{'type':'object','properties':{'command':{'type':'string'}},
                          'required':['command'],'additionalProperties':False}}]}
    req=urllib.request.Request('https://api.anthropic.com/v1/messages',data=json.dumps(payload).encode(),
        headers={'x-api-key':key,'anthropic-version':'2023-06-01','content-type':'application/json'})
    try:
        with urllib.request.urlopen(req,timeout=timeout) as response: return json.load(response)
    except urllib.error.HTTPError as e:
        # Never print headers or credential-bearing request data.
        detail=e.read().decode()[:1000]
        raise RuntimeError(f'Anthropic HTTP {e.code}: {detail}') from None


def frozen_submission(run,work):
    dest=run/'submission'
    dest.mkdir(exist_ok=True)
    manifest={}
    for path in work.rglob('*'):
        if not path.is_file() or path.is_symlink() or '__pycache__' in path.parts: continue
        rel=path.relative_to(work)
        if rel.parts[0] in PUBLIC: continue
        target=dest/rel
        target.parent.mkdir(parents=True,exist_ok=True)
        shutil.copy2(path,target)
        manifest[str(rel)]=hashlib.sha256(path.read_bytes()).hexdigest()
    (run/'submission-manifest.json').write_text(json.dumps(manifest,indent=2))
    return dest


def grade(run,seeds):
    out=run/'grading'
    out.mkdir(exist_ok=True)
    args=['docker','run','--rm','--network','none','--cap-drop','ALL',
        '--security-opt','no-new-privileges','--user',f'{os.getuid()}:{os.getgid()}','--memory','768m','--cpus','1','--pids-limit','128',
        '--read-only','--tmpfs','/tmp:rw,size=128m',
        '--mount',f'type=bind,src={ROOT}/private,dst=/grader/private,readonly',
        '--mount',f'type=bind,src={ROOT}/public,dst=/grader/public,readonly',
        '--mount',f'type=bind,src={run}/submission,dst=/submission,readonly',
        '--mount',f'type=bind,src={out},dst=/output',grading_image(run),
        'python','/grader/private/grade.py','/submission/solution.py',
        '--seeds',str(seeds),'--output','/output/grade.json']
    r=command(args,timeout=1800)
    (run/'grader.log').write_text(r.stdout+r.stderr)
    if not (out/'grade.json').exists(): raise RuntimeError('grader did not produce a report; inspect grader.log')
    return json.loads((out/'grade.json').read_text())


def main(a):
    if not a.smoke and not os.environ.get('ANTHROPIC_API_KEY'):
        raise SystemExit('Sonnet execution blocked: configure ANTHROPIC_API_KEY. No API request was made.')
    run,work,prompt=prepare(a.run_id,a.arm)
    name='snappea-eval-'+a.run_id.replace('_','-')
    metadata={'run_id':a.run_id,'arm':a.arm,'prompt_version':prompt_version(),'prompt_variant':prompt_variant(a.arm),'model':a.model,'image':image_for_arm(a.arm),
        'limits':{'seconds':a.seconds,'generated_tokens':a.generated_tokens,'tool_calls':a.tool_calls},
        'public_sha256':{n:hashlib.sha256((ROOT/'public'/n).read_bytes()).hexdigest() for n in PUBLIC},
        'prompt_sha256':hashlib.sha256(prompt.encode()).hexdigest(),
        'smoke':a.smoke or getattr(a,'protocol_test',False)}
    (run/'metadata.json').write_text(json.dumps(metadata,indent=2))
    messages=[{'role':'user','content':prompt}]
    transcript=[]
    generated=0
    input_tokens=0
    calls=0
    start=time.monotonic()
    r=command(container_args(name,work,a.arm))
    if r.returncode: raise RuntimeError(r.stderr)
    status='running'
    try:
        if a.smoke:
            # Exercises both isolation and arm provisioning; NOT an agent trial.
            r=shell(name,'python --version; python -m unittest -v test_public')
            transcript.append({'smoke_public':r})
            assert r['exit_code'] != 0, 'starter should fail public tests'
            if a.arm == 'p':
                r=shell(name,'/opt/snappea version; cp -r /opt/inventory ./inventory; cd inventory; /opt/snappea check tcAtomicReserve')
                transcript.append({'smoke_p':r})
                assert r['exit_code']==0,r
            # Calibration reference enters only smoke runs, never real agent runs.
            shutil.copy2(ROOT/'private/reference.py',work/'solution.py')
            r=shell(name,'python -m unittest -v test_public')
            transcript.append({'smoke_reference':r})
            assert r['exit_code']==0,r
            status='smoke_completed'
        else:
            while time.monotonic()-start < a.seconds and generated < a.generated_tokens and calls < a.tool_calls:
                response=request(a.model,messages,min(8192,a.generated_tokens-generated), timeout=min(120,max(1,a.seconds-(time.monotonic()-start))))
                usage=response.get('usage',{})
                generated+=usage.get('output_tokens',0)
                input_tokens+=usage.get('input_tokens',0)
                transcript.append({'response':response,'elapsed':time.monotonic()-start})
                content=response['content']
                messages.append({'role':'assistant','content':content})
                results=[]
                for block in content:
                    if block.get('type') != 'tool_use': continue
                    if calls >= a.tool_calls or time.monotonic()-start >= a.seconds: break
                    calls+=1
                    result=shell(name,block['input']['command'],timeout=min(40,max(1,int(a.seconds-(time.monotonic()-start)))))
                    transcript.append({'tool_use_id':block['id'],'command':block['input']['command'],'result':result})
                    results.append({'type':'tool_result','tool_use_id':block['id'],
                        'content':json.dumps(result)[:16000]})
                (run/'transcript.json').write_text(json.dumps(transcript,indent=2))
                if not results:
                    status='agent_finished'
                    break
                messages.append({'role':'user','content':results})
            else: status='budget_exhausted'
    except Exception as exc:
        status='infrastructure_error'
        metadata['error']=f'{type(exc).__name__}: {exc}'
        raise
    finally:
        command(['docker','rm','-f',name])
        (run/'transcript.json').write_text(json.dumps(transcript,indent=2))
        metadata.update(status=status,seconds=round(time.monotonic()-start,3),
                        generated_tokens=generated,input_tokens=input_tokens,tool_calls=calls)
        metadata['public_files_unchanged']=all((work/n).is_file() and hashlib.sha256((work/n).read_bytes()).hexdigest()==metadata['public_sha256'][n] for n in PUBLIC)
        frozen_submission(run,work)
        (run/'metadata.json').write_text(json.dumps(metadata,indent=2))
    report=grade(run,a.seeds)
    print(json.dumps({'run':a.run_id,'status':status,'passed':report['passed'],'total':report['total']}))

if __name__=='__main__':
    p=argparse.ArgumentParser()
    p.add_argument('--run-id',required=True)
    p.add_argument('--arm',choices=['p','python'],required=True)
    p.add_argument('--model',default='claude-sonnet-4-6')
    p.add_argument('--seconds',type=int,default=900)
    p.add_argument('--generated-tokens',type=int,default=24000)
    p.add_argument('--tool-calls',type=int,default=80)
    p.add_argument('--seeds',type=int,default=24)
    p.add_argument('--smoke',action='store_true')
    main(p.parse_args())
