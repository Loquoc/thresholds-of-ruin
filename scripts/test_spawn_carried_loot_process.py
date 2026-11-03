"""Ordinary carried stock, death drops, finite use and source-free recovery."""
import copy
import shutil
import subprocess
import tomllib
import unittest

from arena_matrix import toml_source
from process_harness import ProcessTestCase, ROOT, SPECTATOR_TOKEN, explore_to_stairs, settle_rogue_action, underfoot
from rogue_roster_package import encounter_sources, unrelated_fallback
from status_condition_package import checkpoint


def view(frame):
    return frame["state"]["observation"]


class SpawnCarriedLootProcesses(ProcessTestCase):
    def fixture(self, numerator=1, denominator=1, *, weapon_capacity=None, care_known=None):
        package = self.directory / f"carried-{numerator}-{denominator}-{weapon_capacity}-{care_known}"
        package.mkdir()
        shutil.copy2(ROOT / "scenarios/rogue-exploration/rules.toml", package / "rules.toml")
        source = tomllib.loads((ROOT / "scenarios/tests/generated-filler/scenario.toml").read_text(encoding="utf-8"))
        manifest = {key: copy.deepcopy(source[key]) for key in ("format", "version", "ruleset", "creatures")}
        # Current Rogue rules include an opt-in Polymorph roster. Register its
        # immutable definition closure, without enrolling either synthetic actor
        # or changing their one-HD recipes, identities, or birth-stock stream.
        production = encounter_sources()
        self.assertFalse(set(manifest["creatures"]["species"]) & set(production["species"]))
        manifest["creatures"]["species"].update(copy.deepcopy(production["species"]))
        manifest["creatures"]["body_packages"] = copy.deepcopy(production["body_packages"])
        character = copy.deepcopy(source["characters"][0])
        character.pop("asset", None)
        character["anchor"] = "dungeon/1/start"
        manifest.update(id="spawn-carried-stock", default_character=1, characters=[character],
                        factions={"delvers": ["vermin"], "vermin": []},
                        ai_profiles={"idle": {"care": False, "equipment": False, "loot": False, "flee_percent": 0}})
        rat = copy.deepcopy(source["archetypes"]["rat"])
        rat.pop("asset", None)
        manifest["creatures"]["species"]["cave_rat"]["behavior"] = {
            "disposition": "passive", "idle": "wait", "memory_ticks": 1000}
        manifest["archetypes"] = {"carrier": rat, "tonic": {
            "identity": "item.potion.stock-tonic", "name": "stock tonic", "class": "potion", "stackable": True,
            "consumable": {"effects": [{"kind": "heal", "amount": {"kind": "fixed", "amount": 1}}]}}}
        manifest["generation_recipes"] = {"tiny": {"kind": "grid_rooms", "version": 1,
            "room_width": [4, 4], "room_height": [3, 3], "extra_connections": [0, 0]}}
        manifest["procedural_worlds"] = {"dungeon": {"kind": "grid_chain", "version": 1,
            "base_region": 1, "grid": [1, 1], "size": [8, 5, 2], "recipe": "tiny",
            "domain": {"kind": "open", "max_depth": 100}}}
        manifest["floor_activation"] = {"version": 1, "protagonist": 1,
            "families": {"dungeon": "baseline"},
            "carried_item_pools": {"supplies": {"version": 1, "choices": [
                {"archetype": "tonic", "weight": 1, "quantity": 2}]}},
            "profiles": {"baseline": {"version": 1, "stages": [{"id": "carriers", "kind": "actors",
                "version": 1, "count": [1, 1], "ai": "idle", "bands": [{
                    "depth": {"kind": "open", "minimum": 1}, "choices": [{"archetype": "carrier", "weight": 1,
                    "carried_loot": {"version": 1, "pool": "supplies", "chance": {
                        "numerator": numerator, "denominator": denominator}}}]}]}]}}}
        if weapon_capacity is not None:
            species = manifest["creatures"]["species"]["cave_rat"]
            species["kind"] = "humanoid"
            if weapon_capacity:
                species.pop("anatomy")  # Ordinary humanoid equipment anatomy.
            manifest["archetypes"]["carrier"]["creature"]["name"] = "stock keeper"
            manifest["ai_profiles"]["idle"]["equipment"] = True
            manifest["archetypes"]["blades"] = {
                "identity": "item.weapon.stock-blade", "name": "stock blade",
                "class": "weapon", "stackable": True,
                "equipment": {"slot": "weapon", "modifiers": [],
                    "attack": {"skill": "light_weaponry", "bonus": 5, "wind_up": 20, "recovery": 20,
                        "damage": {"primary": {"category": "keen"}, "components": [
                            {"category": "keen", "amount": {"type": "fixed", "value": 4}}]}},
                    "weapon_profile": {"expertise": "rogue.knife", "base_traits": [],
                        "expert_traits": {"add": [], "remove": []}}}}
            manifest["floor_activation"]["carried_item_pools"]["supplies"]["choices"] = [
                {"archetype": "blades", "weight": 1, "quantity": 3}]
        if care_known is not None:
            manifest["ai_profiles"]["idle"]["care"] = True
            species = manifest["creatures"]["species"]["cave_rat"]
            species["kind"] = "humanoid"
            species.pop("anatomy")
            species["modifiers"].append({"scope": "always", "op": {
                "kind": "add", "stat": {"kind": "health"}, "value": 20}})
            manifest["archetypes"]["carrier"]["creature"]["name"] = "stock medic"
            # A normal checkless fixture attack establishes a nonfatal deficit.
            strike = manifest["creatures"]["species"]["delver"]["melee"]["strikes"][0]
            strike.pop("check")
            strike["on_success"][0]["damage"]["components"][0]["amount"]["amount"] = 15
            manifest["archetypes"]["tonic"]["consumable"] = {"discovery": "on_use", "effects": [
                {"kind": "heal_per_hit_die", "sides": 2, "bonus": 100, "overflow": "none"}]}
            if not care_known:
                manifest["appearance_pools"] = {"stock_doses": {
                    "appearances": [{"name": "unfamiliar dose"}]}}
                manifest["archetypes"]["tonic"]["appearance_pool"] = "stock_doses"
            # No known_identities are granted. The positive item is openly
            # authored; the otherwise identical negative is concealed.
        (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8", newline="\n")
        result = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
                                text=True, capture_output=True, encoding="utf-8", timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        return package

    def execute(self, player, action):
        return settle_rogue_action(self, player, self.act(player, action))

    def approach_and_defeat(self, player, frame, name="cave rat"):
        self.assertTrue(any(row["name"] == name for row in view(frame)["visible_actors"]),
                        "tiny lit fixture must initially disclose its real carrier")
        for _ in range(30):
            foes = [row for row in view(frame)["visible_actors"] if row["name"] == name]
            if not foes:
                return frame
            foe = foes[0]
            p = foe["position"]
            if max(abs(p["x"]), abs(p["y"])) > 1:
                direction = ("east" if p["x"] > 0 else "west") if p["x"] else ("south" if p["y"] > 0 else "north")
                frame = self.execute(player, {"type": "move", "direction": direction})
            else:
                frame = self.execute(player, {"type": "use_ability", "id": "basic_melee",
                    "invocation": {"aim": {"kind": "actor_target", "target": foe["id"]}, "weapon": {"kind": "default"}}, "supplies": {"kind": "carried"}})
            self.assertIsNone(frame["error"])
        self.fail("Ordinary melee did not defeat the passive carrier within the bounded fixture")

    def collect_tonic(self, player, frame):
        for _ in range(10):
            stocks = [row for row in view(frame)["ground_items"] if row["item"]["name"] == "stock tonic"]
            self.assertEqual(len(stocks), 1, "death must disclose one ordinary birth stack")
            stock = stocks[0]
            p = stock["position"]
            if p["x"] == 0 and p["y"] == 0:
                result = self.execute(player, {"type": "take", "item": stock["item"]["id"], "quantity": None})
                self.assertIsNone(result["error"])
                return result
            direction = ("east" if p["x"] > 0 else "west") if p["x"] else ("south" if p["y"] > 0 else "north")
            frame = self.execute(player, {"type": "move", "direction": direction})
            self.assertIsNone(frame["error"])
        self.fail("The ordinary dropped stack was not reachable")

    def test_birth_death_take_use_source_deleted_restart_and_unvisited_floor(self):
        package = self.fixture()
        server = self.server("--checkpoint-interval", 1, scenario=package)
        player, initial = self.client()
        self.assertFalse(initial["state"]["wizard_game"])
        first = self.private_receipts()
        loot = first[0]["decisions"][0]["result"]["attempts"][0]["entity"]["carried_loot"]
        self.assertEqual((loot["kind"], loot["content"], loot["quantity"]), ("carried", "tonic", 2))
        frame = self.collect_tonic(player, self.approach_and_defeat(player, initial))
        tonic = next(row for row in view(frame)["inventory"] if row["name"] == "stock tonic")
        self.assertEqual(tonic["quantity"], "2")
        text, _ = self.text_client(SPECTATOR_TOKEN, observe=True)
        self.assertIn("2 x stock tonic", text.command("inventory"))
        text.stop()
        for remaining in ["1", None]:
            frame = self.execute(player, {"type": "drink", "item": tonic["id"]})
            self.assertIsNone(frame["error"])
            inventory = [row for row in view(frame)["inventory"] if row["name"] == "stock tonic"]
            self.assertEqual([row["quantity"] for row in inventory], [] if remaining is None else [remaining])
        self.assertEqual(self.private_receipts(), first, "consumption cannot rewrite birth evidence")
        before = self.request(player, {"type": "snapshot"})
        self.flush_save(); player.stop(); server.stop()
        shutil.rmtree(package)
        server = self.server("--checkpoint-interval", 1, scenario=unrelated_fallback(self), seed=None)
        player, restored = self.client()
        self.assertDisclosedStateEqual(restored["state"], before["state"])
        self.assertEqual(restored["history"], before["history"])
        self.assertEqual(self.private_receipts(), first)
        state = explore_to_stairs(self, player, restored["state"])
        self.assertTrue(underfoot(state)["stairs_down"])
        entered = self.execute(player, {"type": "move", "direction": "down"})
        self.assertIsNone(entered["error"])
        receipts = self.private_receipts()
        self.assertEqual(len(receipts), 2)
        second = receipts[1]["decisions"][0]["result"]["attempts"][0]["entity"]["carried_loot"]
        self.assertEqual(second["quantity"], 2)
        self.assertGreater(second["item"], loot["item"])
        self.assertIsNone(self.execute(player, {"type": "move", "direction": "up"})["error"])
        returned = self.execute(player, {"type": "move", "direction": "down"})
        self.assertIsNone(returned["error"])
        self.assertEqual(self.private_receipts(), receipts)
        self.assertFalse(any(row["name"] == "stock tonic" for row in view(returned)["inventory"]))
        player.stop(); server.stop()

    def test_born_weapon_stack_uses_normal_ai_equip_and_anatomy_after_source_deleted_restart(self):
        fallback = unrelated_fallback(self)
        for capacity in (True, False):
            with self.subTest(weapon_capacity=capacity):
                package = self.fixture(weapon_capacity=capacity)
                self.save = self.directory / f"weapon-capacity-{capacity}.db"
                server = self.server("--checkpoint-interval", 1, scenario=package)
                player, frame = self.client()
                self.assertFalse(frame["state"]["wizard_game"])
                receipts = self.private_receipts()
                birth = receipts[0]["decisions"][0]["result"]["attempts"][0]["entity"]["carried_loot"]
                self.assertEqual((birth["kind"], birth["content"], birth["quantity"]),
                                 ("carried", "blades", 3))

                def visible_weapons(current):
                    carriers = [row for row in view(current)["visible_actors"] if row["name"] == "stock keeper"]
                    self.assertEqual(len(carriers), 1, "ordinary sight must disclose the passive carrier")
                    return [row for row in carriers[0].get("worn", []) if row["slot"] == "weapon"]

                self.assertEqual(visible_weapons(frame), [], "born stock starts carried, not pre-equipped")
                # Only ordinary player waits drive the AI. Birth receipts are
                # immutable audit evidence, never a source of action targets.
                for _ in range(8):
                    frame = self.execute(player, {"type": "wait"})
                    self.assertIsNone(frame["error"])
                    weapons = visible_weapons(frame)
                    if capacity and weapons:
                        break
                self.assertEqual(len(weapons), int(capacity))
                if capacity:
                    self.assertEqual(weapons[0]["class"], "weapon")
                before = self.request(player, {"type": "snapshot"})
                self.flush_save(); player.stop(); server.stop()
                shutil.rmtree(package)
                server = self.server("--checkpoint-interval", 1, scenario=fallback, seed=None)
                player, restored = self.client()
                self.assertDisclosedStateEqual(restored["state"], before["state"])
                self.assertEqual(restored["history"], before["history"])
                self.assertEqual(self.private_receipts(), receipts)
                self.assertEqual(visible_weapons(restored), weapons)
                dropped = self.approach_and_defeat(player, restored, name="stock keeper")
                blades = [row["item"] for row in view(dropped)["ground_items"]
                          if row["item"]["name"] == "stock blade"]
                self.assertEqual(sorted(int(row["quantity"]) for row in blades),
                                 [1, 2] if capacity else [3],
                                 "death exposes the equipped singleton plus carried remainder, never a whole equipped stack")
                self.assertEqual(sum(int(row["quantity"]) for row in blades), 3)
                self.assertEqual(self.private_receipts(), receipts)
                self.flush_save(); player.stop(); server.stop()

    def test_born_medicine_uses_ordinary_ai_care_but_unknown_dose_is_retained(self):
        for known in (True, False):
            with self.subTest(known=known):
                package = self.fixture(care_known=known)
                self.save = self.directory / f"care-known-{known}.db"
                server = self.server("--checkpoint-interval", 1, scenario=package)
                player, frame = self.client()
                self.assertFalse(frame["state"]["wizard_game"])
                receipts = self.private_receipts()
                entity = receipts[0]["decisions"][0]["result"]["attempts"][0]["entity"]
                birth = entity["carried_loot"]
                actor_key = str(entity["id"])
                self.assertEqual((birth["kind"], birth["quantity"]), ("carried", 2))
                for _ in range(10):
                    carriers = [row for row in view(frame)["visible_actors"] if row["name"] == "stock medic"]
                    self.assertEqual(len(carriers), 1)
                    carrier = carriers[0]
                    position = carrier["position"]
                    if max(abs(position["x"]), abs(position["y"])) <= 1:
                        break
                    direction = ("east" if position["x"] > 0 else "west") if position["x"] else ("south" if position["y"] > 0 else "north")
                    frame = self.execute(player, {"type": "move", "direction": direction})
                    self.assertIsNone(frame["error"])
                else:
                    self.fail("The disclosed medic was not reachable within the fixture bound")
                before = checkpoint(self, player)
                self.assertEqual(before["actors"][actor_key]["combat"]["creature"]["injury"], 0)
                self.assertNotIn("item.potion.stock-tonic", before["actors"][actor_key]["knowledge"],
                                 "birth does not grant type knowledge, even for openly authored stock")
                self.assertEqual(before["items"][str(birth["item"])]["quantity"], 2,
                                 "uninjured AI must retain its finite medicine")
                frame = self.execute(player, {"type": "use_ability", "id": "basic_melee",
                    "invocation": {"aim": {"kind": "actor_target", "target": carrier["id"]},
                                   "weapon": {"kind": "default"}}, "supplies": {"kind": "carried"}})
                self.assertIsNone(frame["error"])
                # Only normal player turns advance the autonomous patient's
                # preparation and consumption. Private receipts never drive actions.
                for _ in range(6):
                    frame = self.execute(player, {"type": "wait"})
                    self.assertIsNone(frame["error"])
                saved = checkpoint(self, player)
                self.assertEqual(saved["items"][str(birth["item"])]["quantity"], 1 if known else 2,
                                 "known healing uses one ordinary unit; unknown effects are not guessed")
                injury = saved["actors"][actor_key]["combat"]["creature"]["injury"]
                if known:
                    self.assertEqual(injury, 0, "the paid dose actually heals the ordinary inflicted injury")
                else:
                    self.assertGreater(injury, 0, "the unknown control really is injured, not an unused happy path")
                    self.assertNotIn("item.potion.stock-tonic", saved["actors"][actor_key]["knowledge"])
                self.assertEqual(self.private_receipts(), receipts,
                                 "ordinary consumption never rewrites immutable birth evidence")
                player.stop(); server.stop()

    def test_explicit_miss_and_fixed_one_fifth_seed_pair_survive_reconnect(self):
        # The policy domain uses the same canonical names as the pinned unit vectors.
        for numerator, denominator, seed, expected in [(0, 1, 0, "not_carried"), (1, 5, 0, "carried"), (1, 5, 1, "not_carried")]:
            with self.subTest(chance=(numerator, denominator), seed=seed):
                package = self.fixture(numerator, denominator)
                self.save = self.directory / f"chance-{numerator}-{denominator}-{seed}.db"
                server = self.server(scenario=package, seed=seed)
                player, before = self.client()
                receipts = self.private_receipts()
                self.assertEqual(receipts[0]["decisions"][0]["result"]["attempts"][0]["entity"]["carried_loot"]["kind"], expected)
                player.stop()
                player, after = self.client()
                self.assertDisclosedStateEqual(after["state"], before["state"])
                self.assertEqual(self.private_receipts(), receipts)
                self.flush_save(); player.stop(); server.stop()
                shutil.rmtree(package)
                server = self.server(seed=None)
                player, resumed = self.client()
                self.assertDisclosedStateEqual(resumed["state"], before["state"])
                self.assertEqual(resumed["history"], before["history"])
                self.assertEqual(self.private_receipts(), receipts)
                player.stop(); server.stop()


if __name__ == "__main__":
    unittest.main()
