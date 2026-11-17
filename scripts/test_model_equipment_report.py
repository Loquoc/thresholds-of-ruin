import copy
import unittest
from model_performance_report import COUNTS, FRAME_TIMINGS, validate
from model_equipment_report import RECIPES, validate_equipment


def samples():
    rows = []
    for case, item, woman, build, motion, velocity in RECIPES:
        for sample in range(3):
            row = dict(version=6, suite="equipment-construction-v1", case=case,
                       equipment=item, model="human", wardrobe="native", sample=sample,
                       seed=7, hz=50, woman=woman, build=build, motion=motion, velocity=velocity)
            row.update({key: 1 for key in COUNTS})
            row.update(cold_meshes=2, cold_mesh_bytes=2, motion_frames=30, changed_pose_frames=30)
            row.update({key: [0.0] * 30 for key in FRAME_TIMINGS})
            row.update(pose_update_ms=[1.0] * 30, cold_spawn_ms=1.0, warm_spawn_ms=1.0)
            rows.append(row)
    return rows


class EquipmentValidation(unittest.TestCase):
    def test_accepts_complete_equipment_without_requiring_nymph_hair(self):
        validate_equipment(samples())
        with self.assertRaises(ValueError):
            validate(samples())

    def test_rejects_changed_identity_or_invalid_measurement(self):
        for key, value in [("version", 5), ("suite", "other"), ("equipment", "boots"),
                           ("model", "mage"), ("wardrobe", "royal"), ("woman", 0),
                           ("build", True), ("motion", "jump"), ("velocity", [False, 0, 1.5]),
                           ("velocity", [0, 0, float("nan")]), ("changed_pose_frames", 0),
                           ("warm_added_meshes", 2), ("cold_spawn_ms", -1),
                           ("pose_update_ms", [1] * 29), ("robe_fit_ms", [2] * 30)]:
            with self.subTest(key=key):
                rows = copy.deepcopy(samples())
                rows[0][key] = value
                with self.assertRaises(ValueError):
                    validate_equipment(rows)

    def test_rejects_missing_reordered_unknown_and_inconsistent_counts(self):
        rows = samples()
        changed = copy.deepcopy(rows)
        changed[1]["cold_mesh_bytes"] = 3
        for invalid in [rows[:-1], rows[::-1], [dict(rows[0], extra=1), *rows[1:]], changed]:
            with self.assertRaises(ValueError):
                validate_equipment(invalid)


if __name__ == "__main__":
    unittest.main()
