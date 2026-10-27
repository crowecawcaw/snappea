"""Meaningful grading, freeze and bridge regressions; no solver/model API needed."""
import importlib.util
import json
import io
import os
import runpy
from pathlib import Path
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch, mock_open

from common import classify, command, digest, manifest, policy_audit, write_json, protected_input_drift
from grade import verify, selected_monitors


class HarnessTests(unittest.TestCase):
    def test_only_real_assertions_are_fault_detection(self):
        def result(report, code=0):
            return {'exit_code': code, 'stdout': json.dumps(report), 'stderr': '', 'timeout': False}
        self.assertEqual(classify(result({'error': 'Syntax error'}, 2))['status'], 'infrastructure_error')
        self.assertEqual(classify(result({'test_cases': []}))['status'], 'infrastructure_error')
        self.assertEqual(classify(result({'test_cases': [{'result': 'ok', 'search_complete': False}]}))['status'], 'accepted')
        bug = {'test_cases': [{'result': 'bug', 'bug': {'report': 'Assertion Failed: monitor.p:4:3 bad result'}}]}
        self.assertEqual(classify(result(bug, 1))['status'], 'assertion_failure')
        bug['test_cases'][0]['bug']['report'] = 'Monitor remains hot at termination'
        self.assertEqual(classify(result(bug, 1))['status'], 'other_bug')
        self.assertEqual(classify({**result(bug, 1), 'timeout': True})['status'], 'timeout')

    def test_binary_stdout_does_not_abort_bridge(self):
        r = command([sys.executable, '-c', "import sys; sys.stdout.buffer.write(b'\\xff')"])
        self.assertEqual(r.returncode, 0)
        self.assertEqual(r.stdout, '\ufffd')

    def test_immutable_changes_and_suppressions_are_reported(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            public, submission = root/'public', root/'submission'
            public.mkdir(); submission.mkdir()
            for name in ['model.p', 'monitor.p', 'driver.p', 'tests.p']:
                (public/name).write_text(name)
                (submission/name).write_text(name)
            self.assertTrue(policy_audit(public, submission)['instruction_compliant'])
            (submission/'monitor.p').write_text('assert true;')
            (submission/'driver.p').write_text('// snappea-lint: allow coverage -- skip\n')
            report = policy_audit(public, submission)
            self.assertFalse(report['instruction_compliant'])
            self.assertEqual(report['immutable_inputs_changed'], ['monitor.p'])
            self.assertEqual(len(report['new_suppressions']), 1)

    def test_assume_mentions_in_comments_and_strings_are_not_policy_failures(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); public = root/'public'; sub = root/'submission'
            public.mkdir(); sub.mkdir()
            for name in ['driver.p', 'tests.p']:
                (public/name).write_text('')
                (sub/name).write_text('')
            path = sub/'driver.p'
            path.write_text('// assume values are bounded\n/* assume true; */\nprint "assume false";')
            self.assertEqual(policy_audit(public, sub)['new_assumptions'], [])
            path.write_text('// assume values are bounded\nassume false;')
            self.assertEqual(len(policy_audit(public, sub)['new_assumptions']), 1)

    def test_required_monitor_selection_ignores_comments_and_strings(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); public = root/'public'; sub = root/'submission'
            public.mkdir(); sub.mkdir()
            (public/'monitor.p').write_text('spec Contract observes eDone {}')
            for text, expected in [
                ('// assert Contract in M;\ntest T: M;', False),
                ('/* assert Contract in M */ test T: M;', False),
                ('print "assert Contract";', False),
                ('test T [main=Main]: assert Contract in M;', True),
            ]:
                (sub/'tests.p').write_text(text)
                self.assertEqual(selected_monitors(public, sub), {'Contract': expected})

    def test_monitor_selection_accepts_declaration_moved_to_driver(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); public = root/'public'; sub = root/'submission'
            public.mkdir(); sub.mkdir()
            (public/'monitor.p').write_text('spec Contract observes eDone {}')
            (sub/'tests.p').write_text('// Test declaration moved to driver.p.')
            driver = sub/'driver.p'
            driver.write_text('machine Main {}\ntest T [main=Main]: assert Contract in M;')
            self.assertEqual(selected_monitors(public, sub), {'Contract': True})
            driver.write_text('// assert Contract in M;\nprint "assert Contract";')
            self.assertEqual(selected_monitors(public, sub), {'Contract': False})

    def test_wrapper_collects_identically_and_filters_stdout_and_sidecars(self):
        wrapper = Path(__file__).with_name('cli_wrapper.py')
        original = {'test_cases': [{'name': 'T', 'driver': {'facts': ['fixed input']}, 'result': 'ok'}], 'exit_code': 0}
        effective = []
        for arm in ['on', 'off']:
            stdout = io.StringIO()
            native = SimpleNamespace(returncode=0, stdout=json.dumps(original), stderr='')
            with patch.object(sys, 'argv', ['snappea', 'check', 'model.p', '--json-report', 'report.json']), \
                 patch.dict(os.environ, {'DRIVER_REPORTS': arm}), \
                 patch('subprocess.run', return_value=native) as child, \
                 patch.object(sys, 'stdout', stdout), patch.object(sys, 'stderr', io.StringIO()), \
                 patch('pathlib.Path.write_text') as sidecar, patch('builtins.open', mock_open()):
                with self.assertRaises(SystemExit) as stopped:
                    runpy.run_path(str(wrapper), run_name='__main__')
                self.assertEqual(stopped.exception.code, 0)
                effective.append(child.call_args.args[0])
                output = json.loads(stdout.getvalue())
                self.assertEqual('driver' in output['test_cases'][0], arm == 'on')
                self.assertEqual(json.loads(sidecar.call_args.args[0]), output)
        self.assertEqual(effective[0], effective[1])
        self.assertNotIn('--no-driver-facts', effective[0])
        self.assertNotIn('--json-report', effective[0])

    def test_wrapper_preserves_parse_errors_and_plain_help(self):
        wrapper = Path(__file__).with_name('cli_wrapper.py')
        cases = [
            (['check', 'broken.p'], 2, json.dumps({'error': 'syntax error', 'exit_code': 2})),
            (['check', '--help'], 1, 'Usage: snappea check\n'),
        ]
        for args, code, text in cases:
            with self.subTest(args=args):
                stdout = io.StringIO()
                native = SimpleNamespace(returncode=code, stdout=text, stderr='original diagnostic')
                with patch.object(sys, 'argv', ['snappea', *args]), \
                     patch.dict(os.environ, {'DRIVER_REPORTS': 'off'}), \
                     patch('subprocess.run', return_value=native), \
                     patch.object(sys, 'stdout', stdout), patch.object(sys, 'stderr', io.StringIO()) as stderr, \
                     patch('builtins.open', mock_open()):
                    with self.assertRaises(SystemExit) as stopped:
                        runpy.run_path(str(wrapper), run_name='__main__')
                    self.assertEqual(stopped.exception.code, code)
                    if args[-1] == '--help':
                        self.assertEqual(stdout.getvalue(), text)
                    else:
                        self.assertEqual(json.loads(stdout.getvalue()), json.loads(text))
                    self.assertEqual(stderr.getvalue(), 'original diagnostic')

    def test_protected_drift_catches_temporary_changes_and_ignores_driver(self):
        with tempfile.TemporaryDirectory() as temp:
            work = Path(temp)
            (work/'model.p').write_text('model')
            (work/'driver.p').write_text('driver')
            expected = manifest(work)
            (work/'driver.p').write_text('new tests')
            self.assertEqual(protected_input_drift(work, expected), {})
            (work/'model.p').write_text('assert true;')
            drift = protected_input_drift(work, expected)
            self.assertIn('model.p', drift)
            self.assertNotEqual(drift['model.p']['actual'], expected['model.p'])
            (work/'model.p').unlink()
            (work/'model.p').symlink_to('/etc/passwd')
            self.assertEqual(protected_input_drift(work, expected)['model.p']['kind'], 'symlink')
            self.assertIsNone(protected_input_drift(work, expected)['model.p']['actual'])

    def test_archive_verification_is_independent_of_live_fixture(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); run = root/'run'; run.mkdir()
            for name in ['public', 'private', 'harness', 'submission']:
                (run/name).mkdir()
                (run/name/'file').write_text(name)
            binary = root/'snappea'; binary.write_text('pinned')
            meta = {'status': 'agent_finished', 'binary': str(binary), 'binary_sha256': digest(binary),
                    **{n+'_sha256': manifest(run/n) for n in ['public', 'private', 'harness']}}
            write_json(run/'metadata.json', meta)
            write_json(run/'submission-manifest.json', manifest(run/'submission'))
            verify(run)
            # No live fixture or live harness is consulted.
            (root/'live.py').write_text('changed live code')
            verify(run)
            for path in [run/'private/file', run/'submission/file', binary]:
                old = path.read_text(); path.write_text('tampered')
                with self.subTest(path=path), self.assertRaises(ValueError):
                    verify(run)
                path.write_text(old)


if __name__ == '__main__':
    unittest.main()
