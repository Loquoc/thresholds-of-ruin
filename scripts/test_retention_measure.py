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

    def test_workflow_is_read_only_path_scoped_and_retains_failures(self):
        workflow = (Path(__file__).resolve().parents[1] / '.github/workflows/retention-diagnostic.yml').read_text()
        self.assertIn('  pull_request:\n    paths:', workflow)
        self.assertNotIn('pull_request_target', workflow)
        self.assertNotIn('  push:', workflow)
        self.assertNotIn('schedule:', workflow)
        self.assertIn('contents: read', workflow)
        self.assertNotIn(': write', workflow)
        self.assertIn('persist-credentials: false', workflow)
        self.assertIn('ref: ${{ github.sha }}', workflow)
        self.assertIn('if: always()', workflow)
        self.assertIn('retention-days: 14', workflow)
        self.assertNotIn('continue-on-error', workflow)
        self.assertIn('actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1', workflow)
        self.assertIn('actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9', workflow)


if __name__ == '__main__':
    unittest.main()
