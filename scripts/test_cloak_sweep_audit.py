"""Independent local witnesses for the diagnostic, not native render acceptance."""
import copy
import itertools
import math
import random
import unittest

from audit_cloak_sweep import audit, dot, support_planes, traces


def fixture(witness):
    return {"face": [0, 1, 2], "source": "arms", "support_point": witness,
            "face_reference": [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
            "face_axes": [[0., 0., 1.]] * 3, "face_budgets": [0.2] * 3,
            "face_budget_endpoints": [[0., 0., 0.2], [1., 0., 0.2], [0., 1., 0.2]],
            "face_final": [[0., 0., 0.1], [1., 0., 0.1], [0., 1., 0.1]]}


class SweptFaceAudit(unittest.TestCase):
    def test_bbox_false_contact_is_excluded_but_real_interior_is_not(self):
        # (0.9,0.9) lies in the independent X/Y prism but outside x+y<=1.
        result = audit(fixture([0.9, 0.9, 0.1]))
        self.assertEqual(result["status"], "excluded_from_clearance_sweep")
        self.assertAlmostEqual(result["maximum_plane_excess"], 0.8 / math.sqrt(2))
        self.assertEqual(audit(fixture([0.3, 0.3, 0.1]))["status"], "not_excluded")

    def test_clearance_expansion_is_not_a_raw_hull_test(self):
        self.assertEqual(audit(fixture([0.2, 0.2, 0.22]))["status"], "not_excluded")
        self.assertEqual(audit(fixture([0.2, 0.2, 0.24]))["status"], "excluded_from_clearance_sweep")

    def test_all_independent_segments_and_convex_faces_remain_contained(self):
        starts = [(0., 0., 0.), (1., 0., 0.), (0., 1., 0.)]
        ends = [(0., 0., 0.3), (1.2, 0.1, 0.), (0., 1.2, 0.2)]
        planes = support_planes(starts + ends)
        rng = random.Random(197)
        for _ in range(200):
            travel = [rng.random() for _ in range(3)]
            weights = [rng.random() for _ in range(3)]
            total = sum(weights)
            point = tuple(sum(weights[i] / total * (starts[i][j] * (1 - travel[i]) + ends[i][j] * travel[i]) for i in range(3)) for j in range(3))
            self.assertTrue(all(dot(normal, point) <= height + 1e-10 for normal, height in planes))
        for ends_choice in itertools.product((False, True), repeat=3):
            for i, choose_end in enumerate(ends_choice):
                point = ends[i] if choose_end else starts[i]
                self.assertTrue(all(dot(n, point) <= h + 1e-10 for n, h in planes))

    def test_planar_collinear_and_point_sweeps_are_bounded_conservatively(self):
        for points, outside in [([(0., 0., 0.), (1., 0., 0.), (0., 1., 0.)], (0.9, 0.9, 0.)),
                                ([(0., 0., 0.), (1., 1., 0.)], (0.1, 0.9, 0.)),
                                ([(0., 0., 0.)], (0., 0., 0.1))]:
            planes = support_planes(points)
            self.assertTrue(all(dot(n, p) <= h + 1e-10 for p in points for n, h in planes))
            self.assertGreater(max(dot(n, outside) - h for n, h in planes), 0.032)

    def test_missing_and_corrupt_evidence_never_claims_exclusion(self):
        self.assertEqual(audit({})["status"], "missing_evidence")
        for mutate in [lambda t: t["face_budget_endpoints"][0].__setitem__(2, 0.9),
                       lambda t: t["face_final"][0].__setitem__(2, 0.3),
                       lambda t: t["face_axes"][0].__setitem__(2, float("nan"))]:
            trace = copy.deepcopy(fixture([0.9, 0.9, 0.1]))
            mutate(trace)
            with self.assertRaises(ValueError):
                audit(trace)

    def test_excluded_inner_peak_does_not_waive_layer_containment(self):
        trace = fixture([0.9, 0.9, 0.1])
        trace["source"] = "inner"
        result = audit(trace)
        self.assertEqual(result["status"], "excluded_from_clearance_sweep")
        self.assertIn("outermost containment may still require", result["interpretation"])

    def test_capture_traversal_finds_only_actual_witnesses(self):
        trace = fixture([0.2, 0.2, 0.1])
        found = list(traces({"captures": [{"trace": trace}, {"unrelated": 1}]}))
        self.assertEqual(found, [("$.captures[0].trace", trace)])


if __name__ == "__main__":
    unittest.main()
