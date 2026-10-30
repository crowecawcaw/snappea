"""A correct safety prefix must not excuse two clients that never finish."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

class CompletionTests(unittest.TestCase):
    def test_missing_client_completion_is_a_liveness_failure(self):
        binary = os.environ.get('SNAPPEA_BIN') or shutil.which('snappea')
        self.assertIsNotNone(binary, 'install snappea or set SNAPPEA_BIN')
        binary = str(Path(binary).resolve())
        source = Path(__file__).with_name('Revalidated.p').read_text()
        completion = 'entry { announce ePublisherDone, this; }'
        self.assertEqual(source.count(completion), 1)
        with tempfile.TemporaryDirectory() as tmp:
            Path(tmp, 'Dropped.p').write_text(source.replace(completion, 'entry { }'))
            checked = subprocess.run(
                [binary, 'check', 'Dropped.p', '--no-mutation', '--timeout', '5s', '--json'],
                cwd=tmp, text=True, capture_output=True, timeout=30)
        self.assertEqual(checked.returncode, 1, checked.stdout + checked.stderr)
        report = json.loads(checked.stdout)
        self.assertEqual(report['summary']['status'], 'tests_failed')
        self.assertIn('liveness bug in hot state', report['test_cases'][0]['bug']['report'])

if __name__ == '__main__':
    unittest.main()
