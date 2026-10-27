#!/usr/bin/env python3
"""POSTHOC priority pop/re-admission probe; separate from primary scores.

Origin: source review found Luna priority tests re-admit canceled IDs but not
popped IDs. The original broad stale-admission-position variant can be killed
by either path, hiding that distinction. This narrower variant was selected
AFTER reviewing those sources. It is exploratory, not a preregistered effect
estimate. Frozen submissions, fixtures, grading reports, and scores are unchanged.

The candidate fault keeps cancel/re-admission correct but forgets admission-stamp
cleanup on pop. A private calibration witness admits equally ranked A and B,
pops A, re-admits A, and requires B next. Controls include both original correct
representations plus an equivalent implementation that clears stamps on both
removal paths. Full commands/results are saved under ignored runs/.

Observed on these four frozen submissions: Sol report-on and report-off drivers
detect the added fault; Luna report-on and report-off drivers do not. All four
accept all three correct controls. The Sol control searches remain incomplete;
the Luna controls complete. This supports the audited pop-reuse coverage gap,
not an estimated model or report-feature effect. Original primary scores stay
unchanged; this posthoc result is never pooled into them.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile

ROOT = Path(__file__).resolve().parents[1]
RUN_IDS = ['sol-priority-a01', 'sol-priority-b01',
           'luna-priority-a01', 'luna-priority-b01']
WITNESS_DRIVER = '''// snappea: driver
machine Main {
  var queue: machine;
  start state Run {
    entry {
      queue = new PriorityQueue();
      Call(0, 0, 0);
      Call(0, 1, 0);
      Call(3, 0, 0);
      Call(0, 0, 0);
      Call(3, 0, 0);
      Call(3, 0, 0);
      Call(3, 0, 0);
      Call(2, 2, 0);
      send queue, eFinish;
    }
  }
  fun Call(op: int, id: int, priority: int) {
    send queue, eCommand, (op=op, id=id, priority=priority, reply=this);
    receive { case eAck: { } }
  }
}
'''


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def manifest(path):
    return {str(p.relative_to(path)): digest(p) for p in sorted(Path(path).rglob('*'))
            if p.is_file() and not p.is_symlink()
            and not any(n in p.parts for n in ['__pycache__', 'PCheckerOutput', '.pytest_cache'])}


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def verify_frozen(run):
    metadata = json.loads((run/'metadata.json').read_text())
    assert metadata['status'] != 'running', run.name
    assert digest(metadata['binary']) == metadata['binary_sha256'], run.name
    for directory in ['public', 'private', 'harness']:
        assert manifest(run/directory) == metadata[directory+'_sha256'], (run.name, directory)
    expected = json.loads((run/'submission-manifest.json').read_text())
    assert manifest(run/'submission') == expected, run.name
    return metadata


def replace_once(text, old, new):
    assert text.count(old) == 1, (old, text.count(old))
    return text.replace(old, new, 1)


def probe_variants(correct, alternative):
    # Reuse a stamp only while it exists. Correct cleanup must remove it on BOTH
    # cancellation and pop. The mutant omits exactly the pop-side cleanup.
    mutant = replace_once(correct, '          admission[c.id] = serial;',
        '          if (!(c.id in admission)) { admission[c.id] = serial; }')
    mutant = replace_once(mutant, '          active -= (c.id);',
        '          active -= (c.id);\n          admission -= (c.id);')
    equivalent = replace_once(mutant, '        if (found) { active -= (chosen); }',
        '        if (found) { active -= (chosen); admission -= (chosen); }')
    return {'correct': correct, 'correct_alt': alternative,
            'correct_cleanup': equivalent, 'pop_reuse_old_position': mutant}


def check(run, metadata, common, sources, variant, driver, tests):
    with tempfile.TemporaryDirectory(prefix='priority-pop-reuse-') as t:
        work = Path(t)
        shutil.copytree(run/'public', work, dirs_exist_ok=True)
        (work/'model.p').write_text(sources[variant])
        (work/'driver.p').write_text(driver)
        (work/'tests.p').write_text(tests)
        return common.run_check(work, metadata, run/'harness/cli_wrapper.py')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run-ids', nargs='+', default=RUN_IDS)
    parser.add_argument('--output', type=Path,
        default=ROOT/'runs/probes-priority-pop-reuse/results.json')
    args = parser.parse_args()
    rows = []
    calibration = []
    evidence = []
    canonical = None
    for i, run_id in enumerate(args.run_ids):
        run = ROOT/'runs'/run_id
        metadata = verify_frozen(run)
        retained_paths = ['metadata.json', 'submission-manifest.json',
                          'grading/grade.json', 'tool-transcript.jsonl']
        retained_before = {name: digest(run/name) for name in retained_paths}
        common = load(run/'harness/common.py', 'pop_reuse_common_'+str(i))
        original_variants = load(run/'private/variants.py', 'pop_reuse_variants_'+str(i))
        correct = (run/'public/model.p').read_text()
        sources = probe_variants(correct, original_variants.variants(correct)['correct_alt'])
        protected = {n: metadata['public_sha256'][n] for n in ['events.p', 'model.p', 'monitor.p']}
        if canonical is None:
            canonical = protected
            # Calibrate once against original driver and the new, explicit witness.
            for driver_name, driver in [('initial', (run/'public/driver.p').read_text()),
                                        ('witness', WITNESS_DRIVER)]:
                for variant in sources:
                    result = check(run, metadata, common, sources, variant, driver,
                                   (run/'public/tests.p').read_text())
                    expected = ('assertion_failure' if driver_name == 'witness'
                                and variant == 'pop_reuse_old_position' else 'accepted')
                    assert result['status'] == expected, (driver_name, variant, result)
                    if expected == 'assertion_failure':
                        assert any('monitor.p:' in b and 'stable admission order' in b
                                   for b in result['assertion_bugs'])
                    calibration.append({'driver': driver_name, 'variant': variant, **result})
        else:
            assert protected == canonical, run_id
        for variant in sources:
            result = check(run, metadata, common, sources, variant,
                (run/'submission/driver.p').read_text(), (run/'submission/tests.p').read_text())
            rows.append({'run': run_id, 'variant': variant, **result})
        # Recheck all archives after running from temporary restored copies.
        verify_frozen(run)
        retained_after = {name: digest(run/name) for name in retained_paths}
        assert retained_after == retained_before, run_id
        evidence.append({'run': run_id, 'model': metadata['model'],
            'reasoning_effort': metadata['reasoning_effort'], 'reports': metadata['reports'],
            'binary_sha256': metadata['binary_sha256'], 'image': metadata['image'],
            'protected_public_sha256': protected,
            'submission_manifest_sha256': digest(run/'submission-manifest.json'),
            'primary_grade_sha256': digest(run/'grading/grade.json'),
            'retained_artifacts_before_sha256': retained_before,
            'retained_artifacts_after_sha256': retained_after,
            'source_sha256': {name: hashlib.sha256(text.encode()).hexdigest()
                              for name, text in sources.items()}})
    summary = []
    for run_id in args.run_ids:
        selected = [r for r in rows if r['run'] == run_id]
        controls = [r for r in selected if r['variant'].startswith('correct')]
        fault = next(r for r in selected if r['variant'] == 'pop_reuse_old_position')
        valid = all(r['status'] == 'accepted' for r in controls)
        detected = (valid and fault['status'] == 'assertion_failure'
            and any('monitor.p:' in b for b in fault['assertion_bugs']))
        summary.append({'run': run_id, 'correct_controls_accepted':
            sum(r['status'] == 'accepted' for r in controls), 'correct_controls_total': len(controls),
            'correct_controls_incomplete': sum(r['status'] == 'accepted'
                and not r['search_complete'] for r in controls),
            'fault_status': fault['status'], 'fault_detected': detected})
    output = {'label': 'POSTHOC exploratory probe, separate from primary scores',
        'origin': 'Chosen after source audit found missing popped-ID re-admission in Luna tests; broad original stale-order fault was caught through cancellation.',
        'scope': 'One additional narrowly isolated fault; no population/effect-size claim and no changes to frozen artifacts or original scores.',
        'probe_source_sha256': digest(__file__),
        'summary': summary, 'evidence': evidence,
        'calibration': calibration, 'results': rows}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(output, indent=2)+'\n')
    print(json.dumps({'output': str(args.output), 'summary': summary}, indent=2))


if __name__ == '__main__':
    main()
