"""Validate equipment CPU diagnostic v6, separately from v4/v5 qualification."""
import argparse
import json
import math
from pathlib import Path
from model_performance_report import validate

RECIPES = (
    ("human-plumed-helmet", "plumed-helmet", False, 1, "walk", [0.0, 0.0, 1.5]),
    ("human-padded-gloves", "padded-gloves", True, 0, "strike", [0.0, 0.0, 0.0]),
    ("human-hiking-boots", "hiking-boots", False, 2, "walk", [0.0, 0.0, 1.5]),
)
EXTRA = {"suite", "equipment", "model", "wardrobe", "woman", "build", "motion", "velocity"}


def validate_equipment(rows):
    if [(r.get("case"), r.get("sample")) for r in rows] != [
        (r[0], sample) for r in RECIPES for sample in range(3)
    ]:
        raise ValueError("Missing, duplicated or reordered equipment samples")
    canonical = []
    for index, row in enumerate(rows):
        _, item, woman, build, motion, velocity = RECIPES[index // 3]
        if (type(row.get("version")) is not int or row["version"] != 6
                or row.get("suite") != "equipment-construction-v1"
                or row.get("equipment") != item or row.get("model") != "human"
                or row.get("wardrobe") != "native"
                or type(row.get("woman")) is not bool or row["woman"] != woman
                or type(row.get("build")) is not int or row["build"] != build
                or row.get("motion") != motion
                or type(row.get("velocity")) is not list or len(row["velocity"]) != 3
                or any(type(x) not in (int, float) or not math.isfinite(x) for x in row["velocity"])
                or row["velocity"] != velocity):
            raise ValueError("Equipment recipe identity changed")
        copy = {key: value for key, value in row.items() if key not in EXTRA}
        copy["version"] = 4
        canonical.append(copy)
    return validate(canonical, cases=tuple(r[0] for r in RECIPES), active_hair_cases=())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    args = parser.parse_args()
    rows = [json.loads(line) for line in args.input.read_text(encoding="utf-8-sig").splitlines() if line.strip()]
    validate_equipment(rows)
    print(f"Validated {len(rows)} equipment CPU samples; no GPU/FPS claim")


if __name__ == "__main__":
    main()
