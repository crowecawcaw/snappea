#!/usr/bin/env python3
"""Uniform declarations-only assembly repair for existing P-dependent test axes."""
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import sys
import tempfile
from source_assembly import corrected_events
ROOT=Path(__file__).resolve().parent

def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def write(path,data):Path(path).write_text(json.dumps(data,indent=2)+'\n')

def main():
 records=[]
 for row in json.loads((ROOT/'runs/map.json').read_text()):
  if row['task']!='flow':continue
  run=Path(row['run']);meta=json.loads((run/'metadata.json').read_text());out=run/'supplemental-observation-source-v3';out.mkdir(exist_ok=False)
  assert meta['status']!='running'
  for name,want in json.loads((run/'submission-manifest.json').read_text()).items():assert sha(run/'submission'/name)==want
  for name,want in meta['grader_sha256'].items():assert sha(run/'private'/name)==want
  assert sha(meta['binary'])==meta['binary_sha256']
  sys.path.insert(0,str(run/'private'))
  spec=importlib.util.spec_from_file_location('archived_flow_grader_observation',run/'private/grade.py');g=importlib.util.module_from_spec(spec);spec.loader.exec_module(g)
  source=corrected_events(run/'public/events.p',run/'submission/events.p')
  record={'run_id':row['run_id'],'supplemental':True,'correction':'preserve exact public schema plus nonconflicting type/event declarations before or after it',
   'original_grade_sha256':sha(run/'grading/grade.json'),'submission_manifest_sha256':sha(run/'submission-manifest.json'),
   'script_sha256':sha(__file__),'assembly_helper_sha256':sha(ROOT/'source_assembly.py'),
   'assembled_events_sha256':hashlib.sha256(source.encode()).hexdigest(),
   'source_assembly_changed':source!=(run/'public/events.p').read_text(),'timeout_seconds':90}
  with tempfile.TemporaryDirectory(prefix='onefile-observation-source-') as temp:
   public=Path(temp)/'public';shutil.copytree(run/'public',public);(public/'events.p').write_text(source)
   # All three archived functions read their original public inputs through PUBLIC.
   # Only schema assembly changes; original runtime/tests/faults/limits stay exact.
   g.PUBLIC=public
   for name,function in [('submitted_tests',g.run_submitted_suite),('monitor_traces',g.run_monitor_traces),('portable_probes',g.run_portable_probes)]:
    record[name]=function(run/'submission',Path(meta['binary']),meta['image'],90)
    write(out/'result.json',record);print(row['run_id'],name,record[name].get('status',record[name].get('reference_status')),flush=True)
  records.append(record)
 write(ROOT/'runs/observation-source-v3.json',records)
if __name__=='__main__':main()
