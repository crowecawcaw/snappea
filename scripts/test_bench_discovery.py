import importlib.util
import math
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('bench_discovery', Path(__file__).with_name('bench-discovery.py'))
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)


class DiscoveryTest(unittest.TestCase):
    def test_a_missed_site_is_a_point_with_no_values(self):
        rows = [dict(case='c', seed=3, expected=['a', 'b'], first={'a': 7}, first_seconds={'a': 0.25})]
        self.assertEqual(bench.discoveries(rows), [
            dict(case='c', site='a', seed=3, schedules=7, seconds=0.25),
            dict(case='c', site='b', seed=3, schedules=None, seconds=None),
        ])

    def test_a_miss_is_infinite_in_a_quantile(self):
        self.assertEqual(bench.quantile([1, 2, 3, None], 0.5), 2)
        self.assertTrue(math.isinf(bench.quantile([1, 2, 3, None], 0.9)))

    def test_bins_cover_the_values_and_count_each_once(self):
        bins = bench.edges(3, 700)
        self.assertEqual(bins, [2, 5, 10, 20, 50, 100, 200, 500, 1000])
        counts, missed = bench.histogram([3, 5, 9, 700, None], bins)
        self.assertEqual((sum(counts), missed), (4, 1))
        self.assertEqual(counts[:2], [1, 2])

    def test_the_tail_lists_each_site_by_its_worst_seed(self):
        points = [dict(case='c', site='a', schedules=5), dict(case='c', site='a', schedules=50),
                  dict(case='c', site='b', schedules=None)]
        self.assertEqual(bench.tail(points, 'schedules'), [(('c', 'b'), math.inf), (('c', 'a'), 50)])

    def test_labels_are_short(self):
        self.assertEqual(bench.label(0.000522174, 's'), '0.522 ms')
        self.assertEqual(bench.label(10105, ''), '10.1k')

    def test_the_picture_draws_both_axes_and_an_earlier_run(self):
        points = [dict(schedules=n, seconds=n / 1000) for n in (1, 40, 900)] + [dict(schedules=None, seconds=None)]
        picture = bench.svg(points, points)
        for text in ('by schedules', 'by time', 'not found', 'before'):
            self.assertIn(text, picture)


if __name__ == '__main__':
    unittest.main()
