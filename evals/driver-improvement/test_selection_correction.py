"""Selection-only regrading must preserve original execution and false-positive gates."""
from copy import deepcopy
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

from common import classify, digest, manifest, policy_audit, write_json
from correct_selection import correct, summary_with_selection

ROOT = Path(__file__).resolve().parent


def executed(name, status, correct_variant=False):
    if status == 'accepted':
        payload = {'test_cases': [{'name': 'T', 'result': 'ok', 'search_complete': True}]}
        code, timeout = 0, False
    elif status == 'assertion_failure':
        payload = {'test_cases': [{'name': 'T', 'result': 'bug', 'bug': {'report': 'Assertion Failed: monitor.p:2:3 mismatch'}}]}
        code, timeout = 1, False
    elif status == 'other_bug':
        payload = {'test_cases': [{'name': 'T', 'result': 'bug', 'bug': {'report': 'Monitor remains hot'}}]}
        code, timeout = 1, False
    else:
        payload = {'error': 'syntax error'}
        code, timeout = 2, status == 'timeout'
    result = {'variant': name, 'expected_correct': correct_variant, 'exit_code': code,
              'stdout': json.dumps(payload), 'stderr': '', 'timeout': timeout, 'seconds': 0}
    result.update(classify(result))
    return result


def original_report(run='trial'):
    results = [executed('correct', 'accepted', True), executed('correct_alt', 'accepted', True),
               executed('known_regression', 'assertion_failure'), executed('new_fault', 'assertion_failure'),
               executed('setup_error', 'infrastructure_error'), executed('hot_failure', 'other_bug')]
    baseline = [executed(r['variant'], 'assertion_failure' if r['variant'] == 'known_regression' else 'accepted', r['expected_correct'])
                for r in results]
    report = {'version': 'driver-improvement-v1', 'run': run,
              'summary': {'static_instruction_checks_passed': True, 'instruction_review_required': True,
                          'runtime_instruction_mentions': 0},
              'policy': {'instruction_compliant': True}, 'selected_specs': {'Contract': False},
              'results': results, 'baseline_results': baseline}
    report['summary'] = summary_with_selection(report, report['selected_specs'])
    return report


class SelectionCorrectionTests(unittest.TestCase):
    def test_correction_credits_only_assertions_and_preserves_baseline(self):
        original = original_report()
        frozen = deepcopy(original)
        corrected = summary_with_selection(original, {'Contract': True})
        self.assertTrue(corrected['valid'])
        self.assertEqual(corrected['faults_detected'], 2)
        self.assertEqual(corrected['faults_total'], 4)
        self.assertEqual(corrected['new_faults_detected'], 1)
        self.assertEqual(corrected['baseline_faults_detected'], 1)
        self.assertEqual(corrected['baseline_faults_lost'], 0)
        self.assertEqual(corrected['infrastructure_errors'], 1)
        self.assertEqual(original, frozen)

    def test_selection_fix_does_not_override_invalid_policy_or_correct_variant(self):
        for reason in ['policy', 'correct_rejected']:
            report = original_report()
            if reason == 'policy':
                report['policy']['instruction_compliant'] = False
            else:
                report['results'][0] = executed('correct', 'assertion_failure', True)
            corrected = summary_with_selection(report, {'Contract': True})
            self.assertFalse(corrected['valid'])
            self.assertIsNone(corrected['faults_detected'])
            self.assertIsNone(corrected['new_faults_detected'])
            self.assertEqual(corrected['fault_assertion_failures_uncredited'], 2)

    def test_incomplete_correct_results_remain_qualified(self):
        report = original_report()
        report['results'][0]['search_complete'] = False
        corrected = summary_with_selection(report, {'Contract': True})
        self.assertTrue(corrected['valid'])
        self.assertEqual(corrected['correct_incomplete'], 1)
        self.assertEqual(corrected['correct_search_complete_count'], 1)
        self.assertEqual(corrected['score_qualification'], 'incomplete_correct_search')

    def test_supplement_verifies_archives_and_preserves_original_report_without_checker(self):
        with tempfile.TemporaryDirectory() as temp:
            run = Path(temp)/'trial'; run.mkdir()
            for name in ['public', 'private', 'harness', 'submission', 'grading']:
                (run/name).mkdir()
            contents = {'model.p': 'correct algorithm', 'monitor.p': 'spec Contract observes eDone {}',
                        'driver.p': 'machine Main {}', 'tests.p': 'test T [main=Main]: assert Contract in M;'}
            for name, text in contents.items():
                (run/'public'/name).write_text(text)
                (run/'submission'/name).write_text(text)
            (run/'submission/driver.p').write_text(contents['driver.p']+'\n'+contents['tests.p'])
            (run/'submission/tests.p').write_text('// Declaration is in driver.p.')
            names = ['correct', 'correct_alt', 'known_regression', 'new_fault', 'setup_error', 'hot_failure']
            (run/'private/variants.py').write_text('def variants(source):\n    return '+repr({n: 'correct algorithm' if n == 'correct' else n for n in names})+'\n')
            shutil.copy2(ROOT/'common.py', run/'harness/common.py')
            (run/'harness/grade.py').write_text('''
import importlib.util
from pathlib import Path
import re
from common import p_code

def load_variants(path):
    spec = importlib.util.spec_from_file_location('archived_variants', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

def selected_monitors(public, submission):
    return {'Contract': bool(re.search(r'\\bassert\\s+Contract\\b', p_code((Path(submission)/'tests.p').read_text())))}
''')
            binary = Path(temp)/'binary'; binary.write_text('pinned binary')
            meta = {'status': 'agent_finished', 'binary': str(binary), 'binary_sha256': digest(binary), 'image': 'test-image',
                    **{n+'_sha256': manifest(run/n) for n in ['public', 'private', 'harness']}}
            write_json(run/'metadata.json', meta)
            write_json(run/'submission-manifest.json', manifest(run/'submission'))
            report = original_report(run.name)
            report['policy'] = policy_audit(run/'public', run/'submission')
            write_json(run/'grading/grade.json', report)
            original_hash = digest(run/'grading/grade.json')
            with patch('subprocess.run', side_effect=AssertionError('checker must not run')):
                corrected, provenance = correct(run)
            self.assertTrue(corrected['summary']['valid'])
            self.assertEqual(corrected['summary']['faults_detected'], 2)
            self.assertEqual(corrected['results'], report['results'])
            self.assertEqual(corrected['baseline_results'], report['baseline_results'])
            self.assertEqual(digest(run/'grading/grade.json'), original_hash)
            self.assertEqual(provenance['original_grade_sha256'], original_hash)
            self.assertFalse(provenance['checker_rerun'])
            self.assertTrue((run/'supplemental-selection-v2/grade.json').is_file())
            self.assertFalse(list((run/'harness').rglob('__pycache__')))
            self.assertFalse(list((run/'private').rglob('__pycache__')))
            # Stored execution fields cannot disagree with their original native output.
            report['results'][0]['status'] = 'assertion_failure'
            write_json(run/'grading/grade.json', report)
            with self.assertRaisesRegex(ValueError, 'classification mismatch'):
                correct(run)


if __name__ == '__main__':
    unittest.main()
