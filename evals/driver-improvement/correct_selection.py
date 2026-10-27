#!/usr/bin/env python3
"""Supplemental v2 correction for test declarations moved into driver.p.

Never runs the checker and never changes frozen inputs, execution records or the
original grade. Only monitor-selection-dependent eligibility/counts change.
"""
import argparse
from copy import deepcopy
from contextlib import contextmanager
import importlib.util
import json
from pathlib import Path
import sys
import time

from common import digest, write_json
from grade import selected_monitors, verify

ROOT = Path(__file__).resolve().parent


@contextmanager
def no_bytecode_writes():
    previous = sys.dont_write_bytecode
    sys.dont_write_bytecode = True
    try:
        yield
    finally:
        sys.dont_write_bytecode = previous


def module_at(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    with no_bytecode_writes():
        spec.loader.exec_module(module)
    return module


def summary_with_selection(report, selection):
    """Preserve all non-selection fields; qualify credit using original outcomes."""
    summary = deepcopy(report['summary'])
    correct = [r for r in report['results'] if r['expected_correct']]
    faults = [r for r in report['results'] if not r['expected_correct']]
    baseline = {r['variant'] for r in report['baseline_results']
                if not r['expected_correct'] and r['status'] == 'assertion_failure'}
    detected = {r['variant'] for r in faults if r['status'] == 'assertion_failure'}
    incomplete = sum(r['status'] == 'accepted' and not r['search_complete'] for r in correct)
    valid = (report['policy']['instruction_compliant'] and bool(selection)
             and all(selection.values()) and bool(correct)
             and all(r['status'] == 'accepted' for r in correct))
    summary.update(valid=valid, required_monitors_selected=bool(selection) and all(selection.values()),
                   correct_accepted=sum(r['status'] == 'accepted' for r in correct),
                   correct_total=len(correct), correct_incomplete=incomplete,
                   correct_search_complete_count=sum(r['status'] == 'accepted' and r['search_complete'] for r in correct),
                   correct_all_search_complete=not incomplete and bool(correct) and all(r['status'] == 'accepted' for r in correct),
                   score_qualification='incomplete_correct_search' if incomplete else 'complete_correct_search',
                   baseline_faults_detected=len(baseline),
                   new_faults_detected=len(detected-baseline) if valid else None,
                   baseline_faults_lost=len(baseline-detected) if valid else None,
                   faults_detected=len(detected) if valid else None, faults_total=len(faults),
                   fault_assertion_failures_uncredited=len(detected) if not valid else 0,
                   infrastructure_errors=sum(r['status'] in ['infrastructure_error', 'timeout'] for r in report['results']))
    return summary


def verify_original_report(run, original, archived_common, archived_grade):
    if original.get('version') != 'driver-improvement-v1' or original.get('run') != run.name:
        raise ValueError('unexpected original grade identity/version')
    with no_bytecode_writes():
        variants = archived_grade.load_variants(run/'private/variants.py').variants((run/'public/model.p').read_text())
    if variants.get('correct') != (run/'public/model.p').read_text():
        raise ValueError('archived variants do not preserve original correct model')
    expected = set(variants)
    for group in ['results', 'baseline_results']:
        rows = original.get(group, [])
        if len(rows) != len(expected) or {r.get('variant') for r in rows} != expected:
            raise ValueError('original grade variant set mismatch: '+group)
        for row in rows:
            if row['expected_correct'] != row['variant'].startswith('correct'):
                raise ValueError('original expected-correct label mismatch')
            classified = archived_common.classify(row)
            for name, value in classified.items():
                if row.get(name) != value:
                    raise ValueError(f'original execution classification mismatch: {group}/{row["variant"]}/{name}')
    if original['policy'] != archived_common.policy_audit(run/'public', run/'submission'):
        raise ValueError('original policy result does not match frozen inputs')
    old_selection = archived_grade.selected_monitors(run/'public', run/'submission')
    if original['selected_specs'] != old_selection:
        raise ValueError('original selection result does not match archived grader')
    if original['summary'] != summary_with_selection(original, old_selection):
        raise ValueError('original summary does not match original executed results')


def correct(run):
    run = Path(run).resolve()
    meta = verify(run)  # Verifies original archived inputs/harness, tool and submission.
    original_path = run/'grading/grade.json'
    original_hash = digest(original_path)
    original = json.loads(original_path.read_text())
    archived_common = module_at(run/'harness/common.py', 'selection_v2_archived_common')
    # Archived grade imports its own frozen common helpers, never the new ones.
    previous = sys.modules.get('common')
    try:
        sys.modules['common'] = archived_common
        archived_grade = module_at(run/'harness/grade.py', 'selection_v2_archived_grade')
    finally:
        if previous is None:
            sys.modules.pop('common', None)
        else:
            sys.modules['common'] = previous
    verify_original_report(run, original, archived_common, archived_grade)
    selection = selected_monitors(run/'public', run/'submission')
    updated = deepcopy(original)
    updated['version'] = 'driver-improvement-selection-v2'
    updated['selected_specs'] = selection
    updated['summary'] = summary_with_selection(original, selection)
    updated['correction'] = 'Monitor selection scans both allowed driver.p and tests.p; all original checker executions are reused unchanged.'
    provenance = {
        'recorded_unix': time.time(), 'run': run.name,
        'reason': 'Original grader incorrectly assumed test declarations must live in tests.p.',
        'original_grade_sha256': original_hash, 'original_grade_version': original['version'],
        'original_valid': original['summary']['valid'], 'original_selected_specs': original['selected_specs'],
        'corrected_selected_specs': selection,
        'original_metadata_sha256': digest(run/'metadata.json'),
        'original_submission_manifest_sha256': digest(run/'submission-manifest.json'),
        'original_binary_sha256': meta['binary_sha256'], 'original_image': meta['image'],
        'original_harness_sha256': meta['harness_sha256'],
        'original_public_sha256': meta['public_sha256'], 'original_private_sha256': meta['private_sha256'],
        'fix_source_sha256': {name: digest(ROOT/name) for name in ['correct_selection.py', 'grade.py', 'common.py']},
        'checker_rerun': False, 'executed_results_changed': False,
    }
    if digest(original_path) != original_hash:
        raise ValueError('original grade changed during correction')
    output = run/'supplemental-selection-v2'
    output.mkdir(exist_ok=False)
    write_json(output/'grade.json', updated)
    write_json(output/'provenance.json', provenance)
    return updated, provenance


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run-root', type=Path, required=True)
    args = parser.parse_args()
    report, provenance = correct(args.run_root)
    print(json.dumps({'run': report['run'], 'original_valid': provenance['original_valid'], **report['summary']}))
