"""Every Rogue catalog identity/form has an explicit, truthful cosmetic recipe.

This is catalog coverage, not a claim that all runtime permutations were rendered.
Pool labels intentionally share a recipe; true identity never selects geometry.
"""
import json
from pathlib import Path
import re
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
EXPECTED = json.loads((ROOT / "scripts/fixtures/rogue-item-model-coverage.json").read_text())


class RogueItemModelCoverage(unittest.TestCase):
    def test_rogue_identity_hierarchy_preserves_all_old_equivalence_groups(self):
        rows = json.loads((ROOT / "scripts/fixtures/rogue-identity-coverage.json").read_text())
        old_to_new, new_to_old = {}, {}
        self.assertEqual(len(rows), 85)
        for row in rows:
            old, new = row["old_identity"], row["new_identity"]
            self.assertRegex(new, r"^item\.[a-z0-9-]+\.[a-z0-9-]+(?:\.[a-z0-9-]+)*$")
            self.assertFalse(new.endswith(".unknown"))
            self.assertEqual(old_to_new.setdefault(old, new), new)
            self.assertEqual(new_to_old.setdefault(new, old), old)
        self.assertEqual(len(old_to_new), 83)
        self.assertEqual(len(new_to_old), 83)
        for package in ("rogue", "rogue-exploration"):
            catalog = tomllib.loads((ROOT / "scenarios" / package / "catalog/items.toml").read_text())["items"]
            self.assertEqual({key: value["identity"] for key, value in catalog.items() if key != "rogue.generated_arrow"},
                             {row["archetype"]: row["new_identity"] for row in rows})

    def test_seeded_names_have_explicit_paired_identity_records(self):
        sources = []
        for root in (ROOT / "scenarios", ROOT / "crates/client-3d/scenarios"):
            for path in root.rglob("*.toml"):
                items = tomllib.loads(path.read_text()).get("items", [])
                if not isinstance(items, list):
                    continue
                for item in items:
                    if not item.get("seed_names"):
                        continue
                    sources.append(str(path.relative_to(ROOT)))
                    self.assertNotIn("seed_identities", item)
                    self.assertEqual(item["seed_names"], [
                        {"name": "copper token", "identity": "item.misc.copper-token"},
                        {"name": "silver token", "identity": "item.misc.silver-token"},
                        {"name": "iron token", "identity": "item.misc.iron-token"},
                    ])
        self.assertEqual(len(sources), 21)

    def test_all_74_hierarchical_appearance_assets_have_family_bounded_surfaces(self):
        expected = json.loads((ROOT / "scripts/fixtures/rogue-appearance-model-coverage.json").read_text())
        text = (ROOT / "crates/client-3d/model/src/appearance.rs").read_text()
        actual = {(kind, asset): (surface, int(index)) for kind, asset, surface, index in
                  re.findall(r'\(ItemModel::(\w+), "([^"]+)"\)\s*=>\s*\{?\s*Some\(ItemSurface::(\w+)\((\d+)\)\)', text)}
        self.assertEqual(actual, {(row["surface_family"], row["asset"]):
                                 (row["surface_family"], row["surface_index"]) for row in expected})
        self.assertEqual(len(actual), 74)
        self.assertEqual(len({row["asset"] for row in expected}), 74)

    def test_authored_appearance_records_preserve_explicit_label_key_pairs(self):
        expected = json.loads((ROOT / "scripts/fixtures/rogue-appearance-model-coverage.json").read_text())
        for package in ("rogue", "rogue-exploration"):
            pools = tomllib.loads((ROOT / "scenarios" / package / "catalog/items.toml").read_text())["appearance_pools"]
            for pool, values in pools.items():
                self.assertNotIn("asset", values)
                self.assertEqual(values["appearances"], [{"name": row["appearance"], "asset": row["asset"]}
                                 for row in expected if row["pool"] == pool])

    def test_all_catalog_identities_and_physical_forms_resolve_expected_recipes(self):
        text = (ROOT / "crates/client-3d/model/src/appearance.rs").read_text()
        table = text.split("fn items()", 1)[1].split("/// Resolve only", 1)[0]
        recipes = dict(re.findall(r'\("([^"]+)", ItemModel::(\w+)\)', table))
        for package in ("rogue", "rogue-exploration"):
            root = ROOT / "scenarios" / package
            catalog = tomllib.loads((root / "catalog/items.toml").read_text())
            palette = tomllib.loads((root / "scenario.toml").read_text())["assets"]["dungeon"]
            self.assertEqual(set(catalog["items"]), set(EXPECTED["items"]), package)
            forms = 0
            for identity, item in catalog["items"].items():
                forms += len(item.get("forms", [{}]))
                with self.subTest(package=package, identity=identity):
                    actual = []
                    for form_index, form in enumerate(item.get("forms", [{}])):
                        pool = form.get("appearance_pool", item.get("appearance_pool"))
                        expected = EXPECTED["items"][identity][form_index]
                        assets = [entry["asset"] for entry in catalog["appearance_pools"][pool]["appearances"]] if pool else [item.get("asset")]
                        for asset in assets:
                            self.assertIn(asset, palette, (package, identity))
                            if pool:
                                self.assertTrue(asset.startswith(expected["asset"] + "."), (identity, asset))
                            key = asset
                            while key and key not in recipes:
                                key = key.rsplit(".", 1)[0] if "." in key else None
                            self.assertEqual(recipes.get(key), expected["model"], (identity, asset))
                        actual.append({"pool": pool, "asset": expected["asset"] if pool else assets[0], "model": expected["model"]})
                    self.assertEqual(actual, EXPECTED["items"][identity], (package, identity))
            self.assertEqual(forms, 100)
            self.assertEqual({key: len(pool["appearances"]) for key, pool in catalog["appearance_pools"].items()},
                             EXPECTED["pool_label_counts"])
            self.assertEqual(sum(EXPECTED["pool_label_counts"].values()), 74)

    def test_review_generator_covers_every_form_and_every_public_appearance_label(self):
        import importlib.util
        path = ROOT / "crates/client-3d/scripts/generate_rogue_item_review.py"
        spec = importlib.util.spec_from_file_location("rogue_item_reviews", path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        pages, index = module.reviews(ROOT)
        self.assertEqual((index["catalog_definitions"], index["physical_forms"],
                          index["appearance_labels"], index["unique_recipes"]), (86, 100, 74, 28))
        self.assertEqual(index["true_identity_groups"], 83)
        self.assertEqual(len(index["forms"]), 100)
        catalog = tomllib.loads((ROOT / "scenarios/rogue/catalog/items.toml").read_text())["items"]
        self.assertEqual({row.get("archetype") for row in index["forms"]}, set(catalog))
        for row in index["forms"]:
            self.assertEqual(row["identity"], catalog[row["archetype"]]["identity"])
        self.assertEqual(sum(len(v["ground_items"]) for k, v in pages.items()
                             if k.startswith("all-forms-")), 100)
        self.assertEqual(sum(len(v["ground_items"]) for k, v in pages.items()
                             if k.startswith("appearances-")), 74)
        for name, page in pages.items():
            self.assertLessEqual(len(page["ground_items"]), 15)
            self.assertEqual(page["captures"][0]["name"], name)
            self.assertTrue(all(item["disclosed_name"] for item in page["ground_items"]))


    def test_dungeon_review_scenes_use_normal_camera_and_real_picker(self):
        root = ROOT / "crates/client-3d/scripts/fixtures/rogue-items"
        for name in ("rogue-world-light", "rogue-world-dark", "dungeon-light", "dungeon-dark", "dungeon-picker"):
            scene = json.loads((root / f"{name}.json").read_text())
            self.assertTrue(scene["dungeon"])
            self.assertTrue(scene["actors"])
            camera = scene["cameras"][0]
            distance_squared = sum((a - b) ** 2 for a, b in zip(camera["position"], camera["target"], strict=True))
            self.assertAlmostEqual(distance_squared, 8.5 ** 2)
            self.assertEqual(scene["captures"][0]["name"], name)
            self.assertLessEqual(len(scene["ground_items"]), 64)
        picker = json.loads((root / "dungeon-picker.json").read_text())
        self.assertTrue(picker["pile_picker"])

    def test_persistence_review_changes_only_names_not_appearance_or_pose(self):
        root = ROOT / "crates/client-3d/scripts/fixtures/rogue-items"
        unknown = json.loads((root / "appearance-persistence-unknown.json").read_text())
        known = json.loads((root / "appearance-persistence-known.json").read_text())
        self.assertFalse(unknown["ground_labels"])
        self.assertFalse(known["ground_labels"])
        self.assertNotEqual([row["disclosed_name"] for row in unknown["ground_items"]],
                            [row["disclosed_name"] for row in known["ground_items"]])
        for scene in (unknown, known):
            scene["captures"][0]["name"] = "same-image"
            for row in scene["ground_items"]:
                row.pop("disclosed_name")
        self.assertEqual(unknown, known)


if __name__ == "__main__":
    unittest.main()
