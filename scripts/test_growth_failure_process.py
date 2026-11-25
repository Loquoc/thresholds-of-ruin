"""A rejected growth transaction retires its admission without applying combat effects."""
import copy
import json
import shutil
import sqlite3
import subprocess
import tomllib
import unittest
from contextlib import closing

from arena_matrix import toml_source
from process_harness import ProcessTestCase, creature_package
from rogue_roster_package import unrelated_fallback
from status_condition_package import actor_creature, checkpoint
from stream_relay import StreamRelay


def growth_package(test, *, reject_growth):
    """A valid racial creature whose next selected class changes an ability.

    Neither current creature owns a Mage HD. Only earned growth activates
    its modifier, making the currently harmless non-hostile melee invalid.
    The separate hostile finisher is always valid and has no random check.
    """
    name = "growth-refusal" if reject_growth else "growth-control"
    package = creature_package(test, name, abilities=())
    path = package / "scenario.toml"
    manifest = tomllib.loads(path.read_text(encoding="utf-8"))
    manifest["id"] = name
    manifest["factions"] = {"owner": ["foe"], "foe": ["owner"]}
    owner = manifest["characters"][0]["creature"]
    owner.update(faction="owner", hit_dice=[{"source": "racial"}],
                 future_growth={"kind": "selected", "source": {"kind": "class", "class": "mage"}})
    species = manifest["creatures"]["species"]["human"]
    species["attributes"]["speed"] = 0
    species["behavior"] = {"disposition": "passive", "idle": "wait", "memory_ticks": 1000}
    damage = species["melee"]["strikes"][0]["on_success"][0]["damage"]
    damage["components"][0]["amount"] = {"kind": "fixed", "amount": 0}
    manifest["ai_profiles"]["practice"] = {
        "care": False, "equipment": False, "loot": False, "flee_percent": 0}
    path.write_text(toml_source(manifest), encoding="utf-8", newline="\n")

    path = package / "regions/1.toml"
    region = tomllib.loads(path.read_text(encoding="utf-8"))
    region["actors"][0]["turn_ticks"] = 1000
    region["actors"][0]["creature"].update(faction="foe", future_growth={"kind": "disabled"})
    region["items"] = [
        {"id": 20, "at": [2, 1, 0], "carried_by": 2, "archetype": "token"},
        {"id": 21, "at": [1, 1, 0], "carried_by": 1, "archetype": "tablet"},
    ]
    path.write_text(toml_source(region), encoding="utf-8", newline="\n")

    path = package / "rules.toml"
    rules = tomllib.loads(path.read_text(encoding="utf-8"))
    rules["growth"].update(threshold_base=100, threshold_per_tier=0, reward_base=100,
                           reward_per_relative_tier=0, minimum_reward=100, maximum_reward=100)
    melee = next(row for row in rules["abilities"] if row["id"] == "basic_melee")
    melee.update(interaction="non_hostile",
                 strikes={"kind": "natural_melee", "additional_riders": []})
    finisher = copy.deepcopy(melee)
    finisher.update(id="fixture_growth_finisher", name="Growth finisher", basic_attack=False,
                    interaction="hostile_to_target", reach={"kind": "sight", "range": 6},
                    timing={"source": {"kind": "fixed", "prepare": 1, "recover": 1}},
                    strikes={"kind": "authored", "strikes": [{
                        "offset": 0, "on_failure": [], "descriptors": [], "riders": [],
                        "on_success": [{"kind": "damage", "damage": {
                            "kind": "components", "components": [{"category": "impact",
                                "amount": {"kind": "rolled", "dice": {"count": 1, "sides": 2, "bonus": 1000}}}]}}]}]})
    finisher.pop("cost", None)
    finisher.pop("cooldown", None)
    rules["abilities"].append(finisher)
    rules["constants"]["default_abilities"].append(finisher["id"])
    if reject_growth:
        mage = next(row for row in rules["classes"] if row["id"] == "mage")
        mage["modifiers"].append({"scope": "always", "op": {"kind": "add", "stat": {
            "kind": "ability", "ability": "basic_melee", "stat": "flat_bonus"}, "value": 1}})
    path.write_text(toml_source(rules), encoding="utf-8", newline="\n")
    validated = subprocess.run([test.bin / ("tor-scenario" + test.suffix), "validate", package],
                               capture_output=True, text=True, encoding="utf-8", timeout=30)
    test.assertEqual(validated.returncode, 0, validated.stderr)
    return package


class GrowthFailureProcesses(ProcessTestCase):
    def prepared(self, package):
        server = self.server("--checkpoint-interval", 1, scenario=package)
        player, _ = self.client()
        waited = self.act(player, {"type": "wait"})
        self.assertIsNone(waited["error"])

        def ready(frame):
            view = frame["state"]["observation"]
            return view["ready"] and view["tick"] == "100"

        if not ready(waited):
            self.frame(player, ready)
        before = self.request(player, {"type": "snapshot"})
        self.assertTrue(ready(before))
        self.assertFalse(before["state"]["wizard_game"])
        saved = checkpoint(self, player)
        creature = actor_creature(saved)
        self.assertEqual([row["source"] for row in creature["build"]["hd"]], [{"kind": "racial"}])
        self.assertEqual(creature["build"]["future_growth"], {
            "kind": "selected", "source": {"kind": "class", "class": "mage"}})
        self.assertEqual(creature["earned_progress"]["earned_xp"], 0)
        self.assertEqual(saved["actors"]["2"]["ready_at"], 1000)
        self.assertFalse(actor_creature(saved, 2)["dead"])
        self.assertEqual(saved["items"]["20"]["location"], {"Carried": 2})
        target = next(row["id"] for row in before["state"]["observation"]["visible_actors"]
                      if row["name"] == "practice target")
        action = {"type": "use_ability", "id": "fixture_growth_finisher",
                  "invocation": {"aim": {"kind": "actor_target", "target": target}, "weapon": {"kind": "default"}}, "supplies": {"kind": "carried"}}
        return server, player, before, saved, action

    def saved_records(self, player):
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with closing(sqlite3.connect(self.save.resolve().as_uri() + "?mode=ro", uri=True)) as database:
            rows = database.execute(
                "SELECT sequence,frame FROM journal WHERE sequence>0 "
                "UNION ALL SELECT sequence,frame FROM history WHERE sequence>0 "
                "ORDER BY sequence").fetchall()
        return [json.loads(frame[24:])["record"] for _, frame in rows]

    def rollback_update(self, player, since):
        def matches(message):
            body = message.get("update", {}).get("body", {})
            return body.get("type") == "intention" and body.get("failure") == {"rolled_back": "use_ability"}

        for line in player.transcript[since:]:
            if line.startswith("{"):
                message = json.loads(line).get("message") or {}
                if matches(message):
                    return message
        return self.frame(player, lambda row: matches(row.get("message") or {}))["message"]

    def test_text_explains_rollback_and_accepts_next_command(self):
        package = growth_package(self, reject_growth=True)
        _, setup, _, saved, _ = self.prepared(package)
        setup.stop()
        observer, before = self.client(observe=True, omit_memory=True)
        records_before = self.saved_records(observer)
        replies = []

        def record(message):
            if message["type"] in ("ack", "error") or message.get("update", {}).get("body", {}).get("failure"):
                replies.append(message)
                if len(replies) > 8:
                    raise AssertionError("Unbounded Text retry after admitted growth refusal")

        upstream = self.address
        relay = StreamRelay(upstream, on_server_message=record)
        self.addCleanup(relay.close)
        self.address = relay.address
        try:
            text, _ = self.adventure()
        finally:
            self.address = upstream
        replies.clear()
        self.send(text, "fixture_growth_finisher practice target, then wait")
        # The ordinary execution error should finish this response, rather than
        # waiting for the client's ten-second unanswered-request timeout.
        told = text.until(lambda line: line == "> ", seconds=8)
        self.assertNotIn("Time passes", told)
        self.assertNotIn("server did not answer", told)
        self.assertEqual([row["type"] for row in replies], ["ack", "update"])
        admitted, update = replies
        rejected = update["update"]["body"]
        self.assertEqual(rejected["failure"], {"rolled_back": "use_ability"})
        self.assertEqual(rejected["status"]["entry_id"], admitted["receipt"]["entry_id"])
        self.assertEqual(rejected["status"]["phase"], "failed")
        self.assertNotIn("still queued", told)
        self.assertIn("Using the ability failed.", told)
        self.assertIn("choose another action", told)
        for private_detail in ("InvalidInteraction", "basic_melee", "mage", "flat_bonus"):
            self.assertNotIn(private_detail, told)
        pending = self.request(observer, {"type": "snapshot"})
        self.assertDisclosedStateEqual(pending["state"], before["state"])
        self.assertEqual(checkpoint(self, observer), saved,
                         "Text refusal must not change time, RNG, HP, growth, or carried loot")
        self.assertEqual(pending["intentions"], [])
        records = self.saved_records(observer)
        self.assertEqual(records[:-2], records_before)
        failure = records[-1]["entry"]["content"]
        self.assertEqual(failure["type"], "intention_failed")
        self.assertEqual(failure["cause"], "host_rollback")
        self.assertEqual(failure["admission"], admitted["receipt"]["entry_id"])
        self.say(text, "wait")
        after = self.request(observer, {"type": "snapshot"})
        self.assertGreater(int(after["state"]["observation"]["tick"]), 100)

    def test_control_fatal_kill_earns_selected_class_hd_and_drops_carried_item(self):
        package = growth_package(self, reject_growth=False)
        _, player, _, before, action = self.prepared(package)
        self.assertIsNone(self.act(player, action)["error"])
        after = checkpoint(self, player)
        self.assertTrue(actor_creature(after, 2)["dead"])
        owner = actor_creature(after)
        self.assertEqual(owner["earned_progress"]["earned_xp"], 100)
        self.assertEqual(owner["earned_progress"]["spent_xp"], 100)
        self.assertEqual(owner["earned_progress"]["held_xp"], 0)
        self.assertEqual([row["source"] for row in owner["build"]["hd"]], [
            {"kind": "racial"}, {"kind": "class", "id": "mage"}])
        self.assertNotEqual(after["rules_rng"], before["rules_rng"], "the fatal control consumes combat RNG")
        self.assertIn("Ground", after["items"]["20"]["location"])
        self.assertEqual(after["items"]["21"], before["items"]["21"])
        self.assertGreater(len(after["items"]), len(before["items"]), "fatal control creates a corpse")

    def test_rolled_back_growth_preserves_world_and_stays_retired_after_restart(self):
        package = growth_package(self, reject_growth=True)
        server, player, before, saved, action = self.prepared(package)
        before_records = self.saved_records(player)
        start = len(player.transcript)
        admitted = self.command(player, {"type": "act", "action": action})
        self.assertIsNone(admitted["error"])
        self.rollback_sample = self.rollback_update(player, start)
        pending = self.request(player, {"type": "snapshot"})
        self.assertDisclosedStateEqual(pending["state"], before["state"])
        self.assertEqual(checkpoint(self, player), saved,
                         "failed growth rolls back damage, death, XP, HD, loot, time and RNG")
        self.assertEqual(pending["intentions"], [])
        records = self.saved_records(player)
        self.assertEqual(records[:-2], before_records)
        self.assertEqual(records[-1]["entry"]["content"]["cause"], "host_rollback")
        server.stop()
        player.stop()
        shutil.rmtree(package)
        self.server("--checkpoint-interval", 1, scenario=unrelated_fallback(self), seed=None)
        player, restored = self.client()
        self.assertEqual(restored["intentions"], [])
        self.assertDisclosedStateEqual(restored["state"], before["state"])
        self.assertEqual(checkpoint(self, player), saved)
        self.assertEqual(self.saved_records(player), records)
        self.assertIsNone(self.act(player, {"type": "wait"})["error"])


if __name__ == "__main__":
    unittest.main()
