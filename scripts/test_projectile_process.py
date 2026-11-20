"""Real source pickers, exact previews and projectile payment/recovery.

Default test_*_process discovery registers all methods. The graphical class
places the native keyboard witness in CI's exclusive native lane; only the Text
methods is explicitly non-graphical. No production test API or save mutation.
"""
import copy
import json
import shutil
import sqlite3
import unittest
from unittest.mock import Mock, patch

from process_harness import ProcessTestCase, SPECTATOR_TOKEN, non_graphical, resource, hit_die_counts
from projectile_package import projectile_package
from rogue_roster_package import unrelated_fallback
from stream_relay import StreamRelay


def view(frame):
    return frame["state"]["observation"]


def item(frame, name):
    return next(row for row in view(frame)["inventory"] if row["name"] == name)


def physical_missile_sample(messages, name):
    """Select an untouched wire Outcome and require its exact cosmetic payload."""
    if name not in ('dagger', 'arrow'):
        raise ValueError('This production witness owns only dagger and arrow')
    expected = {'form': 'missile', 'visual': 'neutral', 'style': None,
                'object': {'name': name, 'asset': 'item.weapon.' + name,
                           'kind': {'kind': 'item', 'class': 'weapon'}}}
    samples = []
    for message in messages:
        body = message.get('update', {}).get('body', {})
        if message.get('type') != 'update' or body.get('type') != 'outcome':
            continue
        for fact in body['facts']['facts']:
            event = fact.get('event', {})
            if fact['kind'] == 'combat' and event.get('type') == 'effect' and event.get('delivery') == 'projectile':
                if event.get('presentation') != expected or event.get('cue') != 'physical':
                    raise AssertionError('Physical flight must carry the exact public object, not a generic dart or launcher')
                samples.append(message)
    if not samples:
        raise AssertionError('No actual physical projectile Outcome captured')
    return copy.deepcopy(samples[-1])


def preview_lines(preview):
    rows = ["Fresh action: start cost is paid when preparation starts; resolution cost is reserved."]
    if preview["cost"] is None:
        rows.append("No resource charge.")
    else:
        for resource in ("stamina", "focus", "mana"):
            cost = preview["cost"][resource]
            if cost is not None:
                rows.append(f"Cost {cost['resource']['name']}: start {cost['start']}, resolution {cost['resolution']}")
    rows.append(f"Time to next strike: {preview['preparation']} ticks; recovery: {preview['recovery']} ticks.")
    return rows


class ProjectileProcesses(ProcessTestCase):
    graphical = True

    def setup_encounter(self, *, short_ranges=False):
        self.package = projectile_package(self, short_ranges=short_ranges)
        self.game = self.server("--checkpoint-interval", 1, scenario=self.package, seed=42)
        self.probe, initial = self.client(observe=True)
        self.target, target = self.client(actor=3)
        self.assertFalse(initial["state"]["wizard_game"])
        self.target_health = int(view(target)["combat"]["hp"])
        return initial

    def snapshot(self):
        return self.request(self.probe, {"type": "snapshot"})

    def settle(self):
        # The external target takes ordinary waits to advance the player's
        # finite preparation/recovery. Never mistake admission for completion.
        # 16 * 100 ticks covers this fixture's acquisition and equipment work.
        for _ in range(16):
            current = self.snapshot()
            if view(current)["ready"] and not current["intentions"]:
                return current
            peer = self.request(self.target, {"type": "snapshot"})
            if not view(peer)["ready"]:
                # These connections do not share an atomic snapshot. The player
                # can finish between the first snapshot and the target's reply.
                current = self.snapshot()
                if view(current)["ready"] and not current["intentions"]:
                    return current
            self.assertTrue(view(peer)["ready"], "Fixture target cannot advance pending player work")
            self.assertIsNone(self.act(self.target, {"type": "wait"})["error"])
        self.fail("Projectile fixture did not settle within 1600 ticks of ordinary target waits")

    def capture_client(self, native=False, adventure=False):
        self.replies = []
        relay = StreamRelay(self.address, on_server_message=self.replies.append)
        self.addCleanup(relay.close)
        upstream = self.address
        self.address = relay.address
        try:
            if native:
                # --automation consumes stdin instead of native keyboard input.
                window = self.launch("tor-client-ascii", [
                    "--connect", self.address, "--report-frames"])
                frame = self.ascii_frame(window, lambda f: f["state"] is not None and not f["busy"])
                self.assertTrue(frame["window_open"])
                return window, frame
            return self.adventure() if adventure else self.text_client()
        finally:
            self.address = upstream

    def reply(self, kind, since):
        replies = [row for row in self.replies[since:] if row["type"] == kind]
        self.assertEqual(len(replies), 1, (kind, replies))
        return replies[0]

    def admissions(self, ability):
        # Read-only chronological evidence after a save barrier, never setup.
        self.flush_save()
        with sqlite3.connect(self.save) as db:
            rows = db.execute("SELECT sequence,frame FROM journal WHERE sequence>0 "
                              "UNION ALL SELECT sequence,frame FROM history WHERE sequence>0 "
                              "ORDER BY sequence").fetchall()
        records = [json.loads(frame[24:])["record"]["entry"]["content"] for _, frame in rows]
        return [row for row in records if row["type"] == "intention_admitted"
                and row["action"].get("id") == ability]

    def assert_preview(self, text, reply, ability, delivery):
        preview = reply["preview"]
        self.assertEqual(preview["ability"], ability)
        self.assertEqual(preview["phase"], "fresh")
        self.assertEqual(preview["supplies"], {"kind": "carried"})
        self.assertEqual(preview["invocation"]["aim"]["direction"], "east")
        self.assertEqual(preview["invocation"]["aim"]["delivery"], delivery)
        for line in preview_lines(preview):
            self.assertIn(line, text)
        return preview

    def learn_knife(self, text):
        self.assertEqual(hit_die_counts(view(self.snapshot())["combat"]["own_stats"]["hit_dice"]),
                         {("racial", "racial"): 1, ("class", "warrior"): 1})
        self.assertIn("Starting skills", text.command("advancement"))
        preview = text.command("advancement purchase 2 advancement expertise rogue.knife")
        self.assertIn("expertise rogue.knife", preview)
        self.assertIn("Confirm", preview)
        self.assertNotIn("Server error", text.command("advancement confirm"))
        state = self.settle()
        self.assertIn({"expertise": {"id": "rogue.knife", "name": "Knife"},
                       "status": "active", "source_count": 1},
                      view(state)["combat"]["own_stats"]["expertises"])
        return state

    def text_throw_preview(self, text):
        start = len(self.replies)
        self.assertIn("dagger", text.command("throw"))
        source_reply = self.reply("projectile_choices", start)
        page = source_reply["page"]
        self.assertEqual(page["query"]["phase"], {"kind": "sources"})
        self.assertEqual(len(page["choices"]), 1)
        choice = page["choices"][0]
        self.assertEqual(choice["kind"], "throw")
        self.assertEqual(choice["source"]["socket"], 10)
        self.assertIn("Which direction?", text.command("1"))
        start = len(self.replies)
        output = text.command("east")
        preview = self.assert_preview(output, self.reply("ability_weapon_preview", start),
                                      "rogue.throw", choice)
        self.assertIn("Use this exact weapon action? (yes/no)", output)
        self.assertGreater(sum(cost["start"] + cost["resolution"]
                               for cost in preview["cost"].values() if cost), 0)
        return preview

    @non_graphical
    def test_selected_projectile_range_matches_horizontal_and_vertical_cell_aims(self):
        self.setup_encounter(short_ranges=True)
        owner, _ = self.client()
        spectator, _ = self.client(token=SPECTATOR_TOKEN, observe=True)
        for command in ("overview", "purchase 2 advancement expertise rogue.knife", "confirm"):
            self.assertIsNone(self.command(owner, {"type": "advancement", "command": command})["error"])
        current = self.settle()

        def check(ability, source, delivery, band, expected):
            spectator_view = view(self.request(spectator, {"type": "snapshot"}))
            self.assertNotIn("known_item_uses", spectator_view)
            facts = next(row for row in view(current)["known_item_uses"] if row["item"] == source)
            self.assertEqual(facts[band], {"short_cells": expected - 1, "long_cells": expected})
            visible = {tuple(cell["position"][axis] for axis in "xyz") for cell in view(current)["visible_cells"]}

            def shoot(offset):
                self.assertIn(tuple(offset), visible)
                invocation = {"aim": {"kind": "cell_projectile", "offset": offset, "delivery": delivery},
                              "weapon": {"kind": "default"}}
                return self.act(owner, {"type": "use_ability", "id": ability, "invocation": invocation})

            for too_far in ([expected + 1, 0, 1], [1, 0, expected + 1]):
                result = shoot(too_far)
                self.assertIsNotNone(result["error"], result)
                self.assertDisclosedStateEqual(self.snapshot()["state"], current["state"])
            self.assertIsNone(shoot([1, 0, expected])["error"])
            return self.settle()

        dagger = item(current, "dagger")["id"]
        thrown = {"kind": "throw", "source": {"kind": "equipped", "item": dagger, "socket": 10}}
        current = check("rogue.throw", dagger, thrown, "throw_range", 2)
        self.assertFalse(any(row["id"] == dagger for row in view(current)["inventory"]))
        bow = item(current, "shortbow")["id"]
        self.assertIsNone(self.act(owner, {"type": "equip", "item": bow, "slot": 0})["error"])
        current = self.settle()
        arrows = item(current, "arrow")
        launch = {"kind": "launch", "launcher": {"item": bow, "socket": 0}, "ammunition": arrows["id"]}
        current = check("rogue.launch", bow, launch, "launch_range", 3)
        self.assertEqual(int(item(current, "arrow")["quantity"]), int(arrows["quantity"]) - 1)

    @non_graphical
    def test_headless_projectile_payment_recovery_and_source_free_restart(self):
        self.setup_encounter()
        self.replies = []
        relay = StreamRelay(self.address, on_server_message=self.replies.append)
        self.addCleanup(relay.close)
        upstream, self.address = self.address, relay.address
        try:
            owner, _ = self.client()
        finally:
            self.address = upstream

        def query(command, reply_kind):
            current = self.request(owner, {"type": "snapshot"})
            start = len(self.replies)
            result = self.request(owner, {"type": "command", "context": current["input_context"],
                "branch": current["branch"], "command": {
                    **command, "expected_revision": current["state"]["revision"]}})
            self.assertIsNone(result["error"])
            return self.reply(reply_kind, start)

        def choices(ability, phase):
            return query({"type": "projectile_choices", "query": {
                "ability": ability, "phase": phase, "offset": 0}}, "projectile_choices")["page"]["choices"]

        def fire(ability, delivery, name):
            invocation = {"aim": {"kind": "directional_projectile", "direction": "east", "delivery": delivery},
                          "weapon": {"kind": "default"}}
            before = self.snapshot()
            terms = query({"type": "preview_ability_weapon", "ability": ability,
                           "invocation": invocation, "supplies": {"kind": "carried"}},
                          "ability_weapon_preview")["preview"]
            self.assertEqual(terms["invocation"], invocation)
            self.assertEqual(terms["phase"], "fresh")
            self.assertDisclosedStateEqual(self.snapshot()["state"], before["state"])
            start = len(self.replies)
            admitted = self.act(owner, {"type": "use_ability", "id": ability,
                                       "invocation": invocation, "supplies": {"kind": "carried"}})
            self.assertIsNone(admitted["error"])
            started = self.snapshot()
            focus = terms["cost"]["focus"] if terms["cost"] else None
            if focus:
                self.assertEqual(resource(started, "focus")["balance"],
                                 resource(before, "focus")["balance"] - focus["start"])
                self.assertEqual(resource(started, "focus")["reserved"], focus["resolution"])
            after = self.settle()
            self.assertGreater(int(view(after)["tick"]), int(view(before)["tick"]))
            self.request(owner, {"type": "snapshot"})
            sample = physical_missile_sample(self.replies[start:], name)
            self.assertEqual(len(self.admissions(ability)), 1)
            if ability == "rogue.throw":
                self.assertIsNotNone(focus)
                self.assertGreater(focus["start"] + focus["resolution"], 0)
                self.assertLess(int(view(after)["tick"]), 300)
                self.assertEqual(resource(after, "focus")["balance"],
                                 resource(before, "focus")["balance"] - focus["start"] - focus["resolution"])
                self.assertEqual(resource(after, "focus")["reserved"], 0)
            return after, sample

        self.assertEqual(choices("rogue.throw", {"kind": "sources"}), [])
        for command in ("overview", "purchase 2 advancement expertise rogue.knife", "confirm"):
            self.assertIsNone(self.command(owner, {"type": "advancement", "command": command})["error"])
        learned = self.settle()
        dagger = item(learned, "dagger")["id"]
        source, = choices("rogue.throw", {"kind": "sources"})
        self.assertEqual(source, {"kind": "throw", "source": {"kind": "equipped", "item": dagger, "socket": 10}})
        thrown, dagger_sample = fire("rogue.throw", source, "dagger")
        self.assertFalse(any(row["id"] == dagger for row in view(thrown)["inventory"]))
        self.assertEqual(next(row for row in view(thrown)["ground_items"]
                              if row["item"]["id"] == dagger)["item"]["quantity"], "1")
        self.assertLess(int(view(self.request(self.target, {"type": "snapshot"}))["combat"]["hp"]), self.target_health)
        bow = item(thrown, "shortbow")["id"]
        self.assertIsNone(self.act(owner, {"type": "equip", "item": bow, "slot": 0})["error"])
        equipped = self.settle()
        arrow = item(equipped, "arrow")["id"]
        launcher_choice, = choices("rogue.launch", {"kind": "sources"})
        launcher = launcher_choice["launcher"]
        self.assertEqual(launcher["item"], bow)
        self.assertEqual(choices("rogue.launch", {"kind": "ammunition", "launcher": launcher}),
                         [{"kind": "ammunition", "item": arrow}])
        launched, arrow_sample = fire("rogue.launch", {
            "kind": "launch", "launcher": launcher, "ammunition": arrow}, "arrow")
        self.assertEqual(item(launched, "arrow")["quantity"], "5")
        self.assertEqual(item(launched, "shortbow")["id"], bow)
        self.assertFalse(any(row["item"]["name"] == "arrow" for row in view(launched)["ground_items"]))
        self.assertIsNone(self.command(self.target, {"type": "act", "action": {"type": "move", "direction": "north"}})["error"])
        for action in ({"type": "wait"}, {"type": "move", "direction": "east"},
                       {"type": "take", "item": dagger, "quantity": None}):
            self.assertIsNone(self.act(owner, action)["error"])
            final = self.settle()
        self.assertEqual(item(final, "dagger")["quantity"], "1")
        self.flush_save()
        owner.stop(); self.probe.stop(); self.target.stop(); self.game.stop()
        shutil.rmtree(self.package)
        self.server(scenario=unrelated_fallback(self), seed=None)
        _, restored = self.client()
        self.assertDisclosedStateEqual(restored["state"], final["state"])
        self.assertEqual(restored["history"], final["history"])
        self.assertEqual(restored["intentions"], [])
        self.physical_projectile_samples = {"dagger": dagger_sample, "arrow": arrow_sample}

    @non_graphical
    def test_text_expertise_throw_launch_payment_recovery_and_source_free_restart(self):
        initial = self.setup_encounter()
        text, _ = self.capture_client()
        start = len(self.replies)
        self.assertIn("No choices on this page.", text.command("throw"))
        self.assertEqual(self.reply("projectile_choices", start)["page"]["choices"], [])
        text.command("cancel")
        self.assertDisclosedStateEqual(self.snapshot()["state"], initial["state"])
        learned = self.learn_knife(text)
        dagger = item(learned, "dagger")["id"]
        self.text_throw_preview(text)
        self.assertDisclosedStateEqual(self.snapshot()["state"], learned["state"])
        text.command("no")
        self.assertDisclosedStateEqual(self.snapshot()["state"], learned["state"])
        self.assertEqual(self.admissions("rogue.throw"), [])
        terms = self.text_throw_preview(text)
        before_focus = resource(learned, "focus")["balance"]
        throw_start = len(self.replies)
        self.assertNotIn("Server error", text.command("yes"))
        started = self.snapshot()
        focus = terms["cost"]["focus"]
        self.assertEqual(resource(started, "focus")["balance"], before_focus - focus["start"])
        self.assertEqual(resource(started, "focus")["reserved"], focus["resolution"])
        thrown = self.settle()
        self.assertNotIn("Server error", text.command("sync"))
        dagger_sample = physical_missile_sample(self.replies[throw_start:], 'dagger')
        # Both start and settlement occur before the production 300-tick Focus
        # recovery period, from the initial full pool (the purchase costs 0 ticks).
        self.assertLess(int(view(thrown)["tick"]), 300)
        self.assertEqual(resource(thrown, "focus")["reserved"], 0)
        self.assertEqual(resource(thrown, "focus")["balance"],
                         before_focus - focus["start"] - focus["resolution"])
        self.assertEqual(len(self.admissions("rogue.throw")), 1)
        self.assertFalse(any(row["id"] == dagger for row in view(thrown)["inventory"]))
        landed = next(row for row in view(thrown)["ground_items"] if row["item"]["id"] == dagger)
        self.assertEqual(landed["item"]["quantity"], "1")
        self.assertLess(int(view(self.request(self.target, {"type": "snapshot"}))["combat"]["hp"]), self.target_health)
        self.assertNotIn("Server error", text.command("equip shortbow"))
        equipped = self.settle()
        # Peer waits settle equipment on the server, not on this Text stream.
        # Establish its completed picture before opening a NEW source menu;
        # never retry or rebind an already issued projectile selection.
        start = len(self.replies)
        self.assertNotIn("Server error", text.command("sync"))
        synchronized = self.reply("snapshot", start)["snapshot"]
        self.assertDisclosedStateEqual(synchronized["state"], equipped["state"])
        bow = item(equipped, "shortbow")["id"]
        arrow = item(equipped, "arrow")["id"]
        start = len(self.replies)
        self.assertIn("shortbow", text.command("launch"))
        launchers = self.reply("projectile_choices", start)["page"]
        self.assertEqual(len(launchers["choices"]), 1)
        launcher = launchers["choices"][0]["launcher"]
        self.assertEqual(launcher["item"], bow)
        start = len(self.replies)
        ammunition_text = text.command("1")
        ammunition = self.reply("projectile_choices", start)["page"]
        self.assertEqual(ammunition["query"]["phase"], {"kind": "ammunition", "launcher": launcher})
        self.assertEqual(ammunition["choices"], [{"kind": "ammunition", "item": arrow}])
        self.assertIn("compatible ammunition", ammunition_text)
        self.assertIn("arrow", ammunition_text)
        self.assertIn("Which direction?", text.command("1"))
        start = len(self.replies)
        self.assert_preview(text.command("east"), self.reply("ability_weapon_preview", start),
                            "rogue.launch", {"kind": "launch", "launcher": launcher, "ammunition": arrow})
        self.assertDisclosedStateEqual(self.snapshot()["state"], equipped["state"])
        launch_start = len(self.replies)
        self.assertNotIn("Server error", text.command("yes"))
        launched = self.settle()
        self.assertNotIn("Server error", text.command("sync"))
        arrow_sample = physical_missile_sample(self.replies[launch_start:], 'arrow')
        final = launched
        self.assertEqual(item(final, "arrow")["quantity"], "5")
        self.assertEqual(item(final, "shortbow")["id"], bow)
        self.assertFalse(any(row["item"]["name"] == "arrow" for row in view(final)["ground_items"]))
        self.assertEqual(len(self.admissions("rogue.launch")), 1)
        self.assertEqual(len(self.admissions("rogue.throw")), 1)
        # Vacate the hit cell through ordinary play, then recover the same
        # durable unit. The target is adjacent; a pickup requires its exact cell.
        moved = self.command(self.target, {"type": "act", "action": {"type": "move", "direction": "north"}})
        self.assertIsNone(moved["error"])
        self.assertNotIn("Server error", text.command("wait"))
        self.settle()
        self.assertNotIn("Server error", text.command("east"))
        at_landing = self.settle()
        self.assertTrue(next(row for row in view(at_landing)["ground_items"]
                             if row["item"]["id"] == dagger)["reachable"])
        self.assertIn("You pick up ", text.command(f"take #{dagger}"))
        final = self.settle()
        self.assertEqual(item(final, "dagger")["quantity"], "1")
        self.flush_save()
        text.stop(); self.probe.stop(); self.target.stop(); self.game.stop()
        shutil.rmtree(self.package)
        self.server(scenario=unrelated_fallback(self), seed=None)
        resumed, restored = self.client()
        self.assertDisclosedStateEqual(restored["state"], final["state"])
        self.assertEqual(restored["history"], final["history"])
        self.assertEqual(restored["intentions"], [])
        self.physical_projectile_samples = {'dagger': dagger_sample, 'arrow': arrow_sample}

    @non_graphical
    def test_adventure_projectile_menu_and_exact_preview_gate(self):
        self.setup_encounter()
        setup, _ = self.text_client()
        self.learn_knife(setup)
        setup.stop()
        text, _ = self.capture_client(adventure=True)
        before = self.snapshot()

        def choose():
            start = len(self.replies)
            self.assertIn("dagger", self.say(text, "throw"))
            source = self.reply("projectile_choices", start)["page"]["choices"][0]
            self.assertIn("Which direction?", self.say(text, "1"))
            start = len(self.replies)
            output = self.say(text, "east")
            self.assert_preview(output, self.reply("ability_weapon_preview", start), "rogue.throw", source)

        choose()
        self.say(text, "no")
        self.assertDisclosedStateEqual(self.snapshot()["state"], before["state"])
        self.assertEqual(self.admissions("rogue.throw"), [])
        choose()
        self.say(text, "yes")
        final = self.settle()
        self.assertEqual(len(self.admissions("rogue.throw")), 1)
        self.assertFalse(any(row["name"] == "dagger" for row in view(final)["inventory"]))
        self.assertEqual(sum(int(row["item"]["quantity"]) for row in view(final)["ground_items"]
                             if row["item"]["name"] == "dagger"), 1)

    def test_native_projectile_source_direction_preview_cancel_and_confirm(self):
        self.setup_encounter()
        setup, _ = self.text_client()
        self.learn_knife(setup)
        setup.stop()
        window, initial = self.capture_client(native=True)
        native = self.native_keys(window)

        def press(name, key):
            if name == "Z":
                native("Shift_L", True)
                try:
                    return self.native_press(window, native, "z", key)
                finally:
                    native("Shift_L", False)
            return self.native_press(window, native, name, key)

        def choose_throw():
            menu = press("Z", "abilities")
            index = [row["id"] for row in menu["ability_choices"]].index("rogue.throw")
            for _ in range(index):
                press("Down", "down")
            sources = press("Return", "enter")
            self.assertIn("dagger", "\n".join(sources["projectile_rows"]))
            aiming = press("Return", "enter")
            self.assertEqual(aiming.get("projectile_prompt"), "Throw the dagger. Choose a direction.")
            start = len(self.replies)
            preview = press("Right", "right")
            self.assertEqual(preview.get("projectile_prompt"), "Throw the dagger east? Enter confirms. Esc cancels.")
            server = self.reply("ability_weapon_preview", start)
            self.assertEqual(preview["weapon_preview"]["preview"], server["preview"])
            for line in preview_lines(server["preview"]):
                self.assertIn(line, "\n".join(preview["weapon_preview_rows"]))
            return preview

        preview = choose_throw()
        self.assertEqual(preview["state"], initial["state"])
        cancelled = press("Escape", "escape")
        self.assertIsNone(cancelled["weapon_preview"])
        self.assertEqual(cancelled["state"], initial["state"])
        self.assertEqual(self.admissions("rogue.throw"), [])
        choose_throw()
        committed = press("Return", "enter")
        self.assertTrue(committed["window_open"])
        self.assertEqual(len(self.admissions("rogue.throw")), 1)
        final = self.settle()
        self.assertFalse(any(row["name"] == "dagger" for row in view(final)["inventory"]))
        self.assertEqual(sum(int(row["item"]["quantity"]) for row in view(final)["ground_items"]
                             if row["item"]["name"] == "dagger"), 1)

        # Prepare the launcher with the ordinary headless equipment action,
        # then exercise the separate native launcher → ammunition path.
        window.stop()
        owner, owned = self.client()
        self.assertIsNone(self.act(owner, {"type": "equip", "item": item(owned, "shortbow")["id"], "slot": 0})["error"])
        self.settle()
        owner.stop()
        window, initial = self.capture_client(native=True)
        native = self.native_keys(window)
        sources = press("f", "fire")
        self.assertIn("shortbow", "\n".join(sources["projectile_rows"]))
        empty = press("Tab", "tab")
        self.assertIn("> No choices on this page.", empty["projectile_rows"])
        dismissed = press("Return", "enter")
        self.assertIsNone(dismissed["projectile_selection"])
        self.assertEqual(dismissed["state"], initial["state"])
        press("f", "fire")
        start = len(self.replies)
        ammunition = press("Return", "enter")
        page = self.reply("projectile_choices", start)["page"]
        self.assertEqual(page["query"]["phase"]["kind"], "ammunition")
        self.assertEqual(page["choices"], [{"kind": "ammunition", "item": item(initial, "arrow")["id"]}])
        self.assertEqual(ammunition["projectile_selection"]["page"], page)
        self.assertEqual(ammunition["projectile_rows"], ["> arrow"])
        aiming = press("Return", "enter")
        self.assertEqual(aiming.get("projectile_prompt"), "Fire an arrow with the shortbow. Choose a direction.")
        start = len(self.replies)
        preview = press("Right", "right")
        self.assertEqual(preview.get("projectile_prompt"), "Fire an arrow with the shortbow east? Enter confirms. Esc cancels.")
        server = self.reply("ability_weapon_preview", start)
        self.assertEqual(preview["weapon_preview"]["preview"], server["preview"])
        for line in preview_lines(server["preview"]):
            self.assertIn(line, "\n".join(preview["weapon_preview_rows"]))
        self.assertEqual(preview["state"], initial["state"])
        press("Return", "enter")
        launched = self.settle()
        self.assertEqual(item(launched, "arrow")["quantity"], "5")
        self.assertEqual(item(launched, "shortbow")["id"], item(initial, "shortbow")["id"])
        self.assertFalse(any(row["item"]["name"] == "arrow" for row in view(launched)["ground_items"]))
        self.assertEqual(len(self.admissions("rogue.launch")), 1)


class ProjectileSettle(unittest.TestCase):
    """Deterministic cross-client snapshot schedules; no server or native window."""

    @staticmethod
    def observed(tick, ready, intentions=()):
        return {"state": {"observation": {"tick": str(tick), "ready": ready}},
                "intentions": list(intentions)}

    def test_player_completion_between_snapshots_does_not_require_target_wait(self):
        harness = ProjectileProcesses()
        harness.target = object()
        pending = self.observed(300, False)
        finished = self.observed(344, True)
        with patch.object(harness, "snapshot", side_effect=[pending, finished]) as snapshot, \
                patch.object(harness, "request", return_value=self.observed(344, False)) as request, \
                patch.object(harness, "act") as act:
            self.assertIs(harness.settle(), finished)
        self.assertEqual(snapshot.call_count, 2)
        request.assert_called_once_with(harness.target, {"type": "snapshot"})
        act.assert_not_called()

    def test_unready_target_and_still_pending_player_remain_a_failure(self):
        for fresh in (self.observed(344, False), self.observed(344, True, [{"phase": "queued"}])):
            harness = ProjectileProcesses()
            harness.target = object()
            with self.subTest(fresh=fresh), \
                    patch.object(harness, "snapshot", side_effect=[self.observed(300, False), fresh]), \
                    patch.object(harness, "request", return_value=self.observed(344, False)), \
                    patch.object(harness, "act") as act:
                with self.assertRaisesRegex(AssertionError, "Fixture target cannot advance pending player work"):
                    harness.settle()
                act.assert_not_called()

    def test_ready_target_advances_player_with_an_ordinary_wait(self):
        harness = ProjectileProcesses()
        harness.target = object()
        finished = self.observed(344, True)
        with patch.object(harness, "snapshot", side_effect=[self.observed(300, False), finished]), \
                patch.object(harness, "request", return_value=self.observed(300, True)), \
                patch.object(harness, "act", return_value={"error": None}) as act:
            self.assertIs(harness.settle(), finished)
        act.assert_called_once_with(harness.target, {"type": "wait"})

    def test_initially_finished_player_needs_no_peer_request(self):
        harness = ProjectileProcesses()
        finished = self.observed(344, True)
        with patch.object(harness, "snapshot", return_value=finished), \
                patch.object(harness, "request") as request, patch.object(harness, "act") as act:
            self.assertIs(harness.settle(), finished)
        request.assert_not_called()
        act.assert_not_called()

    def test_wait_errors_and_finite_progress_bound_remain_failures(self):
        harness = ProjectileProcesses()
        harness.target = object()
        with patch.object(harness, "snapshot", return_value=self.observed(300, False)), \
                patch.object(harness, "request", return_value=self.observed(300, True)), \
                patch.object(harness, "act", return_value={"error": "rejected"}) as act:
            with self.assertRaises(AssertionError):
                harness.settle()
            self.assertEqual(act.call_count, 1)
        with patch.object(harness, "snapshot", return_value=self.observed(300, False)), \
                patch.object(harness, "request", return_value=self.observed(300, True)), \
                patch.object(harness, "act", return_value={"error": None}) as act:
            with self.assertRaisesRegex(AssertionError, "did not settle within 1600 ticks"):
                harness.settle()
            self.assertEqual(act.call_count, 16)


class ProjectileCaptureLaunch(unittest.TestCase):
    """Launch wiring checks do not start a server or a native window."""

    def test_native_capture_keeps_os_input_and_relayed_frame_reports(self):
        harness = ProjectileProcesses()
        harness.address = "127.0.0.1:41001"
        window = object()
        initial = {"state": {"observation": {}}, "busy": False, "window_open": True}
        relay = Mock(address="127.0.0.1:41002")

        def presented(client, predicate):
            self.assertIs(client, window)
            self.assertFalse(predicate({**initial, "state": None}))
            self.assertFalse(predicate({**initial, "busy": True}))
            self.assertTrue(predicate(initial))
            return initial

        with patch(f"{__name__}.StreamRelay", return_value=relay) as capture, \
                patch.object(harness, "launch", return_value=window) as launch, \
                patch.object(harness, "ascii_frame", side_effect=presented):
            self.addCleanup(harness.doCleanups)
            self.assertEqual(harness.capture_client(native=True), (window, initial))
        launch.assert_called_once_with("tor-client-ascii", [
            "--connect", relay.address, "--report-frames"])
        self.assertEqual(harness.address, "127.0.0.1:41001")
        self.assertEqual(capture.call_args.args, (harness.address,))
        message = {"type": "projectile_choices"}
        capture.call_args.kwargs["on_server_message"](message)
        self.assertEqual(harness.replies, [message])
        harness.doCleanups()
        relay.close.assert_called_once_with()


if __name__ == "__main__":
    unittest.main()
