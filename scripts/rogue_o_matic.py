"""Play the Rogue scenario through the Python player API."""

import argparse
import json
import os

DEFAULT_OBJECTIVE_IDENTITY = "item.amulet.amulet-of-yendor"
DEFAULT_RETRIEVAL_DEPTH = 26


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--connect", default="127.0.0.1:4000")
    parser.add_argument("--actor", type=int, default=1)
    parser.add_argument("--budget", type=int, default=20_000)
    parser.add_argument("--retrieval-depth", type=int, default=DEFAULT_RETRIEVAL_DEPTH)
    parser.add_argument("--objective-identity", default=DEFAULT_OBJECTIVE_IDENTITY)
    args = parser.parse_args()
    if args.retrieval_depth < 1:
        parser.error("Retrieval depth must be positive")
    from rogue_player import Adventurer, play
    from tor_client_bot import Session

    def emit(row):
        print(json.dumps(row), flush=True)

    with Session.connect(args.connect, os.environ["TOR_SERVER_TOKEN"], args.actor) as game:
        player = Adventurer(args.retrieval_depth, args.objective_identity)
        report = play(game, player, args.budget, emit)
        emit(report)
        return 0 if report["terminal"] == "victory" else 1


if __name__ == "__main__":
    raise SystemExit(main())
