import importlib.util
import math
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('bench_instructions', Path(__file__).with_name('bench-instructions.py'))
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)


def row(seed, instructions, **changes):
    base = dict(case='c', seed=seed, schedules=10, settled=instructions is not None, instructions=instructions)
    base.update(changes)
    return base


class InstructionMetricsTests(unittest.TestCase):
    def test_bug_case_settles_when_every_site_failed(self):
        self.assertTrue(bench.settled(dict(expected=['a'], schedules_to_all=7, exhausted=False, first={'a': 7})))
        self.assertFalse(bench.settled(dict(expected=['a', 'b'], schedules_to_all=None, exhausted=True, first={'a': 7})))

    def test_safe_case_settles_only_by_exhaustion_without_failures(self):
        self.assertTrue(bench.settled(dict(expected=[], schedules_to_all=None, exhausted=True, first={})))
        self.assertFalse(bench.settled(dict(expected=[], schedules_to_all=None, exhausted=False, first={})))
        self.assertFalse(bench.settled(dict(expected=[], schedules_to_all=None, exhausted=True, first={'x': 1})))

    def test_p90_keeps_unsettled_runs(self):
        nine = [row(i, 100 + i) for i in range(9)] + [row(9, None)]
        self.assertEqual(bench.summaries(nine)[0]['p90'], 108)
        eight = [row(i, 100 + i) for i in range(8)] + [row(8, None), row(9, None)]
        result = bench.summaries(eight)[0]
        self.assertIsNone(result['p90'])
        self.assertEqual(result['settled'], 8)

    def test_aggregate_counts_censored_cases_apart(self):
        total = bench.aggregate([dict(p90=100), dict(p90=10000), dict(p90=None)])
        self.assertTrue(math.isclose(total['geomean_p90'], 1000))
        self.assertEqual((total['total_p90'], total['censored'], total['cases']), (10100, 1, 3))

    def test_sample_cases_name_existing_tests(self):
        cases = bench.load_cases([bench.ROOT / 'benchmarks/checker/samples.json'], '')
        self.assertTrue(cases)
        for case in cases:
            text = '\n'.join(source for _, source in case['sources'])
            self.assertIn(f"test {case['test']} ", text, case['name'])
            for expected in case['expected']:
                self.assertIn(expected['match'], text, case['name'])


if __name__ == '__main__':
    unittest.main()
