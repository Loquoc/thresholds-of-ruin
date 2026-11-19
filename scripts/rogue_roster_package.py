"""New reconstruction of isolated, normally compiled Rogue roster encounters.

Copies production species, recipes, rules and actual selected-character kit.
Only encounter geometry/control changes. This is not a balance-study runner.
"""
import copy
import shutil
import subprocess
import tomllib

from arena_matrix import toml_source
from process_harness import ROOT
from outcome_scripts import ROGUE_EXPLORATION, copy_declared

PRODUCTION = ROOT / "scenarios/rogue-exploration"
SPECIES = ("bat", "kestrel", "venus_flytrap", "griffin", "emu", "snake", "rattlesnake", "hobgoblin", "zombie", "quagga", "centaur", "yeti", "troll", "wraith",
           "black_unicorn", "ice_monster", "vampire", "jabberwock")
# Retained eighteen-foe static encounter set; production additionally owns L/N/O.
# Theft witnesses register generated floor context separately.
PLAYABLE_SPECIES = SPECIES


def encounter_sources(*, production=None):
    """Read one explicit source; the historical default is rogue-exploration."""
    production = PRODUCTION if production is None else production
    def read(name):
        return tomllib.loads((production / name).read_text(encoding="utf-8"))
    return {"manifest": read("scenario.toml"),
            "actors": read("catalog/actors.toml")["actors"],
            "species": read("catalog/creatures.toml")["creatures"]["species"],
            "body_packages": read("catalog/bodies.toml")["creatures"]["body_packages"],
            "items": read("catalog/items.toml")["items"]}


def encounter(species, *, character=1, autonomous=False, height=2, production=None):
    if type(height) is not int or not 2 <= height <= 8:
        raise ValueError("Encounter height must be an integer in 2..8")
    if species not in SPECIES or type(character) is not int or character not in (1, 2):
        raise ValueError("Choose an authored roster species and Warrior/Mage preset")
    source = encounter_sources(production=production)
    selected = next((row for row in source["manifest"]["characters"] if row["id"] == character), None)
    if selected is None:
        raise ValueError("The selected character is not authored in this source package")
    selected = copy.deepcopy(selected)
    selected["anchor"] = "1/start"
    manifest = {key: copy.deepcopy(source["manifest"][key]) for key in
                ("format", "version", "ruleset", "modules", "factions", "ai_profiles", "themes", "assets")}
    manifest.update(id="rogue-roster-" + species, default_character=character, characters=[selected])
    foe = {"id": 3, "at": [2, 2, 0], "archetype": "rogue." + species,
           "controller": "ai" if autonomous else "external"}
    if autonomous:
        foe["ai"] = "rogue_hostile"
    region = {"id": 1, "name": "Isolated Rogue content encounter", "size": [7, 5, height],
              "chamber": True, "gravity": [0, 0, -1], "anchors": {"start": [1, 2, 0]},
              "actors": [foe]}
    return manifest, region


def roster_package(test, species, *, character=1, autonomous=False, height=2, production=None):
    production = PRODUCTION if production is None else production
    manifest, region = encounter(species, character=character, autonomous=autonomous,
                                 height=height, production=production)
    package = test.directory / ("roster-" + species)
    (package / "regions").mkdir(parents=True)
    (package / "catalog").mkdir()
    for module in manifest["modules"]:
        shutil.copy2(production / module, package / module)
    shutil.copy2(production / "rules.toml", package / "rules.toml")
    copy_declared(manifest, package, ROGUE_EXPLORATION)
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    (package / "regions/1.toml").write_text(toml_source(region), encoding="utf-8", newline="\n")
    compiler = test.bin / ("tor-scenario" + test.suffix)
    result = subprocess.run([compiler, "validate", package], capture_output=True,
                            text=True, encoding="utf-8", timeout=30)
    test.assertEqual(result.returncode, 0, result.stderr)
    return package


def unrelated_fallback(test):
    """Validate unrelated startup content after the actual saved source is gone."""
    package = test.directory / "unrelated-fallback"
    shutil.copytree(ROOT / "scenarios/two-room", package)
    result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                            capture_output=True, text=True, encoding="utf-8", timeout=30)
    test.assertEqual(result.returncode, 0, result.stderr)
    return package
