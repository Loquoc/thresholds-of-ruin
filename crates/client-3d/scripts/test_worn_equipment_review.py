"""Deterministic source fixtures; these tests do not claim rendered acceptance."""
import json
import unittest
from pathlib import Path
import generate_worn_equipment_review as generator


class WornReview(unittest.TestCase):
    def test_committed_scripts_are_current_and_bounded(self):
        fixture = Path(__file__).parent / 'fixtures/worn-equipment-review'
        pages = generator.reviews()
        for name, script in pages.items():
            self.assertEqual(script, json.loads((fixture / f'{name}.json').read_text()))
            self.assertLessEqual(len(script['actors']), 8)
            self.assertEqual(len(script['captures']), 5 + len(script['actors']))
            self.assertTrue(all(a['position'][1] == -0.5 for a in script['actors']))
            self.assertLessEqual(max(c['tick'] for c in script['captures']), 100)
            for actor in script['actors']:
                camera = next(c for c in script['cameras'] if c['id'] == f'detail-{actor["id"]}')
                self.assertLess(camera['position'][2] - actor['position'][2], 2.7,
                                'detail camera must sit before the neighboring row, not behind it')
                self.assertGreaterEqual(camera['fov_degrees'], 50)

    def test_catalog_and_layer_witnesses_are_complete(self):
        pages = generator.reviews()
        names = [a['equipment'][0] for page, s in pages.items() if page != 'layering' for a in s['actors']]
        self.assertEqual(names, generator.catalog())
        boots = set(generator.catalog()[-10:])
        self.assertTrue(all(a['outfit'] == 'adventurer' for page, scene in pages.items()
                            if page != 'layering' for a in scene['actors'] if a['id'] in boots))
        layers = {a['id']: a for a in pages['layering']['actors']}
        self.assertEqual(layers['centaur-head-and-hands']['model'], 'Centaur')
        self.assertTrue(layers['robe-over-full-skirt']['body']['woman'])
        self.assertEqual(layers['apron-over-caster-robe']['model'], 'Mage')

class RenderReceipt(unittest.TestCase):
    def test_capture_batches_preserve_complete_ordered_workload(self):
        import render_worn_equipment_review as runner
        for script in generator.reviews().values():
            batches = runner.capture_batches(script)
            self.assertEqual([c for batch in batches for c in batch['captures']], script['captures'])
            for batch in batches:
                self.assertLessEqual(len(batch['captures']), 5)
                end = max(c['tick'] for c in batch['captures'])
                self.assertEqual(batch['events'], [e for e in script['events'] if e['tick'] <= end])
                self.assertEqual({k: v for k, v in batch.items() if k not in ('captures', 'events')},
                                 {k: v for k, v in script.items() if k not in ('captures', 'events')})

    def test_failed_batch_retains_independent_later_evidence_without_acceptance(self):
        import tempfile
        from types import SimpleNamespace
        from unittest.mock import patch
        import render_worn_equipment_review as runner

        calls = []
        def process(command, **kwargs):
            calls.append(command)
            if len(calls) == 1:
                return SimpleNamespace(returncode=1)
            source = Path(command[command.index('--scene-script') + 1])
            target = Path(command[command.index('--capture') + 1])
            target.mkdir()
            script = json.loads(source.read_text())
            (target / 'manifest.json').write_text(json.dumps({'captures': script['captures']}))
            return SimpleNamespace(returncode=0)

        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'result'
            with patch('sys.argv', ['runner', '--binary', '/unused', '--output', str(output)]), \
                 patch.object(runner.subprocess, 'check_output', return_value='head'), \
                 patch.object(runner.subprocess, 'run', side_effect=process), \
                 patch.object(runner, 'digest', return_value='sha'), \
                 patch.object(runner, 'validate_manifest'):
                with self.assertRaisesRegex(RuntimeError, '1 capture batches failed'):
                    runner.main()
            receipt = json.loads((output / 'receipt.json').read_text())
            expected = sum(len(runner.capture_batches(script)) for script in generator.reviews().values())
            self.assertEqual(len(calls), expected)
            self.assertEqual(len(receipt['scenes']), expected - 1)
            self.assertEqual(len(receipt['failures']), 1)
            self.assertEqual(receipt['status'], 'failed')
            self.assertEqual(receipt['visual_acceptance'], 'not_reviewed')
            self.assertTrue(any(s['script'] == 'layering.json' for s in receipt['scenes']))

    def test_manifest_requires_exact_names_ticks_renderer_and_actual_images(self):
        import copy
        import struct
        import tempfile
        import zlib
        import render_worn_equipment_review as runner

        def chunk(kind, payload):
            return struct.pack('>I', len(payload)) + kind + payload + struct.pack('>I', zlib.crc32(kind + payload))

        # Tiny synthetic RGBA encoder fixture exercises the real PNG decoder.
        png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 2, 2, 8, 6, 0, 0, 0))
        png += chunk(b'IDAT', zlib.compress((b'\0' + bytes([30, 60, 90, 255]) * 2) * 2)) + chunk(b'IEND', b'')
        script = {'viewport': [2, 2], 'captures': [{'name': 'front', 'tick': 0}]}
        manifest = {'status': 'complete', 'capture_mode': 'offscreen',
                    'captures': [{'name': 'front', 'tick': 0, 'renderer': {'name': 'test-adapter'}}]}
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            (output / 'front.png').write_bytes(png)
            runner.validate_manifest(script, manifest, output)
            for key, value in [('status', 'failed'), ('capture_mode', 'window')]:
                bad = copy.deepcopy(manifest)
                bad[key] = value
                with self.assertRaises(ValueError):
                    runner.validate_manifest(script, bad, output)
            for key, value in [('name', 'other'), ('tick', 1), ('renderer', {})]:
                bad = copy.deepcopy(manifest)
                bad['captures'][0][key] = value
                with self.assertRaises(ValueError):
                    runner.validate_manifest(script, bad, output)
            recorded = copy.deepcopy(manifest)
            recorded['captures'][0]['actors'] = [
                {'id': 'cope', 'cloth_contact': {'solver_fallbacks': 0}}]
            runner.validate_manifest(script, recorded, output)
            for count in [1, 3, -1, True, '0']:
                recorded['captures'][0]['actors'][0]['cloth_contact']['solver_fallbacks'] = count
                with self.assertRaisesRegex(ValueError, 'fallback'):
                    runner.validate_manifest(script, recorded, output)
            (output / 'front.png').unlink()
            with self.assertRaises(ValueError):
                runner.validate_manifest(script, manifest, output)


if __name__ == '__main__':
    unittest.main()
