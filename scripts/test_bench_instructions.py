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

    def test_cases_are_compared_one_by_one(self):
        before = [dict(case='small', p90=100), dict(case='large', p90=10**9), dict(case='hard', p90=None), dict(case='lost', p90=5)]
        after = [dict(case='small', p90=300), dict(case='large', p90=5 * 10**8), dict(case='hard', p90=7), dict(case='lost', p90=None),
                 dict(case='new', p90=1)]
        rows = bench.compare(before, after)
        self.assertEqual([r['case'] for r in rows], ['small', 'large', 'hard', 'lost'])
        self.assertEqual([r['ratio'] for r in rows], [3.0, 0.5, None, None])
        # the large case halving does not hide the small one tripling
        self.assertEqual(bench.verdicts(rows), dict(cheaper=1, dearer=1, same=0, newly_settled=1, no_longer_settled=1, unsettled=0))

    def test_spread_over_seeds_is_reported(self):
        result = bench.summaries([row(i, 100 + i) for i in range(10)])[0]
        self.assertEqual((result['least'], result['most'], result['p90']), (100, 109, 108))

    def test_sample_cases_name_existing_tests(self):
        cases = bench.load_cases([bench.ROOT / 'benchmarks/checker/samples.json'], '')
        self.assertTrue(cases)
        for case in cases:
            text = '\n'.join(source for _, source in case['sources'])
            self.assertIn(f"test {case['test']} ", text, case['name'])
            for expected in case['expected']:
                self.assertIn(expected['match'], text, case['name'])

    def test_a_patch_must_apply_exactly_once(self):
        sources = [['a/PSrc/M.p', 'if (x) { y(); }'], ['b/PSrc/M.p', 'if (x) { z(); }']]
        patch = dict(file='a/PSrc/M.p', find='if (x)', replace='if (true)')
        self.assertEqual(bench.patched(sources, [patch])[0][1], 'if (true) { y(); }')
        self.assertEqual(sources[0][1], 'if (x) { y(); }')
        for wrong in (dict(patch, find='if (z)'), dict(patch, file='PSrc/M.p'), dict(patch, find='(')):
            with self.assertRaises(SystemExit):
                bench.patched(sources, [wrong])


if __name__ == '__main__':
    unittest.main()
