"""Bounded ordinary package for the projectile application boundary.

Uses production Rogue abilities, expertise and item payloads. Encounter-only
changes remove random equipment birth and give the stationary external target
ample Health and low defense, so this is acceptance, never balance evidence.
"""
import shutil
import subprocess
import tomllib

from arena_matrix import toml_source
from rogue_roster_package import PRODUCTION, encounter


def projectile_package(test, *, short_ranges=False):
    manifest, region = encounter("emu", height=6 if short_ranges else 2)
    manifest["id"] = "projectile-client-acceptance"
    character = manifest["characters"][0]
    character["known_identities"] += ["item.weapon.dagger", "item.weapon.shortbow", "item.weapon.arrow"]
    # Socket 10 is the ordinary secondary weapon socket. Its nonzero Focus
    # price makes cancellation/payment observable with unchanged Rogue rules.
    character["loadout"] = [
        {"archetype": "rogue.dagger", "quantity": 1, "equipped_slot": 10},
        {"archetype": "rogue.shortbow", "quantity": 1},
        {"archetype": "rogue.arrow", "quantity": 6},
    ]
    package = test.directory / "projectile-client-acceptance"
    (package / "regions").mkdir(parents=True)
    (package / "catalog").mkdir()
    for module in manifest["modules"]:
        shutil.copy2(PRODUCTION / module, package / module)
    shutil.copy2(PRODUCTION / "rules.toml", package / "rules.toml")
    species_path = package / "catalog/creatures.toml"
    species = tomllib.loads(species_path.read_text(encoding="utf-8"))
    species["creatures"]["species"]["rogue.emu"]["modifiers"] = [
        {"scope": "always", "op": {"kind": "add", "stat": {"kind": "health"}, "value": 100}},
        {"scope": "always", "op": {"kind": "add", "stat": {"kind": "defense", "defense": "physical"}, "value": -100}},
    ]
    species_path.write_text(toml_source(species), encoding="utf-8", newline="\n")
    items_path = package / "catalog/items.toml"
    items = tomllib.loads(items_path.read_text(encoding="utf-8"))
    for name in ("rogue.dagger", "rogue.shortbow"):
        items["items"][name].pop("equipment_generation")
    if short_ranges:
        # Keep the inside and outside boundaries of both deliveries in sight.
        dagger = items["items"]["rogue.dagger"]["equipment"]["weapon_profile"]
        thrown = next(trait for trait in dagger["expert_traits"]["add"] if trait["kind"] == "thrown")
        thrown["range"] = {"short_feet": 5, "long_feet": 10}
        bow = items["items"]["rogue.shortbow"]["equipment"]["weapon_profile"]
        bow["launcher"]["range"] = {"short_feet": 10, "long_feet": 15}
    items_path.write_text(toml_source(items), encoding="utf-8", newline="\n")
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    (package / "regions/1.toml").write_text(toml_source(region), encoding="utf-8", newline="\n")
    result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                            capture_output=True, text=True, encoding="utf-8", timeout=30)
    test.assertEqual(result.returncode, 0, result.stderr)
    return package
