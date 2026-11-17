"""Validate targeted contact diagnostic v5; not a replacement for v4."""
import argparse
import json
import math
from pathlib import Path
from model_performance_report import validate, CASES, HAIR_PHASES

RECIPES = (
    ("nymph-slender-walk", True, 0, "walk", [0.0, 0.0, 1.5]),
    ("nymph-sturdy-strike", True, 2, "strike", [0.0, 0.0, 0.0]),
    ("mage-male-walk", False, 1, "walk", [0.0, 0.0, 1.5]),
)
EXTRA = {"suite", "woman", "build", "motion", "velocity"}


def validate_targeted(rows):
    if [(r.get("case"), r.get("sample")) for r in rows] != [
        (recipe[0], sample) for recipe in RECIPES for sample in range(3)
    ]:
        raise ValueError("Missing, duplicated or reordered targeted samples")
    canonical = []
    for row in rows:
        index = next(i for i, recipe in enumerate(RECIPES) if recipe[0] == row["case"])
        _, woman, build, motion, velocity = RECIPES[index]
        if (type(row.get("version")) is not int or row["version"] != 5
                or row.get("suite") != "targeted-contact-v1"
                or type(row.get("woman")) is not bool or row["woman"] != woman
                or type(row.get("build")) is not int or row["build"] != build
                or row.get("motion") != motion
                or type(row.get("velocity")) is not list or len(row["velocity"]) != 3
                or any(type(x) not in (int, float) or not math.isfinite(x) for x in row["velocity"])
                or row["velocity"] != velocity):
            raise ValueError("Targeted recipe identity changed")
        copy = {key: value for key, value in row.items() if key not in EXTRA}
        copy.update(version=4, case=CASES[index])
        canonical.append(copy)
    counts = validate(canonical)
    for row in rows[:6]:
        if any(sum(row[key]) <= 0 for key in HAIR_PHASES):
            raise ValueError("Missing active nymph hair stage")
    return counts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    args = parser.parse_args()
    rows = [json.loads(line) for line in args.input.read_text().splitlines() if line.strip()]
    validate_targeted(rows)
    print(f"Validated {len(rows)} targeted CPU samples; no GPU/FPS claim")


if __name__ == "__main__":
    main()
