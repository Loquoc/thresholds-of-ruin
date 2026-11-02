"""Independent explicit identity-key oracle for Rogue content/fixture tests."""
import json
from pathlib import Path

IDENTITIES = {row["archetype"]: row["new_identity"] for row in json.loads(
    (Path(__file__).parent / "fixtures/rogue-identity-coverage.json").read_text())}
