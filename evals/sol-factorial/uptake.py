#!/usr/bin/env python3
"""Reconstruct bridge-visible reference text; line matches are descriptive evidence."""
import json
from pathlib import Path
ROOT=Path(__file__).resolve().parent

def visible_stdout(row,record):
    raw=record['result']['stdout'];limit=18000 if row['task']=='flow' else 16000
    return raw if len(raw)<=limit else raw[:limit//2]+'\n[bridge truncated middle]\n'+raw[-limit//2:]

def ranges(numbers):
    groups=[]
    for n in sorted(set(numbers)):
        if groups and n==groups[-1][-1]+1:groups[-1].append(n)
        else:groups.append([n])
    return [str(g[0]) if len(g)==1 else f'{g[0]}-{g[-1]}' for g in groups]

def main():
    results=[]
    for row in json.loads((ROOT/'runs/map.json').read_text()):
        run=Path(row['run']);meta=json.loads((run/'metadata.json').read_text())
        logs=[json.loads(l) for l in (run/'tool-transcript.jsonl').read_text().splitlines()]
        records=[]
        if meta['skill_snapshot']:
            snapshot=Path(meta['skill_snapshot'])
            # Restrict matching to explicit reference-path calls. Ignore short/common
            # P lines, and preserve line ranges for human review. These matches show
            # text delivered by the bridge, not comprehension or copying.
            files=[p for p in snapshot.rglob('*') if p.is_file() and p.suffix in ['.p','.py','.md'] and p.name!='snapshot.json']
            for log in logs:
                if '/opt/p-modeling' not in log['command']:continue
                output=visible_stdout(row,log);output_lines=set(output.splitlines())
                matches=[]
                for p in files:
                    text=p.read_text();lines=text.splitlines()
                    nums=[i for i,line in enumerate(lines,1) if len(line.strip())>=16 and line in output_lines]
                    if len(nums)>=3:
                        matches.append({'file':str(p.relative_to(snapshot)),'matching_substantive_lines':len(nums),'line_ranges':ranges(nums),'source_lines':len(lines)})
                records.append({'call':log['call'],'command':log['command'],'stdout_characters':len(log['result']['stdout']),'bridge_truncated':len(log['result']['stdout'])>(18000 if row['task']=='flow' else 16000),'visible_file_line_matches':matches})
        results.append({**row,'reference_calls':records,'checker_exit2_calls':[x['call'] for x in logs if x['result']['exit_code']==2 and ('snappea' in x['command'])]})
    (ROOT/'uptake.json').write_text(json.dumps(results,indent=2)+'\n')
if __name__=='__main__':main()
