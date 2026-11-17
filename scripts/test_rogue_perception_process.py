"""One bounded M/P/X mechanism contract through ordinary server/client actions.

Production Medusa, Phantom and Xeroc species, recipes, purchases and rules are
copied unchanged. The lit room and externally controlled foe are authored test
geometry/control. The explicitly synthetic living witness has durable Health,
low defenses and an explicit basic-melee check bonus with harmless-sized damage. These are mechanism
witnesses, not actual starter-kit survival, autonomous AI or balance evidence.

The separate tuned-Xeroc autonomous witness below keeps its real starter kit and
checks touch-owned continuation through pending source-free restart. Separate
autonomous Medusa cases distinguish tuned Rogue claw attacks from exploration
gaze/cooldown behavior, including opaque separation and source-free reopen.

All five original fresh encounters use seed 42, without retries or seed search. Assertions
use client-visible state and ordinary commands, never wizard edits or checkpoint
target lookup. The three X encounters separately exercise Take, Search and the
first outgoing attack; a revealed incarnation cannot legitimately rearm itself.

Existing Text/ASCII, native-input and stream-repair contract owners remain
test_items_process, test_ascii_interactions_process, test_ascii_process and
test_stream_recovery_process. This test adds the production content mechanism
sequence rather than multiplying those application-boundary witnesses.

After the compiler owner releases qualified binaries:
TOR_TEST_BINARIES=/absolute/path/to/debug python scripts/test_runner.py \
    test_rogue_perception_process -v
"""
import copy
import json
import shutil
import subprocess
import unittest

from arena_matrix import toml_source
from process_harness import ProcessTestCase
from process_work_budget import ProcessWorkBudget
from rogue_roster_package import PRODUCTION, encounter_sources
from scalar_wire import u64
from status_condition_package import combat, observation, checkpoint, actor_creature


GAZE = "rogue.confusing_gaze"
CONFUSED = "rogue.confused"
SEARCH = "rogue.search"
WITNESS = "Authored perception witness"


def perception_package(test, species, arm, *, turn_ticks=25, cancellation=False):
    """Change only fixture geometry/control and the explicitly synthetic victim."""
    source = encounter_sources()
    manifest = {key: copy.deepcopy(source["manifest"][key]) for key in
                ("format", "version", "ruleset", "modules", "factions", "ai_profiles", "themes", "assets")}
    victim = copy.deepcopy(source["species"]["rogue.human"])
    victim["melee"]["strikes"][0]["on_success"] = [{"kind": "damage", "damage": {
        "kind": "components", "components": [{"category": "impact",
            "amount": {"kind": "fixed", "amount": 1}}]}}]
    victim["modifiers"] = [
        # NaturalAttack uses its skill check, not the legacy melee metadata bonus.
        # This explicit synthetic accuracy modifier keeps the real 3d6 check.
        {"scope": "always", "op": {"kind": "add", "stat": {
            "kind": "ability", "ability": "basic_melee", "stat": "check_bonus"}, "value": 100}},
        {"scope": "always", "op": {"kind": "add", "stat": {"kind": "health"}, "value": 500}},
        *[{"scope": "always", "op": {"kind": "add", "stat": {
            "kind": "defense", "defense": defense}, "value": -100}}
          for defense in ("physical", "spiritual")]]
    manifest["creatures"] = {"species": {"fixture.perception_witness": victim}}
    character = copy.deepcopy(source["manifest"]["characters"][0])
    body = copy.deepcopy(source["body_packages"][character["creature"]["body_package"]])
    body["species"] = "fixture.perception_witness"
    manifest["creatures"]["body_packages"] = {"fixture.perception_witness.body": body}
    character["creature"]["body_package"] = "fixture.perception_witness.body"
    character.update(anchor="1/start", turn_ticks=turn_ticks, loadout=[], known_identities=[])
    character["creature"].update(species="fixture.perception_witness", name=WITNESS,
                                 nutrition={"kind": "exempt"}, future_growth={"kind": "disabled"})
    if cancellation:
        character["loadout"] = [{"archetype": "rogue.wand_cancellation", "quantity": 1}]
    manifest.update(id="rogue-perception-" + arm, default_character=1, characters=[character])
    region = {"id": 1, "name": "Authored lit perception encounter", "size": [7, 5, 2],
              "chamber": True, "gravity": [0, 0, -1], "anchors": {"start": [1, 2, 0]},
              "actors": [{"id": 3, "at": [2, 2, 0], "archetype": "rogue." + species,
                          "controller": "external"}]}
    package = test.directory / ("perception-" + arm)
    (package / "regions").mkdir(parents=True)
    (package / "catalog").mkdir()
    for module in manifest["modules"]:
        shutil.copy2(PRODUCTION / module, package / module)
    shutil.copy2(PRODUCTION / "rules.toml", package / "rules.toml")
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    (package / "regions/1.toml").write_text(toml_source(region), encoding="utf-8", newline="\n")
    for relative in (*manifest["modules"], "rules.toml"):
        test.assertEqual((package / relative).read_bytes(), (PRODUCTION / relative).read_bytes())
    result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                            capture_output=True, text=True, encoding="utf-8", timeout=30)
    (test.directory / (arm + "-validation.log")).write_text(
        result.stdout + result.stderr, encoding="utf-8")
    test.assertEqual(result.returncode, 0, result.stderr)
    return package


def tick(frame):
    return u64(observation(frame)["tick"])


def ability(identity, target):
    # Contact targets intentionally use the ordinary opaque actor_target wire
    # variant; only the server may resolve its private target domain.
    return {"type": "use_ability", "id": identity,
            "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}},
            "supplies": {"kind": "carried"}}


def cooldown(frame, identity=GAZE):
    return u64(next(row["cooldown_remaining"] for row in combat(frame)["own_stats"]["abilities"]
                    if row["id"] == identity))


def confused(frame):
    return next((row for row in combat(frame)["own_stats"]["conditions"]
                 if row["condition"]["id"] == CONFUSED), None)


def other_actors(frame):
    view = observation(frame)
    return [row for row in view["visible_actors"] if row["id"] != view["self_target"]]


class RoguePerceptionProcesses(ProcessTestCase):
    def test_tuned_xeroc_continues_hostile_touch_after_recovery_and_reopen(self):
        self.tuned_xeroc_continuation(checkpoint_interval=1)

    def test_tuned_xeroc_hostile_touch_journal_replays_without_source(self):
        self.tuned_xeroc_continuation(checkpoint_interval=4096)

    def tuned_xeroc_continuation(self, *, checkpoint_interval):
        from contextlib import closing
        import sqlite3
        from process_harness import ROOT, headless_fight_completion
        from rogue_roster_package import unrelated_fallback
        self.work_budget = ProcessWorkBudget(requests=128, actor_ticks=2000,
                                             transcript_bytes=32 * 1024 * 1024)
        production = ROOT / "scenarios/rogue"
        source = encounter_sources(production=production)
        manifest = {key: copy.deepcopy(source["manifest"][key]) for key in
                    ("format","version","ruleset","modules","factions","ai_profiles","themes","assets")}
        character = copy.deepcopy(source["manifest"]["characters"][0])
        character["anchor"] = "1/start"
        manifest.update(id="tuned-xeroc-touch",default_character=1,characters=[character])
        region = {"id":1,"name":"Tuned Xeroc touch continuity","size":[7,5,3],"chamber":True,
                  "gravity":[0,0,-1],"anchors":{"start":[2,2,0]},
                  "actors":[{"id":3,"at":[3,2,0],"archetype":"rogue.xeroc",
                             "controller":"ai","ai":"rogue_hostile"}]}
        package = self.directory / "tuned-xeroc"
        (package/"regions").mkdir(parents=True)
        (package/"catalog").mkdir()
        for module in manifest["modules"]:
            shutil.copy2(production/module,package/module)
        shutil.copy2(production/"rules.toml",package/"rules.toml")
        (package/"scenario.toml").write_text(toml_source(manifest),encoding="utf-8")
        (package/"regions/1.toml").write_text(toml_source(region),encoding="utf-8")
        validated = subprocess.run([self.bin/("tor-scenario"+self.suffix),"validate",package],
                                   capture_output=True,text=True,timeout=30)
        self.assertEqual(validated.returncode,0,validated.stdout+validated.stderr)
        server = self.server("--checkpoint-interval",checkpoint_interval,scenario=package,seed=42)
        player, initial = self.client(actor=1)
        self.assertFalse(initial["state"]["wizard_game"])
        self.assertEqual(u64(combat(initial)["hp"]),18)
        # This actual starter already recognizes the disguise. That private fact
        # is not an instruction for the wearer to attack. Never invent a Take ID.
        self.assertTrue(any(r["name"] == "xeroc" and r["position"] == {"x":1,"y":0,"z":0}
                            for r in observation(initial)["visible_actors"]))
        self.assertEqual(observation(initial)["ground_items"],[])
        def completed(action):
            frame = self.act(player,action)
            self.assertIsNone(frame["error"])
            if not headless_fight_completion(frame):
                frame = self.frame(player,headless_fight_completion)
            self.assertFalse(combat(frame)["dead"])
            return frame
        for expected_tick in (100,200):
            peaceful = completed({"type":"wait"})
            self.assertEqual(u64(observation(peaceful)["tick"]),expected_tick)
            self.assertEqual(u64(combat(peaceful)["hp"]),18)
        before_contact = checkpoint(self,player) if checkpoint_interval == 1 else None
        if before_contact is not None:
            self.assertIsNotNone(before_contact["actors"]["3"]["guise"]["appearance"])
            self.assertIsNone(before_contact["actors"]["3"]["behavior"]["engagement"])
        touched = completed({"type":"move","direction":"east"})
        self.assertEqual(u64(observation(touched)["tick"]),300)
        self.assertEqual(u64(combat(touched)["hp"]),18,"fixed first touch strike misses")
        pending = checkpoint(self,player) if checkpoint_interval == 1 else None
        if pending is not None:
            predator = pending["actors"]["3"]
            self.assertEqual(predator["readied_ambush"]["state"],"spent")
            self.assertIsNone(predator["guise"]["appearance"])
            self.assertEqual(predator["behavior"]["engagement"]["target"],1)
            self.assertEqual(predator["pending"]["started"],284)
            self.assertEqual(predator["pending"]["plan"]["ability"],"basic_melee")
            self.assertEqual(predator["pending"]["plan"]["invocation"]["aim"],
                             {"kind":"actor_target","target":1})
            for actor in ("1","3"):
                self.assertEqual(pending["actors"][actor]["location"],before_contact["actors"][actor]["location"])
            self.assertGreater(predator["pending"]["remaining"],0,"ordinary fresh preparation, not free credit")
        before = self.snapshot(player)
        self.flush_save()
        if checkpoint_interval != 1:
            with closing(sqlite3.connect(self.save)) as db:
                checkpoint_row = db.execute("SELECT sequence FROM checkpoint").fetchone()
                # Before the first periodic checkpoint, replay starts from the
                # original archive at sequence zero rather than a checkpoint row.
                sequence = checkpoint_row[0] if checkpoint_row is not None else 0
                self.assertGreater(db.execute("SELECT count(*) FROM journal WHERE sequence > ?",(sequence,)).fetchone()[0],0,
                                   "restore must replay accepted touch work, not only load a latest checkpoint")
        player.stop(save=False)
        server.stop(save=False)
        package.rename(self.directory/"retired-xeroc-source")
        server = self.server("--checkpoint-interval",checkpoint_interval,scenario=unrelated_fallback(self),seed=None)
        player, reopened = self.client(actor=1)
        self.assertDisclosedStateEqual(reopened["state"],before["state"])
        self.assertEqual(reopened["history"],before["history"])
        if pending is not None:
            self.assertEqual(checkpoint(self,player),pending)
        after = completed({"type":"wait"})
        self.assertEqual(u64(observation(after)["tick"]),400)
        # The baseline retaliation control included a player attack and thus
        # consumed different combat rolls. Require a real resolved hit here,
        # not its unrelated exact damage amount.
        self.assertGreater(u64(combat(after)["hp"]),0)
        self.assertLess(u64(combat(after)["hp"]),18)
        self.flush_save()
        with closing(sqlite3.connect(self.save)) as db:
            rows = db.execute("SELECT sequence,frame FROM journal WHERE sequence>0 UNION ALL "
                              "SELECT sequence,frame FROM history WHERE sequence>0 ORDER BY sequence").fetchall()
        payloads = [json.loads(frame[24:]) for _,frame in rows]
        entries = [row["record"]["entry"] for row in payloads if "record" in row]
        (self.directory/"xeroc-continuation-journal.json").write_text(json.dumps(entries,indent=2),encoding="utf-8")
        starts = [entry for entry in entries if entry.get("actor") == 3
                  and entry["content"]["type"] == "intention_started"]
        self.assertEqual([(entry["tick"],entry["content"]["action"]["type"]) for entry in starts],
                         [(0,"wait"),(100,"wait"),(284,"use_ability")])
        attack = starts[-1]["content"]
        self.assertEqual(attack["action"],{"type":"use_ability","id":"basic_melee",
            "invocation":{"aim":{"kind":"actor_target","target":1},"weapon":{"kind":"default"}},
            "supplies":{"kind":"carried"}})
        self.assertTrue(any(end == {"actor":3,"intention":attack["intention"],"kind":"resolved"}
                            for entry in entries for end in entry["intention_ends"]))
        player.stop(save=False)
        server.stop(save=False)

    def test_tuned_medusa_uses_authored_claws_after_source_free_reopen(self):
        self.autonomous_medusa("rogue", blocked=False)

    def test_exploration_medusa_gaze_cooldown_survives_source_free_reopen(self):
        self.autonomous_medusa("rogue-exploration", blocked=False)

    def test_exploration_medusa_does_not_gaze_through_opaque_partition(self):
        self.autonomous_medusa("rogue-exploration", blocked=True)

    def autonomous_medusa(self, variant, *, blocked):
        """Unchanged authored kits, fixed three Waits, no authored target commands."""
        from contextlib import closing
        import sqlite3
        from process_harness import ROOT, headless_fight_completion
        from rogue_roster_package import unrelated_fallback
        self.work_budget = ProcessWorkBudget(requests=128, actor_ticks=1000,
                                             transcript_bytes=32 * 1024 * 1024)
        production = ROOT / "scenarios" / variant
        source = encounter_sources(production=production)
        manifest = {key: copy.deepcopy(source["manifest"][key]) for key in
                    ("format", "version", "ruleset", "modules", "factions", "ai_profiles", "themes", "assets")}
        character = copy.deepcopy(source["manifest"]["characters"][0])
        character["anchor"] = "1/start"
        manifest.update(id="autonomous-medusa", default_character=1, characters=[character])
        region = {"id": 1, "name": "Authored Medusa continuity", "size": [9, 5, 3],
                  "chamber": True, "gravity": [0, 0, -1], "anchors": {"start": [2, 2, 0]},
                  "actors": [{"id": 3, "at": [5, 2, 0], "archetype": "rogue.medusa",
                              "controller": "ai", "ai": "rogue_hostile"}]}
        if blocked:
            region["walls"] = [[4, y, z] for y in range(5) for z in range(3)]
        package = self.directory / "medusa-source"
        (package / "regions").mkdir(parents=True)
        (package / "catalog").mkdir()
        for module in manifest["modules"]:
            shutil.copy2(production / module, package / module)
        shutil.copy2(production / "rules.toml", package / "rules.toml")
        (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8")
        (package / "regions/1.toml").write_text(toml_source(region), encoding="utf-8")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, timeout=30)
        self.assertEqual(validated.returncode, 0, validated.stdout + validated.stderr)
        server = self.server("--checkpoint-interval", 1, scenario=package, seed=42)
        player, initial = self.client(actor=1)
        self.assertFalse(initial["state"]["wizard_game"])
        self.assertEqual(any(row["name"] == "medusa" for row in observation(initial)["visible_actors"]),
                         not blocked)

        def wait_to(expected):
            frame = self.act(player, {"type": "wait"})
            self.assertIsNone(frame["error"])
            if not headless_fight_completion(frame):
                frame = self.frame(player, headless_fight_completion)
            self.assertFalse(combat(frame)["dead"])
            self.assertEqual(tick(frame), expected)
            return frame

        wait_to(100)
        saved = checkpoint(self, player)
        cooldowns = actor_creature(saved, 3)["cooldowns"]
        gaze_cooldowns = [row for row in cooldowns if row["ability"] == GAZE]
        if variant == "rogue-exploration" and not blocked:
            self.assertEqual(len(gaze_cooldowns), 1)
            self.assertGreater(gaze_cooldowns[0]["remaining"], 0)
        else:
            self.assertEqual(gaze_cooldowns, [])
        before = self.snapshot(player)
        player.stop(save=False)
        server.stop(save=False)
        package.rename(self.directory / "retired-medusa-source")
        server = self.server("--checkpoint-interval", 1, scenario=unrelated_fallback(self), seed=None)
        player, reopened = self.client(actor=1)
        self.assertDisclosedStateEqual(reopened["state"], before["state"])
        self.assertEqual(reopened["history"], before["history"])
        self.assertEqual(checkpoint(self, player), saved)
        wait_to(200)
        final = wait_to(300)
        if blocked:
            self.assertFalse(any(row["name"] == "medusa" for row in observation(final)["visible_actors"]))
        self.flush_save()
        with closing(sqlite3.connect(self.save)) as db:
            rows = db.execute("SELECT sequence,frame FROM journal WHERE sequence>0 UNION ALL "
                              "SELECT sequence,frame FROM history WHERE sequence>0 ORDER BY sequence").fetchall()
        payloads = [json.loads(frame[24:]) for _, frame in rows]
        entries = [row["record"]["entry"] for row in payloads if "record" in row]
        (self.directory / "medusa-journal.json").write_text(json.dumps(entries, indent=2), encoding="utf-8")
        starts = [(entry["tick"], entry["content"]["action"]) for entry in entries
                  if entry.get("actor") == 3 and entry["content"]["type"] == "intention_started"]
        self.assertTrue(starts, "autonomous control must actually run")
        if blocked:
            self.assertTrue(all(action["type"] in ("move", "wait") for _, action in starts))
        elif variant == "rogue":
            self.assertEqual([(at, action["type"], action.get("id")) for at, action in starts],
                             [(0, "move", None), (100, "move", None), (200, "use_ability", "basic_melee")])
        else:
            self.assertEqual([(at, action["type"], action.get("id")) for at, action in starts],
                             [(0, "use_ability", GAZE), (200, "move", None)])
            self.assertEqual(starts[0][1]["invocation"]["aim"], {"kind": "actor_target", "target": 1})
        player.stop(save=False)
        server.stop(save=False)

    def setUp(self):
        super().setUp()
        # One non-resetting diagnostic budget across M, P and all three X arms.
        # These are work/output guards, not production performance thresholds.
        # Retain raw protocol snapshots: 500 requests at 256 KiB/request plus
        # 3 MiB shared startup allowance gives a fixed 128 MiB aggregate guard.
        # This is an output allowance, not a claimed maximum frame/request size.
        self.work_budget = ProcessWorkBudget(requests=500, actor_ticks=12000,
                                             transcript_bytes=128 * 1024 * 1024)
        self.arm_reports = []

    def tearDown(self):
        report = self.work_budget.report()
        report["arms"] = self.arm_reports
        (self.directory / "work-budget.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print("Perception process budget: " + json.dumps(report, sort_keys=True), flush=True)
        if not self._failed():
            self.work_budget.check()
        super().tearDown()

    def launch(self, *args, **kwargs):
        self.work_budget.check()
        process = super().launch(*args, **kwargs)
        self.work_budget.output.paths += (process.transcript.path,)
        self.work_budget.check()
        return process

    def client(self, *args, **kwargs):
        kwargs.update(omit_memory=True, compact_background=True)
        client, frame = super().client(*args, **kwargs)
        self.work_budget.track(client, frame)
        return client, frame

    def command(self, client, value):
        self.work_budget.before()
        frame = super().command(client, value)
        self.work_budget.after(client, frame)
        return frame

    def frame(self, client, predicate, seconds=15):
        return self.work_budget.frame_wait(super().frame, client, predicate, seconds=seconds)

    def snapshot(self, client):
        return self.request(client, {"type": "snapshot"})

    def start_encounter(self, species, arm, *, turn_ticks=25, cancellation=False):
        package = perception_package(self, species, arm, turn_ticks=turn_ticks, cancellation=cancellation)
        self.save = self.directory / (arm + ".db")
        extra = ("--checkpoint-interval", 1) if cancellation else ()
        server = self.server(*extra, scenario=package, character=1, seed=42)
        player, initial = self.client(actor=1)
        foe, foe_initial = self.client(actor=3)
        self.assertFalse(initial["state"]["wizard_game"])
        if cancellation:
            self.assertEqual(len(observation(initial)["inventory"]), 1)
            self.assertEqual(observation(initial)["inventory"][0]["class"], "wand")
            self.assertFalse(observation(initial)["inventory"][0]["identified"])
        else:
            self.assertEqual(observation(initial)["inventory"], [])
        self.assertEqual((tick(initial), tick(foe_initial)), (0, 0))
        return server, (player, foe), (initial, foe_initial)

    def advance_until(self, clients, predicate):
        """Advance only the publicly ready actor, using bounded ordinary Waits."""
        for _ in range(64):
            frames = tuple(self.snapshot(client) for client in clients)
            self.assertEqual(tick(frames[0]), tick(frames[1]))
            self.assertFalse(any(combat(frame)["dead"] for frame in frames))
            if predicate(frames):
                return frames
            ready = [index for index, frame in enumerate(frames) if observation(frame)["ready"]]
            self.assertEqual(len(ready), 1, "The fixture must expose exactly one ordinary decision owner")
            self.assertIsNone(self.act(clients[ready[0]], {"type": "wait"})["error"])
        self.fail("Perception mechanism did not reach its boundary in 64 ordinary decisions")

    def advance_to(self, clients, at):
        frames = self.advance_until(clients, lambda frames: tick(frames[0]) >= at)
        self.assertEqual(tick(frames[0]), at, "The fixture's 25-tick witness must expose the exact boundary")
        return frames

    def queue_foe(self, clients, action):
        # act() waits for execution of queued intentions. Use command() for the
        # non-ready foe, then let the actual ready player's Wait admit execution.
        accepted = self.command(clients[1], {"type": "act", "action": action})
        self.assertIsNone(accepted["error"])
        self.assertIsNone(self.act(clients[0], {"type": "wait"})["error"])
        return tuple(self.snapshot(client) for client in clients)

    def victim_target(self, frame):
        return next(row["id"] for row in other_actors(frame) if row["name"] == WITNESS)

    def assert_anonymous_contact(self, frame):
        rows = other_actors(frame)
        self.assertTrue(rows, "An adjacent invisible body must retain anonymous physical contact")
        targets = {row["id"] for row in rows}
        self.assertEqual(len(targets), 1)
        for row in rows:
            self.assertEqual(row["name"], "")
            self.assertEqual(row["description"], "")
            self.assertIsNone(row.get("asset"))
            self.assertIsNone(row.get("appearance_seed"))
        self.assertFalse(any(row["actor"] in targets for row in combat(frame)["actors"]),
                         "Anonymous contact must not disclose faction, alliance or injury")
        return next(iter(targets))

    def assert_mace_guise(self, frame):
        self.assertEqual(other_actors(frame), [])
        ground = observation(frame)["ground_items"]
        self.assertEqual(len(ground), 1)
        row, = ground
        item = row["item"]
        self.assertEqual((item["name"], item["appearance"], item["description"]), ("mace", "mace", ""))
        self.assertEqual((item["class"], item["asset"], item["identified"], u64(item["quantity"])),
                         ("weapon", "item.weapon.mace", True, 1))
        self.assertIsNone(item["known_supply"])
        self.assertFalse(row["reachable"], "Adjacent guise keeps the ordinary same-cell pickup affordance")
        self.assertFalse(row["supply_reachable"])
        return item["id"]

    def assert_xeroc_revealed(self, frame):
        self.assertEqual(observation(frame)["ground_items"], [])
        self.assertEqual(observation(frame)["inventory"], [], "A guise is never a carried item")
        rows = other_actors(frame)
        self.assertTrue(rows)
        self.assertTrue(all(row["name"] == "xeroc" and row.get("asset") == "creature.xeroc" for row in rows))

    def test_production_cancellation_suppresses_active_phantom_and_xeroc_sources(self):
        # Three isolated exact production recipes, not a full roster/balance claim.
        for species in ("ice_monster", "phantom", "xeroc"):
            with self.subTest(species=species):
                server, clients, initial = self.start_encounter(
                    species, "cancel-" + species, turn_ticks=100, cancellation=True)
                # A real paid turn creates an accepted interval-1 checkpoint;
                # Save merely flushes and must not be treated as a checkpoint creator.
                self.assertTrue(observation(initial[0])["ready"])
                waited = self.act(clients[0], {"type": "wait"})
                self.assertIsNone(waited["error"])
                baseline = self.advance_until(clients, lambda frames:
                    observation(frames[0])["ready"] and not frames[0]["intentions"]
                    and combat(frames[0])["preparation_remaining"] is None
                    and u64(combat(frames[0])["recovery_remaining"]) == 0)
                self.assertGreater(tick(baseline[0]), tick(initial[0]))
                initial = baseline  # Every measured premise is refreshed after paid setup.
                foe_before = combat(initial[1])["own_stats"]
                before_ids = {row["id"] for row in foe_before["abilities"]}
                self.assertIn("basic_melee", before_ids)
                if species == "ice_monster":
                    self.assertIn("rogue.freezing_touch", before_ids)
                elif species == "phantom":
                    self.assert_anonymous_contact(initial[0])
                else:
                    self.assert_mace_guise(initial[0])
                    self.assertIn("rogue.object_guise", {row["id"] for row in foe_before["active_talents"]})
                wand, = observation(initial[0])["inventory"]
                offered = next(row for row in observation(initial[0])["interactions"]["inventory"]
                               if row["item"] == wand["id"])
                self.assertEqual(offered["device_activation"], "direction")
                self.assertIsNone(offered["charges"])
                before = checkpoint(self, clients[0])
                self.assertFalse(actor_creature(before, 3)["cancelled"])
                charge_rows = {key: row["charges"] for key, row in before["items"].items()
                               if row.get("charges") is not None}
                self.assertEqual(len(charge_rows), 1)
                physical, charge = next(iter(charge_rows.items()))
                self.assertTrue(3 <= charge <= 7)
                admitted = self.act(clients[0], {"type": "zap", "item": wand["id"],
                                               "aim": {"kind": "direction", "direction": "east"}})
                self.assertIsNone(admitted["error"])
                completed = self.advance_until(clients, lambda frames:
                    observation(frames[0])["ready"] and not frames[0]["intentions"]
                    and combat(frames[0])["preparation_remaining"] is None
                    and u64(combat(frames[0])["recovery_remaining"]) == 0)
                after = checkpoint(self, clients[0])
                self.assertTrue(actor_creature(after, 3)["cancelled"])
                self.assertEqual(after["items"][physical]["charges"], charge - 1)
                self.assertGreater(tick(completed[0]), tick(initial[0]))
                self.assertEqual(after["rules_rng"], before["rules_rng"])
                self.assertEqual(combat(completed[1])["hp"], combat(initial[1])["hp"])
                foe_after = combat(completed[1])["own_stats"]
                after_ids = {row["id"] for row in foe_after["abilities"]}
                self.assertIn("basic_melee", after_ids)
                for field in ("hit_dice", "attributes", "defenses", "active_talents", "dormant_talents"):
                    self.assertEqual(foe_after[field], foe_before[field], field)
                if species == "ice_monster":
                    self.assertNotIn("rogue.freezing_touch", after_ids)
                else:
                    rows = other_actors(completed[0])
                    self.assertTrue(rows)
                    self.assertTrue(all(row["name"] == species for row in rows))
                    if species == "xeroc":
                        self.assertEqual(observation(completed[0])["ground_items"], [])
                        self.assertTrue(all(row.get("asset") == "creature.xeroc" for row in rows))
                retained, = observation(completed[0])["inventory"]
                self.assertEqual(retained["id"], wand["id"])
                self.assertFalse(retained["identified"])
                self.assertFalse(completed[0]["state"]["wizard_game"])
                for process in (*clients, server):
                    process.stop()

    def medusa_contract(self):
        server, clients, initial = self.start_encounter("medusa", "m")
        action = ability(GAZE, self.victim_target(initial[1]))
        preparing = self.queue_foe(clients, action)
        self.assertEqual(tick(preparing[0]), 25)
        self.assertEqual(u64(combat(preparing[1])["preparation_remaining"]), 75)
        self.assertTrue(combat(preparing[1])["preparation_active"])
        self.assertEqual(cooldown(preparing[1]), 675)
        self.assertIsNone(confused(preparing[0]))

        affected = self.advance_to(clients, 100)
        self.assertEqual((confused(affected[0])["phase"], u64(confused(affected[0])["remaining_ticks"])),
                         ("active", 300))
        self.assertEqual(combat(affected[0])["hp"], combat(initial[0])["hp"])
        self.assertIsNone(combat(affected[1])["preparation_remaining"])
        self.assertEqual(u64(combat(affected[1])["recovery_remaining"]), 100)
        self.assertEqual(cooldown(affected[1]), 600)

        recovered = self.advance_to(clients, 200)
        self.assertEqual(u64(combat(recovered[1])["recovery_remaining"]), 0)
        self.assertEqual(cooldown(recovered[1]), 500)
        foe_ready = self.advance_until(clients, lambda frames: observation(frames[1])["ready"])
        self.assertEqual(tick(foe_ready[1]), 200)
        denied = self.command(clients[1], {"type": "act", "action": action})
        self.assertIsNotNone(denied["error"], "Recovery alone must not bypass the actual gaze cooldown")
        self.assertEqual((tick(denied), cooldown(denied)), (200, 500))
        self.assertIsNone(combat(denied)["preparation_remaining"])

        protected = self.advance_to(clients, 400)
        self.assertEqual((confused(protected[0])["phase"], u64(confused(protected[0])["remaining_ticks"])),
                         ("protection", 400))
        cooled = self.advance_to(clients, 700)
        self.assertEqual(cooldown(cooled[1]), 0)
        self.assertEqual((confused(cooled[0])["phase"], u64(confused(cooled[0])["remaining_ticks"])),
                         ("protection", 100))
        expired = self.advance_to(clients, 800)
        self.assertIsNone(confused(expired[0]))
        for process in (*clients, server):
            process.stop()

    def phantom_contract(self):
        server, clients, initial = self.start_encounter("phantom", "p")
        contact = self.assert_anonymous_contact(initial[0])
        started = self.act(clients[0], ability("basic_melee", contact))
        self.assertIsNone(started["error"], "Ordinary basic melee must accept the disclosed contact target")
        attacked = self.advance_until(clients, lambda frames:
            u64(combat(frames[1])["hp"]) < u64(combat(initial[1])["hp"])
            and observation(frames[0])["ready"]
            and combat(frames[0])["preparation_remaining"] is None)
        self.assertEqual(self.assert_anonymous_contact(attacked[0]), contact)
        self.assertFalse(combat(attacked[1])["dead"])
        before_hp = u64(combat(attacked[0])["hp"])
        self.queue_foe(clients, ability("basic_melee", self.victim_target(attacked[1])))
        retaliated = self.advance_until(clients, lambda frames: u64(combat(frames[0])["hp"]) < before_hp)
        self.assertEqual(self.assert_anonymous_contact(retaliated[0]), contact,
                         "Neither receiving nor making an attack removes living Phantom invisibility")
        for process in (*clients, server):
            process.stop()

    def xeroc_contract(self, probe):
        server, clients, initial = self.start_encounter("xeroc", "x-" + probe,
                                                       turn_ticks=25 if probe == "attack" else 100)
        item = self.assert_mace_guise(initial[0])
        if probe == "take":
            # Quantity denials must not spend the otherwise valid physical probe.
            for quantity in ("0", "2"):
                denied = self.act(clients[0], {"type": "take", "item": item, "quantity": quantity})
                self.assertIsNotNone(denied["error"])
                self.assertEqual(tick(denied), 0)
                self.assertEqual(self.assert_mace_guise(denied), item)
            touched = self.act(clients[0], {"type": "take", "item": item, "quantity": None})
            self.assertIsNone(touched["error"])
            self.assert_xeroc_revealed(touched)
            # The already-prepared touch strike resolves at contact and spends
            # the foe's decision on recovery. There is no stale foe prompt at
            # tick 0: the next player publication completes the paid 50-tick
            # probe. Observe its damage, not just a changed guise.
            self.assertEqual((tick(touched), u64(combat(touched)["recovery_remaining"])), (50, 0))
            self.assertLess(u64(combat(touched)["hp"]), u64(combat(initial[0])["hp"]))
            self.assertIsNone(combat(touched)["preparation_remaining"])
            ready = [touched, self.snapshot(clients[1])]
            self.assertTrue(observation(ready[0])["ready"])
        elif probe == "search":
            started = self.act(clients[0], ability(SEARCH, observation(initial[0])["self_target"]))
            self.assertIsNone(started["error"])
            self.assertEqual((tick(started), u64(combat(started)["preparation_remaining"])), (0, 50))
            self.assertEqual(self.assert_mace_guise(started), item)
            # The other actor's 100-tick Wait spans effect completion at 50 and
            # the separate 50 recovery. Its next published decision is at 100.
            ready = self.advance_to(clients, 100)
            self.assertTrue(observation(ready[0])["ready"])
            self.assertIsNone(combat(ready[0])["preparation_remaining"])
            self.assertEqual(u64(combat(ready[0])["recovery_remaining"]), 0)
        else:
            self.assertEqual(probe, "attack")
            started = self.queue_foe(clients, ability("basic_melee", self.victim_target(initial[1])))
            self.assertEqual(tick(started[0]), 25)
            self.assertIsNotNone(combat(started[1])["preparation_remaining"])
            self.assertTrue(combat(started[1])["preparation_active"])
            self.assert_xeroc_revealed(started[0])
            self.assertEqual(combat(started[0])["hp"], combat(initial[0])["hp"],
                             "The first outgoing preparation reveals before its checked damage")
            ready = self.advance_until(clients, lambda frames:
                u64(combat(frames[0])["hp"]) < u64(combat(initial[0])["hp"])
                and combat(frames[1])["preparation_remaining"] is None)
        self.assert_xeroc_revealed(ready[0])
        # Free repeated reads must not restore the globally consumed disguise.
        self.assert_xeroc_revealed(self.snapshot(clients[0]))
        for process in (*clients, server):
            process.stop()

    def run_arm(self, name, contract, *args):
        before = self.work_budget.usage()
        completed = False
        try:
            contract(*args)
            completed = True
        finally:
            # Report the failing arm too, without resetting any shared limit or
            # masking the original failure if output measurement itself fails.
            after = self.work_budget.report()
            report = {"arm": name, "completed": completed,
                      "used": {key: after["used"][key] - before[key] for key in before},
                      "measurement_error": after["measurement_error"]}
            self.arm_reports.append(report)
            print("Perception arm: " + json.dumps(report, sort_keys=True), flush=True)

    def test_production_m_p_x_preserve_perception_and_paid_lifecycles(self):
        self.run_arm("medusa", self.medusa_contract)
        self.run_arm("phantom", self.phantom_contract)
        for probe in ("take", "search", "attack"):
            self.run_arm("xeroc-" + probe, self.xeroc_contract, probe)


if __name__ == "__main__":
    unittest.main()
