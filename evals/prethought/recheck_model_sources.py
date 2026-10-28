#!/usr/bin/env python3
"""Supplemental model-only recheck retaining nonconflicting internal event additions.

The original FIFO grader restores events.p, discarding legal internal event
extensions. Keep its raw result. Apply this source-assembly correction uniformly
to all four submissions with identical checker settings; do not rerun other axes.
"""
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import shutil
import sys
import tempfile

ROOT=Path(__file__).resolve().parent

def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()

def corrected_events(public, candidate):
    original=public.read_text()
    if not candidate.is_file(): return original
    text=candidate.read_text()
    if not text.startswith(original):
        raise ValueError('original public event declarations must remain an exact prefix')
    extra=text[len(original):]
    # This correction accepts only appended event declarations, never replacement
    # schema, handlers, assumptions, specs, functions, machines or suppression.
    declarations=re.findall(r'\bevent\s+(\w+)\s*(?::[^;]+)?;',extra)
    residual=re.sub(r'\bevent\s+\w+\s*(?::[^;]+)?;','',extra)
    if residual.strip(): raise ValueError('only appended internal event declarations supported')
    original_names=re.findall(r'\bevent\s+(\w+)',original)
    if len(set(declarations+original_names))!=len(declarations+original_names):
        raise ValueError('duplicate or conflicting events')
    return text

def main():
    results=[]
    for row in json.loads((ROOT/'runs/map.json').read_text()):
        if row['task']!='flow': continue
        run=Path(row['run']);meta=json.loads((run/'metadata.json').read_text())
        assert meta['status']!='running'
        assert sha(Path(meta['binary']))==meta['binary_sha256']
        for name,want in json.loads((run/'submission-manifest.json').read_text()).items():
            assert sha(run/'submission'/name)==want
        sys.path.insert(0,str(run/'private'))
        # Private grader modules are identical across these frozen inputs. Load
        # each archived grade.py; auxiliary modules stay pinned by the manifest.
        for name,want in meta['grader_sha256'].items(): assert sha(run/'private'/name)==want
        spec=importlib.util.spec_from_file_location('archived_flow_grade',run/'private/grade.py')
        g=importlib.util.module_from_spec(spec);spec.loader.exec_module(g)
        source=corrected_events(run/'public/events.p',run/'submission/events.p')
        with tempfile.TemporaryDirectory(prefix='prethought-model-source-') as temp:
            work=Path(temp);(work/'events.p').write_text(source)
            for name in ['monitor.p','model.p','tests.p','DESIGN.md']:
                shutil.copy2(run/'submission'/name,work/name)
            result=g.run_container(work,Path(meta['binary']),meta['image'],
                ['snappea','check','events.p','monitor.p','model.p','tests.p',
                 '--timeout','5s','--mutation-timeout','5s','--json'],90)
        record={'run_id':row['run_id'],'supplemental':True,
            'correction':'preserve nonconflicting appended internal event declarations',
            'original_grade_sha256':sha(run/'grading/grade.json'),
            'submission_manifest_sha256':sha(run/'submission-manifest.json'),
            'script_sha256':sha(Path(__file__)),
            'checker_args':['--timeout','5s','--mutation-timeout','5s','--json'],
            'public_events_sha256':sha(run/'public/events.p'),
            'assembled_events_sha256':hashlib.sha256(source.encode()).hexdigest(),
            'source_assembly_changed':source!=(run/'public/events.p').read_text(),
            'result':result}
        out=run/'supplemental-model-source-v2';out.mkdir(exist_ok=False)
        (out/'result.json').write_text(json.dumps(record,indent=2)+'\n')
        results.append(record)
        print(row['run_id'],result['exit_code'],flush=True)
    (ROOT/'runs/model-source-v2.json').write_text(json.dumps(results,indent=2)+'\n')

if __name__=='__main__': main()
