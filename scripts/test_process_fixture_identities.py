"""Source-level fixture metadata checks; compiler execution is deliberately mocked.

Real package qualification remains the process suite's job. These checks prevent
required identity records from disappearing from the wire recorder's helpers.
"""
import copy
from pathlib import Path
import re
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import tomllib
import unittest
from unittest.mock import patch

import linked_hold_package
import nutrition_package
import public_plan_package
import restorable_loss_package
import status_condition_package
import strike_sequence_package
import supply_package
from process_harness import ROOT

KEY = re.compile(r"item(?:\.[a-z][a-z0-9]*(?:-[a-z0-9]+)*){2,}\Z")


class ProcessFixtureIdentities(unittest.TestCase):
    def check_records(self, value):
        if isinstance(value, list):
            for row in value:
                self.check_records(row)
        elif isinstance(value, dict):
            if {"species", "name"} <= value.keys():
                self.assertRegex(value.get("corpse_identity", ""), r"^item\.corpse\.")
            if "identity" in value:
                self.assertRegex(value["identity"], KEY)
                self.assertFalse(value["identity"].endswith(".unknown"))
            for entry in value.get("seed_names", []):
                self.assertEqual(set(entry), {"name", "identity"})
                self.assertRegex(entry["identity"], KEY)
            if "appearances" in value:
                for entry in value["appearances"]:
                    self.assertIsInstance(entry, dict)
                    self.assertIn("name", entry)
            for row in value.values():
                self.check_records(row)

    def test_service_fixtures_have_current_explicit_metadata_without_starting_host(self):
        import discovery_package
        import hallucination_package
        import service_contract_tests
        import theft_service_contracts
        import utility_potions_package
        import utility_wands_package
        builders = [
            service_contract_tests.catalog_fixture,
            service_contract_tests.sequence_rider_fixture,
            service_contract_tests.idle_roaming_fixture,
            discovery_package.fixture,
            discovery_package.observer_fixture,
            hallucination_package.fixture,
            theft_service_contracts.fixture,
            utility_potions_package.fixture,
            utility_wands_package.fixture,
        ]
        for build in builders:
            with self.subTest(builder=build.__module__ + "." + build.__name__):
                self.check_records(build())

    def test_wire_recording_helpers_author_explicit_paired_identity_metadata(self):
        rules = tomllib.loads((ROOT / "crates/simulation/data/default-ruleset.toml").read_text())
        helpers = [
            (nutrition_package, nutrition_package.prepare, {"peer": True, "concealed": True}),
            (supply_package, supply_package.prepare, {}),
            (status_condition_package, status_condition_package.prepare, {}),
            (public_plan_package, public_plan_package.public_plan_package, {}),
            (restorable_loss_package, restorable_loss_package.prepare, {"peer": True}),
            (strike_sequence_package, strike_sequence_package.sequence_package, {"autonomous": False, "natural_source": True}),
        ]
        for module, prepare, options in helpers:
            with self.subTest(helper=module.__name__), TemporaryDirectory() as directory:
                self.directory, self.bin, self.suffix = Path(directory), Path(directory), ""
                with patch.object(module, "load_rules", return_value=copy.deepcopy(rules)), patch.object(
                        module.subprocess, "run", return_value=SimpleNamespace(returncode=0, stderr="")):
                    package = prepare(self, **options)
                for path in package.rglob("*.toml"):
                    if path.name != "rules.toml":
                        self.check_records(tomllib.loads(path.read_text()))
        self.check_records(linked_hold_package.fixture()["manifest"])
        for region in linked_hold_package.fixture()["regions"]:
            self.check_records(region)


if __name__ == "__main__":
    unittest.main()
