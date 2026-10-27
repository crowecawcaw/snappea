"""CLI comparisons preserve the native Python backend and frozen trial inputs."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace

import run_sonnet
import tool_variant

PHASES = Path(__file__).resolve().parents[2] / 'modeling-skill/phases'
spec = importlib.util.spec_from_file_location('phase_runner', PHASES/'runner.py')
phase_runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(phase_runner)


class ToolVariantTests(unittest.TestCase):
    def test_both_container_builders_separate_selected_cli_and_native_python_backend(self):
        native = Path('/tools/snappea')
        for tool in tool_variant.TOOLS:
            with self.subTest(tool=tool), patch.object(run_sonnet, 'tool_paths', return_value=(native, Path('/source'))):
                builders = [
                    phase_runner.container_args(Path('/work'), binary=native, tool=tool),
                    run_sonnet.container_args('run', Path('/work'), 'p', 'baseline', tool=tool),
                ]
                for argv in builders:
                    mounts = [v for v in argv if v.startswith('type=bind')]
                    self.assertIn(f'type=bind,src=/tools/{tool},dst=/opt/{tool},readonly', mounts)
                    self.assertIn('type=bind,src=/tools/snappea,dst=/opt/python-snappea,readonly', mounts)
                    self.assertIn('SNAPPEA_BIN=/opt/python-snappea', argv)
                    self.assertNotIn('SNAPPEA_BIN=/opt/snappea', argv)
                    other = 'snappea-compat' if tool == 'snappea' else 'snappea'
                    self.assertFalse(any(f'dst=/opt/{other},' in v for v in mounts))
                    self.assertFalse(any('p-modeling' in v for v in mounts))

    def test_tool_freeze_rejects_each_binary_changed(self):
        with tempfile.TemporaryDirectory() as tmp:
            native = Path(tmp)/'snappea'; native.write_text('native')
            compat = Path(tmp)/'snappea-compat'; compat.write_text('compat')
            meta = tool_variant.metadata(native, 'snappea-compat')
            tool_variant.verify(meta)
            compat.write_text('changed')
            with self.assertRaisesRegex(ValueError, 'cli_binary'):
                tool_variant.verify(meta)
            compat.write_text('compat')
            native.write_text('changed')
            with self.assertRaisesRegex(ValueError, 'python_backend_binary'):
                tool_variant.verify(meta)

    def test_old_phase_container_default_unchanged(self):
        argv = phase_runner.container_args(Path('/work'), binary=Path('/tools/snappea'))
        self.assertIn('SNAPPEA_BIN=/opt/snappea', argv)
        self.assertFalse(any('python-snappea' in v for v in argv))

    def test_phase_provisioning_freezes_adapted_public_task_and_both_binary_hashes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            public = root/'flow/public'
            public.mkdir(parents=True)
            (public/'TASK.md').write_text('snappea check events.p monitor.p model.p tests.p\n')
            (root/'flow/private').mkdir()
            (root/'flow/private/grade.py').write_text('grader')
            for name in ('runner.py', 'bridge.py', 'grade.py', 'grade_novel.py'):
                (root/name).write_text(name)
            native = root/'snappea'; native.write_text('native')
            compat = root/'snappea-compat'; compat.write_text('compat')
            args = SimpleNamespace(run_id='test', phase='flow', binary=native, tool='snappea-compat',
                                   tool_binary=None, condition='baseline', model='gpt-6-luna', reasoning_effort='high')
            ok = SimpleNamespace(returncode=0, stdout='image-id\n', stderr='')
            with patch.object(phase_runner, 'ROOT', root), patch.object(phase_runner, 'command', return_value=ok):
                phase_runner.start(args)
            import json
            run = root/'runs/test'
            meta = json.loads((run/'metadata.json').read_text())
            task = (run/'public/TASK.md').read_text()
            self.assertIn('snappea-compat check --pfiles events.p monitor.p model.p tests.p', task)
            self.assertEqual(task, (run/'work/TASK.md').read_text())
            self.assertEqual(meta['public_sha256'], phase_runner.manifest(run/'public'))
            self.assertEqual(meta['original_public_sha256'], phase_runner.manifest(public))
            self.assertEqual(meta['binary'], str(native))
            self.assertEqual(meta['python_backend_sha256'], phase_runner.digest(native))
            self.assertEqual(meta['cli_binary_sha256'], phase_runner.digest(compat))
            self.assertIsNone(meta['skill_sha256'])
            self.assertIn('tool_variant.py', meta['harness_sha256'])
            prompt = (run/'prompt.txt').read_text()
            self.assertNotIn('SKILL.md', prompt)
            self.assertIn('Use snappea-compat', prompt)


if __name__ == '__main__':
    unittest.main()
