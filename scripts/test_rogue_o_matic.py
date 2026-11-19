"""Policy fixtures use only ordinary observation fields."""

from pathlib import Path
import sys
import unittest

# The public policy and model can be tested from source without loading the native extension.
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "crates/client-bot/python"))
from rogue_player import Adventurer
from tor_client_bot import Direction, Move, Pickup, Turn, UseAbility, Wait

from rogue_policy_reference import DEFAULT_OBJECTIVE_IDENTITY, Floor, Policy

AMULET = "Amulet of Yendor"


def cell(key, x=0, *, down=False, up=False):
    return {"key": key, "position": {"x": x, "y": 0, "z": 0}, "wall": False,
            "stairs_down": down, "stairs_up": up}


def state(cells, *, actors=(), hostiles=(), abilities=(), inventory=(), ground=()):
    return {"observation": {
        "ready": True, "self_target": "player", "visible_cells": cells,
        "visible_actors": actors, "inventory": inventory, "ground_items": ground,
        "combat": {"hp": "10", "max_hp": "10", "terminal": False, "dead": False,
                   "preparation_remaining": None,
                   "actors": [{"actor": actor, "hostile": True} for actor in hostiles],
                   "own_stats": {"abilities": abilities, "nutrition": {"hunger": "normal"}}},
    }}


def player_turn(cells, *, inventory=(), ground=(), actors=(), hostiles=(), abilities=(), tick=0,
                combat=None, stats=None, interactions=None):
    picture = state(cells, inventory=inventory, ground=ground, actors=actors,
                    hostiles=hostiles, abilities=abilities)
    view = picture["observation"]
    view["interactions"] = interactions or {}
    view["combat"].update(combat or {})
    view["combat"]["own_stats"].update(stats or {})
    view["tick"] = str(tick)
    view["visible_actors"] = [{"id": "player", "name": "you", "position": {"x": 0, "y": 0, "z": 0}},
                              *actors]
    return Turn({"state": picture, "readiness": {"admission": True}})


def choose(player, turn):
    player.observe(turn)
    return player.choose(turn)


class RogueOMaticPolicyTests(unittest.TestCase):
    def test_moves_onto_freed_downstairs_with_a_second_opponent_one_step_from_contact(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
        turn = player_turn([cell("here"), cell("west", -1), cell("exit", 1, down=True), cell("east", 2)],
                           actors=[{"id": "west", "name": "orc", "position": cell("west", -1)["position"]},
                                   {"id": "east", "name": "emu", "position": cell("east", 2)["position"]}],
                           hostiles=["west", "east"],
                           abilities=[{"id": "basic_melee", "name": "Strike", "basic_attack": True,
                                       "target_kind": "actor", "reach": {"kind": "melee"},
                                       "cost": None, "cooldown_remaining": 0}])
        self.assertEqual(choose(player, turn), (Move(Direction.EAST), "retreat_to_stairs"))

    def test_pinned_player_pushes_only_when_it_can_see_space_beyond_the_blocker(self):
        foes = [{"id": name, "name": name, "position": cell(name, x)["position"]}
                for name, x in (("emu", -1), ("orc", 1))]
        abilities = [{"id": name, "name": name, "basic_attack": name == "basic_melee", "cost": None,
                      "cooldown_remaining": 0, "target_kind": "actor", "reach": {"kind": "melee"}}
                     for name in ("basic_melee", "dislocating_strike")]
        for space, expected in (([], "basic_melee"), ([{**cell("wall", 2), "wall": True}], "basic_melee"),
                                ([cell("free", 2)], "dislocating_strike")):
            with self.subTest(space=space):
                turn = player_turn([cell("here"), cell("west", -1), cell("exit", 1, down=True), *space],
                                   actors=foes, hostiles=["emu", "orc"], abilities=abilities)
                action, _ = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual((action.ability.identity, action.target.name), (expected, "orc"))

    def test_drinks_known_haste_before_a_group_closes_and_avoids_a_second_dose(self):
        potion = {"id": "dose", "name": "potion of haste", "identity": "item.potion.haste",
                  "quantity": 2, "identified": True, "class": "potion"}
        enemies = [{"id": name, "name": name, "position": cell(name, x)["position"]}
                   for name, x in (("emu", 3), ("orc", -3))]
        for condition, count, expected in ((None, 2, True), (None, 1, False),
                                           ("rogue.hasted", 2, False), ("rogue.haste_exhaustion", 2, False)):
            with self.subTest(condition=condition, count=count):
                foes = enemies[:count]
                conditions = [{"condition": {"id": condition, "name": condition},
                               "phase": "active", "remaining_ticks": 300}] if condition else []
                turn = player_turn([cell("here")], actors=foes, hostiles=[f["id"] for f in foes],
                                   stats={"conditions": conditions}, inventory=[potion],
                                   interactions={"inventory": [{"item": "dose", "drinkable": True}]})
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                self.assertEqual(choose(player, turn)[1] == "combat_haste", expected)

    def test_uses_room_light_only_in_disclosed_darkness_and_remembers_an_unlit_passage(self):
        from tor_client_bot import Zap
        wand = {"id": "light", "name": "staff of light", "identity": "item.wand.wand-light",
                "identified": True, "quantity": 1, "class": "wand"}
        offer = {"item": "light", "zappable": True, "device_activation": "self_target", "charges": 3}
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        def picture(lit):
            terrain = cell("here")
            if lit is not None:
                terrain["lit"] = lit
            return player_turn([terrain, cell("east", 1)], inventory=[wand], interactions={"inventory": [offer]})
        for illumination in (None, True):
            turn = picture(illumination)
            self.assertIs(turn.cell.lit, illumination)
            self.assertNotIsInstance(choose(player, turn)[0], Zap)
        dark = picture(False)
        action, reason = choose(player, dark)
        self.assertEqual((action, reason), (Zap(dark.devices[0], None), "light_room"))
        player.admitted(dark, action, reason=reason)
        self.assertNotIsInstance(choose(player, dark)[0], Zap)
        choose(player, picture(True))
        self.assertEqual(choose(player, dark)[1], "light_room")

    def test_unknown_scroll_trial_uses_a_safe_outbound_stair_once_per_appearance(self):
        scroll = {"id": "scroll", "class": "scroll", "name": "runed scroll", "quantity": 2, "identified": False}
        ring = {"id": "ring", "class": "ring", "name": "opal ring", "quantity": 1, "identified": False}
        amulet = {"id": "amulet", "class": "amulet", "name": "Amulet", "quantity": 1, "identified": True,
                  "identity": DEFAULT_OBJECTIVE_IDENTITY}
        for depth, down, hp, pack, guarded, expected in (
            (3, True, 20, [scroll, ring], False, True),
            (1, True, 20, [scroll, ring], False, False),
            (3, False, 20, [scroll, ring], False, False),
            (3, True, 15, [scroll, ring], False, False),
            (3, True, 20, [scroll], False, False),
            (3, True, 20, [scroll, ring, amulet], False, False),
            (3, True, 20, [scroll, ring], True, False),
        ):
            with self.subTest(depth=depth, down=down, hp=hp, pack=len(pack), guarded=guarded):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=depth)
                turn = player_turn([cell("here", down=down)], inventory=pack,
                                   interactions={"inventory": [{"item": "scroll", "readable": True}]},
                                   combat={"hp": hp, "max_hp": 20})
                if guarded:
                    player.unsafe_stairs[("here", Direction.DOWN)] = 1200
                action, reason = choose(player, turn)
                self.assertEqual(reason == "study_scroll_at_exit", expected)
                if expected:
                    player.admitted(turn, action, reason=reason)
                    self.assertNotEqual(choose(player, turn)[1], reason)

    def test_ring_or_wand_scroll_matches_either_unidentified_category(self):
        for category, should_read in [("ring", True), ("wand", True), ("armor", False)]:
            with self.subTest(category=category):
                items = [{"id": "scroll", "class": "scroll", "name": "lore", "identified": True,
                          "identity": "item.scroll.identify-wand", "quantity": 1},
                         {"id": "unknown", "class": category, "name": "mystery", "identified": False,
                          "identity": f"item.{category}.unknown", "quantity": 1}]
                turn = player_turn([cell("stairs", down=True)], inventory=items,
                                   interactions={"inventory": [{"item": "scroll", "readable": True}]})
                _, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual(reason == "identify_gear", should_read)

    def test_protects_disclosed_unprotected_armor_and_preserves_scroll_otherwise(self):
        from tor_client_bot import UseItem, ItemUse
        body = {"id": "body_armor", "name": "body"}
        inventory = [
            {"id": "mail", "identity": "item.armor.ring-mail", "name": "mail", "class": "armor"},
            {"id": "scroll", "identity": "item.scroll.protect-armor", "name": "lore", "class": "scroll"},
        ]
        for item in inventory:
            item.update(identified=True, quantity=1)
        for known in (False, True, None):
            turn = player_turn([cell("stairs", down=True)], inventory=inventory,
                               interactions={"slots": [body], "inventory": [
                                   {"item": "mail", "slot": body, "equipped_slot": 0, "known_protected": known},
                                   {"item": "scroll", "readable": True}]})
            decision = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
            self.assertEqual(decision, (UseItem(turn.inventory[1], ItemUse.READ), "protect_armor") if known is False
                             else (Move(Direction.DOWN), "descend"))

    def test_trial_armor_needs_a_large_known_improvement_or_a_way_to_remove_a_curse(self):
        from tor_client_bot import Unequip
        slot = {"id": "body_armor", "name": "Armor"}
        for candidate, cure, trial in [("plate-mail", False, True), ("scale-mail", False, False),
                                       ("scale-mail", True, True), ("unrecognized", True, False)]:
            with self.subTest(candidate=candidate, cure=cure):
                suits = [{"id": name, "name": name, "class": "armor", "quantity": 1,
                          "identified": True, "identity": "item.armor." + name} for name in ("ring-mail", candidate)]
                offers = [{"item": item["id"], "slot": slot, "equipped_slot": 0 if i == 0 else None}
                          for i, item in enumerate(suits)]
                if cure:
                    suits.append({"id": "cure", "class": "scroll", "name": "remove curse", "quantity": 1,
                                  "identified": True, "identity": "item.scroll.remove-curse"})
                    offers.append({"item": "cure", "readable": True})
                turn = player_turn([cell("exit", down=True)], inventory=suits,
                                   interactions={"slots": [slot], "inventory": offers})
                self.assertIsNone(turn.inventory[1].physical_reduction)
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual((action, reason), (Unequip(turn.equipment[0]), "try_armor") if trial else
                                 (Move(Direction.DOWN), "descend"))

    def test_swaps_redundant_ring_for_slow_digestion(self):
        from tor_client_bot import Unequip
        slots = [{"id": "ring", "name": "Finger"}] * 2
        rings = [{"id": str(i), "name": name, "class": "ring", "quantity": 1, "identified": True,
                  "identity": "item.ring.ring-" + name} for i, name in enumerate(
                      ["protection", "protection", "slow-digestion"])]
        offers = [{"item": item["id"], "slot": slots[0], "equipped_slot": i if i < 2 else None}
                  for i, item in enumerate(rings)]
        turn = player_turn([cell("here", down=True)], inventory=rings,
                           interactions={"slots": slots, "inventory": offers, "curse_blocked": ["0"]})
        self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn),
                         (Unequip(turn.equipment[1]), "change_ring"))

    def test_unseen_attack_uses_a_free_sight_ring_and_keeps_a_spare_dose(self):
        from tor_client_bot import describe_action

        ring = {"id": "band", "class": "ring", "quantity": 1, "identified": True, "name": "sight ring",
                "identity": "item.ring.ring-see-invisible"}
        dose = {"id": "dose", "class": "potion", "quantity": 1, "identified": True, "name": "sight potion",
                "identity": "item.potion.see-invisible"}
        slot = {"id": "ring", "name": "Finger"}
        for equipped, cursed, expected in [(None, False, "equip"), (0, False, "move"), (None, True, "drink")]:
            with self.subTest(equipped=equipped, cursed=cursed):
                offers = [{"item": "band", "slot": slot, "equipped_slot": equipped, "known_cursed": cursed},
                          {"item": "dose", "drinkable": True}]
                turn = player_turn([{**cell("here"), "lit": True}, cell("west", -1)], inventory=[ring, dose],
                                   interactions={"slots": [slot], "inventory": offers})
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.last_unseen_attack = "here", 0
                action, _ = choose(player, turn)
                self.assertEqual(describe_action(action)["type"], expected)

    def test_melee_upgrade_compares_disclosed_damage_accuracy_and_timing(self):
        from tor_client_bot import Equip, Unequip

        slot = {"id": "weapon", "name": "Primary"}
        inventory = [{"id": name, "identity": f"item.weapon.{name}", "name": name,
                      "quantity": 1, "class": "weapon", "identified": True}
                     for name in ("mace", "long-sword", "dagger")]
        def weapon(name, damage, duration):
            attack = {"skill": {"id": "heavy_weaponry"}, "bonus": 2, "damage_enchantment": 1,
                      "wind_up": duration, "recovery": 50, "damage": {"components": [
                          {"amount": {"type": "rolled", "count": 2, "sides": damage, "bonus": 0}}]}}
            return {"item": name, "slot": slot, "known_equipment": {"attack": attack, "modifiers": []}}
        offers = [dict(weapon("mace", 3, 50), equipped_slot=0), weapon("long-sword", 6, 50),
                  weapon("dagger", 8, 300)]
        def picture():
            return player_turn([cell("here", down=True)], inventory=inventory,
                               interactions={"slots": [slot], "inventory": offers})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        turn = picture()
        self.assertEqual(turn.inventory[1].weapon_attack.mean_damage, 8)
        self.assertEqual(choose(player, turn), (Unequip(turn.equipment[0]), "change_weapon"))
        del offers[0]["equipped_slot"]
        turn = picture()
        self.assertEqual(choose(player, turn), (Equip(turn.inventory[1], turn.equipment[0]), "equip_melee"))
        offers[1]["equipped_slot"] = 0
        self.assertEqual(choose(player, picture()), (Move(Direction.DOWN), "descend"))

    def test_short_ring_detour_requires_an_upgrade_over_the_worn_ring(self):
        here, near = cell("exit", down=True), cell("ring", 1)
        slot = {"id": "ring", "name": "Finger"}
        def ring(name):
            return {"id": name, "identity": "item.ring.ring-" + name, "name": name,
                    "quantity": 1, "class": "ring", "identified": True}
        for worn, found, expected in [("searching", "slow-digestion", "seek_supplies"),
                                      ("slow-digestion", "searching", "descend"),
                                      ("searching", "searching", "descend")]:
            with self.subTest(worn=worn, found=found):
                ground = {**near, "item": {**ring(found), "id": "new"}, "reachable": False}
                turn = player_turn([here, near], inventory=[ring(worn)], ground=[ground],
                                   interactions={"slots": [slot], "inventory": [
                                       {"item": worn, "slot": slot, "equipped_slot": 0}]})
                self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)[1], expected)

    def test_nearby_armor_detour_depends_on_retrieval_phase(self):
        here, nearby = cell("exit", up=True, down=True), cell("mail", 1)
        armor = {"id": "plate", "identity": "item.armor.plate-mail", "name": "plate",
                 "quantity": 1, "class": "armor", "identified": True}
        amulet = {**armor, "id": "prize", "class": "amulet", "identity": DEFAULT_OBJECTIVE_IDENTITY}
        for pack, decision in [([], (Move(Direction.EAST), "seek_supplies")),
                               ([amulet], (Move(Direction.UP), "ascend"))]:
            turn = player_turn([here, nearby], inventory=pack, ground=[{**nearby, "item": armor, "reachable": False}])
            self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn), decision)

    def test_exploration_continues_to_a_nearby_frontier_before_doubling_back(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        west = player_turn([cell("west"), cell("here", 1)])
        player.observe(west)
        player.admitted(west, Move(Direction.EAST), reason="explore")
        cells = [cell("here"), cell("west", -1), cell("unknown-west", -2),
                 cell("east", 1), cell("far-east", 2), cell("unknown-east", 3)]
        turn = player_turn(cells)
        player.observe(turn)
        player.maps[player.depth].visited.update({"east", "far-east"})
        self.assertEqual(player.choose(turn), (Move(Direction.EAST), "explore"))
        player.maps[player.depth].visited.add("unknown-east")
        self.assertEqual(player.choose(turn), (Move(Direction.WEST), "explore"))

    def test_take_corridor_before_two_foes_make_contact(self):
        def tile(x, y, wall=False):
            return {**cell(f"{x},{y}", x), "position": {"x": x, "y": y, "z": 0}, "wall": wall}
        cells = [tile(0, 0), tile(-1, 0), tile(-2, 0), tile(-1, -1, True), tile(-1, 1, True),
                 tile(2, 0), tile(2, 1)]
        foes = [{"id": str(i), "name": "troll", "position": c["position"]} for i, c in enumerate(cells[-2:])]
        turn = player_turn(cells, actors=foes, hostiles=["0", "1"])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        self.assertEqual(choose(player, turn), (Move(Direction.WEST), "take_corridor"))
        cells[2]["wall"] = True
        turn = player_turn(cells, actors=foes, hostiles=["0", "1"])
        self.assertNotEqual(choose(player, turn)[1], "take_corridor")

    def test_player_compares_disclosed_armor_and_respects_blocked_removal(self):
        from tor_client_bot import Equip, Unequip

        here = cell("stairs", down=True)
        slot = {"id": "body_armor", "name": "Body"}
        inventory = [{"id": name, "identity": name, "identified": True, "name": name,
                      "quantity": 1, "class": "armor"} for name in ("worn", "plate", "cursed", "unknown")]
        def offer(name, amount):
            modifiers = [{"id": "reduction", "fields": [
                {"id": "scope", "value": {"value": "always"}},
                {"id": "category", "value": {"value": {"id": category}}},
                {"id": "amount", "value": {"value": str(amount)}},
            ]} for category in ("impact", "keen")]
            return {"item": name, "slot": slot, "known_equipment": {"modifiers": modifiers}}
        offers = [dict(offer("worn", 2), equipped_slot=0), offer("plate", 6),
                  dict(offer("cursed", 9), known_cursed=True), {"item": "unknown", "slot": slot}]
        interactions = {"slots": [slot], "inventory": offers}
        def picture():
            return player_turn([here], inventory=inventory, interactions=interactions)
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        turn = picture()
        self.assertEqual([i.physical_reduction for i in turn.inventory], [2, 6, 9, None])
        self.assertEqual(choose(player, turn), (Unequip(turn.equipment[0]), "upgrade_armor"))
        interactions["curse_blocked"] = ["worn"]
        self.assertEqual(choose(player, picture()), (Move(Direction.DOWN), "descend"))
        interactions["curse_blocked"] = []
        del offers[0]["equipped_slot"]
        turn = picture()
        self.assertEqual(choose(player, turn), (Equip(turn.inventory[1], turn.equipment[0]), "upgrade_armor"))
        offers[1]["equipped_slot"] = 0
        self.assertEqual(choose(player, picture()), (Move(Direction.DOWN), "descend"))

    def test_bot_extension_uses_test_profile_with_downloaded_binaries(self):
        from pathlib import Path
        from types import SimpleNamespace
        from unittest.mock import patch
        from test_rogue_bot_process import RogueBotProcess

        fixture = SimpleNamespace(bin=Path("downloaded/bin"), directory=Path("test-output"))
        for profile, cargo_profile in (("debug", "dev"), ("release", "release")):
            with self.subTest(profile=profile), patch.dict("os.environ", {"TOR_TEST_PROFILE": profile}, clear=True):
                with patch("subprocess.run") as build, patch("shutil.copyfile"), patch("shutil.copytree"):
                    RogueBotProcess.install_bot_extension(fixture)
                command = build.call_args.args[0]
                self.assertEqual(command[command.index("--profile") + 1], cargo_profile)

    def test_objective_uses_disclosed_identity_and_rejects_a_lookalike(self):
        target = {"id": "relic", "name": "Copper seal", "identity": "item.misc.copper-seal",
                  "identified": True, "quantity": "1"}
        lookalike = {**target, "id": "wrong", "identity": "item.misc.ceremonial-seal"}
        rows = [{"item": item, "reachable": True, "position": {"x": 0, "y": 0, "z": 0}}
                for item in (lookalike, target)]
        policy = Adventurer(1, "item.misc.copper-seal")
        turn = player_turn([cell("exit", up=True)], ground=rows)
        self.assertIs(turn.ground_items[1].cell, turn.cell)
        self.assertEqual(choose(policy, turn), (Pickup(turn.ground_items[1].item), "take"))
        self.assertEqual(choose(policy, player_turn([cell("exit", up=True)], inventory=[target])),
                         (Move(Direction.UP), "ascend"))

    def test_authored_growth_never_requests_a_purchase_or_balance_page(self):
        from rogue_policy_reference import buy_starting_talent, earned_training

        data = state([cell("here")])
        calls = []

        class Session:
            def observe(self):
                return type("Observation", (), {"data": lambda _: {"state": data}})()

            def advancement(self, request):
                calls.append(request)
                self.assert_overview(request)
                return {"status": "confirmed", "page": {"entries": [
                    {"value": {"locked": True, "points": {"unspent": "1"}}}]}}

            @staticmethod
            def assert_overview(request):
                if request != "overview":
                    raise AssertionError(request)

        session = Session()
        buy_starting_talent(session, 1)
        self.assertIsNone(earned_training(session.advancement, data["observation"]))
        self.assertEqual(calls, ["overview", "overview"])

    def test_actor_one_in_another_scenario_does_not_need_the_rogue_preset(self):
        from rogue_policy_reference import buy_starting_talent

        data = state([cell("here")])
        data["observation"]["combat"]["own_stats"]["hit_dice"] = {
            "total": "2", "sources": [{"source_kind": "class", "source": {"id": "mage"}, "count": "2"}],
        }

        class Session:
            def observe(self):
                return type("Observation", (), {"data": lambda _: {"state": data}})()

            def advancement(self, request):
                if request != "overview":
                    raise AssertionError(request)
                return {"status": "confirmed", "page": {"entries": [
                    {"value": {"locked": False, "points": {"unspent": "1"}}}]}}

        buy_starting_talent(Session(), 1)

    def test_ascent_routes_through_previously_disclosed_diagonals_beyond_the_next_step(self):
        item = {"id": "objective", "identity": DEFAULT_OBJECTIVE_IDENTITY, "name": AMULET,
                "quantity": 1, "identified": True}
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        corners = [cell("corner"), cell("side-x", 1), cell("side-y"), cell("stairs", 1, up=True)]
        for tile in corners[2:]:
            tile["position"]["y"] = -1
        player.observe(player_turn(corners))
        corridor = player_turn([cell("start"), cell("corner", 1)], inventory=[item])
        self.assertEqual(choose(player, corridor), (Move(Direction.EAST), "seek_stairs"))
        self.assertEqual(player.maps[1].distance("start", {"stairs"}), 2)

    def test_estimates_return_travel_on_a_route_with_a_remembered_diagonal(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
        tiles = [cell("corner"), cell("west", -1), cell("north"), cell("up", -1, up=True)]
        for tile in tiles[2:]:
            tile["position"]["y"] = -1
        player.observe(player_turn(tiles))
        upper = player_turn([cell("down", down=True), cell("corner", -1)], tick=100)
        player.observe(upper)
        player.admitted(upper, Move(Direction.DOWN))
        player.observe(player_turn([cell("lower", up=True)], tick=200))
        self.assertEqual(player.depth, 3)
        self.assertEqual(player.ascent_ticks_by_floor, {2: 300})

    def test_uses_clear_diagonals_to_reach_stairs_without_visible_enemies(self):
        room = [cell("here"), cell("west", -1), cell("north"), cell("stairs", -1, down=True)]
        for tile in room[2:]:
            tile["position"]["y"] = -1
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        self.assertEqual(choose(player, player_turn(room)), (Move(Direction.NORTH_WEST), "seek_stairs"))
        room[-1]["stairs_down"] = False
        room[-1]["stairs_up"] = True
        amulet = {"identity": DEFAULT_OBJECTIVE_IDENTITY, "name": AMULET, "class": "amulet",
                  "identified": True, "id": "carried-amulet", "quantity": "1"}
        self.assertEqual(choose(player, player_turn(room, inventory=[amulet])),
                         (Move(Direction.NORTH_WEST), "seek_stairs"))

    def test_routes_to_descent_and_confirms_stair_arrival(self):
        policy = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        self.assertEqual(choose(policy, player_turn([cell("up", up=True), cell("down", 1, down=True)])),
                         (Move(Direction.EAST), "seek_stairs"))
        reached = player_turn([cell("down", down=True), cell("up", -1, up=True)])
        down, reason = choose(policy, reached)
        self.assertEqual((down, reason), (Move(Direction.DOWN), "descend"))
        policy.admitted(reached, down)
        policy.observe(player_turn([cell("below", up=True)], tick=100))
        self.assertEqual(policy.depth, 2)

    def test_confirms_stair_arrival_beside_the_disclosed_landing(self):
        policy = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        projection = cell("landing", up=True)
        projection["position"]["z"] = -20
        before = player_turn([cell("origin", down=True), projection])
        down, _ = choose(policy, before)
        policy.admitted(before, down)
        policy.observe(player_turn([cell("adjacent"), cell("landing", 1, up=True)], tick=100))
        self.assertEqual(policy.depth, 2)
        self.assertTrue(policy.maps[2].cells["landing"].stairs_up)

    def test_receipt_phase_limits_what_a_landing_can_establish(self):
        from tor_client_bot import Result

        before = player_turn([cell("origin", down=True)])
        after = player_turn([cell("below", up=True)], tick=100)
        row = {"actor": 1, "branch": "branch", "intention": "move", "phase": "queued"}
        accepted = Result.from_reply({"status": "confirmed", "receipt": {"type": "admitted", **row}})
        attempt = before.stair_attempt(Move(Direction.DOWN), accepted)
        import json
        picture = json.loads(after.recording())
        for phase, expected in [("started", None), ("failed", False), ("resolved", True)]:
            picture["finished_intentions"] = [dict(row, phase=phase)]
            self.assertIs(Turn(picture).stair_arrival(attempt), expected)
        refused = Result.from_reply({"status": "rejected"})
        self.assertIs(after.stair_arrival(before.stair_attempt(Move(Direction.DOWN), refused)), False)

    def test_ready_picture_at_the_origin_cannot_discard_a_pending_stair(self):
        import json
        from tor_client_bot import Result

        policy = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        origin = player_turn([cell("origin", down=True)])
        move, _ = choose(policy, origin)
        row = {"actor": 1, "branch": "branch", "intention": "down", "phase": "queued"}
        receipt = Result.from_reply({"status": "confirmed", "receipt": {"type": "admitted", **row}})
        policy.admitted(origin, move, receipt)
        picture = json.loads(origin.recording())
        picture["finished_intentions"] = [row]
        pending = Turn(picture)
        policy.observe(pending)
        self.assertEqual(policy.choose(pending), (None, "uncertain_stair_landing"))
        landed = json.loads(player_turn([cell("below", up=True)], tick=100).recording())
        landed["finished_intentions"] = [dict(row, phase="resolved")]
        policy.observe(Turn(landed))
        self.assertEqual(policy.depth, 2)

    def test_body_clearance_after_a_step_counts_the_nearest_observed_cells(self):
        actors = [{"id": who, "name": who, "position": {"x": x, "y": 0, "z": 0}}
                  for who, x in [("player", 1), ("foe", 4), ("foe", 5)]]
        turn = player_turn([cell("origin")], actors=actors)
        foe, = turn.creatures
        self.assertEqual(foe.distance, 4)
        self.assertEqual(turn.distance_to(foe), 3)
        self.assertEqual(turn.distance_to(foe, from_offset=(1, 0, 0)), 2)

    def test_searches_an_unseen_side_of_a_visited_site_with_a_finite_budget(self):
        search = {"id": "rogue.search", "name": "Search", "cooldown_remaining": "0", "cost": None,
                  "basic_attack": False, "reach": {"kind": "self_only"}, "target_kind": "self_only"}
        turn = player_turn([cell("dark")], abilities=[search])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        memory = player.maps[1]
        for _ in range(4):
            decision = player.search(turn, memory, returning=False)
            self.assertIsNotNone(decision)
            action, reason = decision
            self.assertEqual((action.ability.identity, reason), ("rogue.search", "search"))
            player.admitted(turn, action)
        self.assertIsNone(player.search(turn, memory, returning=False))

    def test_mapped_room_with_only_an_entrance_gets_a_second_bounded_wall_search(self):
        ability = {"id": "rogue.search", "name": "Search", "cooldown_remaining": "0", "cost": None,
                   "basic_attack": False, "reach": {"kind": "self_only"}, "target_kind": "self_only"}
        cells = [cell("entrance", up=True)]
        for x, y in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            cells.append({**cell(f"wall-{x}-{y}"), "wall": True, "position": {"x": x, "y": y, "z": 0}})
        turn = player_turn(cells, abilities=[ability])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
        player.observe(turn)
        for _ in range(6):
            action, reason = player.choose(turn)
            self.assertEqual(reason, "search")
            player.admitted(turn, action)
        self.assertEqual(player.choose(turn), (None, "no_disclosed_route"))

    def test_search_credits_disclosed_nearby_walls_on_the_current_floor(self):
        search = {"id": "rogue.search", "name": "Search", "cooldown_remaining": "0", "cost": None,
                  "basic_attack": False, "reach": {"kind": "self_only"}, "target_kind": "self_only"}
        room = [cell("here")]
        for key, x, y, z in (("diagonal", 1, 1, 0), ("middle", -3, 2, 0),
                             ("far", 4, 0, 0), ("above", 1, 1, 1)):
            room.append({**cell(key), "wall": True, "position": {"x": x, "y": y, "z": z}})
        turn = player_turn(room, abilities=[search])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        player.admitted(turn, UseAbility(turn.abilities[0]))
        self.assertEqual(dict(player.wall_searches), {"diagonal": 1, "middle": 1})

    def test_equivalent_search_sites_are_ranked_independently_of_opaque_cell_keys(self):
        ability = {"id": "rogue.search", "name": "Search", "cooldown_remaining": "0", "cost": None,
                   "basic_attack": False, "reach": {"kind": "self_only"}, "target_kind": "self_only"}
        def decision(left, right):
            player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
            cells = [cell("here"), cell(left, -1), cell(right, 1),
                     {**cell("left-wall", -2), "wall": True}, {**cell("right-wall", 2), "wall": True}]
            turn = player_turn(cells, abilities=[ability])
            player.observe(turn)
            return player.search(turn, player.maps[1], returning=False)

        self.assertEqual(decision("a", "z"), decision("z", "a"))
        self.assertIsInstance(decision("a", "z")[0], Move)

    def test_waits_on_the_stair_for_levitation_to_end(self):
        condition = {"condition": {"id": "rogue.levitation", "name": "Levitating"},
                     "phase": "active", "remaining_ticks": "1958"}
        policy = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        stairs = [cell("down", down=True)]
        floating = player_turn(stairs, stats={"conditions": [condition]})
        self.assertEqual(choose(policy, floating), (Wait(), "wait_for_ground_contact"))
        self.assertEqual(choose(policy, player_turn(stairs)), (Move(Direction.DOWN), "descend"))

    def test_samples_an_unknown_potion_appearance_once_while_healthy(self):
        item = {"id": "a", "name": "flask", "appearance": "smoky flask", "identified": False,
                "quantity": 2, "class": "potion"}
        offer = {"item": "a", "drinkable": True}
        turn = player_turn([cell("stairs", down=True)], inventory=[item],
                           interactions={"inventory": [offer]})
        policy = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        action, reason = choose(policy, turn)
        self.assertEqual(reason, "test_unknown_potion")
        self.assertIs(action.item, turn.inventory[0])
        self.assertEqual(action.item.appearance, "smoky flask")
        policy.admitted(turn, action)
        other = player_turn([cell("stairs", down=True)], inventory=[{**item, "id": "b"}],
                            interactions={"inventory": [{**offer, "item": "b"}]})
        self.assertEqual(choose(policy, other), (Move(Direction.DOWN), "descend"))

    def test_collects_a_healing_reserve_and_bounds_unknown_supplies(self):
        potion = {"id": "flask", "name": "cerulean potion", "identified": False,
                  "quantity": 1, "class": "potion"}
        ground = {"item": potion, "reachable": True, "position": {"x": 0, "y": 0, "z": 0}}
        for identified in (False, True):
            item = {**potion, "identified": identified, "identity": "item.potion.healing"}
            turn = player_turn([cell("here", down=True)], ground=[{**ground, "item": item}])
            policy = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
            self.assertEqual(choose(policy, turn), (Pickup(turn.ground_items[0].item), "take"))
        full = [{**potion, "id": "pack", "quantity": 4}]
        turn = player_turn([cell("here", down=True)], inventory=full, ground=[ground])
        self.assertEqual(choose(policy, turn), (Pickup(turn.ground_items[0].item), "take"))
        for category in ("wand", "ring", "scroll", "potion"):
            with self.subTest(category=category):
                item = {**potion, "class": category}
                here, nearby = cell("here", down=True), cell("nearby", 1)
                stock = [{**item, "id": "stock", "quantity": 5}]
                turn = player_turn([here, nearby], inventory=stock,
                                   ground=[{**here, "item": item, "reachable": True}])
                self.assertEqual(choose(policy, turn), (Pickup(turn.ground_items[0].item), "take"))
                turn = player_turn([here, nearby], inventory=stock,
                                   ground=[{**nearby, "item": item, "reachable": False}])
                self.assertEqual(choose(policy, turn), (Move(Direction.DOWN), "descend"))

    def test_recovers_reachable_arrows_when_carrying_the_bow(self):
        policy = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        def weapon(name):
            return {"id": name, "name": name, "identity": f"item.weapon.{name}",
                    "identified": True, "quantity": 1, "class": "weapon"}
        ground = {"item": weapon("arrow"), "reachable": True, "position": {"x": 0, "y": 0, "z": 0}}
        turn = player_turn([cell("here", down=True)], inventory=[weapon("shortbow")], ground=[ground])
        self.assertEqual(choose(policy, turn), (Pickup(turn.ground_items[0].item), "take"))

    def test_collects_visible_spent_arrows_beyond_an_immediate_detour(self):
        cells = [cell(str(i), i, down=i == 0) for i in range(5)]
        bow, arrow = [{"id": name, "name": name, "identity": f"item.weapon.{name}",
                       "identified": True, "quantity": 12, "class": "weapon"} for name in ("shortbow", "arrow")]
        ground = [{**cells[-1], "item": {**arrow, "id": "spent", "quantity": 1}, "reachable": False}]
        turn = player_turn(cells, inventory=[bow, arrow], ground=ground)
        self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn),
                         (Move(Direction.EAST), "recover_arrows"))

    def test_uses_a_ready_contact_attack_instead_of_throwing_a_carried_dart(self):
        from unittest.mock import Mock
        from tor_client_bot import Shot

        foe = {"id": "emu", "name": "emu", "position": {"x": 1, "y": 0, "z": 0}}
        melee = {"id": "basic_melee", "name": "Strike", "basic_attack": True, "target_kind": "actor",
                 "reach": {"kind": "melee"}, "cooldown_remaining": "0", "cost": None}
        throw = {**melee, "id": "throw", "basic_attack": False, "target_kind": "directional_projectile",
                 "reach": {"kind": "projectile"}}
        dart = {"id": "d", "name": "dart", "identity": "item.weapon.dart", "identified": True,
                "quantity": 12, "class": "weapon"}
        turn = player_turn([cell("here"), cell("enemy", 1)], actors=[foe], hostiles=["emu"],
                           abilities=[melee, throw], inventory=[dart])
        target, = turn.hostiles
        shot = Shot(turn.abilities[1], target, (1, 0, 0), turn.inventory[0], None, 0)
        turn.ranged_attacks = Mock(return_value=[shot])
        self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn),
                         (turn.attack(target), "melee"))

    def test_replaces_empty_bow_while_an_enemy_is_visible(self):
        from tor_client_bot import Unequip

        foe = {"id": "snake", "name": "snake", "position": {"x": 3, "y": 0, "z": 0}}
        inventory = [{"id": name, "name": name, "identity": f"item.weapon.{name}",
                      "identified": True, "quantity": 1} for name in ("shortbow", "mace")]
        slot = {"id": "weapon", "name": "Weapon"}
        offers = [{"item": "shortbow", "slot": slot, "equipped_slot": 0},
                  {"item": "mace", "slot": slot, "equipped_slot": None}]
        turn = player_turn([cell("here")], actors=[foe], hostiles=["snake"], inventory=inventory,
                           interactions={"slots": [slot], "inventory": offers})
        self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn),
                         (Unequip(turn.equipment[0]), "change_weapon"))

    def test_readies_a_melee_weapon_when_a_foe_closes_even_with_arrows_left(self):
        from tor_client_bot import Unequip

        inventory = [{"id": name, "name": name, "identity": f"item.weapon.{name}",
                      "identified": True, "quantity": 1} for name in ("shortbow", "mace", "arrow")]
        slot = {"id": "weapon", "name": "Weapon"}
        offers = [{"item": "shortbow", "slot": slot, "equipped_slot": 0},
                  {"item": "mace", "slot": slot, "equipped_slot": None}]
        foe = {"id": "snake", "name": "snake", "position": cell("snake", 2)["position"]}
        turn = player_turn([cell("here")], actors=[foe], hostiles=["snake"], inventory=inventory,
                           interactions={"slots": [slot], "inventory": offers})
        self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn),
                         (Unequip(turn.equipment[0]), "change_weapon"))

    def test_stair_retreat_does_not_reverse_into_the_same_pursuit(self):
        snake = {"id": "snake", "name": "rattlesnake", "position": {"x": 2, "y": 0, "z": 0}}
        policy = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=6)
        policy.draining_creatures.add("rattlesnake")
        first = player_turn([cell("origin"), cell("detour", 1), cell("stairs", -1, up=True)],
                            actors=[snake], hostiles=["snake"])
        policy.observe(first)
        policy.admitted(first, Move(Direction.EAST), reason="retreat_to_stairs")
        later = player_turn([cell("detour"), cell("origin", -1), cell("stairs", -2, up=True)],
                            actors=[snake], hostiles=["snake"], tick=100)
        policy.observe(later)
        self.assertNotEqual(policy.combat(later), (Move(Direction.WEST), "retreat_to_stairs"))

    def test_outnumbered_player_steps_onto_nearby_downstairs_then_descends(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
        for on_stair in (False, True):
            cells = [cell("exit" if on_stair else "here", down=on_stair), cell("hall", -1)]
            if not on_stair:
                cells.append(cell("exit", 1, down=True))
            actors = []
            for x, name in enumerate(("emu", "hobgoblin")):
                square = cell(name, x - int(on_stair))
                square["position"]["y"] = 1
                cells.append(square)
                actors.append({"id": name, "name": name, "position": square["position"]})
            turn = player_turn(cells, actors=actors, hostiles=["emu", "hobgoblin"],
                               combat={"hp": "19", "max_hp": "19"})
            expected = (Move(Direction.DOWN), "retreat_downstairs") if on_stair else (
                Move(Direction.EAST), "retreat_to_stairs")
            self.assertEqual(choose(player, turn), expected)

    def test_uses_down_stair_underfoot_to_escape_a_known_draining_creature(self):
        policy = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=8)
        policy.draining_creatures.add("rattlesnake")
        snake = {"id": "snake", "name": "rattlesnake", "position": {"x": 1, "y": 0, "z": 0}}
        turn = player_turn([cell("escape", down=True), cell("away", -1), cell("snake", 1)],
                           actors=[snake], hostiles=["snake"])
        self.assertEqual(choose(policy, turn), (Move(Direction.DOWN), "retreat_downstairs"))

    def test_explores_past_a_landing_when_a_known_drainer_is_distant(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
        player.draining_creatures.add("rattlesnake")
        foe = {"id": "snake", "name": "rattlesnake", "position": cell("snake", 9)["position"]}
        turn = player_turn([cell("landing", up=True), cell("hall", -1)], actors=[foe], hostiles=["snake"],
                           combat={"hp": "25", "max_hp": "26"})
        self.assertEqual(choose(player, turn), (Move(Direction.WEST), "explore"))

    def test_does_not_reverse_an_escape_stair_at_sight_of_a_distant_foe(self):
        self.check_escape_landing(1)

    def test_remembers_a_stair_retreat_from_a_foe_outside_contact(self):
        self.check_escape_landing(2)

    def check_escape_landing(self, distance):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=6)
        snake = {"id": "bite", "name": "dragon", "position": cell("enemy", distance)["position"]}
        before = player_turn([cell("lower", up=True), cell("enemy", distance)], actors=[snake], hostiles=["bite"])
        escape, reason = choose(player, before)
        self.assertEqual((escape, reason), (Move(Direction.UP), "retreat_upstairs"))
        player.admitted(before, escape, reason=reason)
        distant = {**snake, "id": "other", "position": cell("foe", 3)["position"]}
        cells = [cell("upper", down=True), cell("away", -1), cell("foe", 3)]
        after = player_turn(cells, tick=100, actors=[distant], hostiles=["other"])
        self.assertNotEqual(choose(player, after)[0], Move(Direction.DOWN))
        quiet = player_turn(cells, tick=200)
        self.assertNotEqual(choose(player, quiet)[0], Move(Direction.DOWN))
        later = player_turn(cells, tick=2000, actors=[distant], hostiles=["other"])
        self.assertEqual(choose(player, later), (Move(Direction.DOWN), "retreat_downstairs"))

    def test_takes_the_nearby_down_stair_route_away_from_a_draining_pursuer(self):
        foe = {"id": "poison", "name": "rattlesnake", "position": {"x": 0, "y": -1, "z": 0}}
        room = [cell("here"), cell("escape", 1, down=True), cell("corridor", -1), cell("up", -2, up=True),
                {**cell("snake"), "position": foe["position"]}]
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=8)
        player.draining_creatures.add("rattlesnake")
        turn = player_turn(room, actors=[foe], hostiles=["poison"])
        self.assertEqual(choose(player, turn), (Move(Direction.EAST), "retreat_to_stairs"))

    def test_unrelated_up_stair_cannot_confirm_the_expected_landing(self):
        policy = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        landing = cell("paired", up=True)
        landing["position"]["z"] = -20
        before = player_turn([cell("origin", down=True), landing])
        down, _ = choose(policy, before)
        policy.admitted(before, down)
        ambiguous = player_turn([cell("unrelated", up=True)], tick=100)
        policy.observe(ambiguous)
        self.assertEqual(policy.depth, 1)
        self.assertNotIn(2, policy.maps)
        self.assertNotIn("unrelated", policy.maps[1].cells)
        self.assertEqual(policy.choose(ambiguous), (None, "uncertain_stair_landing"))
        established = player_turn([cell("paired", up=True)], tick=101)
        policy.observe(established)
        policy.observe(established)
        self.assertEqual(policy.depth, 2)
        self.assertIn("paired", policy.maps[2].visited)

    def test_retrieves_amulet_and_returns_through_known_stairs(self):
        policy = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        item = {"identity": DEFAULT_OBJECTIVE_IDENTITY, "name": AMULET, "id": "disclosed-amulet",
                "quantity": "1", "class": "amulet", "identified": True}
        ground = {**cell("near", 1), "item": item, "reachable": True}
        room = [cell("here"), cell("entrance", 1, up=True)]
        turn = player_turn(room, ground=[ground])
        self.assertEqual(choose(policy, turn), (Pickup(turn.ground_items[0].item), "take"))
        self.assertEqual(choose(policy, player_turn(room, inventory=[item])), (Move(Direction.EAST), "seek_stairs"))
        self.assertEqual(choose(policy, player_turn([cell("entrance", up=True)], inventory=[item])),
                         (Move(Direction.UP), "ascend"))

    def test_hostility_comes_from_combat_disclosure(self):
        policy = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        actor = {"id": "foe", "name": "kobold", "position": {"x": 1, "y": 0, "z": 0}}
        melee = {"id": "strike", "name": "Strike", "cooldown_remaining": "0", "cost": None,
                 "basic_attack": True, "reach": {"kind": "melee"}, "target_kind": "actor"}
        tiles = [cell("here"), cell("near", 1)]
        turn = player_turn(tiles, actors=[actor], hostiles=["foe"], abilities=[melee])
        self.assertEqual(choose(policy, turn), (UseAbility(turn.basic_attack, turn.hostiles[0]), "melee"))
        peaceful = player_turn(tiles, actors=[actor], abilities=[melee])
        self.assertFalse(peaceful.hostiles)
        self.assertNotIsInstance(choose(policy, peaceful)[0], UseAbility)

    def test_retaliates_against_a_diagonal_foe_when_both_approaches_are_open(self):
        foe = {"id": "foe", "position": {"x": 1, "y": 1, "z": 0}}
        side = cell("south")
        side["position"]["y"] = 1
        target = cell("target", 1)
        target["position"]["y"] = 1
        melee = {"id": "basic_melee", "cooldown_remaining": "0", "cost": None}
        room = [cell("here"), cell("east", 1), side, target]
        data = state(room, actors=[foe], hostiles=["foe"], abilities=[melee])
        data["observation"]["combat"].update(hp="1", max_hp="10")
        policy = Policy()
        self.assertEqual(policy.choose(data)[1], "melee")
        side["wall"] = True
        self.assertEqual(policy.choose(data), ({"type": "wait"}, "hold_position"))

    def test_one_point_of_regeneration_during_pursuit_does_not_restart_melee(self):
        foe = {"id": "hunter", "name": "medusa", "position": {"x": 1, "y": 0, "z": 0}}
        melee = {"id": "basic_melee", "name": "Attack", "cooldown_remaining": "0", "cost": None,
                 "basic_attack": True, "reach": {"kind": "melee"}, "target_kind": "actor"}
        def pursued(hp, tick):
            return player_turn([cell("here"), cell("away", -1), cell("enemy", 1)], actors=[foe],
                               hostiles=["hunter"], abilities=[melee], tick=tick,
                               combat={"hp": str(hp), "max_hp": "69"})

        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        first = pursued(34, 100)
        step, reason = choose(player, first)
        self.assertEqual((step, reason), (Move(Direction.WEST), "retreat"))
        player.admitted(first, step, reason=reason)
        self.assertEqual(choose(player, pursued(35, 200)), (Move(Direction.WEST), "retreat"))
        self.assertEqual(choose(player, pursued(56, 300))[1], "melee")

    def test_newly_seen_medusa_inherits_the_damage_estimate_from_an_earlier_encounter(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        room = [cell("here"), cell("away", -1), cell("foe", 1)]
        def scene(target, hp, events=()):
            enemy = {"id": target, "name": "medusa", "position": {"x": 1, "y": 0, "z": 0}}
            return player_turn(room, actors=[enemy], hostiles=[target],
                               combat={"hp": hp, "max_hp": 69, "events": events})
        player.observe(scene("first", 69))
        player.observe(scene("first", 51, [{"type": "ability", "target": "player",
                                            "caster": "first", "outcome": "applied"}]))
        self.assertEqual(choose(player, scene("second", 35)), (Move(Direction.WEST), "retreat"))

    def test_makes_a_short_safe_detour_for_a_visible_unidentified_potion(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        potion = {"id": "dose", "class": "potion", "name": "blue potion",
                  "quantity": 1, "identified": False}
        def sight(distance):
            tiles = [cell("stairs", down=True)] + [cell(str(x), x) for x in range(1, distance + 1)]
            ground = [{**tiles[-1], "item": potion, "reachable": False}]
            return player_turn(tiles, ground=ground)
        self.assertEqual(choose(player, sight(3)), (Move(Direction.EAST), "seek_supplies"))
        self.assertEqual(choose(player, sight(4)), (Move(Direction.DOWN), "descend"))

    def test_collects_a_safe_potion_and_remembers_the_guarded_scroll_after_stepping_back(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        potion = {"id": "blue", "class": "potion", "name": "blue potion", "quantity": 1, "identified": False}
        scroll = {**potion, "id": "roll", "name": "scroll", "class": "scroll"}
        watcher = {"id": "foe", "name": "ice monster", "position": {"x": 2, "y": 0, "z": 0}}
        room = [cell("potion"), cell("scroll", 1), cell("guard", 2)]
        turn = player_turn(room, actors=[watcher], hostiles=["foe"],
                           ground=[{**room[0], "item": potion, "reachable": True},
                                   {**room[1], "item": scroll, "reachable": False}])
        self.assertEqual(choose(player, turn), (Pickup(turn.ground_items[0].item), "take"))
        stairs = [cell("stairs", down=True), cell("potion", 1), cell("scroll", 2)]
        later = player_turn(stairs, ground=[{**stairs[2], "item": scroll, "reachable": False}])
        self.assertEqual(choose(player, later), (Move(Direction.DOWN), "descend"))

    def test_prefers_a_known_teleport_to_another_step_in_a_lethal_pursuit(self):
        from tor_client_bot import ItemUse, UseItem

        foe = {"id": "hunter", "name": "troll", "position": {"x": 1, "y": 0, "z": 0}}
        scroll = {"id": "escape", "name": "renamed scroll", "identity": "item.scroll.teleportation",
                  "class": "scroll", "identified": True, "quantity": 1}
        turn = player_turn([cell("here"), cell("away", -1), cell("foe", 1)], actors=[foe],
                           hostiles=["hunter"], inventory=[scroll], combat={"hp": 17, "max_hp": 59},
                           interactions={"inventory": [{"item": "escape", "readable": True}]})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observed_loss["hunter"] = 17
        self.assertEqual(choose(player, turn), (UseItem(turn.inventory[0], ItemUse.READ), "escape_pursuit"))

    def test_uses_a_recognized_escape_wand_on_a_lethal_adjacent_enemy(self):
        from tor_client_bot import Zap

        foe = {"id": "fang", "name": "rattlesnake", "position": {"x": 1, "y": 0, "z": 0}}
        item = {"id": "staff", "class": "wand", "name": "displacing staff", "quantity": 1,
                "identified": True, "identity": "item.wand.wand-teleport-away"}
        offer = {"item": "staff", "zappable": True, "device_activation": "direction", "charges": None}
        turn = player_turn([cell("here"), cell("snake", 1)], actors=[foe], hostiles=["fang"],
                           inventory=[item], combat={"hp": 5, "max_hp": 39},
                           interactions={"inventory": [offer]})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observed_loss["fang"] = 7
        action, reason = choose(player, turn)
        self.assertEqual((action, reason), (Zap(turn.devices[0], turn.cell_at((1, 0, 0))), "displace_threat"))
        player.admitted(turn, action, reason=reason)
        self.assertNotIsInstance(player.choose(turn)[0], Zap)

    def test_when_pinned_attacks_the_equally_healthy_foe_blocking_the_down_stair(self):
        melee = {"id": "basic_melee", "name": "Attack", "cooldown_remaining": "0", "cost": None,
                 "basic_attack": True, "reach": {"kind": "melee"}, "target_kind": "actor"}
        room = [cell("here"), cell("plain", 1), cell("exit", -1, down=True)]
        enemies = [{"id": name, "name": "rattlesnake", "position": tile["position"]}
                   for name, tile in zip(["other", "blocker"], room[1:])]
        turn = player_turn(room, actors=enemies, hostiles=["other", "blocker"], abilities=[melee],
                           combat={"hp": 7, "max_hp": 42})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=8)
        action, reason = choose(player, turn)
        self.assertEqual((reason, action.target.target), ("melee", "blocker"))
        self.assertEqual(turn.cells_of(action.target), (turn.cell_at((-1, 0, 0)),))

    def test_control_a_dangerous_approach_or_step_to_an_exit(self):
        from tor_client_bot import Zap
        here, lane, distant = cell("here"), cell("lane", 1), cell("enemy", 2)
        foe = {"id": "troll", "name": "dragon", "position": distant["position"]}
        wand = {"id": "slow", "class": "wand", "identity": "item.wand.wand-slow",
                "identified": True, "name": "slow staff", "quantity": 1}
        offer = {"item": "slow", "zappable": True, "device_activation": "direction", "charges": 3}
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=4)
        player.observed_loss_by_name["dragon"] = 7
        def picture(extra=(), actors=()):
            return player_turn([here, lane, distant, *extra], actors=[foe, *actors], hostiles=["troll"],
                               inventory=[wand], interactions={"inventory": [offer]})
        turn = picture()
        action, reason = choose(player, turn)
        self.assertEqual((action, reason), (Zap(turn.devices[0], turn.cell_at((2, 0, 0))), "slow_approach"))
        exit_turn = picture([cell("escape", -1, up=True)])
        self.assertEqual(choose(player, exit_turn), (Move(Direction.WEST), "retreat_to_stairs"))
        blocked = picture(actors=[{"id": "bystander", "name": "friend", "position": lane["position"]}])
        self.assertNotIsInstance(choose(player, blocked)[0], Zap)
        player.admitted(turn, action, reason=reason)
        self.assertNotIsInstance(choose(player, turn)[0], Zap)

    def test_elemental_wand_requires_a_recognized_effect_and_an_available_charge(self):
        cells = [cell(str(i), i) for i in range(6)]
        foe = {"id": "foe", "name": "hunter", "position": cells[3]["position"]}
        for known, charges, fires in [(True, 1, True), (True, None, True), (True, 0, False), (False, 2, False)]:
            with self.subTest(known=known, charges=charges):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                wand = {"id": "rod", "class": "wand", "name": "rod", "quantity": 1, "identified": known,
                        "identity": "item.wand.wand-cold" if known else None}
                offer = {"item": "rod", "zappable": True, "device_activation": "direction", "charges": charges}
                turn = player_turn(cells, actors=[foe], hostiles=["foe"], inventory=[wand],
                                   interactions={"inventory": [offer]})
                action, reason = choose(player, turn)
                self.assertEqual(reason == "elemental_ray", fires)
                if fires and charges is None:
                    player.admitted(turn, action, reason=reason)
                    self.assertNotEqual(player.choose(turn)[1], "elemental_ray")

    def test_polymorph_is_reserved_for_a_dangerous_uninjured_approach(self):
        wand = {"id": "staff", "class": "wand", "identity": "item.wand.wand-polymorph",
                "identified": True, "name": "staff of polymorph", "quantity": 1}
        offer = {"item": "staff", "zappable": True, "device_activation": "direction_or_self", "charges": 3}
        cells = [cell("here"), cell("lane", 1), cell("foe", 2)]
        foe = {"id": "hunter", "name": "griffin", "position": cells[-1]["position"]}
        for damage, injury, expected in [(10, "healthy", True), (5, "healthy", False), (10, "near_death", False)]:
            with self.subTest(damage=damage, injury=injury):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observed_loss_by_name["griffin"] = damage
                turn = player_turn(cells, actors=[foe], inventory=[wand], interactions={"inventory": [offer]},
                                   combat={"actors": [{"actor": "hunter", "hostile": True, "injury": injury}]})
                action, reason = choose(player, turn)
                self.assertEqual(reason == "polymorph_threat", expected)
                if expected:
                    self.assertEqual(action.target, turn.cell_at((2, 0, 0)))
                    player.admitted(turn, action, reason=reason)
                    self.assertNotEqual(choose(player, turn)[1], reason)

    def test_only_a_witnessed_resisted_polymorph_permits_one_measured_retry(self):
        from tor_client_bot import CombatReport

        cells = [cell("here"), cell("enemy", 1)]
        wand = {"id": "staff", "class": "wand", "identity": "item.wand.wand-polymorph", "identified": True,
                "quantity": 1, "name": "staff"}
        foe = {"id": "hunter", "name": "griffin", "position": cells[1]["position"]}
        own_cast = {"type": "ability", "caster": "player", "target": "hunter",
                    "ability": {"id": "rogue.wand_polymorph"}, "outcome": "unaffected"}
        for fields, charges, retry in [
            ({}, 3, True), ({}, None, False), ({"caster": "other"}, 3, False),
            ({"target": "other"}, 3, False), ({"outcome": "applied"}, 3, False),
            ({"ability": None}, 3, False),
        ]:
            with self.subTest(fields=fields, charges=charges):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                turn = player_turn(cells, actors=[foe], inventory=[wand],
                                   interactions={"inventory": [{"item": "staff", "zappable": True,
                                     "device_activation": "direction_or_self", "charges": charges}]},
                                   combat={"actors": [{"actor": "hunter", "hostile": True, "injury": "healthy"}]})
                player.observed_loss_by_name["griffin"] = 10
                action, reason = choose(player, turn)
                self.assertEqual(reason, "polymorph_threat")
                player.admitted(turn, action, reason=reason)
                report = CombatReport("player", ({**own_cast, **fields},))
                player.observe(turn, report)
                action, reason = player.choose(turn)
                self.assertEqual(reason == "polymorph_threat", retry)
                if retry:
                    player.admitted(turn, action, reason=reason)
                    player.observe(turn, report)
                    self.assertNotEqual(player.choose(turn)[1], "polymorph_threat")

    def test_polymorph_targets_the_lethal_troll_while_another_foe_is_in_contact(self):
        from tor_client_bot import CombatReport

        wand = {"id": "staff", "class": "wand", "identity": "item.wand.wand-polymorph",
                "identified": True, "name": "staff of polymorph", "quantity": 1}
        offer = {"item": "staff", "zappable": True, "device_activation": "direction_or_self", "charges": 3}
        for distance in (1, 2):
            with self.subTest(distance=distance):
                cells = [cell("aquator", -1), cell("here"), cell("lane", 1), cell("beyond", 2)]
                foes = [{"id": name, "name": name, "position": cell(name, x)["position"]}
                        for name, x in (("aquator", -1), ("troll", distance))]
                combat = {"hp": "13", "max_hp": "56", "actors": [
                    {"actor": f["id"], "hostile": True, "injury": "healthy"} for f in foes]}
                turn = player_turn(cells, actors=foes, inventory=[wand],
                                   interactions={"inventory": [offer]}, combat=combat)
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observed_loss_by_name["troll"] = 19
                action, reason = choose(player, turn)
                self.assertEqual(reason, "polymorph_threat")
                self.assertEqual(action.target, turn.cell_at((distance, 0, 0)))
                player.admitted(turn, action, reason=reason)
                resisted = {"type": "ability", "ability": {"id": "rogue.wand_polymorph"},
                            "caster": "player", "target": "troll", "outcome": "unaffected"}
                player.observe(turn, CombatReport("player", (resisted,)))
                self.assertNotEqual(player.choose(turn)[1], "polymorph_threat")
                alone = player_turn(cells, actors=foes[1:], inventory=[wand],
                                    interactions={"inventory": [offer]}, combat=combat)
                self.assertEqual(choose(player, alone)[1], "polymorph_threat")

    def test_weak_player_eats_a_carried_ration_during_a_prolonged_pursuit(self):
        from tor_client_bot import ItemUse, UseItem

        enemy = {"id": "foe", "name": "centaur", "position": {"x": 1, "y": 0, "z": 0}}
        ration = {"id": "food", "name": "ration", "identity": "item.food.ration",
                  "class": "food", "identified": True, "quantity": "1"}
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, recovering_from_pursuit=True)
        turn = player_turn([cell("here"), cell("away", -1), cell("foe", 1)], actors=[enemy],
                           hostiles=["foe"], combat={"hp": "24", "max_hp": "35"}, inventory=[ration],
                           interactions={"inventory": [{"item": "food", "edible": True}]},
                           stats={"nutrition": {"hunger": "weak"}})
        self.assertEqual(choose(player, turn), (UseItem(turn.inventory[0], ItemUse.EAT), "eat"))

    def test_wounded_pursuit_uses_a_remembered_exit_before_running_another_lap(self):
        foe = {"id": "pursuer", "name": "centaur", "position": {"x": 1, "y": 0, "z": 0}}
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=10, recovering_from_pursuit=True)
        turn = player_turn([cell("here", up=True), cell("clear", -1), cell("enemy", 1)],
                           actors=[foe], hostiles=["pursuer"], combat={"hp": "24", "max_hp": "35"})
        self.assertEqual(choose(player, turn), (Move(Direction.UP), "retreat_upstairs"))

    def test_stair_approach_breaks_dangerous_contact_when_there_is_room(self):
        foe = {"id": "centaur", "name": "centaur", "position": {"x": 1, "y": 1, "z": 0}}
        for health, side_open, expected in [(9, True, Direction.WEST), (9, False, Direction.EAST),
                                            (35, True, Direction.EAST)]:
            with self.subTest(health=health, side_open=side_open):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=13, recovering_from_pursuit=True)
                player.observed_loss_by_name["centaur"] = 16
                cells = [cell("here"), cell("stairs", 1, up=True), {**cell("enemy"), "position": foe["position"]}]
                if side_open:
                    cells.append(cell("side", -1))
                turn = player_turn(cells, actors=[foe], hostiles=["centaur"],
                                   combat={"hp": str(health), "max_hp": "35"})
                player.observe(turn)
                player.recovering_from_pursuit = True
                self.assertEqual(player.retreat_to_stairs(turn)[0], Move(expected))

    def test_a_retreat_can_open_a_door_or_break_contact_before_opening_it(self):
        from tor_client_bot import OpenDoor
        door = {"id": "gate", "open": False, "reachable": True}
        obstacle = {**cell("threshold", 1), "door": door}
        enemy = {"id": "griffin", "name": "griffin", "position": {"x": 0, "y": 1, "z": 0}}
        for can_evade in (False, True):
            player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
            room = [cell("here"), obstacle, cell("stairs", 2, up=True), {**cell("foe"), "position": enemy["position"]}]
            if can_evade:
                room.append({**cell("side"), "position": {"x": 0, "y": -1, "z": 0}})
            turn = player_turn(room, actors=[enemy], hostiles=["griffin"])
            player.observed_loss_by_name["griffin"] = 20
            action, reason = choose(player, turn)
            if can_evade:
                self.assertEqual(reason, "retreat")
                self.assertFalse(turn.in_contact(turn.hostiles[0], from_offset=action.direction.offset))
            else:
                self.assertEqual((action, reason), (OpenDoor(turn.cell_at((1, 0, 0)).door), "retreat_to_stairs"))

    def test_retreat_rejects_a_diagonal_tile_still_in_melee_reach(self):
        foe = {"id": "foe", "position": {"x": 0, "y": 1, "z": 0}}
        enemy_tile = cell("enemy")
        enemy_tile["position"]["y"] = 1
        corner = cell("corner", 1)
        corner["position"]["y"] = 1
        melee = {"id": "basic_melee", "cooldown_remaining": "0", "cost": None}
        room = [cell("here"), cell("false_exit", 1), enemy_tile, corner]
        data = state(room, actors=[foe], hostiles=["foe"], abilities=[melee])
        data["observation"]["combat"].update(hp="1", max_hp="10")
        self.assertEqual(Policy().choose(data)[1], "melee")

    def test_reduces_multiple_melee_contacts_even_without_a_clear_exit(self):
        def at(key, x, y):
            result = cell(key, x)
            result["position"]["y"] = y
            return result

        foes = [{"id": "east", "position": {"x": 1, "y": 0, "z": 0}},
                {"id": "north", "position": {"x": 0, "y": -1, "z": 0}}]
        room = [at("here", 0, 0), at("east", 1, 0), at("north", 0, -1),
                at("south", 0, 1), at("corner", 1, 1)]
        melee = {"id": "basic_melee", "cooldown_remaining": "0", "cost": None}
        data = state(room, actors=foes, hostiles=["east", "north"], abilities=[melee])
        self.assertEqual(Policy().choose(data), ({"type": "move", "direction": "south"}, "retreat"))

    def test_public_policy_steps_out_of_one_of_two_melee_contacts(self):
        def tile(x, y):
            result = cell(f"{x},{y}", x)
            result["position"]["y"] = y
            return result

        room = [tile(x, y) for x, y in [(0, 0), (1, 0), (0, -1), (0, 1), (1, 1)]]
        foes = [{"id": name, "name": name, "position": {"x": x, "y": y, "z": 0}}
                for name, x, y in [("east", 1, 0), ("north", 0, -1)]]
        turn = player_turn(room, actors=foes, hostiles=["east", "north"], combat={"hp": 9, "max_hp": 58})
        self.assertEqual(sum(turn.in_contact(foe) for foe in turn.hostiles), 2)
        self.assertEqual(sum(turn.in_contact(foe, from_offset=(0, 1, 0)) for foe in turn.hostiles), 1)
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        self.assertEqual(choose(player, turn), (Move(Direction.SOUTH), "retreat"))

    def test_multicell_foe_counts_as_one_melee_contact(self):
        foe = {"id": "foe", "position": {"x": 1, "y": 0, "z": 0}}
        top = {**foe, "position": {"x": 1, "y": 0, "z": 1}}
        overhead = cell("overhead")
        overhead["position"]["z"] = 1
        melee = {"id": "basic_melee", "cooldown_remaining": "0", "cost": None}
        data = state([cell("escape", -1), cell("here"), cell("foe", 1), overhead],
                     actors=[foe, top], hostiles=["foe"], abilities=[melee])
        self.assertEqual(Policy().choose(data)[1], "melee")

    def test_prioritizes_a_disclosed_near_death_foe_when_cornered(self):
        north = cell("north")
        north["position"]["y"] = -1
        east = cell("east", 1)
        foes = [{"id": position["key"], "position": position["position"]} for position in (north, east)]
        melee = {"id": "strike", "name": "Strike", "cooldown_remaining": "0", "cost": None,
                 "basic_attack": True, "reach": {"kind": "melee"}, "target_kind": "actor"}
        for foe in foes:
            foe["name"] = "opponent"
        turn = player_turn([cell("here"), north, east], actors=foes, abilities=[melee],
                           combat={"actors": [{"actor": "north", "injury": "healthy", "hostile": True},
                                              {"actor": "east", "injury": "near_death", "hostile": True}]})
        action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
        self.assertEqual(reason, "melee")
        self.assertEqual(action.target.target, "east")

    def test_bear_trap_prevents_retreat_but_allows_melee_until_it_expires(self):
        foe = {"id": "foe", "name": "Wraith", "position": {"x": 1, "y": 0, "z": 0}}
        melee = {"id": "basic_melee", "cooldown_remaining": "0", "cost": None}
        data = state([cell("escape", -1), cell("here"), cell("foe", 1)], actors=[foe],
                     hostiles=["foe"], abilities=[melee])
        stats = data["observation"]["combat"]["own_stats"]
        stats["conditions"] = [{"condition": {"id": "rogue.bear_trap"}, "phase": "active",
                                "remaining_ticks": "200"}]
        policy = Policy()
        self.assertEqual(policy.choose(data)[1], "melee")
        stats["conditions"] = []
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "west"}, "retreat"))

    def test_waits_for_bear_trap_to_release_before_descending(self):
        trap = {"condition": {"id": "rogue.bear_trap", "name": "Bear trap"}, "phase": "active",
                "remaining_ticks": "100", "appearance": "status_indicator", "cue": "status"}
        tiles = [cell("stairs", down=True)]
        policy = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        self.assertEqual(choose(policy, player_turn(tiles, stats={"conditions": [trap]})),
                         (Wait(), "wait_for_restraint"))
        self.assertEqual(choose(policy, player_turn(tiles)), (Move(Direction.DOWN), "descend"))

    def test_stalled_campaign_reports_the_last_picture_and_the_admitted_stair_phase(self):
        import json
        from unittest.mock import Mock
        from rogue_player import play
        from tor_client_bot import ReadinessTimeout, Session

        actor = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        transport = Mock()
        ready = Turn(json.loads(player_turn([cell("stair", down=True)]).recording()),
                     connection=transport, snapshot=Mock())
        receipt = {"type": "admitted", "actor": 1, "branch": "game", "intention": "down", "phase": "queued"}
        transport.act.return_value = {"status": "confirmed", "receipt": receipt}
        data = json.loads(player_turn([cell("stair", down=True)], tick=100).recording())
        data["readiness"] = {"admission": False}
        data["intentions"] = [dict(receipt, phase="started")]
        session = Session(Mock())
        session.wait_for_decision = Mock(side_effect=[ready, ReadinessTimeout(Turn(data))])
        report = play(session, actor, 10, lambda row: None, record_decisions=True)
        self.assertEqual((report["terminal"], report["tick"], report["actions"]), ("readiness_timeout", 100, 1))
        self.assertEqual((report["previous_intention_id"], report["previous_intention_phase"]), ("down", "started"))
        self.assertFalse(Turn.replay(report["recording"]).can_act)

    def test_cancels_only_its_own_stalled_queued_stair_and_waits_once_before_retry(self):
        import json
        from tor_client_bot import ActivityChoice, Result

        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        landing = player_turn([cell("stair", down=True)])
        move, _ = choose(player, landing)
        receipt = {"type": "admitted", "actor": 1, "branch": "game", "intention": "down", "phase": "queued"}
        result = Result.from_reply({"status": "confirmed", "receipt": receipt})
        player.admitted(landing, move, result)
        data = json.loads(landing.recording())
        data.update(can_cancel=True, intentions=[receipt], readiness={"admission": False})
        blocked = Turn(data)
        self.assertEqual(player.stalled(blocked, None), (None, "readiness_timeout"))
        self.assertEqual(player.stalled(blocked, result), (ActivityChoice.CANCEL, "cancel_stalled_stair"))
        player.admitted(blocked, ActivityChoice.CANCEL, reason="cancel_stalled_stair")
        clear = Turn({**data, "intentions": [], "finished_intentions": [dict(receipt, phase="cancelled")],
                      "can_cancel": False, "readiness": {"admission": True}})
        self.assertEqual(choose(player, clear), (Wait(), "wait_after_cancel"))
        player.admitted(clear, Wait(), reason="wait_after_cancel")
        self.assertEqual(choose(player, landing), (move, "descend"))
        player.admitted(landing, move, result)
        self.assertEqual(player.stalled(blocked, result), (None, "readiness_timeout"))

    def test_victory_on_last_budgeted_action_is_reported(self):
        from unittest.mock import Mock
        from rogue_player import play
        from tor_client_bot import Outcome, Result

        objective = {"id": "amulet", "name": AMULET, "identity": DEFAULT_OBJECTIVE_IDENTITY,
                     "quantity": 1, "identified": True}
        event = {"type": "ability", "target": "player", "caster": "foe", "outcome": "miss"}
        from json import loads
        def observed(turn):
            picture = loads(turn.recording())
            picture["combat_updates"] = [{"cursor": {"sequence": "1"}, "events": [event]}]
            return Turn(picture)

        ongoing = observed(player_turn([cell("exit", up=True)], inventory=[objective]))
        finished = observed(player_turn([], combat={"terminal": True, "victory": True}))
        ongoing.submit = Mock(return_value=Result(Outcome.CONFIRMED, None, None))
        from tor_client_bot import Session
        session = Session(Mock())
        session.wait_for_decision = Mock(side_effect=[ongoing, finished])
        trace = []
        report = play(session, Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), 1, trace.append)
        self.assertEqual(report["terminal"], "victory")
        self.assertTrue(report["amulet_acquired"])
        self.assertEqual((report["actions"], report["max_depth"]), (1, 1))
        ongoing.submit.assert_called_once_with(Move(Direction.UP))
        self.assertEqual(trace[0]["events_since_previous_action"], [event])
        self.assertEqual(report["events_since_previous_action"], [])

    def test_a_terminal_only_attachment_reports_the_carried_objective(self):
        from unittest.mock import Mock
        from rogue_player import play
        from tor_client_bot import Session

        item = {"id": "amulet", "name": AMULET, "identity": DEFAULT_OBJECTIVE_IDENTITY,
                "quantity": 1, "identified": True}
        terminal = player_turn([], inventory=[item], combat={"terminal": True, "victory": True})
        session = Session(Mock())
        session.wait_for_decision = Mock(return_value=terminal)
        report = play(session, Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), 0, self.fail)
        self.assertEqual(report["terminal"], "victory")
        self.assertTrue(report["amulet_acquired"])
        self.assertEqual(report["actions"], 0)

    def test_terminal_without_death_or_victory_is_reported_as_terminal(self):
        from unittest.mock import Mock
        from rogue_player import play

        for dead, expected in [(True, "death"), (False, "terminal")]:
            with self.subTest(dead=dead):
                from tor_client_bot import Session
                session = Session(Mock())
                session.wait_for_decision = Mock()
                session.wait_for_decision.return_value = player_turn(
                    [], combat={"terminal": True, "dead": dead, "victory": False})
                report = play(session, Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), 0, self.fail)
                self.assertEqual(report["terminal"], expected)

    def test_runner_defers_failed_projectile_until_time_advances(self):
        from contextlib import redirect_stdout
        from io import StringIO
        from rogue_policy_reference import run

        class ProjectilePolicy(Policy):
            def choose(self, data, *, projectile_choices=None, chart=None):
                if projectile_choices is not None:
                    return {"type": "wait"}, "throw_projectile"
                return {"type": "wait"}, "advance_after_failed_projectile"

        class Session:
            tick = 100
            actions = 0

            def observe(self):
                picture = state([cell("here")])
                picture["observation"]["tick"] = str(self.tick)
                payload = {"state": picture, "cursor": self.actions,
                           "readiness": {"admission": True},
                           "finished_intentions": [{"intention": "shot", "phase": "failed"}]}
                return type("Observation", (), {"data": lambda _: payload})()

            def projectile_choices(self, *args):
                raise AssertionError("The fixture policy does not need an actual query")

            def act(self, action, observation):
                self.actions += 1
                if self.actions > 1:
                    self.tick += 100
                return {"status": "confirmed", "receipt": {"intention": "shot", "phase": "queued"}}

        session = Session()
        with redirect_stdout(StringIO()) as output:
            self.assertEqual(run(session, ProjectilePolicy(), budget=3), "action_budget")
        import json
        rows = [json.loads(line) for line in output.getvalue().splitlines()]
        self.assertEqual([row["reason"] for row in rows],
                         ["throw_projectile", "advance_after_failed_projectile", "throw_projectile"])
        self.assertEqual(rows[1]["previous_intention_phase"], "failed")

    def test_explores_a_disclosed_diagonal_frontier_in_one_move(self):
        room = [cell("here"), cell("west", -1), cell("north"), cell("frontier", -1)]
        for tile in room[2:]:
            tile["position"]["y"] = -1
        turn = player_turn(room)
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        player.maps[1].visited.update({"west", "north"})
        self.assertEqual(player.choose(turn), (Move(Direction.NORTH_WEST), "explore"))
        room[1]["door"] = {"open": False, "reachable": True, "id": "door"}
        self.assertNotEqual(choose(player, player_turn(room))[0], Move(Direction.NORTH_WEST))

    def test_retreats_diagonally_when_both_cardinal_steps_stay_in_melee_range(self):
        foes = [{"id": name, "name": name, "position": {"x": x, "y": y, "z": 0}}
                for name, x, y in [("east", 1, 0), ("south", 0, 1)]]
        room = []
        for x in (-1, 0, 1):
            for y in (-1, 0, 1):
                tile = cell(f"{x}-{y}", x)
                tile["position"]["y"] = y
                room.append(tile)
        policy = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        def decision():
            return choose(policy, player_turn(room, actors=foes, hostiles=["east", "south"]))
        self.assertEqual(decision(), (Move(Direction.NORTH_WEST), "retreat"))
        room[1]["wall"] = True
        self.assertNotEqual(decision()[0], Move(Direction.NORTH_WEST))

    def test_heals_earlier_when_two_enemies_are_in_melee_range(self):
        foes = [{"id": name, "name": name, "position": {"x": x, "y": 0, "z": 0}}
                for name, x in [("east", 1), ("west", -1)]]
        potion = {"id": "heal", "name": "elixir", "identity": "item.potion.healing",
                  "identified": True, "quantity": 1}
        for hp, healing in [(7, True), (8, False)]:
            with self.subTest(hp=hp):
                turn = player_turn([cell("here"), cell("west", -1), cell("east", 1)], actors=foes,
                                   hostiles=["east", "west"], inventory=[potion], combat={"hp": str(hp)},
                                   interactions={"inventory": [{"item": "heal", "drinkable": True}]})
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual(reason == "heal", healing)
                if healing:
                    self.assertEqual(action.item.target, "heal")

    def test_combines_exposure_from_distinct_foes_for_healing_and_attack_cancellation(self):
        left, right = cell("left", -1), cell("right", 1)
        actors = [{"id": "left", "position": left["position"]},
                  {"id": "right", "position": right["position"]},
                  {"id": "right", "position": {"x": 1, "y": 0, "z": 1}}]
        potion = {"id": "potion", "name": "potion of healing", "identity": "item.potion.healing", "identified": True}
        data = state([cell("here"), left, right], actors=actors, hostiles=["left", "right"], inventory=[potion])
        view = data["observation"]
        view["combat"].update(hp="9", max_hp="10")
        view["interactions"] = {"inventory": [{"item": "potion", "drinkable": True}]}
        policy = Policy()
        policy.largest_hits.update({("actor", "left"): 3, ("actor", "right"): 3})
        self.assertFalse(policy.unsafe_health_against(9, "left"))
        self.assertFalse(policy.unsafe_health_against(9, "right"))
        self.assertTrue(policy.may_need_escape(view))
        self.assertEqual(policy.choose(data), ({"type": "drink", "item": "potion"}, "heal"))
        actors[0]["position"] = {"x": -3, "y": 0, "z": 0}
        self.assertFalse(policy.may_need_escape(view))

    def test_uses_healing_before_a_known_lethal_hit_when_cornered(self):
        foe = {"id": "foe", "name": "troll", "position": {"x": 1, "y": 0, "z": 0}}
        potion = {"id": "dose", "name": "medicine", "identity": "item.potion.healing",
                  "identified": True, "quantity": 1}
        melee = {"id": "strike", "name": "Strike", "cooldown_remaining": "0", "cost": None,
                 "basic_attack": True, "reach": {"kind": "melee"}, "target_kind": "actor"}
        def picture(hp, events):
            return player_turn([cell("here"), cell("foe", 1)], actors=[foe], hostiles=["foe"],
                               inventory=[potion], abilities=[melee], combat={"hp": hp, "max_hp": 12,
                               "events": events}, interactions={"inventory": [{"item": "dose", "drinkable": True}]})
        for loss, expected in [(6, "heal"), (5, "melee")]:
            with self.subTest(loss=loss):
                policy = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                policy.observe(picture(6 + loss, []))
                turn = picture(6, [{"type": "ability", "target": "player", "caster": "foe", "outcome": "applied"}])
                self.assertEqual(choose(policy, turn)[1], expected)
                self.assertEqual(policy.observed_loss["foe"], loss)

    def test_selects_emergency_consumables_by_identity_and_server_offer(self):
        renamed = {"id": "real", "name": "Greater restoration", "identified": True,
                   "identity": "item.potion.extra-healing", "quantity": 1}
        decoy = {**renamed, "id": "wrong", "name": "potion of extra healing", "identity": "item.potion.fixture.decoy"}
        for offered in (True, False):
            with self.subTest(offered=offered):
                turn = player_turn([cell("here")], inventory=[decoy, renamed], combat={"hp": 2},
                                   interactions={"inventory": [{"item": "real", "drinkable": offered},
                                                               {"item": "wrong", "drinkable": True}]})
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual(reason == "heal", offered)
                if offered:
                    self.assertEqual(action.item.target, "real")

    def test_healing_uses_only_an_identified_drinkable_potion(self):
        items = [
            {"id": "unknown", "name": "potion of healing", "identified": False, "quantity": 1},
            {"id": "known", "name": "potion of extra healing", "identified": True,
             "identity": "item.potion.extra-healing", "quantity": 1},
        ]
        turn = player_turn([cell("here")], inventory=items, combat={"hp": 3}, interactions={"inventory": [
            {"item": item["id"], "drinkable": True} for item in items]})
        action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
        self.assertEqual((action.item.target, reason), ("known", "heal"))

    def test_moves_to_a_disclosed_contact_cell_without_stepping_into_enemy(self):
        foe = {"id": "foe", "position": {"x": 2, "y": 0, "z": 0}}
        data = state([cell("here"), cell("contact", 1), cell("blocked", 2)],
                     actors=[foe], hostiles=["foe"])
        self.assertEqual(Policy().choose(data),
                         ({"type": "move", "direction": "east"}, "approach_foe"))

    def test_breaks_contact_instead_of_chasing_during_recovery(self):
        foe = {"id": "foe", "name": "rattlesnake", "position": {"x": 2, "y": 0, "z": 0}}
        turn = player_turn([cell("here"), cell("contact", 1), cell("foe_cell", 2), cell("escape", -1)],
                           actors=[foe], hostiles=["foe"], combat={"hp": "17", "max_hp": "38"})
        self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=8), turn),
                         (Move(Direction.WEST), "retreat"))

    def test_routes_around_an_occupied_cell_on_an_alternate_disclosed_path(self):
        def lower(row):
            row["position"]["y"] = 1
            return row

        blocker = {"id": "neutral", "name": "neutral", "position": {"x": 1, "y": 0, "z": 0}}
        room = [cell("here"), cell("occupied", 1), cell("goal", 2),
                lower(cell("safe")), lower(cell("corner", 1)), lower(cell("bend", 2))]
        turn = player_turn(room, actors=[blocker])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        player.maps[1].visited.update({"occupied", "safe", "corner", "bend"})
        self.assertEqual(player.choose(turn), (Move(Direction.SOUTH), "explore"))

    def test_remembers_a_disclosed_trap_and_routes_around_it(self):
        def below(row):
            row["position"]["y"] = 1
            return row

        trap = {"id": "trap-target", "family": None, "reachable": True}
        room = [cell("here"), {**cell("trap", 1), "trap": trap}, cell("goal", 2),
                below(cell("safe")), below(cell("corner", 1)), below(cell("bend", 2))]
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(player_turn(room))
        memory = player.maps[1]
        self.assertTrue(memory.cells["trap"].trapped)
        self.assertNotEqual(memory.toward(player_turn(room), {"goal"}), Move(Direction.EAST))
        self.assertEqual(memory.toward(player_turn(room), {"goal"}, prefer_safe=False), Move(Direction.EAST))
        memory.visited.update({"trap", "safe", "corner", "bend"})
        sheltered = player_turn([row for row in room if row["key"] != "trap"])
        self.assertEqual(choose(player, sheltered), (Move(Direction.SOUTH), "explore"))
        self.assertTrue(memory.cells["trap"].trapped)
        player.observe(player_turn([cell("here"), cell("trap", 1)]))
        self.assertFalse(memory.cells["trap"].trapped)

    def test_accepts_a_disclosed_trap_on_the_only_known_return_route(self):
        amulet = {"id": "artifact", "identity": DEFAULT_OBJECTIVE_IDENTITY, "name": AMULET,
                  "quantity": "1", "identified": True}
        trap = {**cell("trap", 1), "trap": {"id": "trap-target", "family": "pit", "reachable": True}}
        turn = player_turn([cell("here"), trap, cell("exit", 2, up=True)], inventory=[amulet])
        self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn),
                         (Move(Direction.EAST), "seek_stairs"))

    def test_resumes_paused_intention_through_the_observed_handle(self):
        import json
        from unittest.mock import Mock
        from rogue_player import play
        from tor_client_bot import Session

        native = Mock()
        paused, ready = Mock(), Mock()
        here, far = cell("here"), cell("other", 2)
        meal = {"id": "ration-a", "name": "ration", "class": "food", "quantity": "2", "identified": False}
        ground = [{**here, "item": meal, "reachable": True},
                  {**far, "item": {**meal, "id": "ration-b"}, "reachable": False}]
        picture = json.loads(player_turn([here, far], ground=ground).recording())
        paused.data.return_value = {**picture, "can_resume": True, "readiness": {"admission": False}}
        ready.data.return_value = picture
        native.observe.side_effect = [paused, ready]
        native.resume.return_value = {"status": "confirmed"}
        trace = []
        report = play(Session(native), Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), 1, trace.append)
        native.resume.assert_called_once_with(paused)
        native.act.assert_not_called()
        self.assertEqual(report["terminal"], "action_budget")
        self.assertEqual(trace[0]["action"], {"type": "resume"})
        self.assertEqual(trace[0]["reachable_pickups"], [
            {"item": "ration-a", "category": "food", "identity": None, "quantity": 2, "cell": "here"}])

    def test_switches_to_stronger_disclosed_armor_and_ignores_cursed_gear(self):
        slot = {"id": "armor"}
        def armor(item, reduction, *, equipped=None, cursed=False):
            return {"item": item, "slot": slot, "equipped_slot": equipped,
                    "known_cursed": cursed, "known_equipment": {
                        "attack": None, "modifiers": [{"id": "reduction", "fields": [
                            {"id": "amount", "value": {"value": str(reduction)}},
                        ]}],
                    }}

        data = state([cell("here")])
        old, upgrade, cursed = armor("old", 1, equipped=0), armor("upgrade", 3), armor("cursed", 9, cursed=True)
        data["observation"]["interactions"] = {"slots": [slot], "inventory": [old, upgrade, cursed]}
        policy = Policy()
        self.assertEqual(policy.choose(data), ({"type": "unequip", "item": "old"}, "upgrade_equipment"))
        old["equipped_slot"] = None
        self.assertEqual(policy.choose(data),
                         ({"type": "equip", "item": "upgrade", "slot": 0}, "upgrade_equipment"))

    def test_identifies_only_items_offered_by_current_prompt(self):
        data = state([cell("here")], inventory=[
            {"id": "absent", "class": "wand", "name": "wand", "identified": False},
            {"id": "offered", "class": "ring", "name": "ring", "identified": False},
        ])
        data["observation"]["interactions"] = {"identification": {
            "choice": "current-choice", "items": ["offered"], "classes": ["ring", "wand"],
        }}
        self.assertEqual(Policy().choose(data),
                         ({"type": "identify", "choice": "current-choice", "item": "offered"},
                          "identify"))

    def test_reads_matching_identification_scroll_for_held_unidentified_item(self):
        inventory = [{"id": "scroll", "class": "scroll", "identified": True,
                      "identity": "item.scroll.identify-armor", "name": "lore of robes"},
                     {"id": "armor", "class": "armor", "identified": False, "name": "mail"}]
        data = state([cell("here")], inventory=inventory)
        data["observation"]["interactions"] = {"inventory": [{"item": "scroll", "readable": True}]}
        self.assertEqual(Policy().choose(data), ({"type": "read", "item": "scroll"}, "identify_gear"))

    def test_restores_disclosed_strength_loss_before_descending(self):
        potion = {"id": "restore", "name": "Restoring draught", "identity": "item.potion.restore-strength",
                  "identified": True, "quantity": 1}
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        for loss, expected in [(2, "restore_strength"), (0, "descend")]:
            attributes = dict.fromkeys(["speed", "intellect", "willpower", "awareness", "presence"], 0)
            attributes["strength"] = loss
            turn = player_turn([cell("stairs", down=True)], inventory=[potion], interactions={
                "inventory": [{"item": "restore", "drinkable": True}]}, stats={
                "restorable_losses": {"attributes": attributes, "hit_dice": "0", "maximum_health": 0}})
            self.assertEqual(turn.restorable_losses.strength, loss)
            self.assertEqual(choose(player, turn)[1], expected)

    def test_clears_a_disclosed_worn_curse_before_descending(self):
        scroll = {"id": "scroll", "name": "Cursebreak", "identity": "item.scroll.remove-curse", "identified": True}
        data = state([cell("stairs", down=True)], inventory=[scroll])
        view = data["observation"]
        armor = {"item": "armor", "equipped_slot": 2, "known_cursed": True}
        view["interactions"] = {"inventory": [{"item": "scroll", "readable": True}, armor]}
        policy = Policy()
        self.assertEqual(policy.choose(data), ({"type": "read", "item": "scroll"}, "remove_curse"))
        armor["known_cursed"] = False
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "down"}, "descend"))

    def test_enchants_an_equipped_weapon_but_not_an_equipped_staff(self):
        scroll = {"id": "scroll", "name": "Arms lore", "identity": "item.scroll.enchant-weapon", "identified": True}
        weapon = {"id": "weapon", "name": "mace", "class": "weapon", "identified": True}
        slot = {"id": "weapon"}
        data = state([cell("stairs", down=True)], inventory=[scroll, weapon])
        data["observation"]["interactions"] = {
            "slots": [slot], "inventory": [{"item": "scroll", "readable": True},
                                           {"item": "weapon", "slot": slot, "equipped_slot": 0}]}
        self.assertEqual(Policy().choose(data), ({"type": "read", "item": "scroll"}, "enchant_weapon"))
        weapon["class"] = "wand"
        self.assertEqual(Policy().choose(data), ({"type": "move", "direction": "down"}, "descend"))

    def test_enchants_worn_armor_before_descending(self):
        scroll = {"id": "scroll", "name": "Armor lore", "identity": "item.scroll.enchant-armor", "identified": True}
        slot = {"id": "body_armor"}
        armor = {"item": "armor", "slot": slot, "equipped_slot": 0}
        data = state([cell("stairs", down=True)], inventory=[scroll])
        data["observation"]["interactions"] = {
            "slots": [slot], "inventory": [{"item": "scroll", "readable": True}, armor]}
        self.assertEqual(Policy().choose(data), ({"type": "read", "item": "scroll"}, "enchant_armor"))
        armor["equipped_slot"] = None
        self.assertEqual(Policy().choose(data), ({"type": "move", "direction": "down"}, "descend"))

    def test_protects_disclosed_unprotected_worn_armor_before_descending(self):
        scroll = {"id": "scroll", "name": "Armor ward", "identity": "item.scroll.protect-armor", "identified": True}
        slot = {"id": "body_armor"}
        armor = {"item": "armor", "slot": slot, "equipped_slot": 0, "known_protected": False}
        data = state([cell("stairs", down=True)], inventory=[scroll])
        data["observation"]["interactions"] = {
            "slots": [slot], "inventory": [{"item": "scroll", "readable": True}, armor]}
        policy = Policy()
        self.assertEqual(policy.choose(data), ({"type": "read", "item": "scroll"}, "protect_armor"))
        armor["known_protected"] = True
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "down"}, "descend"))

    def test_identifies_known_armor_with_undisclosed_instance_stats(self):
        inventory = [{"id": "scroll", "class": "scroll", "identified": True,
                      "identity": "item.scroll.identify-armor", "name": "lore of robes"},
                     {"id": "plate", "class": "armor", "identified": True, "name": "plate mail"}]
        data = state([cell("here")], inventory=inventory)
        offers = [{"item": "scroll", "readable": True},
                  {"item": "plate", "slot": {"id": "body_armor"}, "known_equipment": None}]
        interactions = {"inventory": offers}
        data["observation"]["interactions"] = interactions
        policy = Policy()
        self.assertEqual(policy.choose(data), ({"type": "read", "item": "scroll"}, "identify_gear"))
        interactions["identification"] = {"choice": "armor-offer", "items": ["plate"]}
        self.assertEqual(policy.choose(data),
                         ({"type": "identify", "choice": "armor-offer", "item": "plate"}, "identify"))

    def test_keeps_a_spear_for_ranged_combat_even_when_weapon_stock_is_full(self):
        inventory = [{"id": key, "name": "mace", "class": "weapon", "quantity": 1,
                      "identity": "item.weapon.mace", "identified": True} for key in ("one", "two", "three")]
        spear = {"id": "spear", "name": "Ash pike", "identity": "item.weapon.spear", "class": "weapon",
                 "quantity": 1, "identified": True}
        here = cell("room")
        ground = [{**here, "item": spear, "reachable": True}]
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        turn = player_turn([here], inventory=inventory, ground=ground)
        self.assertEqual(choose(player, turn), (Pickup(turn.ground_items[0].item), "take"))
        inventory.append({**spear, "id": "carried-spear"})
        self.assertIsInstance(choose(player, player_turn([here], inventory=inventory, ground=ground))[0], Pickup)

    def test_collects_nearby_loot_with_a_full_pack_of_the_same_type(self):
        here, hall, near = cell("here"), cell("hall", 1), cell("loot", 2)
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        for category, identity in [("weapon", "item.weapon.arrow"), ("coin", None),
                                   ("tool", None), ("misc", "item.misc.clean-bandage"), ("wand", None)]:
            with self.subTest(category=category):
                item = {"id": "ground", "class": category, "identity": identity, "name": category,
                        "identified": identity is not None, "quantity": 1}
                pack = [{**item, "id": "stock", "quantity": 40}]
                for pos, reachable, reason in ((near, False, "seek_supplies"), (here, True, "take")):
                    turn = player_turn([here, hall, near], inventory=pack,
                                       ground=[{**pos, "item": item, "reachable": reachable}])
                    self.assertEqual(choose(player, turn)[1], reason)

    def test_searches_closed_corridor_end_six_times_before_exploring_elsewhere(self):
        import json
        from tor_client_bot import Result
        here = cell("end")
        walls = [{**cell(key), "position": {"x": x, "y": y, "z": 0}, "wall": True} for key, x, y in
                 [("north", 0, -1), ("east", 1, 0), ("south", 0, 1)]]
        ability = {"id": "rogue.search", "name": "Search", "cost": None, "cooldown_remaining": 0,
                   "basic_attack": False, "target_kind": "self_only", "reach": {"kind": "self_only"}}
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        room = [here, cell("hall", -1), *walls]
        turn = player_turn(room, abilities=[ability])
        for index, phase in enumerate(["failed"] + ["resolved"] * 6):
            action, reason = choose(player, turn)
            self.assertEqual(reason, "search")
            activity = {"type": "admitted", "actor": 1, "branch": "game", "intention": str(index), "phase": "queued"}
            receipt = Result.from_reply({"status": "confirmed", "receipt": activity})
            player.admitted(turn, action, receipt)
            picture = json.loads(turn.recording())
            picture["finished_intentions"] = [{**activity, "phase": phase}]
            turn = Turn(picture)
        self.assertEqual(choose(player, turn)[1], "explore")
        player.completed_end_searches["end"] = 1
        walls[1]["wall"] = False
        self.assertEqual(choose(player, player_turn(room, abilities=[ability]))[1], "explore")

    def test_full_weapon_stock_does_not_hide_a_stronger_known_weapon(self):
        inventory = [{"id": item, "name": "mace", "identity": "item.weapon.mace",
                      "class": "weapon", "quantity": "1", "identified": True}
                     for item in ("one", "two", "three")]
        sword = {"id": "sword", "name": "Steel blade", "identity": "item.weapon.long-sword",
                 "class": "weapon", "quantity": "1", "identified": True}
        here = cell("room")
        data = state([here], inventory=inventory, ground=[{**here, "item": sword, "reachable": True}])
        self.assertEqual(Policy().choose(data), ({"type": "take", "item": "sword", "quantity": "1"}, "loot"))
        inventory.append({**sword, "id": "carried-sword"})
        self.assertNotEqual(Policy().choose(data)[1], "loot")

    def test_collects_potions_and_gear_but_bounds_unidentified_inventory(self):
        potion = {"id": "red", "name": "red potion", "class": "potion",
                  "quantity": "1", "identified": False}
        ground = {**cell("near", 1), "item": potion, "reachable": True}
        room = [cell("here"), cell("near", 1)]
        self.assertEqual(Policy().choose(state(room, ground=[ground])),
                         ({"type": "take", "item": "red", "quantity": "1"}, "loot"))
        held = [{"id": str(i), "name": "other potion", "class": "potion", "identified": False}
                for i in range(4)]
        self.assertEqual(Policy().choose(state(room, ground=[ground], inventory=held)),
                         ({"type": "move", "direction": "east"}, "explore"))

    def test_eats_ordinary_food_when_the_ruleset_omits_the_ration_tag(self):
        meal = {"id": "meal", "name": "fixture meal", "class": "food", "quantity": 1, "identified": False}
        stone = {**meal, "id": "stone", "name": "stone food"}
        turn = player_turn([cell("here"), cell("frontier", 1)], inventory=[stone, meal],
                           stats={"nutrition": {"hunger": "hungry"}}, interactions={"inventory": [
                               {"item": "stone", "edible": False}, {"item": "meal", "edible": True}]})
        action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
        self.assertEqual((action.item.target, reason), ("meal", "eat"))

    def test_collects_visible_food_before_descending_even_when_stocked(self):
        here = cell("stairs", down=True)
        meal = {"id": "meal", "name": "food ration", "class": "food", "quantity": 1, "identified": False,
                "known_supply": {"id": "ration"}}
        for inventory in [[], [{**meal, "id": "spare"}]]:
            turn = player_turn([here], inventory=inventory, ground=[{**here, "item": meal, "reachable": True}])
            self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn),
                             (Pickup(turn.ground_items[0].item), "take"))
        food_cell = cell("food", 1)
        turn = player_turn([here, food_cell], ground=[{**food_cell, "item": meal, "reachable": False}])
        self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn), (Move(Direction.EAST), "seek_item"))

    def test_collects_food_without_a_ration_tag_before_descending(self):
        here = cell("stairs", down=True)
        meal = {"id": "meal", "name": "fixture meal", "class": "food", "quantity": 2, "identified": False}
        turn = player_turn([here], ground=[{**here, "item": meal, "reachable": True}])
        self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn),
                         (Pickup(turn.ground_items[0].item), "take"))

    def test_hungry_bot_returns_to_a_last_seen_ration_before_collecting_gear(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        meal = {"id": "meal", "name": "food", "class": "food", "quantity": 1, "identified": False}
        track = [cell("near"), cell("middle", 1), cell("meal", 2)]
        player.observe(player_turn(track, ground=[{**track[-1], "item": meal, "reachable": False}]))
        sword = {"id": "spear", "name": "spear", "class": "weapon", "quantity": 1,
                 "identified": True, "identity": "item.weapon.spear"}
        ground = [{**track[0], "item": sword, "reachable": True}]
        hungry = {"nutrition": {"hunger": "weak"}}
        self.assertEqual(choose(player, player_turn(track[:2], ground=ground, stats=hungry)),
                         (Move(Direction.EAST), "forage"))
        # Seeing the food cell empty supersedes the old sighting.
        turn = player_turn(track, ground=ground, stats=hungry)
        self.assertEqual(choose(player, turn), (Pickup(turn.ground_items[0].item), "take"))

    def test_takes_an_immediate_objective_before_starting_melee(self):
        foe = {"id": "foe", "position": {"x": 1, "y": 0, "z": 0}}
        melee = {"id": "basic_melee", "cooldown_remaining": "0", "cost": None}
        amulet = {"identity": DEFAULT_OBJECTIVE_IDENTITY, "name": AMULET, "id": "amulet", "quantity": "1"}
        scenarios = (
            (24, cell("here", down=True), [], [], {"type": "move", "direction": "down"}, "descend"),
            (26, cell("here", up=True), [amulet], [], {"type": "move", "direction": "up"}, "ascend"),
            (26, cell("here"), [], [{**cell("here"), "item": amulet, "reachable": True}],
             {"type": "take", "item": "amulet", "quantity": "1"}, "loot"),
        )
        for floor, here, inventory, ground, action, reason in scenarios:
            with self.subTest(reason=reason):
                data = state([here, cell("foe", 1)], actors=[foe], hostiles=["foe"],
                             abilities=[melee], inventory=inventory, ground=ground)
                policy = Policy()
                policy.floor = floor
                self.assertEqual(policy.choose(data), (action, reason))

    def test_prioritizes_disclosed_descent_before_the_amulet_depth(self):
        turn = player_turn([cell("here"), cell("frontier", -1), cell("way", 1), cell("stairs", 2, down=True)])
        self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn),
                         (Move(Direction.EAST), "seek_stairs"))

    def test_exposes_new_terrain_before_visiting_known_interior(self):
        def wall(key, x, y):
            value = cell(key, x)
            value["wall"] = True
            value["position"]["y"] = y
            return value

        view = [cell("here"), cell("interior", -1), cell("passage", 1), cell("frontier", 2),
                wall("west", -2, 0), *[wall(f"{x},{y}", x, y)
                                       for x in (-1, 0, 1, 2) for y in (-1, 1)]]
        self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), player_turn(view)),
                         (Move(Direction.EAST), "explore"))

    def test_retreat_takes_a_clear_diagonal_toward_a_stair(self):
        room = []
        for x, y in ((0, 0), (-1, 0), (0, -1), (-1, -1), (1, 0), (0, 1), (2, 0)):
            tile = cell(f"{x},{y}", x, up=(x, y) == (-1, -1))
            tile["position"]["y"] = y
            room.append(tile)
        foe = {"id": "foe", "name": "wraith", "position": {"x": 2, "y": 0, "z": 0}}
        policy = Policy()
        policy.floor = 2
        self.assertEqual(policy.choose(state(room, actors=[foe], hostiles=["foe"])),
                         ({"type": "move", "direction": "north_west"}, "retreat_to_stairs"))

    def test_retreat_routes_to_the_stair_without_stepping_closer_to_the_foe(self):
        north, stair = cell("north"), cell("stair", 1, up=True)
        north["position"]["y"] = stair["position"]["y"] = -1
        foe = {"id": "foe", "name": "griffin", "position": {"x": 3, "y": 0, "z": 0}}
        south = [cell(f"south-{distance}") for distance in range(1, 4)]
        for distance, tile in enumerate(south, 1):
            tile["position"]["y"] = distance
        data = state([cell("here"), cell("east", 1), north, stair, cell("foe", 3), *south],
                     actors=[foe], hostiles=["foe"])
        policy = Policy()
        policy.floor = 2
        self.assertEqual(policy.choose(data),
                         ({"type": "move", "direction": "north"}, "retreat_to_stairs"))

    def test_retreats_up_disclosed_stairs_to_break_a_low_health_pursuit(self):
        foe = {"id": "foe", "position": {"x": -1, "y": 0, "z": 0}}
        policy = Policy()
        policy.floor = 8
        data = state([cell("foe", -1), cell("stairs", up=True), cell("escape", 1)],
                     actors=[foe], hostiles=["foe"])
        data["observation"]["combat"]["hp"] = "2"
        data["observation"]["interactions"] = {"inventory": []}
        self.assertEqual(policy.choose(data),
                         ({"type": "move", "direction": "up"}, "retreat_upstairs"))

    def test_reports_known_navigation_when_no_disclosed_route_remains(self):
        from rogue_policy_reference import run

        class Session:
            def observe(self):
                return type("Observation", (), {"data": lambda _: {
                    "state": state([cell("here", up=True)]), "cursor": 0,
                    "readiness": {"admission": True},
                }})()

        report = {}
        self.assertEqual(run(Session(), Policy(), budget=1, report=report), "no_disclosed_route")
        navigation = report["navigation"]
        self.assertEqual((navigation["floor"], navigation["cell"]), (1, "here"))
        self.assertEqual((navigation["known_up_stairs"], navigation["known_down_stairs"]), (1, 0))
        self.assertEqual((navigation["visited_cells"], navigation["searches_admitted"]), (1, 0))

    def test_retries_search_on_a_disclosed_wall_with_a_finite_budget(self):
        wall = {**cell("wall", 1), "wall": True}
        search = {"id": "rogue.search", "cooldown_remaining": "0", "cost": None}
        data = state([cell("here"), wall], abilities=[search])
        policy = Policy()
        for _ in range(2):
            action, reason = policy.choose(data)
            self.assertEqual(reason, "search")
            self.assertEqual(action["id"], "rogue.search")
            policy.accepted(action, data["observation"])
        for _ in range(6):
            action, _ = policy.choose(data)
            policy.accepted(action, data["observation"])
        self.assertEqual(policy.choose(data), (None, "no_disclosed_route"))

    def test_recovers_from_moderate_injury_on_shallow_floor_with_food(self):
        policy = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=8)
        turn = player_turn([cell("here"), cell("frontier", 1)], combat={"hp": "13", "max_hp": "22"},
                           stats={"nutrition": {"kind": "required", "reserve": 869, "maximum": 1300,
                                                "unit_ticks": "100", "elapsed": "0", "hunger": "nourished"},
                                  "health_recovery": {"period_ticks": "1000", "elapsed_ticks": "0",
                                                      "reset_percent": 100, "stopped": False}})
        self.assertEqual(choose(policy, turn), (Wait(), "recover"))

    def test_recovers_outside_recent_threat_cells_when_a_visited_route_is_open(self):
        turn = player_turn([cell("here"), cell("cleared", -1), cell("unknown", 1)], tick=200,
                           combat={"hp": 6}, stats={"health_recovery": {"period_ticks": 1000,
                           "elapsed_ticks": 0, "reset_percent": 100, "stopped": False}})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        player.maps[1].visited.add("cleared")
        player.danger_until["here"] = 1200
        self.assertEqual(player.choose(turn), (Move(Direction.WEST), "reposition_to_recover"))
        player.danger_until["cleared"] = 1200
        self.assertEqual(player.choose(turn), (Wait(), "recover"))

    def test_explores_away_from_recent_danger_when_another_route_exists(self):
        turn = player_turn([cell("here"), cell("east", 1), cell("west", -1)])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        first, reason = player.choose(turn)
        self.assertEqual(reason, "explore")
        danger = turn.cell_at(first.direction.offset).key
        player.danger_until[danger] = 1200
        other, reason = player.choose(turn)
        self.assertEqual(reason, "explore")
        self.assertNotEqual(other, first)
        player.maps[1].visited.add(turn.cell_at(other.direction.offset).key)
        self.assertEqual(player.choose(turn), (first, "explore"))

    def test_recovery_moves_after_an_unseen_attack_without_a_visible_enemy(self):
        data = state([cell("exposed"), cell("cleared", -1), cell("unknown", 1)])
        view = data["observation"]
        view["tick"] = "100"
        view["combat"].update(hp="7", max_hp="10")
        policy = Policy()
        policy.floors[1] = Floor(visited={"exposed", "cleared"})
        self.assertEqual(policy.choose(data), ({"type": "wait"}, "recover"))

        view["tick"] = "200"
        view["combat"].update(hp="5", events=[
            {"type": "unseen_ability", "caster": "unseen", "target": "player", "outcome": "applied"},
        ])
        policy.observe_combat(view)
        view["combat"]["events"] = []
        self.assertEqual(policy.choose(data),
                         ({"type": "move", "direction": "west"}, "evade_unseen_attack"))

    def test_keeps_moving_between_unseen_attacks_before_resuming_recovery(self):
        data = state([cell("first"), cell("middle", 1), cell("far", 2)])
        policy = Policy()
        policy.floors[1] = Floor(visited={"first", "middle", "far"})
        view = data["observation"]
        view["tick"] = "100"
        view["combat"].update(hp="6", events=[
            {"type": "unseen_ability", "caster": "unseen", "target": "player", "outcome": "miss"},
        ])
        action, reason = policy.choose(data)
        self.assertEqual((action, reason), ({"type": "move", "direction": "east"}, "evade_unseen_attack"))
        policy.accepted(action, view, reason=reason)
        view["visible_cells"] = [cell("middle"), cell("first", -1), cell("far", 1)]
        view["tick"] = "200"
        view["combat"]["events"] = []
        self.assertEqual(policy.choose(data),
                         ({"type": "move", "direction": "east"}, "evade_unseen_attack"))
        view["tick"] = "550"
        self.assertEqual(policy.choose(data), ({"type": "wait"}, "recover"))

    def test_uses_known_sight_after_an_unseen_attack_in_a_lit_room(self):
        potion = {"id": "dose", "name": "clear potion", "identified": True,
                  "identity": "item.potion.see-invisible", "class": "potion"}
        data = state([{**cell("here"), "lit": True}, cell("west", -1)], inventory=[potion])
        view = data["observation"]
        view["tick"] = "100"
        view["interactions"] = {"inventory": [{"item": "dose", "drinkable": True}]}
        view["combat"]["events"] = [
            {"type": "unseen_ability", "caster": "unseen", "target": "player", "outcome": "miss"},
        ]
        policy = Policy()
        self.assertEqual(policy.choose(data), ({"type": "drink", "item": "dose"}, "reveal_attacker"))
        view["combat"]["own_stats"]["conditions"] = [
            {"condition": {"id": "rogue.see_invisible"}, "phase": "active"},
        ]
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "west"}, "evade_unseen_attack"))

    def test_moderate_injury_recovers_while_walking_a_visited_route_to_stairs(self):
        room = [cell("here"), cell("hall", 1), cell("stairs", 2, down=True)]
        stats = {"health_recovery": {"period_ticks": 1000, "elapsed_ticks": 0, "reset_percent": 100,
                                     "stopped": False}}
        turn = player_turn(room, combat={"hp": 6}, stats=stats)
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        self.assertEqual(choose(player, turn), (Wait(), "recover"))
        player.maps[1].visited.add("hall")
        player.unsafe_stairs["stairs", Direction.DOWN] = turn.tick + 100
        self.assertEqual(player.choose(turn), (Wait(), "recover"))
        scarce = player_turn(room, combat={"hp": 6}, stats={**stats, "nutrition": {
            "kind": "required", "reserve": 50, "maximum": 2000, "unit_ticks": 100,
            "hunger": "nourished", "elapsed": 0}})
        player.ascent_ticks_by_floor[1] = scarce.nutrition.ticks_until_empty
        self.assertIsNone(player.recovery_action(scarce))
        player.ascent_ticks_by_floor.clear()
        player.unsafe_stairs["stairs", Direction.DOWN] = turn.tick
        self.assertEqual(player.choose(turn), (Move(Direction.EAST), "recover_en_route"))
        player.danger_until["hall"] = 1200
        self.assertEqual(player.choose(turn), (Wait(), "recover"))
        on_stairs = player_turn([cell("stairs", down=True)], combat={"hp": 6}, stats=stats)
        self.assertEqual(choose(player, on_stairs), (Wait(), "recover"))

    def test_finishes_recovery_before_exposing_itself_to_more_combat(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        room = [cell("here"), cell("frontier", 1)]
        stats = {"health_recovery": {"period_ticks": 1000, "elapsed_ticks": 0, "reset_percent": 100,
                                     "stopped": False}}
        for hp in (4, 6, 9):
            turn = player_turn(room, combat={"hp": hp}, stats=stats)
            expected = (Wait(), "recover") if hp < 8 else (Move(Direction.EAST), "explore")
            self.assertEqual(choose(player, turn), expected)

    def test_retreats_to_known_upstairs_when_recent_combat_interrupts_deep_recovery(self):
        foe = {"id": "foe", "name": "Wraith", "position": {"x": 1, "y": 0, "z": 0}}
        cells = [cell("here"), cell("foe", 1), cell("upstairs", -1, up=True)]
        pressured = state(cells, actors=[foe], hostiles=["foe"])
        hidden = state(cells)
        for picture in (pressured, hidden):
            picture["observation"]["combat"].update(hp="12", max_hp="48")
        policy = Policy()
        policy.floor = 24
        policy.choose(pressured)
        self.assertEqual(policy.choose(hidden),
                         ({"type": "move", "direction": "west"}, "retreat_to_stairs"))

    def test_equips_a_known_slow_digestion_ring_before_optional_recovery(self):
        ring = {"id": "ring", "name": "Temperance", "class": "ring", "identified": True,
                "identity": "item.ring.ring-slow-digestion", "quantity": 1}
        stats = {"health_recovery": {"period_ticks": "100", "elapsed_ticks": "0", "stopped": False,
                                     "reset_percent": 0},
                 "nutrition": {"kind": "required", "hunger": "nourished", "reserve": 120,
                               "maximum": 150, "unit_ticks": "100", "elapsed": "0"}}
        slot = {"id": "ring", "name": "finger"}
        offer = {"item": "ring", "slot": slot, "equipped_slot": None}
        for equipped, expected in [(None, "conserve_food"), (0, "recover")]:
            with self.subTest(equipped=equipped):
                offer["equipped_slot"] = equipped
                turn = player_turn([cell("here")], inventory=[ring], combat={"hp": 6}, stats=stats,
                                   interactions={"slots": [slot], "inventory": [offer]})
                self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)[1], expected)

    def test_provisions_known_return_route_before_taking_stairs(self):
        nutrition = {"kind": "required", "unit_ticks": "100", "elapsed": "0",
                     "maximum": 1300, "hunger": "nourished"}
        ration = {"id": "ration", "class": "food", "name": "ration", "quantity": 1, "identified": False}
        relic = {"id": "objective", "identity": DEFAULT_OBJECTIVE_IDENTITY, "name": AMULET,
                 "identified": True, "quantity": 1}
        for carrying, direction in [(True, Direction.UP), (False, Direction.DOWN)]:
            with self.subTest(direction=direction):
                player = Adventurer(6, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
                player.ascent_ticks_by_floor = {1: 20000, 2: 20000}
                player.observe(player_turn([cell("passage"), cell("food", 1)], ground=[
                    {"item": ration, "reachable": False, "position": {"x": 1, "y": 0, "z": 0}},
                ]))
                for reserve, expected in [(350, Direction.EAST), (400, direction)]:
                    turn = player_turn([cell("stairs", up=carrying, down=not carrying), cell("passage", 1)],
                                       inventory=[relic] if carrying else [],
                                       stats={"nutrition": dict(nutrition, reserve=reserve)})
                    self.assertEqual(choose(player, turn)[0], Move(expected))

    def test_injured_food_scout_keeps_moving_until_the_scouting_budget_expires(self):
        player = Adventurer(6, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
        cells = [cell("hall"), cell("stairs", -1, down=True), cell("frontier", 1)]
        stats = {"health_recovery": {"period_ticks": 1000, "elapsed_ticks": 0, "reset_percent": 100,
                                     "stopped": False},
                 "nutrition": {"kind": "required", "reserve": 400, "maximum": 2000, "unit_ticks": 100,
                               "hunger": "nourished", "elapsed": 0}}
        player.ascent_ticks_by_floor[2] = 50000
        player.food_scouts[3] = 1000, 0
        for tick, expected in [(100, (Move(Direction.EAST), "scout_for_food")),
                               (1000, (Move(Direction.WEST), "recover_en_route"))]:
            with self.subTest(tick=tick):
                turn = player_turn(cells, tick=tick, combat={"hp": 6}, stats=stats)
                player.observe(turn)
                player.maps[3].visited.add("stairs")
                self.assertEqual(player.choose(turn), expected)

    def test_food_scout_finishes_once_the_bot_heads_back_to_the_stairs(self):
        player = Adventurer(6, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
        player.ascent_ticks_by_floor[2] = 50000
        player.food_scouts[3] = 1000, 0
        cells = [cell("hall"), cell("stairs", -1, down=True), cell("frontier", 1)]
        turn = player_turn(cells, tick=100, stats={"nutrition": {
            "kind": "required", "reserve": 400, "maximum": 2000, "unit_ticks": 100,
            "hunger": "nourished", "elapsed": 0}})
        player.observe(turn)
        player.maps[3].visited.add("stairs")
        self.assertEqual(player.choose(turn), (Move(Direction.EAST), "scout_for_food"))
        player.admitted(turn, Move(Direction.WEST), reason="seek_stairs")
        self.assertEqual(player.choose(turn), (Move(Direction.WEST), "seek_stairs"))

    def test_recovers_along_a_visited_food_route_when_rest_is_also_affordable(self):
        player = Adventurer(6, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
        cells = [cell("here"), cell("stairs", -1, down=True), cell("passage", 1), cell("frontier", 2)]
        stats = {"health_recovery": {"period_ticks": 1000, "elapsed_ticks": 0, "reset_percent": 100,
                                     "stopped": False},
                 "nutrition": {"kind": "required", "reserve": 400, "maximum": 2000,
                               "unit_ticks": 100, "hunger": "nourished", "elapsed": 0}}
        player.descent_ticks_by_floor[2] = 50000
        player.food_scouts[3] = 1000, 0
        turn = player_turn(cells, tick=100, combat={"hp": 6}, stats=stats)
        player.observe(turn)
        player.maps[3].visited.update({"stairs", "passage"})
        self.assertEqual(player.choose(turn), (Move(Direction.EAST), "scout_for_food"))
        player.danger_until["passage"] = 900
        self.assertEqual(player.choose(turn), (Wait(), "recover"))

    def test_scouts_for_food_before_an_underfunded_descent(self):
        data = state([cell("stairs", down=True), cell("frontier", 1)])
        view = data["observation"]
        view["combat"]["own_stats"]["nutrition"] = {
            "kind": "required", "reserve": 400, "unit_ticks": "100", "hunger": "nourished",
            "elapsed": "0",
        }
        policy = Policy(retrieval_depth=6)
        policy.floor = 3
        policy.return_travel_ticks = {1: 50000}
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "east"}, "scout_for_food"))
        view["tick"] = "10000"
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "down"}, "descend"))

    def test_scout_budget_includes_unmapped_descent_and_carried_food(self):
        ration = {"class": "food", "id": "ration", "identity": "item.food.ration", "quantity": "2"}
        data = state([cell("stairs", down=True), cell("frontier", 1)], inventory=[ration])
        view = data["observation"]
        nutrition = {"kind": "required", "reserve": 250, "unit_ticks": "100",
                     "hunger": "hungry", "elapsed": "0"}
        view["combat"]["own_stats"]["nutrition"] = nutrition
        view["interactions"] = {"inventory": [{"item": "ration", "edible": True}]}
        policy = Policy(retrieval_depth=6)
        policy.floor = 3
        policy.accepted({"type": "eat", "item": "ration"}, view)
        ration["quantity"] = "1"
        nutrition.update(reserve=1450, hunger="nourished")
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "down"}, "descend"))
        nutrition["reserve"] = 400
        policy.return_travel_ticks = {1: 20000, 2: 20000}
        policy.descent_travel_ticks = {1: 110000, 2: 110000}
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "east"}, "scout_for_food"))
        found = {**ration, "id": "found", "quantity": "1"}
        view["ground_items"] = [{"item": found, "reachable": True, "position": {"x": 0, "y": 0, "z": 0}}]
        action, reason = policy.choose(data)
        self.assertEqual((action, reason), ({"type": "take", "item": "found", "quantity": "1"}, "loot"))
        policy.accepted(action, view, reason=reason)
        view["ground_items"] = []
        ration["quantity"] = "2"
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "down"}, "descend"))

    def test_searches_a_boundary_that_may_reconnect_the_return_route(self):
        room = [cell("decoy-side", -2), {**cell("decoy", -1), "wall": True}, cell("here"),
                cell("search-site", 1), {**cell("connection", 2), "wall": True}, cell("stair", 3, up=True)]
        item = {"id": "objective", "identity": DEFAULT_OBJECTIVE_IDENTITY, "name": AMULET,
                "quantity": 1, "identified": True}
        search = {"id": "rogue.search", "name": "Search", "cooldown_remaining": "0", "cost": None,
                  "basic_attack": False, "reach": {"kind": "personal"}, "target_kind": "self"}
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=4)
        turn = player_turn(room, inventory=[item], abilities=[search])
        player.observe(turn)
        player.maps[4].visited.update({"here", "search-site"})
        self.assertEqual(player.choose(turn), (Move(Direction.EAST), "search"))
        self.assertEqual({c.key for c in player.maps[4].adjacent("connection")}, {"search-site", "stair"})

        # The link is still worth trying if it joins a hallway leading to the stairs.
        room[-1:] = [cell("far-hall", 3), cell("stair", 4, up=True)]
        room.append({**cell("unvisited"), "position": {"x": 0, "y": 1, "z": 0}})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=4)
        turn = player_turn(room, inventory=[item], abilities=[search])
        player.observe(turn)
        player.maps[4].visited.update({"here", "search-site"})
        self.assertEqual(player.choose(turn), (Move(Direction.EAST), "search"))
        player.searches["search-site"] = 3
        self.assertEqual(player.choose(turn), (Move(Direction.SOUTH), "explore"))


    def test_uses_known_teleportation_when_the_remembered_return_route_is_disconnected(self):
        items = [{"id": "objective", "identity": DEFAULT_OBJECTIVE_IDENTITY, "name": AMULET,
                  "quantity": 1, "identified": True},
                 {"id": "scroll", "name": "Wayfarer", "identity": "item.scroll.teleportation",
                  "quantity": 1, "class": "scroll", "identified": True}]
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=4)
        player.observe(player_turn([cell("lost-stair", up=True)]))
        def picture(tiles):
            return player_turn(tiles, inventory=items, interactions={"inventory": [{"item": "scroll", "readable": True}]})
        turn = picture([cell("here")])
        action, reason = choose(player, turn)
        self.assertEqual((action.item.target, reason), ("scroll", "find_return_route"))
        self.assertEqual(choose(player, picture([cell("here"), cell("frontier", 1)])), (Move(Direction.EAST), "explore"))

    def test_partial_recovery_before_reentering_recent_danger_has_a_food_limit(self):
        room = [cell("shelter"), cell("hall", 1), cell("exit", 2, down=True)]
        for reserve, travel_ticks, expected in [
            (15, 400, (Wait(), "recover")),
            (15, 500, (Move(Direction.EAST), "seek_stairs")),
            (5, 0, (Move(Direction.EAST), "seek_stairs")),
        ]:
            with self.subTest(reserve=reserve, travel_ticks=travel_ticks):
                turn = player_turn(room, combat={"hp": "5", "max_hp": "10"}, stats={
                    "nutrition": {"kind": "required", "reserve": reserve, "maximum": 1300, "unit_ticks": 100,
                                  "elapsed": 0, "hunger": "nourished"},
                    "health_recovery": {"period_ticks": 1000, "elapsed_ticks": 0,
                                        "reset_percent": 100, "stopped": False}})
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
                player.observe(turn)
                player.maps[2].visited.add("hall")
                player.ascent_ticks_by_floor[1] = travel_ticks
                player.danger_until["hall"] = 1200
                self.assertEqual(player.choose(turn), expected)

    def test_return_recovery_budgets_only_the_floors_still_ahead(self):
        turn = player_turn([cell("stairs", up=True)],
                           inventory=[{"id": "amulet", "name": AMULET, "identity": DEFAULT_OBJECTIVE_IDENTITY,
                                       "class": "amulet", "identified": True, "quantity": 1}],
                           combat={"hp": 6, "max_hp": 10}, stats={
                               "nutrition": {"kind": "required", "reserve": 40, "maximum": 1300, "unit_ticks": 100,
                                             "elapsed": 0, "hunger": "nourished"},
                               "health_recovery": {"period_ticks": 1000, "elapsed_ticks": 0,
                                                   "reset_percent": 100, "stopped": False}})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
        player.ascent_ticks_by_floor.update({1: 500, 2: 500, 3: 9000})
        self.assertEqual(choose(player, turn), (Wait(), "recover"))
        player.ascent_ticks_by_floor[1] = 1500
        self.assertEqual(player.choose(turn), (Move(Direction.UP), "ascend"))

    def test_recovery_preserves_food_for_previously_mapped_floors(self):
        item = {"id": "objective", "identity": DEFAULT_OBJECTIVE_IDENTITY, "name": AMULET}
        data = state([cell("stairs", up=True)], inventory=[item])
        combat = data["observation"]["combat"]
        combat.update(hp="6", max_hp="10")
        combat["own_stats"].update(
            nutrition={"kind": "required", "reserve": 350, "unit_ticks": "100",
                       "elapsed": "0", "hunger": "nourished"},
            health_recovery={"period_ticks": "15000", "elapsed_ticks": "0", "stopped": False},
        )
        for carrying in (True, False):
            with self.subTest(carrying=carrying):
                if not carrying:
                    data["observation"]["inventory"] = []
                    data["observation"]["visible_cells"] = [cell("stairs", down=True)]
                policy = Policy()
                policy.floor = 3
                self.assertEqual(policy.choose(data), ({"type": "wait"}, "recover"))
                policy.return_travel_ticks = {1: 1000, 2: 6000}
                direction, reason = ("up", "ascend") if carrying else ("down", "descend")
                self.assertEqual(policy.choose(data), ({"type": "move", "direction": direction}, reason))

    def test_takes_available_stairs_if_full_recovery_would_exhaust_reserve(self):
        data = state([cell("stairs", down=True)])
        combat = data["observation"]["combat"]
        combat.update(hp="6", max_hp="10")
        combat["own_stats"].update(
            nutrition={"kind": "required", "reserve": 350, "unit_ticks": "100",
                       "elapsed": "0", "hunger": "nourished"},
            health_recovery={"period_ticks": "40000", "elapsed_ticks": "0", "stopped": False},
        )
        policy = Policy()
        policy.floor = 20
        policy.recovering = True
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "down"}, "descend"))
        combat["own_stats"]["nutrition"]["reserve"] = 1000
        self.assertEqual(policy.choose(data), ({"type": "wait"}, "recover"))

    def test_explores_when_recovery_would_exhaust_food_and_no_stair_is_known(self):
        data = state([cell("here"), cell("frontier", 1)])
        combat = data["observation"]["combat"]
        combat.update(hp="6", max_hp="10")
        combat["own_stats"].update(
            nutrition={"kind": "required", "reserve": 350, "unit_ticks": "100",
                       "elapsed": "0", "hunger": "nourished"},
            health_recovery={"period_ticks": "40000", "elapsed_ticks": "0", "stopped": False},
        )
        policy = Policy()
        self.assertEqual(policy.choose(data),
                         ({"type": "move", "direction": "east"}, "recover_en_route"))
        combat["own_stats"]["health_recovery"]["period_ticks"] = "15000"
        policy = Policy()
        policy.floor = 3
        policy.return_travel_ticks = {1: 1000, 2: 6000}
        self.assertEqual(policy.choose(data), ({"type": "wait"}, "recover"))

    def test_uses_carried_first_aid_before_leaving_a_deep_safe_landing(self):
        bandage = {"id": "bandage", "name": "clean bandage", "known_supply": {"id": "bandage"}}
        care = {"id": "bind_wounds", "cooldown_remaining": "0", "cost": None}
        data = state([cell("stairs", down=True)], inventory=[bandage], abilities=[care])
        view = data["observation"]
        view["combat"].update(hp="30", max_hp="45")
        policy = Policy()
        policy.floor = 24
        action, reason = policy.choose(data)
        self.assertEqual(reason, "first_aid")
        self.assertEqual(action["id"], "bind_wounds")
        care["cooldown_remaining"] = "100"
        self.assertEqual(policy.choose(data), ({"type": "wait"}, "recover"))
        view["combat"]["hp"] = "37"
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "down"}, "descend"))

    def test_holds_a_dangerous_pursuer_before_retreating_and_saves_the_second_scroll(self):
        foe = {"id": "foe", "name": "vampire", "position": {"x": 2, "y": 0, "z": 0}}
        scroll = {"id": "hold", "name": "scroll of hold monster", "identity": "item.scroll.hold-monster", "identified": True}
        data = state([cell("here"), cell("west", -1), cell("east", 1), cell("foe", 2)],
                     actors=[foe], hostiles=["foe"], inventory=[scroll])
        view = data["observation"]
        view["interactions"] = {"inventory": [{"item": "hold", "readable": True}]}
        view["combat"]["events"] = [{
            "type": "ability", "caster": "foe", "target": "player", "outcome": "miss",
            "ability": {"id": "basic_melee"}}]
        policy = Policy()
        action, reason = policy.choose(data)
        self.assertEqual((action, reason), ({"type": "read", "item": "hold"}, "hold_threat"))
        policy.accepted(action, view, reason=reason)
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "west"}, "retreat"))

    def test_uses_an_offensive_wand_before_a_dangerous_pursuer_closes_in(self):
        foe = {"id": "foe", "name": "dragon", "position": {"x": 3, "y": 0, "z": 0}}
        wand = {"id": "wand", "name": "wand of fire", "identified": True}
        room = [cell("here"), cell("west", -1), cell("path", 1), cell("path2", 2), cell("foe", 3), cell("clear", 4)]
        data = state(room, actors=[foe], hostiles=["foe"], inventory=[wand])
        view = data["observation"]
        view["interactions"] = {"inventory": [
            {"item": "wand", "zappable": True, "charges": 1, "device_activation": "direction"}]}
        self.assertEqual(Policy().choose(data), ({"type": "zap", "item": "wand", "aim": {
            "kind": "direction", "direction": "east"}}, "ranged_device"))
        foe["position"]["x"] = 2
        self.assertEqual(Policy().choose(data), ({"type": "move", "direction": "west"}, "retreat"))

    def test_tries_a_known_offensive_wand_once_when_charges_are_undisclosed(self):
        foe = {"id": "foe", "position": {"x": 3, "y": 0, "z": 0}}
        wand = {"id": "wand", "name": "wand of fire", "identified": True}
        room = [cell("here"), cell("path", 1), cell("path2", 2), cell("foe", 3), cell("clear", 4)]
        data = state(room, actors=[foe], hostiles=["foe"], inventory=[wand])
        view = data["observation"]
        offer = {"item": "wand", "zappable": True, "charges": None, "device_activation": "direction"}
        view["interactions"] = {"inventory": [offer]}
        policy = Policy()
        action, reason = policy.choose(data)
        self.assertEqual(reason, "ranged_device")
        self.assertEqual(action["item"], "wand")
        policy.accepted(action, view, reason=reason)
        self.assertNotEqual(policy.choose(data)[1], "ranged_device")
        offer["charges"] = 2
        self.assertEqual(policy.choose(data)[1], "ranged_device")

    def test_declines_an_elemental_ray_that_can_bounce_back_and_uses_a_missile_instead(self):
        foe = {"id": "foe", "position": {"x": 2, "y": 0, "z": 0}}
        wall = {**cell("wall", 4), "wall": True}
        room = [cell("here"), cell("one", 1), cell("foe", 2), cell("three", 3), wall]
        inventory = [{"id": "bolt", "name": "wand of lightning", "identified": True},
                     {"id": "missile", "name": "wand of magic missile", "identified": True}]
        data = state(room, actors=[foe], hostiles=["foe"], inventory=inventory)
        data["observation"]["interactions"] = {"inventory": [
            {"item": item["id"], "zappable": True, "charges": 2, "device_activation": "direction"}
            for item in inventory]}
        self.assertEqual(Policy().choose(data)[0]["item"], "missile")
        wall["wall"] = False
        self.assertEqual(Policy().choose(data)[0]["item"], "bolt")

    def test_elemental_ray_checks_open_corners_beyond_a_diagonal_hostile(self):
        foe = {"id": "foe", "position": {"x": 2, "y": -2, "z": 0}}
        wand = {"id": "wand", "name": "staff of lightning", "identified": True}
        room = [cell("here")]
        for step in range(1, 5):
            for x, y in ((step, -step), (step - 1, -step), (step, 1 - step)):
                tile = cell(f"{x},{y}", x)
                tile["position"]["y"] = y
                room.append(tile)
        corner = room[-1]
        corner["wall"] = True
        data = state(room, actors=[foe], hostiles=["foe"], inventory=[wand])
        data["observation"]["interactions"] = {"inventory": [
            {"item": "wand", "zappable": True, "charges": 2, "device_activation": "direction"}]}
        self.assertNotEqual(Policy().choose(data)[1], "ranged_device")
        corner["wall"] = False
        action, reason = Policy().choose(data)
        self.assertEqual(reason, "ranged_device")
        self.assertEqual(action["aim"]["direction"], "north_east")

    def test_player_samples_a_wand_appearance_once_and_only_with_clearance(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        cells = [cell(str(x), x) for x in range(5)]
        wand = {"id": "wand", "name": "oak wand", "class": "wand", "appearance": "oak wand", "identified": False, "quantity": 1}
        offer = {"item": "wand", "device_activation": "direction", "zappable": True}
        def picture(distance=3, hp=10):
            foe = {"id": "foe", "name": "emu", "position": cell("foe", distance)["position"]}
            return player_turn(cells, actors=[foe], hostiles=["foe"], inventory=[wand], combat={"hp": hp},
                               interactions={"inventory": [offer]})

        self.assertIsNone(player.try_unknown_wand(picture(distance=2)))
        self.assertIsNone(player.try_unknown_wand(picture(hp=7)))
        cells[-1]["wall"] = True
        self.assertIsNone(player.try_unknown_wand(picture()))
        cells[-1]["wall"] = False
        turn = picture()
        action, reason = choose(player, turn)
        self.assertEqual(reason, "test_unknown_device")
        player.admitted(turn, action, reason=reason)
        wand["id"] = offer["item"] = "second"
        self.assertIsNone(player.try_unknown_wand(picture()))

    def test_does_not_test_an_unknown_wand_without_room_to_clear_a_reflection(self):
        foe = {"id": "foe", "position": {"x": 3, "y": 0, "z": 0}}
        wand = {"id": "wand", "name": "oak wand", "class": "wand",
                "identified": False, "appearance": "oak wand"}
        data = state([cell("here"), cell("one", 1), cell("two", 2), cell("foe", 3)],
                     actors=[foe], hostiles=["foe"], inventory=[wand])
        data["observation"]["interactions"] = {"inventory": [
            {"item": "wand", "device_activation": "direction", "charges": None, "zappable": True}]}
        self.assertNotEqual(Policy().choose(data)[1], "test_unknown_device")

    def test_uses_a_disclosed_charged_wand_when_first_body_is_hostile(self):
        foe = {"id": "foe", "position": {"x": 3, "y": 0, "z": 0}}
        wand = {"id": "wand", "name": "wand of magic missile", "identified": True}
        room = [cell("here"), cell("path", 1), cell("path2", 2), cell("foe", 3)]
        data = state(room, actors=[foe], hostiles=["foe"], inventory=[wand])
        view = data["observation"]
        view["interactions"] = {"inventory": [
            {"item": "wand", "zappable": True, "charges": 3, "device_activation": "direction"},
        ]}
        expected = {"type": "zap", "item": "wand", "aim": {"kind": "direction", "direction": "east"}}
        self.assertEqual(Policy().choose(data), (expected, "ranged_device"))
        view["visible_actors"] = [foe, {"id": "ally", "position": {"x": 2, "y": 0, "z": 0}}]
        self.assertNotEqual(Policy().choose(data)[0], expected)

    def test_uses_known_teleport_scroll_before_a_desperate_melee(self):
        foe = {"id": "foe", "position": {"x": 1, "y": 0, "z": 0}}
        scroll = {"id": "scroll", "name": "scroll of teleportation", "identity": "item.scroll.teleportation", "identified": True}
        data = state([cell("here"), cell("foe", 1)], actors=[foe], hostiles=["foe"],
                     inventory=[scroll])
        data["observation"]["interactions"] = {"inventory": [{"item": "scroll", "readable": True}]}
        data["observation"]["combat"]["hp"] = "2"
        self.assertEqual(Policy().choose(data),
                         ({"type": "read", "item": "scroll"}, "emergency_teleport"))

    def test_tries_one_unknown_potion_when_cornered_and_a_hit_may_kill(self):
        foe = {"id": "foe", "position": {"x": 1, "y": 0, "z": 0}}
        potions = [{"id": item, "name": "amber potion", "identified": False,
                    "class": "potion", "appearance": "amber"} for item in ("dose", "spare")]
        data = state([cell("here"), cell("foe", 1)], actors=[foe], hostiles=["foe"],
                     inventory=potions)
        view = data["observation"]
        view["interactions"] = {"inventory": [{"item": item["id"], "drinkable": True} for item in potions]}
        view["combat"]["hp"] = "2"
        policy = Policy()
        self.assertEqual(policy.choose(data), ({"type": "drink", "item": "dose"}, "test_unknown_magic"))
        policy.tested_magic.add(("potion", "amber"))
        self.assertNotEqual(policy.choose(data)[0]["type"], "drink")

        view["combat"]["actors"][0]["injury"] = "near_death"
        self.assertNotEqual(Policy().choose(data)[0]["type"], "drink")

    def test_tries_one_unknown_scroll_when_cornered_without_a_potion(self):
        foe = {"id": "foe", "position": {"x": 1, "y": 0, "z": 0}}
        scroll = {"id": "scroll", "name": "scroll titled DOR", "identified": False,
                  "class": "scroll", "appearance": "DOR"}
        spare = {**scroll, "id": "spare", "appearance": "ZO"}
        data = state([cell("here"), cell("foe", 1)], actors=[foe], hostiles=["foe"], inventory=[scroll, spare],
                     abilities=[{"id": "basic_melee", "cooldown_remaining": "0", "cost": None}])
        view = data["observation"]
        view["interactions"] = {"inventory": [{"item": "scroll", "readable": True}, {"item": "spare", "readable": True}]}
        view["combat"]["hp"] = "2"
        policy = Policy()
        action, reason = policy.choose(data)
        self.assertEqual((action, reason), ({"type": "read", "item": "scroll"}, "test_unknown_magic"))
        policy.accepted(action, view, reason=reason)
        self.assertEqual(policy.choose(data)[1], "melee")

    def test_uses_teleport_instead_of_retrying_escape_when_badly_hurt(self):
        foe = {"id": "foe", "position": {"x": 1, "y": 0, "z": 0}}
        scroll = {"id": "scroll", "name": "scroll of teleportation", "identity": "item.scroll.teleportation", "identified": True}
        data = state([cell("here"), cell("foe", 1)], actors=[foe], hostiles=["foe"], inventory=[scroll])
        view = data["observation"]
        view["combat"]["hp"] = "2"
        view["linked_holds"] = [{"target": "hold", "holder": "foe"}]
        view["interactions"] = {"inventory": [{"item": "scroll", "readable": True}]}
        self.assertEqual(Policy().choose(data),
                         ({"type": "read", "item": "scroll"}, "emergency_teleport"))

    def test_explores_before_trying_an_unknown_scroll_once(self):
        scroll = {"id": "scroll", "name": "labelled scroll", "class": "scroll",
                  "appearance": "rune Z", "identified": False}
        data = state([cell("here"), cell("frontier", 1)], inventory=[scroll])
        data["observation"]["interactions"] = {"inventory": [{"item": "scroll", "readable": True}]}
        policy = Policy()
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "east"}, "explore"))
        data["observation"]["visible_cells"] = [cell("here", -1), cell("frontier")]
        action, reason = policy.choose(data)
        self.assertEqual((action, reason),
                         ({"type": "read", "item": "scroll"}, "test_unknown_magic"))
        policy.accepted(action, data["observation"])
        self.assertEqual(policy.choose(data),
                         (None, "no_disclosed_route"))

    def test_checks_the_current_site_then_tries_an_unknown_scroll(self):
        scroll = {"id": "scroll", "name": "labelled scroll", "class": "scroll",
                  "appearance": "rune Z", "identified": False}
        search = {"id": "rogue.search", "cooldown_remaining": "0", "cost": None}
        data = state([cell("here")], inventory=[scroll], abilities=[search])
        view = data["observation"]
        view["interactions"] = {"inventory": [{"item": "scroll", "readable": True}]}
        for hunger in ("nourished", "hungry"):
            with self.subTest(hunger=hunger):
                view["combat"]["own_stats"]["nutrition"]["hunger"] = hunger
                policy = Policy()
                action, reason = policy.choose(data)
                self.assertEqual((action.get("id"), reason), ("rogue.search", "search"))
                policy.accepted(action, view)
                self.assertEqual(policy.choose(data),
                                 ({"type": "read", "item": "scroll"}, "test_unknown_magic"))

    def test_searches_a_disclosed_floor_edge_when_the_adjacent_tile_is_unseen(self):
        search = {"id": "rogue.search", "cooldown_remaining": "0", "cost": None}
        data = state([cell("dark_here")], abilities=[search])
        policy = Policy()
        action, reason = policy.choose(data)
        self.assertEqual(reason, "search")
        self.assertEqual(action["id"], "rogue.search")
        self.assertEqual(policy.floors[1].graph.edges, {"dark_here": {}})

    def test_escapes_a_disclosed_hold_when_too_injured_to_keep_fighting(self):
        foe = {"id": "foe", "position": {"x": 1, "y": 0, "z": 0}}
        data = state([cell("here"), cell("foe", 1)], actors=[foe], hostiles=["foe"])
        view = data["observation"]
        view["linked_holds"] = [{"target": "current-hold", "holder": "foe"}]
        view["combat"]["hp"] = "4"
        view["interactions"] = {"inventory": []}
        self.assertEqual(Policy().choose(data),
                         ({"type": "escape", "hold": "current-hold"}, "escape_hold"))

    def test_returns_to_a_disclosed_amulet_location_after_it_leaves_view(self):
        policy = Policy()
        amulet = {"id": "previous-handle", "identity": DEFAULT_OBJECTIVE_IDENTITY, "name": AMULET, "quantity": "1"}
        original = state([cell("here"), cell("hall", 1), cell("amulet-cell", 2)],
                         ground=[{**cell("amulet-cell", 2), "item": amulet, "reachable": False}])
        self.assertEqual(policy.choose(original)[1], "approach_loot")
        later = state([cell("here"), cell("hall", 1)])
        self.assertEqual(policy.choose(later),
                         ({"type": "move", "direction": "east"}, "remembered_amulet"))
        self.assertNotEqual(policy.choose(state(original["observation"]["visible_cells"]))[1],
                            "remembered_amulet")

    def test_depth_report_waits_for_stair_work_to_settle(self):
        from contextlib import redirect_stdout
        from io import StringIO
        from rogue_policy_reference import run

        origin = state([cell("exit", down=True)])
        destination = state([cell("arrival", up=True)])
        frames = iter([(origin, True), (origin, False), (destination, True)])

        class Session:
            def observe(self):
                picture, admitting = next(frames)
                return type("Observation", (), {"data": lambda _: {
                    "state": picture, "cursor": 0, "readiness": {"admission": admitting},
                }})()

            def act(self, action, observation):
                return {"status": "confirmed"}

        report = {}
        with redirect_stdout(StringIO()):
            self.assertEqual(run(Session(), Policy(), budget=1, report=report), "action_budget")
        self.assertEqual(report["max_depth"], 2)

    def test_keeps_a_throwable_stack_even_when_starting_weapons_fill_the_stock_limit(self):
        held = [{"id": name, "name": name, "class": "weapon"}
                for name in ("mace", "shortbow", "arrow")]
        dart = {"id": "darts", "name": "dart", "class": "weapon", "identified": True,
                "identity": "item.weapon.dart", "quantity": "12"}
        here = cell("here")
        throw = {"id": "rogue.throw", "cooldown_remaining": "0", "cost": None}
        data = state([here, cell("frontier", 1)], inventory=held, abilities=[throw],
                     ground=[{**here, "item": dart, "reachable": True}])
        self.assertEqual(Policy().choose(data), ({"type": "take", "item": "darts", "quantity": "12"}, "loot"))
        data["observation"]["inventory"].append({**dart, "id": "carried-darts"})
        self.assertEqual(Policy().choose(data), ({"type": "move", "direction": "east"}, "explore"))

    def test_uses_server_selected_carried_projectile_without_discarding_equipped_weapon(self):
        foe = {"id": "foe", "position": {"x": 3, "y": 0, "z": 0}}
        throw = {"id": "rogue.throw", "cooldown_remaining": "0", "cost": None}
        data = state([cell("here"), cell("path", 1), cell("path2", 2), cell("foe", 3)],
                     actors=[foe], hostiles=["foe"], abilities=[throw])
        queries = []

        def choices(ability, phase, offset):
            queries.append((ability, phase, offset))
            return {"choices": [
                {"kind": "throw", "source": {"kind": "equipped", "item": "mace", "socket": 0}},
                {"kind": "throw", "source": {"kind": "carried", "item": "spear", "socket": 1}},
            ], "next_offset": None}

        action, reason = Policy().choose(data, projectile_choices=choices)
        self.assertEqual(reason, "throw_projectile")
        self.assertEqual(action["invocation"]["aim"], {
            "kind": "directional_projectile", "direction": "east",
            "delivery": {"kind": "throw", "source": {"kind": "carried", "item": "spear", "socket": 1}},
        })
        self.assertEqual(queries, [("rogue.throw", {"kind": "sources"}, 0)])

    def test_runner_replans_when_a_queued_throw_returns_without_advancing(self):
        from contextlib import redirect_stdout
        from io import StringIO
        from rogue_policy_reference import run

        foe = {"id": "foe", "position": {"x": 3, "y": 0, "z": 0}}
        throw = {"id": "rogue.throw", "cooldown_remaining": "0", "cost": None}
        data = state([cell("here"), cell("path", 1), cell("path2", 2), cell("foe", 3)],
                     actors=[foe], hostiles=["foe"], abilities=[throw])
        data["observation"]["tick"] = "100"
        actions = []

        class Session:
            def observe(self):
                return type("Observation", (), {"data": lambda _: {
                    "state": data, "cursor": len(actions), "readiness": {"admission": True},
                }})()

            def projectile_choices(self, observation, identity, phase, offset):
                return {"choices": [
                    {"kind": "throw", "source": {"kind": "carried", "item": "spear", "socket": 1}},
                ], "next_offset": None}

            def act(self, action, observation):
                actions.append(action)
                if action["type"] == "move":
                    data["observation"]["tick"] = "200"
                return {"status": "confirmed", "receipt": {"phase": "queued", "intention": str(len(actions))}}

        with redirect_stdout(StringIO()):
            self.assertEqual(run(Session(), Policy(), budget=3), "action_budget")
        self.assertEqual([action["type"] for action in actions], ["use_ability", "move", "use_ability"])

    def test_waits_before_trying_to_walk_while_the_server_reports_a_fall(self):
        data = state([cell("here"), cell("east", 1)])
        view = data["observation"]
        view["motion"] = {"velocity": ["0", "0", "-1600"]}
        policy = Policy()
        self.assertEqual(policy.choose(data), ({"type": "wait"}, "settle_fall"))
        view["motion"]["velocity"][2] = "0"
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "east"}, "explore"))

    def test_waits_once_when_every_disclosed_exit_rejects_movement(self):
        room = [cell("here"), cell("west", -1), cell("east", 1)]
        data = state(room)
        view = data["observation"]
        policy = Policy()
        for direction in ("west", "east"):
            action, _ = policy.choose(data)
            self.assertEqual(action, {"type": "move", "direction": direction})
            self.assertTrue(policy.rejected(action, view, {"code": "invalid_action"}))
        action, reason = policy.choose(data)
        self.assertEqual((action, reason), ({"type": "wait"}, "settle_movement"))
        policy.accepted(action, view, reason=reason)
        for direction in ("west", "east"):
            action, _ = policy.choose(data)
            self.assertEqual(action, {"type": "move", "direction": direction})
            self.assertTrue(policy.rejected(action, view, {"code": "invalid_action"}))
        self.assertEqual(policy.choose(data), (None, "no_disclosed_route"))

    def test_replans_around_a_rejected_disclosed_step(self):
        policy = Policy()
        room = [cell("here"), cell("west", -1), cell("east", 1)]
        data = state(room)
        first, _ = policy.choose(data)
        self.assertEqual(first, {"type": "move", "direction": "west"})
        self.assertTrue(policy.rejected(first, data["observation"], {"code": "invalid_action"}))
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "east"}, "explore"))

    def test_launches_beyond_throw_range_on_a_disclosed_diagonal(self):
        launch = {"id": "rogue.launch", "cooldown_remaining": "0", "cost": None}
        foe = {"id": "foe", "position": {"x": 5, "y": 5, "z": 0}}
        tiles = []
        for x in range(6):
            for y in range(6):
                tile = cell(f"{x}-{y}", x)
                tile["position"]["y"] = y
                tiles.append(tile)
        data = state(tiles, actors=[foe], hostiles=["foe"], abilities=[launch])

        def choices(ability, phase, offset):
            bow = {"item": "bow", "socket": 0}
            row = ({"kind": "launcher", "launcher": bow} if phase["kind"] == "sources"
                   else {"kind": "ammunition", "item": "arrow"})
            return {"choices": [row], "next_offset": None}

        action, reason = Policy().choose(data, projectile_choices=choices)
        self.assertEqual(reason, "launch_projectile")
        self.assertEqual(action["invocation"]["aim"]["direction"], "south_east")
        tiles[1]["wall"] = True
        self.assertNotEqual(Policy().choose(data, projectile_choices=choices)[1], "launch_projectile")

    def test_uses_a_ranged_opening_against_a_dangerous_foe_at_distance(self):
        launch = {"id": "rogue.launch", "cooldown_remaining": "0", "cost": None}
        foe = {"id": "foe", "name": "wraith", "position": {"x": 4, "y": 0, "z": 0}}
        room = [cell("west", -1)] + [cell(str(x), x) for x in range(5)]
        data = state(room, actors=[foe], hostiles=["foe"], abilities=[launch])

        def choices(identity, phase, offset):
            row = ({"kind": "launcher", "launcher": {"item": "bow", "socket": 0}}
                   if phase["kind"] == "sources" else {"kind": "ammunition", "item": "arrow"})
            return {"choices": [row], "next_offset": None}

        self.assertEqual(Policy().choose(data, projectile_choices=choices)[1], "launch_projectile")
        data["observation"]["combat"]["hp"] = "2"
        self.assertEqual(Policy().choose(data, projectile_choices=choices),
                         ({"type": "move", "direction": "west"}, "retreat"))

    def test_launch_uses_paginated_compatible_ammunition_for_the_exact_launcher(self):
        foe = {"id": "foe", "position": {"x": 3, "y": 0, "z": 0}}
        launch = {"id": "rogue.launch", "cooldown_remaining": "0", "cost": None}
        data = state([cell("here"), cell("path", 1), cell("path2", 2), cell("foe", 3)],
                     actors=[foe], hostiles=["foe"], abilities=[launch])
        bow = {"item": "equipped-bow", "socket": 2}
        queries = []

        def choices(identity, phase, offset):
            queries.append((identity, phase, offset))
            if phase["kind"] == "sources":
                return {"choices": [{"kind": "launcher", "launcher": bow}], "next_offset": None}
            if offset == 0:
                return {"choices": [], "next_offset": 4}
            return {"choices": [{"kind": "ammunition", "item": "compatible-stack"}],
                    "next_offset": None}

        action, reason = Policy().choose(data, projectile_choices=choices)
        self.assertEqual(reason, "launch_projectile")
        self.assertEqual(action["invocation"]["aim"]["delivery"],
                         {"kind": "launch", "launcher": bow, "ammunition": "compatible-stack"})
        ammunition = {"kind": "ammunition", "launcher": bow}
        self.assertEqual(queries, [("rogue.launch", {"kind": "sources"}, 0),
                                   ("rogue.launch", ammunition, 0), ("rogue.launch", ammunition, 4)])

    def test_learns_lasting_injury_from_an_observed_attack_in_any_ruleset(self):
        foe = {"id": "first", "name": "crypt moth", "position": {"x": 2, "y": 0, "z": 0}}
        room = [cell("here"), cell("west", -1), cell("east", 1), cell("foe", 2)]
        initial = state(room, actors=[foe], hostiles=["first"])["observation"]
        initial["combat"]["own_stats"]["restorable_losses"] = {
            "attributes": {"strength": 0}, "maximum_health": 0, "hit_dice": "0"}
        policy = Policy()
        policy.observe_combat(initial)

        initial["combat"]["own_stats"]["restorable_losses"]["attributes"]["strength"] = 1
        updates = [{"cursor": {"sequence": "12"}, "events": [
            {"type": "ability", "caster": "first", "target": "player", "outcome": "applied",
             "ability": {"id": "custom.enfeeble"}},
        ]}]
        policy.observe_combat(initial, updates)
        self.assertIn(("actor", "first"), policy.lasting_injury_foes)
        initial["combat"]["hp"] = "7"
        policy.observe_combat(initial, updates)
        self.assertEqual(policy.largest_hits, {})

        another = {**foe, "id": "second"}
        data = state(room, actors=[another], hostiles=["second"])
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "west"}, "retreat"))

    def test_reacts_to_observed_energy_drain_even_with_high_health(self):
        foe = {"id": "foe", "position": {"x": 1, "y": 0, "z": 0}}
        data = state([cell("escape", -1), cell("here"), cell("foe", 1)], actors=[foe],
                     hostiles=["foe"], abilities=[{"id": "basic_melee", "cooldown_remaining": "0", "cost": None}])
        data["observation"]["combat"]["events"] = [
            {"type": "ability", "caster": "foe", "target": "player",
             "ability": {"id": "rogue.energy_drain"}, "outcome": "applied"},
        ]
        self.assertEqual(Policy().choose(data),
                         ({"type": "move", "direction": "west"}, "retreat"))

    def test_pinned_bot_opens_contact_before_slowing_one_of_two_foes(self):
        room = [cell("here"), cell("east", 1), cell("west", -1), cell("push_destination", -2)]
        actors = [{"id": "one", "name": "black unicorn", "position": room[2]["position"]},
                  {"id": "two", "name": "griffin", "position": room[1]["position"]}]
        rod = {"id": "rod", "name": "staff of slow monster", "identified": True}
        ability = {"id": "dislocating_strike", "cooldown_remaining": "0", "cost": None}
        data = state(room, actors=actors, hostiles=["one", "two"], abilities=[ability], inventory=[rod])
        view = data["observation"]
        view["interactions"] = {"inventory": [
            {"item": "rod", "zappable": True, "charges": 2, "device_activation": "direction"}]}
        action, reason = Policy().choose(data)
        self.assertEqual((action["type"], action["id"], reason), ("use_ability", "dislocating_strike", "melee"))
        self.assertEqual(action["invocation"]["aim"]["target"], "one")

    def test_uses_dislocating_strike_to_make_space_when_cornered(self):
        foe = {"id": "foe", "name": "dragon", "position": {"x": 1, "y": 0, "z": 0}}
        attack = {"id": "dislocating_strike", "cooldown_remaining": "0", "cost": None}
        basic = {"id": "basic_melee", "cooldown_remaining": "0", "cost": None}
        landing = cell("landing", 2)
        data = state([cell("here"), cell("foe", 1), landing],
                     actors=[foe], hostiles=["foe"], abilities=[attack, basic])
        self.assertEqual(Policy().choose(data)[0]["id"], "dislocating_strike")
        landing["wall"] = True
        self.assertEqual(Policy().choose(data)[0]["id"], "basic_melee")

    def test_spends_all_eligible_advancement_points_before_resuming_exploration(self):
        from contextlib import redirect_stdout
        from io import StringIO
        from unittest.mock import patch
        from rogue_policy_reference import run

        picture = state([cell("here"), cell("east", 1)])
        picture["observation"]["combat"]["own_stats"]["hit_dice"] = {"total": "4"}
        actions = []

        class Session:
            def observe(self):
                return type("Observation", (), {"data": lambda _: {
                    "state": picture, "cursor": 1, "readiness": {"admission": True}}})()

            def advancement(self, command):
                raise AssertionError(command)

            def act(self, action, observation):
                actions.append(action)
                return {"status": "confirmed"}

        purchases = [{"status": "confirmed"}, {"status": "confirmed"}, None]
        with patch("rogue_policy_reference.earned_training", side_effect=purchases) as train, redirect_stdout(StringIO()):
            self.assertEqual(run(Session(), Policy(), budget=3), "action_budget")
        self.assertEqual(train.call_count, 3)
        self.assertEqual(actions, [{"type": "move", "direction": "east"}])

    def test_earned_training_uses_exact_unspent_owner_and_prefers_carried_spear(self):
        from rogue_policy_reference import earned_training

        view = state([cell("here")], inventory=[
            {"id": "spear", "name": "Ash pike", "identity": "item.weapon.spear", "identified": True},
        ])["observation"]
        view["combat"]["own_stats"]["hit_dice"] = {"total": "3"}
        commands = []

        def advancement(command):
            commands.append(command)
            page = None
            if command == "balances":
                page = {"entries": [{"value": {"owner": "2", "pool": {"id": "advancement"},
                                               "unspent": 1, "allowed": ["skill_rank", "expertise"]}}],
                        "next_offset": None}
            return {"status": "confirmed", "page": page}

        self.assertEqual(earned_training(advancement, view)["status"], "confirmed")
        self.assertEqual(commands, ["overview", "balances",
                                    "purchase 3 advancement expertise rogue.spear", "confirm"])

    def test_starts_a_two_step_escape_when_every_first_step_is_still_in_reach(self):
        foe = {"id": "foe", "name": "vampire", "position": {"x": 0, "y": 1, "z": 0}}
        southwest = cell("southwest", -1)
        southwest["position"]["y"] = 1
        south = cell("foe")
        south["position"]["y"] = 1
        escape = cell("escape", -2)
        room = [cell("here"), south, southwest, cell("west", -1), escape]
        melee = {"id": "strike", "name": "Strike", "cooldown_remaining": "0", "cost": None,
                 "basic_attack": True, "reach": {"kind": "melee"}, "target_kind": "actor"}
        def decision(hp, learned=0):
            player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
            player.observed_loss["foe"] = learned
            turn = player_turn(room, actors=[foe], hostiles=["foe"], abilities=[melee], combat={"hp": hp})
            return choose(player, turn)
        self.assertEqual(decision(2), (Move(Direction.WEST), "break_contact"))
        foe["name"] = "black unicorn"
        self.assertEqual(decision(9, 4)[1], "melee")
        self.assertEqual(decision(8, 4)[1], "break_contact")
        escape["wall"] = True
        self.assertEqual(decision(2)[1], "melee")

    def test_retreat_prefers_an_open_route_over_an_equally_safe_dead_end(self):
        foe = {"id": "foe", "name": "Wraith", "position": {"x": 2, "y": 0, "z": 0}}
        room = [cell("here"), cell("east", 1), cell("foe", 2)]
        for name, x, y, wall in (("north", 0, -1, False), ("far-north", 0, -2, False),
                                 ("south", 0, 1, False), ("south-end", 0, 2, True),
                                 ("south-west", -1, 1, True), ("south-east", 1, 1, True)):
            tile = cell(name, x)
            tile["position"]["y"] = y
            tile["wall"] = wall
            room.append(tile)
        data = state(room, actors=[foe], hostiles=["foe"])
        self.assertEqual(Policy().choose(data),
                         ({"type": "move", "direction": "north"}, "retreat"))

    def test_retreat_can_use_a_clear_diagonal_to_open_distance(self):
        foe = {"id": "foe", "name": "wraith", "position": {"x": 1, "y": 1, "z": 0}}
        room = [cell("here"), cell("west", -1), cell("east", 1)]
        for name, x, y in (("north", 0, -1), ("northwest", -1, -1),
                           ("south", 0, 1), ("foe", 1, 1)):
            tile = cell(name, x)
            tile["position"]["y"] = y
            room.append(tile)
        data = state(room, actors=[foe], hostiles=["foe"])
        self.assertEqual(Policy().choose(data),
                         ({"type": "move", "direction": "north_west"}, "retreat"))

    def test_active_contact_keeps_the_larger_gap_while_routing_to_stairs(self):
        foe = {"id": "foe", "name": "wraith", "position": {"x": 1, "y": 1, "z": 0}}
        room = [cell("here"), cell("west", -1), cell("east", 1)]
        for name, x, y in (("stairs", 0, -1), ("northwest", -1, -1),
                           ("south", 0, 1), ("foe", 1, 1)):
            tile = cell(name, x, up=name == "stairs")
            tile["position"]["y"] = y
            room.append(tile)
        policy = Policy()
        policy.floor = 11
        data = state(room, actors=[foe], hostiles=["foe"])
        self.assertEqual(policy.choose(data),
                         ({"type": "move", "direction": "north_west"}, "retreat_to_stairs"))

    def test_escape_continuation_detects_a_loop_without_reentering_the_start(self):
        floor = Floor()
        floor.graph.edges = {
            "start": {"north": "north", "east": "east"},
            "north": {"south": "start", "east": "corner"},
            "corner": {"west": "north", "south": "east"},
            "east": {"north": "corner", "west": "start"},
        }
        self.assertEqual(floor.escape_continuation("north", {"east"}, {"start"}), (True, 3))
        floor.rejected_edges.add(("corner", "south"))
        self.assertEqual(floor.escape_continuation("north", {"east"}, {"start"}), (False, 2))

    def test_retreat_prefers_a_longer_corridor_over_a_two_step_pocket(self):
        foe = {"id": "foe", "name": "wraith", "position": {"x": 2, "y": 0, "z": 0}}
        room = [cell("here"), cell("east", 1), cell("foe", 2)]
        for y in (-3, -2, -1, 1, 2):
            tile = cell(str(y))
            tile["position"]["y"] = y
            room.append(tile)
        data = state(room, actors=[foe], hostiles=["foe"])
        self.assertEqual(Policy().choose(data),
                         ({"type": "move", "direction": "north"}, "retreat"))

    def test_closes_a_narrow_door_on_a_dragon_but_not_on_an_occupied_cell(self):
        foe = {"id": "foe", "name": "dragon", "position": {"x": 2, "y": 0, "z": 0}}
        entrance = cell("entrance", 1)
        entrance["door"] = {"id": "door", "open": True, "reachable": True}
        sides = [{**cell("upper", 1), "wall": True}, {**cell("lower", 1), "wall": True}]
        sides[0]["position"]["y"], sides[1]["position"]["y"] = -1, 1
        data = state([cell("here"), cell("west", -1), entrance, cell("foe", 2), *sides],
                     actors=[foe], hostiles=["foe"])
        policy = Policy()
        action, reason = policy.choose(data)
        self.assertEqual((action, reason), ({"type": "set_door", "door": "door", "open": False}, "close_retreat_door"))
        policy.accepted(action, data["observation"], reason=reason)
        self.assertNotEqual(policy.choose(data)[1], "close_retreat_door")
        foe["position"]["x"] = 1
        self.assertNotEqual(Policy().choose(data)[1], "close_retreat_door")

    def test_explores_away_from_a_recent_threat_after_losing_sight_of_it(self):
        foe = {"id": "foe", "name": "dragon", "position": {"x": -2, "y": 0, "z": 0}}
        room = [cell("here"), cell("west", -1), cell("lair", -2), cell("east", 1)]
        present = state(room, actors=[foe], hostiles=["foe"])
        hidden = state(room)
        present["observation"]["tick"] = "100"
        hidden["observation"]["tick"] = "200"
        policy = Policy()
        policy.choose(present)
        self.assertEqual(policy.choose(hidden), ({"type": "move", "direction": "east"}, "explore"))
        hidden["observation"]["tick"] = "1500"
        self.assertEqual(policy.choose(hidden), ({"type": "move", "direction": "west"}, "explore"))

    def test_avoids_dragon_and_jabberwock_before_observing_a_hit(self):
        for name in ("dragon", "jabberwock"):
            with self.subTest(name=name):
                foe = {"id": "foe", "name": name, "position": {"x": 2, "y": 0, "z": 0}}
                data = state([cell("here"), cell("retreat", -1), cell("east", 1), cell("foe", 2)],
                             actors=[foe], hostiles=["foe"])
                self.assertEqual(Policy().choose(data), ({"type": "move", "direction": "west"}, "retreat"))

    def test_recognizes_vampires_and_learns_a_disguised_vitality_drainer(self):
        for name in ("vampire", "unknown creature"):
            with self.subTest(name=name):
                foe = {"id": "foe", "name": name, "position": {"x": 2, "y": 0, "z": 0}}
                data = state([cell("here"), cell("west", -1), cell("east", 1), cell("foe", 2)],
                             actors=[foe], hostiles=["foe"])
                if name != "vampire":
                    data["observation"]["combat"]["events"] = [{
                        "type": "ability", "caster": "foe", "target": "player",
                        "ability": {"id": "rogue.drain_vitality"}, "outcome": "unaffected"}]
                policy = Policy()
                self.assertEqual(policy.choose(data), ({"type": "move", "direction": "west"}, "retreat"))
                data["observation"]["combat"]["events"] = []
                self.assertEqual(policy.choose(data), ({"type": "move", "direction": "west"}, "retreat"))

    def test_retreats_from_a_recognized_wraith_before_first_energy_drain(self):
        foe = {"id": "foe", "name": "Wraith", "position": {"x": 1, "y": 0, "z": 0}}
        data = state([cell("escape", -1), cell("here"), cell("foe", 1)], actors=[foe],
                     hostiles=["foe"], abilities=[{"id": "basic_melee", "cooldown_remaining": "0", "cost": None}])
        self.assertEqual(Policy().choose(data),
                         ({"type": "move", "direction": "west"}, "retreat"))

    def test_retreats_before_an_observed_attack_can_consume_half_remaining_health(self):
        foe = {"id": "foe", "position": {"x": 1, "y": 0, "z": 0}}
        melee = {"id": "basic_melee", "cooldown_remaining": "0", "cost": None}
        data = state([cell("escape", -1), cell("here"), cell("foe", 1)],
                     actors=[foe], hostiles=["foe"], abilities=[melee])
        view = data["observation"]
        view["combat"].update(max_hp="36", hp="36")
        policy = Policy()
        self.assertEqual(policy.choose(data)[1], "melee")
        view["combat"].update(hp="22", events=[
            {"type": "ability", "caster": "foe", "target": "player",
             "ability": {"id": "basic_melee"}, "outcome": "applied", "strike": 1},
        ])
        self.assertEqual(policy.choose(data),
                         ({"type": "move", "direction": "west"}, "retreat"))

    def test_cancels_a_dangerous_attack_once_then_resumes_the_same_retry(self):
        from contextlib import redirect_stdout
        from io import StringIO
        import json
        from rogue_policy_reference import run

        foe = {"id": "foe", "position": {"x": 1, "y": 0, "z": 0}}
        data = state([cell("here"), cell("foe", 1)], actors=[foe], hostiles=["foe"],
                     abilities=[{"id": "basic_melee", "cooldown_remaining": "0", "cost": None}])
        paused = state([cell("here"), cell("foe", 1)], actors=[foe], hostiles=["foe"])
        paused["observation"]["ready"] = False
        paused["observation"]["combat"].update(hp="3", preparation_remaining="25", events=[
            {"type": "ability", "caster": "foe", "target": "player", "outcome": "applied"}])
        replan = state([cell("here"), cell("foe", 1)], actors=[foe], hostiles=["foe"],
                       abilities=[{"id": "basic_melee", "cooldown_remaining": "0", "cost": None}])
        replan["observation"]["combat"]["hp"] = "3"
        interrupted_retry = state([cell("here"), cell("foe", 1)], actors=[foe], hostiles=["foe"])
        interrupted_retry["observation"]["ready"] = False
        interrupted_retry["observation"]["combat"].update(hp="2", preparation_remaining="25")
        frames = iter([
            (data, {"admission": True}),
            (paused, {"admission": False, "resume": ["work"], "cancel": ["work"]}),
            (replan, {"admission": True}),
            (interrupted_retry, {"admission": False, "resume": ["retry"], "cancel": ["retry"]}),
            (replan, {"admission": True}),
        ])

        class Session:
            def __init__(self):
                self.interruptions = []

            def observe(self):
                picture, readiness = next(frames)
                return type("Observation", (), {"data": lambda _: {
                    "state": picture, "cursor": 1, "readiness": readiness,
                }})()

            def act(self, action, observation):
                return {"status": "confirmed"}

            def cancel(self, observation):
                self.interruptions.append("cancel")
                return {"status": "confirmed"}

            def resume(self, observation):
                self.interruptions.append("resume")
                return {"status": "confirmed"}

        session = Session()
        output = StringIO()
        with redirect_stdout(output):
            self.assertEqual(run(session, Policy(), budget=4), "action_budget")
        self.assertEqual(session.interruptions, ["cancel", "resume"])
        interrupted = json.loads(output.getvalue().splitlines()[1])
        self.assertEqual(interrupted["hostiles"], [{"actor": "foe", "name": None,
                                                     "injury": None, "cells": [[1, 0, 0]]}])
        self.assertEqual((interrupted["hp"], interrupted["max_hp"]), ("3", "10"))

    def test_resumes_an_attack_when_health_has_not_worsened_since_admission(self):
        from contextlib import redirect_stdout
        from io import StringIO
        from rogue_policy_reference import run

        foe = {"id": "foe", "position": {"x": 1, "y": 0, "z": 0}}
        data = state([cell("here"), cell("foe", 1)], actors=[foe], hostiles=["foe"],
                     abilities=[{"id": "basic_melee", "cooldown_remaining": "0", "cost": None}])
        data["observation"]["combat"]["hp"] = "3"
        paused = state([cell("here"), cell("foe", 1)], actors=[foe], hostiles=["foe"])
        paused["observation"]["ready"] = False
        paused["observation"]["combat"].update(hp="3", preparation_remaining="25")
        frames = iter([
            (data, {"admission": True}),
            (paused, {"admission": False, "resume": ["work"], "cancel": ["work"]}),
            (data, {"admission": True}),
        ])

        class Session:
            def __init__(self):
                self.resumed = False

            def observe(self):
                picture, readiness = next(frames)
                return type("Observation", (), {"data": lambda _: {
                    "state": picture, "cursor": 1, "readiness": readiness,
                }})()

            def act(self, action, observation):
                return {"status": "confirmed"}

            def cancel(self, observation):
                raise AssertionError("Cancelled even though health did not worsen")

            def resume(self, observation):
                self.resumed = True
                return {"status": "confirmed"}

        session = Session()
        policy = Policy()
        policy.largest_hits[("actor", "foe")] = 3
        with redirect_stdout(StringIO()):
            self.assertEqual(run(session, policy, budget=2), "action_budget")
        self.assertTrue(session.resumed)

    def test_reobserves_and_waits_before_retrying_a_rejected_stair(self):
        policy = Policy()
        data = state([cell("stair", down=True)])
        action, _ = policy.choose(data)
        self.assertTrue(policy.rejected(action, data["observation"], {"code": "invalid_action"}))
        retry, reason = policy.choose(data)
        self.assertEqual((retry, reason), ({"type": "wait"}, "stair_backoff"))
        policy.accepted(retry, data["observation"])
        self.assertEqual(policy.choose(data), (action, "descend"))

    def test_successful_crossing_resets_stair_rejection_budget(self):
        policy = Policy()
        upper = state([cell("upper", down=True)])
        lower = state([cell("lower", up=True)])
        down, _ = policy.choose(upper)
        up = {"type": "move", "direction": "up"}
        rejected = {"code": "invalid_action"}
        for _ in range(4):
            self.assertTrue(policy.rejected(down, upper["observation"], rejected))
            wait, reason = policy.choose(upper)
            self.assertEqual(reason, "stair_backoff")
            policy.accepted(wait, upper["observation"])
            self.assertEqual(policy.choose(upper), (down, "descend"))
            policy.accepted(down, upper["observation"])
            policy.observe_stair(lower["observation"]["visible_cells"][0])
            self.assertEqual(policy.floor, 2)
            policy.accepted(up, lower["observation"])
            policy.observe_stair(upper["observation"]["visible_cells"][0])
            self.assertEqual(policy.floor, 1)

    def test_recovers_after_retreating_upstairs_at_more_than_half_health(self):
        foe = {"id": "foe", "name": "Wraith", "position": {"x": 2, "y": 0, "z": 0}}
        lower = state([cell("lower", up=True)], actors=[foe], hostiles=["foe"])
        upper = state([cell("upper", down=True)])
        for picture in (lower, upper):
            picture["observation"]["combat"].update(hp="24", max_hp="42")
        policy = Policy()
        policy.floor = 17
        action, reason = policy.choose(lower)
        self.assertEqual(reason, "retreat_upstairs")
        policy.accepted(action, lower["observation"], reason=reason)
        self.assertEqual(policy.choose(upper), ({"type": "wait"}, "recover"))
        upper["observation"]["combat"]["hp"] = "34"
        self.assertEqual(policy.choose(upper), ({"type": "move", "direction": "down"}, "descend"))

    def test_does_not_reverse_a_recovery_retreat_to_escape_a_new_foe(self):
        policy = Policy()
        policy.floor = 24
        lower = state([cell("lower", up=True)])
        retreat = {"type": "move", "direction": "up"}
        policy.accepted(retreat, lower["observation"], reason="retreat_upstairs")
        foe = {"id": "foe", "name": "jabberwock", "position": {"x": 1, "y": 0, "z": 0}}
        upper = state([cell("upper", down=True), cell("foe_cell", 1), cell("escape", -1)],
                      actors=[foe], hostiles=["foe"])
        policy.observe_stair(upper["observation"]["visible_cells"][0])
        upper["observation"]["combat"].update(hp="11", max_hp="66")
        action, _ = policy.choose(upper)
        self.assertEqual(action, {"type": "move", "direction": "west"})

    def test_recovers_on_upper_landing_while_pursuer_is_visible_below(self):
        foe = {"id": "foe", "name": "wraith", "position": {"x": 1, "y": 0, "z": 0}}
        lower = state([cell("lower", up=True), cell("threat", 1)], actors=[foe], hostiles=["foe"])
        projected = cell("lower", up=True)
        projected["position"]["z"] = -20
        silhouette = {**foe, "position": {"x": 0, "y": 0, "z": -18}}
        upper = state([cell("upper", down=True), projected], actors=[silhouette], hostiles=["foe"])
        for picture in (lower, upper):
            picture["observation"]["combat"].update(hp="24", max_hp="42")
        policy = Policy()
        policy.floor = 18
        action, reason = policy.choose(lower)
        self.assertEqual(reason, "retreat_upstairs")
        policy.accepted(action, lower["observation"], reason=reason)
        self.assertEqual(policy.choose(upper), ({"type": "wait"}, "recover"))
        upper["observation"]["combat"]["hp"] = "34"
        self.assertEqual(policy.choose(upper), ({"type": "move", "direction": "down"}, "descend"))

    def test_configured_retrieval_depth_changes_when_exploration_takes_priority(self):
        data = state([cell("stairs", down=True), cell("frontier", 1)])
        policy = Policy(retrieval_depth=3)
        policy.floor = 2
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "down"}, "descend"))
        policy.floor = 3
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "east"}, "explore"))

    def test_takes_known_descent_before_chasing_optional_loot(self):
        sword = {"id": "sword", "class": "weapon", "name": "long sword",
                 "quantity": "1", "identified": True}
        data = state([cell("stair", down=True), cell("gear", 1)],
                     ground=[{**cell("gear", 1), "item": sword, "reachable": True}])
        self.assertEqual(Policy().choose(data),
                         ({"type": "move", "direction": "down"}, "descend"))

    def test_does_not_retry_stair_immediately_when_hostile_is_visible(self):
        foe = {"id": "foe", "position": {"x": 3, "y": 0, "z": 0}}
        policy = Policy()
        room = [cell("stair", down=True), cell("foe", 3)]
        initial = state(room)
        action, _ = policy.choose(initial)
        self.assertTrue(policy.rejected(action, initial["observation"], {"code": "invalid_action"}))
        threatened = state(room, actors=[foe], hostiles=["foe"])
        self.assertEqual(policy.choose(threatened),
                         ({"type": "wait"}, "stair_backoff"))

    def test_stops_chasing_an_evasive_foe_after_bounded_pursuit(self):
        foe = {"id": "evasive", "position": {"x": 2, "y": 0, "z": 0}}
        room = [cell("here"), cell("contact", 1), cell("foe", 2), cell("way_down", -1, down=True)]
        data = state(room, actors=[foe], hostiles=["evasive"])
        policy = Policy()
        for _ in range(12):
            action, reason = policy.choose(data)
            self.assertEqual(reason, "approach_foe")
            policy.accepted(action, data["observation"], reason=reason)
        self.assertEqual(policy.choose(data),
                         ({"type": "move", "direction": "west"}, "find_descent"))

    def test_waits_for_slow_paid_work_while_game_time_continues_advancing(self):
        from unittest.mock import patch
        from rogue_policy_reference import run

        def view_at(tick):
            picture = state([cell("here")])
            picture["observation"]["tick"] = str(tick)
            return picture

        frames = iter([(view_at(0), False), (view_at(100), False),
                       (view_at(200), False), (view_at(300), True)])

        class Session:
            def observe(self):
                picture, ready = next(frames)
                return type("Observation", (), {"data": lambda _: {
                    "state": picture, "cursor": 0, "readiness": {"admission": ready},
                }})()

        report = {}
        policy = Policy()
        policy.return_travel_ticks = {1: 400, 2: 700}
        with patch("rogue_policy_reference.time.monotonic", side_effect=range(0, 150, 10)), \
                patch("rogue_policy_reference.time.sleep"):
            self.assertEqual(run(Session(), policy, budget=0, report=report), "action_budget")
        self.assertEqual(report["tick"], 300)
        self.assertEqual(report["estimated_ascent_ticks_by_floor"], {1: 400, 2: 700})
        self.assertEqual(report["estimated_ascent_ticks"], 1100)

    def test_waits_before_retrying_a_canceled_stalled_stair(self):
        self.check_stalled_move([cell("here", down=True)], "down")

    def test_waits_before_retrying_a_canceled_stalled_walk(self):
        self.check_stalled_move([cell("here"), cell("goal", 1)], "east")

    def test_retries_a_canceled_move_immediately_if_the_simulation_advanced(self):
        self.check_stalled_move([cell("here"), cell("goal", 1)], "east", advances=True)

    def check_stalled_move(self, cells, direction, *, advances=False):
        from contextlib import redirect_stdout
        from io import StringIO
        from unittest.mock import patch
        from rogue_policy_reference import run

        ready = state(cells)
        ready["observation"]["tick"] = "100"
        queued = {"intention": "work", "phase": "queued"}
        frames = iter([
            {"admission": True}, {"admission": False, "cancel": ["work"]},
            {"admission": True}, {"admission": True}, {"admission": True},
        ])

        actions = []

        class Session:
            def observe(self):
                readiness = next(frames)
                return type("Observation", (), {"data": lambda _: {
                    "state": ready, "cursor": 0, "readiness": readiness,
                    "intentions": [] if readiness["admission"] else [queued],
                }})()

            def act(self, action, observation):
                actions.append(action.get("direction", action["type"]))
                return {"status": "confirmed", "receipt": {"intention": "work"}}

            def cancel(self, observation):
                actions.append("cancel")
                if advances:
                    ready["observation"]["tick"] = "200"
                return {"status": "confirmed"}

        expected = [direction, "cancel", direction]
        if not advances:
            expected.insert(2, "wait")
        output = StringIO()
        with patch("rogue_policy_reference.time.monotonic", side_effect=[0, 1, 22, 23, 24, 25]), \
                patch("rogue_policy_reference.time.sleep"), redirect_stdout(output):
            self.assertEqual(run(Session(), Policy(), budget=len(expected)), "action_budget")
        self.assertEqual(actions, expected)

    def test_submits_one_offered_wait_when_recovery_stops_advancing(self):
        from contextlib import redirect_stdout
        from io import StringIO
        from unittest.mock import patch
        from rogue_policy_reference import run

        initial = state([cell("stairs", down=True)])
        initial["observation"].update(tick="100", ready=False)
        recovered = state([cell("stairs", down=True)])
        recovered["observation"]["tick"] = "200"
        frames = iter([initial, initial, recovered])
        actions = []

        class Session:
            def observe(self):
                picture = next(frames)
                return type("Observation", (), {"data": lambda _: {
                    "state": picture, "cursor": 0, "readiness": {"admission": True},
                    "intentions": [],
                }})()

            def act(self, action, observation):
                actions.append(action)
                return {"status": "confirmed", "receipt": {"intention": "wait"}}

        with patch("rogue_policy_reference.time.monotonic", side_effect=[0, 1, 22, 23]), \
                patch("rogue_policy_reference.time.sleep"), redirect_stdout(StringIO()):
            self.assertEqual(run(Session(), Policy(), budget=1), "action_budget")
        self.assertEqual(actions, [{"type": "wait"}])

    def test_waits_for_confirmed_identification_prompt_to_clear(self):
        from contextlib import redirect_stdout
        from io import StringIO
        from unittest.mock import patch
        from rogue_policy_reference import run

        pending = state([cell("here")])
        pending["observation"]["interactions"] = {"identification": {"choice": "offer", "items": []}}
        frames = iter([pending, pending, state([cell("here")])])
        actions = []

        class Session:
            def observe(self):
                picture = next(frames)
                return type("Observation", (), {"data": lambda _: {
                    "state": picture, "cursor": 0, "readiness": {"admission": True}}})()

            def act(self, action, observation):
                actions.append(action)
                return {"status": "confirmed"}

        with patch("rogue_policy_reference.time.sleep"), redirect_stdout(StringIO()):
            self.assertEqual(run(Session(), Policy(), budget=1), "action_budget")
        self.assertEqual(actions, [{"type": "identify", "choice": "offer", "item": None}])
        self.assertIsNone(next(frames, None))

    def test_replans_after_the_server_reports_actor_busy(self):
        from contextlib import redirect_stdout
        from io import StringIO
        from rogue_policy_reference import run

        initial = state([cell("stairs", down=True)])
        held = state([cell("stairs", down=True)])
        held["observation"]["linked_holds"] = [{"target": "hold"}]
        views = iter([initial, held, initial])
        actions = []

        class Session:
            def observe(self):
                picture = next(views)
                return type("Observation", (), {"data": lambda _: {
                    "state": picture, "cursor": len(actions), "readiness": {"admission": True}}})()

            def act(self, action, observation):
                actions.append(action)
                if len(actions) == 1:
                    return {"status": "rejected", "code": "actor_busy"}
                return {"status": "confirmed"}

        with redirect_stdout(StringIO()):
            self.assertEqual(run(Session(), Policy(), budget=2), "action_budget")
        self.assertEqual(actions, [{"type": "move", "direction": "down"}, {"type": "escape", "hold": "hold"}])

    def test_tests_a_directional_unknown_wand_once_from_safe_distance(self):
        foe = {"id": "foe", "position": {"x": 3, "y": 0, "z": 0}}
        wand = {"id": "wand", "name": "oak wand", "class": "wand",
                "identified": False, "appearance": "oak wand"}
        room = [cell("here"), cell("path", 1), cell("path2", 2), cell("foe", 3), cell("clear", 4)]
        data = state(room, actors=[foe], hostiles=["foe"], inventory=[wand])
        view = data["observation"]
        view["interactions"] = {"inventory": [
            {"item": "wand", "device_activation": "direction", "charges": None, "zappable": True},
        ]}
        policy = Policy()
        action, reason = policy.choose(data)
        self.assertEqual(reason, "test_unknown_device")
        self.assertEqual(action, {"type": "zap", "item": "wand",
                                  "aim": {"kind": "direction", "direction": "east"}})
        policy.accepted(action, view, reason=reason)
        self.assertNotEqual(policy.choose(data)[1], "test_unknown_device")

    def test_retreat_threshold_accounts_for_a_disclosed_multi_strike_attacker(self):
        foe = {"id": "foe", "position": {"x": 1, "y": 0, "z": 0}}
        melee = {"id": "basic_melee", "cooldown_remaining": "0", "cost": None}
        data = state([cell("escape", -1), cell("here"), cell("foe", 1)],
                     actors=[foe], hostiles=["foe"], abilities=[melee])
        view = data["observation"]
        view["combat"].update(max_hp="41", hp="33")
        policy = Policy()
        self.assertEqual(policy.choose(data)[1], "melee")
        view["combat"].update(hp="23", events=[
            {"type": "ability", "caster": "foe", "target": "player",
             "ability": {"id": "basic_melee"}, "outcome": "applied", "strike": 2},
        ])
        self.assertEqual(policy.choose(data),
                         ({"type": "move", "direction": "west"}, "retreat"))

    def test_reads_known_hold_scroll_when_cornered_by_a_dangerous_hostile(self):
        foe = {"id": "foe", "name": "Wraith", "position": {"x": 1, "y": 0, "z": 0}}
        scroll = {"id": "hold", "name": "scroll of hold monster", "identity": "item.scroll.hold-monster", "identified": True}
        data = state([cell("here"), cell("foe", 1)], actors=[foe], hostiles=["foe"],
                     inventory=[scroll])
        data["observation"]["interactions"] = {"inventory": [{"item": "hold", "readable": True}]}
        self.assertEqual(Policy().choose(data),
                         ({"type": "read", "item": "hold"}, "hold_threat"))

    def test_uses_haste_with_room_to_escape_a_dangerous_pursuer(self):
        foe = {"id": "foe", "name": "vampire", "position": {"x": 3, "y": 0, "z": 0}}
        potion = {"id": "haste", "name": "potion of haste", "identity": "item.potion.haste", "identified": True}
        room = [cell("here"), cell("west", -1), cell("east", 1), cell("path", 2), cell("foe", 3)]
        data = state(room, actors=[foe], hostiles=["foe"], inventory=[potion])
        view = data["observation"]
        view["interactions"] = {"inventory": [{"item": "haste", "drinkable": True}]}
        self.assertEqual(Policy().choose(data), ({"type": "drink", "item": "haste"}, "combat_haste"))
        view["combat"]["own_stats"]["conditions"] = [{"condition": {"id": "rogue.hasted"}}]
        self.assertEqual(Policy().choose(data), ({"type": "move", "direction": "west"}, "retreat"))

    def test_uses_haste_when_cornered_without_taking_a_second_active_dose(self):
        foe = {"id": "foe", "name": "Wraith", "position": {"x": 1, "y": 0, "z": 0}}
        haste = {"id": "haste", "name": "potion of haste", "identity": "item.potion.haste", "identified": True}
        melee = {"id": "basic_melee", "cooldown_remaining": "0", "cost": None}
        data = state([cell("here"), cell("foe", 1)], actors=[foe], hostiles=["foe"],
                     abilities=[melee], inventory=[haste])
        view = data["observation"]
        view["interactions"] = {"inventory": [{"item": "haste", "drinkable": True}]}
        policy = Policy()
        self.assertEqual(policy.choose(data), ({"type": "drink", "item": "haste"}, "combat_haste"))
        view["combat"]["own_stats"]["conditions"] = [{"condition": {"id": "rogue.hasted"}}]
        self.assertEqual(policy.choose(data)[1], "melee")

    def test_slows_a_dangerous_pursuer_before_it_reaches_melee(self):
        foe = {"id": "foe", "name": "Wraith", "position": {"x": 3, "y": 0, "z": 0}}
        staff = {"id": "staff", "name": "staff of slow monster", "identified": True}
        data = state([cell("here"), cell("retreat", -1), cell("one", 1), cell("two", 2), cell("foe", 3)],
                     actors=[foe], hostiles=["foe"], inventory=[staff])
        view = data["observation"]
        view["interactions"] = {"inventory": [
            {"item": "staff", "zappable": True, "charges": 2, "device_activation": "direction"}]}
        policy = Policy()
        action, reason = policy.choose(data)
        self.assertEqual((action, reason), ({"type": "zap", "item": "staff", "aim": {
            "kind": "direction", "direction": "east"}}, "control_threat"))
        policy.accepted(action, view, reason=reason)
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "west"}, "retreat"))

    def test_uses_known_slow_staff_once_against_a_cornering_foe(self):
        foe = {"id": "foe", "name": "Wraith", "position": {"x": 1, "y": 0, "z": 0}}
        staff = {"id": "staff", "name": "staff of slow monster", "identified": True}
        melee = {"id": "basic_melee", "cooldown_remaining": "0", "cost": None}
        data = state([cell("here"), cell("foe", 1)], actors=[foe], hostiles=["foe"],
                     abilities=[melee], inventory=[staff])
        view = data["observation"]
        offer = {"item": "staff", "zappable": True, "charges": None, "device_activation": "direction"}
        view["interactions"] = {"inventory": [offer]}
        policy = Policy()
        action, reason = policy.choose(data)
        self.assertEqual((action, reason), ({"type": "zap", "item": "staff", "aim": {
            "kind": "direction", "direction": "east"}}, "control_threat"))
        policy.accepted(action, view, reason=reason)
        self.assertEqual(policy.choose(data)[1], "melee")
        offer["charges"] = 0
        self.assertEqual(Policy().choose(data)[1], "melee")

    def test_cancellation_targets_an_enemy_with_a_known_special_ability(self):
        foe = {"id": "foe", "name": "vampire", "position": {"x": 3, "y": 0, "z": 0}}
        rod = {"id": "rod", "name": "staff of cancellation", "identified": True}
        data = state([cell("here"), cell("one", 1), cell("two", 2), cell("foe", 3)],
                     actors=[foe], hostiles=["foe"], inventory=[rod])
        view = data["observation"]
        view["interactions"] = {"inventory": [
            {"item": "rod", "zappable": True, "charges": 2, "device_activation": "direction"}]}
        policy = Policy()
        action, reason = policy.choose(data)
        self.assertEqual((action, reason), ({"type": "zap", "item": "rod",
                                          "aim": {"kind": "direction", "direction": "east"}}, "control_threat"))
        policy.accepted(action, view, reason=reason)
        self.assertNotEqual(policy.choose(data)[1], "control_threat")
        foe["name"] = "griffin"
        self.assertNotEqual(Policy().choose(data)[1], "control_threat")

    def test_control_staff_can_aim_diagonally_through_an_open_corner(self):
        foe = {"id": "foe", "name": "Wraith", "position": {"x": 1, "y": 1, "z": 0}}
        staff = {"id": "staff", "name": "staff of slow monster", "identified": True}
        south = cell("south")
        south["position"]["y"] = 1
        target = cell("foe", 1)
        target["position"]["y"] = 1
        data = state([cell("here"), cell("east", 1), south, target],
                     actors=[foe], hostiles=["foe"], inventory=[staff])
        data["observation"]["interactions"] = {"inventory": [
            {"item": "staff", "zappable": True, "charges": 2, "device_activation": "direction"}]}
        policy = Policy()
        self.assertEqual(policy.choose(data), ({"type": "zap", "item": "staff", "aim": {
            "kind": "direction", "direction": "south_east"}}, "control_threat"))
        south["wall"] = True
        self.assertNotEqual(policy.choose(data)[1], "control_threat")

    def test_uses_known_growth_potion_when_safe(self):
        item = {"id": "growth", "name": "Ascension draught", "identity": "item.potion.raise-level", "class": "potion", "identified": True}
        data = state([cell("here"), cell("frontier", 1)], inventory=[item])
        data["observation"]["interactions"] = {"inventory": [{"item": "growth", "drinkable": True}]}
        self.assertEqual(Policy().choose(data),
                         ({"type": "drink", "item": "growth"}, "permanent_growth"))

    def test_search_prefers_observed_wall_with_floor_disclosed_beyond_it(self):
        def wall(key, x, y):
            result = cell(key, x)
            result["wall"] = True
            result["position"]["y"] = y
            return result

        search = {"id": "rogue.search", "cooldown_remaining": "0", "cost": None}
        data = state([cell("here"), cell("passage", 1), wall("between", 2, 0),
                      cell("other_side", 3), wall("outer", 0, -1)], abilities=[search])
        policy = Policy()
        policy.floors[1] = Floor(visited={"here", "passage", "other_side"})
        self.assertEqual(policy.choose(data),
                         ({"type": "move", "direction": "east"}, "search"))

    def test_uses_a_known_self_aimed_light_device_in_darkness(self):
        dark = {**cell("here"), "lit": False}
        wand = {"id": "wand", "name": "Illuminator", "identity": "item.wand.wand-light", "identified": True}
        data = state([dark, cell("east", 1)], inventory=[wand])
        view = data["observation"]
        view["interactions"] = {"inventory": [
            {"item": "wand", "zappable": True, "charges": None, "device_activation": "self_target"}]}
        policy = Policy()
        action, reason = policy.choose(data)
        self.assertEqual((action, reason), ({"type": "zap", "item": "wand",
                                            "aim": {"kind": "self_target"}}, "light_room"))
        policy.accepted(action, view, reason=reason)
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "east"}, "explore"))

    def test_search_prefers_a_site_that_covers_more_untried_boundaries_at_equal_cost(self):
        room = [cell("here"), cell("west", -1), cell("east", 1)]
        for x, y in ((1, -1), (1, 1), (-1, -1)):
            wall = {**cell(f"wall-{x}-{y}", x), "wall": True}
            wall["position"]["y"] = y
            room.append(wall)
        search = {"id": "rogue.search", "cooldown_remaining": "0", "cost": None}
        policy = Policy()
        memory = Floor(visited={"here", "west", "east"})
        memory.tactile_attempts.update({key: 4 for key in memory.visited})
        policy.floors[1] = memory
        self.assertEqual(policy.choose(state(room, abilities=[search])),
                         ({"type": "move", "direction": "east"}, "search"))

    def test_searches_underfoot_corridor_end_before_backtracking_to_frontiers(self):
        room = [cell("end"), cell("back", -1), cell("frontier", -2),
                {**cell("front", 1), "wall": True}]
        for y in (-1, 1):
            wall = {**cell(f"wall-{y}"), "wall": True}
            wall["position"]["y"] = y
            room.append(wall)
        search = {"id": "rogue.search", "cooldown_remaining": "0", "cost": None}
        data = state(room, abilities=[search])
        policy = Policy()
        action, reason = policy.choose(data)
        self.assertEqual((action["type"], reason), ("use_ability", "search"))
        policy.accepted(action, data["observation"])
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "west"}, "explore"))

    def test_searches_a_disclosed_corridor_end_before_a_room_wall(self):
        room = [cell("room"), cell("end", 1), {**cell("sealed", 2), "wall": True}]
        for x, y in ((0, -1), (1, -1), (1, 1)):
            wall = {**cell(f"wall-{x}-{y}", x), "wall": True}
            wall["position"]["y"] = y
            room.append(wall)
        search = {"id": "rogue.search", "cooldown_remaining": "0", "cost": None}
        policy = Policy()
        policy.floors[1] = Floor(visited={"room", "end"})
        self.assertEqual(policy.choose(state(room, abilities=[search])),
                         ({"type": "move", "direction": "east"}, "search"))

    def test_rechecks_a_corridor_end_before_sweeping_room_walls(self):
        room = [cell("room"), cell("end", 1), {**cell("cap", 2), "wall": True}]
        for name, x, y in (("room-wall", 0, -1), ("left-wall", 1, -1), ("right-wall", 1, 1)):
            wall = {**cell(name, x), "wall": True}
            wall["position"]["y"] = y
            room.append(wall)
        data = state(room, abilities=[{"id": "rogue.search", "cooldown_remaining": "0", "cost": None}])
        memory = Floor(visited={"room", "end"})
        memory.search_attempts.update(dict.fromkeys(("cap", "left-wall", "right-wall"), 1))
        memory.tactile_attempts.update({"room": 4, "end": 4})
        policy = Policy()
        policy.floors[1] = memory
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "east"}, "search"))
        memory.search_attempts.update(dict.fromkeys(("cap", "left-wall", "right-wall"), 2))
        self.assertEqual(policy.choose(data)[0]["type"], "use_ability")

    def test_uses_a_clear_diagonal_between_search_sites(self):
        room = [cell("here"), cell("west", -1), cell("north"), cell("site", -1),
                {**cell("wall", -2), "wall": True}]
        for tile in room[2:]:
            tile["position"]["y"] = -1
        search = {"id": "rogue.search", "cooldown_remaining": "0", "cost": None}
        policy = Policy()
        memory = Floor(visited={"here", "west", "north", "site"})
        memory.tactile_attempts.update({key: 4 for key in memory.visited})
        policy.floors[1] = memory
        self.assertEqual(policy.choose(state(room, abilities=[search])),
                         ({"type": "move", "direction": "north_west"}, "search"))

    def test_mapping_hints_require_unique_wall_alignment(self):
        from rogue_policy_reference import MappingChart, position

        tiles = [cell("here"), cell("west", -1), cell("north"), cell("south")]
        tiles[2]["position"]["y"] = -1
        tiles[3]["position"]["y"] = 1
        for tile in tiles[1:]:
            tile["wall"] = True
        faces = [
            {"component": "0", "position": p, "mark": {"fact": {"kind": "wall_face", "normal": n}}}
            for p, n in [([8, 19, 0], [0, 1, 0]), ([9, 20, 0], [-1, 0, 0]),
                         ([7, 20, 0], [1, 0, 0])]
        ]
        secret = {"component": "0", "position": [8, 19, 0], "mark": {"fact": {"kind": "secret_door"}}}
        chart = {"components": [{"component": "0", "alignment": "aligned"}], "rows": [*faces, secret]}
        visible = {position(tile): tile for tile in tiles}
        self.assertEqual(MappingChart(chart).search_walls(visible), {"west"})
        chart["rows"] += [{**r, "position": [r["position"][0] + 10, *r["position"][1:]]} for r in faces]
        self.assertEqual(MappingChart(chart).search_walls(visible), set())

    def test_reads_known_mapping_when_exploration_reaches_blind_search(self):
        scroll = {"name": "Hidden atlas", "identity": "item.scroll.magic-mapping", "id": "scroll", "identified": True}
        wall = {**cell("wall", 1), "wall": True}
        search = {"id": "rogue.search", "cooldown_remaining": "0", "cost": None}
        data = state([cell("here"), wall], abilities=[search], inventory=[scroll])
        view = data["observation"]
        view["charts"] = [{"kind": "mapping", "supported": True, "acquired_at": None, "chart": None}]
        view["interactions"] = {"inventory": [{"item": "scroll", "readable": True}]}
        policy = Policy()
        action, reason = policy.choose(data, chart=lambda kind: None)
        self.assertEqual((action, reason), ({"type": "read", "item": "scroll"}, "map_floor"))
        policy.accepted(action, view, reason=reason)
        self.assertEqual(policy.choose(data, chart=lambda kind: None)[1], "search")

    def test_repeated_search_uses_nearby_wall_after_the_first_sweep(self):
        near = {**cell("near_wall", -1), "wall": True}
        far = {**cell("far_wall", 2), "wall": True}
        search = {"id": "rogue.search", "cooldown_remaining": "0", "cost": None}
        data = state([near, cell("here"), cell("far_side", 1), far], abilities=[search])
        policy = Policy()
        memory = Floor(visited={"here", "far_side"})
        memory.tactile_attempts.update({"here": 4, "far_side": 4})
        memory.search_attempts.update({"near_wall": 2, "far_wall": 1})
        policy.floors[1] = memory
        action, reason = policy.choose(data)
        self.assertEqual(reason, "search")
        self.assertEqual(action["type"], "use_ability")
        self.assertEqual(action["id"], "rogue.search")

        memory.search_attempts["far_wall"] = 0
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "east"}, "search"))

    def test_rechecks_local_wall_before_a_distant_first_search(self):
        search = {"id": "rogue.search", "cooldown_remaining": "0", "cost": None}
        near = {**cell("near_wall", -1), "wall": True}
        far = {**cell("far_wall", 3), "wall": True}
        data = state([near, cell("here"), cell("passage", 1), cell("far_side", 2), far], abilities=[search])
        policy = Policy()
        memory = policy.floors[1] = Floor(visited={"here", "passage", "far_side"})
        memory.tactile_attempts.update({"here": 1, "passage": 4, "far_side": 4})
        memory.search_attempts["near_wall"] = 1
        step, reason = policy.choose(data)
        self.assertEqual((step["type"], step["id"], reason), ("use_ability", "rogue.search", "search"))
        policy.accepted(step, data["observation"], reason=reason)
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "east"}, "search"))

    def test_equips_a_recognized_searching_ring_without_stacking_duplicates(self):
        slot = {"id": "ring"}
        items = [{"id": item, "name": "Discovery band", "identity": "item.ring.ring-searching", "identified": True}
                 for item in ("first", "duplicate")]
        offers = [{"item": item["id"], "slot": slot, "equipped_slot": None,
                   "known_cursed": False, "known_equipment": None} for item in items]
        data = state([cell("here")], inventory=items)
        data["observation"]["interactions"] = {"slots": [slot, slot], "inventory": offers}
        policy = Policy()
        self.assertEqual(policy.choose(data),
                         ({"type": "equip", "item": "first", "slot": 0}, "upgrade_equipment"))
        offers[0]["equipped_slot"] = 0
        self.assertNotEqual(policy.choose(data)[1], "upgrade_equipment")

    def test_equips_known_protection_before_descending_safe_stairs(self):
        slot = {"id": "ring"}
        ring = {"id": "protection", "name": "Guardian band", "identity": "item.ring.ring-protection", "identified": True}
        offer = {"item": ring["id"], "slot": slot, "equipped_slot": None,
                 "known_cursed": False, "known_equipment": None}
        data = state([cell("stairs", down=True)], inventory=[ring])
        data["observation"]["interactions"] = {"slots": [slot], "inventory": [offer]}
        policy = Policy()
        self.assertEqual(policy.choose(data),
                         ({"type": "equip", "item": ring["id"], "slot": 0}, "upgrade_equipment"))
        offer["equipped_slot"] = 0
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "down"}, "descend"))

    def test_avoids_optional_pursuit_when_deep_floor_descent_is_known(self):
        foe = {"id": "foe", "position": {"x": -2, "y": 0, "z": 0}}
        data = state([cell("foe", -2), cell("contact", -1), cell("here"), cell("descent", 1, down=True)],
                     actors=[foe], hostiles=["foe"])
        policy = Policy()
        policy.floor = 17
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "east"}, "find_descent"))

    def test_explores_away_from_a_deep_floor_hostile_before_pursuing(self):
        foe = {"id": "foe", "position": {"x": 2, "y": 0, "z": 0}}
        data = state([cell("frontier", -1), cell("here"), cell("contact", 1), cell("foe", 2)],
                     actors=[foe], hostiles=["foe"])
        policy = Policy()
        policy.floor = 17
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "west"}, "avoid_foe"))

    def test_deep_floor_avoidance_yields_after_revisiting_the_same_cell(self):
        foe = {"id": "foe", "position": {"x": 2, "y": 0, "z": 0}}
        room = [cell("detour", -1), cell("here"), cell("contact", 1), cell("foe", 2)]
        data = state(room, actors=[foe], hostiles=["foe"])
        policy = Policy()
        policy.floor = 17
        for _ in range(2):
            action, reason = policy.choose(data)
            self.assertEqual(reason, "avoid_foe")
            policy.accepted(action, data["observation"], reason=reason)
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "east"}, "approach_foe"))

    def test_mage_spends_earned_rank_on_disclosed_spellcasting(self):
        from rogue_policy_reference import earned_training

        commands = []

        def advancement(command):
            commands.append(command)
            page = None
            if command == "overview":
                page = {"entries": [{"value": {"starting_skills": [{"id": "spellcasting"}]}}]}
            if command == "balances":
                page = {"entries": [{"value": {"owner": "2", "pool": {"id": "advancement"},
                                               "unspent": 1, "allowed": ["skill_rank"]}}],
                        "next": None}
            return {"status": "confirmed", "page": page}

        view = state([cell("here")])["observation"]
        self.assertEqual(earned_training(advancement, view)["status"], "confirmed")
        self.assertIn("purchase 3 advancement skill spellcasting", commands)

    def test_replans_after_rejected_spell_without_repeating_the_same_aim(self):
        foe = {"id": "foe", "position": {"x": 3, "y": 0, "z": 0}}
        spark = {"id": "spark_lance_cold", "cooldown_remaining": "0", "cost": None,
                 "reach": {"range": 10}}
        data = state([cell("here"), cell("path", 1), cell("contact", 2), cell("foe", 3)],
                     actors=[foe], hostiles=["foe"], abilities=[spark])
        policy = Policy()
        action, reason = policy.choose(data)
        self.assertEqual(reason, "ranged")
        self.assertTrue(policy.rejected(action, data["observation"], {"code": "invalid_action"}))
        self.assertEqual(policy.choose(data),
                         ({"type": "move", "direction": "east"}, "approach_foe"))
        foe["position"]["x"] = 2
        self.assertEqual(policy.choose(data)[1], "ranged")

    def test_uses_available_mage_spell_at_contact_before_falling_back_to_melee(self):
        foe = {"id": "foe", "position": {"x": 1, "y": 0, "z": 0}}
        spark = {"id": "spark_lance_cold", "cooldown_remaining": "0", "cost": None,
                 "reach": {"range": 10}}
        melee = {"id": "basic_melee", "cooldown_remaining": "0", "cost": None}
        data = state([cell("here"), cell("foe", 1)], actors=[foe],
                     hostiles=["foe"], abilities=[spark, melee])
        policy = Policy()
        first, reason = policy.choose(data)
        self.assertEqual(reason, "ranged")
        self.assertEqual(first["id"], "spark_lance_cold")
        spark["cooldown_remaining"] = "1"
        self.assertEqual(policy.choose(data)[0]["id"], "basic_melee")

    def test_prefers_a_clear_route_to_stairs_over_a_short_path_beside_a_hostile(self):
        def at(key, x, y=0, *, down=False):
            result = cell(key, x, down=down)
            result["position"]["y"] = y
            return result

        foe = {"id": "foe", "position": {"x": 1, "y": -1, "z": 0}}
        room = [at("here", 0), at("exposed", 1), at("stairs", 2, down=True),
                at("detour1", 0, 1), at("detour2", 1, 1), at("detour3", 2, 1)]
        policy = Policy()
        policy.floor = 17
        data = state(room, actors=[foe], hostiles=["foe"])
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "south"}, "find_descent"))

    def test_takes_a_safe_step_toward_descent_when_a_dangerous_foe_is_nearby(self):
        foe = {"id": "dragon", "name": "dragon", "position": {"x": 2, "y": 0, "z": 0}}
        stairs = cell("stairs", down=True)
        stairs["position"]["y"] = -1
        data = state([cell("here"), cell("west", -1), cell("east", 1), cell("foe", 2), stairs],
                     actors=[foe], hostiles=["dragon"])
        policy = Policy()
        policy.floor = 24
        self.assertEqual(policy.choose(data), ({"type": "move", "direction": "north"}, "find_descent"))

    def test_retreat_follows_safe_stair_route_without_increasing_distance_every_step(self):
        north = cell("corridor")
        north["position"]["y"] = -1
        stairs = cell("stairs", 1, up=True)
        stairs["position"]["y"] = -1
        room = [cell("here"), north, stairs, cell("dead_end", -1)]
        foes = [{"id": "wraith", "name": "Wraith", "position": {"x": 2, "y": 0, "z": 0}},
                {"id": "foe", "position": {"x": 0, "y": -3, "z": 0}}]
        policy = Policy()
        policy.floor = 17
        data = state(room, actors=foes, hostiles=["wraith", "foe"])
        self.assertEqual(policy.choose(data),
                         ({"type": "move", "direction": "north"}, "retreat_to_stairs"))

    def test_retreat_reuses_damage_seen_from_another_monster_of_the_same_kind(self):
        foe = {"id": "first", "name": "Troll", "position": {"x": 1, "y": 0, "z": 0}}
        melee = {"id": "basic_melee", "cooldown_remaining": "0", "cost": None}
        room = [cell("escape", -1), cell("here"), cell("foe", 1)]
        first = state(room, actors=[foe], hostiles=["first"], abilities=[melee])
        view = first["observation"]
        view["combat"].update(hp="40", max_hp="40")
        policy = Policy()
        policy.observe_combat(view)
        view["combat"].update(hp="26", events=[
            {"type": "ability", "caster": "first", "target": "player",
             "ability": {"id": "basic_melee"}, "outcome": "applied", "strike": 2},
        ])
        policy.observe_combat(view)
        next_foe = {**foe, "id": "second"}
        second = state(room, actors=[next_foe], hostiles=["second"], abilities=[melee])
        second["observation"]["combat"].update(hp="35", max_hp="40")
        self.assertTrue(policy.may_need_escape(second["observation"]))
        self.assertEqual(policy.choose(second), ({"type": "move", "direction": "west"}, "retreat"))

    def test_does_not_step_into_enemy_reach_at_critical_health(self):
        foe = {"id": "foe", "position": {"x": 2, "y": 0, "z": 0}}
        room = [cell("here"), cell("exposed", 1), cell("occupied", 2)]
        data = state(room, actors=[foe], hostiles=["foe"])
        data["observation"]["combat"].update(hp="5", max_hp="42")
        policy = Policy()
        policy.floor = 17
        self.assertEqual(policy.choose(data), ({"type": "wait"}, "hold_position"))

    def test_carrying_objective_does_not_chase_a_foe_while_seeking_the_exit(self):
        foe = {"id": "foe", "name": "orc", "position": {"x": 2, "y": 0, "z": 0}}
        objective = {"id": "objective", "identity": DEFAULT_OBJECTIVE_IDENTITY, "name": AMULET}
        room = [cell("here"), cell("west", -1), cell("east", 1), cell("foe", 2)]
        view = state(room, actors=[foe], hostiles=["foe"], inventory=[objective])
        policy = Policy()
        policy.floors[1] = Floor(visited={"here", "east", "foe"})
        self.assertEqual(policy.choose(view), ({"type": "move", "direction": "west"}, "explore"))

    def test_does_not_pursue_another_enemy_while_evading_a_dangerous_foe(self):
        wraith = {"id": "wraith", "name": "Wraith", "position": {"x": -2, "y": 0, "z": 0}}
        foe = {"id": "foe", "position": {"x": 2, "y": 0, "z": 0}}
        room = [cell("here"), cell("exposed", 1), cell("occupied", 2)]
        data = state(room, actors=[wraith, foe], hostiles=["wraith", "foe"])
        data["observation"]["combat"].update(hp="14", max_hp="39")
        policy = Policy()
        policy.floor = 13
        self.assertEqual(policy.choose(data), ({"type": "wait"}, "hold_position"))
