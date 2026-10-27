"""Calibrate weak/strong drivers against corrects and semantic faults."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
from variants import variants

ROOT = Path(__file__).resolve().parent.parent


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--output', type=Path)
    p.add_argument('--raw-output', type=Path,
        default=ROOT.parents[1]/'runs/calibration-priority/results.json')
    args = p.parse_args()
    correct = (ROOT/'public/model.p').read_text()
    rows = []
    for driver in ['initial', 'reference', 'negative_id']:
        for name, source in variants(correct).items():
            if driver == 'negative_id' and not name.startswith('correct'):
                continue
            with tempfile.TemporaryDirectory(prefix='priority-driver-calibration-') as t:
                work = Path(t)
                for f in (ROOT/'public').glob('*.p'):
                    shutil.copy2(f, work/f.name)
                (work/'model.p').write_text(source)
                if driver == 'reference':
                    shutil.copy2(ROOT/'private/reference_driver.p', work/'driver.p')
                if driver == 'negative_id':
                    driver_path = work/'driver.p'
                    text = driver_path.read_text()
                    assert 'Call(0, 0, -1);' in text
                    driver_path.write_text(text.replace('Call(0, 0, -1);',
                        'Call(0, -2, -1);', 1))
                command = [str(args.binary.resolve()), 'check', 'events.p',
                    'model.p', 'monitor.p', 'driver.p', 'tests.p', '--no-mutation',
                    '--timeout', '2s', '--allow-incomplete-coverage', '--json']
                r = subprocess.run(command,
                    cwd=work, text=True, capture_output=True, timeout=15)
                data = json.loads(r.stdout)
                cases = data.get('test_cases', [])
                assertion = r.returncode == 1 and any(
                    c.get('result') == 'bug' and 'Assertion Failed: monitor.p:' in
                    c.get('bug', {}).get('report', '') for c in cases)
                row = {'driver': driver, 'variant': name, 'exit_code': r.returncode,
                       'semantic_assertion': assertion, 'report': data,
                       'command': command, 'stdout': r.stdout, 'stderr': r.stderr}
                rows.append(row)
                expected = (1 if driver == 'negative_id' else
                            0 if name.startswith('correct') else
                            1 if driver == 'reference' or name == 'cancel_no_effect' else 0)
                assert r.returncode == expected, (driver, name, r.returncode, r.stdout)
                if expected == 1:
                    assert assertion and 'monitor.p' in r.stdout, (driver, name, r.stdout)
                if driver == 'negative_id':
                    assert 'item IDs must be nonnegative' in r.stdout
                print(driver, name, r.returncode)
    args.raw_output.parent.mkdir(parents=True, exist_ok=True)
    args.raw_output.write_text(json.dumps({'fixture': 'priority', 'rows': rows},
        indent=2)+'\n')
    summary_rows = [{k: v for k, v in row.items()
                     if k not in ('report', 'command', 'stdout', 'stderr')}
                    for row in rows]
    output = {'fixture': 'priority', 'rows': summary_rows,
              'initial_faults_detected': 1, 'reference_faults_detected': 6,
              'latent_faults': ['ties_lifo', 'reprioritize_renumbers',
                               'readmit_keeps_old_position', 'absolute_priority',
                               'duplicate_overwrites_priority'],
              'baseline_detected_regression_control': 'cancel_no_effect',
              'correct_variants_accepted': 2,
              'negative_id_rejected_by_correct_variants': 2,
              'raw_results': 'runs/calibration-priority/results.json'}
    if args.output:
        args.output.write_text(json.dumps(output, indent=2)+'\n')


if __name__ == '__main__':
    main()
