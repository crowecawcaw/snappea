from pathlib import Path
import json,sys,shutil,tempfile,subprocess
root=Path(__file__).resolve().parent;sys.path.insert(0,str(root))
from common import IMAGE,container_args,command,write_json,manifest,digest
binary=root.parents[1]/'target/driver-eval/release/snappea'
reports=[]
pair=[]
with tempfile.TemporaryDirectory(prefix='driver-wrapper-preflight-') as t:
 work=Path(t);shutil.copytree(root/'fixtures/priority/public',work,dirs_exist_ok=True)
 for arm in ['on','off']:
  r=command(container_args(work,binary,root/'cli_wrapper.py',reports=arm)+['snappea','check','events.p','model.p','monitor.p','driver.p','tests.p','--json-report','sidecar.json'])
  d=json.loads(r.stdout);cases=d.get('test_cases',[]);has=any('driver' in c for c in cases)
  assert r.returncode==0 and cases and has==(arm=='on'),(arm,r.stdout,r.stderr)
  assert json.loads((work/'sidecar.json').read_text())==d
  journal=[json.loads(x) for x in (work/'.check-invocations.jsonl').read_text().splitlines()]
  pair.append((d,journal[-1]['effective']))
  reports.append({'arm':arm,'exit_code':r.returncode,'driver_report_present':has,'summary':d.get('summary')})
 def normalize(x):
  if isinstance(x,dict):return {k:normalize(v) for k,v in x.items() if k not in ['driver','seconds']}
  if isinstance(x,list):return [normalize(v) for v in x]
  return x
 assert pair[0][1]==pair[1][1]
 assert normalize(pair[0][0])==normalize(pair[1][0])
for fixture in ['priority','retry','sessions']:
 with tempfile.TemporaryDirectory(prefix='driver-harness-reference-') as t:
  run=Path(t)
  for directory in ['public','private']:shutil.copytree(root/'fixtures'/fixture/directory,run/directory)
  shutil.copytree(run/'public',run/'submission')
  shutil.copy2(run/'private/reference_driver.p',run/'submission/driver.p')
  (run/'harness').mkdir()
  for name in ['common.py','runner.py','bridge.py','grade.py','cli_wrapper.py']:shutil.copy2(root/name,run/'harness'/name)
  write_json(run/'metadata.json',{'status':'smoke','binary':str(binary),'binary_sha256':digest(binary),'image':IMAGE,**{d+'_sha256':manifest(run/d) for d in ['public','private','harness']}})
  write_json(run/'submission-manifest.json',manifest(run/'submission'))
  r=command([sys.executable,str(root/'grade.py'),'--run-root',str(run)],timeout=120)
  assert r.returncode==0,(fixture,r.stdout,r.stderr)
  report=json.loads((run/'grading/grade.json').read_text());s=report['summary'];assert s['valid'] and s['faults_detected']==s['faults_total'] and s['baseline_faults_detected']==1,(fixture,s)
  reports.append({'fixture':fixture,'reference_summary':s})
(root/'calibration.json').write_text(json.dumps(reports,indent=2)+'\n')
print(json.dumps(reports,indent=2))
