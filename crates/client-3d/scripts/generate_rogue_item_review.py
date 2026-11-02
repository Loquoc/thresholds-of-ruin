"""Generate bounded real-render review scripts for the complete Rogue item catalog.

Run from any directory. This writes scripts, not screenshots; the built 3D client
renders them with --scene-script SCRIPT --capture NEW_DIRECTORY --offscreen.
Names are explicit diagnostic labels, never a live identity/discovery oracle.
"""
import argparse
import json
from pathlib import Path
import tomllib


def sheet(rows, name):
    return {
        "version": 1, "hz": 50, "viewport": [1920, 1280], "actors": [],
        "ground_items": [
            {"model": model, "position": [(i % 5 - 2) * 2, -0.5, (i // 5 - 1) * 2],
             "yaw": 25, "disclosed_name": label, "quantity": 1}
            for i, (label, model) in enumerate(rows)],
        "ground_labels": True, "floor_color": [0.25, 0.27, 0.30],
        "cameras": [{"id": "sheet", "position": [0, 8.2, 5.3],
                     "target": [0, -0.5, 0], "fov_degrees": 45}],
        "events": [], "captures": [{"tick": 0, "name": name, "camera": "sheet"}],
    }


def reviews(root):
    expected = json.loads((root / "scripts/fixtures/rogue-item-model-coverage.json").read_text())["items"]
    catalog = tomllib.loads((root / "scenarios/rogue/catalog/items.toml").read_text())
    pages, index, forms = {}, [], []
    for identity, variants in expected.items():
        item = catalog["items"][identity]
        for variant, authored in zip(variants, item.get("forms", [item]), strict=True):
            forms.append((authored.get("name", item["name"]), variant["model"], identity, variant["asset"], variant["pool"]))
    for offset in range(0, len(forms), 15):
        name = f"all-forms-{offset // 15 + 1:02}"
        rows = forms[offset:offset + 15]
        pages[name] = sheet([(row[0], row[1]) for row in rows], name)
        index.extend({"page": name, "cell": i, "name": row[0], "model": row[1],
                      "archetype": row[2], "identity": catalog["items"][row[2]]["identity"], "shape_asset": row[3],
                      "public_assets": [entry.get("asset") for entry in catalog["appearance_pools"][row[4]]["appearances"]] if row[4] else [row[3]]} for i, row in enumerate(rows))
    pool_models = {v["pool"]: v["model"] for variants in expected.values() for v in variants if v["pool"]}
    appearances = [(entry["name"], pool_models[key], entry.get("asset")) for key, pool in catalog["appearance_pools"].items()
                   for entry in pool["appearances"]]
    for offset in range(0, len(appearances), 15):
        name = f"appearances-{offset // 15 + 1:02}"
        rows = appearances[offset:offset + 15]
        pages[name] = sheet([(label, model) for label, model, _ in rows], name)
        for item, (_, _, asset) in zip(pages[name]["ground_items"], rows, strict=True):
            item["asset"] = asset
    groups = {
        "weapons": ["Mace", "LongSword", "Dagger", "Shortbow", "Arrow", "Greatsword", "Spear", "Dart", "Shuriken"],
        "armor": ["LeatherArmor", "StuddedLeather", "RingMail", "ScaleMail", "ChainMail", "SplintMail", "BandedMail", "PlateMail"],
        "supplies": ["Bandage", "Ration", "Fruit", "Amulet", "Potion", "Coins", "Scroll", "Wand", "Staff", "Ring", "Torch"],
    }
    for name, kinds in groups.items():
        pages[name] = sheet([(kind, kind) for kind in kinds], name)
    return pages, {"catalog_definitions": len(expected), "true_identity_groups": len({item["identity"] for item in catalog["items"].values()}), "physical_forms": len(forms),
                   "appearance_labels": len(appearances), "unique_recipes": len({row[1] for row in forms}),
                   "forms": index}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="new output directory")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[3]
    pages, index = reviews(root)
    args.output.mkdir(parents=True, exist_ok=False)
    for name, script in pages.items():
        (args.output / f"{name}.json").write_text(json.dumps(script, indent=2) + "\n")
    (args.output / "coverage-index.json").write_text(json.dumps(index, indent=2) + "\n")
    print(json.dumps({key: value for key, value in index.items() if key != "forms"}))


if __name__ == "__main__":
    main()
