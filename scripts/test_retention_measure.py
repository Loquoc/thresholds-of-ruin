"""Independent matrix, result validation and workflow safety witnesses."""
import copy
import json
from pathlib import Path
import unittest

import retention_measure as measure


def sample(history=0, owners=1):
    stages = []
    for phase, roots, handles, records in (
        ('seeded', min(history + 1, 128), 0, history),
        ('rewind_expired', 128 + 32 * owners, 32 * owners, history + 32 * owners + 128),
        ('capabilities_released', 128, 0, history + 32 * owners + 128),
    ):
        stages.append({'phase': phase, 'unique_boundary_roots': roots, 'presentation_handles': handles,
                       'records': records, 'rss_bytes': 1024, 'boundary_inline_bytes': roots * 8,
                       'snapshot_batch_json_bytes': 100, 'snapshot_batch_root_limit': 257,
                       'snapshot_batches': (roots + 256) // 257})
    return {'version': 2, 'profile': 'release', 'history': history, 'attachments': owners,
            'recovery': {'records_loaded': history + 32 * owners + 129, 'records_replayed': 0},
            'stages': stages, 'rss_before_engine': 1024, 'checkpoint_json_bytes': 100,
            'saved_bytes': 100, 'restart_ms': 1.5}


class RetentionMeasurement(unittest.TestCase):
    def test_all_eight_cases_validate_and_decode(self):
        for history, owners in measure.CASES:
            row = sample(history, owners)
            self.assertEqual(measure.decode('retention-sample ' + json.dumps(row), history, owners), row)

    def test_rejects_missing_duplicate_and_wrong_identity_output(self):
        line = 'retention-sample ' + json.dumps(sample())
        for text in ('', line + '\n' + line, 'retention-sample {}', line.replace('release', 'debug')):
            with self.subTest(text=text), self.assertRaises((ValueError, KeyError)):
                measure.decode(text, 0, 1)

    def test_rejects_wrong_counts_recovery_and_nonfinite_samples(self):
        original = sample(10000, 8)
        mutations = [lambda r: r['stages'][1].update(unique_boundary_roots=128),
                     lambda r: r['stages'][2].update(presentation_handles=256),
                     lambda r: r['stages'][1].update(snapshot_batches=1),
                     lambda r: r['recovery'].update(records_replayed=1),
                     lambda r: r.update(restart_ms=float('nan')),
                     lambda r: r.update(restart_ms=-1),
                     lambda r: r['stages'][0].update(rss_bytes=None),
                     lambda r: r.update(stages=r['stages'][:2])]
        for mutate in mutations:
            row = copy.deepcopy(original)
            mutate(row)
            with self.assertRaises((ValueError, KeyError)):
                measure.validate(row, 10000, 8)

    def test_complete_matrix_and_distribution_require_every_round(self):
        rows = [dict(sample(h, c), round=n) for n in range(3) for h, c in measure.CASES]
        result = measure.summarize(rows)
        self.assertEqual(len(result), 8)
        self.assertEqual(result[0]['restart_ms']['n'], 3)
        self.assertEqual(measure.distribution([3, 1, 2]), {'n': 3, 'p50': 2, 'p95': 3, 'max': 3, 'min': 1})
        for bad in (rows[:-1], rows[:-1] + [rows[0]]):
            with self.assertRaises(ValueError):
                measure.summarize(bad)

    def test_expensive_diagnostics_remain_local_only(self):
        root = Path(__file__).resolve().parents[1]
        workflows = root / '.github/workflows'
        for workflow, driver in (
            ('retention-diagnostic.yml', 'scripts/retention_measure.py'),
            ('worn-equipment-review.yml', 'crates/client-3d/scripts/render_worn_equipment_review.py'),
        ):
            with self.subTest(workflow=workflow):
                self.assertTrue((root / driver).is_file(), 'preserve the local diagnostic tool')
                self.assertFalse((workflows / workflow).exists(), 'diagnostics must not have hosted entry points')
                for hosted in sorted(workflows.glob('*')):
                    if hosted.suffix in ('.yml', '.yaml'):
                        self.assertNotIn(Path(driver).name, hosted.read_text(encoding='utf-8'),
                                         f'{hosted.name} must not invoke the local-only driver')


if __name__ == '__main__':
    unittest.main()
