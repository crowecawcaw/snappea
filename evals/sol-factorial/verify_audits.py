#!/usr/bin/env python3
"""Verify every file anchor recorded by the independent source/transcript audits."""
import hashlib
import json
from pathlib import Path
ROOT=Path(__file__).resolve().parent
REPO=ROOT.parents[1]
count=0
for name in ['AUDIT-FIFO-source-hashes.json','AUDIT-FIFO-transcript-hashes.json','AUDIT-VCS-source-hashes.json','AUDIT-VCS-transcript-hashes.json']:
    saved=json.loads((ROOT/name).read_text());files=saved.get('files',saved)
    for key,value in files.items():
        want=value['sha256'] if isinstance(value,dict) else value
        path=Path(key);path=path if path.is_absolute() else REPO/path
        actual=hashlib.sha256(path.read_bytes()).hexdigest()
        assert actual==want,(name,str(path))
        count+=1
    print(name,len(files))
print('Verified',count,'independent audit file anchors.')
