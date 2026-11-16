import copy
import unittest
from model_performance_report import COUNTS, FRAME_TIMINGS
from model_targeted_report import RECIPES, validate_targeted


def samples():
    rows = []
    for case, woman, build, motion, velocity in RECIPES:
        for sample in range(3):
            row = dict(version=5, suite="targeted-contact-v1", case=case, sample=sample,
                       seed=7, hz=50, woman=woman, build=build, motion=motion, velocity=velocity)
            row.update({key: 1 for key in COUNTS})
            row.update(cold_meshes=2, cold_mesh_bytes=2, motion_frames=30, changed_pose_frames=30)
            row.update({key: [0.1] * 30 for key in FRAME_TIMINGS})
            row.update(pose_update_ms=[1.0] * 30, hair_fit_ms=[0.5] * 30,
                       cold_spawn_ms=1.0, warm_spawn_ms=1.0)
            rows.append(row)
    return rows


class TargetedValidation(unittest.TestCase):
    def test_accepts_complete_bounded_recipe(self):
        validate_targeted(samples())

    def test_rejects_identity_motion_and_inactive_pose(self):
        for key, value in [("version", 4), ("woman", 1), ("build", True),
                           ("motion", "jump"), ("velocity", [0, 0, 0]),
                           ("velocity", [False, False, 1.5]), ("velocity", (0, 0, 1.5)),
                           ("velocity", [0, 0, float("nan")]),
                           ("suite", "other"), ("changed_pose_frames", 0),
                           ("hair_constraints_ms", [0] * 30)]:
            with self.subTest(key=key):
                rows = copy.deepcopy(samples())
                rows[0][key] = value
                with self.assertRaises(ValueError):
                    validate_targeted(rows)

    def test_rejects_missing_reordered_and_unknown_fields(self):
        rows = samples()
        for invalid in [rows[:-1], list(reversed(rows)), [dict(rows[0], extra=1), *rows[1:]]]:
            with self.assertRaises(ValueError):
                validate_targeted(invalid)


if __name__ == "__main__":
    unittest.main()
