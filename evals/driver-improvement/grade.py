#!/usr/bin/env python3
"""Grade frozen driver/tests against archived correct and faulty algorithms."""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
from common import digest, manifest, policy_audit, run_check, write_json, transcript_evidence, p_code


def load_variants(path):
    spec = importlib.util.spec_from_file_location('driver_eval_variants', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def verify(run):
    meta = json.loads((run/'metadata.json').read_text())
    if meta['status'] == 'running':
        raise ValueError('freeze before grading')
    if digest(meta['binary']) != meta['binary_sha256']:
        raise ValueError('binary changed')
    for directory in ['public', 'private', 'harness']:
        if manifest(run/directory) != meta[directory+'_sha256']:
            raise ValueError('archived '+directory+' inputs changed')
    if manifest(run/'submission') != json.loads((run/'submission-manifest.json').read_text()):
        raise ValueError('frozen submission changed')
    return meta


def selected_monitors(public, submission):
    required = re.findall(r'\bspec\s+(\w+)\s+observes\b', (Path(public)/'monitor.p').read_text())
    # Both editable files are compiled together; P permits test declarations
    # in either file. Their filename does not determine monitor selection.
    text = '\n'.join((Path(submission)/name).read_text()
                     for name in ['driver.p', 'tests.p']
                     if (Path(submission)/name).is_file())
    text = p_code(text)
    return {s: bool(re.search(r'\bassert\s+'+re.escape(s)+r'\b', text)) for s in required}


def grade(run):
    meta = verify(run)
    public = run/'public'
    submission = run/'submission'
    module = load_variants(run/'private/variants.py')
    correct = (public/'model.p').read_text()
    variants = module.variants(correct)
    if variants.get('correct') != correct or not any(k.startswith('correct_') for k in variants):
        raise ValueError('variants require unchanged correct and alternative correct')
    if len(set(variants.values())) != len(variants):
        raise ValueError('duplicate or no-op variants')
    policy = policy_audit(public, submission)
    results = []
    baseline_results = []
    selected_specs = selected_monitors(public, submission)
    # The native unselected-spec lint separately validates program selection. This
    # lexical check additionally prevents suppressing/omitting a required monitor.
    for name, model in variants.items():
        with tempfile.TemporaryDirectory(prefix='driver-eval-baseline-') as temp:
            work = Path(temp)
            shutil.copytree(public, work, dirs_exist_ok=True)
            (work/'model.p').write_text(model)
            baseline_results.append({'variant': name, 'expected_correct': name.startswith('correct'),
                                     **run_check(work, meta, run/'harness/cli_wrapper.py')})
        with tempfile.TemporaryDirectory(prefix='driver-eval-grade-') as temp:
            work = Path(temp)
            shutil.copytree(public, work, dirs_exist_ok=True)
            for filename in ['driver.p', 'tests.p']:
                source = submission/filename
                if source.is_file() and not source.is_symlink():
                    shutil.copy2(source, work/filename)
                else:
                    (work/filename).write_text('')
            (work/'model.p').write_text(model)
            outcome = run_check(work, meta, run/'harness/cli_wrapper.py')
            results.append({'variant': name, 'expected_correct': name.startswith('correct'), **outcome})
    correct_results = [r for r in results if r['expected_correct']]
    faults = [r for r in results if not r['expected_correct']]
    valid = (policy['instruction_compliant'] and all(selected_specs.values())
             and all(r['status'] == 'accepted' for r in correct_results))
    baseline_detected = {r['variant'] for r in baseline_results if not r['expected_correct'] and r['status'] == 'assertion_failure'}
    detected = {r['variant'] for r in faults if r['status'] == 'assertion_failure'}
    correct_incomplete = sum(r['status'] == 'accepted' and not r['search_complete'] for r in correct_results)
    runtime_evidence = transcript_evidence(run)
    summary = {'valid': valid, 'static_instruction_checks_passed': policy['instruction_compliant'],
               'instruction_review_required': True,
               'runtime_instruction_mentions': len(runtime_evidence['potential_attempts']),
               'required_monitors_selected': all(selected_specs.values()),
               'correct_accepted': sum(r['status'] == 'accepted' for r in correct_results),
               'correct_total': len(correct_results),
               'correct_incomplete': correct_incomplete,
               'correct_search_complete_count': sum(r['status'] == 'accepted' and r['search_complete'] for r in correct_results),
               'correct_all_search_complete': not correct_incomplete and all(r['status'] == 'accepted' for r in correct_results),
               'score_qualification': 'incomplete_correct_search' if correct_incomplete else 'complete_correct_search',
               'baseline_faults_detected': len(baseline_detected),
               'new_faults_detected': len(detected-baseline_detected) if valid else None,
               'baseline_faults_lost': len(baseline_detected-detected) if valid else None,
               'faults_detected': sum(r['status'] == 'assertion_failure' for r in faults) if valid else None,
               'faults_total': len(faults),
               'fault_assertion_failures_uncredited': sum(r['status'] == 'assertion_failure' for r in faults) if not valid else 0,
               'infrastructure_errors': sum(r['status'] in ['infrastructure_error', 'timeout'] for r in results)}
    report = {'version': 'driver-improvement-v1', 'run': run.name, 'summary': summary,
              'policy': policy, 'instruction_compliance_scope': 'Static source/input checks; runtime attempts require transcript review.',
              'runtime_instruction_evidence': runtime_evidence,
              'selected_specs': selected_specs, 'results': results, 'baseline_results': baseline_results,
              'scope': 'Correct-variant acceptance and fault detection are bounded checks, not proofs; source/transcript review checks instructions and feedback uptake.'}
    (run/'grading').mkdir(exist_ok=True)
    write_json(run/'grading/grade.json', report)
    print(json.dumps(summary))
    return report


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--run-root', required=True, type=Path)
    a = p.parse_args()
    run = a.run_root.resolve()
    archived = run/'harness/grade.py'
    if Path(__file__).resolve() != archived.resolve():
        raise SystemExit(subprocess.run([sys.executable, str(archived), '--run-root', str(run)]).returncode)
    grade(run)
