#!/usr/bin/env python3
"""POSTHOC source-informed retry eligibility probe; never changes primary scores.

The additional fault preserves rejection receipts verbatim until their expected
version becomes current. This isolates actually crossing that eligibility boundary
from merely observing a newer version in a recomputed rejection.
"""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import shutil
import sys
import tempfile

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
RUN_IDS = ('sol-retry-a01', 'sol-retry-b01', 'luna-retry-a01', 'luna-retry-b01')
ORIGIN = ('Posthoc, source-informed exploration after primary scores were frozen: '
          'the Luna retry scripts do not necessarily advance to the rejected '
          'request\'s expected version. The primary recomputation fault also changes '
          'the returned version, so detecting it does not establish eligibility-boundary coverage.')


def load_module(path: Path, name: str):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def eligibility_fault(correct: str) -> str:
    """Reconsider a rejected receipt only once its expected version is live."""
    old = '      if (p.command.id in memo) {'
    new = ('      if (p.command.id in memo && '
           '(memo[p.command.id].accepted || p.command.expected != version)) {')
    if correct.count(old) != 1:
        raise ValueError('original correct-model memo guard changed')
    return correct.replace(old, new, 1)


def verify(run: Path, common):
    meta = json.loads((run/'metadata.json').read_text())
    if meta['status'] == 'running':
        raise ValueError('probe requires a frozen submission')
    if common.digest(meta['binary']) != meta['binary_sha256']:
        raise ValueError('checker binary changed')
    for directory in ('public', 'private', 'harness'):
        if common.manifest(run/directory) != meta[directory+'_sha256']:
            raise ValueError(f'archived {directory} inputs changed: {run.name}')
    frozen = json.loads((run/'submission-manifest.json').read_text())
    if common.manifest(run/'submission') != frozen:
        raise ValueError(f'frozen submission changed: {run.name}')
    return meta


def check(run: Path, meta, common, model: str, driver: Path, tests: Path):
    with tempfile.TemporaryDirectory(prefix='retry-eligibility-posthoc-') as temp:
        work = Path(temp)
        shutil.copytree(run/'public', work, dirs_exist_ok=True)
        shutil.copy2(driver, work/'driver.p')
        shutil.copy2(tests, work/'tests.p')
        (work/'model.p').write_text(model)
        result = common.run_check(work, meta, run/'harness/cli_wrapper.py')
    result['monitor_assertion_failure'] = (
        result['status'] == 'assertion_failure' and
        any('monitor.p:' in text for text in result['assertion_bugs']))
    return result


def main():
    basis = ROOT/'runs'/RUN_IDS[0]
    common = load_module(basis/'harness/common.py', 'retry_probe_archived_common')
    base_meta = verify(basis, common)
    correct = (basis/'public/model.p').read_text()
    original = load_module(basis/'private/variants.py', 'retry_probe_original_variants')
    alternative = original.variants(correct)['correct_alt']
    fault = eligibility_fault(correct)
    models = {'correct': correct, 'correct_alt': alternative, 'posthoc_rejected_becomes_eligible': fault}
    out = ROOT/'runs/probes-retry-eligibility'
    out.mkdir(parents=True, exist_ok=True)
    report = {
        'version': 'posthoc-retry-eligibility-v1', 'origin': ORIGIN,
        'not_part_of_primary_inventory_or_scores': True,
        'contract': 'Every rejection is an immutable per-ID outcome, even if its expected version later becomes current.',
        'checker_sha256': base_meta['binary_sha256'],
        'model_sha256': {name: common.digest(basis/'public/model.p') if name == 'correct' else
                         __import__('hashlib').sha256(text.encode()).hexdigest()
                         for name, text in models.items()},
        'mutation': {'old_guard': 'p.command.id in memo',
                     'new_guard': 'p.command.id in memo && (memo[p.command.id].accepted || p.command.expected != version)',
                     'other_behavior': 'Original rejection result is returned verbatim while expected != current; accepted receipts always respected.'},
        'calibration': [], 'candidates': [],
    }
    for label, driver in [('weak', basis/'public/driver.p'),
                          ('strong', basis/'private/reference_driver.p')]:
        for model_name, model in models.items():
            result = check(basis, base_meta, common, model, driver, basis/'public/tests.p')
            report['calibration'].append({'driver': label, 'variant': model_name, **result})
            if model_name.startswith('correct') or label == 'weak':
                assert result['status'] == 'accepted', (label, model_name, result)
            else:
                assert result['monitor_assertion_failure'], (label, model_name, result)
    for run_id in RUN_IDS:
        run = ROOT/'runs'/run_id
        meta = verify(run, common)
        if (run/'public/model.p').read_text() != correct:
            raise ValueError('candidate archived correct model differs')
        results = {name: check(run, meta, common, model, run/'submission/driver.p', run/'submission/tests.p')
                   for name, model in models.items()}
        policy = common.policy_audit(run/'public', run/'submission')
        correct_accepted = all(results[name]['status'] == 'accepted' for name in ('correct', 'correct_alt'))
        valid = correct_accepted and policy['instruction_compliant']
        detected = results['posthoc_rejected_becomes_eligible']['monitor_assertion_failure']
        report['candidates'].append({
            'run': run_id, 'model': meta['model'], 'reasoning_effort': meta['reasoning_effort'],
            'reports': meta['reports'], 'source_integrity_verified': True,
            'submission_manifest_sha256': common.digest(run/'submission-manifest.json'),
            'public_model_sha256': common.digest(run/'public/model.p'),
            'driver_sha256': common.digest(run/'submission/driver.p'),
            'tests_sha256': common.digest(run/'submission/tests.p'),
            'policy': policy, 'correct_variants_accepted': correct_accepted,
            'valid_for_probe_credit': valid,
            'correct_all_search_complete': correct_accepted and all(results[n]['search_complete'] for n in ('correct', 'correct_alt')),
            'additional_fault_monitor_assertion': detected,
            'additional_fault_detected_credited': bool(valid and detected),
            'results': results,
        })
        verify(run, common)
    common.write_json(out/'results.json', report)
    summary = {key: report[key] for key in ('version', 'origin', 'not_part_of_primary_inventory_or_scores', 'contract', 'checker_sha256')}
    summary['raw_results_sha256'] = common.digest(out/'results.json')
    summary['calibration'] = [{'driver': row['driver'], 'variant': row['variant'],
                               'status': row['status'], 'search_complete': row['search_complete']}
                              for row in report['calibration']]
    summary['candidates'] = [{key: row[key] for key in (
        'run', 'model', 'reports', 'source_integrity_verified', 'driver_sha256', 'tests_sha256',
        'correct_variants_accepted', 'valid_for_probe_credit', 'correct_all_search_complete',
        'additional_fault_monitor_assertion', 'additional_fault_detected_credited')}
        for row in report['candidates']]
    common.write_json(Path(__file__).with_name('retry_eligibility_POSTHOC_summary.json'), summary)
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
