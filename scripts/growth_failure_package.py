"""Shared scenario for exercising growth commit and rollback through real clients."""
import copy
import subprocess
import tomllib

from arena_matrix import toml_source
from process_harness import creature_package


def growth_package(test, *, reject_growth):
    """A valid racial creature whose next selected class changes an ability.

    Neither current creature owns a Mage HD. Only earned growth activates
    its modifier, making the currently harmless non-hostile melee invalid.
    The separate hostile finisher is always valid and has no random check.
    """
    name = "growth-refusal" if reject_growth else "growth-control"
    package = creature_package(test, name, abilities=())
    path = package / "scenario.toml"
    manifest = tomllib.loads(path.read_text(encoding="utf-8"))
    manifest["id"] = name
    manifest["factions"] = {"owner": ["foe"], "foe": ["owner"]}
    owner = manifest["characters"][0]["creature"]
    owner.update(faction="owner", hit_dice=[{"source": "racial"}],
                 future_growth={"kind": "selected", "source": {"kind": "class", "class": "mage"}})
    species = manifest["creatures"]["species"]["human"]
    species["attributes"]["speed"] = 0
    species["behavior"] = {"disposition": "passive", "idle": "wait", "memory_ticks": 1000}
    damage = species["melee"]["strikes"][0]["on_success"][0]["damage"]
    damage["components"][0]["amount"] = {"kind": "fixed", "amount": 0}
    manifest["ai_profiles"]["practice"] = {
        "care": False, "equipment": False, "loot": False, "flee_percent": 0}
    path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")

    path = package / "regions/1.toml"
    region = tomllib.loads(path.read_text(encoding="utf-8"))
    region["actors"][0]["turn_ticks"] = 1000
    region["actors"][0]["creature"].update(faction="foe", future_growth={"kind": "disabled"})
    region["items"] = [
        {"id": 20, "at": [2, 1, 0], "carried_by": 2, "archetype": "token"},
        {"id": 21, "at": [1, 1, 0], "carried_by": 1, "archetype": "tablet"},
    ]
    path.write_text(toml_source(region), encoding="utf-8", newline="\n")

    path = package / "rules.toml"
    rules = tomllib.loads(path.read_text(encoding="utf-8"))
    rules["growth"].update(threshold_base=100, threshold_per_tier=0, reward_base=100,
                           reward_per_relative_tier=0, minimum_reward=100, maximum_reward=100)
    melee = next(row for row in rules["abilities"] if row["id"] == "basic_melee")
    melee.update(interaction="non_hostile",
                 strikes={"kind": "natural_melee", "additional_riders": []})
    finisher = copy.deepcopy(melee)
    finisher.update(id="fixture_growth_finisher", name="Growth finisher", basic_attack=False,
                    interaction="hostile_to_target", reach={"kind": "sight", "range": 6},
                    timing={"source": {"kind": "fixed", "prepare": 1, "recover": 1}},
                    strikes={"kind": "authored", "strikes": [{
                        "offset": 0, "on_failure": [], "descriptors": [], "riders": [],
                        "on_success": [{"kind": "damage", "damage": {
                            "kind": "components", "components": [{"category": "impact",
                                "amount": {"kind": "rolled", "dice": {"count": 1, "sides": 2, "bonus": 1000}}}]}}]}]})
    finisher.pop("cost", None)
    finisher.pop("cooldown", None)
    rules["abilities"].append(finisher)
    rules["constants"]["default_abilities"].append(finisher["id"])
    if reject_growth:
        mage = next(row for row in rules["classes"] if row["id"] == "mage")
        mage["modifiers"].append({"scope": "always", "op": {"kind": "add", "stat": {
            "kind": "ability", "ability": "basic_melee", "stat": "flat_bonus"}, "value": 1}})
    path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
    validated = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                               capture_output=True, text=True, encoding="utf-8", timeout=30)
    test.assertEqual(validated.returncode, 0, validated.stderr)
    return package
