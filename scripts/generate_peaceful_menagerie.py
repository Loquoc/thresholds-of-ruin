"""Derive the peaceful 3D viewing hall from the current Rogue catalog.

The hall keeps every Rogue mob's catalog body, model and abilities, but makes
every species passive, lets the visitor see invisible creatures and presents
the xeroc openly. Catalog and rules files are copies of
`scenarios/rogue-exploration` with exact, checked edits. The manifest and hall
region are authored here. Rerun after Rogue catalog changes, then validate:

    python scripts/generate_peaceful_menagerie.py
    target/debug/tor-scenario validate scenarios/peaceful-menagerie
"""
import re
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ROGUE = ROOT / "scenarios/rogue-exploration"
OUT = ROOT / "scenarios/peaceful-menagerie"
CATALOG = ("actors", "bodies", "creatures", "items")
GUISE_TALENTS = ("rogue.object_guise", "rogue.ambush_from_guise")
SEE_INVISIBLE = (
    "# The visitor can view the naturally invisible phantom.\n"
    'modifiers = [{ scope = "always", op = { kind = "perception", '
    'value = { kind = "see_invisible", enabled = true } } }]\n'
)
# One place per mob, in a 6-wide grid that leaves the visitor's start clear.
COLUMNS = (8, 11, 14, 18, 21, 24)
ROWS = (6, 9, 12, 15, 18)
START = (16, 12)


def read(path):
    return path.read_text(encoding="utf-8").replace("\r\n", "\n")


def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(text.encode("utf-8"))


def peaceful_creatures(text):
    passive, count = re.subn(r'disposition = "[a-z_]+"', 'disposition = "passive"', text)
    if count == 0:
        raise SystemExit("Rogue creatures declare no dispositions to make passive")
    header = '[creatures.species."rogue.human"]\n'
    if passive.count(header) != 1:
        raise SystemExit("expected exactly one rogue.human species table")
    human = tomllib.loads(passive)["creatures"]["species"]["rogue.human"]
    if "modifiers" in human:
        raise SystemExit("rogue.human already declares modifiers; merge see_invisible by hand")
    return passive.replace(header, header + SEE_INVISIBLE)


def open_xeroc(text):
    lines = []
    found = []
    for line in text.split("\n"):
        talent = next(
            (talent for talent in GUISE_TALENTS
             if line.startswith("value = { ") and f'id = "{talent}"' in line),
            None,
        )
        if talent is not None:
            cut = line.find(", modifiers = [")
            if cut < 0 or not line.endswith("] }"):
                raise SystemExit(f"unexpected {talent} talent shape")
            lines.append("# The viewing hall presents the xeroc openly and without a touch ambush.")
            line = line[:cut] + ", modifiers = [] }"
            found.append(talent)
        lines.append(line)
    if sorted(found) != sorted(GUISE_TALENTS):
        raise SystemExit(f"expected each guise talent once, found {found}")
    return "\n".join(lines)


def toml_string_list(values):
    return "[" + ", ".join(f'"{value}"' for value in values) + "]"


def manifest(rogue, actors):
    character = rogue["characters"][0]
    creature = character["creature"]
    assets = sorted({*rogue["assets"]["dungeon"], *(actor["asset"] for actor in actors.values())})
    return f'''# All catalog mobs share a peaceful, fully lit viewing hall.
format = {rogue["format"]}
id = "peaceful-menagerie"
version = "1.0"
ruleset = "rules.toml"
modules = {toml_string_list(f"catalog/{name}.toml" for name in CATALOG)}
default_character = 1
themes = ["dungeon"]
factions = {{ delvers = [], dungeon_foes = [] }}
ai_profiles = {{ wander = {{ care = false, equipment = false, loot = false, flee_percent = 0 }} }}

[[characters]]
asset = "{character["asset"]}"
id = 1
anchor = "1/start"
turn_ticks = {character["turn_ticks"]}
unselected = "omit"
body = {{ cells = [[0, 0, 0], [0, 0, 1]], eye = [0, 0, 1], mass = 80 }}
known_identities = {toml_string_list(character["known_identities"])}
loadout = []

[characters.creature]
body_package = "{creature["body_package"]}"
species = "{creature["species"]}"
name = "Visitor"
corpse_identity = "{creature["corpse_identity"]}"
faction = "delvers"
binding = "{creature["binding"]}"
starting_skills = {toml_string_list(creature["starting_skills"])}
restoration = "{creature["restoration"]}"
nutrition = {{ kind = "exempt" }}
future_growth = {{ kind = "selected", source = {{ kind = "class", class = "warrior" }} }}
hit_dice = [{{ source = "racial", purchases = [] }}, {{ source = "warrior", purchases = [] }}]

[assets]
dungeon = {toml_string_list(assets)}
'''


def region(actors):
    cells = [(x, y) for y in ROWS for x in COLUMNS if (x, y) != START]
    if len(cells) < len(actors):
        raise SystemExit(f"the hall has {len(cells)} places for {len(actors)} mobs")
    placed = ", ".join(
        f'{{ id = {number}, at = [{x}, {y}, 0], archetype = "{archetype}", controller = "ai", ai = "wander" }}'
        for number, ((x, y), archetype) in enumerate(zip(cells, sorted(actors)), start=2)
    )
    return f'''lit = true
id = 1
name = "Peaceful Menagerie"
size = [32, 25, 4]
chamber = true
gravity = [0, 0, -1]
anchors = {{ start = [{START[0]}, {START[1]}, 0] }}
places = [{{ at = [{START[0]}, {START[1]}, 0], name = "Peaceful viewing hall" }}]
actors = [{placed}]
'''


def main():
    rogue = tomllib.loads(read(ROGUE / "scenario.toml"))
    actors = tomllib.loads(read(ROGUE / "catalog/actors.toml"))["actors"]
    for name in CATALOG:
        text = read(ROGUE / f"catalog/{name}.toml")
        write(OUT / f"catalog/{name}.toml", peaceful_creatures(text) if name == "creatures" else text)
    write(OUT / "rules.toml", open_xeroc(read(ROGUE / "rules.toml")))
    write(OUT / "scenario.toml", manifest(rogue, actors))
    write(OUT / "regions/1.toml", region(actors))
    print(f"wrote {OUT.relative_to(ROOT)} with {len(actors)} mobs")


if __name__ == "__main__":
    main()
