"""A rejected growth transaction retires its admission without applying combat effects."""
import json
import shutil
import sqlite3
import unittest
from contextlib import closing

from growth_failure_package import growth_package
from process_harness import ProcessTestCase
from rogue_roster_package import unrelated_fallback
from status_condition_package import actor_creature, checkpoint
from stream_relay import StreamRelay


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
