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
            self.assertEqual(len(script['captures']), 5)
            self.assertLessEqual(max(c['tick'] for c in script['captures']), 100)

    def test_catalog_and_layer_witnesses_are_complete(self):
        pages = generator.reviews()
        names = [a['equipment'][0] for page, s in pages.items() if page != 'layering' for a in s['actors']]
        self.assertEqual(names, generator.catalog())
        layers = {a['id']: a for a in pages['layering']['actors']}
        self.assertEqual(layers['centaur-head-and-hands']['model'], 'Centaur')
        self.assertTrue(layers['robe-over-full-skirt']['body']['woman'])
        self.assertEqual(layers['apron-over-caster-robe']['model'], 'Mage')


if __name__ == '__main__':
    unittest.main()
