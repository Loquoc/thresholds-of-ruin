"""Outcome scripts shipped in packages match the canonical copies generators use."""
import tempfile
import tomllib
import unittest
from pathlib import Path

import outcome_scripts

ROOT = Path(__file__).resolve().parents[1]
SCENARIOS = ROOT / "scenarios"
DEATH = ["tests/advancement-owner", "tests/ai-interactions", "tests/generated-filler",
         "tests/interactions", "tests/magic-items", "tests/streaming-corridor"]
SEAL = ["first-dungeon", "tests/dungeon-death", "tests/dungeon-loop"]


def script(package):
    return (SCENARIOS / package / "scripts/outcome.luau").read_text(encoding="utf-8")


class OutcomeScripts(unittest.TestCase):
    def test_death_and_elimination_packages_ship_the_canonical_scripts(self):
        for package in DEATH:
            self.assertEqual(script(package), outcome_scripts.script("death"), package)
        self.assertEqual(script("mob-arena"), outcome_scripts.script("elimination"))

    def test_seal_packages_and_rogue_packages_share_one_script_each(self):
        self.assertEqual({script(package) for package in SEAL}, {script(SEAL[0])})
        self.assertEqual(script("rogue"), script("rogue-exploration"))

    def test_every_run_package_binds_its_outcome(self):
        # Every package that configures a run decides how it ends in a script.
        for package in [*DEATH, *SEAL, "mob-arena", "rogue", "rogue-exploration",
                        "tests/dungeon-characters", "tutorial"]:
            manifest = tomllib.loads((SCENARIOS / package / "scenario.toml").read_text(encoding="utf-8"))
            self.assertNotIn("objective", manifest, package)
            hooks = {hook["on"] for hook in manifest["scripting"]["hooks"]}
            self.assertIn("died", hooks, package)

    def test_apply_replaces_inherited_scripts_and_writes_the_file(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest = {"scripting": {"scripts": ["scripts/other.luau"], "hooks": []}}
            outcome_scripts.apply(manifest, directory, "elimination")
            self.assertEqual(manifest["scripting"]["scripts"], ["scripts/outcome.luau"])
            self.assertEqual(manifest["scripting"]["hooks"],
                             [{"on": "died", "actors": "character", "handler": "outcome.died"}])
            written = (Path(directory) / "scripts/outcome.luau").read_text(encoding="utf-8")
            self.assertEqual(written, outcome_scripts.script("elimination"))
        with self.assertRaises(ValueError):
            outcome_scripts.script("victory")


if __name__ == "__main__":
    unittest.main()
