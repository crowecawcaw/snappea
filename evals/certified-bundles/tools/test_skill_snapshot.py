"""Focused checks for the skill-treatment freeze and paired task prompts."""
from pathlib import Path
import hashlib
import json
from types import SimpleNamespace
from unittest.mock import patch
import run_sonnet
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

    def test_grading_uses_archived_inputs_and_rejects_tampering(self):
        with tempfile.TemporaryDirectory() as tmp:
            run = Path(tmp)
            for name in ('private', 'public', 'submission', 'grading'):
                (run / name).mkdir()
            fixture = run / 'private' / 'grade.py'
            fixture.write_text('original grader')
            meta = {'image': 'pinned-image', 'archived_inputs_sha256': {
                'private/grade.py': hashlib.sha256(fixture.read_bytes()).hexdigest()}}
            (run / 'metadata.json').write_text(json.dumps(meta))
            (run / 'grading' / 'grade.json').write_text(json.dumps({'passed': 1, 'total': 1}))
            with patch.object(run_sonnet, 'command', return_value=SimpleNamespace(stdout='', stderr='')) as command:
                self.assertEqual(run_sonnet.grade(run, 64)['passed'], 1)
                argv = command.call_args.args[0]
                self.assertIn(f'type=bind,src={run}/private,dst=/grader/private,readonly', argv)
                self.assertIn(f'type=bind,src={run}/public,dst=/grader/public,readonly', argv)
                fixture.write_text('changed grader')
                with self.assertRaisesRegex(ValueError, 'archived grading input changed'):
                    run_sonnet.grade(run, 64)
                self.assertEqual(command.call_count, 1)

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
