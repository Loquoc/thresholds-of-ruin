"""Ordinary Rogue bot startup against the bundled authored-growth scenario."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys

from process_harness import ProcessTestCase, ROOT, TOKEN


def add_injury_potion(manifest, *, damage=5, stackable=False):
    manifest["archetypes"]["injury"] = {
        "class": "potion", "identity": "item.potion.fixture.injury", "name": "injury potion", "stackable": stackable,
        "consumable": {"effects": [{"kind": "damage", "damage": {
            "kind": "components", "components": [
                {"category": "vital", "amount": {"kind": "fixed", "amount": damage}}
            ],
        }}]},
    }


def identify_staff(game, turn, opponents):
    from tor_client_bot import ItemUse, Outcome, UseItem, Wait

    scroll = next(item for item in turn.inventory if ItemUse.READ in item.uses)
    assert turn.submit(UseItem(scroll, ItemUse.READ)).status is Outcome.CONFIRMED
    for opponent in opponents:
        assert opponent.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
    turn = game.wait_for_decision()
    prompt = turn.identification
    staff = next(item for item in prompt.items if item.target == turn.devices[0].item.target)
    assert turn.submit(prompt.select(staff)).status is Outcome.CONFIRMED
    turn = game.wait_for_decision()
    assert turn.can_act and turn.identification is None
    assert turn.devices[0].charges >= 3
    return turn


def check_resisted_polymorph(game, player, after, foe, initial_charges, second_attacker):
    from tor_client_bot import AbilityOutcome, Outcome, Zap

    player.observe(after)
    retry, reason = player.choose(after)
    assert after.devices[0].charges == initial_charges - 1
    if second_attacker:
        assert any(other.target != foe.target and after.in_contact(other) for other in after.hostiles)
        assert reason != "polymorph_threat", (retry, reason)
        return
    assert isinstance(retry, Zap) and reason == "polymorph_threat", (retry, reason)
    receipt = after.submit(retry)
    assert receipt.status is Outcome.CONFIRMED
    player.admitted(after, retry, receipt, reason=reason)
    final = game.wait_for_decision()
    assert final.outcome_of(receipt).value == "resolved"
    assert final.devices[0].charges == initial_charges - 2
    assert any(effect.target == foe.target and effect.ability == "rogue.wand_polymorph"
               and effect.outcome is AbilityOutcome.UNAFFECTED for effect in final.recent_combat.outgoing)
    player.observe(final)
    assert player.choose(final)[1] != "polymorph_threat"


def check_wand_encounter(address, *, wand, contact, resists, blocked_ray, second_attacker, unknown=False):
    from contextlib import ExitStack
    from rogue_player import Adventurer
    from tor_client_bot import AbilityOutcome, Outcome, Session, Wait, Zap

    token = os.environ["TOR_SERVER_TOKEN"]
    with ExitStack() as sessions:
        game = sessions.enter_context(Session.connect(address, token, 1))
        enemy = sessions.enter_context(Session.connect(address, token, 3))
        opponents = [enemy]
        if second_attacker:
            opponents.append(sessions.enter_context(Session.connect(address, token, 4)))
        turn = game.wait_for_decision()
        if resists:
            turn = identify_staff(game, turn, opponents)
        if second_attacker:
            foe = next(foe for foe in turn.hostiles if foe.name.casefold() == "griffin")
        else:
            foe, = turn.hostiles
        assert turn.in_contact(foe) == contact
        before = enemy.observe()
        player = Adventurer(26, "item.amulet.amulet-of-yendor")
        player.observe(turn)
        player.opponents.damage_by_target[foe.target] = turn.health.current
        action, reason = player.choose(turn)
        if blocked_ray:
            assert not isinstance(action, Zap), (action, reason)
            direct = turn.aim_device(turn.devices[0], foe, clear_horizontal_ray=True)
            assert direct and turn.aim_device(turn.devices[0], foe, reflected_ray_range=6) is None
            # Fire the declined shot in this fixture to verify what the clearance check prevented.
            assert turn.submit(direct).status is Outcome.CONFIRMED
            assert enemy.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
            reflected = game.wait_for_decision()
            assert reflected.health.current < turn.health.current
            assert any(effect.target == turn.self_target and effect.ability == "rogue.wand_lightning"
                       and effect.outcome is AbilityOutcome.APPLIED for effect in reflected.recent_combat.outgoing)
            return
        expected = {"fire": "elemental_ray", "cold": "elemental_ray", "lightning": "elemental_ray",
                    "slow": "slow_approach", "teleport_away": "displace_threat",
                    "magic_missile": "magic_missile", "polymorph": "polymorph_threat"}
        if unknown:
            assert turn.devices[0].item.identity is None
        assert isinstance(action, Zap) and reason == ("test_unknown_device" if unknown else expected[wand]), (action, reason)
        receipt = turn.submit(action)
        assert receipt.status is Outcome.CONFIRMED, receipt
        player.admitted(turn, action, receipt, reason=reason)
        if not resists:
            assert enemy.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
        after = game.wait_for_decision()
        assert after.outcome_of(receipt).value == "resolved"
        if unknown:
            assert after.devices[0].item.identity == "item.wand.wand-fire"
            assert after.health == turn.health
            return
        if wand == "slow":
            assert enemy.observe().has_condition("rogue.slowed")
        elif wand in {"magic_missile", "fire", "cold", "lightning"}:
            assert enemy.observe().health.current < before.health.current
            assert after.health.current == turn.health.current
        elif wand == "polymorph":
            outcome = AbilityOutcome.UNAFFECTED if resists else AbilityOutcome.APPLIED
            assert any(e.target == foe.target and e.ability == "rogue.wand_polymorph" and e.outcome is outcome
                       for e in after.recent_combat.outgoing), after.recent_combat.outgoing
            changed = enemy.observe()
            assert changed.hit_dice is not None and before.hit_dice is not None
            if resists:
                assert changed.hit_dice == before.hit_dice
                check_resisted_polymorph(game, player, after, foe, turn.devices[0].charges, second_attacker)
            else:
                assert changed.hit_dice < before.hit_dice, (changed.hit_dice, before.hit_dice)
        else:
            assert not any(c.target == foe.target and after.in_contact(c) for c in after.hostiles)


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

    def check_policy_scenario(self, contents, name, code):
        from discovery_package import write_package

        self.install_bot_extension()
        server = self.server(scenario=write_package(self, contents, name))
        try:
            result = subprocess.run(
                [sys.executable, "-c", code], cwd=self.directory,
                env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                     "BOT_TEST_ADDRESS": self.address}, text=True, capture_output=True, timeout=55,
            )
        finally:
            server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_budget_trace_identifies_the_action_rolled_back_by_the_server(self):
        from test_growth_failure_process import growth_package

        self.install_bot_extension()
        self.server(scenario=growth_package(self, reject_growth=True))
        code = """
import os
from rogue_player import Adventurer, play
from tor_client_bot import Session, UseAbility, Wait

class GrowthPlayer(Adventurer):
    def choose(self, turn):
        if turn.tick == 0:
            return Wait(), "wait"
        attack = next(ability for ability in turn.abilities if ability.identity == "fixture_growth_finisher")
        target = next(foe for foe in turn.creatures if foe.name == "practice target")
        return UseAbility(attack, target), "trigger_growth"

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    rows = []
    report = play(game, GrowthPlayer(26, "item.amulet.amulet-of-yendor"), 2, rows.append)
    assert report["terminal"] == "action_budget", report
    assert report["previous_intention_id"] == rows[-1]["result"]["receipt"]["intention"]
    assert report["previous_intention_phase"] == "failed", report
    assert report["previous_intention_failure"] == "rolled_back", report
    assert report["previous_failed_action"] == "use_ability", report
"""
        run = subprocess.run([sys.executable, "-c", code], cwd=self.directory,
                             env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                                  "BOT_TEST_ADDRESS": self.address}, text=True, capture_output=True, timeout=55)
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)

    def test_lost_reply_identifies_the_single_durable_action(self):
        self.server(seed=None, spectator=False)
        self.install_bot_extension()
        code = """
import os, sys
from stream_relay import StreamRelay
from tor_client_bot import Outcome, Session, Wait

relay = StreamRelay(sys.argv[1])
try:
    session = Session.connect(relay.address, os.environ["TOR_SERVER_TOKEN"], 1)
    turn = session.wait_for_decision()
    relay.gate.clear()
    result = turn.submit(Wait())
    assert relay.held.is_set()
    assert result.status is Outcome.UNKNOWN and result.request_id, result
    print(result.request_id)
finally:
    relay.close()
"""
        run = subprocess.run([sys.executable, "-c", code, self.address], cwd=self.directory,
                             env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN},
                             capture_output=True, text=True, timeout=40)
        self.assertEqual(run.returncode, 0, run.stderr)
        self.flush_save(actor=1)
        with sqlite3.connect(self.save) as db:
            records = [json.loads(frame[24:])["record"] for (frame,) in
                       db.execute("SELECT frame FROM journal WHERE sequence > 0 ORDER BY sequence")]
        admission, = [r for r in records if r["receipt"] and
                      r["receipt"]["command"].get("action") == {"type": "wait"}]
        self.assertEqual(admission["receipt"]["request_id"], run.stdout.strip())
        execution, = [r["entry"]["content"] for r in records
                      if r["entry"]["content"].get("admission") == admission["entry"]["id"]]
        self.assertEqual(execution["type"], "intention_started")

    def test_session_keeps_action_error_when_save_worker_is_delayed(self):
        self.server("--save-target-ms", "1", "--save-max-ms", "60000", "--save-idle-ms", "0",
                    seed=None, spectator=False, writer_barrier=True)
        self.install_bot_extension()
        code = """
import os, sys
from tor_client_bot import Outcome, Session, Wait

class ActionError(Exception):
    pass

try:
    with Session.connect(sys.argv[1], os.environ["TOR_SERVER_TOKEN"], 1) as session:
        turn = session.wait_for_decision()
        assert turn.submit(Wait()).status is Outcome.CONFIRMED
        raise ActionError("movement outcome unknown")
except ActionError as error:
    assert str(error) == "movement outcome unknown"
"""
        with self.held_save_writer() as barrier:
            run = subprocess.run([sys.executable, "-c", code, self.address], cwd=self.directory,
                                 env={**os.environ, "TOR_SERVER_TOKEN": TOKEN},
                                 capture_output=True, text=True, timeout=25)
            barrier.wait()
            barrier.assert_held()
            self.assertEqual(run.returncode, 0, run.stderr)
            self.assertIn("Session close also failed: Saving is behind schedule", run.stderr)
        self.flush_save(actor=1)
        barrier.assert_released()

    def test_player_waits_through_a_fall_then_walks_on_landing(self):
        self.server(scenario="physics", seed=None, spectator=False)
        self.install_bot_extension()
        code = """
import os, sys
from rogue_player import Adventurer
from tor_client_bot import Direction, Move, Outcome, Session, Wait

with Session.connect(sys.argv[1], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    turn = game.wait_for_decision()
    assert turn.submit(Wait()).status is Outcome.CONFIRMED
    turn = game.wait_for_decision()
    assert turn.falling
    denied = turn.submit(Move(Direction.EAST))
    assert denied.status is Outcome.REJECTED, denied
    assert 'ground' in denied.reason.lower(), denied
    turn = game.observe()
    assert turn.falling
    for _ in range(6):
        player.observe(turn)
        action, reason = player.choose(turn)
        if not turn.falling:
            assert reason != "settle_fall"
            break
        assert isinstance(action, Wait) and reason == "settle_fall", (action, reason)
        assert turn.submit(action).status is Outcome.CONFIRMED
        turn = game.wait_for_decision()
    assert not turn.falling
    origin = turn.cell.key
    assert turn.submit(Move(Direction.EAST)).status is Outcome.CONFIRMED
    assert game.wait_for_decision().cell.key != origin
"""
        run = subprocess.run([sys.executable, "-c", code, self.address], cwd=self.directory,
                             env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN},
                             capture_output=True, text=True, timeout=45)
        self.assertEqual(run.returncode, 0, run.stderr)

    def test_player_api_interprets_real_observation_and_submits_wait(self):
        self.server(scenario=ROOT / "scenarios/rogue")
        self.install_bot_extension()
        code = """
import os
from tor_client_bot import ActivityPhase, Outcome, Session, Unequip, Wait

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
    assert 'grip' in refusal.reason.lower(), refusal
    turn = game.observe()
    assert turn.tick == 0 and any(slot.occupant and slot.occupant.identity == "item.weapon.mace"
                                  for slot in turn.equipment)
    admitted = turn.submit(Wait())
    assert admitted.status is Outcome.CONFIRMED, admitted
    assert admitted.activity is not None
    assert admitted.activity.actor == 1
    following = game.wait_for_decision()
    assert following.outcome_of(admitted) is ActivityPhase.RESOLVED, following.finished_activities
    mace = next(slot for slot in following.equipment
               if slot.occupant and slot.occupant.identity == "item.weapon.mace")
    removed = following.submit(Unequip(mace))
    assert removed.status is Outcome.CONFIRMED, removed
    following = game.wait_for_decision()
    assert following.outcome_of(removed) is ActivityPhase.RESOLVED
    bow = next(choice for choice in following.equip_choices()
               if choice.item.identity == "item.weapon.shortbow" and choice.slot.index == bow.slot.index)
    equipped = following.submit(bow)
    assert equipped.status is Outcome.CONFIRMED, equipped
    following = game.wait_for_decision()
    assert following.outcome_of(equipped) is ActivityPhase.RESOLVED
    assert following.equipment[bow.slot.index].occupant.identity == "item.weapon.shortbow"
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
        purchases = [row["action"] for row in actions if row["reason"] == "earned_training"]
        self.assertEqual([(p["hit_die"], p["pool"]) for p in purchases], [(2, "advancement"), (3, "advancement")])
        self.assertEqual(purchases[0]["id"], "rattling_blow")
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
        budget = 70  # Include the first encounter after starting at the up stair.
        output = self.directory / "cohort"
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic_cohort.py"),
             "--server", str(self.bin / ("tor-server" + self.suffix)),
             "--output", str(output), "--budget", str(budget), "42"],
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
        bindings = summary["inputs"]["bot_bindings_sha256"]
        native = "_native.pyd" if os.name == "nt" else "_native.so"
        for name in ("__init__.py", "game.py", native):
            expected = hashlib.sha256((self.directory / "tor_client_bot" / name).read_bytes()).hexdigest()
            self.assertEqual(bindings[name], expected)
            self.assertEqual(hashlib.sha256((output / "policy/tor_client_bot" / name).read_bytes()).hexdigest(), expected)
        self.assertEqual(summary["inputs"]["policy_sha256"], hashes["rogue_o_matic.py"])
        self.assertEqual({p.name for p in (output / "policy").glob("*.py")}, hashes.keys())
        for name, digest in hashes.items():
            self.assertEqual(hashlib.sha256((output / "policy" / name).read_bytes()).hexdigest(), digest)
        report, = summary["results"]
        self.assertEqual(report["terminal"], "action_budget")
        self.assertEqual(report["actions"], budget)
        self.assertGreater(report["floors"]["1"]["moves_admitted"], 0)
        self.assertIsNotNone(report["floors"]["1"]["first_reserve"])
        trace = (output / "seed-42" / "turns.jsonl").read_text(encoding="utf-8")
        turns = [json.loads(line) for line in trace.splitlines() if line]
        self.assertNotIn("earned_training", {turn.get("reason") for turn in turns})
        disclosed = [turn["health_recovery"] for turn in turns if "action" in turn]
        self.assertEqual(len(disclosed), budget)
        self.assertTrue(all(int(clock["period_ticks"]) > 0 for clock in disclosed))
        events = [event for turn in turns if "action" in turn
                  for event in turn["events_since_previous_action"]]
        self.assertTrue(any(event["type"] == "ability" for event in events), events)

    def test_bot_does_not_repeat_a_failed_throw_at_a_stalled_tick(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        manifest["characters"][0]["loadout"] = [
            {"archetype": "rogue.mace", "quantity": 1, "equipped_slot": 0},
            {"archetype": "rogue.dart", "quantity": 12},
        ]
        contents["regions"][0]["actors"][0].update(archetype="rogue.hobgoblin", at=[4, 2, 0])
        self.server(scenario=write_package(self, contents, "unfunded-throw"))
        self.install_bot_extension()
        code = """
import json, os, sys
from rogue_player import Adventurer
from tor_client_bot import ActivityPhase, Outcome, Session, Turn, Wait

token = os.environ["TOR_SERVER_TOKEN"]
with Session.connect(sys.argv[1], token, 1) as game, Session.connect(sys.argv[1], token, 3) as enemy:
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    turn = game.wait_for_decision()
    for attempt in range(12):
        player.observe(turn)
        action, reason = player.choose(turn)
        assert reason == "throw_projectile", (attempt, action, reason)
        receipt = turn.submit(action)
        assert receipt.status is Outcome.CONFIRMED, receipt
        player.admitted(turn, action, receipt, reason=reason)
        if attempt == 0:
            first_picture = turn.recording()
            assert enemy.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
        following = game.wait_for_decision()
        if following.outcome_of(receipt) is ActivityPhase.FAILED:
            break
        turn = following
    assert following.outcome_of(receipt) is ActivityPhase.FAILED, following.finished_activities
    assert following.tick == turn.tick, (turn.tick, following.tick)
    failed = next(activity for activity in following.finished_activities
                  if activity.intention == receipt.activity.intention)
    assert (failed.actor, failed.branch) == (receipt.activity.actor, receipt.activity.branch)
    assert failed.failure == "unfunded", failed
    def resources(picture):
        return json.loads(picture.recording())["state"]["observation"]["combat"]["own_stats"]["resources"]
    def inventory(picture):
        return [(item.identity, item.quantity) for item in picture.inventory]
    assert resources(following) == resources(turn)
    assert inventory(following) == inventory(turn)
    earlier = Turn.replay(first_picture)
    assert earlier.tick < following.tick
    assert player.ranged_combat(earlier, []) is None
    player.observe(following)
    action, reason = player.choose(following)
    assert action is not None and reason not in {"throw_projectile", "launch_projectile"}, (action, reason)
    assert following.submit(action).status is Outcome.CONFIRMED
    assert game.wait_for_decision().tick > following.tick
"""
        run = subprocess.run([sys.executable, "-c", code, self.address], cwd=self.directory,
                             env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN},
                             capture_output=True, text=True, timeout=45)
        self.assertEqual(run.returncode, 0, run.stderr)

    def test_bundled_seed_47_replays_the_bots_decisions_and_projectile_receipts(self):
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
        self.assertEqual(summary["runs_by_max_depth"], {str(report["max_depth"]): 1})
        trace = (output / "seed-47" / "turns.jsonl").read_text(encoding="utf-8")
        turns = [json.loads(line) for line in trace.splitlines()]
        self.assertEqual([i for i, row in enumerate(turns) if "terminal" in row], [len(turns) - 1])
        self.assertEqual(turns[-1]["terminal"], report["terminal"])
        self.assertEqual(len(turns) - 1, report["actions"])
        shots = [row for row in turns if row.get("reason") in {"throw_projectile", "launch_projectile"}
                 and row["result"]["status"] == "confirmed"]
        self.assertTrue(shots, "Seed 47 produced no confirmed projectile actions")
        weapons = report["equipment_opportunities"]["weapon"]
        for field in ("projectile", "launcher"):
            distinct = {row["action"][field] for row in shots if row["action"][field] is not None}
            self.assertEqual(weapons.get(f"{field}_admitted", 0), len(distinct))
        for shot in shots:
            self.assertTrue(json.loads(shot["recording"]).get("queried_projectiles"),
                            f"Missing recorded projectile choices at tick {shot['tick']}")
        ticks = [row["tick"] for row in shots]
        self.assertEqual(len(ticks), len(set(ticks)), ticks)
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
    expected_id = previous.intention if previous else None
    assert row.get("previous_intention_id") == expected_id, (turn.tick, expected_id)
    expected_phase = turn.outcome_of(previous) if previous else None
    assert row.get("previous_intention_phase") == expected_phase, (turn.tick, expected_phase)
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
        self.check_sealed_room(after_descent=False)

    def test_bot_searches_a_sealed_landing_again_before_reporting_no_route(self):
        self.check_sealed_room(after_descent=True)

    def check_sealed_room(self, *, after_descent):
        from discovery_package import write_package
        from rogue_o_matic_cohort import trace_report
        from utility_potions_package import fixture

        contents = fixture()
        contents["manifest"]["characters"][0]["loadout"] = []
        region, = contents["regions"]
        region["actors"] = []
        region["walls"] = [[x, y, z] for x, y in ((0, 2), (2, 2), (1, 1), (1, 3)) for z in range(2)]
        if after_descent:
            contents["manifest"]["stair_pairs"] = {"landing": {"upper": "1/start", "lower": "2/up"}}
            contents["regions"].append({"id": 2, "name": "Sealed landing", "size": [7, 5, 2],
                                        "chamber": True, "anchors": {"up": [1, 2, 0]},
                                        "walls": region.pop("walls")})
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
        depth = 2 if after_descent else 1
        self.assertEqual((navigation["floor"], navigation["visited_cells"]), (depth, 1))
        self.assertEqual(navigation["known_up_stairs"], 1 if after_descent else 0)
        self.assertEqual(navigation["known_down_stairs"], 0)
        searches = sum(row["action"].get("id") == "rogue.search" for row in turns[:-1])
        self.assertEqual(navigation["searches_admitted"], searches)
        self.assertGreater(searches, 0)
        if after_descent:
            self.assertEqual(searches, 6)
        trace = self.directory / "trace.jsonl"
        trace.write_text(result.stdout, encoding="utf-8")
        _, progress = trace_report(trace)
        self.assertEqual(progress["floors"][str(depth)]["searches_resolved"], searches)
        self.assertEqual(progress["floors"][str(depth)]["searches_failed"], 0)

    def test_bot_routes_away_from_a_foe_toward_a_disclosed_stair(self):
        self.check_safe_stair_route("open")

    def test_outnumbered_bot_takes_nearby_downstairs(self):
        self.check_outnumbered_descent([(2, 3), (3, 3)])

    def test_bot_steps_onto_stairs_before_two_foes_close_from_separate_directions(self):
        self.check_outnumbered_descent([(2, 0), (2, 4)])

    def check_outnumbered_descent(self, positions):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        contents["manifest"]["characters"][0]["loadout"] = []
        contents["manifest"]["stair_pairs"] = {"escape": {"upper": "1/down", "lower": "2/up"}}
        room, = contents["regions"]
        room["anchors"].update(start=[2, 2, 0], down=[3, 2, 0])
        room["actors"] = [
            {"archetype": species, "id": number, "at": [x, y, 0],
             "controller": "external", "turn_ticks": 1000000}
            for (number, species), (x, y) in zip(((3, "rogue.emu"), (4, "rogue.hobgoblin")), positions)
        ]
        contents["regions"].append({"id": 2, "name": "Landing", "size": [5, 5, 2],
                                    "chamber": True, "anchors": {"up": [2, 2, 0]}})
        self.server(scenario=write_package(self, contents, "outnumbered-descent"))
        self.install_bot_extension()
        code = """
from contextlib import ExitStack
import os, sys
from rogue_player import Adventurer
from tor_client_bot import Direction, Move, Outcome, Session, Wait

with ExitStack() as stack:
    game, emu, hobgoblin = [stack.enter_context(Session.connect(sys.argv[1], os.environ["TOR_SERVER_TOKEN"], i))
                             for i in (1, 3, 4)]
    player = Adventurer(26, "item.amulet.amulet-of-yendor", depth=2)
    for direction, expected in ((Direction.EAST, "retreat_to_stairs"), (Direction.DOWN, "retreat_downstairs")):
        turn = game.wait_for_decision()
        player.observe(turn)
        action, reason = player.choose(turn)
        assert (action, reason) == (Move(direction), expected), (action, reason)
        receipt = turn.submit(action)
        assert receipt.status is Outcome.CONFIRMED
        player.admitted(turn, action, receipt, reason=reason)
        if direction == Direction.EAST:
            for enemy in (emu, hobgoblin):
                assert enemy.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
    landing = game.wait_for_decision()
    player.observe(landing)
    assert player.depth == 3 and landing.cell.stairs_up
"""
        result = subprocess.run([sys.executable, "-c", code, self.address], cwd=self.directory,
                                env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN},
                                text=True, capture_output=True, timeout=45)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_bot_keeps_the_refuge_after_escaping_a_guarded_stair(self):
        self.check_stair_refuge(enemy_x=2, expect_escape=True)

    def test_bot_keeps_the_refuge_when_it_escaped_before_contact(self):
        self.check_stair_refuge(enemy_x=3, expect_escape=True)

    def test_bot_keeps_exploring_the_landing_while_the_drainer_is_distant(self):
        self.check_stair_refuge(enemy_x=5, expect_escape=False)

    def test_amulet_carrier_ascends_instead_of_fighting_an_ordinary_enemy(self):
        self.check_stair_refuge(enemy_x=2, expect_escape=True, carrying=True)

    def check_stair_refuge(self, enemy_x, *, expect_escape, carrying=False):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        character, = contents["manifest"]["characters"]
        character["loadout"] = [{"archetype": "rogue.amulet_of_yendor", "quantity": 1}] if carrying else []
        if carrying:
            character["known_identities"].append("item.amulet.amulet-of-yendor")
        contents["manifest"]["stair_pairs"] = {"escape": {"upper": "2/down", "lower": "1/start"}}
        contents["regions"][0]["actors"] = [
            {"archetype": "rogue.hobgoblin", "id": 3, "at": [enemy_x, 2, 0], "controller": "external",
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
    if sys.argv[3] == "carrying":
        assert player.carrying_objective(turn) and turn.in_contact(turn.hostiles[0])
    else:
        player.opponents.draining_names.add(turn.hostiles[0].name)
    player.observe(turn)
    step, reason = player.choose(turn)
    if sys.argv[2] == "explore":
        assert turn.hostiles[0].distance == 4
        assert isinstance(step, Move) and reason == "explore", (step, reason)
        destination = turn.cell_at(step.direction.offset).key
        assert turn.submit(step).status is Outcome.CONFIRMED
        assert enemy.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
        assert game.wait_for_decision().cell.key == destination
        sys.exit(0)
    assert step == Move(Direction.UP) and reason == "retreat_upstairs", (step, reason)
    result = turn.submit(step)
    assert result.status is Outcome.CONFIRMED
    player.admitted(turn, step, result, reason=reason)
    assert enemy.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
    refuge = game.wait_for_decision()
    player.observe(refuge)
    assert player.depth == 5 and refuge.cell.stairs_down
    if sys.argv[3] == "carrying":
        assert player.carrying_objective(refuge)
        sys.exit(0)
    step, reason = player.choose(refuge)
    assert step is not None and step != Move(Direction.DOWN), (step, reason)
    if turn.in_contact(turn.hostiles[0]):
        # Revisit the stair with the same opponent and a previously observed light hit.
        player.opponents.damage_by_target[turn.hostiles[0].target] = 1
        descent = Move(Direction.DOWN)
        receipt = refuge.submit(descent)
        assert receipt.status is Outcome.CONFIRMED
        player.admitted(refuge, descent, receipt, reason="descend")
        landing = game.wait_for_decision()
        player.observe(landing)
        assert player.depth == 6 and landing.cell.key == turn.cell.key
        strike, reason = player.choose(landing)
        assert reason == "melee" and strike.target.target == turn.hostiles[0].target, (strike, reason)
        receipt = landing.submit(strike)
        assert receipt.status is Outcome.CONFIRMED
        assert game.wait_for_decision().outcome_of(receipt).value == "resolved"
        sys.exit(0)
    receipt = refuge.submit(step)
    assert receipt.status is Outcome.CONFIRMED
    assert game.wait_for_decision().outcome_of(receipt).value == "resolved"
"""
        expected = "escape" if expect_escape else "explore"
        result = subprocess.run([sys.executable, "-c", code, self.address, expected,
                                 "carrying" if carrying else "outbound"], cwd=self.directory,
                                env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN},
                                text=True, capture_output=True, timeout=45)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_food_forecast_keeps_first_descent_time_after_a_stair_round_trip(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        contents["regions"][0]["actors"] = []
        contents["manifest"]["stair_pairs"] = {"exit": {"upper": "1/start", "lower": "2/up"}}
        contents["regions"].append({"id": 2, "name": "Landing", "size": [5, 5, 2],
                                    "chamber": True, "anchors": {"up": [2, 2, 0]}})
        self.server(scenario=write_package(self, contents, "revisited-stair"))
        self.install_bot_extension()
        code = """
import os, sys
from rogue_player import Adventurer
from tor_client_bot import ActivityPhase, Direction, Move, Outcome, Session, Wait
player = Adventurer(26, "item.amulet.amulet-of-yendor")
with Session.connect(sys.argv[1], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.wait_for_decision()
    start = turn.tick
    first = None
    player.observe(turn)
    for action in (Wait(), Wait(), Move(Direction.DOWN), Move(Direction.UP), Move(Direction.DOWN)):
        receipt = turn.submit(action)
        assert receipt.status is Outcome.CONFIRMED
        player.admitted(turn, action, receipt)
        turn = game.wait_for_decision()
        assert turn.outcome_of(receipt) is ActivityPhase.RESOLVED
        player.observe(turn)
        if isinstance(action, Move) and action.direction is Direction.DOWN and first is None:
            first = turn.tick - start
            assert player.travel_times.descent[1] == first
    assert player.depth == 2
    assert player.travel_times.descent[1] == first, (player.travel_times.descent, first)
"""
        result = subprocess.run([sys.executable, "-c", code, self.address], cwd=self.directory,
                                env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN},
                                capture_output=True, text=True, timeout=45)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_bot_uses_a_safe_diagonal_to_a_disclosed_stair(self):
        self.check_safe_stair_route("diagonal")

    def test_bot_opens_a_closed_door_on_its_remembered_stair_route(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        manifest["characters"][0]["loadout"] = []
        manifest["stair_pairs"] = {"escape": {"upper": "2/down", "lower": "1/start"}}
        region, = contents["regions"]
        region.update(anchors={"start": [4, 2, 0]}, doors=[{"id": 1, "at": [3, 2, 0], "height": 2, "open": False}],
                      actors=[{"archetype": "rogue.griffin", "id": 3, "at": [1, 2, 0],
                               "turn_ticks": 1000000, "controller": "external"}])
        contents["regions"].append({"id": 2, "name": "Upper", "size": [5, 5, 2], "chamber": True,
                                    "anchors": {"down": [2, 2, 0]}})
        self.server(scenario=write_package(self, contents, "stair-behind-door"))
        self.install_bot_extension()
        code = """
import os, sys
from rogue_player import Adventurer
from tor_client_bot import Direction, Move, OpenDoor, Outcome, Session, Wait
token = os.environ["TOR_SERVER_TOKEN"]
with Session.connect(sys.argv[1], token, 1) as game, Session.connect(sys.argv[1], token, 3) as guard:
    player = Adventurer(26, "item.amulet.amulet-of-yendor", depth=2)
    turn = game.wait_for_decision()
    assert turn.cell.stairs_up
    player.observe(turn)
    # Map a route around the closed door to the other side without opening it.
    for index, direction in enumerate([Direction.NORTH, Direction.WEST, Direction.WEST, Direction.SOUTH]):
        result = turn.submit(Move(direction))
        assert result.status is Outcome.CONFIRMED
        if index == 0:
            assert guard.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
        turn = game.wait_for_decision()
        assert turn.outcome_of(result).value == "resolved"
        player.observe(turn)
    door = turn.cell_at((1, 0, 0)).door
    assert not door.open and turn.hostiles

    # A previously visited doorway can close again. Recovery can use the mapped detour.
    from rogue_retreat import recovery_stair_step
    memory = player.maps[2]
    memory.visited.add(turn.cell_at(Direction.EAST.offset).key)
    step = recovery_stair_step(turn, memory, memory.stairs(Direction.UP), set())
    assert isinstance(step, Move) and turn.cell_at(step.direction.offset).key in memory.visited, step

    def take_retreat_action(turn, expected):
        action, reason = player.choose(turn)
        assert (action, reason) == (expected, "retreat_to_stairs"), (action, reason)
        result = turn.submit(action)
        assert result.status is Outcome.CONFIRMED
        player.admitted(turn, action, result, reason=reason)
        after = game.wait_for_decision()
        assert after.outcome_of(result).value == "resolved"
        player.observe(after)
        return after

    opened = take_retreat_action(turn, OpenDoor(door))
    assert opened.cell.key == turn.cell.key
    doorway = opened.cell_at(Direction.EAST.offset)
    assert doorway.door.open
    entered = take_retreat_action(opened, Move(Direction.EAST))
    assert entered.cell.key == doorway.key
"""
        result = subprocess.run([sys.executable, "-c", code, self.address], cwd=self.directory,
                                env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN},
                                text=True, capture_output=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_stair_approach_breaks_contact_with_a_dangerous_attacker(self):
        self.check_safe_stair_route("guarded")

    def test_stair_retreat_leaves_the_lethal_guard_when_a_weaker_foe_remains(self):
        self.check_safe_stair_route("two_guards")

    def check_safe_stair_route(self, scenario):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        manifest["characters"][0]["loadout"] = []
        manifest["stair_pairs"] = {"escape": {"upper": "2/down", "lower": "1/up"}}
        lower, = contents["regions"]
        if scenario != "open":
            lower["anchors"]["start"] = [2, 2, 0]
        lower["anchors"]["up"] = [1, 1, 0] if scenario == "diagonal" else [2, 1, 0]
        lower["actors"] = [{"archetype": "rogue.griffin", "id": 3, "controller": "external",
                            "at": [4, 2, 0], "turn_ticks": 1000000}]
        if scenario in {"guarded", "two_guards"}:
            lower["anchors"]["up"] = [3, 2, 0]
            lower["actors"][0]["at"] = [3, 3, 0]
        if scenario == "two_guards":
            lower["walls"] = [[x, 1, z] for x in (1, 2, 3) for z in range(2)]
            lower["actors"].append({"archetype": "rogue.bat", "id": 4, "at": [2, 3, 0],
                                    "turn_ticks": 1000000, "controller": "external"})
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
        "user_messages", "outcome", "intentions", "finished_intentions", "combat_updates",
        "travel",
    }
    assert snapshot["user_messages"] == [], snapshot["user_messages"]
    snapshot["state"]["observation"]["visible_cells"].clear()
    assert obs.data()["state"]["observation"]["visible_cells"]
finally:
    diagnostics.close()
session, enemy = (Session.connect(address, token, actor) for actor in (1, 3))
scenario = os.environ["BOT_TEST_STAIR_SCENARIO"]
bat = Session.connect(address, token, 4) if scenario == "two_guards" else None
try:
    turn = session.observe()
    policy = Adventurer(26, "item.amulet.amulet-of-yendor", depth=2)
    policy.observe(turn)
    exposed = scenario in {"guarded", "two_guards"}
    if exposed:
        griffin = next(foe for foe in turn.hostiles if foe.name == "griffin")
        policy.opponents.damage_by_name[griffin.name] = turn.health.current
    if scenario == "two_guards":
        policy.opponents.damage_by_name["bat"] = 1
    action, reason = policy.choose(turn)
    if exposed:
        assert isinstance(action, Move) and reason == "retreat", (action, reason)
        assert not turn.in_contact(griffin, from_offset=action.direction.offset)
        if scenario == "two_guards":
            assert any(turn.in_contact(foe, from_offset=action.direction.offset) for foe in turn.hostiles)
    else:
        direction = Direction.NORTH_WEST if scenario == "diagonal" else Direction.NORTH
        assert (action, reason) == (Move(direction), "retreat_to_stairs"), (action, reason)
    offset = action.direction.offset
    goal = next(cell.key for cell in turn.visible_cells if cell.offset == offset)
    assert turn.submit(action).status is Outcome.CONFIRMED
    assert enemy.observe().submit(Wait()).status is Outcome.CONFIRMED
    if bat:
        assert bat.observe().submit(Wait()).status is Outcome.CONFIRMED
    for _ in range(150):
        frame = session.observe()
        if frame.can_act and frame.cell and frame.cell.key == goal:
            break
        time.sleep(0.02)
    else:
        raise AssertionError("The safe stair step did not settle")
finally:
    if bat:
        bat.close()
    enemy.close()
    session.close()
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address, "BOT_TEST_STAIR_SCENARIO": scenario},
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

    def assert_admitted_or_stale(result):
        # Another actor can advance the shared input boundary after observe().
        # Local detection and an authoritative refusal are equally retryable;
        # all other refusals and unknown outcomes remain failures.
        assert result.status in {Outcome.CONFIRMED, Outcome.STALE} or (
            result.status is Outcome.REJECTED and result.code == "stale_context"
        ), result

    for _ in range(30):
        turn = player.observe()
        policy.observe(turn, journal.read(turn))
        if turn.restorable_losses.strength > 0:
            attacker, = turn.hostiles
            assert attacker.target in policy.opponents.draining_targets
            assert "rattlesnake" in policy.opponents.draining_names, policy.opponents.draining_names
            break
        enemy = snake.observe()
        if enemy.can_act:
            victim = next(actor for actor in enemy.creatures if actor.name == "Warrior")
            bite = next(ability for ability in enemy.abilities if ability.identity == "rogue.venomous_bite")
            result = enemy.submit(UseAbility(bite, victim))
            assert_admitted_or_stale(result)
        if turn.can_act:
            result = turn.submit(Wait())
            assert_admitted_or_stale(result)
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
                "format": 38, "id": "occupied-paired-stair", "version": "1.0",
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
        add_injury_potion(manifest, damage=6)
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

    def test_exploration_takes_another_passage_after_a_recent_threat(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        contents["manifest"]["characters"][0]["loadout"] = []
        contents["regions"][0].update(actors=[], anchors={"start": [3, 2, 0]})
        self.server(scenario=write_package(self, contents, "two-exploration-routes"))
        self.install_bot_extension()
        code = """
import os, sys
from rogue_player import Adventurer
from tor_client_bot import Move, Outcome, Session

with Session.connect(sys.argv[1], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.wait_for_decision()
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    player.observe(turn)
    ordinary, reason = player.choose(turn)
    assert isinstance(ordinary, Move) and reason == "explore", (ordinary, reason)
    dangerous = turn.cell_at(ordinary.direction.offset).key
    player.danger_until[dangerous] = turn.tick + 1200
    alternative, reason = player.choose(turn)
    assert isinstance(alternative, Move) and reason == "explore", (alternative, reason)
    destination = turn.cell_at(alternative.direction.offset).key
    assert destination != dangerous
    assert turn.submit(alternative).status is Outcome.CONFIRMED
    assert game.wait_for_decision().cell.key == destination
"""
        run = subprocess.run([sys.executable, "-c", code, self.address], cwd=self.directory,
                             env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN},
                             capture_output=True, text=True, timeout=45)
        self.assertEqual(run.returncode, 0, run.stderr)

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
    player.search_history.admissions.update({key: 3 for key in memory.cells if key not in {east, west}})
    player.search_history.completed_end_searches.update({key: 6 for key in memory.cells if memory.is_dead_end(key)})
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
    assert all(player.search_history.wall_searches[key] == 2 for key in walls(east))
    across_room = {cell.key for cell in after.visible_cells
                   if cell.wall and cell.offset[2] == 0 and 2 <= max(map(abs, cell.offset)) <= 3}
    assert across_room and all(player.search_history.wall_searches[key] == 2 for key in across_room)
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bot_reaches_the_objective_via_an_unguarded_approach(self):
        self.check_objective_approach(trapped_detours=False)

    def test_bot_crosses_a_guard_warning_to_keep_the_objective_route_trap_free(self):
        self.check_objective_approach(trapped_detours=True)

    def check_objective_approach(self, *, trapped_detours):
        from utility_potions_package import fixture

        contents = fixture()
        identity = "item.amulet.amulet-of-yendor"
        character, = contents["manifest"]["characters"]
        character.update(loadout=[], known_identities=[identity])
        region, = contents["regions"]
        region.update(actors=[], items=[{"id": 10, "archetype": "rogue.amulet_of_yendor", "at": [5, 2, 0]}])
        if trapped_detours:
            contents["manifest"]["stair_pairs"] = {"exit": {"upper": "2/down", "lower": "1/start"}}
            contents["regions"].append({"id": 2, "name": "Upper", "size": [3, 3, 2], "chamber": True,
                                        "anchors": {"down": [1, 1, 0]}})
            region["traps"] = [
                {"id": y + 1, "at": [2, y, 0], "dc": 0, "avoidance_dc": 100,
                 "kind": {"kind": "bear_trap_escape", "condition": "rogue.bear_trap", "escape_dc": 0}}
                for y in (0, 1, 3, 4)
            ]
        self.check_policy_scenario(contents, "objective-detour", f"""
import os
from rogue_player import Adventurer, RECENT_DANGER_TICKS
from tor_client_bot import Direction, Move, Outcome, Pickup, Session

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    player = Adventurer(26, "item.amulet.amulet-of-yendor", depth=2)
    turn = game.wait_for_decision()
    player.observe(turn)
    direct, reason = player.choose(turn)
    assert isinstance(direct, Move) and reason == "seek_objective", (direct, reason)
    guarded = turn.cell_at(direct.direction.offset).key
    player.supplies_guarded_until[guarded] = turn.tick + RECENT_DANGER_TICKS
    traps = player.maps[player.depth].known_traps
    assert len(traps) == {4 if trapped_detours else 0}, traps
    visited = set()
    for _ in range(8):
        action, reason = player.choose(turn)
        if isinstance(action, Move):
            assert reason == "seek_objective", reason
            destination = turn.cell_at(action.direction.offset).key
            assert destination not in traps, action
            visited.add(destination)
        else:
            assert isinstance(action, Pickup) and action.item.identity == player.objective, action
        result = turn.submit(action)
        assert result.status is Outcome.CONFIRMED, result
        player.admitted(turn, action, result, reason=reason)
        turn = game.wait_for_decision()
        player.observe(turn)
        if player.carrying_objective(turn):
            break
    assert player.carrying_objective(turn)
    assert (guarded in visited) == {trapped_detours}, visited
    if traps:
        player.danger_until[guarded] = turn.tick + RECENT_DANGER_TICKS
        for _ in range(8):
            action, reason = player.choose(turn)
            if reason == "ascend":
                assert turn.cell.stairs_up and action == Move(Direction.UP), action
            else:
                assert isinstance(action, Move) and reason == "seek_stairs", (action, reason)
                assert turn.cell_at(action.direction.offset).key not in traps, action
            result = turn.submit(action)
            assert result.status is Outcome.CONFIRMED, result
            player.admitted(turn, action, result, reason=reason)
            turn = game.wait_for_decision()
            player.observe(turn)
            if reason == "ascend":
                assert turn.cell.stairs_down and player.depth == 1
                assert player.pending_stair is None and player.carrying_objective(turn)
                break
        else:
            raise AssertionError("Carrier did not ascend with the objective")
""")

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
            {"id": 10, "at": [4, 2, 0], "archetype": "fixture.lookalike", "quantity": 1},
            {"id": 11, "at": [4, 2, 0], "archetype": "fixture.target", "quantity": 1},
            {"id": 12, "at": [4, 2, 0], "archetype": "fixture.target", "quantity": 1},
        ])
        server = self.server(scenario=write_package(self, contents, "rogue-bot-canonical-objective"))
        self.install_bot_extension()
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect", self.address,
             "--objective-identity", identity, "--retrieval-depth", "1", "--budget", "5"],
            cwd=self.directory, env={**os.environ, "PYTHONPATH": str(self.directory),
                                     "TOR_SERVER_TOKEN": TOKEN},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        turns = [json.loads(line) for line in result.stdout.splitlines()]
        self.assertEqual(turns[0]["reason"], "seek_objective")
        objective_pickups = []
        spare_offered = False
        for turn in turns[:-1]:
            offers = {item["item"] for item in turn["reachable_pickups"] if item["identity"] == identity}
            if turn["amulet_acquired"] and offers:
                spare_offered = True
            action = turn["action"]
            if action["type"] == "take" and action["item"] in offers:
                objective_pickups.append(turn)
        self.assertTrue(spare_offered, "The spare must be reachable after the first amulet is acquired")
        self.assertEqual(len(objective_pickups), 1)
        self.assertEqual(objective_pickups[0]["result"]["status"], "confirmed")
        self.assertEqual(turns[0]["result"]["status"], "confirmed")
        self.assertTrue(turns[-1]["amulet_acquired"], turns)

    def nutrition_run(self, budget, **scenario):
        from nutrition_package import prepare

        server = self.server(scenario=prepare(self, **scenario))
        self.install_bot_extension()
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect", self.address,
             "--budget", str(budget)],
            cwd=self.directory, env={**os.environ, "PYTHONPATH": str(self.directory), "TOR_SERVER_TOKEN": TOKEN},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        return [json.loads(line) for line in result.stdout.splitlines()]

    def test_starvation_report_includes_the_servers_final_reserve_and_threshold(self):
        from rogue_o_matic_cohort import death_cause

        *actions, report = self.nutrition_run(5, initial=1, unit_ticks=1, consume_turns=2, terminal_on_death=True)
        self.assertEqual(actions[0]["reason"], "eat")
        self.assertEqual(report["terminal"], "death")
        self.assertEqual(report["last_nutrition"], {"kind": "required", "reserve": -3, "starve_at": -3})
        self.assertEqual(death_cause(report), "starvation")
        journal_only = {key: report[key] for key in ("self_target", "events_since_previous_action")}
        self.assertEqual(death_cause(journal_only), "starvation")

    def test_bot_eats_server_offered_food_without_a_ration_tag(self):
        first, second, terminal = self.nutrition_run(2, initial=2, unit_ticks=1000)
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
        meal, = pickup["reachable_pickups"]
        self.assertEqual(meal["item"], pickup["action"]["item"])
        self.assertEqual(meal["category"], "food")
        self.assertEqual(eat["reachable_pickups"], [])
        self.assertEqual(pickup["result"]["status"], "confirmed")
        self.assertEqual(eat["action"]["type"], "eat")
        self.assertEqual(eat["result"]["status"], "confirmed")
        self.assertEqual(terminal["terminal"], "action_budget")

    def start_identification_server(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        lore = manifest["archetypes"]["rogue.identify_potion"]
        lore["name"] = "Lore of draughts"
        lore["identity"] = "item.scroll.fixture-lore"
        character, = manifest["characters"]
        character["known_identities"] = [lore["identity"]]
        character["loadout"] = [
            {"archetype": "rogue.identify_potion", "quantity": 1},
            {"archetype": "rogue.blindness", "quantity": 1},
            {"archetype": "rogue.extra_healing", "quantity": 3},
        ]
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
    assert scroll.identifies == ("potion",), scroll
    assert all(not item.identifies for item in turn.inventory if item.identity is None)
    assert turn.submit(UseItem(scroll, ItemUse.READ)).status is Outcome.CONFIRMED
    first = game.wait_for_decision().identification
    current = game.observe()
    assert current.identification.choice == first.choice
    assert current.submit(first.select(None)).status is Outcome.STALE
    assert current.submit(first.select(first.items[0])).status is Outcome.STALE
    answer = current.identification.select(current.identification.items[0])
    assert current.submit(answer).status is Outcome.CONFIRMED
    following = game.wait_for_decision()
    assert following.can_act and following.identification is None
    assert following.submit(answer).status is Outcome.STALE
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
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        read, identify, terminal = (json.loads(line) for line in result.stdout.splitlines())
        self.assertEqual((read["action"]["type"], read["reason"]), ("read", "identify_gear"))
        self.assertEqual(identify["action"]["type"], "identify")
        self.assertEqual(identify["result"]["status"], "confirmed")
        self.assertEqual(terminal["terminal"], "action_budget")

        probe = """
import json, os, sys
from tor_client_bot import Session

with Session.connect(sys.argv[1], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    potions = [item for item in game.observe().inventory if item.category == "potion"]
    print(json.dumps(sorted((item.quantity, item.identity) for item in potions)))
"""
        observed = subprocess.run(
            [sys.executable, "-c", probe, self.address], cwd=self.directory,
            env={**os.environ, "TOR_SERVER_TOKEN": TOKEN}, text=True, capture_output=True, timeout=30,
        )
        server.stop(save=False)
        self.assertEqual(observed.returncode, 0, observed.stderr)
        self.assertEqual(json.loads(observed.stdout), [[1, None], [3, "item.potion.extra-healing"]])

    def test_bot_walks_to_a_nearby_potion_and_collects_it(self):
        self.check_supply_detour("rogue.healing", [], "test_unknown_potion")

    def test_cli_takes_a_known_healing_potion_before_gold_on_the_same_square(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        character, = contents["manifest"]["characters"]
        character.update(loadout=[], known_identities=["item.potion.healing", "item.potion.extra-healing"])
        room, = contents["regions"]
        room.update(actors=[], items=[{"id": 9, "archetype": "rogue.gold", "at": [1, 2, 0]},
                                      {"id": 10, "archetype": "rogue.healing", "at": [1, 2, 0]},
                                      {"id": 11, "archetype": "rogue.extra_healing", "at": [1, 2, 0], "quantity": 2}])
        self.server(scenario=write_package(self, contents, "healing-before-gold"))
        self.install_bot_extension()
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect", self.address,
             "--budget", "1", "--record-decisions"], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(self.directory), "TOR_SERVER_TOKEN": TOKEN},
            text=True, capture_output=True, timeout=55,
        )
        self.assertEqual(result.returncode, 1, result.stderr)
        action, terminal = map(json.loads, result.stdout.splitlines())
        self.assertEqual((action["reason"], action["result"]["status"], terminal["terminal"]),
                         ("take", "confirmed", "action_budget"))
        inventory = json.loads(terminal["recording"])["state"]["observation"]["inventory"]
        self.assertEqual([(item["identity"], int(item["quantity"])) for item in inventory],
                         [("item.potion.extra-healing", 2)])

    def test_arrow_detours_stop_when_a_cursed_mace_prevents_drawing_the_bow(self):
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        manifest["archetypes"]["rogue.mace"]["equipment_generation"] = {
            "kind": "weapon", "curse_percent": 100, "enchant_percent": 0, "minimum": 1, "maximum": 1}
        character, = manifest["characters"]
        character.update(known_identities=["item.weapon.mace", "item.weapon.shortbow", "item.weapon.dagger",
                                           "item.scroll.identify-weapon"],
                         loadout=[{"archetype": "rogue.mace", "quantity": 1, "equipped_slot": 0},
                                  {"archetype": "rogue.shortbow", "quantity": 1},
                                  {"archetype": "rogue.identify_weapon", "quantity": 1}])
        region, = contents["regions"]
        region.update(actors=[], items=[{"id": 9, "archetype": "rogue.arrow", "at": [5, 2, 0]},
                                       {"id": 10, "archetype": "rogue.dagger", "at": [4, 2, 0]}])
        self.check_policy_scenario(contents, "blocked-bow", """
import os
from rogue_player import Adventurer
from tor_client_bot import ItemUse, Outcome, Session, UseItem

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.wait_for_decision()
    scroll = next(item for item in turn.inventory if item.identity == "item.scroll.identify-weapon")
    assert turn.submit(UseItem(scroll, ItemUse.READ)).status is Outcome.CONFIRMED
    turn = game.wait_for_decision()
    prompt = turn.identification
    mace = next(item for item in prompt.items if item.identity == "item.weapon.mace")
    assert turn.submit(prompt.select(mace)).status is Outcome.CONFIRMED
    turn = game.wait_for_decision()
    assert next(slot for slot in turn.equipment if slot.kind == "weapon").removal_blocked
    assert any(ground.item.identity == "item.weapon.arrow" for ground in turn.ground_items)
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    player.observe(turn)
    outbound = player.loot_goals(turn, turn.ground_items, returning=False)
    dagger = next(g for g in turn.ground_items if g.item.identity == "item.weapon.dagger")
    assert dagger.cell.key not in outbound.supplies
    assert not outbound.recover_arrows
    assert not player.loot_goals(turn, turn.ground_items, returning=True).wanted
""")

    def test_bot_collects_nearby_gold_before_distant_visible_arrows(self):
        self.check_gold_before_arrows(arrow_x=1, guarded=False)

    def test_bot_collects_nearby_gold_before_guarded_arrows_in_recovery_range(self):
        self.check_gold_before_arrows(arrow_x=8, guarded=True)

    def check_gold_before_arrows(self, *, arrow_x, guarded):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        character, = contents["manifest"]["characters"]
        character["loadout"] = [{"archetype": "rogue.shortbow", "quantity": 1}]
        room, = contents["regions"]
        room.update(size=[17, 5, 2], anchors={"start": [13, 2, 0]}, actors=[], items=[
            {"id": 9, "archetype": "rogue.arrow", "at": [arrow_x, 2, 0]},
            {"id": 10, "archetype": "rogue.gold", "at": [15, 2, 0]}])
        self.server(scenario=write_package(self, contents, "loot-in-opposite-directions"))
        self.install_bot_extension()
        code = """
import os, sys
from rogue_player import Adventurer
from tor_client_bot import Move, Outcome, Pickup, Session

with Session.connect(sys.argv[1], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    turn = game.wait_for_decision()
    assert {g.item.identity for g in turn.ground_items} >= {"item.coin.gold", "item.weapon.arrow"}
    if sys.argv[2] == "guarded":
        arrow = next(g for g in turn.ground_items if g.item.identity == "item.weapon.arrow")
        player.supplies_guarded_until[arrow.cell.key] = turn.tick + 1200
    for _ in range(3):
        player.observe(turn)
        action, reason = player.choose(turn)
        if isinstance(action, Move):
            assert reason == "seek_supplies" and action.direction.offset[0] == 1, (action, reason)
        else:
            assert isinstance(action, Pickup) and action.item.identity == "item.coin.gold", (action, reason)
        result = turn.submit(action)
        assert result.status is Outcome.CONFIRMED
        player.admitted(turn, action, result, reason=reason)
        turn = game.wait_for_decision()
        if isinstance(action, Pickup):
            break
    assert any(i.identity == "item.coin.gold" for i in turn.inventory)
"""
        result = subprocess.run([sys.executable, "-c", code, self.address, "guarded" if guarded else "clear"],
                                cwd=self.directory,
                                env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN},
                                capture_output=True, text=True, timeout=55)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_cli_collects_reachable_treasure_and_searches_a_dead_end_six_times(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        contents["manifest"]["characters"][0]["loadout"] = []
        region, = contents["regions"]
        region.update(anchors={"start": [3, 2, 0]}, actors=[], items=[
            {"id": 15, "archetype": "rogue.gold", "at": [3, 2, 0]}],
            walls=[[x, y, z] for x, y in ((3, 1), (3, 3), (4, 2)) for z in range(2)])
        self.server(scenario=write_package(self, contents, "treasure-at-corridor-end"))
        self.install_bot_extension()
        run = subprocess.run([sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect", self.address,
                              "--budget", "8"], cwd=self.directory,
                             env={**os.environ, "PYTHONPATH": str(self.directory), "TOR_SERVER_TOKEN": TOKEN},
                             capture_output=True, text=True, timeout=55)
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
        *actions, terminal = map(json.loads, run.stdout.splitlines())
        self.assertEqual([a["reason"] for a in actions], ["take", *(["search"] * 6), "explore"])
        self.assertEqual(actions[0]["reachable_pickups"][0]["category"], "coin")
        self.assertEqual({a["cell"] for a in actions[:7]}, {actions[0]["cell"]})
        self.assertEqual({a["action"].get("id") for a in actions[1:7]}, {"rogue.search"})
        self.assertTrue(all(a["previous_intention_phase"] == "resolved" for a in actions[1:]))
        self.assertEqual(terminal["terminal"], "action_budget")

    def test_cli_opens_a_concealed_corridor_door_after_the_search_that_reveals_it(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        contents["manifest"]["characters"][0]["loadout"] = []
        region, = contents["regions"]
        region.update(anchors={"start": [3, 2, 0]}, actors=[],
                      walls=[[3, y, z] for y in (1, 3) for z in range(2)],
                      doors=[{"id": 1, "at": [4, 2, 0], "height": 2, "open": False,
                              "concealment": {"dc": 10}}])
        self.server(scenario=write_package(self, contents, "discover-corridor-door"))
        self.install_bot_extension()
        run = subprocess.run([sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect", self.address,
                              "--budget", "7", "--record-decisions"], cwd=self.directory,
                             env={**os.environ, "PYTHONPATH": str(self.directory), "TOR_SERVER_TOKEN": TOKEN},
                             capture_output=True, text=True, timeout=55)
        self.assertEqual(run.returncode, 1, run.stderr)
        rows = list(map(json.loads, run.stdout.splitlines()))
        first = json.loads(rows[0]["recording"])["state"]["observation"]
        self.assertFalse(any(c.get("door") for c in first["visible_cells"]))
        entrance = next(c for c in first["visible_cells"] if c["position"] == {"x": 1, "y": 0, "z": 0})
        self.assertTrue(entrance["wall"])

        def doors(row):
            view = json.loads(row["recording"])["state"]["observation"]
            return [c["door"] for c in view["visible_cells"] if c["key"] == entrance["key"] and c.get("door")]

        reveal = next((i for i, row in enumerate(rows) if doors(row)), None)
        self.assertIn(reveal, range(1, 6), [row.get("reason") for row in rows])
        self.assertTrue(all(row["action"].get("id") == "rogue.search" for row in rows[:reveal]))
        opened = rows[reveal]
        self.assertEqual(opened["previous_intention_phase"], "resolved")
        self.assertEqual(opened["action"]["type"], "set_door", opened["reason"])
        self.assertEqual(rows[reveal + 1]["previous_intention_phase"], "resolved")
        self.assertTrue(any(d["id"] == opened["action"]["door"] and d["open"] for d in doors(rows[reveal + 1])))

    def test_bot_frees_a_cursed_finger_to_wear_a_better_ring(self):
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        manifest["archetypes"]["rogue.ring_adornment"]["equipment_generation"] = {"kind": "cursed"}
        character, = manifest["characters"]
        character.update(known_identities=["item.ring.ring-adornment", "item.ring.ring-slow-digestion",
                                           "item.scroll.identify-wand", "item.scroll.remove-curse"],
                         loadout=[{"archetype": "rogue.ring_adornment", "quantity": 1, "equipped_slot": index}
                                  for index in (7, 8)] + [
                             {"archetype": "rogue.ring_slow_digestion", "quantity": 1},
                             {"archetype": "rogue.identify_wand", "quantity": 2},
                             {"archetype": "rogue.remove_curse", "quantity": 1}])
        contents["regions"][0].update(actors=[], items=[])
        self.check_policy_scenario(contents, "cursed-fingers", """
import os
from rogue_player import Adventurer
from tor_client_bot import ItemUse, Outcome, Session, UseItem

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.wait_for_decision()
    fingers = [slot.occupant.target for slot in turn.equipment if slot.kind == "ring"]
    for target in fingers:
        scroll = next(item for item in turn.inventory if item.identity == "item.scroll.identify-wand")
        assert turn.submit(UseItem(scroll, ItemUse.READ)).status is Outcome.CONFIRMED
        turn = game.wait_for_decision()
        prompt = turn.identification
        assert turn.submit(prompt.select(next(item for item in prompt.items if item.target == target))).status is Outcome.CONFIRMED
        turn = game.wait_for_decision()
    assert all(slot.removal_blocked for slot in turn.equipment if slot.kind == "ring")
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    for expected in ("remove_ring_curse", "change_ring", "conserve_food"):
        player.observe(turn)
        action, reason = player.choose(turn)
        assert reason == expected, (action, reason)
        assert turn.submit(action).status is Outcome.CONFIRMED
        turn = game.wait_for_decision()
    assert any(slot.occupant and slot.occupant.identity == "item.ring.ring-slow-digestion" for slot in turn.equipment)
""")

    def test_bot_walks_to_a_recognized_slow_digestion_ring_and_wears_it(self):
        self.check_supply_detour("rogue.ring_slow_digestion", ["item.ring.ring-slow-digestion"], "conserve_food")

    def test_budget_report_counts_the_last_resolved_supply_step(self):
        from rogue_o_matic_cohort import trace_report

        rows = self.run_supply_detour("rogue.ring_slow_digestion", ["item.ring.ring-slow-digestion"], budget=1)
        move, final = rows
        self.assertEqual(final["terminal"], "action_budget")
        self.assertEqual(final["previous_intention_phase"], "resolved")
        path = self.directory / "turns.jsonl"
        path.write_text("\n".join(map(json.dumps, rows)), encoding="utf-8")
        _, progress = trace_report(path)
        self.assertEqual(progress["actions"], 1)
        self.assertEqual(progress["supply_detours"]["resolved_steps"], 1)
        self.assertGreater(final["tick"], move["tick"])
        self.assertEqual(progress["supply_detours"]["ticks"], final["tick"] - move["tick"])

        for row in rows:
            del row["known_cells"]
        path.write_text("\n".join(map(json.dumps, rows)), encoding="utf-8")
        _, partial = trace_report(path)
        self.assertEqual(partial["supply_detours"], progress["supply_detours"])

    def run_supply_detour(self, archetype, known, *, budget):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        character, = contents["manifest"]["characters"]
        character.update(loadout=[], known_identities=known)
        region, = contents["regions"]
        region.update(actors=[], items=[{"id": 9, "archetype": archetype, "at": [4, 2, 0]}])
        self.server(scenario=write_package(self, contents, "visible-supply-detour"))
        self.install_bot_extension()
        run = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect", self.address,
             "--budget", str(budget)], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(self.directory), "TOR_SERVER_TOKEN": TOKEN},
            text=True, capture_output=True, timeout=55,
        )
        rows = [json.loads(line) for line in run.stdout.splitlines()]
        self.assertEqual(run.returncode, 1, run.stderr)
        self.assertEqual(rows[0]["reason"], "seek_supplies")
        return rows

    def test_unarmed_bot_walks_to_a_visible_dagger_and_equips_it(self):
        self.check_supply_detour("rogue.dagger", ["item.weapon.dagger"], "equip_melee")

    def check_supply_detour(self, archetype, known, use):
        rows = self.run_supply_detour(archetype, known, budget=6)
        take = next(i for i, row in enumerate(rows) if row.get("reason") == "take")
        self.assertEqual(rows[take]["result"]["status"], "confirmed")
        self.assertEqual(rows[take + 1]["reason"], use)
        self.assertEqual(rows[take + 1]["previous_intention_phase"], "resolved")
        if use == "conserve_food":
            report = rows[-1]
            self.assertEqual(report["equipment_opportunities"]["ring"],
                             {"seen_on_ground": 1, "carried": 1, "recognized": 1, "worn": 1})
            self.assertEqual(report["last_inventory_tick"], report["tick"])
            self.assertEqual(report["last_carried_devices"], [])

    def start_paired_stair_fixture(self, *, loadout):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        character, = manifest["characters"]
        character["known_identities"] = []
        character["loadout"] = loadout
        manifest["stair_pairs"] = {"passage": {"upper": "1/start", "lower": "2/up"}}
        contents["regions"][0]["actors"] = []
        contents["regions"].append({"id": 2, "name": "Landing", "size": [5, 5, 2],
                                    "chamber": True, "anchors": {"up": [2, 2, 0]}})
        self.server(scenario=write_package(self, contents, "paired-stairs"))
        self.install_bot_extension()

    def test_cohort_records_landing_on_last_budgeted_descent(self):
        from rogue_o_matic_cohort import trace_report

        self.start_paired_stair_fixture(loadout=[])
        run = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect", self.address, "--budget", "1"],
            cwd=self.directory, env={**os.environ, "PYTHONPATH": str(self.directory), "TOR_SERVER_TOKEN": TOKEN},
            text=True, capture_output=True, timeout=55,
        )
        self.assertEqual(run.returncode, 1, run.stderr)
        trace = self.directory / "turns.jsonl"
        trace.write_text(run.stdout, encoding="utf-8")
        terminal, progress = trace_report(trace)
        self.assertEqual((terminal["terminal"], terminal["floor"]), ("action_budget", 2))
        self.assertEqual((progress["actions"], progress["max_depth"]), (1, 2))
        self.assertEqual([(s["from"], s["to"]) for s in progress["stair_transitions"]], [(1, 2)])
        self.assertEqual(progress["floors"]["2"]["moves_admitted"], 0)

    def test_levitating_player_gets_a_ground_contact_reason_at_the_stair(self):
        self.start_paired_stair_fixture(loadout=[{"archetype": "rogue.levitation", "quantity": 1},
                                                {"archetype": "rogue.mace", "quantity": 1}])
        code = """
import os, sys
from tor_client_bot import Direction, ItemUse, Move, Outcome, Session, UseItem

with Session.connect(sys.argv[1], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.wait_for_decision()
    potion = next(item for item in turn.inventory if ItemUse.DRINK in item.uses)
    assert turn.submit(UseItem(potion, ItemUse.DRINK)).status is Outcome.CONFIRMED
    turn = game.wait_for_decision()
    assert turn.has_condition("rogue.levitation") and turn.cell.stairs_down
    refused = turn.submit(Move(Direction.DOWN))
    assert refused.status is Outcome.REJECTED, refused
    assert "levitating" in refused.reason and "ground" in refused.reason, refused
    current = game.observe()
    assert current.can_act and current.tick == turn.tick and current.cell.key == turn.cell.key
"""
        run = subprocess.run([sys.executable, "-c", code, self.address], cwd=self.directory,
                             env={**os.environ, "TOR_SERVER_TOKEN": TOKEN},
                             capture_output=True, text=True, timeout=40)
        self.assertEqual(run.returncode, 0, run.stderr)

    def test_bot_descends_after_its_sampled_potion_stops_levitation(self):
        self.start_paired_stair_fixture(loadout=[{"archetype": "rogue.levitation", "quantity": 1},
                                                {"archetype": "rogue.mace", "quantity": 1}])
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect", self.address,
             "--budget", "100"], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(self.directory), "TOR_SERVER_TOKEN": TOKEN},
            text=True, capture_output=True, timeout=65,
        )
        rows = [json.loads(line) for line in result.stdout.splitlines()]
        self.assertTrue(rows, result.stderr)
        self.assertEqual([row["reason"] for row in rows[:2]], ["equip_melee", "test_unknown_potion"])
        self.assertTrue(all(row["result"]["status"] == "confirmed" for row in rows[:2]))
        first_descent = next(i for i, row in enumerate(rows) if row.get("reason") == "descend")
        self.assertGreater(first_descent, 2)
        self.assertTrue(all(row["reason"] == "wait_for_ground_contact" for row in rows[2:first_descent]))
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

    def test_locked_armor_suppresses_a_detour_for_plate(self):
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        manifest["archetypes"]["rogue.ring_mail"]["equipment_generation"] = {
            "kind": "armor", "curse_percent": 100, "enchant_percent": 0, "minimum": 1, "maximum": 1}
        character, = manifest["characters"]
        character.update(known_identities=["item.armor.ring-mail", "item.armor.plate-mail",
                                           "item.scroll.identify-armor"],
                         loadout=[{"archetype": "rogue.ring_mail", "quantity": 1, "equipped_slot": 2},
                                  {"archetype": "rogue.identify_armor", "quantity": 1}])
        region, = contents["regions"]
        region.update(actors=[], items=[{"id": 10, "archetype": "rogue.plate_mail", "at": [5, 2, 0]}])
        self.check_policy_scenario(contents, "locked-armor", """
import os
from rogue_player import Adventurer
from tor_client_bot import ItemUse, Outcome, Session, UseItem

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.wait_for_decision()
    scroll = next(item for item in turn.inventory if item.identity == "item.scroll.identify-armor")
    assert turn.submit(UseItem(scroll, ItemUse.READ)).status is Outcome.CONFIRMED
    turn = game.wait_for_decision()
    prompt = turn.identification
    mail = next(item for item in prompt.items if item.identity == "item.armor.ring-mail")
    assert turn.submit(prompt.select(mail)).status is Outcome.CONFIRMED
    turn = game.wait_for_decision()
    assert next(slot for slot in turn.equipment if slot.kind == "body_armor").removal_blocked
    plate = next(g for g in turn.ground_items if g.item.identity == "item.armor.plate-mail")
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    player.observe(turn)
    assert plate.cell.key not in player.loot_goals(turn, turn.ground_items, returning=False).supplies
""")

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
        self.check_ground_gear("armor", "rogue.ring_mail", "rogue.plate_mail", 42, found_cursed=True)

    def test_player_identifies_ground_long_sword_and_upgrades_primary_weapon(self):
        self.check_ground_gear("weapon", "rogue.mace", "rogue.long_sword", 43)

    def test_player_enchants_a_disclosed_custom_melee_weapon(self):
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        identity = "item.weapon.fixture-blade"
        manifest["archetypes"]["rogue.long_sword"]["identity"] = identity
        character, = manifest["characters"]
        character.update(known_identities=[identity, "item.weapon.dagger",
                                           "item.scroll.identify-weapon", "item.scroll.enchant-weapon"],
                         loadout=[{"archetype": "rogue.dagger", "quantity": 1},
                                  {"archetype": "rogue.long_sword", "quantity": 1, "equipped_slot": 0},
                                  {"archetype": "rogue.identify_weapon", "quantity": 1},
                                  {"archetype": "rogue.enchant_weapon", "quantity": 1}])
        contents["regions"][0].update(actors=[], items=[])
        self.check_policy_scenario(contents, "custom-enchantment", """
import os
from rogue_player import Adventurer
from tor_client_bot import Outcome, Session

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    turn = game.wait_for_decision()
    for expected in ("identify_gear", "identify"):
        player.observe(turn)
        action, reason = player.choose(turn)
        assert reason == expected, (action, reason)
        if expected == "identify":
            offered = [item.identity for item in turn.identification.items]
            assert offered == ["item.weapon.dagger", "item.weapon.fixture-blade"], offered
            assert action.item.identity == "item.weapon.fixture-blade", action
        assert turn.submit(action).status is Outcome.CONFIRMED
        turn = game.wait_for_decision()
    def attack(turn):
        return next(slot.occupant.weapon_attack for slot in turn.equipment if slot.kind == "weapon")
    before = attack(turn)
    assert before is not None
    player.observe(turn)
    action, reason = player.choose(turn)
    assert reason == "enchant_weapon", (action, reason)
    assert turn.submit(action).status is Outcome.CONFIRMED
    assert attack(game.wait_for_decision()) != before
""")

    def test_player_enchants_the_ground_weapon_upgrade_after_equipping_it(self):
        self.check_ground_gear("weapon", "rogue.mace", "rogue.long_sword", 43, enchant_weapon=True)

    def test_player_lifts_a_weapon_curse_then_equips_the_ground_upgrade(self):
        self.check_ground_gear("weapon", "rogue.shortbow", "rogue.long_sword", 43, starting_cursed=True)

    def test_player_equips_unmeasured_plate_without_an_identification_scroll(self):
        self.check_ground_gear("armor", "rogue.ring_mail", "rogue.plate_mail", 43, identify=False)

    def test_player_protects_an_identified_upgrade_against_rust(self):
        self.check_ground_gear("armor", "rogue.ring_mail", "rogue.plate_mail", 43, protect=True)

    def check_ground_gear(self, category, starting, found, seed, *, found_cursed=False, identify=True, protect=False,
                          starting_cursed=False, enchant_weapon=False):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        contents["seed"] = seed
        archetypes = contents["manifest"]["archetypes"]
        if not found_cursed:
            archetypes[found]["equipment_generation"] = {"kind": "authored"}
        found_id = archetypes[found]["identity"]
        expected = archetypes[starting]["identity"] if found_cursed else found_id
        character, = contents["manifest"]["characters"]
        character["known_identities"] = [archetypes[starting]["identity"], found_id, f"item.scroll.identify-{category}"]
        character["loadout"] = [
            {"archetype": starting, "quantity": 1, "equipped_slot": 2 if category == "armor" else 0},
        ]
        if enchant_weapon:
            character["known_identities"].append("item.scroll.enchant-weapon")
            character["loadout"].append({"archetype": "rogue.enchant_weapon", "quantity": 1})
        if starting_cursed:
            archetypes[starting]["equipment_generation"] = {
                "kind": "weapon", "curse_percent": 100, "enchant_percent": 0, "minimum": 1, "maximum": 1}
            character["known_identities"].append("item.scroll.remove-curse")
            character["loadout"].append({"archetype": "rogue.remove_curse", "quantity": 1})
        if identify:
            character["loadout"].append({"archetype": f"rogue.identify_{category}", "quantity": 2})
        if protect:
            character["known_identities"].append("item.scroll.protect-armor")
            character["loadout"].append({"archetype": "rogue.protect_armor", "quantity": 1})
        # The cursed bow case must justify a detour before the replacement is within reach.
        item_site = [4, 2, 0] if starting_cursed else [1, 2, 0]
        contents["regions"][0].update(actors=[], items=[
            {"id": 10, "at": item_site, "archetype": found, "quantity": 1}
        ])
        self.server(scenario=write_package(self, contents, "player-armor"))
        self.install_bot_extension()
        env = {**os.environ, "PYTHONPATH": str(self.directory), "TOR_SERVER_TOKEN": TOKEN}
        run = subprocess.run([sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect",
                              self.address, "--budget", "15", *(["--record-decisions"] if starting_cursed else [])], cwd=self.directory, env=env,
                             text=True, capture_output=True, timeout=55)
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
        rows = [json.loads(line) for line in run.stdout.splitlines()]
        actions = [row["action"]["type"] for row in rows if "action" in row]
        self.assertEqual("identify" in actions, identify)
        reasons = [row.get("reason") for row in rows]
        if enchant_weapon:
            self.assertLess(reasons.index("equip_melee"), reasons.index("enchant_weapon"))
        if starting_cursed:
            approach = rows[reasons.index("seek_supplies")]
            offers = json.loads(approach["recording"])["state"]["observation"]["interactions"]
            weapon = next(item for item in offers["inventory"] if item.get("equipped_slot") == 0)
            self.assertTrue(weapon.get("known_cursed") or weapon["item"] in offers.get("curse_blocked", ()))
            self.assertLess(reasons.index("seek_supplies"), reasons.index("take"))
            self.assertLess(reasons.index("take"), reasons.index("remove_weapon_curse"))
            self.assertLess(reasons.index("remove_weapon_curse"), reasons.index("equip_melee"))
        if not identify:
            self.assertIn("try_armor", reasons)
        if protect:
            self.assertIn("protect_armor", reasons)
        equipment = rows[-1]["equipment_opportunities"][category]
        self.assertEqual(equipment["seen_on_ground"], 1)
        self.assertEqual(equipment["carried"], 2)
        self.assertEqual(equipment["worn"], 1 if found_cursed else 2)
        kind = "body_armor" if category == "armor" else "weapon"
        observed = self.observe_ground_gear(found_id, kind, env)
        self.assertEqual(observed["worn_identity"], expected)
        self.assertIs(observed["known_cursed"], found_cursed if identify else None)
        if not identify:
            self.assertEqual(observed["physical_reduction"], 6)
        if protect:
            self.assertIs(observed["known_protected"], True)
        self.assertEqual("unequip" in actions, not found_cursed)
        self.assertEqual("equip" in actions, not found_cursed)

    def observe_ground_gear(self, found_id, slot_kind, env):
        probe = """
import json, os, sys
from tor_client_bot import Session

address, found_id, slot_kind = sys.argv[1:]
with Session.connect(address, os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.observe()
    worn = next(slot.occupant for slot in turn.equipment if slot.kind == slot_kind)
    found = next(item for item in turn.inventory if item.identity == found_id)
    print(json.dumps({
        "worn_identity": worn.identity if worn else None,
        "known_cursed": found.known_cursed,
        "physical_reduction": found.physical_reduction,
        "known_protected": found.known_protected,
    }))
"""
        result = subprocess.run(
            [sys.executable, "-c", probe, self.address, found_id, slot_kind],
            cwd=self.directory, env=env, text=True, capture_output=True, timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

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
        if identify:
            inspect_scope = """
import os, sys
from tor_client_bot import ItemUse, Session
with Session.connect(sys.argv[1], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.wait_for_decision()
    scroll = next(item for item in turn.inventory if ItemUse.READ in item.uses)
    assert scroll.identifies == ("ring", "wand"), scroll
    assert not any(item.category == "wand" for item in turn.inventory)
"""
            scope = subprocess.run([sys.executable, "-c", inspect_scope, self.address], cwd=self.directory,
                                   env={**os.environ, "TOR_SERVER_TOKEN": TOKEN},
                                   capture_output=True, text=True, timeout=30)
            self.assertEqual(scope.returncode, 0, scope.stderr)
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/rogue_o_matic.py"), "--connect", self.address,
             "--budget", str(8 if identify else 2 + int(occupied))],
            cwd=self.directory, env={**os.environ, "PYTHONPATH": str(self.directory),
                                     "TOR_SERVER_TOKEN": TOKEN},
            text=True, capture_output=True, timeout=55,
        )
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        *rows, terminal = (json.loads(line) for line in result.stdout.splitlines())
        if identify:
            # The prompt can still be visible while an accepted answer resolves.
            confirmed = [r for r in rows if r["result"]["status"] == "confirmed"]
            actions = [r["action"]["type"] for r in confirmed]
            self.assertLess(actions.index("read"), actions.index("identify"), actions)
            self.assertLess(actions.index("identify"), actions.index("equip"), actions)
            pickup, equip = confirmed[0], confirmed[actions.index("equip")]
            check = """
import os, sys
from tor_client_bot import Session
with Session.connect(sys.argv[1], os.environ["TOR_SERVER_TOKEN"], 1) as session:
    assert any(slot.occupant and slot.occupant.identity == "item.ring.ring-searching"
               for slot in session.observe().equipment)
"""
            check_result = subprocess.run([sys.executable, "-c", check, self.address], cwd=self.directory,
                                          env={**os.environ, "TOR_SERVER_TOKEN": TOKEN},
                                          text=True, capture_output=True, timeout=30)
            self.assertEqual(check_result.returncode, 0, check_result.stderr)
        else:
            pickup, *swaps, equip = rows
        server.stop(save=False)
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

    def test_player_illuminates_a_dark_chamber_before_exploring(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        manifest["characters"][0].update(known_identities=["item.wand.wand-light"],
                                         loadout=[{"archetype": "rogue.wand_light", "quantity": 1}])
        contents["regions"][0].update(actors=[], lit=False)
        manifest["spatial_scopes"] = {"study": {"members": [1], "rooms": {"1": [
            {"region": 1, "min": [0, 0, 0], "max": [6, 4, 1]}]}, "passages": [], "doorways": []}}
        self.server(scenario=write_package(self, contents, "bot-light-room"))
        self.install_bot_extension()
        code = """
import os, sys
from rogue_player import Adventurer
from rogue_runner import play
from tor_client_bot import Session, Outcome, Zap
with Session.connect(sys.argv[1], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.wait_for_decision()
    bot = Adventurer(26, "item.amulet.amulet-of-yendor")
    bot.observe(turn)
    action, reason = bot.choose(turn)
    assert reason == "light_room" and isinstance(action, Zap) and action.target is None, reason
    assert turn.cell.lit is False
    result = turn.submit(action)
    assert result.status is Outcome.CONFIRMED
    after = game.wait_for_decision()
    assert after.outcome_of(result).value == "resolved" and after.cell.lit is True
    assert len(after.visible_cells) > len(turn.visible_cells)
    report = play(game, bot, 0, lambda row: None)
    device, = report["last_carried_devices"]
    assert device["item"] == action.device.item.target and device["identity"] == action.device.item.identity
"""
        result = subprocess.run([sys.executable, "-c", code, self.address], cwd=self.directory,
                                env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN},
                                text=True, capture_output=True, timeout=45)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_unknown_scroll_trial_identifies_a_device_at_a_quiet_exit(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        character, = manifest["characters"]
        character.update(known_identities=[], loadout=[{"archetype": kind, "quantity": 1}
                         for kind in ("rogue.identify_wand", "rogue.wand_polymorph")])
        contents["regions"][0].update(actors=[], items=[])
        manifest["stair_pairs"] = {"exit": {"upper": "1/start", "lower": "2/up"}}
        contents["regions"].append({"id": 2, "name": "Lower", "size": [5, 5, 2], "chamber": True,
                                    "anchors": {"up": [2, 2, 0]}})
        self.server(scenario=write_package(self, contents, "study-scroll"))
        self.install_bot_extension()
        code = """
import os, sys
from rogue_player import Adventurer, play
from tor_client_bot import Session
token = os.environ["TOR_SERVER_TOKEN"]
with Session.connect(sys.argv[1], token, 1) as game:
    rows = []
    player = Adventurer(26, "item.amulet.amulet-of-yendor", depth=3)
    play(game, player, 4, rows.append)
    assert rows[0]["reason"] == "study_scroll_at_exit", rows[0]["reason"]
    assert any(r.get("reason") == "identify" and r["result"]["status"] == "confirmed" for r in rows)
with Session.connect(sys.argv[1], token, 1) as witness:
    staff, = witness.observe().devices
    assert staff.item.identity == "item.wand.wand-polymorph" and staff.charges > 0, staff
"""
        result = subprocess.run([sys.executable, "-c", code, self.address], cwd=self.directory,
                                env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN},
                                text=True, capture_output=True, timeout=45)
        self.assertEqual(result.returncode, 0, result.stderr)

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
    player.search_history.wall_searches.update(walls)
    memory.visited.update(memory.cells)
    player.search_history.completed_end_searches.update({key: 6 for key in memory.cells if key != end and memory.is_dead_end(key)})
    action, reason = player.choose(turn)
    assert (action, reason) == (Move(Direction.EAST), "search"), (action, reason)
    assert turn.submit(action).status is Outcome.CONFIRMED
    turn = game.wait_for_decision()
    assert turn.cell.key == end
    player.observe(turn)
    memory.visited.update(memory.cells)
    for completed in range(1, 7):
        action, reason = player.choose(turn)
        assert isinstance(action, UseAbility) and action.ability.identity == "rogue.search"
        receipt = turn.submit(action)
        assert receipt.status is Outcome.CONFIRMED
        player.admitted(turn, action, receipt)
        turn = game.wait_for_decision()
        player.observe(turn)
        assert player.search_history.completed_end_searches[end] == completed
    assert turn.hunger == "nourished"
    memory.visited.update(memory.cells)
    player.search_history.wall_searches.update({c.key: 3 for c in memory.cells.values() if c.wall})
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

    def test_bot_uses_one_haste_dose_before_a_group_closes(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        character, = contents["manifest"]["characters"]
        character.update(loadout=[{"archetype": "rogue.haste", "quantity": 2}],
                         known_identities=["item.potion.haste"])
        room, = contents["regions"]
        room["actors"] = [{"id": number, "archetype": "rogue.hobgoblin", "at": [4, y, 0],
                           "controller": "external", "turn_ticks": 1000000} for number, y in ((3, 1), (4, 3))]
        self.server(scenario=write_package(self, contents, "haste-before-contact"))
        self.install_bot_extension()
        code = """
from contextlib import ExitStack
import os, sys
from rogue_player import Adventurer
from tor_client_bot import ItemUse, Outcome, Session, UseItem, Wait

with ExitStack() as stack:
    game, first, second = [stack.enter_context(Session.connect(sys.argv[1], os.environ["TOR_SERVER_TOKEN"], i))
                           for i in (1, 3, 4)]
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    before = game.wait_for_decision()
    player.observe(before)
    action, reason = player.choose(before)
    assert isinstance(action, UseItem) and action.use is ItemUse.DRINK and reason == "combat_haste", (action, reason)
    receipt = before.submit(action)
    assert receipt.status is Outcome.CONFIRMED
    player.admitted(before, action, receipt, reason=reason)
    for enemy in (first, second):
        assert enemy.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
    after = game.wait_for_decision()
    player.observe(after)
    assert after.has_condition("rogue.hasted")
    assert sum(i.quantity for i in after.inventory if i.identity == "item.potion.haste") == 1
    assert player.choose(after)[1] != "combat_haste"
"""
        result = subprocess.run([sys.executable, "-c", code, self.address], cwd=self.directory,
                                env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN},
                                capture_output=True, text=True, timeout=55)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_bot_slows_a_dangerous_foe_before_contact(self):
        self.check_control_wand("slow", 3)

    def test_bot_damages_a_dangerous_foe_with_a_known_magic_missile(self):
        self.check_control_wand("magic_missile", 3)

    def test_bot_polymorphs_a_dangerous_approaching_foe(self):
        self.check_control_wand("polymorph", 3)

    def test_bot_polymorphs_a_lethal_foe_in_contact(self):
        self.check_control_wand("polymorph", 2)

    def test_polymorph_reports_an_unaffected_target_after_a_resolved_cast(self):
        self.check_control_wand("polymorph", 2, resists=True)

    def test_fire_wand_damages_the_foe_in_a_clear_lane(self):
        self.check_control_wand("fire", 3)

    def test_cold_wand_damages_the_foe_in_a_clear_lane(self):
        self.check_control_wand("cold", 3)

    def test_lightning_wand_damages_the_foe_in_a_clear_lane(self):
        self.check_control_wand("lightning", 3)

    def test_elemental_device_is_kept_when_the_wall_is_too_close_behind_the_foe(self):
        self.check_control_wand("lightning", 3, blocked_ray=True)

    def test_bot_defers_a_resisted_polymorph_with_a_second_attacker_in_contact(self):
        self.check_control_wand("polymorph", 2, resists=True, second_attacker=True)

    def test_bot_discovers_an_unknown_directional_wand_from_a_safe_distance(self):
        self.check_control_wand("fire", 4, unknown=True)

    def check_control_wand(self, wand, enemy_x, *, resists=False, blocked_ray=False, second_attacker=False, unknown=False):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        character, = contents["manifest"]["characters"]
        character.update(loadout=[{"archetype": f"rogue.wand_{wand}", "quantity": 1}],
                         known_identities=[] if unknown else [f"item.wand.wand-{wand.replace('_', '-')}"])
        if resists:
            character["loadout"].append({"archetype": "rogue.identify_wand", "quantity": 1})
        # Fix the opposed wand check so the fixture exercises the intended outcome.
        species = contents["manifest"]["creatures"]["species"][character["creature"]["species"]]
        species.setdefault("modifiers", []).append({"scope": "always", "op": {
            "kind": "add", "stat": {"kind": "attribute", "attribute": "intellect"}, "value": -100 if resists else 100}})
        region, = contents["regions"]
        species_id = "rogue.griffin" if wand == "polymorph" else "rogue.hobgoblin"
        region.update(size=[17, 9, 2], actors=[{"archetype": species_id, "id": 3,
                      "at": [enemy_x, 2, 0], "controller": "external", "turn_ticks": 1000000}])
        if second_attacker:
            region["actors"].append({"archetype": "rogue.hobgoblin", "id": 4, "at": [0, 2, 0],
                                     "controller": "external", "turn_ticks": 1000000})
        if blocked_ray:
            from outcome_scripts import apply

            apply(contents["manifest"], self.directory / "escape-device", "death")
            # Let the ray pass the dodging target on both legs and reach its caster.
            opponent = contents["manifest"]["creatures"]["species"][species_id]
            opponent.setdefault("modifiers", []).append({"scope": "always", "op": {
                "kind": "add", "stat": {"kind": "attribute", "attribute": "speed"}, "value": 300}})
            region["walls"] = [[5, 2, z] for z in range(2)]
        self.server(scenario=write_package(self, contents, "escape-device"))
        self.install_bot_extension()
        options = {"wand": wand, "unknown": unknown, "contact": enemy_x == 2, "resists": resists,
                   "blocked_ray": blocked_ray, "second_attacker": second_attacker}
        code = ("import json, sys; from test_rogue_bot_process import check_wand_encounter; "
                "check_wand_encounter(sys.argv[1], **json.loads(sys.argv[2]))")
        run = subprocess.run([sys.executable, "-c", code, self.address, json.dumps(options)], cwd=self.directory,
                             env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN},
                             capture_output=True, text=True, timeout=55)
        self.assertEqual(run.returncode, 0, run.stderr)

    def test_bot_reduces_melee_contacts_in_a_confined_room(self):
        self.check_outnumbered_move(False)

    def test_bot_enters_a_corridor_before_two_hostiles_reach_it(self):
        self.check_outnumbered_move(True)

    def test_injured_bot_escapes_an_untimed_trap_then_recovers_in_place(self):
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        add_injury_potion(manifest)
        character, = manifest["characters"]
        character["loadout"] = [{"archetype": "injury", "quantity": 1}]
        character["known_identities"].append("item.potion.fixture.injury")
        region, = contents["regions"]
        region.update(anchors={"start": [2, 1, 0]}, actors=[], traps=[{
            "id": 1, "at": [2, 2, 0], "dc": 100, "avoidance_dc": 100,
            "kind": {"kind": "bear_trap_escape", "condition": "rogue.bear_trap", "escape_dc": 0},
        }])
        code = """
import os
from rogue_player import Adventurer, recovery_deficit
from tor_client_bot import Direction, ItemUse, Move, Outcome, Session, UseItem, Wait

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    policy = Adventurer(26, "item.amulet.amulet-of-yendor")
    turn = game.wait_for_decision()
    dose, = turn.inventory
    assert turn.submit(UseItem(dose, ItemUse.DRINK)).status is Outcome.CONFIRMED
    turn = game.wait_for_decision()
    assert turn.submit(Move(Direction.SOUTH)).status is Outcome.CONFIRMED
    turn = game.wait_for_decision()
    assert policy.trapped(turn) and recovery_deficit(turn.health) > 0
    policy.observe(turn)
    step, reason = policy.choose(turn)
    assert isinstance(step, Move) and reason == "escape_bear_trap", (step, reason)
    receipt = turn.submit(step)
    assert receipt.status is Outcome.CONFIRMED
    policy.admitted(turn, step, receipt, reason=reason)
    released = game.wait_for_decision()
    assert released.cell.key == turn.cell.key and not policy.trapped(released)
    hp = released.health.current
    due = released.tick + released.recovery.ticks_to_regain(1)
    while released.tick < due:
        policy.observe(released)
        action, reason = policy.choose(released)
        assert (action, reason) == (Wait(), "recover"), (action, reason)
        assert released.submit(action).status is Outcome.CONFIRMED
        released = game.wait_for_decision()
    assert released.cell.key == turn.cell.key and released.health.current > hp
"""
        self.check_policy_scenario(contents, "injured-trap-escape", code)

    def test_trapped_bot_does_not_claim_a_corridor_while_pursuers_approach(self):
        self.check_outnumbered_move(True, trapped=True)

    def check_outnumbered_move(self, approaching, *, trapped=False):
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
                actor.update(at=[5, y, 0], turn_ticks=100)
        if trapped:
            region["anchors"]["start"] = [3, 1, 0]
            region["traps"] = [{"id": 1, "at": [3, 2, 0], "dc": 100, "avoidance_dc": 100,
                                "kind": {"kind": "bear_trap_escape", "condition": "rogue.bear_trap", "escape_dc": 0}}]
        self.server(scenario=write_package(self, contents, "confined-melee-retreat"))
        self.install_bot_extension()
        code = """
import os
import time
from contextlib import ExitStack
from rogue_player import Adventurer
from tor_client_bot import Direction, Move, Outcome, Session, Wait

with ExitStack() as stack:
    game, east, north = [stack.enter_context(Session.connect(os.environ["BOT_TEST_ADDRESS"],
                         os.environ["TOR_SERVER_TOKEN"], actor)) for actor in [1, 3, 4]]
    turn = game.wait_for_decision()
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    if os.environ["TRAPPED"] == "True":
        assert turn.submit(Move(Direction.SOUTH)).status is Outcome.CONFIRMED
        for enemy in (east, north):
            assert enemy.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
        turn = game.wait_for_decision()
        assert player.trapped(turn)
        player.observe(turn)
        step, reason = player.choose(turn)
        assert isinstance(step, Move) and reason == "escape_bear_trap", (step, reason)
        receipt = turn.submit(step)
        assert receipt.status is Outcome.CONFIRMED
        player.admitted(turn, step, receipt, reason=reason)
        assert player.corridor_stand is None
        for enemy in (east, north):
            assert enemy.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
        released = game.wait_for_decision()
        assert released.cell.key == turn.cell.key and not player.trapped(released)
        raise SystemExit(0)
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
    receipt = turn.submit(step)
    assert receipt.status is Outcome.CONFIRMED
    player.admitted(turn, step, receipt, reason=reason)
    def next_player_turn(*, advance_east=False):
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            turn = game.observe()
            if turn.can_act:
                return turn
            for enemy in (east, north):
                enemy_turn = enemy.observe()
                if enemy_turn.can_act:
                    move = Move(Direction.WEST) if advance_east and enemy is east else Wait()
                    assert enemy_turn.submit(move).status is Outcome.CONFIRMED
            time.sleep(0.01)
        raise AssertionError("Player did not regain its turn")

    after = next_player_turn()
    assert after.cell.key == destination
    assert sum(after.in_contact(foe) for foe in after.hostiles) == (0 if approaching else 1)
    if approaching:
        player.observe(after)
        action, reason = player.choose(after)
        assert isinstance(action, Wait) and reason == "hold_corridor", (action, reason)
        for _ in range(2):
            assert after.submit(action).status is Outcome.CONFIRMED
            after = next_player_turn(advance_east=True)
            player.observe(after)
            action, reason = player.choose(after)
            assert after.cell.key == destination
            if any(after.in_contact(foe) for foe in after.hostiles):
                break
            assert reason == "hold_corridor", (action, reason)
        assert sum(after.in_contact(foe) for foe in after.hostiles) == 1
        assert reason == "melee", (action, reason)
        assert after.submit(action).status is Outcome.CONFIRMED
"""
        run = subprocess.run([sys.executable, "-c", code], cwd=self.directory,
                             env={**os.environ, "APPROACHING": str(approaching), "TRAPPED": str(trapped), "PYTHONPATH": str(ROOT / "scripts"),
                                  "BOT_TEST_ADDRESS": self.address, "TOR_SERVER_TOKEN": TOKEN},
                             capture_output=True, text=True, timeout=55)
        self.assertEqual(run.returncode, 0, run.stderr)

    def test_pinned_bot_attacks_the_opponent_occupying_its_exit(self):
        self.pinned_melee("basic_melee")

    def test_bot_uses_its_learned_rattling_blow_in_contact(self):
        self.pinned_melee("rattling_blow")

    def test_pinned_bot_pushes_an_opponent_off_the_exit(self):
        self.pinned_melee("dislocating_strike")

    def test_pinned_carrier_clears_the_upstairs_on_its_first_floor(self):
        self.pinned_melee("basic_melee", returning=True)

    def test_extended_player_pushes_from_its_outer_body_cell(self):
        self.pinned_melee("dislocating_strike", extended=True)

    def pinned_melee(self, expected, *, returning=False, extended=False):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        if expected in {"rattling_blow", "dislocating_strike"}:
            build = manifest["characters"][0]["creature"]
            build["hit_dice"][-1]["purchases"].append(
                {"pool": "advancement", "selection": {"kind": "talent", "talent": "rattling_blow"}})
            if expected == "dislocating_strike":
                build["hit_dice"].extend({"source": "warrior", "purchases": []} for _ in range(4))
                build["hit_dice"][-1]["purchases"].append(
                    {"pool": "advancement", "selection": {"kind": "talent", "talent": expected}})
        manifest["characters"][0]["loadout"] = [{"archetype": "rogue.mace", "quantity": 1, "equipped_slot": 0}]
        upper, lower = ("2/landing", "1/guarded") if returning else ("1/guarded", "2/landing")
        manifest["stair_pairs"] = {"exit": {"upper": upper, "lower": lower}}
        region, = contents["regions"]
        width = 4 if expected == "dislocating_strike" else 3
        blocker_x = 3 if extended else 2
        if extended:
            character, = manifest["characters"]
            character["body"] = {"cells": [[0, 0, 0], [0, 0, 1], [1, 0, 0]], "eye": [0, 0, 1], "mass": 80}
            package = character["creature"]["body_package"]
            manifest["creatures"]["body_packages"][package]["body"] = character["body"]
            blocker = manifest["archetypes"]["rogue.hobgoblin"]
            blocker["body"]["cells"].append([1, 0, 0])
            package = blocker["creature"]["body_package"]
            manifest["creatures"]["body_packages"][package]["body"] = blocker["body"]
            width += 2
        region.update(size=[width, 1, 2], anchors={"start": [1, 0, 0], "guarded": [blocker_x, 0, 0]},
                      actors=[{"archetype": "rogue.hobgoblin", "id": i, "at": [x, 0, 0],
                               "controller": "external", "turn_ticks": 1000000} for i, x in [(3, 0), (4, blocker_x)]])
        if extended:
            region["actors"][0]["archetype"] = "rogue.orc"
        contents["regions"].append({"id": 2, "name": "Landing", "size": [3, 3, 2],
                                    "chamber": True, "anchors": {"landing": [1, 1, 0]}})
        if returning:
            character, = manifest["characters"]
            character["loadout"].append({"archetype": "rogue.amulet_of_yendor", "quantity": 1})
            character["known_identities"].append("item.amulet.amulet-of-yendor")
        # Seed 43 lands the strike, allowing the following frame to verify the push.
        self.server(scenario=write_package(self, contents, "stair-blocker"),
                    seed=43 if expected == "dislocating_strike" else 42)
        self.install_bot_extension()
        code = """
import os
from contextlib import ExitStack
from rogue_player import Adventurer
from tor_client_bot import ActivityPhase, Direction, Move, Outcome, Session, Wait

with ExitStack() as clients:
    game, west, east = [clients.enter_context(Session.connect(os.environ["BOT_TEST_ADDRESS"],
                        os.environ["TOR_SERVER_TOKEN"], i)) for i in (1, 3, 4)]
    turn = game.wait_for_decision()
    assert len(turn.hostiles) == 2 and not turn.walkable_directions
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    player.observe(turn)
    returning = player.carrying_objective(turn)
    assert returning == (os.environ["RETURNING"] == "1")
    direction = Direction.UP if returning else Direction.DOWN
    if not returning:
        # At retrieval depth the down stair has no tactical value. Prefer the harder hitter.
        at_goal = Adventurer(26, "item.amulet.amulet-of-yendor", depth=26)
        at_goal.observe(turn)
        harder = turn.hostiles[1]
        at_goal.opponents.damage_by_target[harder.target] = 3
        attack_at_goal, why = at_goal.choose(turn)
        assert why == "melee" and attack_at_goal.target == harder, (attack_at_goal, why)
    blocker_dx = max(x for x, _, _ in turn.body) + 1
    exit_cell = turn.cell_at((blocker_dx, 0, 0))
    other = next(foe for foe in turn.hostiles if exit_cell not in turn.cells_of(foe))
    player.opponents.damage_by_target[other.target] = 3
    player.unsafe_stairs[exit_cell.key, direction] = turn.tick + 1
    guarded_attack, why = player.choose(turn)
    assert why == "melee" and guarded_attack.target == other, (guarded_attack, why, turn.body, exit_cell, other, turn.hostiles)
    player.unsafe_stairs[exit_cell.key, direction] = turn.tick
    action, reason = player.choose(turn)
    assert (exit_cell.stairs_up if returning else exit_cell.stairs_down)
    assert reason == "melee" and exit_cell in turn.cells_of(action.target)
    assert action.ability.identity == os.environ["EXPECTED_MELEE"], (action, turn.abilities)
    if blocker_dx > 1:
        assert (blocker_dx + 1, 0, 0) in action.target.cells, action.target
    attack = turn.submit(action)
    assert attack.status is Outcome.CONFIRMED
    for foe in (west, east):
        assert foe.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
    after = game.wait_for_decision()
    assert after.outcome_of(attack) is ActivityPhase.RESOLVED
    incoming = east.observe().recent_combat.incoming
    assert any(hit.ability == action.ability.identity for hit in incoming), incoming
    if action.ability.identity == "dislocating_strike":
        assert after.cell_at((blocker_dx, 0, 0)).stairs_down
        pushed = next(foe for foe in after.hostiles if foe.target == action.target.target)
        assert set(pushed.cells) == {(x + 1, y, z) for x, y, z in action.target.cells}, pushed
    if action.ability.identity == "dislocating_strike" and blocker_dx == 1:
        player.observe(after)
        step, reason = player.choose(after)
        assert (step, reason) == (Move(Direction.EAST), "retreat_to_stairs"), (step, reason)
        assert after.submit(step).status is Outcome.CONFIRMED
        on_exit = game.wait_for_decision()
        assert on_exit.cell.stairs_down
        player.observe(on_exit)
        assert player.choose(on_exit) == (Move(Direction.DOWN), "retreat_downstairs")

"""
        run = subprocess.run([sys.executable, "-c", code], cwd=self.directory,
                             env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"),
                                  "BOT_TEST_ADDRESS": self.address, "TOR_SERVER_TOKEN": TOKEN, "EXPECTED_MELEE": expected, "RETURNING": str(int(returning))},
                             capture_output=True, text=True, timeout=55)
        self.assertEqual(run.returncode, 0, run.stderr)

    def test_bot_uses_a_two_step_escape_against_a_learned_damage_threat(self):
        self.check_escape_from_learned_threat(
            walls=((2, 1), (3, 2), (1, 1), (3, 3)), foe_at=(2, 3), direction="west", reason="break_contact")

    def test_bot_breaks_contact_toward_the_open_corridor(self):
        self.check_escape_from_learned_threat(
            walls=((1, 1), (1, 2), (1, 3), (2, 0), (3, 1), (3, 3)),
            foe_at=(3, 2), direction="south", reason="retreat")

    def test_bot_avoids_a_disclosed_trap_when_both_steps_break_contact(self):
        self.check_escape_from_learned_threat(
            walls=((1, 1), (1, 2), (1, 3), (2, 0), (3, 1), (3, 3)), foe_at=(3, 2),
            direction="north", reason="retreat", trap_at=(2, 3))

    def test_bot_selects_the_corridor_with_a_clear_second_step(self):
        self.check_escape_from_learned_threat(
            walls=((1, 0), (1, 1), (1, 2), (1, 3), (1, 4), (3, 0), (3, 1), (3, 3), (3, 4)),
            foe_at=(3, 2), direction="south", reason="retreat", trap_at=(2, 0))

    def check_escape_from_learned_threat(self, *, walls, foe_at, direction, reason, trap_at=None):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        region, = contents["regions"]
        region.update(anchors={"start": [2, 2, 0]},
                      walls=[[x, y, z] for x, y in walls
                             for z in range(2)],
                      actors=[{"archetype": "rogue.hobgoblin", "id": 3, "controller": "external",
                               "at": [*foe_at, 0], "turn_ticks": 1000000}])
        if trap_at:
            region["traps"] = [{"id": 1, "at": [*trap_at, 0], "dc": 0, "avoidance_dc": 100,
                                "kind": {"kind": "bear_trap_escape", "condition": "rogue.bear_trap", "escape_dc": 0}}]
        server = self.server(scenario=write_package(self, contents, "rogue-bot-learned-threat-retreat"))
        self.install_bot_extension()
        code = """
import os, sys
from rogue_player import Adventurer
from tor_client_bot import Direction, Move, Outcome, Session, Wait

address, token = os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"]
with Session.connect(address, token, 1) as game, Session.connect(address, token, 3) as enemy:
    turn = game.wait_for_decision()
    if len(sys.argv) > 3:
        search = Adventurer.ready_search(turn)
        assert search and turn.submit(search).status is Outcome.CONFIRMED
        assert enemy.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
        turn = game.wait_for_decision()
        assert turn.cell_at((int(sys.argv[3]) - 2, int(sys.argv[4]) - 2, 0)).trap is not None
    foe = next(c for c in turn.hostiles if c.name.casefold() == "hobgoblin")
    direction = Direction(sys.argv[1])
    destination = turn.cell_at(direction.offset)
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    player.observe(turn)
    assert turn.health.current * 4 > turn.health.maximum
    player.opponents.damage_by_target[foe.target] = (turn.health.current + 1) // 2
    step, reason = player.choose(turn)
    assert (step, reason) == (Move(direction), sys.argv[2]), (step, reason)
    assert turn.submit(step).status is Outcome.CONFIRMED
    if len(sys.argv) == 3:  # The search already advanced the external foe.
        assert enemy.wait_for_decision().submit(Wait()).status is Outcome.CONFIRMED
    assert game.wait_for_decision().cell.key == destination.key
"""
        result = subprocess.run(
            [sys.executable, "-c", code, direction, reason, *map(str, trap_at or ())], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bot_moves_when_an_invisible_enemy_attacks(self):
        self.check_unseen_attack()

    def test_bot_reveals_an_invisible_attacker_with_a_known_potion(self):
        self.check_unseen_attack(sight="potion")

    def test_bot_equips_a_sight_ring_to_reveal_an_invisible_attacker(self):
        self.check_unseen_attack(sight="ring")

    def test_cohort_attributes_a_live_unseen_attack_as_the_cause_of_death(self):
        self.check_unseen_attack(let_die=True)

    def check_unseen_attack(self, *, sight="", let_die=False):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        region, = contents["regions"]
        region["actors"][0].update(at=[2, 2, 0], turn_ticks=100)
        character = contents["manifest"]["characters"][0]
        character["loadout"] = []
        if sight:
            character["loadout"].append({"archetype": "rogue.see_invisible", "quantity": 1})
            character["known_identities"].append("item.potion.see-invisible")
        if sight == "ring":
            character["loadout"].append({"archetype": "rogue.ring_see_invisible", "quantity": 1})
            character["known_identities"].append("item.ring.ring-see-invisible")
        server = self.server(scenario=write_package(self, contents, "rogue-bot-unseen-attack"))
        self.install_bot_extension()
        code = """
import os
import time
from rogue_player import Adventurer
from rogue_o_matic_cohort import death_cause
from tor_client_bot import CombatJournal, Equip, Move, Outcome, Session, UseItem, Wait

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
    assert policy.danger_until.get(start, 0) > turn.tick, policy.danger_until
    step, reason = policy.choose(turn)
    sight = os.environ["BOT_TEST_SIGHT"]
    if sight == "ring":
        assert isinstance(step, Equip) and step.item.identity == "item.ring.ring-see-invisible", (step, reason)
    elif sight:
        assert isinstance(step, UseItem) and reason == "reveal_attacker", (step, reason)
    else:
        assert isinstance(step, Move) and reason == "evade_unseen_attack", (step, reason)
    assert turn.submit(step).status is Outcome.CONFIRMED
    policy.admitted(turn, step)
    for _ in range(150):
        after = player.observe()
        if sight and any(c.name == "phantom" for c in after.creatures):
            break
        if not sight and after.can_act and after.cell.key != start:
            break
        opponent = phantom.observe()
        if opponent.can_act:
            result = opponent.submit(Wait())
            assert result.status in {Outcome.CONFIRMED, Outcome.STALE}, result
        time.sleep(0.02)
    else:
        raise AssertionError(("Unseen attack response did not settle", reason, after.tick))
    if sight == "ring":
        assert any(s.occupant and s.occupant.identity == step.item.identity for s in after.equipment)
        assert any(i.identity == "item.potion.see-invisible" for i in after.inventory)
    elif sight:
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
                 "BOT_TEST_ADDRESS": self.address, "BOT_TEST_SIGHT": sight,
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
        add_injury_potion(manifest)
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
            if not food_limited:
                contents["regions"][-1]["items"] = [
                    {"id": 15, "archetype": "rogue.amulet_of_yendor", "at": [2, 2, 0]}]
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
            player.travel_times.ascent[2] = turn.nutrition.ticks_until_empty
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
        if not limited:
            memory = player.maps[player.depth]
            landing, = memory.stairs(Direction.DOWN)
            assert landing == next(cell.key for cell in turn.visible_cells if cell.stairs_down)
            player.unsafe_stairs[landing, Direction.DOWN] = turn.tick + 100
            action, reason = player.choose(turn)
            assert (action, reason) == (Wait(), "recover"), (action, reason)
            turn = submit(turn, action)
            assert landing in player.safe_stairs(turn, Direction.DOWN)
            hall = turn.cell_at(east.direction.offset).key
            player.danger_until[hall] = turn.tick + 100
            player.travel_times.ascent[2] = turn.nutrition.ticks_until_empty
            safe_step = memory.toward(turn, {landing}, avoid={hall})
            assert safe_step is not None and safe_step != east
            assert player.choose(turn) == (safe_step, "seek_stairs")
            player.travel_times.ascent[2] -= turn.recovery.ticks_to_regain(1) + 1
            action, reason = player.choose(turn)
            assert (action, reason) == (Wait(), "recover"), (action, reason)
            sheltered = submit(turn, action)
            assert sheltered.cell.key == turn.cell.key and hall not in player.danger_until
            turn = sheltered
            player.travel_times.ascent.clear()
            # Replay the decision with the alternate passage remembered as the familiar one.
            alternate = turn.cell_at(safe_step.direction.offset).key
            memory.visited.remove(hall)
            memory.visited.add(alternate)
            assert player.choose(turn) == (safe_step, "recover_en_route")
            memory.visited.remove(alternate)
            memory.visited.add(hall)
        for _ in range(2):
            if not limited:
                assert player.choose(turn) == (east, "recover_en_route")
            before = turn.recovery.elapsed_ticks
            turn = submit(turn, east)
            assert turn.recovery.elapsed_ticks > before
        assert turn.cell.stairs_down
        if not limited:
            assert player.choose(turn) == (Wait(), "recover")
            player.travel_times.ascent[2] = turn.nutrition.ticks_until_empty
        action, reason = player.choose(turn)
        assert (action, reason) == (Move(Direction.DOWN), "descend"), (action, reason)
        landing = submit(turn, action)
        assert landing.cell.stairs_up
        if not limited:
            action, reason = player.choose(landing)
            assert reason == "take", (action, reason)
            landing = submit(landing, action)
            assert player.carrying_objective(landing)
            # The spent route below the player exceeds all remaining food.
            player.travel_times.ascent = {1: 100, player.depth + 1: landing.nutrition.ticks_until_empty}
            assert player.choose(landing) == (Wait(), "recover")
            start_hp = landing.health.current
            delay = landing.recovery.ticks_to_regain(1)
            assert delay is not None and delay > 0 and start_hp < landing.health.maximum
            due = landing.tick + delay
            while landing.tick < due:
                assert landing.health.current == start_hp
                landing = submit(landing, Wait())
            assert landing.health.current == start_hp + 1
            player.travel_times.ascent[1] = landing.nutrition.ticks_until_empty
            action, reason = player.choose(landing)
            assert (action, reason) == (Move(Direction.UP), "ascend")
            assert submit(landing, action).cell.stairs_down
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

    def test_bot_restores_sight_while_healing_a_minor_injury(self):
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        add_injury_potion(manifest, damage=1)
        character, = manifest["characters"]
        character.update(loadout=[{"archetype": name, "quantity": 1} for name in
                                  ("rogue.extra_healing", "rogue.healing", "rogue.blindness", "injury")],
                         known_identities=["item.potion.extra-healing", "item.potion.healing",
                                           "item.potion.blindness", "item.potion.fixture.injury"])
        contents["regions"][0]["actors"] = []
        self.check_policy_scenario(contents, "minor-injury-blindness", """
import os
from rogue_player import Adventurer
from tor_client_bot import ItemUse, Outcome, Session, UseItem

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.wait_for_decision()
    for identity in ("item.potion.fixture.injury", "item.potion.blindness"):
        dose = next(item for item in turn.inventory if item.identity == identity)
        assert turn.submit(UseItem(dose, ItemUse.DRINK)).status is Outcome.CONFIRMED
        turn = game.wait_for_decision()
    assert turn.has_condition("rogue.blindness") and turn.health.current < turn.health.maximum <= 2 * turn.health.current
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    player.observe(turn)
    action, reason = player.choose(turn)
    assert reason == "heal" and action.item.identity == "item.potion.healing", (action, reason)
    assert turn.submit(action).status is Outcome.CONFIRMED
    after = game.wait_for_decision()
    assert not after.has_condition("rogue.blindness") and after.health.current > turn.health.current
    assert any(item.identity == "item.potion.extra-healing" for item in after.inventory)
""")

    def test_injured_bot_cures_hallucination_with_extra_healing(self):
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        character, = manifest["characters"]
        injury = "item.potion.fixture.injury"
        add_injury_potion(manifest, damage=2, stackable=True)
        character["loadout"] = [{"archetype": name, "quantity": quantity} for name, quantity in (
            ("rogue.extra_healing", 1), ("rogue.healing", 1), ("rogue.hallucination", 1), ("injury", 20))]
        character["known_identities"].extend((injury, "item.potion.healing", "item.potion.extra-healing",
                                              "item.potion.hallucination"))
        contents["regions"][0]["actors"] = []
        code = """
import os
from rogue_player import Adventurer
from tor_client_bot import ItemUse, Outcome, Session, UseItem

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as player:
    def drink(turn, identity):
        dose = next(item for item in turn.inventory if item.identity == identity)
        assert turn.submit(UseItem(dose, ItemUse.DRINK)).status is Outcome.CONFIRMED
        return player.wait_for_decision()

    turn = player.wait_for_decision()
    while turn.health.current * 2 >= turn.health.maximum:
        turn = drink(turn, "item.potion.fixture.injury")
    policy = Adventurer(26, "item.amulet.amulet-of-yendor")
    policy.observe(turn)
    action, reason = policy.choose(turn)
    assert reason == "heal" and action.item.identity == "item.potion.healing", (action, reason)
    turn = drink(turn, "item.potion.hallucination")
    assert turn.has_condition("rogue.hallucinating") and not turn.hostiles
    assert any(item.identity == "item.potion.healing" and ItemUse.DRINK in item.uses for item in turn.inventory)
    policy.observe(turn)
    action, reason = policy.choose(turn)
    assert reason == "heal" and action.item.identity == "item.potion.extra-healing", (action, reason)
    assert turn.submit(action).status is Outcome.CONFIRMED
    after = player.wait_for_decision()
    assert after.health.current > turn.health.current and not after.has_condition("rogue.hallucinating")
    assert any(item.identity == "item.potion.healing" for item in after.inventory)
    assert all(item.identity != "item.potion.extra-healing" for item in after.inventory)
"""
        self.check_policy_scenario(contents, "rogue-bot-hallucination-healing", code)

    def test_bot_heals_above_half_health_when_cornered_by_a_lethal_threat(self):
        self.check_cornered_policy(unknown_scrolls=False)

    def test_injured_bot_conserves_extra_healing_against_a_low_damage_contact(self):
        self.check_cornered_policy(unknown_scrolls=False, low_damage=True)

    def test_bot_attacks_after_one_unidentified_scroll_in_a_fight(self):
        self.check_cornered_policy(unknown_scrolls=True)

    def check_cornered_policy(self, *, unknown_scrolls, low_damage=False):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        manifest["archetypes"]["rogue.extra_healing"]["name"] = "Restorative draught"
        add_injury_potion(manifest, stackable=True)
        character, = manifest["characters"]
        gear = ("rogue.enchant_armor", "rogue.protect_armor") if unknown_scrolls else ("rogue.healing", "rogue.extra_healing")
        if low_damage:
            gear = tuple(reversed(gear))
        character["loadout"] = [{"archetype": "injury", "quantity": 20 if low_damage else 1},
                                *({"archetype": name, "quantity": 1} for name in gear)]
        character["known_identities"].extend(("item.potion.fixture.injury", "item.potion.healing", "item.potion.extra-healing"))
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
    low_damage = os.environ["BOT_TEST_LOW_DAMAGE"] == "1"
    while low_damage and turn.health.current * 2 >= turn.health.maximum:
        dose = next(item for item in turn.inventory if item.identity == "item.potion.fixture.injury")
        assert turn.submit(UseItem(dose, ItemUse.DRINK)).status is Outcome.CONFIRMED
        turn = player.wait_for_decision()
    hp = turn.health.current
    assert turn.health.maximum > hp and ((hp * 2 < turn.health.maximum) == low_damage)
    foe = next(c for c in turn.hostiles if c.name.casefold() == "hobgoblin")
    policy = Adventurer(26, "item.amulet.amulet-of-yendor")
    policy.observe(turn)
    policy.opponents.damage_by_target[foe.target] = 1 if low_damage else hp
    action, reason = policy.choose(turn)
    if os.environ["BOT_TEST_UNKNOWN_SCROLLS"] == "1":
        assert isinstance(action, UseItem) and action.use is ItemUse.READ and reason == "test_unknown_magic"
        read = turn.submit(action)
        assert read.status is Outcome.CONFIRMED
        policy.admitted(turn, action, read, reason=reason)
        turn = player.wait_for_decision()
        assert policy.stalled(turn, read)[0] is None
        policy.observe(turn)
        assert any(item.category == "scroll" and item.identity is None for item in turn.inventory)
        action, reason = policy.choose(turn)
        assert isinstance(action, UseAbility) and reason == "melee", (action, reason)
        assert turn.submit(action).status is Outcome.CONFIRMED
        assert player.wait_for_decision().tick > turn.tick
    else:
        assert isinstance(action, UseItem) and action.use is ItemUse.DRINK and reason == "heal"
        if low_damage:
            assert hp > 2 and action.item.identity == "item.potion.healing", action
        else:
            assert action.item.name == "Restorative draught" and action.item.identity == "item.potion.extra-healing"
        assert turn.submit(action).status is Outcome.CONFIRMED
        assert player.wait_for_decision().health.current > hp
"""
        result = subprocess.run(
            [sys.executable, "-c", code], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN,
                 "BOT_TEST_ADDRESS": self.address, "BOT_TEST_LOW_DAMAGE": str(int(low_damage)),
                 "BOT_TEST_UNKNOWN_SCROLLS": str(int(unknown_scrolls))},
            text=True, capture_output=True, timeout=55,
        )
        server.stop(save=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bot_moves_away_from_a_foe_without_reversing_a_recovery_retreat(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        add_injury_potion(manifest)
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
from rogue_player import Adventurer, play
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
    report = play(game, player, 0, print)
    assert report["terminal"] == "action_budget" and report["actions"] == 0, report
    assert report["max_depth"] == 24, report["max_depth"]
    turn = game.wait_for_decision()
    assert player.depth == 23 and turn.cell.stairs_down
    assert turn.health.current * 5 < turn.health.maximum * 4
    threat = next(c for c in turn.hostiles if c.name.casefold() == "hobgoblin")
    action, reason = player.choose(turn)
    assert reason == "retreat", (action, reason)
    player.opponents.damage_by_target[threat.target] = turn.health.current
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

    def test_bot_launches_within_short_range_when_both_ranges_offer_a_target(self):
        from discovery_package import write_package
        from utility_potions_package import fixture

        contents = fixture()
        manifest = contents["manifest"]
        character, = manifest["characters"]
        character["loadout"] = [
            {"archetype": "rogue.shortbow", "equipped_slot": 0, "quantity": 1},
            {"archetype": "rogue.arrow", "quantity": 3},
        ]
        character["known_identities"].extend(("item.weapon.shortbow", "item.weapon.arrow"))
        launcher = manifest["archetypes"]["rogue.shortbow"]["equipment"]["weapon_profile"]["launcher"]
        launcher["range"] = {"short_feet": 15, "long_feet": 25}
        region, = contents["regions"]
        region["actors"] = [
            {"archetype": "rogue.hobgoblin", "id": actor, "controller": "external",
             "at": position, "turn_ticks": 1000000}
            for actor, position in ((3, [5, 1, 0]), (4, [4, 3, 0]))
        ]
        self.server(scenario=write_package(self, contents, "two-bow-targets"))
        self.install_bot_extension()
        code = """
import os, sys
from rogue_player import Adventurer
from tor_client_bot import Outcome, Session

with Session.connect(sys.argv[1], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.wait_for_decision()
    offers = [shot for shot in turn.ranged_attacks() if shot.launcher and shot.target.hostile]
    assert {max(map(abs, shot.offset)) for shot in offers} == {3, 4}, offers
    assert all(shot.range.short_cells == 3 for shot in offers), offers
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    player.observe(turn)
    shot, reason = player.choose(turn)
    assert reason == "launch_projectile" and shot in offers, (shot, reason)
    assert max(map(abs, shot.offset)) == 3, shot
    receipt = turn.submit(shot)
    assert receipt.status is Outcome.CONFIRMED, receipt
"""
        result = subprocess.run(
            [sys.executable, "-c", code, self.address], cwd=self.directory,
            env={**os.environ, "PYTHONPATH": str(ROOT / "scripts"), "TOR_SERVER_TOKEN": TOKEN},
            text=True, capture_output=True, timeout=45,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

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

    def test_visible_guard_is_remembered_at_weapon_and_coin_piles(self):
        from utility_potions_package import fixture

        contents = fixture()
        region, = contents["regions"]
        region.update(actors=[{"archetype": "rogue.hobgoblin", "id": 3, "controller": "external",
                               "at": [4, 2, 0], "turn_ticks": 1000000}],
                      items=[{"id": 9, "archetype": "rogue.dagger", "at": [3, 2, 0]},
                             {"id": 10, "archetype": "rogue.gold", "at": [4, 3, 0]}])
        self.check_policy_scenario(contents, "guarded-piles", """
import os
from rogue_player import Adventurer
from tor_client_bot import Session

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.wait_for_decision()
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    player.observe(turn)
    piles = {ground.cell.key for ground in turn.ground_items if ground.item.category in {"weapon", "coin"}}
    assert turn.hostiles and len(piles) == 2
    assert piles <= player.supplies_guarded_until.keys(), (piles, player.supplies_guarded_until)
""")

    def test_bowless_bot_with_arrows_readies_its_mace_at_medium_range(self):
        from utility_potions_package import fixture

        contents = fixture()
        character, = contents["manifest"]["characters"]
        character.update(loadout=[{"archetype": "rogue.arrow", "quantity": 3},
                                  {"archetype": "rogue.mace", "quantity": 1}],
                         known_identities=["item.weapon.arrow", "item.weapon.mace"])
        region, = contents["regions"]
        region.update(items=[], actors=[{"archetype": "rogue.hobgoblin", "id": 3, "controller": "external",
                                         "at": [5, 2, 0], "turn_ticks": 1000000}])
        self.check_policy_scenario(contents, "bowless-mace", """
import os
from rogue_player import Adventurer
from tor_client_bot import Equip, Outcome, Session

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    turn = game.wait_for_decision()
    assert 3 <= turn.distance_to(turn.hostiles[0]) <= 4
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    player.observe(turn)
    action, reason = player.choose(turn)
    assert isinstance(action, Equip) and reason == "equip_melee", (action, reason)
    assert action.item.identity == "item.weapon.mace"
    assert turn.submit(action).status is Outcome.CONFIRMED
""")

    def test_unarmed_bot_equips_a_recognized_spear_with_undisclosed_stats(self):
        from utility_potions_package import fixture

        contents = fixture()
        character, = contents["manifest"]["characters"]
        character.update(loadout=[], known_identities=["item.weapon.spear"])
        region, = contents["regions"]
        region.update(actors=[], items=[{"id": 9, "at": [1, 2, 0], "archetype": "rogue.gold"},
                                       {"id": 10, "at": [1, 2, 0], "archetype": "rogue.spear", "quantity": 1}])
        self.check_policy_scenario(contents, "unarmed-spear", """
import os
from rogue_player import Adventurer
from tor_client_bot import Equip, Outcome, Pickup, Session

with Session.connect(os.environ["BOT_TEST_ADDRESS"], os.environ["TOR_SERVER_TOKEN"], 1) as game:
    player = Adventurer(26, "item.amulet.amulet-of-yendor")
    turn = game.wait_for_decision()
    for expected in ("weapon", "coin"):
        player.observe(turn)
        take, reason = player.choose(turn)
        assert isinstance(take, Pickup) and reason == "take", (take, reason)
        assert take.item.category == expected, take
        assert turn.submit(take).status is Outcome.CONFIRMED
        turn = game.wait_for_decision()
    player.observe(turn)
    action, reason = player.choose(turn)
    assert isinstance(action, Equip) and reason == "equip_melee", (action, reason)
    assert action.item.identity == "item.weapon.spear" and action.item.weapon_attack is None
    assert turn.submit(action).status is Outcome.CONFIRMED
    after = game.wait_for_decision()
    weapon = next(slot for slot in after.equipment if slot.kind == "weapon")
    assert weapon.occupant.identity == "item.weapon.spear"
""")

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

        terminal, progress = trace_report(output / "seed-42" / "turns.jsonl")
        terminal_trace = self.directory / "terminal-only.jsonl"
        terminal_trace.write_text(json.dumps(terminal), encoding="utf-8")
        _, final_observation = trace_report(terminal_trace)
        self.assertTrue(final_observation["amulet_acquired"])
        self.assertIsNone(final_observation["closest_return_floor"])
        self.assertTrue(progress["amulet_acquired"])
        self.assertEqual(report["max_depth"], 26)
        self.assertEqual(progress["max_depth"], report["max_depth"])
        self.assertEqual(set(progress["floors"]), {str(floor) for floor in range(1, 27)})
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
        add_injury_potion(manifest)
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
    # Prior view from the other side disclosed a passage leading to the stair.
    memory_picture = json.loads(turn.recording())
    prior = memory_picture["state"]["observation"]
    prior["visible_cells"] = [
        {"key": "remembered-stair", "position": {"x": 1, "y": 0, "z": 0}, "wall": False,
         "stairs_up": True, "stairs_down": False},
        {"key": "remembered-hall", "position": {"x": 0, "y": 0, "z": 0}, "wall": False,
         "stairs_up": False, "stairs_down": False},
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
        player.search_history.admissions[key] = 8
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
            add_injury_potion(manifest)
            character["loadout"] = [{"archetype": "rogue.ration", "quantity": 2}, {"archetype": "injury", "quantity": 1}]
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

    def advance(turn, action, reason=None):
        result = turn.submit(action)
        assert result.status is Outcome.CONFIRMED, result
        player.admitted(turn, action, result, reason=reason)
        if isinstance(action, UseItem) and action.use is ItemUse.EAT:
            player.observe(turn)
            assert player.meals.pending is not None
        following = game.wait_for_decision()
        assert following.tick > turn.tick
        player.observe(following)
        return following

    assert bool(turn.ground_items) != scouting
    if scouting:
        action, reason = player.choose(turn)
        assert isinstance(action, UseItem) and action.use is ItemUse.EAT, (action, reason)
        turn = advance(turn, action, reason)
    for _ in range(9 if scouting else 28):
        turn = advance(turn, Move(Direction.EAST))
    if scouting:
        for _ in range(2):
            turn = advance(turn, Move(Direction.WEST))
    assert not turn.ground_items and turn.hunger == "nourished"
    direction = Direction.UP if carrying else Direction.DOWN
    assert player.choose(turn)[0] == Move(direction)
    player.travel_times.ascent[2] = (turn.nutrition.reserve + 1) * turn.nutrition.ticks_per_unit
    if scouting:
        assert player.meals.gains["item.food.ration"] > 0
        player.travel_times.descent[2] = 110000
    action, reason = player.choose(turn)
    assert isinstance(action, Move) and action.direction.offset[0] == (1 if scouting else -1), (action, reason)
    if scouting:
        assert player.depth not in player.food_scouts.by_floor
    destination = turn.cell_at(action.direction.offset).key
    turn = advance(turn, action, reason)
    assert turn.cell.key == destination
    if scouting:
        dose = next(item for item in turn.inventory if ItemUse.DRINK in item.uses)
        turn = advance(turn, UseItem(dose, ItemUse.DRINK))
        injured_health = turn.health.current
        assert injured_health < turn.health.maximum * 0.8
        # The onward journey still needs food. Test a known safe step even with enough reserve to rest here.
        return_estimate = player.travel_times.ascent.pop(2)
        step, why = player.choose(turn)
        assert isinstance(step, Move) and why == "scout_for_food", (step, why)
        assert turn.cell_at(step.direction.offset).key in player.maps[player.depth].visited
        turn = advance(turn, step, why)
        player.travel_times.ascent[2] = return_estimate
    for _ in range(45):
        action, reason = player.choose(turn)
        assert reason in {"scout_for_food", "forage", "seek_item", "take"}, (action, reason)
        turn = advance(turn, action, reason)
        if sum(item.quantity for item in turn.inventory if item.identity == "item.food.ration") == (2 if scouting else 1):
            break
    else:
        raise AssertionError("The bot did not collect the ration")
    if scouting:
        assert turn.health.current > injured_health
    action, reason = player.choose(turn)
    if scouting and turn.health.current < turn.health.maximum * 0.8:
        assert reason == "recover_en_route" and action.direction.offset[0] == -1, (action, reason)
    else:
        assert reason == "seek_stairs", (action, reason)
    if scouting:
        meal = next(item for item in turn.inventory if ItemUse.EAT in item.uses)
        turn = advance(turn, UseItem(meal, ItemUse.EAT))
        action, reason = player.choose(turn)
        assert reason in {"seek_stairs", "recover_en_route"}, (action, reason)
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
