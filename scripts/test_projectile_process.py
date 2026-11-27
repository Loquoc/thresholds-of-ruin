"""Real source pickers, exact previews and projectile payment/recovery.

Default test_*_process discovery registers all methods. The graphical class
places the native keyboard witness in CI's exclusive native lane; only the Text
methods is explicitly non-graphical. No production test API or save mutation.
"""
import copy
import json
import shutil
import sqlite3
import time
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

    def setup_encounter(self, **options):
        self.package = projectile_package(self, **options)
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

    def test_ascii_cell_aim_shows_range_and_submits_one_off_axis_shot(self):
        self.setup_encounter(short_ranges=True)
        owner, initial = self.client()
        bow = item(initial, "shortbow")["id"]
        dagger = item(initial, "dagger")["id"]
        self.assertIsNone(self.act(owner, {"type": "unequip", "item": dagger})["error"])
        self.settle()
        self.assertIsNone(self.act(owner, {"type": "equip", "item": bow, "slot": 0})["error"])
        self.settle()
        owner.stop()
        window, before = self.window()
        self.assertEqual(next(row["launch_range"] for row in view(before)["known_item_uses"]
                              if row["item"] == bow), {"short_cells": 2, "long_cells": 3})

        def press(key):
            return self.key(window, key)

        def cursor(frame, x, y, band):
            self.assertEqual(frame["projectile_cursor"], [{"x": x, "y": y, "z": 0}, band])

        ammunition = press("fire")
        if ammunition["projectile_rows"] != ["> arrow"]:
            ammunition = self.ascii_frame(window, lambda f: not f["busy"] and f["projectile_rows"] == ["> arrow"])
        self.assertEqual(ammunition["projectile_rows"], ["> arrow"])
        press("enter")
        cursor(press("tab"), 0, 0, "origin")
        cursor(press("north_east"), 1, -1, "short")
        cursor(press("right"), 2, -1, "short")   # Last short-range square.
        cursor(press("right"), 3, -1, "long")    # First long-range square.
        cursor(press("right"), 4, -1, "outside") # First out-of-range square.
        invalid = press("enter")
        cursor(invalid, 4, -1, "outside")
        self.assertEqual(view(invalid), view(before))
        cancelled = press("escape")
        self.assertIsNone(cancelled["projectile_cursor"])
        self.assertEqual(view(cancelled), view(before))
        self.assertEqual(self.admissions("rogue.launch"), [])

        aim = press("fire")
        if not aim["projectile_prompt"]:
            aim = self.ascii_frame(window, lambda f: not f["busy"] and f["projectile_prompt"])
        self.assertIn("Tab aims at a square", aim["projectile_prompt"])
        press("tab")
        press("north_east")
        press("right")
        selected = press("right")
        cursor(selected, 3, -1, "long")
        off_axis = [3, -1, 0]
        self.assertFalse(any([actor["position"][axis] for axis in "xyz"] == off_axis
                             for actor in view(selected)["visible_actors"]))
        submitted = self.key(window, "enter", wait_for_simulation=False)
        self.assertIsNone(submitted["weapon_preview"])
        self.assertIsNone(submitted["projectile_cursor"])
        final = self.settle()
        self.assertEqual(int(item(final, "arrow")["quantity"]), int(item(before, "arrow")["quantity"]) - 1)
        admitted, = self.admissions("rogue.launch")
        aim = admitted["action"]["invocation"]["aim"]
        self.assertEqual(aim["kind"], "cell_projectile")
        self.assertEqual(aim["offset"], off_axis)

    def ascii_primary_weapon(self, *, cursed=False, offhand_mace=False):
        self.setup_encounter(primary_dagger=True, cursed_dagger=cursed, offhand_mace=offhand_mace)
        window, before = self.window()
        dagger, bow = (item(before, name)["id"] for name in ("dagger", "shortbow"))
        self.assertEqual(self.sockets(before)[dagger], 0, "ordinary primary held before the attempt")
        self.assertIsNone(self.sockets(before)[bow])
        return window, before, dagger, bow

    @staticmethod
    def sockets(frame):
        return {row["item"]: row["equipped_slot"] for row in view(frame)["interactions"]["inventory"]}

    def choose_shortbow(self, window, before):
        choosing = self.key(window, "wield")
        self.assertEqual(len(choosing["item_rows"]), 1)
        self.assertIn("shortbow", choosing["item_rows"][0])
        self.assertEqual(view(choosing), view(before), "choosing to wield doesn't itself act")
        return self.key(window, "enter", wait_for_simulation=False)

    def settle_weapon_shortcut(self, incoming):
        # There is an ordinary ready boundary after removal and before the
        # ASCII client submits Equip. That first ready boundary is not success.
        for _ in range(60):
            current = self.settle()
            if self.sockets(current)[incoming] == 0:
                return current
            time.sleep(0.05)
        self.fail("Equip never followed the confirmed removal")

    def designate_secondary(self, window, name):
        self.key(window, "commands")
        window.write(json.dumps({"type": "text", "text": "secondary"}))
        command = self.ascii_frame(window, lambda f: f["command_line"] == "secondary")
        self.assertTrue(any("#secondary" in row for row in command["command_rows"]),
                        "The secondary weapon selector must be discoverable")
        choosing = self.key(window, "enter")
        self.assertEqual(len(choosing["item_rows"]), 1)
        self.assertIn(name, choosing["item_rows"][0])
        self.key(window, "enter")
        inventory = self.key(window, "inventory")
        self.assertTrue(any(name in row and "secondary weapon" in row for row in inventory["inventory_rows"]))
        self.key(window, "escape")

    def test_ascii_secondary_weapon_swaps_there_and_back(self):
        window, before, dagger, bow = self.ascii_primary_weapon()
        missing = self.key(window, "swap_weapon")
        self.assertIn("No secondary weapon chosen", missing["status"])
        self.assertDisclosedStateEqual(missing["state"], before["state"])
        self.designate_secondary(window, "shortbow")
        self.assertDisclosedStateEqual(self.snapshot()["state"], before["state"])
        for incoming, outgoing, next_name in [(bow, dagger, "dagger"), (dagger, bow, "shortbow")]:
            self.key(window, "swap_weapon", wait_for_simulation=False)
            after = self.settle_weapon_shortcut(incoming)
            self.assertEqual(self.sockets(after)[incoming], 0)
            self.assertIsNone(self.sockets(after)[outgoing])
            # The observer can see Equip finish before the ASCII window has
            # reconciled its picture and can accept another key.
            self.ascii_frame(window, lambda f: (
                not f["busy"] and f["observation_current"] and f["character_ready"]
                and f["can_admit_intention"] and not f["intentions"]
                and self.sockets(f).get(incoming) == 0
            ))
            inventory = self.key(window, "inventory")
            rows = inventory["inventory_rows"]
            self.assertIsNotNone(rows, "the ready client must open the inventory")
            self.assertTrue(any(next_name in row and "secondary weapon" in row for row in rows))
            self.key(window, "escape")

    def test_ascii_secondary_weapon_stays_designated_when_swap_is_blocked(self):
        window, before, dagger, bow = self.ascii_primary_weapon(cursed=True)
        self.designate_secondary(window, "shortbow")
        self.key(window, "swap_weapon", wait_for_simulation=False)
        after = self.settle()
        self.assertEqual(self.sockets(after)[dagger], 0)
        self.assertIsNone(self.sockets(after)[bow])
        shown = self.ascii_frame(window, lambda f: "it is cursed" in f["messages"]["turn"])
        self.assertIn("dagger", shown["messages"]["turn"])
        # As in the successful swap, the refusal can reach the observer before
        # the ASCII window has reconciled its picture and can accept a key.
        self.ascii_frame(window, lambda f: (
            not f["busy"] and f["observation_current"] and f["character_ready"]
            and f["can_admit_intention"] and not f["intentions"]
            and self.sockets(f).get(dagger) == 0
        ))
        inventory = self.key(window, "inventory")
        rows = inventory["inventory_rows"]
        self.assertIsNotNone(rows, "the ready client must open the inventory")
        self.assertTrue(any("shortbow" in row and "secondary weapon" in row for row in rows))

    def test_ascii_wield_one_command_replaces_the_primary_weapon(self):
        window, before, dagger, bow = self.ascii_primary_weapon()
        attempted = self.choose_shortbow(window, before)
        final = self.settle_weapon_shortcut(bow)
        self.assertEqual(self.sockets(final)[bow], 0, attempted["status"])
        self.assertIsNone(self.sockets(final)[dagger], "the old weapon is carried, not dropped")
        self.assertEqual(int(view(final)["tick"]) - int(view(before)["tick"]), 200,
                         "removal and equip both use the published weapon-slot timing")

    def test_ascii_wield_refuses_cursed_primary_without_changing_equipment(self):
        window, before, dagger, bow = self.ascii_primary_weapon(cursed=True)
        self.choose_shortbow(window, before)
        final = self.settle()
        self.assertEqual(self.sockets(final)[dagger], 0)
        self.assertIsNone(self.sockets(final)[bow])
        self.assertEqual(item(final, "shortbow")["quantity"], item(before, "shortbow")["quantity"])
        shown = self.ascii_frame(window, lambda f: "it is cursed" in f["messages"]["turn"])
        self.assertIn("dagger", shown["messages"]["turn"])

    def test_ascii_wield_checkpoint_resumes_only_the_server_removal(self):
        window, before, dagger, bow = self.ascii_primary_weapon()
        attempted = self.choose_shortbow(window, before)
        preparing = attempted if view(attempted)["combat"]["preparation_active"] else self.ascii_frame(
            window, lambda f: view(f)["combat"]["preparation_active"])
        self.assertEqual(self.sockets(preparing)[dagger], 0)
        self.assertIsNone(self.sockets(preparing)[bow])
        identity, = preparing["intentions"]
        self.assertEqual(identity["phase"], "started")

        # Restart restores only the existing Unequip action. The ASCII
        # follow-up is deliberately not persisted or recovered.
        self.flush_save()
        self.game.stop()
        window.stop(); self.probe.stop(); self.target.stop()
        self.server(scenario=self.package, seed=None)
        self.probe, _ = self.client(observe=True)
        self.target, _ = self.client(actor=3)
        window, paused = self.window()
        restored, = paused["intentions"]
        self.assertEqual(restored["intention"], identity["intention"])
        self.assertEqual(restored["phase"], "paused")
        self.assertEqual(self.sockets(paused), self.sockets(before))
        self.assertEqual(view(paused)["combat"]["preparation_remaining"],
                         view(preparing)["combat"]["preparation_remaining"])

        resumed = self.key(window, "resume_intention", wait_for_simulation=False)
        self.assertEqual([row["intention"] for row in resumed["intentions"]], [identity["intention"]])
        final = self.settle()
        self.assertIsNone(self.sockets(final)[dagger])
        self.assertIsNone(self.sockets(final)[bow])
        self.assertEqual(int(view(final)["tick"]) - int(view(before)["tick"]), 100)

    def test_ascii_wield_reports_unfilled_socket_when_server_refuses_equip(self):
        window, before, dagger, bow = self.ascii_primary_weapon(offhand_mace=True)
        self.choose_shortbow(window, before)
        final = self.settle()
        refused = self.ascii_frame(window, lambda f: "new weapon was not equipped" in f["status"])
        self.assertIn("grip", refused["status"])
        self.assertIsNone(self.sockets(final)[dagger])
        self.assertIsNone(self.sockets(final)[bow])
        self.assertEqual(self.sockets(final)[item(final, "mace")["id"]], 10)

    @non_graphical
    def test_invalid_incoming_weapon_does_not_remove_the_current_one(self):
        self.setup_encounter(primary_dagger=True)
        owner, before = self.client()
        arrow = item(before, "arrow")["id"]
        result = self.act(owner, {"type": "equip", "item": arrow, "slot": 0})
        self.assertIsNotNone(result["error"], result)
        self.assertDisclosedStateEqual(self.snapshot()["state"], before["state"])

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
            self.assertEqual(aiming.get("projectile_prompt"), "Throw the dagger. Choose a direction; Tab aims at a square.")
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
        # Explicit Launch from the ability menu still asks for exact terms.
        # The separate quick Fire path is covered by the quiver process test.
        menu = press("Z", "abilities")
        index = [row["id"] for row in menu["ability_choices"]].index("rogue.launch")
        for _ in range(index):
            press("Down", "down")
        sources = press("Return", "enter")
        self.assertIn("shortbow", "\n".join(sources["projectile_rows"]))
        start = len(self.replies)
        ammunition = press("Return", "enter")
        page = self.reply("projectile_choices", start)["page"]
        self.assertEqual(page["query"]["phase"]["kind"], "ammunition")
        self.assertEqual(page["choices"], [{"kind": "ammunition", "item": item(initial, "arrow")["id"]}])
        self.assertEqual(ammunition["projectile_selection"]["page"], page)
        self.assertEqual(ammunition["projectile_rows"], ["> arrow"])
        aiming = press("Return", "enter")
        self.assertEqual(aiming.get("projectile_prompt"), "Fire an arrow with the shortbow. Choose a direction; Tab aims at a square.")
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
