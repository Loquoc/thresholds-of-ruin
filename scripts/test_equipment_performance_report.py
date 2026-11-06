import copy
import unittest
from equipment_performance_report import summarize
from workload_report import InvalidWorkload


def rows():
    return [dict(version=1, seed=42, items=n, sample=s, decision_ms=1.0,
        selected_item=99+n, gear_projections=n, game_clones=n+1, dynamic_folds=2*n)
        for n in [1, 4, 16, 64] for s in range(20)]


class EquipmentReport(unittest.TestCase):
    def test_complete_positive_matrix(self):
        self.assertEqual(len(summarize(rows())), 4)

    def test_rejects_missing_duplicate_and_invalid_samples(self):
        good = rows()
        variants = [good[:-1], good + [good[0]]]
        for field, value in [("selected_item", 0), ("seed", 7), ("version", 2),
                             ("decision_ms", float("nan")), ("gear_projections", -1),
                             ("game_clones", 0), ("items", True)]:
            altered = copy.deepcopy(good)
            altered[0][field] = value
            variants.append(altered)
        for variant in variants:
            with self.subTest(row=variant[0]):
                with self.assertRaises(InvalidWorkload):
                    summarize(variant)

    def test_perf_compare_owns_equipment_example(self):
        import perf_compare
        unit, = perf_compare.parse_cases(["equipment:i4", "equipment:i64"], 1)
        self.assertEqual(unit.spec.package, "tor-simulation")
        self.assertEqual(unit.spec.example, "equipment_bench")
        self.assertEqual(unit.groups, ("i4", "i64"))


if __name__ == "__main__":
    unittest.main()
