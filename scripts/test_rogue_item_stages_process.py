"""The real compiler accepts Rogue's chance and drought stages and rejects bad ones."""
import shutil
import subprocess
import tomllib
import unittest

from arena_matrix import toml_source
from process_harness import ProcessTestCase, ROOT


class RogueItemStageProcesses(ProcessTestCase):
    def check(self, edit):
        package = self.directory / "rogue-stages"
        shutil.copytree(ROOT / "scenarios/rogue", package, dirs_exist_ok=True)
        manifest = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
        stages = {stage["id"]: stage for stage in
                  manifest["floor_activation"]["profiles"]["rogue.low_depth"]["stages"]}
        edit(stages)
        (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        compiler = self.bin / ("tor-scenario" + self.suffix)
        return subprocess.run([str(compiler), "check", str(package)], capture_output=True,
                              text=True, encoding="utf-8", timeout=120)

    def test_rogue_stages_compile_and_invalid_variants_reject(self):
        accepted = self.check(lambda stages: None)
        self.assertEqual(accepted.returncode, 0, accepted.stderr)

        def no_attempts(stages):
            stages["rogue.items"]["attempts"] = 0

        def impossible_chance(stages):
            stages["rogue.items"]["chance"] = {"numerator": 2, "denominator": 1}

        def no_food(stages):
            stages["rogue.food_drought"]["food"] = []

        def duplicate_food(stages):
            stages["rogue.food_drought"]["food"] = ["rogue.ration", "rogue.ration"]

        def unknown_food(stages):
            stages["rogue.food_drought"]["food"] = ["rogue.cake"]

        def unknown_field(stages):
            stages["rogue.food_drought"]["count"] = [1, 1]

        for edit in (no_attempts, impossible_chance, no_food, duplicate_food, unknown_food,
                     unknown_field):
            with self.subTest(edit=edit.__name__):
                self.assertNotEqual(self.check(edit).returncode, 0)

    def test_grouped_missiles_use_their_own_quantity(self):
        def choice(stages, archetype):
            # Assert the intended operand exists before testing its validation.
            # Generated arrows use a distinct authoring recipe from trap ammo.
            matches = [row for row in stages["rogue.items_other"]["bands"][0]["choices"]
                       if row["archetype"] == archetype]
            self.assertEqual(len(matches), 1, archetype)
            return matches[0]

        def inherited_quantities(stages):
            for row in stages["rogue.items_other"]["bands"][0]["choices"]:
                row.pop("quantity", None)
        accepted = self.check(inherited_quantities)
        self.assertEqual(accepted.returncode, 0, accepted.stderr)

        def dagger_stack(stages):
            choice(stages, "rogue.dagger")["quantity"] = {
                "kind": "uniform", "minimum": 2, "maximum": 5}
        accepted = self.check(dagger_stack)
        self.assertEqual(accepted.returncode, 0, accepted.stderr)

        def bad_quantity(stages):
            choice(stages, "rogue.generated_arrow")["quantity"] = {
                "kind": "uniform", "minimum": 9, "maximum": 2}
        self.assertNotEqual(self.check(bad_quantity).returncode, 0)

        def nonstackable_quantity(stages):
            choice(stages, "rogue.long_sword")["quantity"] = 2
        self.assertNotEqual(self.check(nonstackable_quantity).returncode, 0)


if __name__ == "__main__":
    unittest.main()
