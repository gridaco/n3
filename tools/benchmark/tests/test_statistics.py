"""Small arithmetic oracles and metamorphic checks, independent of report code."""

import random
import unittest

from tools.benchmark.statistics import aggregate, distribution


class StatisticsTests(unittest.TestCase):
    def test_literal_distribution_oracles(self):
        cases = [
            ([], None),
            ([None, None], None),
            ([0, None, 0], {"count": 2, "median": 0, "p95": 0, "min": 0, "max": 0}),
            ([9, 1, 4], {"count": 3, "median": 4, "p95": 9, "min": 1, "max": 9}),
            ([9, 1, None, 7, 3], {"count": 4, "median": 5, "p95": 9, "min": 1, "max": 9}),
            (list(range(1, 21)), {"count": 20, "median": 10.5, "p95": 19, "min": 1, "max": 20}),
            ([1] * 19 + [10000], {"count": 20, "median": 1, "p95": 1, "min": 1, "max": 10000}),
        ]
        for values, expected in cases:
            with self.subTest(values=values):
                self.assertEqual(distribution(iter(values)), expected)

    def test_unequal_run_lengths_get_equal_weight_and_keep_tail_ranges(self):
        # These literal per-run summaries have radically different frame counts.
        # Pooling/weighting by frame count would privilege the slow third run.
        runs = [
            {"count": 2, "median": 1, "p95": 2, "min": 0, "max": 2},
            {"count": 3, "median": 3, "p95": 8, "min": 1, "max": 8},
            {"count": 1000, "median": 9, "p95": 90, "min": 2, "max": 100},
        ]
        self.assertEqual(aggregate(runs), {
            "unit": "ms", "run_count": 3,
            "run_medians": {"median": 3, "min": 1, "max": 9, "mad": 2, "relative_mad_percent": 200 / 3},
            "run_p95": {"median": 8, "min": 2, "max": 90},
            "run_max": {"median": 8, "min": 2, "max": 100},
        })

    def test_zero_baseline_and_missing_stages_do_not_invent_measurements(self):
        self.assertIsNone(aggregate([None, None]))
        zero = {"count": 2, "median": 0, "p95": 0, "min": 0, "max": 0}
        result = aggregate([zero, None, zero])
        self.assertEqual(result["run_count"], 2)
        self.assertEqual(result["run_medians"]["mad"], 0)
        self.assertIsNone(result["run_medians"]["relative_mad_percent"])

    def test_permutation_and_positive_scaling_preserve_statistics(self):
        generator = random.Random(517)
        for count in (1, 2, 3, 19, 20, 21, 100):
            values = [generator.randrange(10000) for _ in range(count)]
            expected = distribution(values)
            shuffled = values.copy()
            generator.shuffle(shuffled)
            self.assertEqual(distribution(shuffled), expected)
            scaled = distribution([value * 4 for value in shuffled])
            self.assertEqual(scaled["count"], count)
            for name in ("median", "p95", "min", "max"):
                self.assertEqual(scaled[name], expected[name] * 4)


if __name__ == "__main__":
    unittest.main()
