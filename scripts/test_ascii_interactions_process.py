"""Equipment and consumption through the ASCII client's native keyboard."""
import json
import shutil
import subprocess
import unittest

from process_harness import ROOT, ProcessTestCase, read_capture


class AsciiInteractionProcesses(ProcessTestCase):
    graphical = True

    def test_fire_quiver_depletion_replacement_and_cancel(self):
        package = self.directory / "rogue-quiver"
        shutil.copytree(ROOT / "scenarios/rogue", package)
        manifest = package / "scenario.toml"
        definition = manifest.read_text()
        swaps = {
            'archetype = "rogue.mace", quantity = 1, equipped_slot = 0':
                'archetype = "rogue.mace", quantity = 1',
            'archetype = "rogue.shortbow", quantity = 1 },\n  # Rogue':
                'archetype = "rogue.shortbow", quantity = 1, equipped_slot = 0 },\n  # Rogue',
            '{ archetype = "rogue.arrow", quantity = 32 },':
                '{ archetype = "rogue.arrow", quantity = 2 },\n'
                '  { archetype = "rogue.spare_arrow", quantity = 3 },',
            '"item.weapon.arrow", "item.food.ration"':
                '"item.weapon.arrow", "item.weapon.spare-arrow", "item.food.ration"',
        }
        for original, replacement in swaps.items():
            self.assertEqual(definition.count(original), 1, original)
            definition = definition.replace(original, replacement)
        manifest.write_text(definition)
        catalog = package / "catalog/items.toml"
        items = catalog.read_text()
        definition = items.split('[items."rogue.arrow"]', 1)[1].split("# Generated loot", 1)[0]
        definition = definition.replace('name = "arrow"', 'name = "spare arrow"')
        definition = definition.replace('identity = "item.weapon.arrow"', 'identity = "item.weapon.spare-arrow"')
        catalog.write_text(items + '\n[items."rogue.spare_arrow"]' + definition)

        validation = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                    capture_output=True, text=True, encoding="utf-8", timeout=120)
        self.assertEqual(validation.returncode, 0, validation.stderr)
        self.server(scenario=package)
        window, initial = self.window()

        def view(frame):
            return frame["state"]["observation"]

        def quantities(frame):
            return {item["name"]: int(item["quantity"]) for item in view(frame)["inventory"]
                    if item["name"] in {"arrow", "spare arrow"}}

        def press(key):
            return self.key(window, key)

        def wait_for(predicate):
            return self.ascii_frame(window, lambda f: not f["busy"] and predicate(f))

        def choose_ammo(frame, name):
            rows = frame["projectile_rows"]
            for _ in range(next(i for i, row in enumerate(rows) if row[2:] == name)):
                press("down")
            return press("enter")

        def aim(frame, name):
            prompt = frame["projectile_prompt"] or ""
            return prompt.startswith(f"Fire {'an' if name == 'arrow' else 'a'} {name} ")

        def fire_ready(name):
            frame = press("fire")
            return frame if aim(frame, name) else wait_for(lambda f: aim(f, name))

        def shot(before, remaining):
            frame = self.key(window, "right", wait_for_admission=True)
            if quantities(frame) != remaining:
                frame = wait_for(lambda f: quantities(f) == remaining and f["can_admit_intention"])
            self.assertGreater(int(view(frame)["tick"]), int(view(before)["tick"]))
            self.assertIsNone(frame["weapon_preview"], "direction must not open a confirmation")
            self.assertIsNone(frame["projectile_selection"])
            self.assertEqual(quantities(frame), remaining)
            return frame

        self.assertEqual(quantities(initial), {"arrow": 2, "spare arrow": 3})
        opened = press("fire")
        if not any(row[2:] == "arrow" for row in opened["projectile_rows"]):
            opened = wait_for(lambda f: any(row[2:] == "arrow" for row in f["projectile_rows"]))
        self.assertEqual(view(opened), view(initial), "opening Fire with an empty quiver is free")
        self.assertNotIn("shortbow", " ".join(opened["projectile_rows"]), "launcher was auto-selected")
        self.assertEqual(view(press("escape")), view(initial))

        press("commands")
        window.write(json.dumps({"type": "text", "text": "quiver"}))
        wait_for(lambda f: any("#quiver" in row for row in f["command_rows"]))
        selection = press("enter")
        if not any(row[2:] == "arrow" for row in selection["projectile_rows"]):
            selection = wait_for(lambda f: any(row[2:] == "arrow" for row in f["projectile_rows"]))
        quivered = choose_ammo(selection, "arrow")
        self.assertIsNone(quivered["projectile_selection"])
        self.assertEqual(view(quivered), view(initial), "quivering is free")

        before = fire_ready("arrow")
        self.assertEqual(view(press("escape")), view(before), "cancelled aim is free")
        shot(fire_ready("arrow"), {"arrow": 1, "spare arrow": 3})
        depleted = shot(fire_ready("arrow"), {"spare arrow": 3})
        replacement = press("fire")
        if not any(row[2:] == "spare arrow" for row in replacement["projectile_rows"]):
            replacement = wait_for(lambda f: any(row[2:] == "spare arrow" for row in f["projectile_rows"]))
        self.assertEqual(view(replacement), view(depleted))
        self.assertTrue(aim(choose_ammo(replacement, "spare arrow"), "spare arrow"))
        self.assertEqual(view(press("escape")), view(depleted), "replacement does not fire")
        shot(fire_ready("spare arrow"), {"spare arrow": 2})

    def test_native_equipment_removal_and_unknown_potion_choices(self):
        self.server(scenario="interactions")
        capture = self.save.parent / "interactions.ppm"
        window = self.launch("tor-client-ascii", ["--connect", self.address, "--report-frames", "--capture", capture])
        initial = self.ascii_frame(window, lambda f: f["state"] and not f["busy"] and f["can_admit_intention"])
        native = self.native_keys(window)

        def press(name, action, **completion):
            if name in ("W", "T"):
                native("Shift_L", True)
                try:
                    return self.native_press(window, native, name.lower(), action, **completion)
                finally:
                    native("Shift_L", False)
            return self.native_press(window, native, name, action, **completion)

        def view(frame):
            return frame["state"]["observation"]

        mail = next(item["id"] for item in view(initial)["inventory"] if item["name"] == "mail")
        # W always asks, even with one eligible armor item.
        asking = press("W", "wear")
        self.assertEqual(view(asking), view(initial), "opening a selection costs no turn")
        equipped = press("Return", "enter", wait_for_ready=True, wait_for_admission=True)
        self.assertEqual(next(item for item in view(equipped)["interactions"]["inventory"] if item["item"] == mail)["equipped_slot"], 1)
        self.assertEqual(int(view(equipped)["tick"]), 300)
        self.assertEqual(equipped["messages"]["turn"].count("You finish equipping the mail."), 1)
        self.assertEqual(equipped["messages"]["turn"], "You finish equipping the mail.")
        selected = press("T", "take_off")
        self.assertEqual(view(selected), view(equipped), "opening a selection costs no turn")
        self.assertEqual(len(selected["item_rows"]), 1, "T excludes equipped weapons and accessories")
        def screenshot_accepted(frame):
            return "Screenshot accepted into game history" in frame["status"] and not frame["busy"]

        captured = press("F1", "screenshot", seconds=8, wait_for_simulation=False)
        if not screenshot_accepted(captured):
            captured = self.ascii_frame(window, screenshot_accepted)
        self.assertEqual(captured["item_rows"], selected["item_rows"])
        self.assertEqual(view(captured), view(selected))
        self.assertIn("b -", selected["item_rows"][0])
        native("b", True)
        try:
            removed = self.ascii_frame(
                window, lambda f: f["can_admit_intention"] and not f["busy"] and int(view(f)["tick"]) >= 600
            )
        finally:
            native("b", False)
        self.assertIsNone(next(item for item in view(removed)["interactions"]["inventory"] if item["item"] == mail)["equipped_slot"])
        self.assertNotEqual(removed["input_done"], "south_west")
        self.assertEqual(view(removed)["position"], view(equipped)["position"])
        self.assertEqual(int(view(removed)["tick"]), 600)
        choosing = press("q", "drink")
        self.assertEqual(view(choosing), view(removed))
        self.assertNotIn("potion of healing", json.dumps(choosing["state"]))
        self.assertNotIn("potion of poison", json.dumps(choosing["state"]))
        cancelled = press("Escape", "escape")
        self.assertEqual(view(cancelled), view(removed))
        press("q", "drink")
        consumed = press("Return", "enter", wait_for_ready=True, wait_for_admission=True)
        potions = [item for item in view(consumed)["inventory"] if item["class"] == "potion"]
        self.assertEqual(sorted(int(item["quantity"]) for item in potions), [1, 2])
        self.assertEqual(int(view(consumed)["tick"]), 700)
        self.assertEqual(consumed["messages"]["turn"].count("You finish drinking the red potion."), 1)
        first_potion = potions[0]["id"]
        press("q", "drink")
        final_unit = press("Return", "enter", wait_for_ready=True, wait_for_admission=True)
        self.assertFalse(any(item["id"] == first_potion for item in view(final_unit)["inventory"]))
        self.assertEqual(final_unit["messages"]["turn"].count("You finish drinking the red potion."), 1)
        pixels = read_capture(capture).split(b"\n", 3)[3]
        self.assertEqual(len(pixels), 1200 * 800 * 3)
        self.assertGreater(len(set(pixels)), 8)


if __name__ == "__main__":
    unittest.main()
