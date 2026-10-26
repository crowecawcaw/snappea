#!/usr/bin/env python3
"""Independent model/monitor or implementation-test checks for novel scenarios."""
import argparse
import importlib.util
import json
import re
from pathlib import Path
import shutil
import subprocess
import tempfile
import uuid

from runner import ROOT,container_args
from grade import verify

CONFIG = {
    'migration': {'spec':'MigrationContract','service':'migration.py'},
    'streaming': {'spec':'WindowContract','service':'window.py'},
    'priority': {'spec':'QueueContract','service':'queue_service.py'},
}


def module(path,name):
    spec=importlib.util.spec_from_file_location(name,path)
    result=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def execute(work,meta,args,timeout=60):
    name='snappea-novel-grade-'+uuid.uuid4().hex[:12]
    command=container_args(work,image=meta['image'],binary=Path(meta['binary']))
    command[2:2]=['--name',name]
    try:
        result=subprocess.run(command+args,text=True,capture_output=True,timeout=timeout)
        return {'exit_code':result.returncode,'stdout':result.stdout,'stderr':result.stderr,'timeout':False}
    except subprocess.TimeoutExpired as e:
        return {'exit_code':124,'stdout':str(e.stdout or ''),'stderr':str(e.stderr or ''),'timeout':True}
    finally:
        subprocess.run(['docker','rm','-f',name],capture_output=True)


def model_grade(run,meta,config,out):
    cases=module(run/'private/cases.py','novel_cases')
    with tempfile.TemporaryDirectory(prefix='snappea-novel-model-') as tmp:
        work=Path(tmp)
        for name in ['model.p','monitor.p','tests.p','DESIGN.md']:
            source=run/'submission'/name
            if source.is_file() and not source.is_symlink():
                shutil.copy2(source,work/name)
        shutil.copy2(run/'public/events.p',work/'events.p')
        native=execute(work,meta,['snappea','check','events.p','monitor.p','model.p','tests.p',
                                  '--timeout','5s','--mutation-timeout','5s','--json'],timeout=75)
        try:
            parsed=json.loads(native['stdout'])
        except ValueError:
            parsed={}
        selected=parsed.get('test_cases',[]) if isinstance(parsed,dict) else []
        traces=[]
        monitor_sources=['events.p','monitor.p']
        full_sources=[name for name in ['events.p','monitor.p','model.p','tests.p'] if (work/name).is_file()]
        for name,valid,events in cases.cases():
            (work/'history.jsonl').write_text(''.join(json.dumps(event)+'\n' for event in events))
            result=execute(work,meta,['snappea','observe',*monitor_sources,'history.jsonl',
                           '--spec',config['spec'],'--strict','--unknown-events','error','--json'],timeout=20)
            if result['exit_code']==2 and monitor_sources!=full_sources:
                # Shared types/helpers may live with the model. Observation loads
                # declarations but does not execute model machines.
                expanded=execute(work,meta,['snappea','observe',*full_sources,'history.jsonl',
                                  '--spec',config['spec'],'--strict','--unknown-events','error','--json'],timeout=20)
                if expanded['exit_code'] in (0,1):
                    result=expanded
                    monitor_sources=full_sources
            try:
                observation=json.loads(result['stdout'])
                violation=any(error.get('kind')=='spec_violation'
                              for spec in observation.get('specs',[])
                              for error in spec.get('errors',[]))
            except (ValueError,AttributeError):
                violation=False
            passed=(result['exit_code']==0 if valid else result['exit_code']==1 and violation)
            traces.append(dict(result,name=name,expected_valid=valid,passed=passed and not result['timeout']))
    summary={'model_check_passed':native['exit_code']==0 and bool(selected),
             'selected_tests':len(selected),'search_complete':bool(selected) and all(t.get('search_complete',False) for t in selected),
             'schedules':sum(t.get('schedules',0) for t in selected),
             'valid_accepted':sum(t['passed'] for t in traces if t['expected_valid']),
             'valid_total':sum(t['expected_valid'] for t in traces),
             'invalid_rejected':sum(t['passed'] for t in traces if not t['expected_valid']),
             'invalid_total':sum(not t['expected_valid'] for t in traces),
             'timeouts':int(native['timeout'])+sum(t['timeout'] for t in traces)}
    summary['all_probes_passed']=(summary['model_check_passed'] and summary['search_complete']
                                  and summary['valid_accepted']==summary['valid_total']
                                  and summary['invalid_rejected']==summary['invalid_total'] and summary['timeouts']==0)
    return {'version':'novel-model-v2','summary':summary,'monitor_source_files':monitor_sources,'native_check':native,'model_report':parsed,'monitor_cases':traces,
            'scope':'Model fidelity requires source audit; monitor trace success alone is insufficient.'}


def classify(result):
    output=result['stdout']+result['stderr']
    match=re.search(r'Ran\s+(\d+)\s+tests?',output)
    count=int(match.group(1)) if match else 0
    setup_markers=['ModuleNotFoundError','ImportError','SyntaxError','SetupError',
                   'SnappeaCompilationError','compilation failed','could not parse',
                   'Could not find the snappea binary','matches no files','unselected-spec']
    failure=re.search(r'FAILED \(([^)]*)\)',output)
    if result['timeout']:
        status='timeout'
    elif count==0 or any(word.lower() in output.lower() for word in setup_markers):
        status='infrastructure_error'
    elif result['exit_code']==0:
        status='passed'
    elif failure and ('failures=' in failure.group(1) or 'errors=' in failure.group(1)):
        status='tests_rejected'
    else:
        status='infrastructure_error'
    return {'status':status,'test_count':count}


def tests_grade(run,meta,config,out):
    variants=module(run/'private/mutants.py','novel_mutants')
    correct=(run/'public'/config['service']).read_text()
    sources=variants.variants(correct)
    if sources.get('correct')!=correct:
        raise ValueError('variants must include the unchanged correct implementation')
    if len(set(sources.values()))!=len(sources):
        raise ValueError('duplicate/no-op mutation')
    results=[]
    for name,source in sources.items():
        with tempfile.TemporaryDirectory(prefix='snappea-novel-tests-') as tmp:
            work=Path(tmp)
            for path in (run/'submission').rglob('*.py'):
                if path.is_symlink() or '__pycache__' in path.parts:
                    continue
                target=work/path.relative_to(run/'submission')
                target.parent.mkdir(parents=True,exist_ok=True)
                shutil.copy2(path,target)
            # Restore fixed public inputs after copying candidate helpers.
            for path in (run/'public').iterdir():
                if path.is_file():shutil.copy2(path,work/path.name)
            (work/config['service']).write_text(source)
            result=execute(work,meta,['python','-m','unittest','discover','-v','-p','test*.py'],timeout=60)
            status=classify(result)
            expected_valid=name=='correct' or name.startswith('valid-')
            results.append(dict(result,**status,name=name,expected_valid=expected_valid))
    valid=[r for r in results if r['expected_valid']]
    faulty=[r for r in results if not r['expected_valid']]
    summary={'valid_passed':sum(r['status']=='passed' for r in valid),'valid_total':len(valid),
             'faults_rejected':sum(r['status']=='tests_rejected' for r in faulty),'faults_total':len(faulty),
             'setup_errors':sum(r['status']=='infrastructure_error' for r in results),
             'timeouts':sum(r['timeout'] for r in results)}
    summary['all_probes_passed']=(summary['valid_passed']==summary['valid_total']
                                  and summary['faults_rejected']==summary['faults_total']
                                  and summary['setup_errors']==0 and summary['timeouts']==0)
    return {'version':'novel-testing-v1','summary':summary,'variants':results,
            'scope':'Actual observation provenance requires source review. Fixed service/monitor, wrapper-captured calls.'}


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run-id',required=True)
    args=parser.parse_args()
    run=ROOT/'runs'/args.run_id
    meta=verify(run)
    scenario,phase=meta['phase'].split('/')
    config=CONFIG[scenario]
    out=run/'grading';out.mkdir(exist_ok=True)
    report=(model_grade if phase=='model' else tests_grade)(run,meta,config,out)
    (out/'grade.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report['summary']))
