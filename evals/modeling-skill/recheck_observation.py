#!/usr/bin/env python3
"""Rerun a frozen submission's saved unittest modules in a fresh P container."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT=Path(__file__).resolve().parents[1]/'certified-bundles'
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('run')
p.add_argument('--binary', type=Path, required=True)
a=p.parse_args()
run=ROOT/'runs'/a.run
meta=json.loads((run/'metadata.json').read_text())
binary=a.binary.resolve()
assert hashlib.sha256(binary.read_bytes()).hexdigest()==meta['snappea_binary_sha256']
modules=sorted(f.stem for f in (run/'submission').glob('test*.py') if f.name!='test_public.py')
if not modules:
    raise SystemExit('No saved custom unittest module')
# Arguments are fixed discovered module basenames, passed as positional shell args.
command=['docker','run','--rm','--network','none','--cap-drop','ALL','--security-opt','no-new-privileges',
         '--user',f'{os.getuid()}:{os.getgid()}','--memory','768m','--cpus','1','--pids-limit','128',
         '--read-only','--tmpfs','/tmp:rw,size=128m',
         '--mount',f'type=bind,src={run}/submission,dst=/submission,readonly',
         '--mount',f'type=bind,src={ROOT}/public,dst=/public,readonly',
         '--mount',f'type=bind,src={binary},dst=/opt/snappea,readonly',
         '--env','SNAPPEA_BIN=/opt/snappea','--env','PATH=/opt:/usr/local/bin:/usr/bin:/bin',
         meta['image'],'bash','-c',
         'mkdir /tmp/run && cp -a /submission/. /tmp/run/ && cp /public/*.py /tmp/run/ && cd /tmp/run && python -m unittest -v test_public "$@"',
         'recheck',*modules]
r=subprocess.run(command,capture_output=True,text=True,timeout=180)
result={'run':a.run,'modules':modules,'exit_code':r.returncode,'stdout':r.stdout,'stderr':r.stderr}
(run/'observation-recheck.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result))
raise SystemExit(r.returncode)
