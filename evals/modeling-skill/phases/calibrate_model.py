#!/usr/bin/env python3
"""Calibrate hidden monitor histories against the fixed lease contract monitor."""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
from runner import ROOT,BINARY


def main(binary):
    spec=importlib.util.spec_from_file_location('model_cases',ROOT/'modeling/private/cases.py')
    cases=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(cases)
    failed=[]
    with tempfile.TemporaryDirectory(prefix='snappea-monitor-calibration-') as tmp:
        work=Path(tmp)
        shutil.copy2(ROOT/'testing/public/lease_contract.p',work/'monitor.p')
        shutil.copy2(ROOT/'modeling/public/events.p',work/'events.p')
        for name,valid,events in cases.cases():
            (work/'history.jsonl').write_text(''.join(json.dumps(e)+'\n' for e in events))
            r=subprocess.run([str(binary),'observe','events.p','monitor.p','history.jsonl',
                              '--spec','LeaseContract','--strict','--unknown-events','error','--json'],
                             cwd=work,text=True,capture_output=True,timeout=15)
            expected=0 if valid else 1
            ok=r.returncode==expected and (valid or 'spec_violation' in r.stdout)
            print(f'{name}: {"PASS" if ok else "FAIL"}')
            if not ok:
                failed.append(name)
    if failed:
        raise SystemExit(f'Calibration failed: {failed}')
    print('Monitor controls: 5 valid accepted, 13 invalid rejected. No algorithm-model claim.')


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary',type=Path,default=BINARY)
    main(p.parse_args().binary.resolve())
