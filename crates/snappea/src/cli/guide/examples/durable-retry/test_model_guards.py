"""Negative control: check public reply data, not a parallel service audit."""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

class ResponseIntegrityTests(unittest.TestCase):
    def test_wrong_actual_reply_result_is_rejected_by_each_workload(self):
        binary = os.environ.get('SNAPPEA_BIN') or shutil.which('snappea')
        self.assertIsNotNone(binary, 'install snappea or set SNAPPEA_BIN')
        binary = str(Path(binary).resolve())
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp) / 'example'
            shutil.copytree(Path(__file__).parent, work,
                            ignore=shutil.ignore_patterns('__pycache__', 'PCheckerOutput'))
            source = work / 'DurableRetry.p'
            before, after = source.read_text().split('machine SplitCommitService {', 1)
            # Corrupt only public reply payloads. Commits/memoized effects remain
            # correct, so rejection must come from observing the actual response.
            before, count = re.subn(
                r'(send call.client, eReply,\s*\(id = call.id, result = )[^,]+',
                r'\g<1>999', before)
            self.assertEqual(count, 2)
            source.write_text(before + 'machine SplitCommitService {' + after)
            checked = subprocess.run(
                [binary, 'check', 'tcDurableRetry', 'tcCompletedReplay',
                 'tcConcurrentRetries', '--no-mutation', '--timeout', '5s', '--json'],
                cwd=work, text=True, capture_output=True, timeout=30)
        self.assertEqual(checked.returncode, 1, checked.stdout + checked.stderr)
        report = json.loads(checked.stdout)
        self.assertEqual(len(report['test_cases']), 3)
        for case in report['test_cases']:
            self.assertEqual(case['result'], 'bug')
            self.assertIn('replayed result 999', case['bug']['report'])

if __name__ == '__main__':
    unittest.main()
