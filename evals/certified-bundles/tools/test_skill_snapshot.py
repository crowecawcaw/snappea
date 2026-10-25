"""Focused checks for the skill-treatment freeze and paired task prompts."""
from pathlib import Path
import tempfile
import unittest

from prompts import task_prompt
from skill_snapshot import create_snapshot, verify_snapshot


def make_writable(tree):
    for path in tree.rglob('*'):
        path.chmod(0o755 if path.is_dir() else 0o644)
    tree.chmod(0o755)


class SkillSnapshotTests(unittest.TestCase):
    def test_snapshot_contains_skill_and_examples_but_skips_generated_artifacts(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / 'source'
            (source / 'examples' / 'model').mkdir(parents=True)
            (source / 'guides').mkdir()
            (source / 'examples' / 'model' / 'Model.p').write_text('model')
            (source / 'guides' / 'use.md').write_text('guide')
            (source / 'SKILL.md').write_text('skill')
            (source / 'PLAN.md').write_text('private eval planning')
            (source / 'snapshot.json').write_text('stale metadata')
            generated = source / 'examples' / 'model' / 'PCheckerOutput' / 'trace.txt'
            generated.parent.mkdir()
            generated.write_text('generated output')
            coverage = source / 'coverage' / 'index.html'
            coverage.parent.mkdir()
            coverage.write_text('generated coverage')
            cache = source / 'examples' / 'model' / '__pycache__' / 'x.pyc'
            cache.parent.mkdir()
            cache.write_bytes(b'compiled')
            (source / 'examples' / 'model' / 'coverage.log').write_text('generated log')
            (source / '.coverage').write_text('generated coverage data')

            snapshot_root = Path(tmp) / 'snapshots'
            snapshot, manifest = create_snapshot(source, snapshot_root)
            actual = {p.relative_to(snapshot).as_posix() for p in snapshot.rglob('*') if p.is_file()}
            self.assertEqual(actual, {'SKILL.md', 'guides/use.md', 'examples/model/Model.p', 'snapshot.json'})
            self.assertNotIn('PLAN.md', manifest['files'])
            self.assertEqual(verify_snapshot(snapshot)['tree_sha256'], manifest['tree_sha256'])
            make_writable(snapshot_root)

    def test_snapshot_detects_file_changes(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / 'source'
            (source / 'examples').mkdir(parents=True)
            (source / 'SKILL.md').write_text('skill')
            (source / 'examples' / 'sample.p').write_text('model')
            snapshot_root = Path(tmp) / 'snapshots'
            snapshot, _ = create_snapshot(source, snapshot_root)
            target = snapshot / 'examples' / 'sample.p'
            target.chmod(0o644)
            target.write_text('changed')
            with self.assertRaisesRegex(ValueError, 'hash mismatch'):
                verify_snapshot(snapshot)
            make_writable(snapshot_root)

    def test_paired_prompts_differ_only_by_skill_instruction(self):
        baseline = task_prompt('p', 'baseline')
        treatment = task_prompt('p', 'skill')
        instruction = 'Use the P modeling skill at /opt/p-modeling/SKILL.md.\n'
        self.assertEqual(treatment, baseline.replace('\n\nDeliver solution.py', f'\n{instruction}\nDeliver solution.py'))
        self.assertIn('Read TASK.md and contract.py.', baseline)
        self.assertIn('Deliver solution.py and DESIGN.md.', baseline)
        self.assertNotIn('testing', baseline.lower())


if __name__ == '__main__':
    unittest.main()
