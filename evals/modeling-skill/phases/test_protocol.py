"""Guard against false eval credit and grading changed artifacts."""
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import subprocess
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parent
sys.path.insert(0,str(ROOT))
import grade
from runner import manifest,digest
sys.path.insert(0,str(ROOT/'testing/private'))
spec=importlib.util.spec_from_file_location('testing_grader',ROOT/'testing/private/grade.py')
testing=importlib.util.module_from_spec(spec)
spec.loader.exec_module(testing)


class ProtocolTests(unittest.TestCase):
    def test_compile_errors_and_empty_suites_are_not_mutation_detection(self):
        for output in ['Ran 0 tests\nOK',
                       'ImportError: missing module\nRan 1 test\nFAILED (errors=1)',
                       'SnappeaCompilationError\nRan 1 test\nFAILED (errors=1)']:
            self.assertEqual(testing.classify(1,output,False)['status'],'infrastructure_error')
        self.assertEqual(testing.classify(1,'Ran 2 tests\nFAILED (failures=1)',False)['status'],'tests_rejected')
        self.assertEqual(testing.classify(None,'',True)['status'],'timeout')

    def test_reference_incompatibility_does_not_count_as_fault_detection(self):
        # Separate interpreter avoids the two graders' local calibration modules
        # sharing a module name in the parent test process.
        code = """
from pathlib import Path
from unittest.mock import patch
import grade
for status in ['infrastructure_error','tests_rejected','timeout']:
    with patch.object(grade,'run_candidate_suite',return_value={'status':status,'test_count':1}) as run:
        result=grade.run_portable_probes(Path('.'),Path('/unused'),'unused',1)
        assert result['portable_probe_valid'] is False
        assert result['mutation_probes_run'] is False
        assert 'mutations_rejected' not in result
        assert run.call_count == 1
"""
        result=subprocess.run([sys.executable,'-c',code],cwd=ROOT/'flow/private',
                              capture_output=True,text=True,timeout=10)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)

    def test_novel_scenario_setup_errors_are_not_fault_detection(self):
        import grade_novel
        for output in ['Ran 0 tests\nOK',
                       'SetupError: no events\nRan 1 test\nFAILED (errors=1)',
                       'SnappeaCompilationError\nRan 1 test\nFAILED (errors=1)']:
            result={'exit_code':1,'stdout':output,'stderr':'','timeout':False}
            self.assertEqual(grade_novel.classify(result)['status'],'infrastructure_error')
        result={'exit_code':1,'stdout':'Ran 2 tests\nFAILED (failures=1)','stderr':'','timeout':False}
        self.assertEqual(grade_novel.classify(result)['status'],'tests_rejected')

    def test_monitor_shared_types_use_project_sources_without_counting_compile_errors(self):
        import grade_novel
        with tempfile.TemporaryDirectory() as tmp:
            run=Path(tmp)
            for folder in ['submission','public','private']:(run/folder).mkdir()
            for name in ['model.p','monitor.p','tests.p','DESIGN.md']:
                (run/'submission'/name).write_text('fixture')
            (run/'public/events.p').write_text('fixture')
            (run/'private/cases.py').write_text(
                "def cases():\n return [('good',True,[{'event':'good'}]),('bad',False,[{'event':'bad'}])]\n")
            def execute(work,meta,args,timeout=60):
                if args[1]=='check':
                    payload={'test_cases':[{'schedules':1,'search_complete':True}]}
                    return {'exit_code':0,'stdout':json.dumps(payload),'stderr':'','timeout':False}
                if 'model.p' not in args:
                    return {'exit_code':2,'stdout':'{}','stderr':'missing shared type','timeout':False}
                bad='bad' in (work/'history.jsonl').read_text()
                payload={'specs':[{'errors':[{'kind':'spec_violation'}] if bad else []}]}
                return {'exit_code':int(bad),'stdout':json.dumps(payload),'stderr':'','timeout':False}
            with patch.object(grade_novel,'execute',side_effect=execute):
                report=grade_novel.model_grade(run,{}, {'spec':'Contract'},run)
            self.assertEqual(report['summary']['valid_accepted'],1)
            self.assertEqual(report['summary']['invalid_rejected'],1)
            self.assertIn('model.p',report['monitor_source_files'])

    def test_grading_rejects_changed_submission_inputs_binary_or_grader(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);run=root/'runs/trial';run.mkdir(parents=True)
            for d in ['submission','public']:
                (run/d).mkdir()
                (run/d/'TASK.md').write_text('original')
            phase=root/'testing'
            for d in ['public','private']:
                (phase/d).mkdir(parents=True)
            (phase/'public/TASK.md').write_text('original')
            (phase/'private/grade.py').write_text('original grader')
            binary=root/'binary';binary.write_text('binary')
            meta={'status':'agent_finished','phase':'testing','binary':str(binary),
                  'binary_sha256':digest(binary),'public_sha256':manifest(phase/'public'),
                  'grader_sha256':manifest(phase/'private')}
            (run/'metadata.json').write_text(json.dumps(meta))
            (run/'submission-manifest.json').write_text(json.dumps(manifest(run/'submission')))
            with patch.object(grade,'ROOT',root):
                grade.verify(run)
                for path in [run/'submission/TASK.md',run/'public/TASK.md',
                             phase/'public/TASK.md',phase/'private/grade.py',binary]:
                    original=path.read_text();path.write_text('changed')
                    with self.subTest(path=path),self.assertRaises(ValueError):
                        grade.verify(run)
                    path.write_text(original)


if __name__=='__main__':
    unittest.main()
