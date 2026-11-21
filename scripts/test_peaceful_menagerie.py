"""The peaceful hall contains the complete current mob catalog."""
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "scenarios/peaceful-menagerie"


def read(path):
    return tomllib.loads(path.read_text(encoding="utf-8"))


class PeacefulMenagerie(unittest.TestCase):
    def test_every_current_mob_has_one_separate_place_in_the_large_hall(self):
        catalog = read(ROOT / "scenarios/rogue-exploration/catalog/actors.toml")["actors"]
        region = read(PACKAGE / "regions/1.toml")
        placed = region["actors"]
        self.assertEqual(len(catalog), 26)
        self.assertCountEqual([actor["archetype"] for actor in placed], catalog)
        self.assertTrue(region["chamber"])
        self.assertTrue(region["lit"])
        self.assertGreaterEqual(region["size"][0], 30)
        self.assertGreaterEqual(region["size"][1], 20)
        cells = {tuple(actor["at"]) for actor in placed}
        self.assertEqual(len(cells), len(placed))
        self.assertNotIn(tuple(region["anchors"]["start"]), cells)
        for actor in placed:
            with self.subTest(actor=actor["archetype"]):
                self.assertEqual(actor["controller"], "ai")
                self.assertEqual(actor["ai"], "wander")
                x, y, z = actor["at"]
                self.assertTrue(0 < x < region["size"][0] - 1)
                self.assertTrue(0 < y < region["size"][1] - 1)
                self.assertEqual(z, 0)

    def test_no_species_or_faction_acquires_hostiles_and_all_models_are_forecast(self):
        manifest = read(PACKAGE / "scenario.toml")
        self.assertTrue(all(not foes for foes in manifest["factions"].values()))
        species = read(PACKAGE / "catalog/creatures.toml")["creatures"]["species"]
        for name, definition in species.items():
            with self.subTest(species=name):
                self.assertEqual(definition["behavior"]["disposition"], "passive")
        senses = species["rogue.human"]["modifiers"]
        self.assertIn({"scope": "always", "op": {"kind": "perception", "value": {"kind": "see_invisible", "enabled": True}}}, senses)
        actors = read(PACKAGE / "catalog/actors.toml")["actors"]
        for actor in actors.values():
            self.assertIn(actor["asset"], manifest["assets"]["dungeon"])
        self.assertEqual(manifest["characters"][0]["creature"]["nutrition"]["kind"], "exempt")


if __name__ == "__main__":
    unittest.main()
