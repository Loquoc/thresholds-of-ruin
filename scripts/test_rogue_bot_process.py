"""Ordinary Rogue bot startup against the bundled authored-growth scenario."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

from process_harness import ProcessTestCase, ROOT, TOKEN


class RogueBotProcess(ProcessTestCase):
    def install_bot_extension(self):
        build = os.environ.get("TOR_TEST_BOT_EXTENSION_DIR")
        if build is None:
            profile = os.environ.get("TOR_TEST_PROFILE", "debug")
            target = self.bin.parent / "bot-python"
            command = ["cargo", "build", "--locked", "-p", "tor-client-bot", "--features", "python",
                       "--target-dir", str(target), "--profile", "dev" if profile == "debug" else profile]
            subprocess.run(command, cwd=ROOT, check=True, timeout=600,
                           env={**os.environ, "PYO3_BUILD_EXTENSION_MODULE": "1",
                                "PYO3_PYTHON": sys.executable})
            build = target / profile
        library = "tor_client_bot.dll" if os.name == "nt" else "libtor_client_bot.so"
        extension = "tor_client_bot.pyd" if os.name == "nt" else "tor_client_bot.so"
        shutil.copyfile(Path(build) / library, self.directory / extension)

    def test_rogue_bot_plays_with_authored_growth(self):
        self.install_bot_extension()
        output = self.directory / "cohort"
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic_cohort.py"),
             "--server", str(self.bin / ("tor-server" + self.suffix)),
             "--output", str(output), "--budget", "20", "42"],
            cwd=self.directory, env={**os.environ, "PYTHONPATH": str(self.directory)},
            text=True, capture_output=True, timeout=100,
        )
        errors = (output / "seed-42" / "bot.stderr").read_text(encoding="utf-8")
        self.assertEqual(result.returncode, 0, result.stderr + errors)
        summary = json.loads(result.stdout)
        self.assertEqual(Path(summary["inputs"]["scenario"]), ROOT / "scenarios" / "rogue")
        self.assertEqual(summary["inputs"]["actor"], 1)
        self.assertEqual(summary["inputs"]["seeds"], [42])
        self.assertEqual(summary["inputs"]["retrieval_depth"], 26)
        with (self.bin / ("tor-server" + self.suffix)).open("rb") as binary:
            self.assertEqual(summary["inputs"]["server_sha256"], hashlib.file_digest(binary, "sha256").hexdigest())
        for key, path in (("scenario_rules_sha256", "rules.toml"),
                          ("scenario_validation_sha256", "validation.json")):
            expected = hashlib.sha256((ROOT / "scenarios/rogue" / path).read_bytes()).hexdigest()
            self.assertEqual(summary["inputs"][key], expected)
        hashes = summary["inputs"]["policy_sources_sha256"]
        self.assertEqual(summary["inputs"]["policy_sha256"], hashes["rogue_o_matic.py"])
        self.assertEqual({p.name for p in (output / "policy").glob("*.py")}, hashes.keys())
        for name, digest in hashes.items():
            self.assertEqual(hashlib.sha256((output / "policy" / name).read_bytes()).hexdigest(), digest)
        report, = summary["results"]
        self.assertEqual(report["terminal"], "action_budget")
        self.assertEqual(report["actions"], 20)
        self.assertGreater(report["floors"]["1"]["moves_admitted"], 0)
        self.assertIsNotNone(report["floors"]["1"]["first_reserve"])
        trace = (output / "seed-42" / "turns.jsonl").read_text(encoding="utf-8")
        turns = [json.loads(line) for line in trace.splitlines() if line]
        self.assertNotIn("earned_training", {turn.get("reason") for turn in turns})
        disclosed = [turn["health_recovery"] for turn in turns if "action" in turn]
        self.assertEqual(len(disclosed), 20)
        self.assertTrue(all(int(clock["period_ticks"]) > 0 for clock in disclosed))
        events = [event for turn in turns if "action" in turn
                  for event in turn["events_since_previous_action"]]
        self.assertTrue(any(event["type"] == "ability" for event in events), events)

    def test_bot_does_not_repeat_projectiles_at_a_stalled_tick_in_bundled_seed_47(self):
        self.install_bot_extension()
        output = self.directory / "cohort"
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic_cohort.py"),
             "--server", str(self.bin / ("tor-server" + self.suffix)),
             "--output", str(output), "--budget", "60", "47"],
            cwd=self.directory, env={**os.environ, "PYTHONPATH": str(self.directory)},
            text=True, capture_output=True, timeout=100,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        summary = json.loads(result.stdout)
        report, = summary["results"]
        self.assertGreater(report["tick"], 2920, report)
        trace = (output / "seed-47" / "turns.jsonl").read_text(encoding="utf-8")
        turns = [json.loads(line) for line in trace.splitlines()]
        shots = [row for row in turns if row.get("reason") in {"throw_projectile", "launch_projectile"}
                 and row["result"]["status"] == "confirmed"]
        self.assertTrue(shots, turns)
        ticks = [row["tick"] for row in shots]
        self.assertEqual(len(ticks), len(set(ticks)), shots)

    def test_bot_reports_navigation_when_a_room_has_no_disclosed_exit(self):
        from discovery_package import write_package
        from rogue_o_matic_cohort import trace_report
        from utility_potions_package import fixture

        contents = fixture()
        contents["manifest"]["characters"][0]["loadout"] = []
        region, = contents["regions"]
        region["actors"] = []
        region["walls"] = [[x, y, z] for x, y in ((0, 2), (2, 2), (1, 1), (1, 3)) for z in range(2)]
        server = self.server(scenario=write_package(self, contents, "rogue-bot-no-route"))
        self.install_bot_extension()
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect", self.address,
             "--budget", "50"],
            cwd=self.directory, env={**os.environ, "PYTHONPATH": str(self.directory),
                                     "TOR_SERVER_TOKEN": TOKEN},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        turns = [json.loads(line) for line in result.stdout.splitlines()]
        self.assertEqual(turns[-1]["terminal"], "no_disclosed_route")
        navigation = turns[-1]["navigation"]
        self.assertEqual((navigation["floor"], navigation["visited_cells"]), (1, 1))
        self.assertEqual(navigation["known_down_stairs"], 0)
        searches = sum(row["action"].get("id") == "rogue.search" for row in turns[:-1])
        self.assertEqual(navigation["searches_admitted"], searches)
        self.assertGreater(searches, 0)
        trace = self.directory / "trace.jsonl"
        trace.write_text(result.stdout, encoding="utf-8")
        _, progress = trace_report(trace)
        self.assertEqual(progress["floors"]["1"]["searches_resolved"], searches)
        self.assertEqual(progress["floors"]["1"]["searches_failed"], 0)

    def test_bot_routes_away_from_a_foe_toward_a_disclosed_stair(self):
        self.check_safe_stair_route(diagonal=False)

    def test_bot_uses_a_safe_diagonal_to_a_disclosed_stair(self):
        self.check_safe_stair_route(diagonal=True)

    def check_safe_stair_route(self, *, diagonal):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        manifest["characters"][0]["loadout"] = []
        manifest["stair_pairs"] = {"escape": {"upper": "2/down", "lower": "1/up"}}
        lower, = contents["regions"]
        if diagonal:
            lower["anchors"]["start"] = [2, 2, 0]
        lower["anchors"]["up"] = [1, 1, 0] if diagonal else [2, 1, 0]
        lower["actors"] = [{"archetype": "rogue.griffin", "id": 3, "controller": "external",
                            "at": [4, 2, 0], "turn_ticks": 1000000}]
        contents["regions"].append({"id": 2, "name": "Upper floor", "size": [5, 5, 2],
                                    "chamber": True, "anchors": {"down": [2, 2, 0]}})
        server = self.server(scenario=write_package(self, contents, "rogue-bot-safe-stair-route"))
        self.install_bot_extension()
        code = """
import os
import time
from rogue_o_matic import Policy, position
from tor_client_bot import Session

address, token = os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"]
session, enemy = (Session.connect(address, token, actor) for actor in (1, 3))
try:
    obs = session.observe()
    snapshot = obs.data()
    assert set(snapshot) == {
        "state", "memory", "cursor", "branch", "readiness", "history", "narration",
        "outcome", "intentions", "finished_intentions", "combat_updates", "travel",
    }
    snapshot["state"]["observation"]["visible_cells"].clear()
    picture = obs.data()["state"]
    assert picture["observation"]["visible_cells"]
    policy = Policy()
    policy.floor = 2
    action, reason = policy.choose(picture)
    diagonal = os.environ["BOT_TEST_DIAGONAL"] == "1"
    direction = "north_west" if diagonal else "north"
    assert (action, reason) == ({"type": "move", "direction": direction}, "retreat_to_stairs"), (action, reason)
    offset = (-1, -1, 0) if diagonal else (0, -1, 0)
    goal = next(c["key"] for c in picture["observation"]["visible_cells"] if position(c) == offset)
    assert session.act(action, obs)["status"] == "confirmed"
    assert enemy.act({"type": "wait"}, enemy.observe())["status"] == "confirmed"
    for _ in range(150):
        frame = session.observe().data()
        if frame["readiness"]["admission"] and any(
                c["key"] == goal and position(c) == (0, 0, 0)
                for c in frame["state"]["observation"]["visible_cells"]):
            break
        time.sleep(0.02)
    else:
        raise AssertionError("The safe stair step did not settle")
finally:
    enemy.close()
    session.close()
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address, "BOT_TEST_DIAGONAL": str(int(diagonal))},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bot_learns_strength_loss_from_live_venomous_bite(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        contents["regions"][0]["actors"] = [
            {"archetype": "rogue.rattlesnake", "id": 3, "controller": "external",
             "at": [2, 2, 0], "turn_ticks": 100},
        ]
        server = self.server(scenario=write_package(self, contents, "rogue-bot-observed-strength-loss"))
        self.install_bot_extension()
        code = '''
import os

from rogue_o_matic import Policy, use
from tor_client_bot import Session

address, token = os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"]
player, snake = (Session.connect(address, token, actor) for actor in (1, 3))
try:
    policy = Policy()
    for _ in range(30):
        current = player.observe()
        data = current.data()
        view = data["state"]["observation"]
        policy.observe_combat(view, data["combat_updates"])
        if view["combat"]["own_stats"]["restorable_losses"]["attributes"]["strength"] > 0:
            assert ("name", "rattlesnake") in policy.lasting_injury_foes, (
                policy.lasting_injury_foes, data["combat_updates"], view["combat"]["actors"],
            )
            break
        enemy = snake.observe()
        if enemy.data()["readiness"]["admission"]:
            actors = enemy.data()["state"]["observation"]["visible_actors"]
            target = next(actor["id"] for actor in actors if actor["name"] == "Warrior")
            assert snake.act(use("rogue.venomous_bite", target), enemy)["status"] == "confirmed"
        if current.data()["readiness"]["admission"]:
            result = player.act({"type": "wait"}, current)
            assert result["status"] in {"confirmed", "stale_observation"}, result
    else:
        raise AssertionError("No lasting strength loss observed")
finally:
    snake.close()
    player.close()
'''
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_occupied_paired_stair_uses_a_nearest_open_landing(self):
        from discovery_package import write_package

        contents = {
            "manifest": {
                "format": 37, "id": "occupied-paired-stair", "version": "1.0",
                "ruleset": "rules.toml", "default_character": 1,
                "characters": [{"id": 1, "anchor": "1/stair", "turn_ticks": 100}],
                "stair_pairs": {"first": {"upper": "1/stair", "lower": "2/up"}},
            },
            "rules": (ROOT / "scenarios/two-room/rules.toml").read_text(encoding="utf-8"),
            "regions": [
                {"id": 1, "name": "Upper floor", "size": [5, 5, 1],
                 "chamber": True, "anchors": {"stair": [1, 1, 0]}},
                {"id": 2, "name": "Lower floor", "size": [5, 5, 1],
                 "chamber": True, "anchors": {"up": [2, 2, 0]},
                 "actors": [{"id": 3, "at": [2, 2, 0], "turn_ticks": 100}]},
            ],
        }
        server = self.server(scenario=write_package(self, contents, "occupied-paired-stair"))
        self.install_bot_extension()
        code = """
import os
import time
from tor_client_bot import Session

def view(session):
    return session.observe().data()["state"]["observation"]

def underfoot(observation):
    return next(cell for cell in observation["visible_cells"]
                if cell["position"] == {"x": 0, "y": 0, "z": 0})

address, token = os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"]
player, blocker = (Session.connect(address, token, actor) for actor in (1, 3))
try:
    start = underfoot(view(player))
    anchor = underfoot(view(blocker))
    assert start["stairs_down"] and anchor["stairs_up"], (start, anchor)
    outcome = player.act({"type": "move", "direction": "down"}, player.observe())
    assert outcome["status"] == "confirmed", outcome
    # Confirmation acknowledges admission. The move itself can still be queued.
    for _ in range(100):
        after = view(player)
        if underfoot(after)["key"] != start["key"]:
            break
        time.sleep(0.02)
    else:
        raise AssertionError(("Stair traversal did not settle", after["tick"], underfoot(after)))
    projected = next(cell for cell in after["visible_cells"] if cell["stairs_up"])
    x, y, z = (projected["position"][axis] for axis in ("x", "y", "z"))
    assert abs(x) + abs(y) == 1 and z == 0, projected
    assert underfoot(view(blocker))["key"] == anchor["key"]
finally:
    blocker.close()
    player.close()
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "TOR_SERVER_TOKEN": TOKEN, "BOT_TEST_ADDRESS": self.address},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bot_moves_to_a_previously_visited_cell_before_recovering(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        contents["regions"][0]["actors"] = []
        manifest = contents["manifest"]
        manifest["archetypes"]["injury"] = {
            "class": "potion", "identity": "item.potion.fixture.injury", "name": "injury potion",
            "consumable": {"effects": [{"kind": "damage", "damage": {
                "kind": "components", "components": [
                    {"category": "vital", "amount": {"kind": "fixed", "amount": 6}}
                ],
            }}]},
        }
        character, = manifest["characters"]
        character["loadout"] = [{"archetype": "injury", "quantity": 1}]
        server = self.server(scenario=write_package(self, contents, "rogue-bot-recovery-position"))
        self.install_bot_extension()
        code = """
import os
import time
from rogue_o_matic import Policy, position
from tor_client_bot import Session

def ready(session, when=lambda state: True):
    for _ in range(100):
        obs = session.observe()
        data = obs.data()
        if data["readiness"]["admission"] and when(data["state"]):
            return obs, data["state"]
        time.sleep(0.02)
    raise AssertionError("No ready player boundary")

def underfoot(state):
    return next(cell["key"] for cell in state["observation"]["visible_cells"]
                if position(cell) == (0, 0, 0))

session = Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1)
try:
    policy = Policy()
    obs, initial = ready(session)
    policy.choose(initial)
    origin = underfoot(initial)
    move = {"type": "move", "direction": "east"}
    assert session.act(move, obs)["status"] == "confirmed"
    obs, after_move = ready(session, lambda state: underfoot(state) != origin)
    exposed = underfoot(after_move)
    item, = after_move["observation"]["inventory"]
    dose = {"type": "drink", "item": item["id"]}
    assert session.act(dose, obs)["status"] == "confirmed"
    obs, injured = ready(session, lambda state: not state["observation"]["inventory"])
    view = injured["observation"]
    assert int(view["combat"]["hp"]) * 5 < int(view["combat"]["max_hp"]) * 4, view["combat"]
    memory = policy.floors[policy.floor]
    memory.recent_danger[exposed] = int(view["tick"]) + 1200
    action, reason = policy.choose(injured)
    assert (action, reason) == ({"type": "move", "direction": "west"}, "reposition_to_recover"), (action, reason)
    assert session.act(action, obs)["status"] == "confirmed"
    _, safer = ready(session, lambda state: underfoot(state) == origin)
    assert policy.choose(safer) == ({"type": "wait"}, "recover")
finally:
    session.close()
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bot_can_open_distance_with_a_clear_diagonal(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        contents["regions"][0]["actors"] = [
            {"archetype": "rogue.wraith", "id": 3, "controller": "external",
             "at": [1, 3, 0], "turn_ticks": 1000000},
        ]
        contents["manifest"]["characters"][0]["loadout"] = []
        contents["manifest"]["stair_pairs"] = {"escape": {"upper": "2/upper", "lower": "1/up"}}
        contents["regions"][0]["anchors"]["up"] = [1, 1, 0]
        contents["regions"].append({"id": 2, "name": "Upper floor", "size": [5, 5, 2],
                                    "chamber": True, "anchors": {"upper": [2, 2, 0]}})
        server = self.server(scenario=write_package(self, contents, "rogue-bot-diagonal-retreat"))
        self.install_bot_extension()
        code = """
import os
import time
from rogue_o_matic import Policy, position
from tor_client_bot import Session

session = Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1)
try:
    obs = session.observe()
    state = obs.data()["state"]
    view = state["observation"]
    assert any(actor["name"].lower() == "wraith" and position(actor) == (0, 1, 0)
               for actor in view["visible_actors"]), view["visible_actors"]
    assert any(c["stairs_up"] and position(c) == (0, -1, 0) for c in view["visible_cells"])
    policy = Policy()
    policy.floor = 11
    action, reason = policy.choose(state)
    assert action["type"] == "move" and action["direction"] in {"north_east", "north_west"}, (action, reason)
    assert reason == "retreat_to_stairs", reason
    dx = 1 if action["direction"] == "north_east" else -1
    target = next(cell["key"] for cell in view["visible_cells"] if position(cell) == (dx, -1, 0))
    outcome = session.act(action, obs)
    assert outcome["status"] == "confirmed", outcome
    for _ in range(100):
        after = session.observe().data()["state"]["observation"]
        if any(cell["key"] == target and position(cell) == (0, 0, 0) for cell in after["visible_cells"]):
            break
        time.sleep(0.02)
    else:
        raise AssertionError(("Retreat did not settle", after["tick"]))
finally:
    session.close()
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bot_searches_the_nearby_site_with_more_disclosed_boundaries(self):
        from discovery_package import fixture, write_package

        contents = fixture()
        region, = contents["regions"]
        region.update(size=[9, 5, 2], anchors={"start": [4, 2, 0]}, doors=[], traps=[],
                      walls=[[x, y, z] for x, y in
                             ((2, 1), (3, 0), (3, 1), (5, 0), (5, 1), (5, 3), (5, 4), (6, 1), (6, 3))
                             for z in range(2)])
        server = self.server(scenario=write_package(self, contents, "rogue-bot-search-coverage"))
        self.install_bot_extension()
        code = """
import os
import time
from rogue_o_matic import Floor, Policy, position
from tor_client_bot import Session

session = Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1)
try:
    obs = session.observe()
    state = obs.data()["state"]
    cells = {position(c): c for c in state["observation"]["visible_cells"]}
    here, east, west = (cells[(x, 0, 0)]["key"] for x in (0, 1, -1))
    memory = Floor(visited={c["key"] for c in cells.values() if not c["wall"]})
    memory.tactile_attempts.update(dict.fromkeys(memory.visited, 4))
    memory.rejected_edges.update((here, direction) for direction in
                                 ("north", "south", "north_east", "north_west", "south_east", "south_west"))
    policy = Policy()
    policy.floors[1] = memory
    action, reason = policy.choose(state)
    assert len(memory.boundaries[east]) == 2 and len(memory.boundaries[west]) == 1, memory.boundaries
    assert (action, reason) == ({"type": "move", "direction": "east"}, "search"), (action, reason)
    outcome = session.act(action, obs)
    assert outcome["status"] == "confirmed", outcome
    for _ in range(100):
        obs = session.observe()
        data = obs.data()
        view = data["state"]["observation"]
        if data["readiness"]["admission"] and any(
                c["key"] == east and position(c) == (0, 0, 0) for c in view["visible_cells"]):
            break
        time.sleep(0.02)
    else:
        raise AssertionError(("Search route did not settle", view["tick"]))
    memory.visited.update(c["key"] for c in view["visible_cells"] if not c["wall"])
    memory.tactile_attempts.update(dict.fromkeys(memory.visited, 4))
    search, reason = policy.choose(data["state"])
    assert search.get("id") == "rogue.search" and reason == "search", (search, reason)
    assert session.act(search, obs)["status"] == "confirmed"
    policy.accepted(search, view, reason=reason)
    tick = int(view["tick"])
    for _ in range(100):
        obs = session.observe()
        data = obs.data()
        view = data["state"]["observation"]
        if data["readiness"]["admission"] and view["ready"] and int(view["tick"]) > tick:
            break
        time.sleep(0.02)
    else:
        raise AssertionError(("Search did not settle", view["tick"]))
    near_walls = set(memory.boundaries[east].values())
    plain_walls = near_walls - memory.joining_wall_hints.keys()
    assert plain_walls, memory.joining_wall_hints
    for wall in near_walls - plain_walls:
        memory.search_attempts[wall] = 8
    memory.search_attempts.update({
        wall: 8 for walls in memory.boundaries.values() for wall in walls.values() if wall not in near_walls
    })
    for wall in memory.boundaries[west].values():
        memory.search_attempts[wall] = 0
    memory.tactile_attempts[east] = 1
    retry, reason = policy.choose(data["state"])
    assert (retry, reason) == (search, "search"), (retry, reason)
    assert session.act(retry, obs)["status"] == "confirmed"
finally:
    session.close()
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bot_cli_retrieves_a_custom_canonical_objective(self):
        from discovery_package import fixture, write_package

        contents = fixture()
        manifest = contents["manifest"]
        identity = "item.amulet.fixture.copper-seal"
        for archetype, canonical in (("fixture.target", identity),
                                     ("fixture.lookalike", "item.amulet.fixture.ceremonial-seal")):
            manifest["archetypes"][archetype] = {
                "class": "amulet", "name": "Copper seal", "identity": canonical,
            }
            manifest["characters"][0]["known_identities"].append(canonical)
        region, = contents["regions"]
        region.update(doors=[], traps=[], items=[
            {"id": 10, "at": [1, 2, 0], "archetype": "fixture.lookalike", "quantity": 1},
            {"id": 11, "at": [1, 2, 0], "archetype": "fixture.target", "quantity": 1},
        ])
        server = self.server(scenario=write_package(self, contents, "rogue-bot-canonical-objective"))
        self.install_bot_extension()
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect", self.address,
             "--objective-identity", identity, "--retrieval-depth", "1", "--budget", "2"],
            cwd=self.directory, env={**os.environ, "PYTHONPATH": str(self.directory),
                                     "TOR_SERVER_TOKEN": TOKEN},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        turns = [json.loads(line) for line in result.stdout.splitlines()]
        self.assertEqual(turns[0]["action"]["type"], "take")
        self.assertEqual(turns[0]["result"]["status"], "confirmed")
        self.assertTrue(turns[-1]["amulet_acquired"], turns)

    def test_bot_eats_server_offered_food_without_a_ration_tag(self):
        from nutrition_package import prepare

        server = self.server(scenario=prepare(self, initial=2, unit_ticks=1000))
        self.install_bot_extension()
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect", self.address,
             "--budget", "2"],
            cwd=self.directory, env={**os.environ, "PYTHONPATH": str(self.directory),
                                     "TOR_SERVER_TOKEN": TOKEN},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        first, second, terminal = (json.loads(line) for line in result.stdout.splitlines())
        self.assertEqual(first["action"]["type"], "eat")
        self.assertEqual(first["result"]["status"], "confirmed")
        self.assertEqual(first["nutrition_reserve"], 2)
        self.assertEqual(second["nutrition_reserve"], 8)
        self.assertEqual(terminal["terminal"], "action_budget")

    def test_bot_picks_up_and_eats_untagged_ground_food(self):
        from nutrition_package import prepare

        server = self.server(scenario=prepare(self, initial=2, unit_ticks=1000, ground_meal=True))
        self.install_bot_extension()
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect", self.address,
             "--budget", "2"],
            cwd=self.directory, env={**os.environ, "PYTHONPATH": str(self.directory),
                                     "TOR_SERVER_TOKEN": TOKEN},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        pickup, eat, terminal = (json.loads(line) for line in result.stdout.splitlines())
        self.assertEqual(pickup["action"]["type"], "take")
        self.assertEqual(pickup["result"]["status"], "confirmed")
        self.assertEqual(eat["action"]["type"], "eat")
        self.assertEqual(eat["result"]["status"], "confirmed")
        self.assertEqual(terminal["terminal"], "action_budget")

    def test_bot_uses_a_renamed_identification_scroll(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        manifest["archetypes"]["rogue.identify_potion"]["name"] = "Lore of draughts"
        character, = manifest["characters"]
        character["known_identities"] = ["item.scroll.identify-potion"]
        character["loadout"] = [{"archetype": item, "quantity": 1}
                                for item in ("rogue.identify_potion", "rogue.extra_healing")]
        contents["regions"][0]["actors"] = []
        server = self.server(scenario=write_package(self, contents, "rogue-bot-rename-identify"))
        self.install_bot_extension()
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect", self.address,
             "--budget", "2"],
            cwd=self.directory, env={**os.environ, "PYTHONPATH": str(self.directory),
                                     "TOR_SERVER_TOKEN": TOKEN},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        read, identify, terminal = (json.loads(line) for line in result.stdout.splitlines())
        self.assertEqual((read["action"]["type"], read["reason"]), ("read", "identify_gear"))
        self.assertEqual(identify["action"]["type"], "identify")
        self.assertEqual(identify["result"]["status"], "confirmed")
        self.assertEqual(terminal["terminal"], "action_budget")

    def test_bot_drinks_a_renamed_growth_potion(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        manifest["archetypes"]["rogue.gain_strength"]["name"] = "Mighty draught"
        character, = manifest["characters"]
        character["known_identities"] = ["item.potion.gain-strength"]
        character["loadout"] = [{"archetype": "rogue.gain_strength", "quantity": 1}]
        contents["regions"][0]["actors"] = []
        server = self.server(scenario=write_package(self, contents, "rogue-bot-rename-growth"))
        self.install_bot_extension()
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect", self.address,
             "--budget", "1"],
            cwd=self.directory, env={**os.environ, "PYTHONPATH": str(self.directory),
                                     "TOR_SERVER_TOKEN": TOKEN},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        drink, terminal = (json.loads(line) for line in result.stdout.splitlines())
        self.assertEqual((drink["action"]["type"], drink["reason"]), ("drink", "permanent_growth"))
        self.assertEqual(drink["result"]["status"], "confirmed")
        self.assertEqual(terminal["terminal"], "action_budget")


    def test_bot_equips_a_renamed_searching_ring(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        manifest["archetypes"]["rogue.ring_searching"]["name"] = "Discovery band"
        character, = manifest["characters"]
        character["known_identities"] = ["item.ring.ring-searching"]
        character["loadout"] = [{"archetype": "rogue.ring_searching", "quantity": 1}]
        contents["regions"][0]["actors"] = []
        server = self.server(scenario=write_package(self, contents, "rogue-bot-rename-ring"))
        self.install_bot_extension()
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect", self.address,
             "--budget", "1"],
            cwd=self.directory, env={**os.environ, "PYTHONPATH": str(self.directory),
                                     "TOR_SERVER_TOKEN": TOKEN},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        equip, terminal = (json.loads(line) for line in result.stdout.splitlines())
        self.assertEqual((equip["action"]["type"], equip["reason"]), ("equip", "upgrade_equipment"))
        self.assertEqual(equip["result"]["status"], "confirmed")
        self.assertEqual(terminal["terminal"], "action_budget")


    def test_bot_explores_before_reading_a_concealed_teleportation_scroll(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        contents["manifest"]["characters"][0]["loadout"] = [
            {"archetype": "rogue.teleportation", "quantity": 1},
        ]
        contents["regions"][0]["actors"] = []
        server = self.server(scenario=write_package(self, contents, "rogue-bot-scroll-route"))
        self.install_bot_extension()
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect", self.address,
             "--budget", "1"],
            cwd=self.directory, env={**os.environ, "PYTHONPATH": str(self.directory),
                                     "TOR_SERVER_TOKEN": TOKEN},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        first, terminal = (json.loads(line) for line in result.stdout.splitlines())
        self.assertEqual((first["action"]["type"], first["reason"]), ("move", "explore"), first)
        self.assertEqual(first["result"]["status"], "confirmed")
        self.assertGreater(terminal["tick"], first["tick"])

    def test_bot_rechecks_a_corridor_end_before_other_boundaries(self):
        from discovery_package import fixture, write_package

        contents = fixture(dc=100)
        contents["manifest"]["characters"][0]["loadout"] = [
            {"archetype": "rogue.teleportation", "quantity": 1},
        ]
        contents["manifest"]["characters"][0]["creature"]["nutrition"] = {"kind": "required"}
        region, = contents["regions"]
        region["traps"] = []
        region["walls"].extend([x, y, z] for x, y in ((1, 1), (2, 1), (2, 3))
                               for z in range(2))
        server = self.server(scenario=write_package(self, contents, "rogue-bot-corridor-search"))
        self.install_bot_extension()
        code = """
import os
import time
from rogue_o_matic import Floor, Policy, position
from tor_client_bot import Session

def ready(session, condition):
    for _ in range(150):
        obs = session.observe()
        data = obs.data()
        if data["readiness"]["admission"] and condition(data["state"]["observation"]):
            return obs, data["state"]
        time.sleep(0.02)
    raise AssertionError("Search fixture did not reach the expected boundary")

session = Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1)
try:
    obs, state = ready(session, lambda view: True)
    cells = {position(c): c for c in state["observation"]["visible_cells"]}
    end = cells[(1, 0, 0)]["key"]
    memory = Floor(visited={c["key"] for c in cells.values() if not c["wall"]})
    memory.tactile_attempts.update(dict.fromkeys(memory.visited, 4))
    memory.search_attempts.update(cells[pos]["key"] for pos in ((1, -1, 0), (1, 1, 0), (2, 0, 0)))
    policy = Policy()
    policy.floors[1] = memory
    step, reason = policy.choose(state)
    assert memory.corridor_end(end), (memory.boundaries.get(end), memory.graph.edges.get(end),
                                      [(pos, cells[pos]['wall'], cells[pos]['door'])
                                       for pos in ((1, -1, 0), (1, 1, 0), (2, 0, 0))])
    assert (step, reason) == ({"type": "move", "direction": "east"}, "search"), (step, reason)
    assert session.act(step, obs)["status"] == "confirmed"
    obs, state = ready(session, lambda view: any(c["key"] == end and position(c) == (0, 0, 0)
                                                for c in view["visible_cells"]))
    memory.visited.update(c["key"] for c in state["observation"]["visible_cells"] if not c["wall"])
    search, reason = policy.choose(state)
    assert search.get("id") == "rogue.search" and reason == "search", (search, reason)
    assert session.act(search, obs)["status"] == "confirmed"
    tick = int(state["observation"]["tick"])
    obs, state = ready(session, lambda view: int(view["tick"]) > tick)
    view = state["observation"]
    assert view["combat"]["own_stats"]["nutrition"]["hunger"] == "nourished"
    policy.choose(state)
    memory.visited.update(memory.graph.edges)
    memory.tactile_attempts.update(dict.fromkeys(memory.visited, 4))
    memory.search_attempts.update({wall: 8 for edges in memory.boundaries.values() for wall in edges.values()})
    wall = min(memory.boundaries[end].values())
    memory.search_attempts[wall] = 3
    action, reason = policy.choose(state)
    assert action["type"] == "read" and reason == "test_unknown_magic", (action, reason)
    assert session.act(action, obs)["status"] == "confirmed"
    tick = int(view["tick"])
    ready(session, lambda view: int(view["tick"]) > tick)
finally:
    session.close()
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bot_uses_a_two_step_escape_against_a_learned_damage_threat(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        region, = contents["regions"]
        region.update(anchors={"start": [2, 2, 0]},
                      walls=[[x, y, z] for x, y in ((2, 1), (3, 2), (1, 1), (3, 3))
                             for z in range(2)],
                      actors=[{"archetype": "rogue.hobgoblin", "id": 3, "controller": "external",
                               "at": [2, 3, 0], "turn_ticks": 1000000}])
        server = self.server(scenario=write_package(self, contents, "rogue-bot-learned-threat-retreat"))
        self.install_bot_extension()
        code = """
import os
import time
from rogue_o_matic import Policy, position
from tor_client_bot import Session

session = Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1)
try:
    obs = session.observe()
    state = obs.data()["state"]
    view = state["observation"]
    target = next(a["id"] for a in view["visible_actors"]
                  if a["name"].lower() == "hobgoblin" and position(a) == (0, 1, 0))
    west = next(c["key"] for c in view["visible_cells"] if position(c) == (-1, 0, 0))
    policy = Policy()
    hp = int(view["combat"]["hp"])
    assert hp * 4 > int(view["combat"]["max_hp"])
    policy.largest_hits[("actor", target)] = (hp + 1) // 2
    step, reason = policy.choose(state)
    assert (step, reason) == ({"type": "move", "direction": "west"}, "break_contact"), (step, reason)
    result = session.act(step, obs)
    assert result["status"] == "confirmed", result
    for _ in range(150):
        after = session.observe().data()["state"]["observation"]
        if any(c["key"] == west and position(c) == (0, 0, 0) for c in after["visible_cells"]):
            break
        time.sleep(0.02)
    else:
        raise AssertionError(("Retreat did not settle", after["tick"]))
finally:
    session.close()
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bot_moves_when_an_invisible_enemy_attacks(self):
        self.check_unseen_attack(use_sight=False)

    def test_bot_reveals_an_invisible_attacker_with_a_known_potion(self):
        self.check_unseen_attack(use_sight=True)

    def test_cohort_attributes_a_live_unseen_attack_as_the_cause_of_death(self):
        self.check_unseen_attack(use_sight=False, let_die=True)

    def check_unseen_attack(self, *, use_sight, let_die=False):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        region, = contents["regions"]
        region["actors"][0].update(at=[2, 2, 0], turn_ticks=100)
        character = contents["manifest"]["characters"][0]
        character["loadout"] = [{"archetype": "rogue.see_invisible", "quantity": 1}] if use_sight else []
        if use_sight:
            character["known_identities"].append("item.potion.see-invisible")
        server = self.server(scenario=write_package(self, contents, "rogue-bot-unseen-attack"))
        self.install_bot_extension()
        code = """
import os
import time
from rogue_o_matic import Policy, position, use
from rogue_o_matic_cohort import death_cause
from tor_client_bot import Session

address, token = os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"]
player, phantom = (Session.connect(address, token, actor) for actor in (1, 3))
try:
    policy = Policy()
    let_die = os.environ["BOT_TEST_UNSEEN_DEATH"] == "1"
    for _ in range(120 if let_die else 25):
        obs = player.observe()
        data = obs.data()
        view = data["state"]["observation"]
        policy.observe_combat(view)
        if let_die and view["combat"]["dead"]:
            report = {"self_target": view["self_target"], "last_combat_events": view["combat"]["events"]}
            assert death_cause(report) == "unseen_ability", report
            raise SystemExit(0)
        if not let_die and policy.unseen_danger and data["readiness"]["admission"]:
            break
        enemy = phantom.observe()
        enemy_data = enemy.data()
        if enemy_data["readiness"]["admission"]:
            target = next(a["id"] for a in enemy_data["state"]["observation"]["visible_actors"]
                          if a["name"] == "Warrior")
            result = phantom.act(use("basic_melee", target), enemy)
            assert result["status"] == "confirmed", result
        if data["readiness"]["admission"]:
            result = player.act({"type": "wait"}, obs)
            assert (result["status"] in {"confirmed", "stale_observation"}
                    or result.get("code") == "stale_context"), result
        time.sleep(0.02)
    else:
        raise AssertionError(("No actionable unseen attack", view["tick"], view["combat"]["events"]))
    assert not any(a["name"] == "phantom" for a in view["visible_actors"])
    start = next(c["key"] for c in view["visible_cells"] if position(c) == (0, 0, 0))
    step, reason = policy.choose(data["state"])
    use_sight = os.environ["BOT_TEST_SIGHT"] == "1"
    if use_sight:
        assert step["type"] == "drink" and reason == "reveal_attacker", (step, reason)
    else:
        assert step["type"] == "move" and reason == "evade_unseen_attack", (step, reason)
    result = player.act(step, obs)
    assert result["status"] == "confirmed", result
    policy.accepted(step, view, reason=reason)
    for _ in range(150):
        observation = player.observe()
        settled = observation.data()
        after = settled["state"]["observation"]
        if use_sight and any(a["name"] == "phantom" for a in after["visible_actors"]):
            break
        if not use_sight and settled["readiness"]["admission"] and after["ready"] and any(
                c["key"] != start and position(c) == (0, 0, 0) for c in after["visible_cells"]):
            break
        opponent = phantom.observe()
        status = opponent.data()
        if status["readiness"]["admission"] and status["state"]["observation"]["ready"]:
            waited = phantom.act({"type": "wait"}, opponent)
            assert waited["status"] in {"confirmed", "stale_observation"}, waited
        time.sleep(0.02)
    else:
        raise AssertionError(("Unseen attack response did not settle", reason, after["tick"]))
    if not use_sight:
        assert not any(event["type"] == "unseen_ability" for event in after["combat"]["events"])
        second, reason = policy.choose(settled["state"])
        assert second["type"] == "move" and reason == "evade_unseen_attack", (second, reason)
        assert player.act(second, observation)["status"] == "confirmed"
finally:
    phantom.close()
    player.close()
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address, "BOT_TEST_SIGHT": str(int(use_sight)),
                 "BOT_TEST_UNSEEN_DEATH": str(int(let_die))},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bot_recovers_on_a_visited_route_to_the_stairs(self):
        self.check_recovery_route(food_limited=False)

    def test_bot_descends_if_recovery_would_exhaust_food(self):
        self.check_recovery_route(food_limited=True)

    def test_bot_keeps_exploring_if_recovery_would_exhaust_food(self):
        self.check_recovery_route(food_limited=True, has_stairs=False)

    def test_bot_rests_before_exploring_if_only_the_return_budget_is_short(self):
        self.check_recovery_route(food_limited=False, has_stairs=False)

    def check_recovery_route(self, *, food_limited, has_stairs=True):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        manifest["archetypes"]["injury"] = {
            "class": "potion", "identity": "item.potion.fixture.injury", "name": "injury potion",
            "consumable": {"effects": [{"kind": "damage", "damage": {
                "kind": "components", "components": [
                    {"category": "vital", "amount": {"kind": "fixed", "amount": 5}}
                ],
            }}]},
        }
        manifest["characters"][0]["loadout"] = [{"archetype": "injury", "quantity": 1}]
        manifest["characters"][0]["creature"]["nutrition"] = {"kind": "required"}
        if food_limited:
            contents["rules"] = contents["rules"].replace(
                "initial = 1300, maximum = 2000, hungry_below = 300, weak_below = 150",
                "initial = 15, maximum = 2000, hungry_below = 2, weak_below = 1",
            )
        region, = contents["regions"]
        region["actors"] = []
        if has_stairs:
            manifest["stair_pairs"] = {"first": {"upper": "1/down", "lower": "2/up"}}
            region["anchors"]["down"] = [3, 2, 0]
            contents["regions"].append({"id": 2, "name": "Lower floor", "size": [5, 5, 2],
                                        "chamber": True, "anchors": {"up": [2, 2, 0]}})
        server = self.server(scenario=write_package(self, contents, "rogue-bot-recover-en-route"))
        self.install_bot_extension()
        code = """
import os
import time
from rogue_o_matic import Policy, position
from tor_client_bot import Session

def ready(session, after_tick=-1):
    for _ in range(100):
        obs = session.observe()
        data = obs.data()
        view = data["state"]["observation"]
        if data["readiness"]["admission"] and view["ready"] and int(view["tick"]) > after_tick:
            return obs, data["state"], view
        time.sleep(0.02)
    raise AssertionError(("No settled player boundary", after_tick))

def submit(session, observation, view, action):
    result = session.act(action, observation)
    assert result["status"] == "confirmed", result
    return ready(session, int(view["tick"]))

session = Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1)
try:
    policy = Policy()
    obs, state, view = ready(session)
    policy.choose(state)
    east = {"type": "move", "direction": "east"}
    obs, state, view = submit(session, obs, view, east)
    policy.choose(state)
    obs, state, view = submit(session, obs, view, {"type": "move", "direction": "west"})
    item, = view["inventory"]
    obs, state, view = submit(session, obs, view, {"type": "drink", "item": item["id"]})
    hp, total = (int(view["combat"][name]) for name in ("hp", "max_hp"))
    assert hp * 2 >= total and hp * 5 < total * 4, (hp, total)
    if os.environ["BOT_TEST_STAIRS"] == "0":
        assert view["combat"]["own_stats"]["nutrition"]["hunger"] == "nourished"
        assert not any(cell["stairs_down"] for cell in view["visible_cells"])
        if os.environ["BOT_TEST_FOOD_LIMITED"] == "0":
            policy.floor = 3
            nutrition = view["combat"]["own_stats"]["nutrition"]
            policy.return_travel_ticks[2] = nutrition["reserve"] * int(nutrition["unit_ticks"])
            action, reason = policy.choose(state)
            assert (action, reason) == ({"type": "wait"}, "recover"), (action, reason)
            _, _, after = submit(session, obs, view, action)
            assert int(after["tick"]) > int(view["tick"])
            start = next(c["key"] for c in view["visible_cells"] if position(c) == (0, 0, 0))
            assert any(c["key"] == start and position(c) == (0, 0, 0) for c in after["visible_cells"])
            raise SystemExit(0)
        action, reason = policy.choose(state)
        assert action["type"] == "move" and reason == "recover_en_route", (action, reason)
        before = int(view["combat"]["own_stats"]["health_recovery"]["elapsed_ticks"])
        _, _, after = submit(session, obs, view, action)
        assert int(after["combat"]["own_stats"]["health_recovery"]["elapsed_ticks"]) > before
    elif os.environ["BOT_TEST_FOOD_LIMITED"] == "1":
        policy.recovering = True
        move = {"type": "move", "direction": "east"}
        obs, state, view = submit(session, obs, view, move)
        obs, state, view = submit(session, obs, view, move)
        assert view["combat"]["own_stats"]["nutrition"]["hunger"] == "nourished"
        action, reason = policy.choose(state)
        assert (action, reason) == ({"type": "move", "direction": "down"}, "descend"), (action, reason)
        _, _, below = submit(session, obs, view, action)
        assert any(c["stairs_up"] and position(c) == (0, 0, 0) for c in below["visible_cells"])
    else:
        before = int(view["combat"]["own_stats"]["health_recovery"]["elapsed_ticks"])
        assert policy.choose(state) == (east, "recover_en_route")
        obs, state, view = submit(session, obs, view, east)
        assert int(view["combat"]["own_stats"]["health_recovery"]["elapsed_ticks"]) > before
        assert policy.choose(state) == (east, "recover_en_route")
        obs, state, view = submit(session, obs, view, east)
        assert any(c["stairs_down"] and position(c) == (0, 0, 0) for c in view["visible_cells"])
        assert policy.choose(state) == ({"type": "wait"}, "recover")
        policy.floor = 3
        nutrition = view["combat"]["own_stats"]["nutrition"]
        policy.return_travel_ticks[2] = nutrition["reserve"] * int(nutrition["unit_ticks"])
        action, reason = policy.choose(state)
        assert (action, reason) == ({"type": "move", "direction": "down"}, "descend"), (action, reason)
        _, _, below = submit(session, obs, view, action)
        assert any(c["stairs_up"] and position(c) == (0, 0, 0) for c in below["visible_cells"])
finally:
    session.close()
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address, "BOT_TEST_FOOD_LIMITED": str(int(food_limited)),
                 "BOT_TEST_STAIRS": str(int(has_stairs))},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bot_heals_above_half_health_when_cornered_by_a_lethal_threat(self):
        self.check_cornered_policy(unknown_scrolls=False)

    def test_bot_attacks_after_one_unidentified_scroll_in_a_fight(self):
        self.check_cornered_policy(unknown_scrolls=True)

    def check_cornered_policy(self, *, unknown_scrolls):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        manifest["archetypes"]["rogue.healing"]["name"] = "Restorative draught"
        manifest["archetypes"]["injury"] = {
            "class": "potion", "identity": "item.potion.fixture.injury", "name": "injury potion",
            "consumable": {"effects": [{"kind": "damage", "damage": {"kind": "components",
                "components": [{"category": "vital", "amount": {"kind": "fixed", "amount": 5}}],
            }}]},
        }
        character, = manifest["characters"]
        gear = ("rogue.enchant_armor", "rogue.protect_armor") if unknown_scrolls else ("rogue.healing",)
        character["loadout"] = [{"archetype": name, "quantity": 1} for name in ("injury", *gear)]
        character["known_identities"].extend(("item.potion.fixture.injury", "item.potion.healing"))
        region, = contents["regions"]
        region.update(walls=[[x, y, z] for x, y in ((0, 2), (1, 1), (1, 3)) for z in range(2)],
                      actors=[{"archetype": "rogue.hobgoblin", "id": 3, "controller": "external",
                               "at": [2, 2, 0], "turn_ticks": 1000000}])
        server = self.server(scenario=write_package(self, contents, "rogue-bot-corner-healing"))
        self.install_bot_extension()
        code = """
import os
import time
from rogue_o_matic import Policy
from tor_client_bot import Session

def ready(session, after_tick=-1):
    for _ in range(150):
        obs = session.observe()
        data = obs.data()
        view = data["state"]["observation"]
        if data["readiness"]["admission"] and view["ready"] and int(view["tick"]) > after_tick:
            return obs, data["state"], view
        time.sleep(0.02)
    raise AssertionError(("Player action did not settle", after_tick))

address, token = os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"]
player, enemy = (Session.connect(address, token, actor) for actor in (1, 3))
try:
    obs, state, view = ready(player)
    dose = next(item["id"] for item in view["inventory"] if item["identity"] == "item.potion.fixture.injury")
    assert player.act({"type": "drink", "item": dose}, obs)["status"] == "confirmed"
    other, _, _ = ready(enemy)
    assert enemy.act({"type": "wait"}, other)["status"] == "confirmed"
    obs, state, view = ready(player, int(view["tick"]))
    hp, maximum = (int(view["combat"][field]) for field in ("hp", "max_hp"))
    assert maximum > hp and hp * 2 >= maximum, (hp, maximum)
    threat = next(a["id"] for a in view["visible_actors"] if a["name"].lower() == "hobgoblin")
    policy = Policy()
    policy.largest_hits[("actor", threat)] = hp
    if os.environ["BOT_TEST_UNKNOWN_SCROLLS"] == "1":
        action, reason = policy.choose(state)
        assert action["type"] == "read" and reason == "test_unknown_magic", (action, reason)
        assert player.act(action, obs)["status"] == "confirmed"
        policy.accepted(action, view, reason=reason)
        obs, state, view = ready(player, int(view["tick"]))
        assert any(item["class"] == "scroll" and not item["identified"] for item in view["inventory"])
        action, reason = policy.choose(state)
        assert action["type"] == "use_ability" and reason == "melee", (action, reason)
        result = player.act(action, obs)
        assert result["status"] == "confirmed", result
        ready(player, int(view["tick"]))
    else:
        item = next(item for item in view["inventory"] if item["identity"] == "item.potion.healing")
        assert item["name"] == "Restorative draught", item
        action, reason = policy.choose(state)
        assert (action, reason) == ({"type": "drink", "item": item["id"]}, "heal"), (action, reason)
        assert player.act(action, obs)["status"] == "confirmed"
        _, _, restored = ready(player, int(view["tick"]))
        assert int(restored["combat"]["hp"]) > hp
finally:
    enemy.close()
    player.close()
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address, "BOT_TEST_UNKNOWN_SCROLLS": str(int(unknown_scrolls))},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bot_moves_away_from_a_foe_without_reversing_a_recovery_retreat(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        manifest["archetypes"]["injury"] = {
            "class": "potion", "identity": "item.potion.fixture.injury", "name": "injury potion",
            "consumable": {"effects": [{"kind": "damage", "damage": {"kind": "components",
                "components": [{"category": "vital", "amount": {"kind": "fixed", "amount": 5}}],
            }}]},
        }
        character, = manifest["characters"]
        character.update(anchor="2/up", loadout=[{"archetype": "injury", "quantity": 1}])
        manifest["stair_pairs"] = {"first": {"upper": "1/down", "lower": "2/up"}}
        region, = contents["regions"]
        region["anchors"]["down"] = [2, 2, 0]
        region["actors"] = [{"archetype": "rogue.hobgoblin", "id": 3, "controller": "external",
                             "at": [4, 2, 0], "turn_ticks": 1000000}]
        contents["regions"].append({"id": 2, "name": "Lower floor", "size": [5, 5, 2],
                                    "chamber": True, "anchors": {"up": [2, 2, 0]}})
        server = self.server(scenario=write_package(self, contents, "rogue-bot-recover-upstairs"))
        self.install_bot_extension()
        code = """
import os
import time
from rogue_o_matic import Policy, position
from tor_client_bot import Session

def ready(session, after_tick=-1):
    for _ in range(150):
        obs = session.observe()
        data = obs.data()
        view = data["state"]["observation"]
        if data["readiness"]["admission"] and view["ready"] and int(view["tick"]) > after_tick:
            return obs, data["state"], view
        time.sleep(0.02)
    raise AssertionError(("Player action did not settle", after_tick))

address, token = os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"]
player, enemy = (Session.connect(address, token, actor) for actor in (1, 3))
try:
    obs, state, view = ready(player)
    dose = view["inventory"][0]["id"]
    assert player.act({"type": "drink", "item": dose}, obs)["status"] == "confirmed"
    other, _, _ = ready(enemy)
    assert enemy.act({"type": "wait"}, other)["status"] == "confirmed"
    obs, state, view = ready(player, int(view["tick"]))
    policy = Policy()
    policy.floor = 24
    retreat = {"type": "move", "direction": "up"}
    assert player.act(retreat, obs)["status"] == "confirmed"
    policy.accepted(retreat, view, reason="retreat_upstairs")
    obs, state, view = ready(player, int(view["tick"]))
    landing = next(c for c in view["visible_cells"] if position(c) == (0, 0, 0))
    policy.observe_stair(landing)
    assert policy.floor == 23 and landing["stairs_down"]
    hp, maximum = (int(view["combat"][field]) for field in ("hp", "max_hp"))
    assert hp * 5 < maximum * 4, (hp, maximum)
    threat = next(a["id"] for a in view["visible_actors"] if a["name"].lower() == "hobgoblin")
    action, reason = policy.choose(state)
    assert reason == "retreat", (action, reason)
    policy.largest_hits[("actor", threat)] = hp
    action, reason = policy.choose(state)
    assert action["type"] == "move" and action["direction"] not in {"up", "down"}, (action, reason)
    assert player.act(action, obs)["status"] == "confirmed"
    _, _, moved = ready(player, int(view["tick"]))
    here = next(c for c in moved["visible_cells"] if position(c) == (0, 0, 0))
    assert here["key"] != landing["key"] and not here["stairs_up"]
finally:
    enemy.close()
    player.close()
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bot_launches_at_a_dangerous_enemy_from_a_clear_distance(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        character, = manifest["characters"]
        character["loadout"] = [{"archetype": "rogue.shortbow", "equipped_slot": 0, "quantity": 1},
                                {"archetype": "rogue.arrow", "quantity": 3}]
        character["known_identities"].extend(("item.weapon.shortbow", "item.weapon.arrow"))
        region, = contents["regions"]
        region["actors"] = [{"archetype": "rogue.wraith", "id": 3, "controller": "external",
                             "at": [5, 2, 0], "turn_ticks": 1000000}]
        server = self.server(scenario=write_package(self, contents, "rogue-bot-ranged-opening"))
        self.install_bot_extension()
        code = """
import os
import time
from rogue_o_matic import Policy
from tor_client_bot import Session

address, token = os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"]
player, enemy = (Session.connect(address, token, actor) for actor in (1, 3))
try:
    obs = player.observe()
    state = obs.data()["state"]
    view = state["observation"]
    arrows = next(item for item in view["inventory"] if item["identity"] == "item.weapon.arrow")
    def query(identity, phase, offset):
        return player.projectile_choices(obs, identity, phase, offset)
    action, reason = Policy().choose(state, projectile_choices=query)
    assert reason == "launch_projectile", (action, reason)
    aim = action["invocation"]["aim"]
    assert aim["direction"] == "east" and aim["delivery"]["ammunition"] == arrows["id"], aim
    result = player.act(action, obs)
    assert result["status"] == "confirmed", result
    other = enemy.observe()
    status = other.data()
    if status["readiness"]["admission"]:
        result = enemy.act({"type": "wait"}, other)
        assert result["status"] in {"confirmed", "stale_observation"}, result
    for _ in range(150):
        after = player.observe().data()["state"]["observation"]
        remaining = next(item["quantity"] for item in after["inventory"] if item["id"] == arrows["id"])
        if remaining == "2":
            break
        time.sleep(0.02)
    else:
        raise AssertionError(("Arrow was not committed", after["tick"], remaining))
finally:
    enemy.close()
    player.close()
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bot_stocks_throwable_darts_with_the_bundled_weapon_loadout(self):
        self.check_stocked_pickup(renamed_spear=False)

    def test_bot_picks_up_a_renamed_spear_with_the_bundled_weapon_loadout(self):
        self.check_stocked_pickup(renamed_spear=True)

    def check_stocked_pickup(self, *, renamed_spear):
        from discovery_package import fixture, write_package

        contents = fixture()
        character, = contents["manifest"]["characters"]
        character["loadout"] = [{"archetype": "rogue.mace", "equipped_slot": 0, "quantity": 1},
                                {"archetype": "rogue.shortbow", "quantity": 1},
                                {"archetype": "rogue.arrow", "quantity": 32}]
        character["known_identities"].extend(("item.weapon.mace", "item.weapon.shortbow",
                                               "item.weapon.arrow", "item.weapon.dart"))
        item = "spear" if renamed_spear else "dart"
        quantity = 1 if renamed_spear else 12
        if renamed_spear:
            contents["manifest"]["archetypes"]["rogue.spear"]["name"] = "Ash pike"
            character["known_identities"].append("item.weapon.spear")
        region, = contents["regions"]
        region.update(doors=[], traps=[], items=[
            {"id": 10, "at": [1, 2, 0], "archetype": f"rogue.{item}", "quantity": quantity},
        ])
        server = self.server(scenario=write_package(self, contents, "rogue-bot-ranged-stock"))
        self.install_bot_extension()
        code = """
import os
import time
from rogue_o_matic import Policy
from tor_client_bot import Session

session = Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1)
try:
    obs = session.observe()
    state = obs.data()["state"]
    row, = state["observation"]["ground_items"]
    identity = os.environ["BOT_TEST_WEAPON"]
    assert row["item"]["identity"] == identity, row
    take, reason = Policy().choose(state)
    assert (take, reason) == ({"type": "take", "item": row["item"]["id"], "quantity": row["item"]["quantity"]}, "loot")
    result = session.act(take, obs)
    assert result["status"] == "confirmed", result
    for _ in range(150):
        current = session.observe()
        data = current.data()
        inventory = data["state"]["observation"]["inventory"]
        weapon = next((item for item in inventory if item["identity"] == identity), None)
        if weapon and data["readiness"]["admission"]:
            break
        time.sleep(0.02)
    else:
        raise AssertionError("Weapon was not added to carried inventory")
    if identity == "item.weapon.dart":
        choices = session.projectile_choices(current, "rogue.throw", {"kind": "sources"}, 0)
        assert any(row.get("source", {}).get("item") == weapon["id"] for row in choices["choices"]), choices
    else:
        assert weapon["name"] == "Ash pike", weapon
finally:
    session.close()
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address, "BOT_TEST_WEAPON": f"item.weapon.{item}"},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_cohort_retrieves_and_independently_verifies_victory_at_full_depth(self):
        import tomllib

        from arena_matrix import toml_source
        from rogue_o_matic_cohort import trace_report

        self.install_bot_extension()
        source = ROOT / "scenarios/rogue"
        package = self.directory / "retrieval-smoke"
        shutil.copytree(source, package, ignore=shutil.ignore_patterns(".compiled"))
        path = package / "scenario.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        manifest["id"] = "rogue-bot-retrieval"
        manifest["characters"][0]["creature"]["nutrition"] = {"kind": "exempt"}
        manifest["generation_recipes"] = {"exploration": {
            "kind": "grid_rooms", "version": 1, "room_width": [5, 5],
            "room_height": [3, 3], "extra_connections": [0, 0],
        }}
        manifest["procedural_worlds"]["dungeon"].update(grid=[1, 1], size=[7, 5, 2])
        manifest["floor_activation"] = {
            "version": 1, "protagonist": "selected_character",
            "families": {"dungeon": "empty"},
            "profiles": {"empty": {"version": 1, "stages": []}},
        }
        path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        validated = subprocess.run(
            [self.bin / ("tor-scenario" + self.suffix), "validate", package],
            capture_output=True, text=True, timeout=30,
        )
        self.assertEqual(validated.returncode, 0, validated.stdout + validated.stderr)

        output = self.directory / "cohort"
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic_cohort.py"),
             "--server", str(self.bin / ("tor-server" + self.suffix)), "--scenario", str(package),
             "--output", str(output), "--budget", "512", "--timeout", "180", "42"],
            cwd=self.directory, env={**os.environ, "PYTHONPATH": str(self.directory)},
            text=True, capture_output=True, timeout=215,
        )
        errors = (output / "seed-42" / "bot.stderr").read_text(encoding="utf-8")
        self.assertEqual(result.returncode, 0, result.stderr + errors)
        summary = json.loads(result.stdout)
        self.assertEqual(summary["victories"], 1, summary)
        report, = summary["results"]
        self.assertTrue(report["victory_verified"])
        self.assertTrue(report["amulet_acquired"])

        _, progress = trace_report(output / "seed-42" / "turns.jsonl")
        self.assertTrue(progress["amulet_acquired"])
        self.assertEqual(report["max_depth"], 26)
        self.assertGreaterEqual(report["estimated_ascent_ticks"], 25 * 100)
        by_floor = report["estimated_ascent_ticks_by_floor"]
        self.assertGreaterEqual(by_floor["4"], 100)
        self.assertEqual(sum(by_floor.values()), report["estimated_ascent_ticks"])
        floors = [1] + [step["to"] for step in report["stair_transitions"]]
        self.assertEqual(floors, list(range(1, 27)) + list(range(25, 0, -1)))

    def test_bot_equips_slow_digestion_and_spends_less_reserve_while_resting(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        character, = manifest["characters"]
        character["creature"]["nutrition"] = {"kind": "required"}
        character["known_identities"].append("item.ring.ring-slow-digestion")
        character["loadout"] = [{"archetype": "rogue.ring_slow_digestion", "quantity": 1},
                                {"archetype": "injury", "quantity": 1}]
        manifest["archetypes"]["injury"] = {
            "class": "potion", "identity": "item.potion.fixture.injury", "name": "injury potion",
            "consumable": {"effects": [{"kind": "damage", "damage": {
                "kind": "components", "components": [
                    {"category": "vital", "amount": {"kind": "fixed", "amount": 5}}
                ],
            }}]},
        }
        contents["regions"][0]["actors"] = []
        server = self.server(scenario=write_package(self, contents, "rogue-bot-slow-digestion"))
        self.install_bot_extension()
        code = """
import os
import time
from rogue_o_matic import Policy
from tor_client_bot import Session

def ready(session, after_tick=-1):
    for _ in range(100):
        obs = session.observe()
        data = obs.data()
        view = data["state"]["observation"]
        if data["readiness"]["admission"] and view["ready"] and int(view["tick"]) > after_tick:
            return obs, data["state"], view
        time.sleep(0.02)
    raise AssertionError(("No settled player boundary", after_tick))

def submit(session, frame, action):
    obs, _, view = frame
    result = session.act(action, obs)
    assert result["status"] == "confirmed", result
    return ready(session, int(view["tick"]))

def reserve(frame):
    return frame[2]["combat"]["own_stats"]["nutrition"]["reserve"]

session = Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1)
try:
    frame = ready(session)
    dose = next(item for item in frame[2]["inventory"] if item["name"] == "injury potion")
    frame = submit(session, frame, {"type": "drink", "item": dose["id"]})
    start = reserve(frame)
    for _ in range(4):
        frame = submit(session, frame, {"type": "wait"})
    before = start - reserve(frame)
    policy = Policy()
    action, reason = policy.choose(frame[1])
    assert action["type"] == "equip" and reason == "conserve_food", (action, reason)
    frame = submit(session, frame, action)
    start = reserve(frame)
    for _ in range(4):
        frame = submit(session, frame, {"type": "wait"})
    after = start - reserve(frame)
    assert 0 < after < before, (before, after)
finally:
    session.close()
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bot_checks_a_possible_connection_to_the_remembered_up_stair(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        character, = contents["manifest"]["characters"]
        character["loadout"] = [{"archetype": "rogue.amulet_of_yendor", "quantity": 1}]
        character["known_identities"].append("item.amulet.amulet-of-yendor")
        region, = contents["regions"]
        region.update(actors=[], walls=[[x, 2, z] for x in (0, 3) for z in range(2)])
        server = self.server(scenario=write_package(self, contents, "rogue-bot-return-connection"))
        self.install_bot_extension()
        code = """
import os
import time
from rogue_o_matic import Floor, Policy, position
from tor_client_bot import Session

session = Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1)
try:
    obs = session.observe()
    state = obs.data()["state"]
    cells = {position(c): c for c in state["observation"]["visible_cells"]}
    west, east = (cells[(x, 0, 0)]["key"] for x in (-1, 2))
    anchor = cells[(1, 0, 0)]["key"]
    policy = Policy()
    policy.floor = 4
    memory = Floor(visited={c["key"] for c in cells.values() if not c["wall"]},
                   stairs_up={"remembered-stair"})
    memory.graph.edges["remembered-stair"] = {}
    memory.joining_wall_hints.update({west: {"decoy-side"}, east: {"remembered-stair"}})
    policy.floors[4] = memory
    action, reason = policy.choose(state)
    assert (action, reason) == ({"type": "move", "direction": "east"}, "search"), (action, reason)
    assert session.act(action, obs)["status"] == "confirmed"
    for _ in range(150):
        obs = session.observe()
        data = obs.data()
        view = data["state"]["observation"]
        if data["readiness"]["admission"] and any(
                c["key"] == anchor and position(c) == (0, 0, 0) for c in view["visible_cells"]):
            action, reason = policy.choose(data["state"])
            assert action.get("id") == "rogue.search" and reason == "search", (action, reason)
            result = session.act(action, obs)
            assert result["status"] == "confirmed", result
            intention = result["receipt"]["intention"]
            break
        time.sleep(0.02)
    else:
        raise AssertionError("The return search route did not settle")
    for _ in range(150):
        frame = session.observe().data()
        if any(row["intention"] == intention and row["phase"] == "resolved"
               for row in frame["finished_intentions"]):
            break
        time.sleep(0.02)
    else:
        raise AssertionError("Search did not resolve")
finally:
    session.close()
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bot_uses_teleportation_as_a_last_resort_for_the_return(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        character, = contents["manifest"]["characters"]
        character["loadout"] = [{"archetype": item, "quantity": 1}
                                for item in ("rogue.amulet_of_yendor", "rogue.teleportation")]
        character["known_identities"].extend(("item.amulet.amulet-of-yendor", "item.scroll.teleportation"))
        contents["regions"][0]["actors"] = []
        server = self.server(scenario=write_package(self, contents, "rogue-bot-last-return-route"))
        self.install_bot_extension()
        code = """
import os
import time
from rogue_o_matic import Floor, Policy, position
from tor_client_bot import Session

session = Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1)
try:
    obs = session.observe()
    state = obs.data()["state"]
    view = state["observation"]
    origin = next(c["key"] for c in view["visible_cells"] if position(c) == (0, 0, 0))
    scroll = next(item for item in view["inventory"] if item["identity"] == "item.scroll.teleportation")
    policy = Policy()
    policy.floor = 4
    memory = Floor(stairs_up={"remembered-landing"})
    policy.floors[4] = memory
    for cell in view["visible_cells"]:
        if cell["wall"]:
            memory.search_attempts[cell["key"]] = 8
        else:
            memory.visited.add(cell["key"])
            memory.tactile_attempts[cell["key"]] = 4
    action, reason = policy.choose(state)
    assert (action, reason) == ({"type": "read", "item": scroll["id"]}, "find_return_route"), (action, reason)
    assert session.act(action, obs)["status"] == "confirmed"
    for _ in range(150):
        data = session.observe().data()
        moved = data["state"]["observation"]
        if data["readiness"]["admission"] and int(moved["tick"]) > int(view["tick"]):
            assert all(item["id"] != scroll["id"] for item in moved["inventory"])
            assert any(c["key"] != origin and position(c) == (0, 0, 0) for c in moved["visible_cells"])
            break
        time.sleep(0.02)
    else:
        raise AssertionError("Teleportation did not settle")
finally:
    session.close()
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bot_returns_to_remembered_food_before_an_underfunded_ascent(self):
        self.check_underfunded_return(carrying=True)

    def test_bot_provisions_the_return_before_the_retrieval_floor(self):
        self.check_underfunded_return(carrying=False, retrieval_depth=6)

    def test_bot_scouts_for_food_before_an_underfunded_descent(self):
        self.check_underfunded_return(carrying=False, retrieval_depth=6, scouting=True)

    def check_underfunded_return(self, *, carrying, retrieval_depth=4, scouting=False):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        character, = manifest["characters"]
        character["creature"]["nutrition"] = {"kind": "required"}
        character["loadout"] = []
        if scouting:
            character["loadout"] = [{"archetype": "rogue.ration", "quantity": 2}]
            contents["rules"] = contents["rules"].replace("initial = 1300,", "initial = 250,")
        if carrying:
            character["loadout"] = [{"archetype": "rogue.amulet_of_yendor", "quantity": 1}]
            character["known_identities"].append("item.amulet.amulet-of-yendor")
        first, second = ("up", "down") if carrying else ("down", "up")
        manifest["stair_pairs"] = {"escape": {"upper": "2/down" if carrying else "1/down",
                                               "lower": "1/up" if carrying else "2/up"}}
        region, = contents["regions"]
        region.update(size=[32, 5, 2], actors=[], items=[
            {"id": 10, "at": [27 if scouting else 1, 2, 0], "archetype": "rogue.ration"},
        ])
        region["anchors"][first] = [8 if scouting else 29, 2, 0]
        contents["regions"].append({"id": 2, "name": "Upper floor", "size": [5, 5, 2],
                                    "chamber": True, "anchors": {second: [2, 2, 0]}})
        server = self.server(scenario=write_package(self, contents, "rogue-bot-provision-ascent"))
        self.install_bot_extension()
        code = """
import os
import time
from rogue_o_matic import Policy, position
from tor_client_bot import Session

def ready(session, after=-1):
    for _ in range(150):
        obs = session.observe()
        data = obs.data()
        view = data["state"]["observation"]
        if data["readiness"]["admission"] and view["ready"] and int(view["tick"]) > after:
            return obs, data["state"], view
        time.sleep(0.02)
    raise AssertionError(("No settled player boundary", after))

session = Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1)
try:
    carrying = os.environ["BOT_TEST_CARRYING"] == "1"
    scouting = os.environ["BOT_TEST_SCOUTING"] == "1"
    policy = Policy(retrieval_depth=int(os.environ["BOT_TEST_RETRIEVAL_DEPTH"]))
    policy.floor = 3
    obs, state, view = ready(session)
    assert bool(view["ground_items"]) != scouting
    if scouting:
        action, reason = policy.choose(state)
        assert action["type"] == "eat" and reason == "eat", (action, reason)
        assert session.act(action, obs)["status"] == "confirmed"
        policy.accepted(action, view, reason=reason)
        obs, state, view = ready(session, int(view["tick"]))
    for _ in range(7 if scouting else 28):
        policy.choose(state)
        assert session.act({"type": "move", "direction": "east"}, obs)["status"] == "confirmed"
        obs, state, view = ready(session, int(view["tick"]))
    assert not view["ground_items"], view["ground_items"]
    assert view["combat"]["own_stats"]["nutrition"]["hunger"] == "nourished"
    direction, reason = ("up", "ascend") if carrying else ("down", "descend")
    assert any(c["stairs_" + direction] and position(c) == (0, 0, 0) for c in view["visible_cells"])
    assert bool(policy.floors[3].food_cells) != scouting
    assert policy.choose(state) == ({"type": "move", "direction": direction}, reason)
    nutrition = view["combat"]["own_stats"]["nutrition"]
    policy.return_travel_ticks[2] = (nutrition["reserve"] + 1) * int(nutrition["unit_ticks"])
    if scouting:
        assert policy.food_value["item.food.ration"] > 0
        policy.descent_travel_ticks = {2: 110000}
    action, reason = policy.choose(state)
    if scouting:
        assert action["type"] == "move" and reason == "scout_for_food", (action, reason)
        offset = {"east": (1, 0, 0), "north_east": (1, -1, 0), "south_east": (1, 1, 0)}[action["direction"]]
    else:
        assert (action, reason) == ({"type": "move", "direction": "west"}, "forage"), (action, reason)
        offset = (-1, 0, 0)
    target = next(c["key"] for c in view["visible_cells"] if position(c) == offset)
    assert session.act(action, obs)["status"] == "confirmed"
    obs, state, after = ready(session, int(view["tick"]))
    assert any(c["key"] == target and position(c) == (0, 0, 0) for c in after["visible_cells"])
    if scouting:
        for _ in range(45):
            action, reason = policy.choose(state)
            assert reason in {"scout_for_food", "approach_loot", "loot"}, (action, reason)
            assert session.act(action, obs)["status"] == "confirmed"
            policy.accepted(action, after, reason=reason)
            obs, state, after = ready(session, int(after["tick"]))
            if sum(int(item["quantity"]) for item in after["inventory"]
                   if item.get("identity") == "item.food.ration") == 2:
                break
        else:
            raise AssertionError("The scout did not collect the ration")
        action, reason = policy.choose(state)
        assert reason == "find_descent", (action, reason)
        assert policy.floors[3].food_scout_end <= int(after["tick"])
        assert session.act(action, obs)["status"] == "confirmed"
finally:
    session.close()
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address, "BOT_TEST_CARRYING": str(int(carrying)),
                 "BOT_TEST_RETRIEVAL_DEPTH": str(retrieval_depth), "BOT_TEST_SCOUTING": str(int(scouting))},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bot_explores_for_the_exit_instead_of_chasing_while_carrying_the_amulet(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        character, = contents["manifest"]["characters"]
        character["loadout"] = [{"archetype": "rogue.amulet_of_yendor", "quantity": 1}]
        character["known_identities"].append("item.amulet.amulet-of-yendor")
        region, = contents["regions"]
        region["actors"] = [{"archetype": "rogue.orc", "id": 3, "controller": "external",
                             "at": [3, 2, 0], "turn_ticks": 1000000}]
        server = self.server(scenario=write_package(self, contents, "rogue-bot-ascent-no-pursuit"))
        self.install_bot_extension()
        code = """
import os
import time
from rogue_o_matic import Floor, Policy, position
from tor_client_bot import Session

session = Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1)
try:
    obs = session.observe()
    state = obs.data()["state"]
    view = state["observation"]
    target = next(c["key"] for c in view["visible_cells"] if position(c) == (-1, 0, 0))
    assert any(a["name"] == "orc" and position(a) == (2, 0, 0) for a in view["visible_actors"])
    policy = Policy()
    policy.floors[1] = Floor(visited={c["key"] for c in view["visible_cells"]
                                    if position(c)[2] == 0 and c["key"] != target and not c["wall"]})
    action, reason = policy.choose(state)
    assert (action, reason) == ({"type": "move", "direction": "west"}, "explore"), (action, reason)
    result = session.act(action, obs)
    assert result["status"] == "confirmed", result
    for _ in range(150):
        after = session.observe().data()["state"]["observation"]
        if any(c["key"] == target and position(c) == (0, 0, 0) for c in after["visible_cells"]):
            break
        time.sleep(0.02)
    else:
        raise AssertionError(("Exit exploration did not settle", after["tick"]))
finally:
    session.close()
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
