"""Benchmark storage keeps individual results intact without one growing corpus file."""
import gzip
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location('bench_storage', ROOT / '.github/scripts/store-bench-result.py')
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class StorageTests(unittest.TestCase):
    def test_snapshot_is_complete_compressed_and_retry_stable(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'result.json'
            result = {'commit': 'abc', 'cases': [{'case': 'many', 'runs': list(range(1000))}]}
            source.write_text(json.dumps(result, indent=2))
            destination = root / 'corpus/abc/12-1.json.gz'
            MODULE.store('snapshot', source, destination)
            first = destination.read_bytes()
            self.assertEqual(json.loads(gzip.decompress(first)), result)
            MODULE.store('snapshot', source, destination)
            self.assertEqual(destination.read_bytes(), first)
            self.assertLess(len(first), source.stat().st_size)

    def test_snapshot_push_preserves_old_history_and_other_runs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            remote = root / 'remote.git'
            subprocess.run(['git', 'init', '--bare', str(remote)], check=True, capture_output=True)
            checkout = root / 'checkout'
            subprocess.run(['git', 'clone', str(remote), str(checkout)], check=True, capture_output=True)
            subprocess.run(['git', '-C', str(checkout), 'config', 'user.email', 'test@example.com'], check=True)
            subprocess.run(['git', '-C', str(checkout), 'config', 'user.name', 'Test'], check=True)
            source = root / 'result.json'
            script = ROOT / '.github/scripts/push-bench-data.sh'
            def push(mode, destination, value):
                source.write_text(json.dumps(value))
                subprocess.run(['bash', str(script), mode, str(source), destination, 'test result'],
                               cwd=checkout, check=True, capture_output=True)
            push('append', 'corpus.jsonl', {'old': 1})
            push('snapshot', 'corpus/abc/1-1.json.gz', {'run': 1})
            push('snapshot', 'corpus/abc/1-2.json.gz', {'run': 2})
            push('snapshot', 'corpus/abc/1-2.json.gz', {'run': 2})
            def contents(path):
                return subprocess.check_output(['git', '--git-dir', str(remote), 'show', f'benchmarks:{path}'])
            self.assertEqual(json.loads(contents('corpus.jsonl')), {'old': 1})
            for attempt in (1, 2):
                self.assertEqual(json.loads(gzip.decompress(contents(f'corpus/abc/1-{attempt}.json.gz'))), {'run': attempt})


if __name__ == '__main__':
    unittest.main()
