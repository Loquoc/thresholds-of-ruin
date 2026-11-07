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

    def test_catalog_and_layer_witnesses_are_complete(self):
        pages = generator.reviews()
        names = [a['equipment'][0] for page, s in pages.items() if page != 'layering' for a in s['actors']]
        self.assertEqual(names, generator.catalog())
        layers = {a['id']: a for a in pages['layering']['actors']}
        self.assertEqual(layers['centaur-head-and-hands']['model'], 'Centaur')
        self.assertTrue(layers['robe-over-full-skirt']['body']['woman'])
        self.assertEqual(layers['apron-over-caster-robe']['model'], 'Mage')

class RenderReceipt(unittest.TestCase):
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
            (output / 'front.png').unlink()
            with self.assertRaises(ValueError):
                runner.validate_manifest(script, manifest, output)


if __name__ == '__main__':
    unittest.main()
