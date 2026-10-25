#!/usr/bin/env python3
"""Check novel scenario monitors and saved-test graders against private controls."""
import argparse
import json
from pathlib import Path
import shutil
import tempfile
from grade_novel import CONFIG,model_grade,tests_grade
from runner import ROOT,BINARY,IMAGE,command

REFERENCE = {
    'migration': {'model':'private/reference_solution','testing':'private/calibration/reference_tests'},
    'streaming': {'model':'private','testing':'private/reference_suite'},
}


def calibrate(scenario,binary,output):
    image=command(['docker','image','inspect',IMAGE,'--format','{{.Id}}'])
    if image.returncode:raise RuntimeError(image.stderr)
    meta={'image':image.stdout.strip(),'binary':str(binary)}
    output.mkdir(parents=True,exist_ok=True)
    passed=True
    for phase in ['model','testing']:
        source=ROOT/scenario/phase
        with tempfile.TemporaryDirectory(prefix='snappea-novel-controls-') as tmp:
            run=Path(tmp)
            for folder in ['public','private']:
                shutil.copytree(source/folder,run/folder,
                                ignore=shutil.ignore_patterns('__pycache__','PCheckerOutput'))
            (run/'submission').mkdir()
            reference=source/REFERENCE[scenario][phase]
            if phase=='model':
                for name in ['model.p','monitor.p','tests.p','DESIGN.md']:
                    shutil.copy2(reference/name,run/'submission'/name)
            else:
                for path in reference.rglob('*.py'):
                    if '__pycache__' not in path.parts:shutil.copy2(path,run/'submission'/path.name)
            report=(model_grade if phase=='model' else tests_grade)(run,meta,CONFIG[scenario],output)
        summary=report['summary']
        # The migration reference P driver is intentionally partial. Calibration
        # establishes the monitor oracle and test sensitivity, not a full design.
        ok=(summary['valid_accepted']==summary['valid_total']
            and summary['invalid_rejected']==summary['invalid_total'] and summary['timeouts']==0
            if phase=='model' else summary['all_probes_passed'])
        passed &= ok
        (output/(phase+'.json')).write_text(json.dumps(report,indent=2)+'\n')
        print(json.dumps({'scenario':scenario,'phase':phase,'calibration_passed':ok,**summary}),flush=True)
    return passed


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('scenario',choices=CONFIG)
    p.add_argument('--binary',type=Path,default=BINARY)
    p.add_argument('--output',type=Path)
    a=p.parse_args()
    out=a.output or ROOT/'runs'/('calibration-'+a.scenario+'-current')
    raise SystemExit(0 if calibrate(a.scenario,a.binary.resolve(),out) else 1)
