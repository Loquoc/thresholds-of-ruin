"""Pinned birth-item contracts, independent of runtime sampling and live tables.

Reference fixture is manually transcribed from Rogue 5.4.4, not generated from
TOR content. These checks do not claim runtime ownership/replay qualification.
"""
from fractions import Fraction
import json
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
REFERENCE = json.loads((ROOT / "scripts/fixtures/rogue-birth-item-reference.json").read_text())
PACKAGES = ("rogue", "rogue-exploration")


def source(package, name):
    return tomllib.loads((ROOT / "scenarios" / package / name).read_text(encoding="utf-8"))


def actor_choices(manifest):
    for profile in manifest["floor_activation"]["profiles"].values():
        for stage in profile["stages"]:
            bands = stage.get("bands", []) if stage["kind"] == "actors" else stage.get("actor_bands", [])
            for band in bands:
                yield from band["choices"]


class RogueBirthInventoryContent(unittest.TestCase):
    def test_reference_is_complete_and_normalized(self):
        self.assertEqual(len(REFERENCE["carry_percent"]), 26)
        self.assertEqual(sum(value > 0 for value in REFERENCE["carry_percent"].values()), 11)
        groups = REFERENCE["categories"]
        self.assertEqual([g["id"] for g in groups],
                         ["potion", "scroll", "food", "weapon", "armor", "ring", "device"])
        self.assertEqual([len(g["choices"]) for g in groups], [14, 18, 2, 9, 8, 14, 14])
        self.assertEqual(sum(g["weight"] for g in groups), 100)
        rows = [row for group in groups for row in group["choices"]]
        self.assertEqual(len({row["archetype"] for row in rows}), 79)
        for group in groups:
            self.assertEqual(sum(row["weight"] for row in group["choices"]), 100)
        self.assertNotIn("rogue.gold", {row["archetype"] for row in rows})
        self.assertNotIn("rogue.amulet_of_yendor", {row["archetype"] for row in rows})

    def test_all_initial_and_treasure_species_have_exact_carry_chances(self):
        for package in PACKAGES:
            seen = set()
            for row in actor_choices(source(package, "scenario.toml")):
                name = row["archetype"]
                seen.add(name)
                with self.subTest(package=package, species=name):
                    percent = REFERENCE["carry_percent"][name]
                    if name == "rogue.leprechaun":
                        self.assertEqual(row["carried_loot"]["pool"], "rogue.leprechaun_gold")
                        chance = row["carried_loot"]["chance"]
                        self.assertEqual(Fraction(chance["numerator"], chance["denominator"]), 1)
                    elif percent:
                        self.assertIn("carried_loot", row)
                        chance = row["carried_loot"]["chance"]
                        self.assertEqual(Fraction(chance["numerator"], chance["denominator"]), Fraction(percent, 100))
                        self.assertNotEqual(row["carried_loot"]["pool"], "rogue.leprechaun_gold")
                    else:
                        self.assertNotIn("carried_loot", row)
            self.assertEqual(seen, set(REFERENCE["carry_percent"]))

    def ordinary_birth_rows(self, package):
        manifest = source(package, "scenario.toml")
        pools = {row["carried_loot"]["pool"] for row in actor_choices(manifest)
                 if REFERENCE["carry_percent"][row["archetype"]] > 0
                 and "carried_loot" in row}
        self.assertEqual(len(pools), 1, "ordinary carriers share one generation recipe")
        pool = manifest["floor_activation"]["carried_item_pools"][next(iter(pools))]
        self.assertEqual(set(pool), {"version", "choices"})
        self.assertEqual(pool["version"], 1)
        return pool["choices"]

    def test_flat_birth_pool_has_exact_category_subtype_product_weights_and_quantities(self):
        # Flat product weights preserve the source probability law without adding
        # a category field to persisted recipes. Birth-only aliases may retain
        # the same identity while selecting a different generation policy.
        for package in PACKAGES:
            with self.subTest(package=package):
                items = source(package, "catalog/items.toml")["items"]
                expected = {items[row["archetype"]]["identity"]:
                            (group["weight"] * row["weight"], row["quantity"])
                            for group in REFERENCE["categories"] for row in group["choices"]}
                rows = self.ordinary_birth_rows(package)
                self.assertEqual(len(rows), 79)
                actual = {items[row["archetype"]]["identity"]:
                          (row["weight"], row["quantity"]) for row in rows}
                self.assertEqual(len(actual), 79, "aliases cannot duplicate probability mass")
                self.assertEqual(actual, expected)
                self.assertEqual(sum(row["weight"] for row in rows), 10000)
                for group in REFERENCE["categories"]:
                    identities = {items[row["archetype"]]["identity"] for row in group["choices"]}
                    self.assertEqual(sum(actual[key][0] for key in identities), group["weight"] * 100)

    def test_resolved_birth_recipes_include_all_source_equipment_and_charge_policies(self):
        for package in PACKAGES:
            with self.subTest(package=package):
                items = source(package, "catalog/items.toml")["items"]
                rows = self.ordinary_birth_rows(package)
                actual = {items[row["archetype"]]["identity"]: items[row["archetype"]] for row in rows}
                for group in REFERENCE["categories"]:
                    for row in group["choices"]:
                        canonical = row["archetype"]
                        identity = items[canonical]["identity"]
                        self.assertIn(identity, actual)
                        item = actual[identity]
                        kind = group["id"]
                        if kind in ("weapon", "armor"):
                            expected = REFERENCE["equipment_recipes"][kind]
                        elif canonical in REFERENCE["signed_rings"]:
                            expected = REFERENCE["equipment_recipes"]["signed_ring"]
                        elif canonical in REFERENCE["cursed_rings"]:
                            expected = REFERENCE["equipment_recipes"]["cursed_ring"]
                        else:
                            expected = None
                        self.assertEqual(item.get("equipment_generation"), expected)
                        if kind == "device":
                            charges = "light" if canonical == "rogue.wand_light" else "other"
                            self.assertEqual(item["device"]["initial_charges"], REFERENCE["device_charges"][charges])
                        # Alias differences must not change item effects or type
                        # knowledge. Quantity may be a stack or individual units.
                        for field in ("name", "class", "appearance_pool", "equipment", "projectile",
                                      "consumable", "readable", "device", "edible", "forms"):
                            self.assertEqual(item.get(field), items[canonical].get(field), (canonical, field))

    def test_floor_daggers_and_generator_use_two_to_five_without_changing_other_quantities(self):
        import rogue_item_table
        expected = {"kind": "uniform", "minimum": 2, "maximum": 5}
        for package in PACKAGES:
            seen = 0
            manifest = source(package, "scenario.toml")
            for profile in manifest["floor_activation"]["profiles"].values():
                for stage in profile["stages"]:
                    bands = stage.get("item_bands", []) if stage["kind"] == "room_cluster" else (
                        stage.get("bands", []) if stage["kind"] != "actors" else [])
                    for band in bands:
                        for row in band["choices"]:
                            if row["archetype"] == "rogue.dagger":
                                seen += 1
                                self.assertEqual(row.get("quantity", stage.get("quantity", 1)), expected)
            self.assertGreater(seen, 0)
            self.assertTrue(source(package, "catalog/items.toml")["items"]["rogue.dagger"]["stackable"])
        row = next(row for row in rogue_item_table.choices() if row[0] == "rogue.dagger")
        self.assertEqual(tomllib.loads("quantity = " + str(row[2]))["quantity"], expected)

    def test_generated_arrow_alias_preserves_mundane_starter_and_trap_recipe(self):
        for package in PACKAGES:
            items = source(package, "catalog/items.toml")["items"]
            base, generated = dict(items["rogue.arrow"]), dict(items["rogue.generated_arrow"])
            self.assertNotIn("equipment_generation", base)
            self.assertEqual(generated.pop("equipment_generation"), REFERENCE["equipment_recipes"]["weapon"])
            self.assertEqual(generated, base)
            self.assertEqual(base["identity"], "item.weapon.arrow")
            self.assertIn("rogue.generated_arrow", {row["archetype"] for row in self.ordinary_birth_rows(package)})

    def test_leprechaun_gold_is_separate_finite_birth_floor_recipe(self):
        for package in PACKAGES:
            manifest = source(package, "scenario.toml")
            pool = manifest["floor_activation"]["carried_item_pools"]["rogue.leprechaun_gold"]
            self.assertEqual(pool, {"version": 1, "choices": [REFERENCE["leprechaun_gold_tor_adaptation"]]})
            # Wandering eligibility references a shared actor stage, not a second
            # stock generator. Runtime owns the no-stock origin boundary.
            wandering = manifest["floor_activation"]["wandering"]
            self.assertEqual(wandering["stage_id"], "rogue.eight_species_foes")
            self.assertNotIn("carried_loot", wandering)
            self.assertIn("rogue.leprechaun", wandering["excluded_wandering_content"])

    def test_reused_catalog_recipes_preserve_buc_enchantments_and_device_charges(self):
        for package in PACKAGES:
            items = source(package, "catalog/items.toml")["items"]
            for group in REFERENCE["categories"]:
                for row in group["choices"]:
                    name = row["archetype"]
                    with self.subTest(package=package, item=name):
                        item = items["rogue.generated_arrow" if name == "rogue.arrow" else name]
                        kind = group["id"]
                        if kind in ("weapon", "armor"):
                            self.assertEqual(item.get("equipment_generation"), REFERENCE["equipment_recipes"][kind])
                        elif name in REFERENCE["signed_rings"]:
                            self.assertEqual(item.get("equipment_generation"), REFERENCE["equipment_recipes"]["signed_ring"])
                        elif name in REFERENCE["cursed_rings"]:
                            self.assertEqual(item.get("equipment_generation"), REFERENCE["equipment_recipes"]["cursed_ring"])
                        else:
                            self.assertNotIn("equipment_generation", item)
                        if kind == "device":
                            charges = "light" if name == "rogue.wand_light" else "other"
                            self.assertEqual(item["device"]["initial_charges"], REFERENCE["device_charges"][charges])


if __name__ == "__main__":
    unittest.main()
