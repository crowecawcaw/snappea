#!/usr/bin/env python3
"""Join preserved prior Sol outcomes with verified missing-cell trials."""
import hashlib
import json
from pathlib import Path
ROOT=Path(__file__).resolve().parent
PRIOR=ROOT.parent/'single-file-examples'
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def load(p):return json.loads(Path(p).read_text())

def main():
    old=[r for r in load(PRIOR/'results.json') if r['model_family']=='sol']
    new=load(ROOT/'results.json');assert len(old)==8 and len(new)==8
    combined=[]
    for block,records,path in [('prior-diagonal',old,PRIOR/'results.json'),('missing-cells',new,ROOT/'results.json')]:
        for original in records:
            row=dict(original);row['execution_block']=block
            row['skill_present']=bool(row['skill_sha256'])
            row['cell']=('compat' if row['tool']=='snappea-compat' else 'native')+('-skill' if row['skill_present'] else '-no-skill')
            row['outcome_source']=str(path);row['outcome_source_sha256']=sha(path)
            combined.append(row)
    assert len({r['run_id'] for r in combined})==16
    for cell in ['compat-no-skill','compat-skill','native-no-skill','native-skill']:
        selected=[r for r in combined if r['cell']==cell]
        assert len(selected)==4
        assert sorted(r['task'] for r in selected)==['flow','flow','vcs','vcs']
    (ROOT/'combined-results.json').write_text(json.dumps(combined,indent=2)+'\n')
    for row in combined:print(row['run_id'],row['cell'],row['execution_block'])
if __name__=='__main__':main()
