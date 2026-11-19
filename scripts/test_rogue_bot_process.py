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
        package = self.directory / "tor_client_bot"
        shutil.copytree(ROOT / "crates/client-bot/python/tor_client_bot", package, dirs_exist_ok=True)
        extension = "_native.pyd" if os.name == "nt" else "_native.so"
        shutil.copyfile(Path(build) / library, package / extension)

    def test_player_api_interprets_real_observation_and_submits_wait(self):
        self.server(scenario=ROOT / "scenarios/rogue")
        self.install_bot_extension()
        code = """
import os
from tor_client_bot import ActivityPhase, Outcome, Session, Wait

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.observe()
    assert turn.can_act and turn.cell.offset == (0, 0, 0)
    assert 0 < turn.health.current <= turn.health.maximum
    assert any(item.quantity > 1 for item in turn.inventory)
    assert all(creature.cells for creature in turn.creatures)
    bow = next(choice for choice in turn.equip_choices()
               if choice.item.identity == "item.weapon.shortbow" and choice.slot.occupant is None)
    refusal = turn.submit(bow)
    assert refusal.status is Outcome.REJECTED and refusal.code and refusal.reason, refusal
    assert refusal.activity is None
    turn = game.observe()
    assert turn.tick == 0 and any(slot.occupant and slot.occupant.identity == "item.weapon.mace"
                                  for slot in turn.equipment)
    admitted = turn.submit(Wait())
    assert admitted.status is Outcome.CONFIRMED, admitted
    assert admitted.activity is not None
    assert admitted.activity.actor == 1
    following = game.wait_for_decision()
    assert following.outcome_of(admitted) is ActivityPhase.RESOLVED, following.finished_activities
"""
        run = subprocess.run([sys.executable, "-c", code], cwd=self.directory,
                             env={**os.environ, "TOR_SERVER_TOKEN": TOKEN, "BOT_TEST_ADDRESS": self.address},
                             capture_output=True, text=True, timeout=30)
        self.assertEqual(run.returncode, 0, run.stderr)

    def test_walks_through_a_two_cell_tall_door_with_one_reachable_approach(self):
        from discovery_package import fixture, write_package

        contents = fixture()
        region, = contents["regions"]
        region["anchors"]["start"] = [2, 2, 0]
        del region["doors"][0]["concealment"]
        region["traps"] = []
        server = self.server(scenario=write_package(self, contents, "tall-door"))
        self.install_bot_extension()
        code = """
import os
from tor_client_bot import Direction, Move, OpenDoor, Outcome, Session

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.wait_for_decision()
    door_cells = [cell for cell in turn.visible_cells if cell.door]
    assert len({cell.offset[2] for cell in door_cells}) == 2, door_cells
    assert len({id(cell.door) for cell in door_cells}) == 1
    opened = turn.step(Direction.EAST)
    assert isinstance(opened, OpenDoor), opened
    assert turn.submit(opened).status is Outcome.CONFIRMED
    turn = game.wait_for_decision()
    assert turn.step(Direction.EAST) == Move(Direction.EAST)
    entered = next(cell.key for cell in turn.visible_cells if cell.offset == (1, 0, 0))
    assert turn.submit(turn.step(Direction.EAST)).status is Outcome.CONFIRMED
    assert game.wait_for_decision().cell.key == entered
"""
        run = subprocess.run([sys.executable, "-c", code], cwd=self.directory,
                             env={**os.environ, "BOT_TEST_ADDRESS": self.address, "TOR_SERVER_TOKEN": TOKEN},
                             capture_output=True, text=True, timeout=30)
        server.stop(save=False)
        self.assertEqual(run.returncode, 0, run.stderr)

    def test_public_training_previews_and_confirms_one_disclosed_purchase(self):
        server = self.server(scenario=ROOT / "scenarios/rogue-exploration", character=1)
        self.install_bot_extension()
        code = """
import os
from tor_client_bot import Outcome, Session, TrainingKind

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    game.wait_for_decision()
    training = game.training
    initial = training.overview()
    assert not initial.locked and initial.unspent == 1, initial
    pool = next(p for p in training.pools() if p.available and TrainingKind.TALENT in p.allowed)
    talent = next(choice for choice in training.options(TrainingKind.TALENT)
                  if choice.identity == "rattling_blow")
    proposal = training.preview(pool, talent)
    assert not proposal.would_die and proposal.resulting_health.current > 0
    assert training.confirm(proposal).status is Outcome.CONFIRMED
    assert training.confirm(proposal).status is Outcome.STALE
    game.wait_for_decision()
    assert training.overview().unspent == 0
"""
        run = subprocess.run([sys.executable, "-c", code], cwd=self.directory,
                             env={**os.environ, "BOT_TEST_ADDRESS": self.address, "TOR_SERVER_TOKEN": TOKEN},
                             capture_output=True, text=True, timeout=45)
        server.stop(save=False)
        self.assertEqual(run.returncode, 0, run.stderr)

    def test_production_bot_spends_an_offered_point_and_learns_rattling_blow(self):
        import tomllib
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        character, = contents["manifest"]["characters"]
        growth = tomllib.loads((ROOT / "scenarios/rogue-exploration/scenario.toml").read_text(encoding="utf-8"))
        build = growth["characters"][0]["creature"]
        for field in ("starting_skills", "hit_dice", "future_growth"):
            character["creature"][field] = build[field]
        character["creature"].pop("growth_schedule", None)
        character.update(loadout=[{"archetype": "rogue.raise_level", "quantity": 1}],
                         known_identities=["item.potion.raise-level"])
        contents["regions"][0]["actors"] = []
        self.server(scenario=write_package(self, contents, "earned-training"))
        self.install_bot_extension()
        env = {**os.environ, "PYTHONPATH": str(self.directory), "TOR_SERVER_TOKEN": TOKEN}
        run = subprocess.run([sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect",
                              self.address, "--budget", "8"], cwd=self.directory, env=env,
                             text=True, capture_output=True, timeout=55)
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
        *actions, terminal = (json.loads(line) for line in run.stdout.splitlines())
        self.assertEqual(actions[0]["reason"], "permanent_growth")
        self.assertIn("earned_training", [row["reason"] for row in actions], (actions, run.stderr))
        self.assertEqual(terminal["terminal"], "action_budget")
        code = """
import os, sys
from tor_client_bot import Session
with Session.connect(sys.argv[1], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    assert game.training.overview().unspent == 0
    assert any(a.identity == "rattling_blow" for a in game.observe().abilities)
"""
        check = subprocess.run([sys.executable, "-c", code, self.address], cwd=self.directory,
                               env=env, text=True, capture_output=True, timeout=30)
        self.assertEqual(check.returncode, 0, check.stderr)

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
             "--output", str(output), "--budget", "110", "--record-decisions", "47"],
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
        failed = {row["previous_intention_id"] for row in turns if row["previous_intention_phase"] == "failed"}
        self.assertTrue(any(row["reason"] == "throw_projectile" and
                            row["result"]["receipt"]["intention"] in failed for row in shots))
        # Begin with fresh policy memory and rebuild it from the same observed decisions.
        replay = """
import json
import sys
from pathlib import Path
from rogue_o_matic import DEFAULT_OBJECTIVE_IDENTITY
from rogue_player import Adventurer
from tor_client_bot import CombatJournal, Outcome, Result, Turn, describe_action

player, journal = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), CombatJournal()
previous = None
deepest = 1
for line in Path(sys.argv[1]).read_text(encoding="utf-8").splitlines():
    row = json.loads(line)
    turn = Turn.replay(row["recording"])
    combat = journal.read(turn)
    if previous:
        assert turn.outcome_of(previous) == row["previous_intention_phase"]
    player.observe(turn, combat)
    deepest = max(deepest, player.depth)
    if "terminal" in row:
        assert turn.tick == row["tick"]
        assert deepest == row["max_depth"]
        break
    action, reason = player.choose(turn)
    assert reason == row["reason"], (turn.tick, reason, row["reason"])
    assert json.loads(json.dumps(describe_action(action))) == row["action"], (turn.tick, action, row["action"])
    assert player.depth == row["floor"]
    previous = Result.from_reply(row["result"])
    if previous.status is Outcome.CONFIRMED:
        player.admitted(turn, action, previous, reason=reason)
"""
        checked = subprocess.run(
            [sys.executable, "-c", replay, str(output / "seed-47/turns.jsonl")],
            cwd=self.directory, env={**os.environ, "PYTHONPATH": str(ROOT / "scripts")},
            capture_output=True, text=True, timeout=45,
        )
        self.assertEqual(checked.returncode, 0, checked.stderr)


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

    def test_bot_keeps_the_refuge_after_escaping_a_guarded_stair(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        contents["manifest"]["characters"][0]["loadout"] = []
        contents["manifest"]["stair_pairs"] = {"escape": {"upper": "2/down", "lower": "1/start"}}
        contents["regions"][0]["actors"] = [
            {"archetype": "rogue.hobgoblin", "id": 3, "at": [2, 2, 0], "controller": "external",
             "turn_ticks": 1000000}]
        contents["regions"].append({"id": 2, "name": "Refuge", "size": [5, 5, 2],
                                    "chamber": True, "anchors": {"down": [2, 2, 0]}})
        self.server(scenario=write_package(self, contents, "guarded-stair"))
        self.install_bot_extension()
        code = """
import os, sys
from rogue_player import Adventurer
from tor_client_bot import Direction, Move, Outcome, Session, Wait
token = os.environ["TOR_SERVER_TOKEN"]
with Session.connect(sys.argv[1], token, 1) as game, Session.connect(sys.argv[1], token, 3) as enemy:
    turn = game.wait_for_decision()
    player = Adventurer(26, "item.amulet.amulet-of-yendor", depth=6)
    player.draining_creatures.add(turn.hostiles[0].name)
    player.observe(turn)
    step, reason = player.choose(turn)
    assert step == Move(Direction.UP) and reason == "retreat_upstairs", (step, reason)
    result = turn.submit(step)
    assert result.status is Outcome.CONFIRMED
    player.admitted(turn, step, result, reason=reason)
    assert enemy.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
    refuge = game.wait_for_decision()
    player.observe(refuge)
    assert player.depth == 5 and refuge.cell.stairs_down
    step, reason = player.choose(refuge)
    assert step is not None and step != Move(Direction.DOWN), (step, reason)
    receipt = refuge.submit(step)
    assert receipt.status is Outcome.CONFIRMED
    assert game.wait_for_decision().outcome_of(receipt).value == "resolved"
"""
        result = subprocess.run([sys.executable, "-c", code, self.address], cwd=self.directory,
                                env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN},
                                text=True, capture_output=True, timeout=45)
        self.assertEqual(result.returncode, 0, result.stderr)

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
from rogue_player import Adventurer
from tor_client_bot import Direction, Move, Outcome, Session, Wait
from tor_client_bot._native import Session as Transport

address, token = os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"]
diagnostics = Transport.connect(address, token, 1)
try:
    obs = diagnostics.observe()
    snapshot = obs.data()
    assert set(snapshot) == {
        "state", "memory", "cursor", "branch", "readiness", "history", "narration",
        "outcome", "intentions", "finished_intentions", "combat_updates", "travel",
    }
    snapshot["state"]["observation"]["visible_cells"].clear()
    assert obs.data()["state"]["observation"]["visible_cells"]
finally:
    diagnostics.close()
session, enemy = (Session.connect(address, token, actor) for actor in (1, 3))
try:
    turn = session.observe()
    policy = Adventurer(26, "item.amulet.amulet-of-yendor", depth=2)
    policy.observe(turn)
    action, reason = policy.choose(turn)
    diagonal = os.environ["BOT_TEST_DIAGONAL"] == "1"
    direction = Direction.NORTH_WEST if diagonal else Direction.NORTH
    assert (action, reason) == (Move(direction), "retreat_to_stairs"), (action, reason)
    offset = (-1, -1, 0) if diagonal else (0, -1, 0)
    goal = next(cell.key for cell in turn.visible_cells if cell.offset == offset)
    assert turn.submit(action).status is Outcome.CONFIRMED
    assert enemy.observe().submit(Wait()).status is Outcome.CONFIRMED
    for _ in range(150):
        frame = session.observe()
        if frame.can_act and frame.cell and frame.cell.key == goal:
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

from rogue_player import Adventurer
from tor_client_bot import CombatJournal, Outcome, Session, UseAbility, Wait

address, token = os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"]
with Session.connect(address, token, 1) as player, Session.connect(address, token, 3) as snake:
    policy = Adventurer(26, "item.amulet.amulet-of-yendor")
    journal = CombatJournal()
    for _ in range(30):
        turn = player.observe()
        policy.observe(turn, journal.read(turn))
        if turn.restorable_losses.strength > 0:
            assert "rattlesnake" in policy.draining_creatures, policy.draining_creatures
            break
        enemy = snake.observe()
        if enemy.can_act:
            victim = next(actor for actor in enemy.creatures if actor.name == "Warrior")
            bite = next(ability for ability in enemy.abilities if ability.identity == "rogue.venomous_bite")
            result = enemy.submit(UseAbility(bite, victim))
            assert result.status in {Outcome.CONFIRMED, Outcome.STALE}, result
        if turn.can_act:
            result = turn.submit(Wait())
            assert result.status in {Outcome.CONFIRMED, Outcome.STALE}, result
    else:
        raise AssertionError("No lasting strength loss observed")
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
from tor_client_bot import Direction, Move, Outcome, Session, Turn

address, token = os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"]
with Session.connect(address, token, 1) as player, Session.connect(address, token, 3) as blocker:
    before = player.observe()
    anchor = blocker.observe().cell
    assert before.cell.stairs_down and anchor.stairs_up
    move = Move(Direction.DOWN)
    receipt = before.submit(move)
    assert receipt.status is Outcome.CONFIRMED, receipt
    attempt = before.stair_attempt(move, receipt)
    for _ in range(100):
        after = player.observe()
        if after.stair_arrival(attempt):
            break
        time.sleep(0.02)
    else:
        raise AssertionError(("Stair traversal did not settle", attempt, after.cell, after.tick, after.outcome_of(receipt), [(c.key, c.offset) for c in after.visible_cells if c.stairs_up]))
    projected = next(cell for cell in after.visible_cells if cell.key == attempt.landing)
    x, y, z = projected.offset
    assert abs(x) + abs(y) == 1 and z == 0, projected
    assert after.cell.key != before.cell.key
    assert blocker.observe().cell.key == anchor.key
    recorded_attempt = Turn.replay(before.recording()).stair_attempt(move, receipt)
    assert Turn.replay(after.recording()).stair_arrival(recorded_attempt) is True
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
from rogue_player import Adventurer
from tor_client_bot import Direction, ItemUse, Move, Outcome, Session, UseItem, Wait

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    turn = game.wait_for_decision()
    player.observe(turn)
    origin = turn.cell.key
    assert turn.submit(Move(Direction.EAST)).status is Outcome.CONFIRMED
    turn = game.wait_for_decision()
    exposed = turn.cell.key
    assert exposed != origin
    dose, = turn.inventory
    assert turn.submit(UseItem(dose, ItemUse.DRINK)).status is Outcome.CONFIRMED
    turn = game.wait_for_decision()
    assert turn.health.current * 5 < turn.health.maximum * 4
    player.observe(turn)
    player.danger_until[exposed] = turn.tick + 1200
    action, reason = player.choose(turn)
    assert (action, reason) == (Move(Direction.WEST), "reposition_to_recover"), (action, reason)
    assert turn.submit(action).status is Outcome.CONFIRMED
    after = game.wait_for_decision()
    player.observe(after)
    assert after.cell.key == origin and player.choose(after) == (Wait(), "recover")
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
        contents["regions"][0]["anchors"]["up"] = [2, 1, 0]
        contents["regions"].append({"id": 2, "name": "Upper floor", "size": [5, 5, 2],
                                    "chamber": True, "anchors": {"upper": [2, 2, 0]}})
        server = self.server(scenario=write_package(self, contents, "rogue-bot-diagonal-retreat"))
        self.install_bot_extension()
        code = """
import os
from rogue_player import Adventurer
from tor_client_bot import Direction, Move, Outcome, Session, Wait

address, token = os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"]
with Session.connect(address, token, 1) as game, Session.connect(address, token, 3) as wraith:
    turn = game.wait_for_decision()
    assert any(c.name.casefold() == "wraith" and (0, 1, 0) in c.cells for c in turn.creatures)
    stair = turn.cell_at((1, -1, 0))
    assert stair.stairs_up
    player = Adventurer(26, "item.amulet.amulet-of-yendor", depth=11)
    player.observe(turn)
    action, reason = player.choose(turn)
    assert (action, reason) == (Move(Direction.NORTH_EAST), "retreat_to_stairs"), (action, reason)
    assert turn.submit(action).status is Outcome.CONFIRMED
    assert wraith.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
    assert game.wait_for_decision().cell.key == stair.key
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
from rogue_player import Adventurer
from tor_client_bot import Direction, Move, Outcome, Session, UseAbility

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.wait_for_decision()
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    player.observe(turn)
    memory = player.maps[1]
    east, west = (turn.cell_at((x, 0, 0)).key for x in (1, -1))
    memory.visited.update(memory.cells)
    # The only remaining candidate sites are equally far away.
    player.searches.update({key: 3 for key in memory.cells if key not in {east, west}})
    walls = lambda key: {cell.key for cell in memory.adjacent(key) if cell.wall}
    assert len(walls(east)) == 2 and len(walls(west)) == 1
    step, reason = player.choose(turn)
    assert (step, reason) == (Move(Direction.EAST), "search"), (step, reason)
    assert turn.submit(step).status is Outcome.CONFIRMED
    after = game.wait_for_decision()
    assert after.cell.key == east
    player.observe(after)
    memory.visited.update(memory.cells)
    for _ in range(2):
        action, reason = player.choose(after)
        assert isinstance(action, UseAbility) and action.ability.identity == "rogue.search" and reason == "search"
        assert after.submit(action).status is Outcome.CONFIRMED
        player.admitted(after, action)
        following = game.wait_for_decision()
        assert following.tick > after.tick
        after = following
        player.observe(after)
    assert all(player.wall_searches[key] == 2 for key in walls(east))
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
        self.assertTrue(result.stdout.strip(), result.stderr)
        first, second, terminal = (json.loads(line) for line in result.stdout.splitlines())
        self.assertEqual(first["action"]["type"], "eat")
        self.assertEqual(first["result"]["status"], "confirmed")
        self.assertEqual(first["nutrition_reserve"], 2)
        self.assertEqual(second["nutrition_reserve"], 8)
        self.assertEqual(terminal["terminal"], "action_budget")

    def test_weak_bot_eats_with_a_visible_melee_opponent(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        character, = contents["manifest"]["characters"]
        character["creature"]["nutrition"] = {"kind": "required"}
        character["loadout"] = [{"archetype": "rogue.ration", "quantity": 1}]
        contents["rules"] = contents["rules"].replace("initial = 1300,", "initial = 100,")
        contents["regions"][0]["actors"] = [
            {"archetype": "rogue.hobgoblin", "id": 3, "controller": "external",
             "at": [2, 2, 0], "turn_ticks": 1000000}
        ]
        self.server(scenario=write_package(self, contents, "weak-ration-in-contact"))
        self.install_bot_extension()
        code = """
import os
from rogue_player import Adventurer
from tor_client_bot import ItemUse, Outcome, ReadinessTimeout, Session, UseItem, Wait

address, token = os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"]
with Session.connect(address, token, 1) as game, Session.connect(address, token, 3) as opponent:
    turn = game.wait_for_decision()
    assert turn.hunger == "weak" and any(turn.in_contact(foe) for foe in turn.hostiles)
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    player.observe(turn)
    action, reason = player.choose(turn)
    assert isinstance(action, UseItem) and action.use is ItemUse.EAT and reason == "eat"
    meal = turn.submit(action)
    assert meal.status is Outcome.CONFIRMED
    try:
        game.wait_for_decision(timeout=0.1)
    except ReadinessTimeout as stalled:
        assert not stalled.turn.can_act and not stalled.turn.terminal
        assert stalled.turn.tick >= turn.tick
    else:
        raise AssertionError("Expected the game to wait for the external opponent")
    assert opponent.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
    assert game.wait_for_decision().food_reserve > turn.food_reserve
"""
        run = subprocess.run([sys.executable, "-c", code], cwd=self.directory,
                             env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"),
                                  "BOT_TEST_ADDRESS": self.address, "TOR_SERVER_TOKEN": TOKEN},
                             capture_output=True, text=True, timeout=55)
        self.assertEqual(run.returncode, 0, run.stderr)

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

    def start_identification_server(self):
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
        return self.server(scenario=write_package(self, contents, "rogue-bot-rename-identify"))

    def test_stale_identification_answers_leave_the_current_prompt_open(self):
        self.start_identification_server()
        self.install_bot_extension()
        code = """
import os
from tor_client_bot import ItemUse, Outcome, Session, UseItem

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.wait_for_decision()
    scroll = next(item for item in turn.inventory if ItemUse.READ in item.uses)
    assert turn.submit(UseItem(scroll, ItemUse.READ)).status is Outcome.CONFIRMED
    first = game.wait_for_decision().identification
    current = game.observe()
    assert current.identification.choice == first.choice
    assert current.submit(first.select(None)).status is Outcome.STALE
    assert current.submit(first.select(first.items[0])).status is Outcome.STALE
    answer = current.identification.select(current.identification.items[0])
    assert current.submit(answer).status is Outcome.CONFIRMED
"""
        run = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "TOR_SERVER_TOKEN": TOKEN, "BOT_TEST_ADDRESS": self.address},
            capture_output=True, text=True, timeout=45,
        )
        self.assertEqual(run.returncode, 0, run.stderr)

    def test_bot_uses_a_renamed_identification_scroll(self):
        server = self.start_identification_server()
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

    def test_bot_walks_to_a_nearby_potion_and_collects_it(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        character, = contents["manifest"]["characters"]
        character.update(loadout=[], known_identities=[])
        region, = contents["regions"]
        region.update(actors=[], items=[{"id": 9, "archetype": "rogue.healing", "at": [4, 2, 0]}])
        self.server(scenario=write_package(self, contents, "visible-potion-detour"))
        self.install_bot_extension()
        run = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect", self.address,
             "--budget", "6"], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(self.directory), "TOR_SERVER_TOKEN": TOKEN},
            text=True, capture_output=True, timeout=55,
        )
        rows = [json.loads(line) for line in run.stdout.splitlines()]
        self.assertEqual(run.returncode, 1, run.stderr)
        self.assertEqual(rows[0]["reason"], "seek_supplies")
        take = next(i for i, row in enumerate(rows) if row.get("reason") == "take")
        self.assertEqual(rows[take]["result"]["status"], "confirmed")
        self.assertEqual(rows[take + 1]["reason"], "test_unknown_potion")
        self.assertEqual(rows[take + 1]["previous_intention_phase"], "resolved")

    def test_bot_descends_after_its_sampled_potion_stops_levitation(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        character, = manifest["characters"]
        character["known_identities"] = []
        character["loadout"] = [{"archetype": "rogue.levitation", "quantity": 1}]
        manifest["stair_pairs"] = {"passage": {"upper": "1/start", "lower": "2/up"}}
        contents["regions"][0]["actors"] = []
        contents["regions"].append({"id": 2, "name": "Landing", "size": [5, 5, 2],
                                    "chamber": True, "anchors": {"up": [2, 2, 0]}})
        self.server(scenario=write_package(self, contents, "levitating-on-stairs"))
        self.install_bot_extension()
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect", self.address,
             "--budget", "100"], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(self.directory), "TOR_SERVER_TOKEN": TOKEN},
            text=True, capture_output=True, timeout=65,
        )
        rows = [json.loads(line) for line in result.stdout.splitlines()]
        self.assertTrue(rows, result.stderr)
        self.assertEqual(rows[0]["reason"], "test_unknown_potion")
        first_descent = next(i for i, row in enumerate(rows) if row.get("reason") == "descend")
        self.assertGreater(first_descent, 1)
        self.assertTrue(all(row["reason"] == "wait_for_ground_contact" for row in rows[1:first_descent]))
        self.assertEqual(rows[first_descent]["result"]["status"], "confirmed")
        self.assertEqual(rows[first_descent + 1]["floor"], 2)

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


    def test_player_identifies_ground_armor_and_replaces_starting_mail(self):
        self.check_ground_gear("armor", "rogue.ring_mail", "rogue.plate_mail", 43)

    def test_outbound_player_detours_for_armor(self):
        self.check_armor_route(carrying=False)

    def test_returning_player_takes_stairs_past_the_same_armor(self):
        self.check_armor_route(carrying=True)

    def check_armor_route(self, *, carrying):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        character, = contents["manifest"]["characters"]
        character.update(known_identities=["item.amulet.amulet-of-yendor"], loadout=(
            [{"archetype": "rogue.amulet_of_yendor", "quantity": 1}] if carrying else []))
        region, = contents["regions"]
        region.update(actors=[], items=[{"id": 10, "archetype": "rogue.plate_mail", "at": [3, 2, 0]}])
        contents["manifest"]["stair_pairs"] = {"exit": {"upper": "2/down", "lower": "1/start"}}
        contents["regions"].append({"id": 2, "name": "Upper", "size": [5, 5, 2],
                                    "chamber": True, "anchors": {"down": [2, 2, 0]}})
        self.server(scenario=write_package(self, contents, "armor-route"))
        self.install_bot_extension()
        result = subprocess.run([sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect",
                                 self.address, "--budget", "1"], cwd=self.directory, env={
                                     **os.environ, "PYTHONPATH": str(self.directory), "TOR_SERVER_TOKEN": TOKEN},
                                text=True, capture_output=True, timeout=30)
        self.assertEqual(result.returncode, 1, result.stderr)
        step, terminal = (json.loads(line) for line in result.stdout.splitlines())
        self.assertEqual(step["reason"], "ascend" if carrying else "seek_supplies")
        self.assertEqual(step["result"]["status"], "confirmed")
        self.assertEqual(terminal["previous_intention_phase"], "resolved")

    def test_player_identifies_cursed_ground_plate_and_keeps_starting_mail(self):
        self.check_ground_gear("armor", "rogue.ring_mail", "rogue.plate_mail", 42, cursed=True)

    def test_player_identifies_ground_long_sword_and_upgrades_primary_weapon(self):
        self.check_ground_gear("weapon", "rogue.mace", "rogue.long_sword", 43)

    def test_player_equips_unmeasured_plate_without_an_identification_scroll(self):
        self.check_ground_gear("armor", "rogue.ring_mail", "rogue.plate_mail", 43, identify=False)

    def test_player_protects_an_identified_upgrade_against_rust(self):
        self.check_ground_gear("armor", "rogue.ring_mail", "rogue.plate_mail", 43, protect=True)

    def check_ground_gear(self, category, starting, found, seed, *, cursed=False, identify=True, protect=False):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        contents["seed"] = seed
        archetypes = contents["manifest"]["archetypes"]
        if not cursed:
            archetypes[found]["equipment_generation"] = {"kind": "authored"}
        found_id = archetypes[found]["identity"]
        expected = archetypes[starting]["identity"] if cursed else found_id
        character, = contents["manifest"]["characters"]
        character["known_identities"] = [archetypes[starting]["identity"], found_id, f"item.scroll.identify-{category}"]
        character["loadout"] = [
            {"archetype": starting, "quantity": 1, "equipped_slot": 2 if category == "armor" else 0},
        ]
        if identify:
            character["loadout"].append({"archetype": f"rogue.identify_{category}", "quantity": 2})
        if protect:
            character["known_identities"].append("item.scroll.protect-armor")
            character["loadout"].append({"archetype": "rogue.protect_armor", "quantity": 1})
        contents["regions"][0].update(actors=[], items=[
            {"id": 10, "at": [1, 2, 0], "archetype": found, "quantity": 1}
        ])
        self.server(scenario=write_package(self, contents, "player-armor"))
        self.install_bot_extension()
        env = {**os.environ, "PYTHONPATH": str(self.directory), "TOR_SERVER_TOKEN": TOKEN}
        run = subprocess.run([sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect",
                              self.address, "--budget", "15"], cwd=self.directory, env=env,
                             text=True, capture_output=True, timeout=55)
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
        rows = [json.loads(line) for line in run.stdout.splitlines()]
        actions = [row["action"]["type"] for row in rows if "action" in row]
        self.assertEqual("identify" in actions, identify)
        if not identify:
            self.assertIn("try_armor", [row.get("reason") for row in rows])
        if protect:
            self.assertIn("protect_armor", [row.get("reason") for row in rows])
        equipment = rows[-1]["equipment_opportunities"][category]
        self.assertEqual(equipment["seen_on_ground"], 1)
        self.assertEqual(equipment["carried"], 2)
        self.assertEqual(equipment["worn"], 1 if cursed else 2)
        check = """
import os, sys
from tor_client_bot import Session
with Session.connect(sys.argv[1], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.observe()
    worn = next(slot.occupant for slot in turn.equipment if slot.kind == sys.argv[4])
    assert worn and worn.identity == sys.argv[2], (worn, turn.inventory)
    found = next(item for item in turn.inventory if item.identity == sys.argv[3])
    if sys.argv[5] == "True":
        assert found.known_cursed == (worn is not found), found
    else:
        assert found.physical_reduction == 6 and found.known_cursed is None, found
    if sys.argv[6] == "True":
        assert found.known_protected is True, found
"""
        kind = "body_armor" if category == "armor" else "weapon"
        final = subprocess.run([sys.executable, "-c", check, self.address, expected, found_id, kind, str(identify), str(protect)], cwd=self.directory,
                               env=env, text=True, capture_output=True, timeout=30)
        self.assertEqual(final.returncode, 0, f"{final.stderr}\n{[(r.get('reason'), r.get('result')) for r in rows]}")
        self.assertEqual("unequip" in actions, not cursed)
        self.assertEqual("equip" in actions, not cursed)

    def test_bot_equips_a_renamed_searching_ring(self):
        self.check_searching_ring(occupied=False)

    def test_bot_replaces_adornment_with_searching_and_keeps_protection(self):
        self.check_searching_ring(occupied=True)

    def test_bot_identifies_and_wears_a_ring_without_carrying_a_wand(self):
        self.check_searching_ring(occupied=False, identify=True)

    def check_searching_ring(self, *, occupied, identify=False):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        manifest["archetypes"]["rogue.ring_searching"]["name"] = "Discovery band"
        character, = manifest["characters"]
        character["known_identities"] = ["item.ring.ring-searching"]
        character["loadout"] = []
        if identify:
            character["known_identities"] = ["item.scroll.identify-wand"]
            character["loadout"] = [{"archetype": "rogue.identify_wand", "quantity": 1}]
        if occupied:
            for slot, name in [(7, "protection"), (8, "adornment")]:
                archetype = "rogue.ring_" + name
                character["loadout"].append({"archetype": archetype, "quantity": 1, "equipped_slot": slot})
                character["known_identities"].append("item.ring.ring-" + name)
        contents["regions"][0]["items"] = [
            {"id": 10, "at": [1, 2, 0], "archetype": "rogue.ring_searching", "quantity": 1}
        ]
        contents["regions"][0]["actors"] = []
        server = self.server(scenario=write_package(self, contents, "rogue-bot-rename-ring"))
        self.install_bot_extension()
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect", self.address,
             "--budget", str(2 + int(occupied) + 2 * int(identify))],
            cwd=self.directory, env={**os.environ, "PYTHONPATH": str(self.directory),
                                     "TOR_SERVER_TOKEN": TOKEN},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        pickup, *swaps, equip, terminal = (json.loads(line) for line in result.stdout.splitlines())
        if identify:
            self.assertEqual([row["action"]["type"] for row in swaps], ["read", "identify"])
            self.assertTrue(all(row["result"]["status"] == "confirmed" for row in swaps))
        if occupied:
            swap, = swaps
            self.assertEqual((swap["action"]["type"], swap["reason"]), ("unequip", "change_ring"))
            self.assertEqual(equip["action"]["slot"], 8)
        self.assertEqual((pickup["action"]["type"], pickup["result"]["status"]), ("take", "confirmed"))
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
from rogue_player import Adventurer
from tor_client_bot import Direction, ItemUse, Move, Outcome, Session, UseAbility, UseItem

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.wait_for_decision()
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    player.observe(turn)
    memory = player.maps[1]
    end = turn.cell_at((1, 0, 0)).key
    walls = {c.key for c in memory.adjacent(end) if c.wall}
    assert len(walls) == 3
    player.wall_searches.update(walls)
    memory.visited.update(memory.cells)
    action, reason = player.choose(turn)
    assert (action, reason) == (Move(Direction.EAST), "search"), (action, reason)
    assert turn.submit(action).status is Outcome.CONFIRMED
    turn = game.wait_for_decision()
    assert turn.cell.key == end
    player.observe(turn)
    memory.visited.update(memory.cells)
    action, reason = player.choose(turn)
    assert isinstance(action, UseAbility) and action.ability.identity == "rogue.search"
    assert turn.submit(action).status is Outcome.CONFIRMED
    player.admitted(turn, action)
    turn = game.wait_for_decision()
    player.observe(turn)
    assert turn.hunger == "nourished"
    memory.visited.update(memory.cells)
    player.wall_searches.update({c.key: 3 for c in memory.cells.values() if c.wall})
    action, reason = player.choose(turn)
    assert isinstance(action, UseItem) and action.use is ItemUse.READ and action.item.identity is None
    assert reason == "test_unknown_magic"
    assert turn.submit(action).status is Outcome.CONFIRMED
    assert game.wait_for_decision().tick > turn.tick
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bot_uses_an_offered_wand_to_displace_a_lethal_attacker(self):
        self.check_control_wand("teleport_away", 2)

    def test_bot_slows_a_dangerous_foe_before_contact(self):
        self.check_control_wand("slow", 3)

    def check_control_wand(self, wand, enemy_x):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        character, = contents["manifest"]["characters"]
        character.update(loadout=[{"archetype": f"rogue.wand_{wand}", "quantity": 1}],
                         known_identities=[f"item.wand.wand-{wand.replace('_', '-')}"])
        # Fix the opposed wand check for this displacement witness.
        species = contents["manifest"]["creatures"]["species"][character["creature"]["species"]]
        species.setdefault("modifiers", []).append({"scope": "always", "op": {
            "kind": "add", "stat": {"kind": "attribute", "attribute": "intellect"}, "value": 100}})
        region, = contents["regions"]
        region.update(size=[17, 9, 2], actors=[{"archetype": "rogue.hobgoblin", "id": 3,
                      "at": [enemy_x, 2, 0], "controller": "external", "turn_ticks": 1000000}])
        self.server(scenario=write_package(self, contents, "escape-device"))
        self.install_bot_extension()
        code = """
import os
from rogue_player import Adventurer
from tor_client_bot import Outcome, Session, Wait, Zap

address, token = os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"]
with Session.connect(address, token, 1) as game, Session.connect(address, token, 3) as enemy:
    turn = game.wait_for_decision()
    foe, = turn.hostiles
    slowing = os.environ["WAND"] == "slow"
    assert turn.in_contact(foe) != slowing
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    player.observe(turn)
    player.observed_loss[foe.target] = turn.health.current
    action, reason = player.choose(turn)
    assert isinstance(action, Zap) and reason == ("slow_approach" if slowing else "displace_threat"), (action, reason)
    receipt = turn.submit(action)
    assert receipt.status is Outcome.CONFIRMED, receipt
    assert enemy.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
    after = game.wait_for_decision()
    assert after.outcome_of(receipt).value == "resolved"
    if slowing:
        assert enemy.observe().has_condition("rogue.slowed")
    else:
        assert not any(c.target == foe.target and after.in_contact(c) for c in after.hostiles)
"""
        run = subprocess.run([sys.executable, "-c", code], cwd=self.directory,
                             env={**os.environ, "WAND": wand, "PYTHONPATH": str(ROOT / "scripts"),
                                  "BOT_TEST_ADDRESS": self.address, "TOR_SERVER_TOKEN": TOKEN},
                             capture_output=True, text=True, timeout=55)
        self.assertEqual(run.returncode, 0, run.stderr)

    def test_bot_reduces_melee_contacts_in_a_confined_room(self):
        self.check_outnumbered_move(False)

    def test_bot_enters_a_corridor_before_two_hostiles_reach_it(self):
        self.check_outnumbered_move(True)

    def check_outnumbered_move(self, approaching):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        contents["manifest"]["characters"][0]["loadout"] = []
        region, = contents["regions"]
        region.update(size=[3, 3, 2], anchors={"start": [1, 1, 0]},
                      walls=[[x, y, z] for x, y in [(0, 0), (0, 1), (0, 2), (2, 0)] for z in range(2)],
                      actors=[{"archetype": "rogue.hobgoblin", "id": actor, "at": point,
                               "controller": "external", "turn_ticks": 1000000}
                              for actor, point in [(3, [2, 1, 0]), (4, [1, 0, 0])]])
        if approaching:
            region.update(size=[8, 5, 2], anchors={"start": [3, 2, 0]},
                          walls=[[2, y, z] for y in (1, 3) for z in range(2)])
            for actor, y in zip(region["actors"], (2, 3)):
                actor["at"] = [5, y, 0]
        self.server(scenario=write_package(self, contents, "confined-melee-retreat"))
        self.install_bot_extension()
        code = """
import os
from contextlib import ExitStack
from rogue_player import Adventurer
from tor_client_bot import Move, Outcome, Session, Wait

with ExitStack() as stack:
    game, east, north = [stack.enter_context(Session.connect(os.environ["BOT_TEST_ADDRESS"],
                         os.environ["TOR_SERVER_TOKEN"], actor)) for actor in [1, 3, 4]]
    turn = game.wait_for_decision()
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    player.observe(turn)
    approaching = os.environ["APPROACHING"] == "True"
    assert sum(turn.in_contact(foe) for foe in turn.hostiles) == (0 if approaching else 2)
    step, reason = player.choose(turn)
    assert isinstance(step, Move) and reason == ("take_corridor" if approaching else "retreat"), (step, reason)
    destination = turn.cell_at(step.direction.offset).key
    if approaching:
        assert step.direction.offset == (-1, 0, 0)
    else:
        assert sum(turn.in_contact(foe, from_offset=step.direction.offset) for foe in turn.hostiles) == 1
    assert turn.submit(step).status is Outcome.CONFIRMED
    for enemy in (east, north):
        assert enemy.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
    after = game.wait_for_decision()
    assert after.cell.key == destination
    assert sum(after.in_contact(foe) for foe in after.hostiles) == (0 if approaching else 1)
"""
        run = subprocess.run([sys.executable, "-c", code], cwd=self.directory,
                             env={**os.environ, "APPROACHING": str(approaching), "PYTHONPATH": str(ROOT / "scripts"),
                                  "BOT_TEST_ADDRESS": self.address, "TOR_SERVER_TOKEN": TOKEN},
                             capture_output=True, text=True, timeout=55)
        self.assertEqual(run.returncode, 0, run.stderr)

    def test_pinned_bot_attacks_the_opponent_occupying_its_exit(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        manifest["characters"][0]["loadout"] = [{"archetype": "rogue.mace", "quantity": 1, "equipped_slot": 0}]
        manifest["stair_pairs"] = {"exit": {"upper": "1/down", "lower": "2/up"}}
        region, = contents["regions"]
        region.update(size=[3, 1, 2], anchors={"start": [1, 0, 0], "down": [2, 0, 0]},
                      actors=[{"archetype": "rogue.hobgoblin", "id": i, "at": [x, 0, 0],
                               "controller": "external", "turn_ticks": 1000000} for i, x in [(3, 0), (4, 2)]])
        contents["regions"].append({"id": 2, "name": "Landing", "size": [3, 3, 2],
                                    "chamber": True, "anchors": {"up": [1, 1, 0]}})
        self.server(scenario=write_package(self, contents, "stair-blocker"))
        self.install_bot_extension()
        code = """
import os
from contextlib import ExitStack
from rogue_player import Adventurer
from tor_client_bot import ActivityPhase, Outcome, Session, Wait

with ExitStack() as clients:
    game, west, east = [clients.enter_context(Session.connect(os.environ["BOT_TEST_ADDRESS"],
                        os.environ["TOR_SERVER_TOKEN"], i)) for i in (1, 3, 4)]
    turn = game.wait_for_decision()
    assert len(turn.hostiles) == 2 and not turn.walkable_directions
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    player.observe(turn)
    action, reason = player.choose(turn)
    assert reason == "melee" and any(c.stairs_down for c in turn.cells_of(action.target))
    attack = turn.submit(action)
    assert attack.status is Outcome.CONFIRMED
    for foe in (west, east):
        assert foe.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
    assert game.wait_for_decision().outcome_of(attack) is ActivityPhase.RESOLVED
"""
        run = subprocess.run([sys.executable, "-c", code], cwd=self.directory,
                             env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"),
                                  "BOT_TEST_ADDRESS": self.address, "TOR_SERVER_TOKEN": TOKEN},
                             capture_output=True, text=True, timeout=55)
        self.assertEqual(run.returncode, 0, run.stderr)

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
from rogue_player import Adventurer
from tor_client_bot import Direction, Move, Outcome, Session, Wait

address, token = os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"]
with Session.connect(address, token, 1) as game, Session.connect(address, token, 3) as enemy:
    turn = game.wait_for_decision()
    foe = next(c for c in turn.hostiles if c.name.casefold() == "hobgoblin" and (0, 1, 0) in c.cells)
    west = turn.cell_at((-1, 0, 0))
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    player.observe(turn)
    assert turn.health.current * 4 > turn.health.maximum
    player.observed_loss[foe.target] = (turn.health.current + 1) // 2
    step, reason = player.choose(turn)
    assert (step, reason) == (Move(Direction.WEST), "break_contact"), (step, reason)
    assert turn.submit(step).status is Outcome.CONFIRMED
    assert enemy.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
    assert game.wait_for_decision().cell.key == west.key
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
from rogue_player import Adventurer
from rogue_o_matic_cohort import death_cause
from tor_client_bot import CombatJournal, Move, Outcome, Session, UseItem, Wait

address, token = os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"]
with Session.connect(address, token, 1) as player, Session.connect(address, token, 3) as phantom:
    policy = Adventurer(26, "item.amulet.amulet-of-yendor")
    journal = CombatJournal()
    let_die = os.environ["BOT_TEST_UNSEEN_DEATH"] == "1"
    for _ in range(120 if let_die else 25):
        turn = player.observe()
        report = journal.read(turn)
        policy.observe(turn, report)
        if let_die and turn.dead:
            cause = death_cause({"self_target": turn.self_target, "last_combat_events": turn.recent_combat.records()})
            assert cause == "unseen_ability", cause
            raise SystemExit(0)
        if not let_die and policy.last_unseen_attack and turn.can_act:
            break
        enemy = phantom.observe()
        if enemy.can_act:
            victim = next(c for c in enemy.creatures if c.name == "Warrior")
            attack = enemy.attack(victim)
            assert attack is not None, victim
            result = enemy.submit(attack)
            assert result.status is Outcome.CONFIRMED, result
        if turn.can_act:
            result = turn.submit(Wait())
            assert result.status in {Outcome.CONFIRMED, Outcome.STALE} or result.code == "stale_context", result
        time.sleep(0.02)
    else:
        raise AssertionError(("No actionable unseen attack", turn.tick, turn.recent_combat.records()))
    assert all(c.name != "phantom" for c in turn.creatures)
    start = turn.cell.key
    step, reason = policy.choose(turn)
    use_sight = os.environ["BOT_TEST_SIGHT"] == "1"
    if use_sight:
        assert isinstance(step, UseItem) and reason == "reveal_attacker", (step, reason)
    else:
        assert isinstance(step, Move) and reason == "evade_unseen_attack", (step, reason)
    assert turn.submit(step).status is Outcome.CONFIRMED
    policy.admitted(turn, step)
    for _ in range(150):
        after = player.observe()
        if use_sight and any(c.name == "phantom" for c in after.creatures):
            break
        if not use_sight and after.can_act and after.cell.key != start:
            break
        opponent = phantom.observe()
        if opponent.can_act:
            result = opponent.submit(Wait())
            assert result.status in {Outcome.CONFIRMED, Outcome.STALE}, result
        time.sleep(0.02)
    else:
        raise AssertionError(("Unseen attack response did not settle", reason, after.tick))
    if use_sight:
        assert any(effect.identity == "rogue.see_invisible" and effect.active for effect in after.conditions)
    else:
        assert not any(event.unseen for event in after.recent_combat.incoming)
        policy.observe(after, journal.read(after))
        second, reason = policy.choose(after)
        assert isinstance(second, Move) and reason == "evade_unseen_attack", (second, reason)
        assert after.submit(second).status is Outcome.CONFIRMED
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
from rogue_player import Adventurer
from tor_client_bot import Direction, ItemUse, Move, Outcome, Session, UseItem, Wait

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    player = Adventurer(26, "item.amulet.amulet-of-yendor", depth=3)

    def submit(turn, action):
        assert turn.submit(action).status is Outcome.CONFIRMED
        player.admitted(turn, action)
        after = game.wait_for_decision()
        assert after.tick > turn.tick
        player.observe(after)
        return after

    turn = game.wait_for_decision()
    player.observe(turn)
    east = Move(Direction.EAST)
    turn = submit(turn, east)
    turn = submit(turn, Move(Direction.WEST))
    dose, = turn.inventory
    turn = submit(turn, UseItem(dose, ItemUse.DRINK))
    assert turn.health.maximum / 2 <= turn.health.current < turn.health.maximum * 0.8
    limited = os.environ["BOT_TEST_FOOD_LIMITED"] == "1"
    stairs = os.environ["BOT_TEST_STAIRS"] == "1"
    assert turn.hunger == "nourished"
    if not stairs:
        assert not any(c.stairs_down for c in turn.visible_cells)
        if not limited:
            player.ascent_ticks_by_floor[2] = turn.nutrition.ticks_until_empty
        action, reason = player.choose(turn)
        before = turn.recovery.elapsed_ticks
        origin = turn.cell.key
        if limited:
            assert isinstance(action, Move) and reason == "explore", (action, reason)
        else:
            assert (action, reason) == (Wait(), "recover")
        after = submit(turn, action)
        assert after.recovery.elapsed_ticks > before
        assert (after.cell.key == origin) == (not limited)
    else:
        for _ in range(2):
            if not limited:
                assert player.choose(turn) == (east, "recover_en_route")
            before = turn.recovery.elapsed_ticks
            turn = submit(turn, east)
            assert turn.recovery.elapsed_ticks > before
        assert turn.cell.stairs_down
        if not limited:
            assert player.choose(turn) == (Wait(), "recover")
            player.ascent_ticks_by_floor[2] = turn.nutrition.ticks_until_empty
        action, reason = player.choose(turn)
        assert (action, reason) == (Move(Direction.DOWN), "descend"), (action, reason)
        assert submit(turn, action).cell.stairs_up
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
from rogue_player import Adventurer
from tor_client_bot import ItemUse, Outcome, Session, UseAbility, UseItem, Wait

address, token = os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"]
with Session.connect(address, token, 1) as player, Session.connect(address, token, 3) as enemy:
    turn = player.wait_for_decision()
    dose = next(item for item in turn.inventory if item.identity == "item.potion.fixture.injury")
    assert turn.submit(UseItem(dose, ItemUse.DRINK)).status is Outcome.CONFIRMED
    assert enemy.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
    turn = player.wait_for_decision()
    hp = turn.health.current
    assert turn.health.maximum > hp and hp * 2 >= turn.health.maximum
    foe = next(c for c in turn.hostiles if c.name.casefold() == "hobgoblin")
    policy = Adventurer(26, "item.amulet.amulet-of-yendor")
    policy.observe(turn)
    policy.observed_loss[foe.target] = hp
    action, reason = policy.choose(turn)
    if os.environ["BOT_TEST_UNKNOWN_SCROLLS"] == "1":
        assert isinstance(action, UseItem) and action.use is ItemUse.READ and reason == "test_unknown_magic"
        assert turn.submit(action).status is Outcome.CONFIRMED
        policy.admitted(turn, action)
        turn = player.wait_for_decision()
        policy.observe(turn)
        assert any(item.category == "scroll" and item.identity is None for item in turn.inventory)
        action, reason = policy.choose(turn)
        assert isinstance(action, UseAbility) and reason == "melee", (action, reason)
        assert turn.submit(action).status is Outcome.CONFIRMED
        assert player.wait_for_decision().tick > turn.tick
    else:
        assert isinstance(action, UseItem) and action.use is ItemUse.DRINK and reason == "heal"
        assert action.item.name == "Restorative draught" and action.item.identity == "item.potion.healing"
        assert turn.submit(action).status is Outcome.CONFIRMED
        assert player.wait_for_decision().health.current > hp
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
from rogue_player import Adventurer
from tor_client_bot import Direction, ItemUse, Move, Outcome, Session, UseItem, Wait

address, token = os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"]
with Session.connect(address, token, 1) as game, Session.connect(address, token, 3) as enemy:
    turn = game.wait_for_decision()
    dose, = turn.inventory
    assert turn.submit(UseItem(dose, ItemUse.DRINK)).status is Outcome.CONFIRMED
    assert enemy.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
    turn = game.wait_for_decision()
    player = Adventurer(26, "item.amulet.amulet-of-yendor", depth=24)
    player.observe(turn)
    retreat = Move(Direction.UP)
    assert turn.submit(retreat).status is Outcome.CONFIRMED
    player.admitted(turn, retreat)
    turn = game.wait_for_decision()
    player.observe(turn)
    assert player.depth == 23 and turn.cell.stairs_down
    assert turn.health.current * 5 < turn.health.maximum * 4
    threat = next(c for c in turn.hostiles if c.name.casefold() == "hobgoblin")
    action, reason = player.choose(turn)
    assert reason == "retreat", (action, reason)
    player.observed_loss[threat.target] = turn.health.current
    action, reason = player.choose(turn)
    assert isinstance(action, Move) and action.direction not in {Direction.UP, Direction.DOWN}
    assert turn.submit(action).status is Outcome.CONFIRMED
    after = game.wait_for_decision()
    assert after.cell.key != turn.cell.key and not after.cell.stairs_up
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
from rogue_player import Adventurer
from tor_client_bot import Outcome, Session, Shot, Wait

address, token = os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"]
player, enemy = (Session.connect(address, token, actor) for actor in (1, 3))
try:
    turn = player.observe()
    policy = Adventurer(26, "item.amulet.amulet-of-yendor")
    policy.observe(turn)
    arrows = next(item for item in turn.inventory if item.identity == "item.weapon.arrow")
    action, reason = policy.choose(turn)
    assert isinstance(action, Shot) and reason == "launch_projectile", (action, reason)
    assert action.offset[0] > 0 and action.offset[1:] == (0, 0), action.offset
    assert action.target.hostile and action.projectile is arrows
    result = turn.submit(action)
    assert result.status is Outcome.CONFIRMED, result
    other = enemy.observe()
    if other.can_act:
        assert other.submit(Wait()).status in {Outcome.CONFIRMED, Outcome.STALE}
    for _ in range(150):
        after = player.observe()
        remaining = next(item.quantity for item in after.inventory if item.target == arrows.target)
        if remaining == 2:
            break
        time.sleep(0.02)
    else:
        raise AssertionError(("Arrow was not committed", after.tick, remaining))
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
from rogue_player import Adventurer
from tor_client_bot import Outcome, Pickup, Session

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.wait_for_decision()
    identity = os.environ["BOT_TEST_WEAPON"]
    row, = turn.ground_items
    assert row.item.identity == identity
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    player.observe(turn)
    key = turn.cell_at(row.offset).key
    assert player.maps[1].items[key][0].identity == identity
    take, reason = player.choose(turn)
    assert isinstance(take, Pickup) and take.item == row.item and reason == "take", (take, reason)
    assert turn.submit(take).status is Outcome.CONFIRMED
    current = game.wait_for_decision()
    player.observe(current)
    assert key not in player.maps[1].items
    weapon = next(item for item in current.inventory if item.identity == identity)
    assert weapon.quantity == row.item.quantity
    if identity == "item.weapon.dart":
        assert any(option.projectile == weapon and option.launcher is None
                   for option in current.projectiles())
    else:
        assert weapon.name == "Ash pike"
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
             "--output", str(output), "--budget", "512", "--record-decisions", "--timeout", "180", "42"],
            cwd=self.directory, env={**os.environ, "PYTHONPATH": str(self.directory)},
            text=True, capture_output=True, timeout=215,
        )
        errors = (output / "seed-42" / "bot.stderr").read_text(encoding="utf-8")
        self.assertEqual(result.returncode, 0, result.stderr + errors)
        summary = json.loads(result.stdout)
        self.assertEqual(summary["victories"], 1, summary)
        report, = summary["results"]
        self.assertTrue(report["victory_verified"])
        self.assertEqual(report["closest_return_floor"], 1)
        replay = subprocess.run(
            [sys.executable, "-c",
             "import sys; from tor_client_bot import Turn; "
             "turn = Turn.replay(sys.stdin.read()); "
             "assert turn.victorious and any(i.identity == 'item.amulet.amulet-of-yendor' for i in turn.inventory)"],
            cwd=self.directory, input=report["recording"], text=True, capture_output=True, timeout=20,
        )
        self.assertEqual(replay.returncode, 0, replay.stderr)
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
from rogue_player import Adventurer
from tor_client_bot import Equip, ItemUse, Outcome, Session, UseItem, Wait

def submit(game, turn, action):
    result = turn.submit(action)
    assert result.status is Outcome.CONFIRMED, result
    after = game.wait_for_decision()
    assert after.tick > turn.tick
    return after

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.wait_for_decision()
    dose = next(item for item in turn.inventory if item.name == "injury potion")
    turn = submit(game, turn, UseItem(dose, ItemUse.DRINK))
    start = turn.nutrition.reserve
    for _ in range(4):
        turn = submit(game, turn, Wait())
    before = start - turn.nutrition.reserve
    policy = Adventurer(26, "item.amulet.amulet-of-yendor")
    policy.observe(turn)
    action, reason = policy.choose(turn)
    assert isinstance(action, Equip) and action.item.identity == "item.ring.ring-slow-digestion"
    assert reason == "conserve_food", reason
    turn = submit(game, turn, action)
    start = turn.nutrition.reserve
    for _ in range(4):
        turn = submit(game, turn, Wait())
    after = start - turn.nutrition.reserve
    assert 0 < after < before, (before, after)
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
import json
from rogue_player import Adventurer
from tor_client_bot import ActivityPhase, Direction, Move, Outcome, Session, Turn, UseAbility

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.wait_for_decision()
    anchor = turn.cell_at((1, 0, 0)).key
    east = turn.cell_at((2, 0, 0)).key
    player = Adventurer(26, "item.amulet.amulet-of-yendor", depth=4)
    # Prior view from the other side of the same wall disclosed a stair.
    memory_picture = json.loads(turn.recording())
    prior = memory_picture["state"]["observation"]
    prior["visible_cells"] = [
        {"key": "remembered-stair", "position": {"x": 0, "y": 0, "z": 0}, "wall": False,
         "stairs_up": True, "stairs_down": False},
        {"key": east, "position": {"x": -1, "y": 0, "z": 0}, "wall": True,
         "stairs_up": False, "stairs_down": False}]
    prior["ground_items"] = []
    player.observe(Turn.replay(json.dumps(memory_picture)))
    player.observe(turn)
    memory = player.maps[4]
    memory.visited.update(memory.cells)
    action, reason = player.choose(turn)
    assert (action, reason) == (Move(Direction.EAST), "search"), (action, reason)
    assert turn.submit(action).status is Outcome.CONFIRMED
    turn = game.wait_for_decision()
    assert turn.cell.key == anchor
    player.observe(turn)
    memory.visited.update(memory.cells)
    action, reason = player.choose(turn)
    assert isinstance(action, UseAbility) and action.ability.identity == "rogue.search"
    result = turn.submit(action)
    assert result.status is Outcome.CONFIRMED
    assert game.wait_for_decision().outcome_of(result) is ActivityPhase.RESOLVED
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
from rogue_player import Adventurer
from tor_client_bot import Outcome, Session, UseItem

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.wait_for_decision()
    origin = turn.cell.key
    scroll = next(item for item in turn.inventory if item.identity == "item.scroll.teleportation")
    player = Adventurer(26, "item.amulet.amulet-of-yendor", depth=4)
    player.observe(turn)
    memory = player.maps[4]
    from dataclasses import replace
    memory.cells["remembered-landing"] = replace(memory.cells[origin], key="remembered-landing", stairs_up=True)
    memory.visited.update(memory.cells)
    for key in memory.cells:
        player.searches[key] = 8
    action, reason = player.choose(turn)
    assert isinstance(action, UseItem) and action.item == scroll and reason == "find_return_route"
    assert turn.submit(action).status is Outcome.CONFIRMED
    after = game.wait_for_decision()
    assert after.cell.key != origin and not any(item.target == scroll.target for item in after.inventory)
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
from rogue_player import Adventurer
from tor_client_bot import Direction, ItemUse, Move, Outcome, Session, UseItem

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    carrying = os.environ["BOT_TEST_CARRYING"] == "1"
    scouting = os.environ["BOT_TEST_SCOUTING"] == "1"
    player = Adventurer(int(os.environ["BOT_TEST_RETRIEVAL_DEPTH"]), "item.amulet.amulet-of-yendor", depth=3)
    turn = game.wait_for_decision()
    player.observe(turn)

    def advance(turn, action):
        assert turn.submit(action).status is Outcome.CONFIRMED
        player.admitted(turn, action)
        following = game.wait_for_decision()
        assert following.tick > turn.tick
        player.observe(following)
        return following

    assert bool(turn.ground_items) != scouting
    if scouting:
        action, reason = player.choose(turn)
        assert isinstance(action, UseItem) and action.use is ItemUse.EAT, (action, reason)
        turn = advance(turn, action)
    for _ in range(7 if scouting else 28):
        turn = advance(turn, Move(Direction.EAST))
    assert not turn.ground_items and turn.hunger == "nourished"
    direction = Direction.UP if carrying else Direction.DOWN
    assert player.choose(turn)[0] == Move(direction)
    player.ascent_ticks_by_floor[2] = (turn.nutrition.reserve + 1) * turn.nutrition.ticks_per_unit
    if scouting:
        assert player.food_value["item.food.ration"] > 0
        player.descent_ticks_by_floor[2] = 110000
    action, reason = player.choose(turn)
    assert isinstance(action, Move) and action.direction.offset[0] == (1 if scouting else -1), (action, reason)
    destination = turn.cell_at(action.direction.offset).key
    turn = advance(turn, action)
    assert turn.cell.key == destination
    for _ in range(45):
        action, reason = player.choose(turn)
        assert reason in {"scout_for_food", "forage", "seek_item", "take"}, (action, reason)
        turn = advance(turn, action)
        if sum(item.quantity for item in turn.inventory if item.identity == "item.food.ration") == (2 if scouting else 1):
            break
    else:
        raise AssertionError("The bot did not collect the ration")
    action, reason = player.choose(turn)
    assert reason == "seek_stairs", (action, reason)
    advance(turn, action)
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
from rogue_player import Adventurer
from tor_client_bot import Direction, Move, Outcome, Session, Wait

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.wait_for_decision()
    target = turn.cell_at((-1, 0, 0))
    assert any(c.name == "orc" and (2, 0, 0) in c.cells for c in turn.creatures)
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    player.observe(turn)
    assert player.carrying_objective(turn)
    player.maps[1].visited.update(c.key for c in turn.visible_cells
                                 if c.offset[2] == 0 and c.key != target.key and not c.wall)
    action, reason = player.choose(turn)
    assert (action, reason) == (Move(Direction.WEST), "explore"), (action, reason)
    assert turn.submit(action).status is Outcome.CONFIRMED
    with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 3) as orc:
        assert orc.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
    assert game.wait_for_decision().cell.key == target.key
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
