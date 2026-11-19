"""Bounded production A/D mechanism cases, not balance or long-survival evidence.

The creature recipes, owned purchases, rules and selected Warrior kit are copied
unchanged. Only the lit encounter geometry and external foe control differ.
Each case uses one fixed seed and ordinary client commands. Public item/creature
state is checked again after deleting the copied source and reopening its save.
Text/ASCII controls and stream repair remain covered by their existing owners.
"""
import copy
import json
import shutil
import sqlite3
import tomllib
from contextlib import closing
import subprocess
import unittest

from arena_matrix import toml_source
from process_harness import ProcessTestCase, WIZARD_TOKEN
from outcome_scripts import apply
from process_work_budget import ProcessWorkBudget
from rogue_roster_package import PRODUCTION, encounter_sources, unrelated_fallback
from scalar_wire import u64
from status_condition_package import combat, observation
from rogue_test_support import disclosed_frames, resolved_events, decision_boundary, ordered_combat_events


def mechanism_package(test, species, *, size=None, start=None, foe=None):
    source = encounter_sources()
    identity = "rogue." + species
    if identity not in source["actors"]:
        raise ValueError("The mechanism case requires an actual production recipe")
    manifest = {key: copy.deepcopy(source["manifest"][key]) for key in
                ("format", "version", "ruleset", "modules", "factions", "ai_profiles", "themes", "assets")}
    character = copy.deepcopy(source["manifest"]["characters"][0])
    character["anchor"] = "1/start"
    manifest.update(id="rogue-mechanism-" + species, default_character=1, characters=[character])
    region = {"id": 1, "name": "Authored Rogue mechanism encounter", "size": size or [9, 5, 2],
              "chamber": True, "gravity": [0, 0, -1], "anchors": {"start": start or [2, 2, 0]},
              "actors": [{"id": 3, "at": foe or [3, 2, 0], "archetype": identity,
                          "controller": "external"}]}
    package = test.directory / ("mechanism-" + species)
    (package / "regions").mkdir(parents=True)
    (package / "catalog").mkdir()
    for module in manifest["modules"]:
        shutil.copy2(PRODUCTION / module, package / module)
    shutil.copy2(PRODUCTION / "rules.toml", package / "rules.toml")
    # The selected character's death ends the run, as it did before
    # outcomes moved into scripts.
    apply(manifest, package, "death")
    (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
    (package / "regions/1.toml").write_text(toml_source(region), encoding="utf-8", newline="\n")
    result = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                            capture_output=True, text=True, encoding="utf-8", timeout=30)
    (test.directory / (species + "-validation.log")).write_text(
        result.stdout + result.stderr, encoding="utf-8")
    test.assertEqual(result.returncode, 0, result.stderr)
    return package


def armor_item(frame):
    return next(item for item in observation(frame)["inventory"] if item["class"] == "armor")


def armor_reductions(frame):
    values = []
    item = armor_item(frame)["id"]
    interaction = next(row for row in observation(frame)["interactions"]["inventory"]
                       if row["item"] == item)
    for modifier in interaction["known_equipment"]["modifiers"]:
        if modifier["id"] == "reduction":
            values.extend(int(field["value"]["value"]) for field in modifier["fields"]
                          if field["id"] == "amount")
    return sorted(values)


class RogueCorrosionRayProcesses(ProcessTestCase):
    def test_autonomous_dragon_approaches_disclosed_low_wall_and_reopens(self):
        self.work_budget = ProcessWorkBudget(requests=128, actor_ticks=1000,
                                             transcript_bytes=32 * 1024 * 1024)
        package = mechanism_package(self, "dragon", size=[11,7,4],
                                    start=[5,2,0], foe=[1,2,0])
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        region["actors"][0].update(controller="ai", ai="rogue_hostile")
        region["walls"] = [[2,2,0]]
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario"+self.suffix), "validate", package],
                                   capture_output=True, text=True, timeout=30)
        self.assertEqual(validated.returncode, 0, validated.stdout+validated.stderr)
        server = self.server("--checkpoint-interval",4096,scenario=package,wizard=True,seed=42)
        observer, initial_dragon = self.client(WIZARD_TOKEN,observe=True,actor=3)
        view = observation(initial_dragon)
        targets = [row for row in view["visible_actors"] if row["name"] == "Warrior"]
        self.assertEqual([r["position"] for r in targets],
                         [{"x":4,"y":0,"z":0},{"x":4,"y":0,"z":1}])
        self.assertEqual(len({r["id"] for r in targets}), 1)
        for x in range(1,5):
            self.assertTrue(any(c["position"] == {"x":x,"y":0,"z":0}
                                and c["wall"] == (x == 1) for c in view["visible_cells"]),
                            "complete disclosed nominal ground prefix")
        self.assertTrue(any(a["id"] == "rogue.breath_weapon"
                            for a in combat(initial_dragon)["own_stats"]["abilities"]))
        self.assertEqual(u64(combat(initial_dragon)["own_stats"]["hit_dice"]["total"]), 10)
        (self.directory/"low-wall-initial-dragon.json").write_text(json.dumps(initial_dragon,indent=2))
        observer.stop(save=False)
        player, initial_player = self.client(actor=1)
        self.assertEqual(u64(combat(initial_player)["hp"]), 18)
        self.assertTrue(observation(initial_player)["ready"] and initial_player["readiness"]["admission"])
        self.assertIsNone(self.act(player,{"type":"wait"})["error"])
        for _ in range(24):
            after = self.snapshot(player)
            self.assertIsNone(after["error"])
            if (u64(observation(after)["tick"]) > u64(observation(initial_player)["tick"])
                    and (combat(after)["terminal"] or
                         (observation(after)["ready"] and after["readiness"]["admission"]))):
                confirmed = self.snapshot(player)
                self.assertIsNone(confirmed["error"])
                if decision_boundary(confirmed) == decision_boundary(after):
                    after = confirmed
                    break
        else:
            self.fail("Single-actor boundary did not settle within 24 read-only attempts")
        self.flush_save()
        with closing(sqlite3.connect(self.save)) as db:
            records = db.execute("SELECT sequence,frame FROM journal WHERE sequence>0 UNION ALL "
                                 "SELECT sequence,frame FROM history WHERE sequence>0 ORDER BY sequence").fetchall()
        payloads = [json.loads(frame[24:]) for _,frame in records]
        entries = [r["record"]["entry"] for r in payloads if "record" in r]
        (self.directory/"low-wall-journal.json").write_text(json.dumps(entries,indent=2))
        starts = [r for r in entries if r.get("actor") == 3
                  and r["content"]["type"] == "intention_started"]
        self.assertTrue(starts, "ordinary Wait must release a real autonomous choice")
        first = starts[0]["content"]
        self.assertEqual(first["action"]["type"], "move",
                         "known complete-prefix wall must not spend the first breath")
        self.assertEqual(first["event"]["type"], "moved")
        self.assertEqual(first["event"]["from"], {"region":1,"x":1,"y":2,"z":0})
        self.assertNotEqual(first["event"]["to"], first["event"]["from"])
        self.assertNotEqual(first["event"]["to"], {"region":1,"x":2,"y":2,"z":0})
        self.assertTrue(any(end == {"actor":3,"intention":first["intention"],"kind":"resolved"}
                            for entry in entries for end in entry["intention_ends"]))
        # No eventual-hit claim: this bounded witness stops at the first ordinary
        # player boundary and preserves whatever actually happened afterwards.
        before = self.snapshot(player)
        self.flush_save()
        player.stop(save=False)
        server.stop(save=False)
        package.rename(self.directory/"retired-low-wall-dragon-source")
        server = self.server(scenario=unrelated_fallback(self),seed=None)
        player, reopened = self.client(actor=1)
        self.assertDisclosedStateEqual(reopened["state"],before["state"])
        self.assertEqual(reopened["history"],before["history"])
        player.stop(save=False)
        server.stop(save=False)

    def test_autonomous_dragon_uses_disclosed_upper_body_around_ally_and_reopens(self):
        package = mechanism_package(self, "dragon", size=[11,5,4], start=[5,2,0], foe=[1,2,0])
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        region["actors"][0].update(controller="ai", ai="rogue_hostile")
        region["actors"].append({"id":4,"at":[4,2,0],"archetype":"rogue.snake","controller":"external"})
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario"+self.suffix), "validate", package], capture_output=True, text=True, timeout=30)
        self.assertEqual(validated.returncode,0,validated.stdout+validated.stderr)
        server = self.server("--checkpoint-interval",4096,scenario=package,wizard=True,seed=42)
        observer, initial_dragon = self.client(WIZARD_TOKEN,observe=True,actor=3)
        rows = observation(initial_dragon)["visible_actors"]
        target_rows = [row for row in rows if row["name"]=="Warrior"]
        self.assertEqual([row["position"] for row in target_rows],
                         [{"x":4,"y":0,"z":0},{"x":4,"y":0,"z":1}])
        self.assertEqual(len({row["id"] for row in target_rows}),1)
        self.assertTrue(any(row["name"]=="snake" and row["position"]=={"x":3,"y":0,"z":0} for row in rows))
        self.assertTrue(any(row["id"]=="rogue.breath_weapon" for row in combat(initial_dragon)["own_stats"]["abilities"]))
        self.assertEqual(u64(combat(initial_dragon)["own_stats"]["hit_dice"]["total"]),10)
        observer.stop()
        player, initial_player = self.client(actor=1)
        snake, initial_snake = self.client(actor=4)
        clients=(player,snake)
        self.assertEqual(u64(combat(initial_snake)["hp"]),8)
        self.assertEqual(u64(combat(initial_player)["hp"]),18)
        # Two ordinary external Waits release the initial autonomous decision.
        self.advance_until(clients,lambda frames: observation(frames[0])["ready"])
        self.assertIsNone(self.act(player,{"type":"wait"})["error"])
        self.advance_until(clients,lambda frames: observation(frames[1])["ready"])
        self.assertIsNone(self.act(snake,{"type":"wait"})["error"])
        after = self.advance_until(clients,lambda frames: combat(frames[0])["dead"] or observation(frames[0])["ready"],allow_dead=True)
        self.flush_save()
        with closing(sqlite3.connect(self.save)) as db:
            records=db.execute("SELECT sequence,frame FROM journal WHERE sequence>0 UNION ALL SELECT sequence,frame FROM history WHERE sequence>0 ORDER BY sequence").fetchall()
        payloads=[json.loads(frame[24:]) for _,frame in records]
        entries=[row["record"]["entry"] for row in payloads if "record" in row]
        (self.directory/"alternate-dragon-journal.json").write_text(json.dumps(entries,indent=2), encoding="utf-8")
        started=[row["content"]["action"] for row in entries if row.get("actor")==3 and row["content"]["type"]=="intention_started"]
        self.assertTrue(started,"ordinary waits must actually execute an autonomous Dragon choice")
        self.assertEqual(started[0],{"type":"use_ability","id":"rogue.breath_weapon",
            "invocation":{"aim":{"kind":"cell_ray","offset":[4,0,1]},"weapon":{"kind":"default"}},
            "supplies":{"kind":"carried"}},"fresh AI must recover the safe disclosed upper target cell")
        self.assertLess(u64(combat(after[0])["hp"]),u64(combat(initial_player)["hp"]))
        self.assertEqual(combat(after[1])["hp"],combat(initial_snake)["hp"])
        before=tuple(self.snapshot(client) for client in clients)
        self.flush_save()
        for client in clients: client.stop()
        server.stop(save=False)
        package.rename(self.directory/"retired-alternate-dragon-source")
        server=self.server(scenario=unrelated_fallback(self),seed=None)
        for actor,prior in zip((1,4),before):
            client,reopened=self.client(actor=actor)
            self.assertDisclosedStateEqual(reopened["state"],prior["state"])
            self.assertEqual(reopened["history"],prior["history"])
            client.stop()
        server.stop(save=False)

    def test_dragon_wall_approach_and_ally_safe_ray_survive_pending_reopen(self):
        from status_condition_package import checkpoint
        self.work_budget = ProcessWorkBudget(requests=128, actor_ticks=2000,
                                             transcript_bytes=64 * 1024 * 1024)
        package = mechanism_package(self, "dragon", size=[11,7,4],
                                    start=[5,2,0], foe=[1,2,0])
        path = package / "regions/1.toml"
        region = tomllib.loads(path.read_text(encoding="utf-8"))
        region["actors"][0].update(controller="ai", ai="rogue_hostile")
        region["actors"].append({"id":4, "at":[4,2,0], "archetype":"rogue.snake",
                                 "controller":"external"})
        region["walls"] = [[2,2,0]]
        path.write_text(toml_source(region), encoding="utf-8", newline="\n")
        validated = subprocess.run([self.bin / ("tor-scenario"+self.suffix), "validate", package],
                                   capture_output=True, text=True, timeout=30)
        self.assertEqual(validated.returncode, 0, validated.stdout+validated.stderr)
        server = self.server("--checkpoint-interval",1,scenario=package,wizard=True,seed=42)
        observer, initial = self.client(WIZARD_TOKEN,observe=True,actor=3)
        view = observation(initial)
        self.assertEqual([r["position"] for r in view["visible_actors"] if r["name"] == "Warrior"],
                         [{"x":4,"y":0,"z":0},{"x":4,"y":0,"z":1}])
        self.assertTrue(any(r["name"] == "snake" and r["position"] == {"x":3,"y":0,"z":0}
                            for r in view["visible_actors"]))
        for x in range(1,5):
            self.assertTrue(any(c["position"] == {"x":x,"y":0,"z":0}
                                and c["wall"] == (x == 1) for c in view["visible_cells"]))
        observer.stop(save=False)
        player, first_player = self.client(actor=1)
        snake, first_snake = self.client(actor=4)
        clients = (player,snake)
        self.assertEqual(u64(combat(first_player)["hp"]),18)
        self.assertEqual(u64(combat(first_snake)["hp"]),8)
        # Fixed ordinary tape, never selected from private checkpoint contents.
        for index in range(6):
            frames = self.decision_snapshots(clients)
            self.assertEqual([u64(observation(f)["tick"]) for f in frames], [100*(index//2)]*2)
            owner = index % 2
            self.assertTrue(observation(frames[owner])["ready"] and frames[owner]["readiness"]["admission"])
            if index == 2:
                observer, moved = self.client(WIZARD_TOKEN,observe=True,actor=3)
                (self.directory/"cross-flight-moved.json").write_text(json.dumps(moved,indent=2))
                moved_view = observation(moved)
                self.assertEqual(u64(moved_view["tick"]),100)
                self.assertEqual([r["position"] for r in moved_view["visible_actors"] if r["name"] == "Warrior"],
                                 [{"x":3,"y":1,"z":0},{"x":3,"y":1,"z":1}])
                self.assertTrue(any(r["name"] == "snake" and r["position"] == {"x":2,"y":1,"z":0}
                                    for r in moved_view["visible_actors"]))
                for x,y in ((1,0),(2,1),(3,1)):
                    for z in (0,1):
                        self.assertTrue(any(c["position"] == {"x":x,"y":y,"z":z} and not c["wall"]
                                            for c in moved_view["visible_cells"]))
                observer.stop(save=False)
            if index == 4:
                self.assertEqual(u64(combat(frames[0])["hp"]),18)
                saved = checkpoint(self,clients[0])
                (self.directory/"cross-flight-pending.json").write_text(json.dumps(saved,indent=2))
                pending = saved["actors"]["3"]["pending"]
                self.assertEqual(saved["tick"],200)
                self.assertEqual(pending["started"],142)
                self.assertTrue(pending["active"])
                self.assertEqual(pending["plan"]["kind"],"ray")
                self.assertEqual(pending["plan"]["ability"],"rogue.breath_weapon")
                self.assertEqual(pending["plan"]["aim"],{"kind":"cell","offset":[3,1,1]})
                before = self.decision_snapshots(clients)
                for client in clients: client.stop(save=False)
                server.stop(save=False)
                package.rename(self.directory/"retired-cross-flight-source")
                server = self.server("--checkpoint-interval",1,scenario=unrelated_fallback(self),seed=None)
                reopened = [self.client(actor=actor) for actor in (1,4)]
                clients = tuple(client for client,_ in reopened)
                for (_,frame),prior in zip(reopened,before):
                    self.assertDisclosedStateEqual(frame["state"],prior["state"])
                    self.assertEqual(frame["history"],prior["history"])
                self.assertEqual(checkpoint(self,clients[0]),saved)
            self.assertIsNone(self.act(clients[owner],{"type":"wait"})["error"])
        after = self.decision_snapshots(clients)
        self.assertEqual([u64(observation(f)["tick"]) for f in after],[242,242])
        self.assertEqual([u64(combat(f)["hp"]) for f in after],[0,8])
        self.assertTrue(combat(after[0])["dead"])
        self.assertFalse(combat(after[1])["dead"])
        self.flush_save()
        with closing(sqlite3.connect(self.save)) as db:
            records = db.execute("SELECT sequence,frame FROM journal WHERE sequence>0 UNION ALL "
                                 "SELECT sequence,frame FROM history WHERE sequence>0 ORDER BY sequence").fetchall()
        payloads = [json.loads(frame[24:]) for _,frame in records]
        entries = [r["record"]["entry"] for r in payloads if "record" in r]
        (self.directory/"cross-flight-journal.json").write_text(json.dumps(entries,indent=2),encoding="utf-8")
        starts = [r for r in entries if r.get("actor") == 3 and r["content"]["type"] == "intention_started"]
        self.assertEqual([r["tick"] for r in starts],[0,142])
        move,breath = [r["content"] for r in starts]
        self.assertEqual(move["action"]["type"],"move")
        self.assertEqual(move["event"],{"type":"moved","from":{"region":1,"x":1,"y":2,"z":0},
                                      "to":{"region":1,"x":2,"y":1,"z":0}})
        self.assertEqual(breath["action"],{"type":"use_ability","id":"rogue.breath_weapon",
            "invocation":{"aim":{"kind":"cell_ray","offset":[3,1,1]},"weapon":{"kind":"default"}},
            "supplies":{"kind":"carried"}})
        self.assertTrue(any(end == {"actor":3,"intention":breath["intention"],"kind":"resolved"}
                            for entry in entries for end in entry["intention_ends"]))
        for client in clients: client.stop(save=False)
        server.stop(save=False)

    def setUp(self):
        super().setUp()
        self.work_budget = ProcessWorkBudget(requests=600, actor_ticks=20000,
                                             transcript_bytes=128 * 1024 * 1024)

    def tearDown(self):
        report = self.work_budget.report()
        (self.directory / "work-budget.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print("Corrosion/ray process budget: " + json.dumps(report, sort_keys=True), flush=True)
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

    def decision_snapshots(self, clients, attempts=24):
        """Read one held external-input boundary without advancing gameplay."""
        for _ in range(attempts):
            frames = tuple(self.snapshot(client) for client in clients)
            for frame in frames:
                self.assertIsNone(frame["error"])
            if observation(frames[0])["tick"] != observation(frames[1])["tick"]:
                continue
            owners = [index for index, frame in enumerate(frames)
                      if observation(frame)["ready"] and frame["readiness"]["admission"]]
            if len(owners) != 1 and not any(combat(frame)["terminal"] for frame in frames):
                # An admission ACK can precede execution of already paid work.
                # Read again; never queue another action to make it settle.
                continue
            owner = owners[0] if len(owners) == 1 else 0
            # Peers sampled before this owner may predate same-tick execution.
            # Reread those peers so the owner's two samples bracket every peer.
            frames = list(frames)
            for index in range(owner):
                frames[index] = self.snapshot(clients[index])
                self.assertIsNone(frames[index]["error"])
            confirmed = self.snapshot(clients[owner])
            self.assertIsNone(confirmed["error"])
            # Snapshot issuance changes transport/display generations even at
            # rest. All disclosed payload and causal permissions must remain
            # exact for this actor across the intervening peer reads.
            if (decision_boundary(confirmed) == decision_boundary(frames[owner])
                    and all(observation(frame)["tick"] == observation(confirmed)["tick"]
                            for frame in frames)):
                return tuple(confirmed if index == owner else frame
                             for index, frame in enumerate(frames))
        self.fail(f"Mechanism snapshots did not align within {attempts} read attempts")

    def advance_until(self, clients, predicate, decisions=24, allow_dead=False):
        for _ in range(decisions):
            frames = self.decision_snapshots(clients)
            self.assertEqual(observation(frames[0])["tick"], observation(frames[1])["tick"])
            if not allow_dead:
                self.assertFalse(any(combat(frame)["dead"] for frame in frames))
            if predicate(frames):
                return frames
            ready = [index for index, frame in enumerate(frames) if observation(frame)["ready"]]
            self.assertEqual(len(ready), 1)
            self.assertTrue(frames[ready[0]]["readiness"]["admission"])
            self.assertIsNone(self.act(clients[ready[0]], {"type": "wait"})["error"])
        self.fail(f"Mechanism did not settle within {decisions} ordinary decisions")

    def reopen_without_source(self, package, server, clients):
        before = tuple(self.snapshot(client) for client in clients)
        self.flush_save()
        for client in clients:
            client.stop()
        server.stop()
        shutil.rmtree(package)
        server = self.server(scenario=unrelated_fallback(self), seed=None)
        reopened = [self.client(actor=actor) for actor in (1, 3)]
        for (_, after), prior in zip(reopened, before):
            self.assertDisclosedStateEqual(after["state"], prior["state"])
            self.assertEqual(after["history"], prior["history"])
        for client, _ in reopened:
            client.stop()
        server.stop()

    def test_production_aquator_rusts_owned_armor_to_floor_without_changing_defense(self):
        package = mechanism_package(self, "aquator")
        server = self.server("--checkpoint-interval", 4096, scenario=package, seed=42)
        player, initial = self.client(actor=1)
        foe, initial_foe = self.client(actor=3)
        clients = (player, foe)
        self.assertFalse(initial["state"]["wizard_game"])
        self.assertEqual(u64(combat(initial_foe)["own_stats"]["hit_dice"]["total"]), 5)
        self.assertEqual(observation(initial_foe)["inventory"], [])
        item = armor_item(initial)["id"]
        gear = next(row for row in observation(initial)["interactions"]["inventory"]
                    if row["item"] == item)
        self.assertIsNone(gear["known_equipment"])
        self.assertIsNone(gear["known_cursed"])
        self.assertIsNone(gear["known_protected"])
        slot = gear["equipped_slot"]
        # Authored starting equipment is not an exact-instance numeric witness.
        # Ordinary removal and wearing observes this armor without identifying
        # its curse/protection state or changing the production kit.
        for action in ({"type": "unequip", "item": item},
                       {"type": "equip", "item": item, "slot": slot}):
            self.advance_until(clients, lambda frames: observation(frames[0])["ready"])
            self.assertIsNone(self.act(player, action)["error"])
            self.advance_until(clients, lambda frames: not frames[0]["intentions"])
        initial, _ = self.advance_until(clients, lambda frames: observation(frames[1])["ready"])
        self.assertEqual(armor_reductions(initial), [2, 2])
        gear = next(row for row in observation(initial)["interactions"]["inventory"]
                    if row["item"] == item)
        self.assertEqual(gear["equipped_slot"], slot)
        self.assertIsNone(gear["known_cursed"])
        self.assertIsNone(gear["known_protected"])
        hp = combat(initial)["hp"]
        physical = combat(initial)["own_stats"]["defenses"]["physical"]
        target = next(actor["id"] for actor in observation(initial_foe)["visible_actors"]
                      if actor["name"] == "Warrior")
        seen = {2}
        floor_contacts = 0
        for _ in range(32):
            before = self.snapshot(foe)
            start = len(foe.transcript)
            accepted = self.command(foe, {"type": "act", "action": {
                "type": "use_ability", "id": "rogue.corrode",
                "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}},
                "supplies": {"kind": "carried"}}})
            self.assertIsNone(accepted["error"])
            current, _ = self.advance_until(clients, lambda frames: not frames[1]["intentions"])
            self.assertEqual(armor_item(current)["id"], item)
            self.assertEqual(combat(current)["hp"], hp)
            self.assertEqual(combat(current)["own_stats"]["defenses"]["physical"], physical)
            reductions = armor_reductions(current)
            self.assertIn(reductions, ([2, 2], [1, 1], []))
            remaining = reductions[0] if reductions else 0
            seen.add(remaining)
            events = list(resolved_events(disclosed_frames(foe, start), before,
                                          "rogue.corrode", target))
            self.assertTrue(events, "The actual completed contact must be disclosed")
            if remaining == 0 and any(event["outcome"] != "miss" for event in events):
                # Include the success which reaches zero, then require another
                # successful zero-damage contact at the exhausted armor floor.
                floor_contacts += 1
                if floor_contacts == 2:
                    break
        else:
            self.fail("Fixed-seed production contacts did not reach and retain the armor floor")
        self.assertEqual(seen, {0, 1, 2})
        self.reopen_without_source(package, server, clients)


    def test_production_dragon_breath_returns_then_hits_and_reopens(self):
        package = mechanism_package(self, "dragon")
        server = self.server("--checkpoint-interval", 4096, scenario=package, seed=42)
        player, initial = self.client(actor=1)
        dragon, original = self.client(actor=3)
        clients = (player, dragon)
        stats = combat(original)["own_stats"]
        self.assertEqual(u64(stats["hit_dice"]["total"]), 10)
        self.assertEqual(stats["attributes"]["willpower"], 3)
        self.assertEqual(next(row["rank"] for row in stats["skills"]
                              if row["skill"]["id"] == "breath"), 2)
        self.assertFalse(initial["state"]["wizard_game"])
        ability_id = "rogue.breath_weapon"
        def known(frame):
            return next(row for row in combat(frame)["own_stats"]["abilities"]
                        if row["id"] == ability_id)
        self.assertEqual(known(original)["target_kind"], "directional_ray")
        self.advance_until(clients, lambda frames: observation(frames[1])["ready"])
        before = self.snapshot(dragon)
        start = len(dragon.transcript)
        self.assertIsNone(self.command(dragon, {"type":"act", "action":{
            "type":"use_ability", "id":ability_id,
            "invocation":{"aim": {"kind":"directional_ray", "direction":"north"}, "weapon": {"kind": "default"}},
            "supplies":{"kind":"carried"}}})["error"])
        current, returned = self.advance_until(clients, lambda frames: not frames[1]["intentions"])
        receipts = list(resolved_events(disclosed_frames(dragon, start), before,
                                       ability_id, observation(before)["self_target"]))
        self.assertTrue(receipts, "The wall-returning ray must encounter its source after leaving its body")
        # Public outcome alone does not distinguish a dodge from component immunity.
        # The exact failed-check/immunity/interception branch has a ray unit owner.
        self.assertTrue(any(row["outcome"] == "unaffected" for row in receipts),
                        "The disclosed returning encounter leaves the fire-immune source unharmed")
        self.assertEqual(combat(returned)["hp"], combat(original)["hp"])
        self.assertEqual(combat(current)["hp"], combat(initial)["hp"])
        self.assertGreater(u64(known(returned)["cooldown_remaining"]), 0)
        self.advance_until(clients, lambda frames: observation(frames[1])["ready"] and
                           u64(known(frames[1])["cooldown_remaining"]) == 0, decisions=64)
        before = self.snapshot(dragon)
        start = len(dragon.transcript)
        target = next(row["id"] for row in observation(before)["visible_actors"] if row["name"] == "Warrior")
        hp = u64(combat(self.snapshot(player))["hp"])
        self.assertIsNone(self.command(dragon, {"type":"act", "action":{
            "type":"use_ability", "id":ability_id,
            "invocation":{"aim": {"kind":"directional_ray", "direction":"west"}, "weapon": {"kind": "default"}},
            "supplies":{"kind":"carried"}}})["error"])
        hit, after = self.advance_until(clients, lambda frames: not frames[1]["intentions"], allow_dead=True)
        receipts = list(resolved_events(disclosed_frames(dragon, start), before, ability_id, target))
        self.assertTrue(any(row["outcome"] == "applied" for row in receipts),
                        "The fixed-seed production ray must reach a failed Agility dodge")
        self.assertGreater(hp - u64(combat(hit)["hp"]), 0)
        self.assertLessEqual(hp - u64(combat(hit)["hp"]), 36)
        self.assertEqual(combat(after)["hp"], combat(original)["hp"])
        self.reopen_without_source(package, server, clients)

    def test_production_dragon_accepts_off_axis_cell_and_emits_actual_path(self):
        package = mechanism_package(self, "dragon", size=[11, 11, 2],
                                    start=[1, 1, 0], foe=[3, 2, 0])
        server = self.server("--checkpoint-interval", 4096, scenario=package, seed=42)
        player, _ = self.client(actor=1)
        dragon, _ = self.client(actor=3)
        clients = (player, dragon)
        self.advance_until(clients, lambda frames: observation(frames[1])["ready"])
        before = self.snapshot(dragon)
        start = len(dragon.transcript)
        self.assertIsNone(self.command(dragon, {"type": "act", "action": {
            "type": "use_ability", "id": "rogue.breath_weapon",
            "invocation": {"aim": {"kind": "cell_ray", "offset": [4, 2, 0]},
                           "weapon": {"kind": "default"}},
            "supplies": {"kind": "carried"}}})["error"])
        self.advance_until(clients, lambda frames: not frames[1]["intentions"])
        effects = [event for event in ordered_combat_events(disclosed_frames(dragon, start), after=before)
                   if event["type"] == "effect" and event["delivery"] == "ray"]
        self.assertTrue(effects, "Production cell aim must produce authoritative visible flight")
        self.assertEqual(effects[-1]["cue"], "fire")
        points = effects[-1]["segments"][0]
        expected = [{"x": x, "y": y, "z": 0} for x, y in
                    [(0, 0), (1, 1), (2, 1), (3, 2), (4, 2), (5, 3), (6, 3)]]
        self.assertEqual(points, expected)
        # Preserve an actual wire Outcome, not the headless projection or a
        # reconstructed event. Expose it only after source-free reopening passes.
        samples = []
        for frame in disclosed_frames(dragon, start):
            message = frame.get("message") or {}
            body = message.get("update", {}).get("body", {})
            if body.get("type") == "outcome" and any(
                    fact["kind"] == "combat" and fact["event"] == effects[-1]
                    for fact in body["facts"]["facts"]):
                samples.append(message)
        self.assertTrue(samples, "actual cell-ray Outcome must be retained")
        self.reopen_without_source(package, server, clients)
        self.cell_ray_sample = copy.deepcopy(samples[-1])


class RogueMechanismSnapshotTests(unittest.TestCase):
    @staticmethod
    def view(tick, sequence, *, ready=False, admission=True, intentions=(), terminal=False):
        return {"cursor": {"sequence": str(sequence), "tick": str(tick)},
                "state": {"revision": str(sequence), "observation": {
                    "tick": str(tick), "ready": ready,
                    "combat": {"dead": terminal, "terminal": terminal}}},
                "readiness": {"revision": str(sequence), "admission": admission},
                "intentions": list(intentions), "history": [], "branch": "branch",
                "has_control": True, "error": None}

    def test_mixed_ticks_are_reread_before_predicate_without_spending_a_decision(self):
        from unittest.mock import Mock, patch
        driver = RogueCorrosionRayProcesses()
        clients = (object(), object())
        player = self.view(800, 32, ready=True)
        foe = self.view(800, 46)
        # Different attachment-local sequences at one stable tick are legal.
        reads = [self.view(768, 31), self.view(800, 46), player, foe, player]
        predicate = Mock(return_value=True)
        with patch.object(driver, "snapshot", side_effect=reads) as snapshot, \
                patch.object(driver, "act") as act:
            self.assertEqual(driver.advance_until(clients, predicate, decisions=1), (player, foe))
            predicate.assert_called_once_with((player, foe))
            self.assertEqual(snapshot.call_count, 5)
            act.assert_not_called()

    def test_same_actor_progress_invalidates_an_equal_tick_pair(self):
        from unittest.mock import Mock, patch
        driver = RogueCorrosionRayProcesses()
        clients = (object(), object())
        old_owner = self.view(800, 32, ready=True)
        player = self.view(800, 33)
        foe = self.view(800, 47, ready=True)
        reads = [old_owner, self.view(800, 46), player, player, foe, player, foe]
        predicate = Mock(return_value=True)
        with patch.object(driver, "snapshot", side_effect=reads), patch.object(driver, "act") as act:
            self.assertEqual(driver.advance_until(clients, predicate, decisions=1), (player, foe))
            predicate.assert_called_once_with((player, foe))
            act.assert_not_called()

    def test_paid_admission_settles_before_one_wait_at_the_current_owner(self):
        from unittest.mock import Mock, patch
        driver = RogueCorrosionRayProcesses()
        clients = (object(), object())
        queued = self.view(800, 32, ready=True, admission=False,
                           intentions=({"phase": "queued"},))
        player = self.view(800, 33, ready=True)
        foe = self.view(800, 46)
        after = (self.view(900, 35), self.view(900, 48, ready=True))
        reads = [queued, foe, player, foe, player, *after, after[0], after[1]]
        predicate = Mock(side_effect=[False, True])
        with patch.object(driver, "snapshot", side_effect=reads), \
                patch.object(driver, "act", return_value={"error": None}) as act:
            self.assertEqual(driver.advance_until(clients, predicate, decisions=2), after)
            self.assertEqual(predicate.call_count, 2)
            act.assert_called_once_with(clients[0], {"type": "wait"})

    def test_never_aligned_reads_fail_closed_without_predicate_or_gameplay(self):
        from unittest.mock import Mock, patch
        driver = RogueCorrosionRayProcesses()
        clients = (object(), object())
        reads = [self.view(768, 31), self.view(800, 46)] * 24
        predicate = Mock(return_value=True)
        with patch.object(driver, "snapshot", side_effect=reads) as snapshot, \
                patch.object(driver, "act") as act:
            with self.assertRaisesRegex(AssertionError, "did not align within 24 read attempts"):
                driver.advance_until(clients, predicate, decisions=1)
            self.assertEqual(snapshot.call_count, 48)
            predicate.assert_not_called()
            act.assert_not_called()

    def test_terminal_boundary_preserves_allow_dead_without_waiting(self):
        from unittest.mock import Mock, patch
        driver = RogueCorrosionRayProcesses()
        clients = (object(), object())
        frames = (self.view(800, 32, admission=False, terminal=True),
                  self.view(800, 46, admission=False, terminal=True))
        for allow_dead in (False, True):
            with self.subTest(allow_dead=allow_dead), \
                    patch.object(driver, "snapshot", side_effect=[*frames, frames[0]]), \
                    patch.object(driver, "act") as act:
                predicate = Mock(return_value=True)
                if allow_dead:
                    self.assertEqual(driver.advance_until(clients, predicate, decisions=1,
                                                          allow_dead=True), frames)
                    predicate.assert_called_once_with(frames)
                else:
                    with self.assertRaises(AssertionError):
                        driver.advance_until(clients, predicate, decisions=1)
                    predicate.assert_not_called()
                act.assert_not_called()


if __name__ == "__main__":
    unittest.main()
