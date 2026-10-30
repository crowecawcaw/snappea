#!/usr/bin/env python3
"""Copy only frozen source artifacts into neutrally named audit directories."""
import json
from pathlib import Path
import random
import shutil
import hashlib
ROOT=Path(__file__).resolve().parent
rows=json.loads((ROOT/'runs/map.json').read_text());random.Random(20117).shuffle(rows)
out=ROOT/'runs/blind-source';out.mkdir(exist_ok=True);mapping={};counters={'flow':0,'vcs':0}
for row in rows:
 counters[row['task']]+=1
 label=('F' if row['task']=='flow' else 'V')+str(counters[row['task']])
 source=Path(row['run'])/'submission'
 if not source.is_dir():continue
 if not (out/label).exists():
  shutil.copytree(source,out/label,symlinks=True)
  for p in (out/label).rglob('*'):
   if p.is_file():p.chmod(0o444)
 mapping[label]={'run_id':row['run_id'],'source':str(source)}
(ROOT/'runs/blind-map.json').write_text(json.dumps(mapping,indent=2)+'\n')
hashes={str(p.relative_to(out)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(out.rglob('*')) if p.is_file()}
(ROOT/'runs/blind-source-hashes.json').write_text(json.dumps(hashes,indent=2)+'\n')
print(out)
print(', '.join(sorted(p.name for p in out.iterdir())))
