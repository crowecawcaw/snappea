import importlib.util
import math
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('bench_checker',Path(__file__).with_name('bench-checker.py'))
bench=importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)

class CheckerMetricsTests(unittest.TestCase):
    def rows(self,misses):
        return [dict(case='mixed',strategy='auto',expected=['easy','hard'],first={'easy':1} if i<misses else {'easy':1,'hard':i+1},schedules_to_all=None if i<misses else i+1) for i in range(10)]
    def test_p90_includes_missed_bugs(self):
        result=bench.summaries(self.rows(2),100)[0]
        self.assertEqual(result['completed'],8)
        self.assertIsNone(result['p90'])
        self.assertAlmostEqual(result['recall'],0.9)
        self.assertGreater(result['par2'],40)
    def test_exact_ninety_percent_completion_has_finite_p90(self):
        result=bench.summaries(self.rows(1),100)[0]
        self.assertEqual(result['completed'],9)
        self.assertEqual(result['p90'],10)
    def test_all_misses_do_not_look_like_zero_cost(self):
        result=bench.summaries(self.rows(10),100)[0]
        self.assertIsNone(result['p90'])
        self.assertEqual(result['par2'],200)
        self.assertEqual(result['recall'],0.5)
    def test_safe_cases_are_not_free_bug_discoveries(self):
        self.assertEqual(bench.summaries([dict(case='safe',strategy='auto',expected=[])],100),[])
    def test_nearest_rank_quantile(self):
        self.assertEqual(bench.percentile([1,2,3,4,math.inf]),math.inf)


class CheckerGateTests(unittest.TestCase):
    def row(self, **changes):
        row = dict(case='mixed', strategy='auto', seed=0, expected=['easy', 'hard'],
                   first={'easy': 1, 'hard': 10}, time_censored=False, exhausted=False)
        row.update(changes)
        return row

    def test_one_missed_site_fails_even_with_high_recall(self):
        failures = bench.check_failures([self.row(first={'easy': 1})], [])
        self.assertEqual(len(failures), 1)
        self.assertIn('missing hard', failures[0])

    def test_success_requires_every_site_without_a_time_cap(self):
        self.assertEqual(bench.check_failures([self.row()], []), [])
        self.assertTrue(bench.check_failures([self.row(time_censored=True)], []))

    def test_safe_controls_check_exhaustion_and_unexpected_bugs(self):
        controls = [dict(name='mixed', expect_exhausted=False)]
        safe = self.row(expected=[], first={})
        self.assertEqual(bench.check_failures([safe], controls), [])
        self.assertTrue(bench.check_failures([dict(safe, exhausted=True)], controls))
        self.assertTrue(bench.check_failures([dict(safe, first={'unexpected': 1})], controls))


if __name__ == "__main__":
    unittest.main()
