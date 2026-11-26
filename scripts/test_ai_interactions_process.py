"""Knowledge-limited AI decisions through actual autonomous server execution."""
from scalar_wire import u64
import shutil
import json
import sqlite3
from contextlib import closing
import subprocess
import tomllib
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import Mock, patch

from arena_matrix import toml_source
from process_harness import ProcessTestCase, ROOT, WIZARD_TOKEN
from process_harness import creature_package, headless_fight_completion
from rogue_roster_package import roster_package


class AiInteractionProcesses(ProcessTestCase):
    def test_tuned_flytrap_repeats_one_escalating_hold_through_source_free_reopen(self):
        from process_work_budget import ProcessWorkBudget
        from status_condition_package import checkpoint, observation
        from rogue_roster_package import unrelated_fallback

        budget = ProcessWorkBudget(requests=128, actor_ticks=2000,
                                   transcript_bytes=64 * 1024 * 1024)
        package = roster_package(self, "venus_flytrap", autonomous=True, height=3,
                                 production=ROOT / "scenarios/rogue")
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        region["anchors"] = {"start": [2, 2, 0]}
        region["actors"][0]["at"] = [3, 2, 0]
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        self.validate_item_package(package)
        server = self.server("--checkpoint-interval", 1, scenario=package, seed=42)
        player, initial = self.client(actor=1, omit_memory=True, compact_background=True)
        budget.output.paths = tuple(p.transcript.path for p in self.processes)
        budget.track(player, initial)
        # Scope instrumentation to this test instance, including any Observe
        # issued internally by act()/picture_after and asynchronous frames.
        original_command, original_frame = self.command, self.frame

        def guarded_command(client, value):
            budget.before()
            result = original_command(client, value)
            budget.after(client, result)
            return result

        def guarded_frame(client, predicate, seconds=15):
            return budget.frame_wait(original_frame, client, predicate, seconds=seconds)

        for attribute, wrapper in (("command", guarded_command), ("frame", guarded_frame)):
            guard = patch.object(self, attribute, side_effect=wrapper)
            guard.start()
            self.addCleanup(guard.stop)
        root = {"region": 1, "position": {"x": 3, "y": 2, "z": 0}}
        samples = []
        try:
            # Fixed ordinary inputs only. Private saved hold facts assert the
            # outcome; they never select actions, force hits or extend the run.
            for turn in range(12):
                frame = self.completed_idle_wait(player)
                saved = checkpoint(self, player)
                budget.check()
                incoming = saved["actors"]["1"]["holds"]["incoming"]
                outgoing = saved["actors"]["3"]["holds"]["outgoing"]
                self.assertEqual(saved["actors"]["3"]["location"], root)
                samples.append({"tick": saved["tick"], "incoming": incoming,
                                "outgoing": outgoing,
                                "public": observation(frame)["linked_holds"]})
                if turn == 5:
                    self.assertEqual(len(incoming), 1,
                                     "ordinary successful Clutch must precede reopening")
                    self.assertEqual(incoming[0]["kind"], "linked")
                    before = self.request(player, {"type": "snapshot"})
                    player.stop(save=False)
                    server.stop(save=False)
                    package.rename(self.directory / "retired-tuned-flytrap-source")
                    server = self.server("--checkpoint-interval", 1,
                                         scenario=unrelated_fallback(self), seed=None)
                    player, reopened = self.client(actor=1, omit_memory=True,
                                                   compact_background=True)
                    budget.output.paths = tuple(p.transcript.path for p in self.processes)
                    budget.track(player, reopened)
                    self.assertDisclosedStateEqual(reopened["state"], before["state"])
                    self.assertEqual(reopened["history"], before["history"])
                    self.assertEqual(checkpoint(self, player), saved,
                                     "reopen preserves hold identity, work, actors and RNG")
                    budget.check()
            self.assertEqual([s["tick"] for s in samples], list(range(100, 1201, 100)))
            self.assertTrue(all(not s["incoming"] for s in samples[:4]))
            links = []
            for sample in samples[4:]:
                self.assertEqual(len(sample["incoming"]), 1)
                self.assertEqual(sample["incoming"][0]["kind"], "linked")
                link = sample["incoming"][0]["link"]
                self.assertEqual(link["holder"], 3)
                self.assertEqual(sample["outgoing"], [{"id": link["id"], "target": 1}])
                self.assertEqual(len(sample["public"]), 1)
                links.append(link)
            self.assertEqual(len({link["id"] for link in links}), 1,
                             "repeat Clutch cannot replace the existing hold")
            self.assertEqual(len({sample["public"][0]["target"] for sample in samples[4:]}), 1,
                             "the ordinary escape selector stays stable for the same hold")
            self.assertEqual([link["escalation"] for link in links],
                             [0, 0, 1, 1, 2, 2, 3, 3],
                             "each later autonomous strike advances the retained hold once")
            with closing(sqlite3.connect(self.save)) as db:
                records = db.execute("SELECT sequence,frame FROM journal WHERE sequence>0 "
                    "UNION ALL SELECT sequence,frame FROM history WHERE sequence>0 ORDER BY sequence").fetchall()
            payloads = [json.loads(frame[24:]) for _, frame in records]
            entries = [p["record"]["entry"] for p in payloads if "record" in p]
            starts = [e for e in entries if e.get("actor") == 3
                      and e["content"]["type"] == "intention_started"]
            self.assertEqual([e["tick"] for e in starts], list(range(0, 1200, 200)))
            expected = {"type": "use_ability", "id": "clutch",
                "invocation": {"aim": {"kind": "actor_target", "target": 1},
                               "weapon": {"kind": "default"}}, "supplies": {"kind": "carried"}}
            self.assertTrue(all(e["content"]["action"] == expected for e in starts))
            (self.directory / "flytrap-hold-sequence.json").write_text(
                json.dumps({"samples": samples, "starts": starts}, indent=2), encoding="utf-8")
            player.stop(save=False)
            server.stop(save=False)
            budget.check()
        finally:
            (self.directory / "flytrap-work-budget.json").write_text(
                json.dumps(budget.report(), indent=2), encoding="utf-8")

    def _stationary_flytrap_acquisition_case(self, case):
        import copy
        from checkpoint_payload import decode_checkpoint_payload

        package = roster_package(self, "venus_flytrap", autonomous=True, height=2)
        manifest = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
        hero = manifest["characters"][0]
        # Keep the authored Flytrap and production delver build unchanged. The
        # second human uses passive human disposition and a diagnostic Wait AI
        # override, rather than an external controller that stalls scheduling.
        manifest["ai_profiles"]["fixture_wait"] = {
            "idle": "wait", "flee_percent": 0, "care": False,
            "equipment": False, "loot": False, "doors": False}
        (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        region["size"] = [9, 5, 2]
        region["anchors"] = {"start": [6, 2, 0] if case == "near-low-id" else [1, 2, 0]}
        region["actors"][0]["at"] = [5, 2, 0]
        if case == "only-near":
            region["walls"] = [[3, y, z] for y in range(5) for z in range(2)]
        region["actors"].append({
            "id": 4, "at": [1, 2, 0] if case == "near-low-id" else [6, 2, 0],
            "controller": "ai", "ai": "fixture_wait", "turn_ticks": hero["turn_ticks"],
            "body": copy.deepcopy(hero["body"]), "creature": copy.deepcopy(hero["creature"])})
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        self.validate_item_package(package)
        server = self.server("--checkpoint-interval", 1, scenario=package, wizard=True, seed=42)
        observer, frame = self.client(WIZARD_TOKEN, observe=True, actor=3)
        view = frame["state"]["observation"]
        # Public observer-local geometry: do not confuse opaque public IDs with
        # the authoritative actor IDs in accepted execution records.
        endpoints = {(row["position"]["x"], row["position"]["y"], row["position"]["z"])
                     for row in view["visible_actors"] if row["name"] == "Warrior"}
        self.assertEqual(endpoints, {(1, 0, 0), (1, 0, 1)} if case == "only-near"
                         else {(-4, 0, 0), (-4, 0, 1), (1, 0, 0), (1, 0, 1)})
        observer.stop()
        player, _ = self.client()

        def saved_state():
            self.assertIsNone(self.request(player, {"type": "save"})["error"])
            with closing(sqlite3.connect(self.save.resolve().as_uri() + "?mode=ro", uri=True)) as db:
                sequence, payload = db.execute("SELECT sequence,payload FROM checkpoint").fetchone()
                self.assertEqual(db.execute("SELECT count(*) FROM journal WHERE sequence>?", (sequence,)).fetchone()[0], 0)
            value = json.loads(decode_checkpoint_payload(payload))
            game, shared = value["game"], value["shared"]
            return {"tick": game["tick"], "rules_rng": game["rules_rng"],
                    "actors": {actor: shared["actor_records"][index]
                               for actor, index in shared["actors"][game["actors"]].items()},
                    "ai": {actor: shared["ai_records"][index]
                           for actor, index in game["combat"]["ai"].items()}}

        # Sequence zero need not materialize a checkpoint. The authored root
        # is an independent oracle; inspect persisted state only after a turn.
        root = {"region": 1, "position": {"x": 5, "y": 2, "z": 0}}
        # One ordinary PC wait suffices to witness the fresh acquisition at tick
        # zero. No private checkpoint fact chooses an action or extends a run.
        self.completed_idle_wait(player)
        saved = saved_state()
        self.assertEqual(saved["actors"]["3"]["location"], root)
        with closing(sqlite3.connect(self.save)) as db:
            frames = db.execute("SELECT sequence,frame FROM journal WHERE sequence>0 "
                "UNION ALL SELECT sequence,frame FROM history WHERE sequence>0 ORDER BY sequence").fetchall()
        payloads = [json.loads(frame[24:]) for _, frame in frames]
        entries = [row["record"]["entry"] for row in payloads if "record" in row]
        starts = [row for row in entries if row.get("actor") == 3
                  and row.get("content", {}).get("type") == "intention_started"]
        self.assertTrue(starts)
        first = starts[0]
        expected_target = 1 if case == "near-low-id" else 4
        self.assertEqual(first["tick"], 0)
        self.assertEqual(first["content"]["action"], {
            "type": "use_ability", "id": "clutch",
            "invocation": {"aim": {"kind": "actor_target", "target": expected_target},
                           "weapon": {"kind": "default"}}, "supplies": {"kind": "carried"}})
        self.assertEqual(first["content"]["event"]["type"], "ability_started")
        self.assertEqual(first["content"]["event"]["target"], expected_target)
        self.assertEqual(saved["actors"]["3"]["behavior"]["engagement"]["target"], expected_target)
        player.stop()
        server.stop(save=False)
        package.rename(self.directory / "stationary-acquisition-source-removed")
        server = self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", wizard=True, seed=None)
        player, _ = self.client()
        self.assertEqual(saved_state(), saved, "source-free reopen preserves the committed engagement and pending work")
        player.stop()
        server.stop(save=False)

    def test_stationary_flytrap_prefers_adjacent_high_id_over_far_low_id(self):
        self._stationary_flytrap_acquisition_case("far-low-id")

    def test_stationary_flytrap_attacks_adjacent_low_id_control(self):
        self._stationary_flytrap_acquisition_case("near-low-id")

    def test_stationary_flytrap_attacks_only_disclosed_near_foe_control(self):
        self._stationary_flytrap_acquisition_case("only-near")

    def test_authored_explorer_retains_destination_and_replays_without_source(self):
        from checkpoint_payload import decode_checkpoint_payload

        package = roster_package(self, "hobgoblin", autonomous=True, height=2)
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        # Production recipes, species, policies and knowledge. Only placement
        # changes; the ordinary PC waits behind an opaque separating wall.
        region.update(size=[21, 13, 2], anchors={"start": [19, 6, 0]},
            walls=[[17, y, z] for y in range(13) for z in range(2)],
            actors=[
                {"id": 3, "at": [2, 2, 0], "archetype": "rogue.hobgoblin", "controller": "ai", "ai": "rogue_hostile"},
                {"id": 4, "at": [12, 9, 0], "archetype": "rogue.ice_monster", "controller": "ai", "ai": "rogue_hostile"}])
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        self.validate_item_package(package)
        server = self.server("--checkpoint-interval", 1, scenario=package, wizard=True, seed=42)
        for actor in (3, 4):
            observer, initial = self.client(WIZARD_TOKEN, observe=True, actor=actor)
            self.assertFalse(any(row["name"] in ("Warrior", "Mage")
                                 for row in initial["state"]["observation"]["visible_actors"]))
            observer.stop()
        player, _ = self.client()

        def saved_state():
            self.assertIsNone(self.request(player, {"type": "save"})["error"])
            with closing(sqlite3.connect(self.save.resolve().as_uri() + "?mode=ro", uri=True)) as db:
                row = db.execute("SELECT sequence,payload FROM checkpoint").fetchone()
                self.assertIsNotNone(row)
                sequence, payload = row
                self.assertEqual(db.execute("SELECT count(*) FROM journal WHERE sequence>?", (sequence,)).fetchone()[0], 0)
            value = json.loads(decode_checkpoint_payload(payload))
            game, shared = value["game"], value["shared"]
            return {"tick": game["tick"], "rules_rng": game["rules_rng"],
                    "actors": {actor: shared["actor_records"][index]
                               for actor, index in shared["actors"][game["actors"]].items()},
                    "ai": {actor: shared["ai_records"][index]
                           for actor, index in game["combat"]["ai"].items()}}

        # Fixed ordinary waits, not actions selected using private checkpoint
        # facts. Stop conditions only bound the acceptance witness.
        pending = None
        for _ in range(4):
            self.completed_idle_wait(player)
            saved = saved_state()
            destination = saved["ai"]["3"]["exploration_destination"]
            if destination is not None and destination != saved["actors"]["3"]["location"]:
                pending = saved
                break
        self.assertIsNotNone(pending, "real Explore species must retain a noncurrent destination")
        destination = pending["ai"]["3"]["exploration_destination"]
        self.assertIsNone(pending["ai"]["4"]["exploration_destination"])
        ice_location = pending["actors"]["4"]["location"]
        player.stop()
        server.stop(save=False)
        package.rename(self.directory / "retained-exploration-source")
        server = self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", wizard=True, seed=None)
        player, _ = self.client()
        self.assertEqual(saved_state(), pending, "source-free checkpoint preserves the exact AI interest")
        arrived = None
        for _ in range(24):
            self.completed_idle_wait(player)
            saved = saved_state()
            self.assertEqual(saved["actors"]["4"]["location"], ice_location)
            self.assertIsNone(saved["ai"]["4"]["exploration_destination"])
            if (saved["actors"]["3"]["location"] == destination
                    and saved["ai"]["3"]["exploration_destination"] is None):
                arrived = saved
                break
        self.assertIsNotNone(arrived, "the retained point must be reached and retired")
        with closing(sqlite3.connect(self.save)) as db:
            frames = db.execute("SELECT sequence,frame FROM journal WHERE sequence>0 "
                "UNION ALL SELECT sequence,frame FROM history WHERE sequence>0 ORDER BY sequence").fetchall()
        payloads = [json.loads(frame[24:]) for _, frame in frames]
        entries = [row["record"]["entry"] for row in payloads if "record" in row]
        explorer = [row for row in entries if row["actor"] == 3 and "event" in row["content"]]
        destination_event = {"region": destination["region"], **destination["position"]}
        arrival = next(i for i, row in enumerate(explorer)
                       if row["content"]["event"].get("to") == destination_event)
        self.assertGreater(len(explorer), arrival + 1)
        self.assertEqual(explorer[arrival + 1]["content"]["action"], {"type": "wait"})
        self.assertEqual(explorer[arrival + 1]["content"]["event"]["type"], "waited")
        ice = [row for row in entries if row["actor"] == 4 and "event" in row["content"]]
        self.assertTrue(ice)
        self.assertTrue(all(row["content"]["event"]["type"] == "waited" for row in ice))
        expected_observation = self.request(player, {"type": "snapshot"})["state"]["observation"]
        player.stop()
        server.stop(save=False)
        # Preserve this stopped database before obtaining a one-action ordinary
        # continuation oracle. No accepted frame is synthesized or rewritten.
        retained = self.directory / "exploration-replay-prefix.sqlite"
        with closing(sqlite3.connect(self.save)) as source, closing(sqlite3.connect(retained)) as target:
            source.backup(target)
        server = self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", wizard=True, seed=None)
        player, reopened = self.client()
        self.assertEqual(reopened["state"]["observation"], expected_observation)
        self.assertEqual(saved_state(), arrived)
        self.completed_idle_wait(player)
        expected_next = saved_state()
        player.stop()
        server.stop(save=False)
        with closing(sqlite3.connect(retained)) as source, closing(sqlite3.connect(self.save)) as target:
            source.backup(target)
        # Existing full-journal replay selection: original frames move verbatim
        # from history to journal; no checkpoint remains. The save owns sources.
        with closing(sqlite3.connect(self.save)) as db:
            with db:
                frames = db.execute("SELECT sequence,frame FROM journal UNION ALL "
                    "SELECT sequence,frame FROM history ORDER BY sequence").fetchall()
                self.assertGreater(db.execute("SELECT count(*) FROM history").fetchone()[0], 0)
                self.assertEqual(db.execute("SELECT count(*) FROM checkpoint").fetchone()[0], 1)
                db.execute("INSERT INTO journal(sequence,frame) SELECT sequence,frame FROM history")
                db.execute("DELETE FROM history")
                db.execute("DELETE FROM checkpoint")
                self.assertEqual(db.execute("SELECT sequence,frame FROM journal ORDER BY sequence").fetchall(), frames)
        self.server("--checkpoint-interval", 1, scenario=ROOT / "scenarios/two-room", wizard=True, seed=None)
        player, replayed = self.client()
        self.assertEqual(replayed["state"]["observation"], expected_observation)
        self.completed_idle_wait(player)
        self.assertEqual(saved_state(), expected_next,
                         "full replay and the same next ordinary action must reproduce actors and pooled AI")

    def idle_item_package(self, name):
        package = self.directory / name
        shutil.copytree(ROOT / "scenarios/tests/ai-interactions", package)
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        # Repair the idle precondition without changing hostility, memory, or
        # item policy. The mean guard has never seen the hostile player. Both
        # sides remain in one active region, and NPC/item cells stay unchanged.
        region["size"] = [9, 3, 2]
        region["anchors"]["start"] = [7, 1, 0]
        region["walls"] = [[5, y, z] for y in range(3) for z in range(2)]
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        return package

    def validate_item_package(self, package):
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(validated.returncode, 0, validated.stderr)

    def idle_item_observation(self, frame):
        view = frame["state"]["observation"]
        others = [row for row in view["visible_actors"] if row["id"] != view["self_target"]]
        self.assertEqual(others, [],
                         "idle item fixture must keep the hostile player completely unseen")
        return view

    def completed_idle_wait(self, player):
        """One ordinary Wait, through the player's recovery and permission update."""
        def completed(frame):
            self.assertIsNone(frame.get("error"))
            self.assertNotEqual((frame.get("message") or {}).get("type"), "error")
            status = ((frame.get("message") or {}).get("update") or {}).get("body", {})
            phase = (status.get("status") or {}).get("phase")
            self.assertNotIn(phase, ("failed", "cancelled", "paused", "suspended"))
            if frame.get("state_omitted", False):
                return False
            self.assertFalse(frame["state"]["observation"]["combat"]["dead"])
            return headless_fight_completion(frame)

        current = self.act(player, {"type": "wait"})
        if not completed(current):
            current = self.frame(player, completed)
        return current

    def test_gold_seeker_detours_around_a_disclosed_idle_bystander(self):
        package = roster_package(self, "hobgoblin", autonomous=True)
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        # Real production recipes and policies; only encounter geometry/control
        # and a disclosed gold pile differ. The player never enters their sight.
        region.update(size=[11, 5, 2], anchors={"start": [9, 2, 0]},
            walls=[[8, y, z] for y in range(5) for z in range(2)],
            actors=[
                {"id": 3, "at": [1, 2, 0], "archetype": "rogue.leprechaun", "controller": "ai", "ai": "rogue_hostile"},
                {"id": 4, "at": [2, 2, 0], "archetype": "rogue.ice_monster", "controller": "ai", "ai": "rogue_hostile"}],
            items=[{"id": 1001, "at": [6, 2, 0], "archetype": "rogue.gold", "quantity": 25}])
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        self.validate_item_package(package)
        server = self.server("--checkpoint-interval", 4096, scenario=package, wizard=True, seed=46)
        observer, initial = self.client(WIZARD_TOKEN, observe=True, actor=3)
        view = initial["state"]["observation"]
        self.assertTrue(any(row["name"] == "ice monster" for row in view["visible_actors"]))
        self.assertFalse(any(row["name"] == "Warrior" for row in view["visible_actors"]))
        self.assertTrue(any(row["item"]["name"] == "gold coin" for row in view["ground_items"]))
        observer.stop()
        player, _ = self.client()
        for _ in range(10):
            self.completed_idle_wait(player)
        observer, final = self.client(WIZARD_TOKEN, observe=True, actor=3)
        self.assertEqual([(row["name"], row["quantity"]) for row in final["state"]["observation"]["inventory"]], [("gold coin", "25")])
        observer.stop()
        player.stop()
        self.flush_save()
        # Read accepted journal evidence only after the save barrier, never to
        # steer a player. Ignore non-record metadata frames explicitly.
        with closing(sqlite3.connect(self.save)) as db:
            frames = db.execute("SELECT sequence,frame FROM journal WHERE sequence>0 "
                "UNION ALL SELECT sequence,frame FROM history WHERE sequence>0 ORDER BY sequence").fetchall()
        payloads = [json.loads(frame[24:]) for _, frame in frames]
        entries = [row["record"]["entry"] for row in payloads if "record" in row]
        seeker = [entry for entry in entries if entry["actor"] == 3 and "event" in entry["content"]]
        first = seeker[0]["content"]["event"]
        self.assertEqual(first["type"], "moved")
        self.assertEqual(first["to"]["x"], 2)
        self.assertIn(first["to"]["y"], (1, 3))
        taken = next(entry for entry in seeker if entry["content"]["event"]["type"] == "taken")
        # Five steps: two 130-tick diagonals and three 91-tick cardinals.
        self.assertEqual(taken["tick"], 533)
        self.assertEqual(taken["content"]["action"], {"type": "take", "item": 1001, "quantity": 25})
        ice = [entry for entry in entries if entry["actor"] == 4 and "event" in entry["content"]]
        self.assertTrue(ice)
        self.assertTrue(all(entry["content"]["event"]["type"] == "waited" for entry in ice))

        server.stop()
        self.server("--checkpoint-interval", 4096, scenario=package, wizard=True, seed=None)
        restored, reopened = self.client(WIZARD_TOKEN, observe=True, actor=3)
        self.assertEqual([(row["name"], row["quantity"]) for row in reopened["state"]["observation"]["inventory"]], [("gold coin", "25")])
        restored.stop()

    def test_hobgoblin_reserves_known_production_healing_without_drinking(self):
        self.production_medicine_reserve("hobgoblin", "healing", known=True, expected=1)

    def test_yeti_reserves_known_production_extra_healing_without_equipment_policy(self):
        self.production_medicine_reserve("yeti", "extra_healing", known=True, expected=1)

    def test_hobgoblin_does_not_invent_unknown_medicine_knowledge(self):
        self.production_medicine_reserve("hobgoblin", "healing", known=False, expected=0)

    def test_emu_does_not_gain_a_medicine_interest_from_knowledge(self):
        self.production_medicine_reserve("emu", "healing", known=True, expected=0)

    def production_medicine_reserve(self, species, potion, *, known, expected):
        package = roster_package(self, species, autonomous=True)
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        actor = {"id": 3, "at": [2, 2, 0], "archetype": "rogue." + species,
                 "controller": "ai", "ai": "rogue_hostile"}
        # Explicit actor-owned prior knowledge is the fixture input. This is
        # not a claim that scenario AI discovered the potion autonomously.
        if known:
            actor["known_identities"] = ["item.potion." + potion.replace("_", "-")]
        region.update(size=[11, 5, 2], anchors={"start": [9, 2, 0]},
            walls=[[8, y, z] for y in range(5) for z in range(2)],
            actors=[actor, {"id": 4, "at": [5, 1, 0], "archetype": "rogue.ice_monster",
                            "controller": "ai", "ai": "rogue_hostile"}],
            items=[{"id": 1001, "at": [2, 2, 0], "archetype": "rogue." + potion, "quantity": 3}])
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        self.validate_item_package(package)
        server = self.server("--checkpoint-interval", 4096, scenario=package, wizard=True, seed=42)
        observer, initial = self.client(WIZARD_TOKEN, observe=True, actor=3)
        view = initial["state"]["observation"]
        self.assertEqual(view["combat"]["hp"], view["combat"]["max_hp"])
        self.assertEqual(view["combat"]["own_stats"]["conditions"], [])
        self.assertFalse(any(row["name"] in ("Warrior", "Mage") for row in view["visible_actors"]))
        self.assertTrue(any(row["name"] == "ice monster" for row in view["visible_actors"]))
        self.assertEqual(len(view["ground_items"]), 1)
        self.assertEqual(view["ground_items"][0]["item"]["identified"], known)
        appearance = view["ground_items"][0]["item"]["appearance"]
        observer.stop()
        player, _ = self.client()
        for _ in range(16):
            self.completed_idle_wait(player)
        observer, final = self.client(WIZARD_TOKEN, observe=True, actor=3)
        view = final["state"]["observation"]
        inventory = view["inventory"]
        self.assertEqual(view["combat"]["hp"], view["combat"]["max_hp"])
        observer.stop()
        # The stationary bystander still sees the original ground pile. Its
        # own item knowledge must not inherit the seeker's seeded knowledge.
        bystander, ground = self.client(WIZARD_TOKEN, observe=True, actor=4)
        piles = [row["item"] for row in ground["state"]["observation"]["ground_items"]
                 if row["item"]["class"] == "potion"]
        bystander.stop()
        player.stop()
        self.flush_save()
        with closing(sqlite3.connect(self.save)) as db:
            frames = db.execute("SELECT sequence,frame FROM journal WHERE sequence>0 "
                "UNION ALL SELECT sequence,frame FROM history WHERE sequence>0 ORDER BY sequence").fetchall()
        payloads = [json.loads(frame[24:]) for _, frame in frames]
        entries = [row["record"]["entry"] for row in payloads if "record" in row]
        seeker = [row for row in entries if row["actor"] == 3 and "event" in row["content"]]
        taken = [row for row in seeker if row["content"]["event"]["type"] == "taken"]
        self.assertEqual(len(taken), expected, "reserve one known dose, never invent another interest")
        if expected:
            self.assertEqual(seeker[0]["tick"], 0)
            self.assertEqual(seeker[0]["content"]["action"], {"type": "take", "item": 1001, "quantity": 1})
            self.assertEqual([(row["name"], row["quantity"]) for row in inventory],
                             [("potion of " + potion.replace("_", " "), "1")])
            self.assertEqual([row["appearance"] for row in inventory], [appearance])
        else:
            self.assertEqual(inventory, [])
        self.assertTrue(all(row["content"]["event"]["type"] in ("taken", "moved", "waited") for row in seeker),
                        "full-health medicine is retained, not repeatedly drunk")
        self.assertEqual([(row["quantity"], row["identified"]) for row in piles], [(str(3 - expected), False)])
        self.assertEqual([row["appearance"] for row in piles], [appearance],
                         "appearance does not become identity or leak another actor's knowledge")
        ice = [row for row in entries if row["actor"] == 4 and "event" in row["content"]]
        self.assertTrue(ice)
        self.assertTrue(all(row["content"]["event"]["type"] == "waited" for row in ice))
        server.stop()
        self.server("--checkpoint-interval", 4096, scenario=package, wizard=True, seed=None)
        restored, reopened = self.client(WIZARD_TOKEN, observe=True, actor=3)
        self.assertEqual([(row["name"], row["quantity"]) for row in reopened["state"]["observation"]["inventory"]],
                         [(row["name"], row["quantity"]) for row in inventory])
        restored.stop()

    def test_hobgoblin_recovers_split_kit_with_bow_underfoot(self):
        self.split_kit_around_idle_bystander("hobgoblin", bow_home=True)

    def test_centaur_recovers_split_kit_with_ammo_underfoot(self):
        self.split_kit_around_idle_bystander("centaur", bow_home=False)

    def test_orc_finishes_split_kit_then_competing_gold(self):
        self.split_kit_around_idle_bystander("orc", bow_home=True, competing_gold=True)

    def split_kit_around_idle_bystander(self, species, *, bow_home, competing_gold=False):
        # The roster helper's retained encounter list predates Orc enrollment;
        # its copied catalog still supplies the exact production Orc below.
        package = roster_package(self, "hobgoblin" if species == "orc" else species,
                                 autonomous=True, height=2)
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        # Production body, recipe, knowledge, item interests and priorities.
        # Only placement changes: a visible idle actor separates a known pair.
        region.update(size=[11, 5, 2], anchors={"start": [9, 2, 0]},
            walls=[[8, y, z] for y in range(5) for z in range(2)],
            actors=[
                {"id": 3, "at": [1, 2, 0], "archetype": "rogue." + species, "controller": "ai", "ai": "rogue_hostile"},
                {"id": 4, "at": [2, 2, 0], "archetype": "rogue.ice_monster", "controller": "ai", "ai": "rogue_hostile"}],
            items=[
                {"id": 1001, "at": [1, 2, 0] if bow_home else [6, 2, 0], "archetype": "rogue.shortbow"},
                {"id": 1002, "at": [6, 2, 0] if bow_home else [1, 2, 0], "archetype": "rogue.arrow", "quantity": 20}])
        if competing_gold:
            region["items"].append({"id": 1003, "at": [1, 3, 0], "archetype": "rogue.gold", "quantity": 25})
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        self.validate_item_package(package)
        server = self.server("--checkpoint-interval", 4096, scenario=package, wizard=True, seed=46)
        observer, initial = self.client(WIZARD_TOKEN, observe=True, actor=3)
        view = initial["state"]["observation"]
        self.assertTrue(any(row["name"] == "ice monster" for row in view["visible_actors"]))
        self.assertFalse(any(row["name"] in ("Warrior", "Mage") for row in view["visible_actors"]))
        expected_items = {"shortbow", "arrow"} | ({"gold coin"} if competing_gold else set())
        self.assertEqual({row["item"]["name"] for row in view["ground_items"]}, expected_items)
        observer.stop()
        player, _ = self.client()
        for _ in range(24):
            self.completed_idle_wait(player)
        observer, final = self.client(WIZARD_TOKEN, observe=True, actor=3)

        def stock(view):
            bows = [row for row in view["inventory"] if row["name"] == "shortbow"]
            slots = {row["item"]: row["equipped_slot"] for row in view["interactions"]["inventory"]}
            arrows = sum(u64(row["quantity"]) for row in view["inventory"] if row["name"] == "arrow")
            return len(bows), slots.get(bows[0]["id"]) if bows else None, arrows

        def gold(view):
            return sum(u64(row["quantity"]) for row in view["inventory"] if row["name"] == "gold coin")

        final_stock = stock(final["state"]["observation"])
        final_gold = gold(final["state"]["observation"])
        observer.stop()
        player.stop()
        self.flush_save()
        # Accepted execution, not offered actions or diagnostic steering.
        with closing(sqlite3.connect(self.save)) as db:
            frames = db.execute("SELECT sequence,frame FROM journal WHERE sequence>0 "
                "UNION ALL SELECT sequence,frame FROM history WHERE sequence>0 ORDER BY sequence").fetchall()
        payloads = [json.loads(frame[24:]) for _, frame in frames]
        entries = [row["record"]["entry"] for row in payloads if "record" in row]
        seeker = [row for row in entries if row["actor"] == 3 and "event" in row["content"]]
        self.assertEqual(seeker[0]["content"]["action"],
            {"type": "take", "item": 1001 if bow_home else 1002, "quantity": 1 if bow_home else 6})
        self.assertEqual(seeker[0]["tick"], 0)
        self.assertEqual(final_stock, (1, 10, 6))
        taken = [row for row in seeker if row["content"]["event"]["type"] == "taken"]
        self.assertEqual(len(taken), 3 if competing_gold else 2, "one pickup per goal, no collection loop")
        self.assertEqual({row["content"]["action"]["item"] for row in taken},
                         {1001, 1002} | ({1003} if competing_gold else set()))
        self.assertEqual(final_gold, 25 if competing_gold else 0)
        equips = [row for row in seeker if row["content"].get("action", {}).get("type") == "equip"]
        self.assertEqual(len(equips), 1, "no repeated equipment oscillation")
        self.assertEqual(equips[0]["content"]["action"], {"type": "equip", "item": 1001, "slot": 10})
        self.assertTrue(all(row["content"]["event"]["type"] in ("moved", "taken", "item_started", "waited")
                            for row in seeker), "no failed or combat execution in the idle fixture")
        ice = [row for row in entries if row["actor"] == 4 and "event" in row["content"]]
        self.assertTrue(ice)
        self.assertTrue(all(row["content"]["event"]["type"] == "waited" for row in ice))
        server.stop()
        self.server("--checkpoint-interval", 4096, scenario=package, wizard=True, seed=None)
        restored, reopened = self.client(WIZARD_TOKEN, observe=True, actor=3)
        self.assertEqual(stock(reopened["state"]["observation"]), (1, 10, 6))
        self.assertEqual(gold(reopened["state"]["observation"]), 25 if competing_gold else 0)
        restored.stop()

    def test_authored_hobgoblin_acquires_secondary_bow_and_bounded_reserve(self):
        package = roster_package(self, "hobgoblin", autonomous=True)
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        # Only encounter geometry and disclosed ground loot change. The actual
        # production recipe, expertise, abilities and priorities stay intact.
        region["size"] = [10, 5, 2]
        region["anchors"]["start"] = [8, 2, 0]
        region["walls"] = [[5, y, z] for y in range(5) for z in range(2)]
        region["items"] = [
            {"id": 1001, "at": [2, 2, 0], "archetype": "rogue.shortbow"},
            {"id": 1002, "at": [2, 2, 0], "archetype": "rogue.arrow", "quantity": 20},
        ]
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        self.validate_item_package(package)
        server = self.server("--checkpoint-interval", 1, scenario=package, wizard=True, seed=42)
        observer, initial = self.client(WIZARD_TOKEN, observe=True, actor=3)
        self.idle_item_observation(initial)
        player, _ = self.client()

        def stock(view):
            bows = [item for item in view["inventory"] if item["name"] == "shortbow"]
            arrows = sum(u64(item["quantity"]) for item in view["inventory"]
                         if item["name"] == "arrow")
            slots = {item["item"]: item["equipped_slot"]
                     for item in view["interactions"]["inventory"]}
            return len(bows), slots.get(bows[0]["id"]) if bows else None, arrows

        for _ in range(24):
            self.completed_idle_wait(player)
            view = self.idle_item_observation(self.request(observer, {"type": "snapshot"}))
            if stock(view) == (1, 10, 6):
                break
        # Six is the current bounded-reserve tuning, not a universal ammunition
        # rule. Secondary placement preserves the authored natural melee attack.
        self.assertEqual(stock(view), (1, 10, 6))
        for _ in range(4):
            self.completed_idle_wait(player)
        view = self.idle_item_observation(self.request(observer, {"type": "snapshot"}))
        self.assertEqual(stock(view), (1, 10, 6), "no collection or equip oscillation")
        self.assertEqual([row["item"]["quantity"] for row in view["ground_items"]
                          if row["item"]["name"] == "arrow"], ["14"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        observer.stop()
        player.stop()
        server.stop()
        self.server(scenario=package, wizard=True, seed=None)
        restored, reopened = self.client(WIZARD_TOKEN, observe=True, actor=3)
        self.assertEqual(stock(self.idle_item_observation(reopened)), (1, 10, 6))
        self.assertEqual(stock(self.idle_item_observation(
            self.request(restored, {"type": "snapshot"}))), (1, 10, 6))

    def test_creature_ai_chooses_a_weapon_using_a_purchased_rank(self):
        package = creature_package(self, "weapon-training", abilities=("fixture_stamina_strike",))
        manifest = package / "scenario.toml"
        source = manifest.read_text(encoding="utf-8")
        weapons = []
        for name, identity, skill, bonus in [("light_blade", "item.weapon.fixture.light-blade", "light_weaponry", 1),
                                             ("heavy_blade", "item.weapon.fixture.heavy-blade", "heavy_weaponry", 0)]:
            weapons.append(name + ' = { class = "weapon", identity = "' + identity + '", name = "' + name + '", '
                           'equipment = { slot = "weapon", attack = { skill = "' + skill + '", '
                           'bonus = ' + str(bonus) + ', wind_up = 60, recovery = 40, damage = { '
                           'primary = { category = "impact", sides = 6 }, components = [{ '
                           'category = "impact", amount = { type = "rolled", count = 2, '
                           'sides = 6, bonus = 0 } }] } } } }')
        source = source.replace('archetypes = { ', 'archetypes = { ' + ', '.join(weapons) + ', ', 1)
        manifest.write_text(source, encoding="utf-8", newline="\n")
        region = package / "regions/1.toml"
        source = region.read_text(encoding="utf-8").replace(
            'hit_dice = [{ source = "racial" }]',
            'hit_dice = [{ source = "warrior", purchases = [{ pool = "advancement", selection = { kind = "skill_rank", skill = "heavy_weaponry" } }] }]')
        source = ('items = [{ id = 100, at = [2,1,0], archetype = "light_blade", carried_by = 2 }, '
                  '{ id = 101, at = [2,1,0], archetype = "heavy_blade", carried_by = 2 }]\n' + source)
        region.write_text(source, encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                   capture_output=True, text=True, encoding="utf-8", timeout=15)
        self.assertEqual(validated.returncode, 0, validated.stderr)
        self.server(scenario=package, wizard=True)
        player, _ = self.client()
        observer, _ = self.client(WIZARD_TOKEN, observe=True, actor=2)
        chosen = None
        for _ in range(12):
            self.completed_idle_wait(player)
            view = self.request(observer, {"type": "snapshot"})["state"]["observation"]
            names = {item["id"]: item["name"] for item in view["inventory"]}
            equipped = [item for item in view["interactions"]["inventory"]
                        if item["equipped_slot"] is not None]
            if equipped:
                self.assertEqual(len(equipped), 1)
                chosen = equipped[0]
                self.assertEqual(names[chosen["item"]], "heavy_blade")
                break
        self.assertIsNotNone(chosen, "AI must equip the actual upgrade")
        self.assertIsNone(chosen["known_equipment"], "base-gear utility estimates must not disclose unobserved instance numbers")

    def test_ai_takes_one_ground_healing_unit_and_keeps_the_remaining_stack(self):
        package = self.idle_item_package("ground-healing")
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        healing = next(item for item in region["items"] if item["id"] == 20)
        del healing["carried_by"]
        healing["quantity"] = 3
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        self.validate_item_package(package)
        self.server(scenario=package, wizard=True)
        observer, initial = self.client(WIZARD_TOKEN, observe=True, actor=2)
        self.idle_item_observation(initial)
        player, _ = self.client()
        for _ in range(12):
            self.completed_idle_wait(player)
            view = self.idle_item_observation(self.request(observer, {"type": "snapshot"}))
            carried = [item for item in view["inventory"] if item["class"] == "potion"]
            if carried:
                break
        self.assertEqual([item["quantity"] for item in carried], ["1"])
        self.assertEqual([item["item"]["quantity"] for item in view["ground_items"] if item["item"]["class"] == "potion"], ["2"])

    def test_ai_walks_to_safe_known_ground_armor_then_replaces_old_gear(self):
        package = self.idle_item_package("ground-armor")
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        mail = next(item for item in region["items"] if item["id"] == 11)
        del mail["carried_by"]
        mail["at"] = [3, 1, 0]
        next(item for item in region["items"] if item["id"] == 14)["equipped_slot"] = 2
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        self.validate_item_package(package)
        self.server(scenario=package, wizard=True)
        observer, initial = self.client(WIZARD_TOKEN, observe=True, actor=2)
        self.idle_item_observation(initial)
        player, _ = self.client()
        equipped = None
        for _ in range(18):
            self.completed_idle_wait(player)
            view = self.idle_item_observation(self.request(observer, {"type": "snapshot"}))
            mail = next((item for item in view["inventory"] if item["name"] == "mail"), None)
            if mail:
                equipped = next(item["equipped_slot"] for item in view["interactions"]["inventory"] if item["item"] == mail["id"])
                if equipped == 0:
                    break
        self.assertEqual(equipped, 0, "AI must travel, pick up known armor and complete replacement")

    def test_ai_replaces_armor_fills_duplicate_ring_and_restores_equipment(self):
        package = self.idle_item_package("idle-equipment")
        self.validate_item_package(package)
        server = self.server(scenario=package, wizard=True)
        observer, initial = self.client(WIZARD_TOKEN, observe=True, actor=2)
        initial_view = self.idle_item_observation(initial)
        player, _ = self.client()
        items = {item["name"]: item["id"] for item in initial_view["inventory"]}

        def equipment(view):
            return {item["item"]: item["equipped_slot"] for item in view["interactions"]["inventory"]}

        expected = {items["old mail"]: None, items["mail"]: 0,
                    items["old ring"]: 1, items["ring"]: 2, items["silver ring"]: None}
        finished = None
        for _ in range(14):
            self.completed_idle_wait(player)
            view = self.idle_item_observation(self.request(observer, {"type": "snapshot"}))
            actual = equipment(view)
            if all(actual[item] == slot for item, slot in expected.items()):
                finished = view
                break
        self.assertIsNotNone(finished, "AI must complete armor replacement and fill its second ring socket")
        hidden = next(item for item in finished["interactions"]["inventory"] if item["item"] == items["silver ring"])
        self.assertIsNone(hidden["known_equipment"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        observer.stop()
        player.stop()
        server.stop()
        self.server(scenario=package, wizard=True, seed=None)
        restored, reopened = self.client(WIZARD_TOKEN, observe=True, actor=2)
        self.idle_item_observation(reopened)
        view = self.idle_item_observation(self.request(restored, {"type": "snapshot"}))
        self.assertEqual({item: equipment(view)[item] for item in expected}, expected)

    def test_human_knowledge_does_not_make_an_unknown_ai_potion_usable(self):
        package = self.save.parent / "unknown-ai"
        shutil.copytree(ROOT / "scenarios/tests/ai-interactions", package)
        region = package / "regions/1.toml"
        region.write_text(region.read_text().replace('known_identities = ["item.potion.healing"]', 'known_identities = []'))
        manifest = package / "scenario.toml"
        manifest.write_text(manifest.read_text().replace('characters = [{ id = 1,', 'characters = [{ id = 1, known_identities = ["item.potion.healing"],'))
        subprocess.run([self.bin / "tor-scenario", "validate", package], check=True, capture_output=True, text=True)
        self.server(scenario=package, wizard=True)
        observer, _ = self.client(WIZARD_TOKEN, observe=True, actor=2)
        player, initial = self.client()
        target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"] if actor["name"] == "ruin guard")
        self.act(player, {"type": "use_ability", "id": "basic_melee", "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}})
        for _ in range(4):
            self.act(player, {"type": "wait"})
        state = self.request(observer, {"type": "snapshot"})["state"]["observation"]
        self.assertEqual(u64(state["combat"]["hp"]), 15)
        potion = next(item for item in state["inventory"] if item["class"] == "potion")
        self.assertEqual(potion["quantity"], "2")
        self.assertEqual(potion["name"], "red potion")
        self.assertFalse(potion["identified"])

    def test_wounded_ai_uses_its_known_healing_and_restores_consumed_stack(self):
        server = self.server(scenario="ai-interactions", wizard=True)
        observer, _ = self.client(WIZARD_TOKEN, observe=True, actor=2)
        player, initial = self.client()
        target = next(actor["id"] for actor in initial["state"]["observation"]["visible_actors"] if actor["name"] == "ruin guard")
        self.act(player, {"type": "use_ability", "id": "basic_melee", "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}})
        healed = None
        for _ in range(5):
            state = self.request(observer, {"type": "snapshot"})
            potions = [item for item in state["state"]["observation"]["inventory"] if item["class"] == "potion"]
            if potions and potions[0]["quantity"] == "1":
                healed = state
                break
            self.act(player, {"type": "wait"})
        self.assertIsNotNone(healed, "autonomous AI must complete known healing")
        self.assertEqual(u64(healed["state"]["observation"]["combat"]["hp"]), 25)
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        observer.stop()
        player.stop()
        server.stop()
        self.server(scenario="ai-interactions", wizard=True, seed=None)
        restored, _ = self.client(WIZARD_TOKEN, observe=True, actor=2)
        state = self.request(restored, {"type": "snapshot"})
        self.assertEqual(u64(state["state"]["observation"]["combat"]["hp"]), 25)
        self.assertEqual([item["quantity"] for item in state["state"]["observation"]["inventory"] if item["class"] == "potion"], ["1"])


class IdleItemWaitContracts(unittest.TestCase):
    """Exercise the actual stock driver against ordered player publications."""

    @staticmethod
    def player_frame(*, ready=True, admission=True):
        return {"state": {"observation": {"ready": ready,
                    "combat": {"dead": False, "preparation_active": False}}},
                "readiness": {"admission": admission, "resume": None, "cancel": None},
                "intentions": [], "error": None}

    OWNERS = {
        "hobgoblin": ("test_authored_hobgoblin_acquires_secondary_bow_and_bounded_reserve", 24),
        "weapon": ("test_creature_ai_chooses_a_weapon_using_a_purchased_rank", 12),
        "healing": ("test_ai_takes_one_ground_healing_unit_and_keeps_the_remaining_stack", 12),
        "armor": ("test_ai_walks_to_safe_known_ground_armor_then_replaces_old_gear", 18),
        "equipment": ("test_ai_replaces_armor_fills_duplicate_ring_and_restores_equipment", 14),
    }

    def run_stock_driver(self, *, owner="hobgoblin", delayed=None, progress=True, rejected=False):
        method, bound = self.OWNERS[owner]
        case = AiInteractionProcesses(method)
        case.bin, case.suffix = Path("unused-scripted-binaries"), ""
        observer, player, restored, server = (Mock() for _ in range(4))
        completed = 0
        submitted = 0
        pending = []

        def item_frame():
            acquired = rejected or (progress and completed >= 2)
            view = {"self_target": "3", "visible_actors": [], "inventory": [],
                    "interactions": {"inventory": []}, "ground_items": []}
            if owner == "hobgoblin":
                if acquired:
                    view["inventory"] = [
                        {"id": "11", "name": "shortbow", "quantity": "1"},
                        {"id": "12", "name": "arrow", "quantity": "6"}]
                    view["interactions"]["inventory"] = [{"item": "11", "equipped_slot": 10}]
                view["ground_items"] = [{"item": {"name": "arrow", "quantity": "14"}}]
            elif owner == "weapon":
                view["inventory"] = [{"id": "11", "name": "heavy_blade"}]
                view["interactions"]["inventory"] = [{"item": "11",
                    "equipped_slot": 0 if acquired else None, "known_equipment": None}]
            elif owner == "healing":
                if acquired:
                    view["inventory"] = [{"class": "potion", "quantity": "1"}]
                view["ground_items"] = [{"item": {"class": "potion", "quantity": "2"}}]
            elif owner == "armor":
                view["inventory"] = [{"id": "11", "name": "mail"}]
                view["interactions"]["inventory"] = [{"item": "11",
                    "equipped_slot": 0 if acquired else None}]
            else:
                names = ("old mail", "mail", "old ring", "ring", "silver ring")
                slots = (None, 0, 1, 2, None) if acquired else (0, None, 1, None, None)
                view["inventory"] = [{"id": str(i), "name": name} for i, name in enumerate(names)]
                view["interactions"]["inventory"] = [{"item": str(i),
                    "equipped_slot": slot, "known_equipment": None} for i, slot in enumerate(slots)]
            return {"state": {"observation": view}}

        def act(client, action):
            nonlocal submitted, completed
            self.assertIs(client, player)
            self.assertEqual(action, {"type": "wait"})
            submitted += 1
            if rejected or pending:
                return {"error": "StaleRevision: Refresh the observation before acting"}
            if delayed is None:
                completed += 1
                return self.player_frame()
            pending.extend([self.player_frame(**delayed), self.player_frame()])
            return self.player_frame(**delayed)

        def frame(client, predicate, *args, **kwargs):
            nonlocal completed
            self.assertIs(client, player, "only the player's stream establishes completion")
            while pending:
                value = pending.pop(0)
                last = not pending
                if last:
                    completed += 1
                if predicate(value):
                    self.assertTrue(last, "an intermediate admission/readiness is not completion")
                    return value
            self.fail("driver waited past the ordered completion publication")

        def request(client, value):
            if client is player:
                self.assertEqual(value, {"type": "save"})
                if not rejected:
                    self.assertEqual(completed, 6 if owner == "hobgoblin" else 2)
                return {"error": None}
            self.assertIn(client, (observer, restored))
            self.assertEqual(value, {"type": "snapshot"})
            return item_frame()

        case.act = Mock(side_effect=act)
        case.frame = Mock(side_effect=frame)
        case.request = Mock(side_effect=request)
        case.server = Mock(return_value=server)
        observers = iter((observer, restored))
        def client(*args, observe=False, **kwargs):
            current = next(observers) if observe else player
            return current, item_frame() if observe else self.player_frame()
        case.client = Mock(side_effect=client)
        case.validate_item_package = Mock()
        with TemporaryDirectory() as directory:
            package = Path(directory)
            (package / "regions").mkdir()
            (package / "scenario.toml").write_text('archetypes = { }\n', encoding="utf-8")
            (package / "regions/1.toml").write_text(
                'size = [10,5,2]\nitems = [{id=11, carried_by=2}, {id=14}, '
                '{id=20, carried_by=2}]\n[anchors]\nstart = [8,2,0]\n', encoding="utf-8")
            case.idle_item_package = Mock(return_value=package)
            with patch(__name__ + ".roster_package", return_value=package), \
                 patch(__name__ + ".creature_package", return_value=package), \
                 patch(__name__ + ".subprocess.run", return_value=Mock(returncode=0, stderr="")):
                if rejected:
                    with self.assertRaisesRegex(AssertionError, "StaleRevision"):
                        getattr(case, method)()
                elif progress:
                    getattr(case, method)()
                else:
                    with self.assertRaises(AssertionError):
                        getattr(case, method)()
        if rejected:
            self.assertEqual((submitted, completed), (1, 0))
            case.request.assert_not_called()
            case.frame.assert_not_called()
            return
        expected = (6 if owner == "hobgoblin" else 2) if progress else bound
        self.assertEqual((submitted, completed), (expected, expected),
                         "no dropped, replaced or retried Waits; original owner bound stays strict")
        self.assertEqual(pending, [])
        self.assertEqual(case.frame.call_count, 0 if delayed is None else expected)
        if owner != "weapon":
            self.assertEqual(case.validate_item_package.call_count, 1)
        if progress and owner in ("hobgoblin", "equipment"):
            server.stop.assert_called_once()

    def test_adjacent_idle_callers_complete_each_wait_before_observing_items(self):
        for owner in ("weapon", "healing", "armor", "equipment"):
            for delayed in (None, {"ready": False, "admission": True},
                            {"ready": True, "admission": False}):
                with self.subTest(owner=owner, delayed=delayed):
                    self.run_stock_driver(owner=owner, delayed=delayed)

    def test_idle_callers_fail_at_their_original_bounds_without_progress(self):
        for owner in self.OWNERS:
            with self.subTest(owner=owner):
                self.run_stock_driver(owner=owner, progress=False,
                                      delayed={"ready": False, "admission": True})

    def test_idle_callers_do_not_hide_rejected_wait_behind_successful_item_state(self):
        for owner in self.OWNERS:
            with self.subTest(owner=owner):
                self.run_stock_driver(owner=owner, rejected=True)

    def test_wait_errors_and_terminal_work_fail_without_retry(self):
        case = AiInteractionProcesses(
            "test_authored_hobgoblin_acquires_secondary_bow_and_bounded_reserve")
        dead = self.player_frame()
        dead["state"]["observation"]["combat"]["dead"] = True
        failed = self.player_frame()
        failed["message"] = {"update": {"body": {
            "type": "intention", "status": {"phase": "failed"}}}}
        wire_error = self.player_frame()
        wire_error["message"] = {"type": "error", "code": "stale_revision"}
        for returned in ({"error": "StaleRevision"}, dead, failed, wire_error):
            with self.subTest(returned=returned):
                case.act = Mock(return_value=returned)
                case.frame = Mock()
                with self.assertRaises(AssertionError):
                    case.completed_idle_wait("player")
                case.act.assert_called_once_with("player", {"type": "wait"})
                case.frame.assert_not_called()

    def test_already_completed_waits_keep_the_original_stock_and_reopen_checks(self):
        self.run_stock_driver()

    def test_admission_before_recovery_does_not_send_the_next_wait(self):
        self.run_stock_driver(delayed={"ready": False, "admission": True})

    def test_ready_before_admission_does_not_send_the_next_wait(self):
        self.run_stock_driver(delayed={"ready": True, "admission": False})


if __name__ == "__main__":
    unittest.main()
