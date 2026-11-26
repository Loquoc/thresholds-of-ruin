"""Policy fixtures use only ordinary observation fields."""

import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

# The public policy and model can be tested from source without loading the native extension.
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "crates/client-bot/python"))
from rogue_combat import choose_melee_attack, has_push_space
from rogue_food import FoodScout, FoodScoutHistory
from rogue_player import Adventurer
from tor_client_bot import Direction, Move, Pickup, Turn, UseAbility, Wait

from rogue_o_matic import DEFAULT_OBJECTIVE_IDENTITY
from rogue_policy_fixture import AMULET, cell, state


def player_turn(cells, *, inventory=(), ground=(), actors=(), hostiles=(), abilities=(), tick=0,
                combat=None, stats=None, interactions=None, known_item_uses=()):
    picture = state(cells, inventory=inventory, ground=ground, actors=actors,
                    hostiles=hostiles, abilities=abilities)
    view = picture["observation"]
    view["interactions"] = interactions or {}
    view["known_item_uses"] = known_item_uses
    view["combat"].update(combat or {})
    view["combat"]["own_stats"].update(stats or {})
    view["tick"] = str(tick)
    view["visible_actors"] = [{"id": "player", "name": "you", "position": {"x": 0, "y": 0, "z": 0}},
                              *actors]
    return Turn({"state": picture, "readiness": {"admission": True}})


def tile(x, y, *, wall=False):
    return cell(f"{x},{y}", x, y, wall=wall)


def west_corridor():
    cells = [tile(0, 0), tile(-1, 0), tile(-2, 0), tile(-1, -1, wall=True), tile(-1, 1, wall=True)]
    approaches = [tile(2, 0), tile(2, 1)]
    foes = [{"id": str(i), "name": "troll", "position": site["position"]} for i, site in enumerate(approaches)]
    return cells + approaches, foes


def healing_potions():
    return [{"id": kind, "name": kind, "identity": f"item.potion.{kind}", "class": "potion",
             "identified": True, "quantity": 1} for kind in ("healing", "extra-healing")]


def scroll_trial_turn(cells, *scrolls, **observation):
    items = [{"id": target, "name": appearance, "class": "scroll", "identified": False, "quantity": 1}
             for target, appearance in scrolls]
    offers = [{"item": item["id"], "readable": True} for item in items]
    return player_turn(cells, inventory=items, interactions={"inventory": offers}, **observation)


def search_activity(phase="queued", *, intention="search"):
    return {"type": "admitted", "actor": 1, "branch": "game", "intention": intention, "phase": phase}


def search_turn(cells):
    ability = {"id": "rogue.search", "name": "Search", "cooldown_remaining": 0, "cost": None,
               "basic_attack": False, "target_kind": "self_only", "reach": {"kind": "self_only"}}
    return player_turn(cells, abilities=[ability])


def read_activity(phase="queued"):
    return {"type": "admitted", "actor": 1, "branch": "game", "intention": "read", "phase": phase}


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

    def test_melee_uses_rattling_blow_to_finish_a_target_even_when_a_push_is_available(self):
        cells = [cell("here"), cell("enemy", 1), cell("landing", 2)]
        foe = {"id": "orc", "name": "orc", "position": cells[1]["position"]}
        abilities = [{"id": name, "name": name, "basic_attack": False, "target_kind": "actor", "reach": {"kind": "melee"},
                      "cost": None, "cooldown_remaining": 0} for name in ("rattling_blow", "dislocating_strike")]
        turn = player_turn(cells, actors=[foe], abilities=abilities,
                           combat={"actors": [{"actor": "orc", "hostile": True, "injury": "near_death"}]})
        attack = choose_melee_attack(turn, turn.hostiles, make_space=True)
        self.assertEqual(attack.ability.identity, "rattling_blow")

    def test_dislocating_strike_uses_contact_from_the_players_outer_body_cell(self):
        tiles = [cell(str(x), x, down=x == 2) for x in range(-1, 4)]
        actors = [{"id": name, "name": name, "position": cell(name, x)["position"]}
                  for name, x in (("player", 1), ("orc", 2), ("emu", -1))]
        abilities = [{"id": name, "name": name, "basic_attack": name == "basic_melee", "cost": None,
                      "cooldown_remaining": 0, "target_kind": "actor", "reach": {"kind": "melee"}}
                     for name in ("basic_melee", "dislocating_strike")]
        for occupied in (False, True):
            with self.subTest(occupied=occupied):
                bystander = [{"id": "ally", "name": "ally", "position": tiles[-1]["position"]}] if occupied else []
                turn = player_turn(tiles, actors=[*actors, *bystander], hostiles=["orc", "emu"], abilities=abilities)
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual((reason, action.target.target), ("melee", "orc"))
                self.assertEqual(action.ability.identity, "basic_melee" if occupied else "dislocating_strike")

    def test_push_checks_the_translated_body_of_a_long_opponent(self):
        actors = [{"id": "orc", "name": "orc", "position": cell("orc", x)["position"]} for x in (1, 2)]
        for landing, clear in (([], False), ([cell("wall", 3, wall=True)], False), ([cell("open", 3)], True)):
            with self.subTest(landing=landing):
                turn = player_turn([cell("here"), cell("near", 1), cell("far", 2), *landing],
                                   actors=actors, hostiles=["orc"])
                self.assertEqual(has_push_space(turn, turn.hostiles[0]), clear)

    def test_push_requires_an_open_door_under_every_landing_cell(self):
        actors = [{"id": "orc", "name": "orc", "position": cell("orc", 1, y)["position"]} for y in (0, 1)]
        for opened in (False, True):
            with self.subTest(opened=opened):
                door = {**cell("gate", 2, 1), "door": {"id": "gate", "open": opened, "reachable": False}}
                turn = player_turn([cell("here"), cell("near", 1), cell("side", 1, 1), cell("open", 2), door],
                                   actors=actors, hostiles=["orc"])
                self.assertEqual(has_push_space(turn, turn.hostiles[0]), opened)

    def test_push_requires_one_horizontal_direction_from_all_face_contacts(self):
        def actor(name, x, y, z):
            return {"id": name, "name": name, "position": {"x": x, "y": y, "z": z}}

        tiles = [{**cell(f"{x},{y},{z}", x, y), "position": {"x": x, "y": y, "z": z}}
                 for x in range(3) for y in range(-1, 3) for z in range(3)]
        for body, target, expected in (
            ([(0, 0, 1)], [(1, 0, 0), (1, 0, 1)], True),
            ([(1, 1, 0)], [(1, 0, 0)], False),
            ([], [(0, 0, 1)], False),
            ([], [(1, 1, 0)], False),
        ):
            with self.subTest(body=body, target=target):
                actors = [actor("player", *offset) for offset in body] + [actor("orc", *offset) for offset in target]
                turn = player_turn(tiles, actors=actors, hostiles=["orc"])
                self.assertEqual(has_push_space(turn, turn.hostiles[0]), expected)

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
        trace = json.loads(json.dumps({"reason": reason}))
        self.assertEqual(trace, {"reason": "light_room"})
        player.admitted(dark, action, reason=trace["reason"])
        self.assertNotIsInstance(choose(player, dark)[0], Zap)
        choose(player, picture(True))
        self.assertEqual(choose(player, dark)[1], "light_room")

    def test_room_light_tries_another_wand_when_the_first_still_has_unknown_charges(self):
        from tor_client_bot import Zap

        wands = [{"id": target, "name": "staff of light", "identity": "item.wand.wand-light",
                  "identified": True, "quantity": 1, "class": "wand"} for target in ("first", "second")]
        offers = [{"item": wand["id"], "zappable": True, "device_activation": "self_target"} for wand in wands]
        def picture(lit):
            return player_turn([{**cell("here"), "lit": lit}, cell("east", 1)], inventory=wands,
                               interactions={"inventory": offers})

        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        dark = picture(False)
        for expected in dark.devices:
            action, reason = choose(player, dark)
            self.assertEqual((action, reason), (Zap(expected, None), "light_room"))
            player.admitted(dark, action, reason=reason)
            player.observe(picture(True))
        self.assertEqual(choose(player, dark)[1], "explore")
        offers[0]["charges"] = 1
        self.assertEqual(choose(player, picture(False))[1], "light_room")

    def test_potion_trials_remember_appearance_across_distinct_items(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        samples = (
            ("first", "first bottle", "amber", True),
            ("second", "other bottle", "amber", False),
            ("third", "other bottle", "cloudy", True),
            ("fourth", "plain flask", None, True),
            ("fifth", "plain flask", None, False),
        )
        for tick, (target, name, appearance, should_sample) in enumerate(samples):
            with self.subTest(target=target):
                item = {"id": target, "class": "potion", "name": name, "appearance": appearance,
                        "quantity": 1, "identified": False}
                turn = player_turn([cell("exit", down=True)], inventory=[item], tick=tick,
                                   interactions={"inventory": [{"item": target, "drinkable": True}]})
                action, reason = choose(player, turn)
                self.assertEqual(reason == "test_unknown_potion", should_sample)
                if should_sample:
                    player.admitted(turn, action, reason=reason)

    def test_scroll_study_requires_a_stair_available_for_escape_after_retreat(self):
        items = [{"id": kind, "class": kind, "name": kind, "quantity": 1, "identified": False}
                 for kind in ("scroll", "ring")]
        turn = player_turn([cell("landing", down=True)], inventory=items,
                           interactions={"inventory": [{"item": "scroll", "readable": True}]})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
        player.recovery_landing = "landing"
        self.assertIsNone(player.study_scroll_at_exit(turn))
        player.recovery_landing = None
        self.assertEqual(player.study_scroll_at_exit(turn)[1], "study_scroll_at_exit")

    def test_unknown_scroll_trial_uses_a_safe_outbound_stair_once_per_appearance(self):
        scroll = {"id": "scroll", "class": "scroll", "name": "runed scroll", "quantity": 2, "identified": False}
        ring = {"id": "ring", "class": "ring", "name": "opal ring", "quantity": 1, "identified": False}
        amulet = {"id": "amulet", "class": "amulet", "name": "Amulet", "quantity": 1, "identified": True,
                  "identity": DEFAULT_OBJECTIVE_IDENTITY}
        for depth, down, hp, pack, guarded, expected in (
            (3, True, 20, [scroll, ring], False, True),
            (3, True, 16, [scroll, ring], False, True),  # Exactly 80% health.
            (25, True, 20, [scroll, ring], False, True),
            (26, True, 20, [scroll, ring], False, False),
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

    def test_identification_scroll_uses_the_disclosed_categories_before_catalog_defaults(self):
        scroll = {"id": "lore", "class": "scroll", "identity": "item.scroll.identify-wand",
                  "name": "lore", "identified": True, "quantity": 1}
        for category, should_read in (("potion", True), ("wand", False)):
            with self.subTest(category=category):
                unknown = {"id": "unknown", "class": category, "name": "mystery", "identified": False, "quantity": 1}
                turn = player_turn([cell("stairs", down=True)], inventory=[scroll, unknown],
                                   interactions={"inventory": [{"item": "lore", "readable": True}]},
                                   known_item_uses=[{"item": "lore", "identifies": ["potion"]}])
                _, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual(reason == "identify_gear", should_read)

    def test_duplicate_identification_scrolls_share_the_current_turns_equipment_check(self):
        from rogue_gear import needs_identification

        scroll = {"id": "one", "class": "scroll", "identity": "item.scroll.identify-weapon",
                  "quantity": 1, "identified": True, "name": "weapon lore"}
        blade = {**scroll, "id": "blade", "class": "weapon", "identity": "fixture.blade"}
        growth = {**scroll, "id": "potion", "class": "potion", "identity": "item.potion.raise-level"}
        slot = {"id": "weapon", "name": "Weapon"}
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        for compatible, expected in ((False, "potion"), (True, "one")):
            with self.subTest(compatible=compatible):
                offers = [{"item": "one", "readable": True}, {"item": "two", "readable": True},
                          {"item": "potion", "drinkable": True}]
                if compatible:
                    offers.append({"item": "blade", "slot": slot})
                turn = player_turn([cell("exit", down=True)], inventory=[scroll, dict(scroll, id="two"), blade, growth],
                                   interactions={"slots": [slot], "inventory": offers})
                with patch("rogue_gear.needs_identification", wraps=needs_identification) as inspect:
                    action, _ = choose(player, turn)
                self.assertEqual(action.item.target, expected)
                inspect.assert_called_once_with(turn.inventory[2], turn)

    def test_unmeasured_plate_waits_only_for_a_readable_scroll_that_identifies_armor(self):
        plate = {"id": "plate", "class": "armor", "name": "plate", "quantity": 1, "identified": True,
                 "identity": "item.armor.plate-mail"}
        slot = {"id": "body_armor", "name": "Body"}
        for identity, categories, expected in (
            ("item.scroll.fixture", ["armor"], "identify_gear"),
            ("item.scroll.identify-armor", ["potion"], "try_armor"),
        ):
            with self.subTest(identity=identity, categories=categories):
                scroll = {"id": "lore", "class": "scroll", "name": "lore", "quantity": 1,
                          "identified": True, "identity": identity}
                turn = player_turn([cell("stairs", down=True)], inventory=[scroll, plate],
                                   interactions={"slots": [slot], "inventory": [
                                       {"item": "plate", "slot": slot}, {"item": "lore", "readable": True}]},
                                   known_item_uses=[{"item": "lore", "identifies": categories}])
                self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)[1], expected)

    def test_armor_selection_skips_scroll_checks_when_no_armor_can_be_worn(self):
        from rogue_gear import choose_armor

        slot = {"id": "body_armor", "name": "Armor"}
        mail = {"id": "plate", "class": "armor", "identity": "item.armor.plate-mail",
                "name": "plate", "identified": True, "quantity": 1}
        turn = player_turn([cell("here")], inventory=[mail], interactions={"slots": [slot], "inventory": [
            {"item": "plate", "slot": slot, "known_cursed": True}]})
        with patch("rogue_gear.identification_categories", side_effect=AssertionError("no armor to identify")):
            self.assertIsNone(choose_armor(turn))

    def test_blocked_armor_upgrade_uses_a_readable_known_cure(self):
        from rogue_gear import choose_armor
        from tor_client_bot import ItemUse, UseItem

        body = {"id": "body_armor", "name": "Body"}
        inventory = [{"id": key, "identity": identity, "name": key, "class": category, "identified": True, "quantity": 1}
                     for key, identity, category in (("mail", "item.armor.ring-mail", "armor"),
                                                     ("plate", "item.armor.plate-mail", "armor"),
                                                     ("cure", "item.scroll.remove-curse", "scroll"),
                                                     ("spare", "item.scroll.remove-curse", "scroll"))]
        for readable in (True, False):
            with self.subTest(readable=readable):
                turn = player_turn([cell("here")], inventory=inventory, interactions={
                    "slots": [body], "curse_blocked": ["mail"], "inventory": [
                        {"item": "mail", "slot": body, "equipped_slot": 0},
                        {"item": "plate", "slot": body}, {"item": "cure", "readable": readable},
                        {"item": "spare", "readable": False}]})
                expected = (UseItem(turn.inventory[2], ItemUse.READ), "remove_armor_curse") if readable else None
                self.assertEqual(choose_armor(turn), expected)

    def test_armor_trial_accounts_for_the_scroll_needed_to_unlock_the_current_mail(self):
        from rogue_gear import choose_armor

        body = {"id": "body_armor", "name": "Body"}
        mail = {"id": "mail", "class": "armor", "name": "mail", "identity": "item.armor.ring-mail",
                "identified": True, "quantity": 1}
        chain = {**mail, "id": "chain", "identity": "item.armor.chain-mail"}
        cure = {"id": "cure", "class": "scroll", "name": "cure", "identity": "item.scroll.remove-curse",
                "identified": True, "quantity": 1}
        for count, spare_readable, expected in ((1, False, None), (2, False, "remove_armor_curse"),
                                                 (1, True, "remove_armor_curse")):
            with self.subTest(count=count, spare_readable=spare_readable):
                turn = player_turn([cell("here")], inventory=[mail, chain, dict(cure, quantity=count),
                                                              dict(cure, id="spare")],
                                   interactions={"slots": [body], "curse_blocked": ["mail"], "inventory": [
                                       {"item": "mail", "slot": body, "equipped_slot": 0},
                                       {"item": "chain", "slot": body}, {"item": "cure", "readable": True},
                                       {"item": "spare", "readable": spare_readable}]})
                decision = choose_armor(turn)
                self.assertEqual(decision and decision[1], expected)

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

    def test_armor_protection_uses_the_first_readable_copy_in_inventory_order(self):
        from tor_client_bot import ItemUse, UseItem

        body = {"id": "body_armor", "name": "body"}
        scrolls = ["blocked", "first", "spare"]
        inventory = [{"id": "mail", "identity": "item.armor.ring-mail", "name": "mail", "class": "armor",
                      "identified": True, "quantity": 1}]
        inventory.extend({"id": name, "identity": "item.scroll.protect-armor", "name": name, "class": "scroll",
                          "identified": True, "quantity": 1} for name in scrolls)
        offers = [{"item": "mail", "slot": body, "equipped_slot": 0, "known_protected": False}]
        offers.extend({"item": name, "readable": name != "blocked"} for name in scrolls)
        turn = player_turn([cell("here")], inventory=inventory, interactions={"slots": [body], "inventory": offers})
        decision = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
        self.assertEqual(decision, (UseItem(turn.inventory[2], ItemUse.READ), "protect_armor"))

    def test_trial_armor_needs_a_large_known_improvement_or_a_way_to_remove_a_curse(self):
        from tor_client_bot import Unequip
        slot = {"id": "body_armor", "name": "Armor"}
        cases = [
            ("plate-mail", False, None, True),
            ("splint-mail", False, None, False),  # Worst-case protection ties the worn ring mail.
            ("scale-mail", False, None, False),
            ("scale-mail", False, False, True),
            ("scale-mail", True, None, True),
            ("unrecognized", True, None, False),
        ]
        for candidate, cure, known_cursed, trial in cases:
            with self.subTest(candidate=candidate, cure=cure, known_cursed=known_cursed):
                suits = [{"id": name, "name": name, "class": "armor", "quantity": 1,
                          "identified": True, "identity": "item.armor." + name} for name in ("ring-mail", candidate)]
                offers = [{"item": item["id"], "slot": slot, "equipped_slot": 0 if i == 0 else None}
                          for i, item in enumerate(suits)]
                if known_cursed is not None:
                    offers[1]["known_cursed"] = known_cursed
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

    def test_ring_upgrade_counts_the_effect_retained_on_a_blocked_finger(self):
        from rogue_gear import choose_ring
        from tor_client_bot import Unequip

        slot = {"id": "ring", "name": "Finger"}
        rings = [{"id": target, "name": name, "class": "ring", "quantity": 1, "identified": True,
                  "identity": "item.ring.ring-" + name}
                 for target, name in (("left", "protection"), ("right", "protection"), ("spare", "searching"))]
        offers = [{"item": "left", "slot": slot, "equipped_slot": 0},
                  {"item": "right", "slot": slot, "equipped_slot": 1}, {"item": "spare", "slot": slot}]
        for blocked, replace in ((["left"], 1), (["right"], 0), (["left", "right"], None)):
            with self.subTest(blocked=blocked):
                turn = player_turn([cell("here")], inventory=rings, interactions={
                    "slots": [slot, slot], "inventory": offers, "curse_blocked": blocked})
                expected = None if replace is None else (Unequip(turn.equipment[replace]), "change_ring")
                self.assertEqual(choose_ring(turn), expected)

    def test_ring_upgrade_spends_a_cure_only_when_both_fingers_are_blocked(self):
        from rogue_gear import choose_ring

        slot = {"id": "ring", "name": "Finger"}
        rings = [{"id": target, "identity": "item.ring.ring-" + name, "name": name,
                  "class": "ring", "quantity": 1, "identified": True}
                 for target, name in (("left", "adornment"), ("right", "adornment"), ("new", "slow-digestion"))]
        cure = {**rings[0], "id": "cure", "identity": "item.scroll.remove-curse", "class": "scroll"}
        offers = [{"item": "left", "slot": slot, "equipped_slot": 0}, {"item": "right", "slot": slot, "equipped_slot": 1},
                  {"item": "new", "slot": slot}, {"item": "cure", "readable": True}]
        for blocked, reason in ((["left", "right"], "remove_ring_curse"), (["left"], "change_ring")):
            with self.subTest(blocked=blocked):
                turn = player_turn([cell("here")], inventory=[*rings, cure], interactions={
                    "slots": [slot, slot], "inventory": offers, "curse_blocked": blocked})
                decision = choose_ring(turn)
                self.assertIsNotNone(decision)
                self.assertEqual(decision[1], reason)

    def test_equal_ring_upgrades_prefer_the_first_free_finger(self):
        slot = {"id": "ring", "name": "Finger"}
        rings = [{"id": target, "name": name, "class": "ring", "quantity": 1, "identified": True,
                  "identity": "item.ring.ring-" + name}
                 for target, name in (("worn", "fixture"), ("first", "searching"), ("second", "searching"))]
        offers = [{"item": ring["id"], "slot": slot} for ring in rings]
        offers[0]["equipped_slot"] = 0
        turn = player_turn([cell("stairs", down=True)], inventory=rings,
                           interactions={"slots": [slot, slot, slot], "inventory": offers})
        action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
        self.assertEqual((action.item.target, action.slot.index, reason), ("first", 1, "upgrade_equipment"))

    def test_regeneration_outranks_slow_digestion_only_when_injured_with_edible_food(self):
        slot = {"id": "ring", "name": "Finger"}
        rings = [{"id": name, "name": name, "class": "ring", "identified": True,
                  "identity": "item.ring.ring-" + name, "quantity": 1} for name in ("regeneration", "slow-digestion")]
        ration = {"id": "ration", "name": "ration", "class": "food", "identified": True,
                  "identity": "item.food.ration", "quantity": 1}
        for hp, edible, expected in ((10, True, "slow-digestion"), (9, False, "slow-digestion"),
                                     (9, True, "regeneration")):
            with self.subTest(hp=hp, edible=edible):
                offers = [{"item": ring["id"], "slot": slot} for ring in rings]
                offers.append({"item": "ration", "edible": edible})
                turn = player_turn([cell("stairs", down=True)], inventory=[*rings, ration],
                                   combat={"hp": hp, "max_hp": 10}, interactions={"slots": [slot], "inventory": offers})
                action, _ = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual(action.item.target, expected)

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

        worn = {**ring, "id": "other", "identity": "item.ring.ring-protection"}
        offers = [{"item": "other", "slot": slot, "equipped_slot": 0},
                  {"item": "band", "slot": slot}, {"item": "dose", "drinkable": True}]
        turn = player_turn([cell("here")], inventory=[worn, ring, dose],
                           interactions={"slots": [slot], "inventory": offers})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.last_unseen_attack = "here", 0
        action, reason = choose(player, turn)
        self.assertEqual((describe_action(action)["type"], reason), ("drink", "reveal_attacker"))

    def test_quiet_stair_turn_skips_visible_opponent_tactics(self):
        turn = player_turn([cell("stairs", down=True)])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        with patch.object(player, "respond_to_pursuit", side_effect=AssertionError("no pursuers")), \
                patch.object(player, "melee_attack", side_effect=AssertionError("no melee targets")):
            self.assertEqual(choose(player, turn), (Move(Direction.DOWN), "descend"))

    def test_unseen_attack_delays_descent_only_during_its_evasion_window(self):
        for tick, expected in ((99, "descend"), (100, "evade_unseen_attack"),
                               (499, "evade_unseen_attack"), (500, "descend")):
            with self.subTest(tick=tick):
                turn = player_turn([cell("stairs", down=True), cell("west", -1)], tick=tick)
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.last_unseen_attack = "stairs", 100
                self.assertEqual(choose(player, turn)[1], expected)

    def test_unseen_hit_that_drains_strength_prompts_evasion_at_unchanged_health(self):
        tiles = [cell("stairs", down=True), cell("west", -1)]
        def picture(tick, strength, events):
            attributes = dict.fromkeys(("strength", "speed", "intellect", "willpower", "awareness", "presence"), 0)
            attributes["strength"] = strength
            return player_turn(tiles, tick=tick, combat={"hp": 10, "events": events}, stats={
                "restorable_losses": {"attributes": attributes, "hit_dice": 0, "maximum_health": 0}})

        hit = {"type": "unseen_ability", "caster": "hunter", "target": "player", "outcome": "applied"}
        for strength, expected in ((0, "descend"), (1, "evade_unseen_attack")):
            with self.subTest(strength=strength):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observe(picture(0, 0, []))
                self.assertEqual(choose(player, picture(100, strength, [hit]))[1], expected)
                for target in ("hunter", "other"):
                    foe = {"id": target, "name": "snake", "position": tiles[1]["position"]}
                    sighting = player_turn(tiles, actors=[foe], hostiles=[target])
                    self.assertEqual(player.opponents.nearby_drainer(sighting, sighting.hostiles[0]),
                                     strength > 0 and target == "hunter")

    def test_unseen_miss_marks_the_tile_dangerous_without_health_loss(self):
        from rogue_player import RECENT_DANGER_TICKS

        cells = [cell("here"), cell("west", -1)]
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(player_turn(cells, combat={"hp": 10}))
        miss = {"type": "unseen_ability", "target": "player", "outcome": "miss"}
        player.observe(player_turn(cells, tick=100, combat={"hp": 10, "events": [miss]}))
        self.assertEqual(player.last_unseen_attack, ("here", 100))
        self.assertEqual(player.danger_until.get("here"), 100 + RECENT_DANGER_TICKS)
        player.observe(player_turn(cells, tick=100 + RECENT_DANGER_TICKS, combat={"hp": 10}))
        self.assertNotIn("here", player.danger_until)

    def test_reflected_damage_does_not_identify_an_unseen_effect_as_an_attack(self):
        cells = [cell("here"), cell("west", -1)]
        reflected = {"type": "ability", "caster": "player", "target": "player", "outcome": "applied",
                     "ability": {"id": "rogue.wand_fire"}}
        unseen = {"type": "unseen_ability", "target": "player", "outcome": "applied"}
        for events, expected in (([unseen], ("here", 100)), ([reflected, unseen], None),
                                 ([unseen, reflected], None)):
            with self.subTest(events=events):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observe(player_turn(cells, combat={"hp": 10}))
                player.observe(player_turn(cells, tick=100, combat={"hp": 8, "events": events}))
                self.assertEqual(player.last_unseen_attack, expected)
                self.assertEqual("here" in player.danger_until, expected is not None)

    def test_unseen_effect_needs_independent_evidence_when_a_visible_hit_explains_the_injury(self):
        tiles = [cell("stairs", down=True), cell("west", -1), cell("east", 1)]
        foe = {"id": "orc", "name": "orc", "position": tiles[2]["position"]}
        visible = {"type": "ability", "caster": "orc", "target": "player", "outcome": "applied"}
        unseen = {"type": "unseen_ability", "target": "player"}
        for outcome, expected in (("applied", "descend"), ("miss", "evade_unseen_attack")):
            effect = {**unseen, "outcome": outcome}
            for events in ([visible, effect], [effect, visible]):
                with self.subTest(outcome=outcome, first=events[0]["type"]):
                    player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                    player.observe(player_turn(tiles, combat={"hp": 10}))
                    player.observe(player_turn(tiles, actors=[foe], hostiles=["orc"], tick=100,
                                               combat={"hp": 9, "events": events}))
                    self.assertEqual(choose(player, player_turn(tiles, tick=101, combat={"hp": 9}))[1], expected)

    def test_unseen_cast_by_a_disclosed_hostile_keeps_the_bot_moving_after_contact_is_lost(self):
        tiles = [cell("stairs", down=True), cell("west", -1), cell("east", 1)]
        caster = {"id": "caster", "name": "phantom", "position": tiles[2]["position"]}
        event = {"type": "unseen_ability", "caster": "caster", "target": "player", "outcome": "unaffected"}
        for hostiles, expected in ((["caster"], "evade_unseen_attack"), ([], "descend")):
            with self.subTest(hostiles=hostiles):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observe(player_turn(tiles, actors=[caster], hostiles=hostiles,
                                           tick=100, combat={"events": [event]}))
                self.assertEqual(choose(player, player_turn(tiles, tick=101))[1], expected)

    def test_unseen_attack_prefers_a_clear_step_and_can_escape_through_the_only_trap(self):
        pit = {"id": "pit", "family": None, "reachable": True}
        east = {**cell("east", 1), "trap": pit}
        for west, expected in (([cell("west", -1)], Direction.WEST), ([], Direction.EAST)):
            with self.subTest(west_open=bool(west)):
                turn = player_turn([cell("here"), east, *west], tick=100)
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.last_unseen_attack = "here", 100
                self.assertEqual(choose(player, turn), (Move(expected), "evade_unseen_attack"))

    def test_unseen_attack_prefers_a_corridor_over_an_equally_distant_blind_alley(self):
        miss = {"type": "unseen_ability", "target": "player", "outcome": "miss"}
        for alley_x in (-1, 1):
            for corridor_open in (True, False):
                with self.subTest(alley_x=alley_x, corridor_open=corridor_open):
                    tiles = [cell("here"), cell("alley", alley_x), cell("back-wall", 2 * alley_x, wall=True),
                             cell("north-wall", alley_x, -1, wall=True), cell("south-wall", alley_x, 1, wall=True)]
                    if corridor_open:
                        tiles.extend([cell("corridor", -alley_x), cell("onward", -2 * alley_x)])
                    turn = player_turn(tiles, tick=100, combat={"events": [miss]})
                    destination_x = -alley_x if corridor_open else alley_x
                    direction = Direction.EAST if destination_x == 1 else Direction.WEST
                    self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn),
                                     (Move(direction), "evade_unseen_attack"))

    def test_unseen_attack_evasion_keeps_distance_from_the_last_attack_site(self):
        for x, expected in ((2, Direction.WEST), (-2, Direction.EAST)):
            with self.subTest(attack_x=x):
                turn = player_turn([cell("here"), cell("west", -1), cell("east", 1), cell("attack", x)], tick=200)
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.last_unseen_attack = "attack", 100
                self.assertEqual(choose(player, turn), (Move(expected), "evade_unseen_attack"))

    def test_unseen_attack_evasion_uses_the_nearest_view_of_the_attack_site(self):
        views = [cell("attack", 4), cell("attack", -2)]
        for order in (views, views[::-1]):
            with self.subTest(first=order[0]["position"]):
                turn = player_turn([cell("here"), cell("west", -1), cell("east", 1), *order], tick=200)
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.last_unseen_attack = "attack", 100
                self.assertEqual(choose(player, turn), (Move(Direction.EAST), "evade_unseen_attack"))

    def test_unseen_attack_evasion_uses_the_mapped_corridor_after_losing_sight_of_the_site(self):
        miss = {"type": "unseen_ability", "target": "player", "outcome": "miss"}
        for away, expected in ((1, Direction.EAST), (-1, Direction.WEST)):
            with self.subTest(away=away):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                corridor = [cell(str(i), away * i) for i in range(4)]
                player.observe(player_turn(corridor, tick=100, combat={"events": [miss]}))
                around_player = [cell(str(i), away * (i - 2)) for i in (1, 2, 3)]
                decision = choose(player, player_turn(around_player, tick=200))
                self.assertEqual(decision, (Move(expected), "evade_unseen_attack"))

    def test_unseen_evasion_only_maps_distances_to_available_steps(self):
        from rogue_retreat import evade_unseen_attacker

        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(player_turn([cell(str(x), x) for x in range(6)]))
        memory = player.maps[1]
        reachable = memory.reachable
        for offsets, expected in (((-1, 0, 1), Move(Direction.EAST)), ((0,), None)):
            with self.subTest(offsets=offsets):
                turn = player_turn([cell(str(x + 2), x) for x in offsets])
                visited = []
                def trace_distances(origin):
                    for site, distance in reachable(origin):
                        visited.append(site.key)
                        yield site, distance
                with patch.object(memory, "reachable", side_effect=trace_distances):
                    self.assertEqual(evade_unseen_attacker(turn, "0", memory), expected)
                self.assertEqual(visited, ["0", "1", "2", "3"] if expected else [])

    def test_primary_weapon_slot_is_selected_by_kind_and_keeps_its_position_when_empty(self):
        from rogue_gear import primary_weapon_slot

        armor = {"id": "body_armor", "name": "Armor"}
        weapon = {"id": "weapon", "name": "Weapon"}
        for slots, expected in (([armor], None), ([armor, weapon, weapon], 1)):
            with self.subTest(slots=slots):
                turn = player_turn([cell("here")], interactions={"slots": slots})
                slot = primary_weapon_slot(turn)
                self.assertEqual(slot.index if slot else None, expected)

    def test_recognized_melee_weapon_can_fill_an_empty_slot_before_its_stats_are_known(self):
        slot = {"id": "weapon", "name": "Weapon"}
        items = [{"id": name, "name": name, "identity": f"item.weapon.{name}", "identified": True,
                  "quantity": 1, "class": "weapon"} for name in ("dart", "spear", "dagger")]
        for equipped, expected in ((None, "equip_melee"), (2, "descend")):
            with self.subTest(equipped=equipped):
                offers = [{"item": item["id"], "slot": slot, "equipped_slot": 0 if i == equipped else None}
                          for i, item in enumerate(items)]
                turn = player_turn([cell("stairs", down=True)], inventory=items,
                                   interactions={"slots": [slot], "inventory": offers})
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual(reason, expected)
                if equipped is None:
                    self.assertEqual(action.item.target, "spear")

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

        # Equal attack ratings preserve the worn weapon, then the first offered upgrade.
        offers[2] = weapon("dagger", 6, 50)
        self.assertEqual(choose(player, picture()), (Move(Direction.DOWN), "descend"))
        del offers[1]["equipped_slot"]
        turn = picture()
        self.assertEqual(choose(player, turn), (Equip(turn.inventory[1], turn.equipment[0]), "equip_melee"))

        # At equal damage and speed, a more accurate dagger is worth switching to.
        offers[1]["equipped_slot"] = 0
        offers[2]["known_equipment"]["attack"]["bonus"] += 1
        turn = picture()
        self.assertEqual(choose(player, turn), (Unequip(turn.equipment[0]), "change_weapon"))
        del offers[1]["equipped_slot"]
        turn = picture()
        self.assertEqual(choose(player, turn), (Equip(turn.inventory[2], turn.equipment[0]), "equip_melee"))

        # Enchant the chosen upgrade after the swap has completed.
        inventory.append({"id": "scroll", "identity": "item.scroll.enchant-weapon", "identified": True,
                          "name": "enchant weapon", "class": "scroll", "quantity": 1})
        offers.append({"item": "scroll", "readable": True})
        offers[1]["equipped_slot"] = 0
        turn = picture()
        self.assertEqual(choose(player, turn), (Unequip(turn.equipment[0]), "change_weapon"))
        del offers[1]["equipped_slot"]
        turn = picture()
        self.assertEqual(choose(player, turn), (Equip(turn.inventory[2], turn.equipment[0]), "equip_melee"))
        offers[2]["equipped_slot"] = 0
        self.assertEqual(choose(player, picture())[1], "enchant_weapon")

        # A disclosed melee attack is sufficient even for a scenario-specific weapon.
        inventory[2]["identity"] = "item.weapon.fixture-blade"
        self.assertEqual(choose(player, picture())[1], "enchant_weapon")

        # A stronger spare that is known to be cursed leaves the worn sword eligible for enchantment.
        del offers[2]["equipped_slot"]
        offers[2]["known_cursed"] = True
        offers[1]["equipped_slot"] = 0
        self.assertEqual(choose(player, picture())[1], "enchant_weapon")

    def test_returning_player_ascends_past_a_duplicate_objective_amulet(self):
        relic = {"id": "carried", "name": "Copper seal", "class": "amulet", "identity": "fixture.seal",
                 "identified": True, "quantity": 1}
        stairs, hall = cell("stairs", up=True), cell("hall", 1)
        for site, reachable in ((stairs, True), (hall, False)):
            with self.subTest(reachable=reachable):
                turn = player_turn([stairs, hall], inventory=[relic], ground=[
                    {**site, "item": {**relic, "id": "spare"}, "reachable": reachable}])
                player = Adventurer(26, relic["identity"], depth=2)
                self.assertEqual(choose(player, turn), (Move(Direction.UP), "ascend"))

    def test_reachable_objective_takes_priority_before_planning_loot_detours(self):
        from unittest.mock import patch

        here = cell("here")
        items = [{"id": "coin", "name": "gold", "class": "coin", "identified": False, "quantity": 1},
                 {"id": "prize", "name": AMULET, "class": "amulet", "identity": DEFAULT_OBJECTIVE_IDENTITY,
                  "identified": True, "quantity": 1}]
        for order in (items, items[::-1]):
            turn = player_turn([here], ground=[{**here, "item": item, "reachable": True} for item in order])
            player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
            with patch.object(player, "loot_goals", side_effect=AssertionError("unexpected detour planning")):
                action, reason = choose(player, turn)
            self.assertEqual((action.item.target, reason), ("prize", "take"))

    def test_pickup_prioritizes_a_needed_healing_reserve_over_incidental_loot(self):
        here = cell("here")
        coin = {"id": "gold", "identity": "item.coin.gold", "class": "coin", "name": "gold", "quantity": 1, "identified": True}
        potion = {"id": "dose", "identity": "item.potion.healing", "class": "potion",
                  "name": "healing", "quantity": 1, "identified": True}
        ground = [{**here, "item": item, "reachable": True} for item in (coin, potion)]
        for stock, expected in ((1, "dose"), (2, "gold")):
            with self.subTest(stock=stock):
                turn = player_turn([here], ground=ground, inventory=[{**potion, "id": "pack", "quantity": stock}])
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual((action.item.target, reason), (expected, "take"))

    def test_first_melee_weapon_takes_priority_over_incidental_pickups(self):
        here = cell("here")
        spear = {"id": "spear", "identity": "item.weapon.spear", "class": "weapon", "name": "spear",
                 "quantity": 1, "identified": True}
        coin = {**spear, "id": "gold", "identity": "item.coin.gold", "class": "coin"}
        dose = {**spear, "id": "heal", "identity": "item.potion.healing", "class": "potion"}
        for pack, pile, expected in (([], [coin, spear], "spear"), ([{**spear, "id": "packed"}], [coin, spear], "gold"),
                                     ([], [coin, spear, dose], "heal")):
            with self.subTest(expected=expected):
                ground = [{**here, "item": item, "reachable": True} for item in pile]
                turn = player_turn([here], inventory=pack, ground=ground)
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual((action.item.target, reason), (expected, "take"))

    def test_first_melee_weapon_keeps_ground_order_when_only_identities_are_known(self):
        here = cell("here")
        for names in (("dagger", "spear"), ("spear", "dagger")):
            with self.subTest(names=names):
                ground = [{**here, "reachable": True, "item": {
                    "id": name, "identity": f"item.weapon.{name}", "class": "weapon",
                    "name": name, "quantity": 1, "identified": True}} for name in names]
                turn = player_turn([here], ground=ground)
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual((action.item.target, reason), (names[0], "take"))

    def test_pickup_fills_the_healing_reserve_in_one_action_when_possible(self):
        here = cell("here")
        dose = {"id": "single", "identity": "item.potion.healing", "class": "potion",
                "name": "healing", "quantity": 1, "identified": True}
        pair = {**dose, "id": "pair", "quantity": 2}
        for order in ((dose, pair), (pair, dose)):
            for stock, expected in ((0, "pair"), (1, order[0]["id"])):
                with self.subTest(first=order[0]["id"], stock=stock):
                    ground = [{**here, "item": item, "reachable": True} for item in order]
                    inventory = [{**dose, "id": "carried"}] if stock else []
                    turn = player_turn([here], ground=ground, inventory=inventory)
                    action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                    self.assertEqual((action.item.target, reason), (expected, "take"))

    def test_pickup_only_checks_foe_distance_for_incidental_loot(self):
        here, far = cell("here"), cell("far", 3)
        foe = {"id": "foe", "name": "orc", "position": far["position"]}
        prize = {"id": "prize", "identity": DEFAULT_OBJECTIVE_IDENTITY, "class": "amulet",
                 "name": "amulet", "quantity": 1, "identified": True}
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        for reachable in (False, True):
            with self.subTest(reachable=reachable):
                turn = player_turn([here, far], actors=[foe], hostiles=["foe"],
                                   ground=[{**here, "item": prize, "reachable": reachable}])
                with patch.object(turn, "distance_to", side_effect=AssertionError("unexpected threat check")):
                    chosen = player.choose_pickup(turn, turn.ground_items, returning=False)
                self.assertEqual(chosen.target if chosen else None, "prize" if reachable else None)

    def test_distant_guard_allows_healing_pickup_at_the_recovery_threshold(self):
        here, far = cell("here"), cell("far", 3)
        foe = {"id": "orc", "name": "orc", "position": far["position"]}
        dose = {"id": "dose", "identity": "item.potion.healing", "class": "potion",
                "name": "healing", "quantity": 1, "identified": True}
        gold = {"id": "gold", "identity": "item.coin.gold", "class": "coin",
                "name": "gold", "quantity": 1, "identified": True}
        ground = [{**here, "item": item, "reachable": True} for item in (gold, dose)]
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        for hp, expected in ((7, None), (8, "dose")):
            with self.subTest(hp=hp):
                turn = player_turn([here, far], actors=[foe], hostiles=["orc"], ground=ground,
                                   combat={"hp": hp, "max_hp": 10})
                picked = player.choose_pickup(turn, turn.ground_items, returning=False)
                self.assertEqual(picked.target if picked else None, expected)

    def test_unsafe_pickup_allows_only_a_reachable_outbound_objective(self):
        here = cell("here")
        foe = {"id": "foe", "name": "orc", "position": cell("east", 1)["position"]}
        potion = {"id": "dose", "identity": "item.potion.healing", "class": "potion",
                  "name": "healing", "quantity": 1, "identified": True}
        relic = {**potion, "id": "prize", "identity": DEFAULT_OBJECTIVE_IDENTITY, "class": "amulet"}
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        for reachable, returning, expected in ((True, False, "prize"), (False, False, None), (True, True, None)):
            with self.subTest(reachable=reachable, returning=returning):
                ground = [{**here, "item": potion, "reachable": True}, {**here, "item": relic, "reachable": reachable}]
                turn = player_turn([here, cell("east", 1)], ground=ground, actors=[foe], hostiles=["foe"])
                chosen = player.choose_pickup(turn, turn.ground_items, returning=returning)
                self.assertEqual(chosen.target if chosen else None, expected)

    def test_comparing_potion_pickups_does_not_inspect_the_weapon_slot(self):
        here = cell("here")
        potion = {"id": "water", "name": "water", "class": "potion", "quantity": 1,
                  "identified": True, "identity": "item.potion.water"}
        healing = {**potion, "id": "dose", "identity": "item.potion.healing"}
        ground = [{**here, "item": item, "reachable": True} for item in (potion, healing)]
        turn = player_turn([here], ground=ground)
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        with patch("rogue_loot.primary_weapon_slot", side_effect=AssertionError("no weapon offered")):
            picked = player.choose_pickup(turn, turn.ground_items, returning=False)
        self.assertEqual(picked.target, "dose")

    def test_known_curse_changes_first_weapon_priority_only_when_the_slot_is_blocked(self):
        mace = {"id": "mace", "identity": "item.weapon.mace", "class": "weapon",
                "name": "mace", "quantity": 1, "identified": True}
        dagger = {**mace, "id": "blade", "identity": "item.weapon.dagger"}
        slot = {"id": "weapon", "name": "Weapon"}
        tiles = [cell("exit", down=True), cell("hall", 1), cell("hall2", 2), cell("stash", 3)]
        for worn, expected in ((True, "descend"), (False, "seek_supplies")):
            with self.subTest(worn=worn):
                offer = {"item": "mace", "slot": slot, "known_cursed": True, "equipped_slot": 0 if worn else None}
                turn = player_turn(tiles, inventory=[mace], interactions={"slots": [slot], "inventory": [offer]},
                                   ground=[{**tiles[-1], "item": dagger, "reachable": False}])
                self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)[1], expected)

    def test_locked_shortbow_allows_a_melee_weapon_detour_when_the_curse_can_be_removed(self):
        slot = {"id": "weapon", "name": "Weapon"}
        tiles = [cell("exit", down=True), cell("hall", 1), cell("hall2", 2), cell("stash", 3)]
        bow = {"id": "bow", "identity": "item.weapon.shortbow", "class": "weapon",
               "identified": True, "name": "shortbow", "quantity": 1}
        blade = {**bow, "id": "blade", "identity": "item.weapon.dagger", "name": "dagger"}
        cure = {**bow, "id": "cure", "identity": "item.scroll.remove-curse", "class": "scroll"}
        for readable, expected in ((True, "seek_supplies"), (False, "descend")):
            with self.subTest(readable=readable):
                turn = player_turn(tiles, inventory=[bow, cure],
                                   ground=[{**tiles[-1], "item": blade, "reachable": False}],
                                   interactions={"slots": [slot], "inventory": [
                                       {"item": "bow", "slot": slot, "known_cursed": True, "equipped_slot": 0},
                                       {"item": "cure", "readable": readable}]})
                self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)[1], expected)

    def test_first_melee_weapon_is_worth_a_short_outbound_detour(self):
        weapon = {"id": "blade", "identity": "item.weapon.dagger", "class": "weapon", "name": "dagger",
                  "quantity": 1, "identified": True}
        for distance, pack, expected in ((3, [], "seek_supplies"), (4, [], "descend"),
                                         (3, [{**weapon, "id": "carried"}], "descend")):
            with self.subTest(distance=distance, armed=bool(pack)):
                tiles = [cell("exit", down=True), *(cell(str(x), x) for x in range(1, distance + 1))]
                turn = player_turn(tiles, inventory=pack, ground=[{**tiles[-1], "item": weapon, "reachable": False}])
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual(reason, expected)
                self.assertEqual(action.direction, Direction.EAST if expected == "seek_supplies" else Direction.DOWN)

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

    def test_ring_detour_with_a_blocked_finger_requires_a_readable_cure(self):
        here, near = cell("exit", down=True), cell("prize", 1)
        slot = {"id": "ring", "name": "Finger"}
        worn = {"id": "worn", "identity": "item.ring.ring-adornment", "name": "band",
                "quantity": 1, "class": "ring", "identified": True}
        cure = {**worn, "id": "scroll", "identity": "item.scroll.remove-curse", "class": "scroll"}
        ring = {**worn, "id": "ring", "identity": "item.ring.ring-slow-digestion"}
        for readable, expected in ((True, "seek_supplies"), (False, "descend")):
            with self.subTest(readable=readable):
                turn = player_turn([here, near], inventory=[worn, cure], ground=[{**near, "item": ring, "reachable": False}],
                                   interactions={"slots": [slot], "curse_blocked": ["worn"], "inventory": [
                                       {"item": "worn", "slot": slot, "equipped_slot": 0},
                                       {"item": "scroll", "readable": readable}]})
                self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)[1], expected)

    def test_cursed_spare_ring_does_not_rule_out_another_copy_of_an_upgrade(self):
        here, near = cell("stairs", down=True), cell("ring", 1)
        slot = {"id": "ring", "name": "Finger"}
        band = {"id": "spare", "name": "searching", "identity": "item.ring.ring-searching",
                "quantity": 1, "class": "ring", "identified": True}
        ground = [{**near, "item": {**band, "id": "found"}, "reachable": False}]
        turn = player_turn([here, near], inventory=[band], ground=ground, interactions={
            "slots": [slot], "inventory": [{"item": "spare", "slot": slot, "known_cursed": True}]})
        self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn),
                         (Move(Direction.EAST), "seek_supplies"))

    def test_optional_loot_waits_until_danger_on_the_route_expires(self):
        for category, distance in (("coin", 2), ("potion", 3)):
            with self.subTest(category=category):
                stairs = cell("exit", -1, down=True)
                cells = [stairs, cell("here"), *(cell(str(x), x) for x in range(1, distance + 1))]
                item = {"id": "loot", "name": category, "class": category, "quantity": 1, "identified": False}
                ground = [{**cells[-1], "item": item, "reachable": False}]
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.danger_until["1"] = 100
                for tick, direction, reason in ((99, Direction.WEST, "seek_stairs"),
                                                (100, Direction.EAST, "seek_supplies")):
                    turn = player_turn(cells, ground=ground, tick=tick)
                    self.assertEqual(choose(player, turn), (Move(direction), reason))

    def test_visible_food_detour_crosses_recent_danger_only_when_required(self):
        stairs, near, hall, far = cell("stairs", down=True), cell("near", 1), cell("hall", -1), cell("far", -2)
        food = {"id": "ration", "name": "ration", "class": "food", "identified": False, "quantity": 1}
        for sites, expected in (((near, far), Direction.WEST), ((near,), Direction.EAST)):
            with self.subTest(destinations=[site["key"] for site in sites]):
                turn = player_turn([stairs, near, hall, far], ground=[
                    {**site, "item": {**food, "id": site["key"]}, "reachable": False} for site in sites])
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observe(turn)
                player.danger_until["near"] = 900
                self.assertEqual(player.choose(turn), (Move(expected), "seek_item"))

    def test_guard_warnings_cover_useful_piles_beside_any_part_of_a_foe(self):
        from rogue_loot import guarded_supply_cells

        cells = [cell(str(x), x) for x in range(7)]
        body = [{"id": "guard", "name": "orc", "position": cells[x]["position"]} for x in (3, 4)]
        ground = [{**cells[x], "reachable": False,
                   "item": {"id": str(i), "name": category, "class": category, "identified": False, "quantity": 1}}
                  for i, (x, category) in enumerate(((0, "food"), (2, "misc"), (5, "misc"),
                                                     (5, "food"), (5, "coin"), (6, "food")))]
        turn = player_turn(cells, actors=body, hostiles=["guard"], ground=ground)
        self.assertEqual(guarded_supply_cells(turn), {"5"})

    def test_food_routes_prefer_an_unguarded_ration_but_can_reach_the_last_guarded_one(self):
        cells = [cell("here", down=True), cell("near", 1), cell("hall", -1), cell("far", -2)]
        for hunger, reason in (("nourished", "seek_item"), ("hungry", "forage")):
            for sites, direction in (((cells[1], cells[3]), Direction.WEST), ((cells[1],), Direction.EAST)):
                with self.subTest(hunger=hunger, direction=direction):
                    ground = [{**site, "item": {"id": site["key"], "name": "ration", "class": "food",
                                                "quantity": 1, "identified": False}, "reachable": False}
                              for site in sites]
                    turn = player_turn(cells, ground=ground, stats={"nutrition": {"hunger": hunger}})
                    player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                    player.supplies_guarded_until["near"] = 900
                    self.assertEqual(choose(player, turn), (Move(direction), reason))

    def test_only_essential_loot_justifies_crossing_a_guarded_passage(self):
        hall = [cell(str(x), x, down=x == 0) for x in range(5)]
        spear = {"id": "spear", "name": "spear", "class": "weapon", "identity": "item.weapon.spear",
                 "identified": True, "quantity": 1}
        ration = {"id": "food", "name": "ration", "class": "food", "identified": False, "quantity": 1}
        weapons = [{**hall[2], "item": spear, "reachable": False}]
        for ground, expected in ((weapons, (Move(Direction.DOWN), "descend")),
                                 (weapons + [{**hall[4], "item": ration, "reachable": False}],
                                  (Move(Direction.EAST), "seek_item"))):
            with self.subTest(items=len(ground)):
                turn = player_turn(hall, ground=ground)
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.supplies_guarded_until["1"] = 900
                self.assertEqual(choose(player, turn), expected)

    def test_loot_detours_cross_a_known_pit_only_for_essential_supplies(self):
        hall = [cell(str(x), x, down=x == 0) for x in range(4)]
        hall[2]["trap"] = {"id": "pit", "family": None, "reachable": False}
        for kind, identity, expected in (
            ("weapon", "item.weapon.spear", (Move(Direction.DOWN), "descend")),
            ("potion", None, (Move(Direction.DOWN), "descend")),
            ("food", None, (Move(Direction.EAST), "seek_item")),
        ):
            with self.subTest(kind=kind):
                item = {"id": "item", "name": kind, "class": kind, "identity": identity,
                        "identified": identity is not None, "quantity": 1}
                turn = player_turn(hall, ground=[{**hall[3], "item": item, "reachable": False}])
                self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn), expected)

    def test_route_to_guarded_food_avoids_a_second_dangerous_square(self):
        target = cell("ration", 2)
        cells = [cell("here", down=True), cell("shortcut", 1), target,
                 cell("detour", 0, 1), cell("wall", 1, 1, wall=True), cell("last", 2, 1)]
        cells.extend(cell(f"hall-{x}", x, 2) for x in range(3))
        food = {"id": "food", "name": "ration", "class": "food", "quantity": 1, "identified": False}
        turn = player_turn(cells, ground=[{**target, "item": food, "reachable": False}])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.danger_until.update(ration=900, shortcut=900)
        self.assertEqual(choose(player, turn), (Move(Direction.SOUTH), "seek_item"))

    def test_food_detour_does_not_require_equipment_choices(self):
        stairs, nearby = cell("stairs", down=True), cell("ration", 1)
        ration = {"id": "food", "name": "ration", "class": "food", "quantity": 1, "identified": False}
        turn = player_turn([stairs, nearby], ground=[{**nearby, "item": ration, "reachable": False}])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        with patch.object(turn, "equip_choices", side_effect=AssertionError("unexpected equipment scan")):
            goals = player.loot_goals(turn, turn.ground_items, returning=False)
        self.assertEqual(goals.essential, {"ration"})

    def test_visible_armor_piles_share_one_inventory_check(self):
        stairs, east, west = cell("stairs", down=True), cell("east", 1), cell("west", -1)
        plate = {"name": "plate", "identity": "item.armor.plate-mail",
                 "class": "armor", "identified": True, "quantity": 1}
        ground = [{**site, "item": {**plate, "id": site["key"]}, "reachable": False} for site in (east, west)]
        turn = player_turn([stairs, east, west], ground=ground)
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        with patch.object(turn, "equip_choices", wraps=turn.equip_choices) as choices:
            goals = player.loot_goals(turn, turn.ground_items, returning=False)
        self.assertEqual(goals.supplies, {"east", "west"})
        choices.assert_called_once_with()

    def test_only_a_usable_unmeasured_spare_can_defer_an_armor_detour(self):
        stairs, near = cell("stairs", down=True), cell("plate", 1)
        slot = {"id": "body_armor", "name": "Armor"}
        plate = {"id": "plate", "name": "plate", "identity": "item.armor.plate-mail",
                 "class": "armor", "identified": True, "quantity": 1}
        ground = [{**near, "item": plate, "reachable": False}]
        for identity in ("item.armor.ring-mail", plate["identity"]):
            for cursed, expected_sites in ((True, {"plate"}), (False, set())):
                with self.subTest(identity=identity, cursed=cursed):
                    spare = {**plate, "id": "spare", "identity": identity}
                    turn = player_turn([stairs, near], inventory=[spare], ground=ground, interactions={
                        "slots": [slot], "inventory": [{"item": "spare", "slot": slot, "known_cursed": cursed}]})
                    player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                    player.observe(turn)
                    goals = player.loot_goals(turn, turn.ground_items, returning=False)
                    self.assertEqual(goals.supplies, expected_sites)

    def test_blocked_armor_detour_requires_a_readable_cure(self):
        stairs, near = cell("stairs", down=True), cell("plate", 1)
        slot = {"id": "body_armor", "name": "Armor"}
        mail = {"id": "mail", "name": "ring mail", "identity": "item.armor.ring-mail",
                "class": "armor", "identified": True, "quantity": 1}
        plate = {**mail, "id": "plate", "identity": "item.armor.plate-mail"}
        cure = {**mail, "id": "cure", "class": "scroll", "identity": "item.scroll.remove-curse"}
        for readable in (False, True):
            with self.subTest(readable=readable):
                turn = player_turn([stairs, near], inventory=[mail, cure],
                                   ground=[{**near, "item": plate, "reachable": False}],
                                   interactions={"slots": [slot], "curse_blocked": ["mail"], "inventory": [
                                       {"item": "mail", "slot": slot, "equipped_slot": 0},
                                       {"item": "cure", "readable": readable}]})
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                goals = player.loot_goals(turn, turn.ground_items, returning=False)
                self.assertEqual(goals.supplies, {"plate"} if readable else set())

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
        # A clear backtrack is better than a detour with a pit on its second step.
        cells[4]["trap"] = {"id": "pit", "family": None, "reachable": True}
        trapped_route = player_turn(cells)
        self.assertEqual(choose(player, trapped_route), (Move(Direction.WEST), "explore"))
        del cells[4]["trap"]
        player.observe(turn)
        player.maps[player.depth].visited.add("unknown-east")
        self.assertEqual(player.choose(turn), (Move(Direction.WEST), "explore"))

    def test_explores_through_recent_danger_when_it_blocks_the_only_frontier(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        turn = player_turn([cell("here"), cell("passage", 1), cell("frontier", 2)])
        player.observe(turn)
        player.maps[player.depth].visited.add("passage")
        player.danger_until["passage"] = turn.tick + 100
        self.assertEqual(player.choose(turn), (Move(Direction.EAST), "explore"))

    def test_take_corridor_before_two_foes_make_contact(self):
        cells, foes = west_corridor()
        turn = player_turn(cells, actors=foes, hostiles=["0", "1"])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        move, reason = choose(player, turn)
        self.assertEqual((move, reason), (Move(Direction.WEST), "take_corridor"))
        player.admitted(turn, move, reason=reason)
        entrance = [cell("-1,0"), cell("-2,0", -1)]
        self.assertEqual(choose(player, player_turn(entrance, tick=599)), (Wait(), "hold_corridor"))
        self.assertNotEqual(choose(player, player_turn(entrance, tick=600))[1], "hold_corridor")

    def test_trapped_player_cannot_claim_a_corridor(self):
        cells, foes = west_corridor()
        for remaining, expected in ((None, "escape_bear_trap"), (500, "wait_for_restraint")):
            with self.subTest(remaining=remaining):
                pinned = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                trap = {"condition": {"id": "rogue.bear_trap", "name": "Bear trap"},
                        "phase": "active", "remaining_ticks": remaining}
                turn = player_turn(cells, actors=foes, hostiles=["0", "1"], stats={"conditions": [trap]})
                action, reason = choose(pinned, turn)
                self.assertEqual(reason, expected)
                pinned.admitted(turn, action, reason=reason)
                self.assertIsNone(pinned.corridor_stand)

    def test_held_player_escapes_the_hold_before_taking_a_corridor(self):
        cells, foes = west_corridor()
        held = json.loads(player_turn(cells, actors=foes, hostiles=["0", "1"]).recording())
        held["state"]["observation"]["linked_holds"] = [{"holder": "unseen", "target": "hold"}]
        turn = Turn(held)
        self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)[1], "escape_hold")

    def test_corridor_clearance_is_measured_from_the_players_outer_body(self):
        cells, foes = west_corridor()
        # The lead body cell gains a step of clearance even though the new distance
        # only equals the distance the pursuers previously had from the player's origin.
        extended_body = {"id": "player", "name": "you", "position": tile(1, 0)["position"]}
        pursuers = [{**foe, "position": tile(3, y)["position"]} for y, foe in enumerate(foes)]
        turn = player_turn([*cells, tile(1, 0), tile(3, 0), tile(3, 1)],
                           actors=[extended_body, *pursuers], hostiles=["0", "1"])
        self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn),
                         (Move(Direction.WEST), "take_corridor"))

    def test_corridor_requires_clear_ground_beyond_the_entrance(self):
        cells, foes = west_corridor()
        pit = {"id": "pit", "family": None, "reachable": True}
        for obstruction in ({"wall": True}, {"trap": pit}):
            with self.subTest(obstruction=obstruction):
                blocked = [*cells]
                blocked[2] = {**cells[2], **obstruction}
                turn = player_turn(blocked, actors=foes, hostiles=["0", "1"])
                self.assertNotEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)[1], "take_corridor")

    def test_claims_a_northbound_corridor_only_at_its_entrance(self):
        corridor = [tile(0, 0), tile(0, -1), tile(0, -2), tile(-1, -1, wall=True), tile(1, -1, wall=True)]
        pursuers = [tile(0, 2), tile(1, 2)]
        foes = [{"id": str(i), "name": "troll", "position": site["position"]} for i, site in enumerate(pursuers)]
        for enclosed in (False, True):
            with self.subTest(enclosed=enclosed):
                sides = [tile(-1, 0, wall=enclosed), tile(1, 0, wall=enclosed)]
                turn = player_turn([*corridor, *sides, *pursuers], actors=foes, hostiles=["0", "1"])
                decision = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual(decision == (Move(Direction.NORTH), "take_corridor"), not enclosed)

    def test_corridor_hold_ends_at_its_deadline_even_without_an_intervening_observation(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        start = player_turn([cell("room"), cell("entrance", -1)], tick=50)
        player.observe(start)
        player.admitted(start, Move(Direction.WEST), reason="take_corridor")
        just_before = player_turn([cell("entrance", down=True)], tick=649)
        self.assertEqual(choose(player, just_before), (Wait(), "hold_corridor"))

        deadline = player_turn([cell("entrance", down=True)], tick=650)
        self.assertEqual(player.choose(deadline), (Move(Direction.DOWN), "descend"))
        player.observe(deadline)
        self.assertIsNone(player.corridor_stand)

    def test_untimed_bear_trap_interrupts_a_corridor_stand_on_arrival(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        start = player_turn([cell("room"), cell("entrance", -1)])
        player.observe(start)
        player.admitted(start, Move(Direction.WEST), reason="take_corridor")

        trap = {"condition": {"id": "rogue.bear_trap", "name": "Bear trap"}, "phase": "active",
                "remaining_ticks": None}
        arrival = player_turn([cell("entrance")], tick=100, stats={"conditions": [trap]})
        self.assertEqual(choose(player, arrival), (Move(Direction.NORTH), "escape_bear_trap"))

    def test_missed_corridor_move_does_not_reserve_the_entrance_for_a_later_visit(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        start = player_turn([cell("room"), cell("entrance", -1)])
        player.observe(start)
        player.admitted(start, Move(Direction.WEST), reason="take_corridor")

        still_in_room = player_turn([cell("room"), cell("entrance", -1)], tick=100)
        player.observe(still_in_room)
        later_visit = player_turn([cell("entrance"), cell("room", 1)], tick=200)
        self.assertNotEqual(choose(player, later_visit)[1], "hold_corridor")

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

        # An unmeasured plate offers six estimated points when known to be uncursed.
        del offers[1]["equipped_slot"]
        inventory[0]["identity"] = "item.armor.plate-mail"
        offers[0] = {"item": "worn", "slot": slot, "known_cursed": False}
        for measured, target, reason in ((6, "plate", "upgrade_armor"), (5, "worn", "try_armor")):
            offers[1] = offer("plate", measured)
            for candidates in (inventory, list(reversed(inventory))):
                with self.subTest(measured=measured, first=candidates[0]["id"]):
                    turn = player_turn([here], inventory=candidates, interactions=interactions)
                    action, decision_reason = choose(player, turn)
                    self.assertIsInstance(action, Equip)
                    self.assertEqual((action.item.target, action.slot, decision_reason), (target, turn.equipment[0], reason))

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

    def test_player_checks_training_on_a_quiet_turn_after_hit_dice_change(self):
        from unittest.mock import Mock
        from rogue_player import play
        from tor_client_bot import Outcome, Result, Session

        foe = {"id": "foe", "name": "orc", "position": cell("east", 1)["position"]}
        for previous, locked, hostiles, expected in (
            (None, False, [], 0), (2, False, [], 0), (1, True, [], 0),
            (1, False, [foe], 0), (1, False, [], 1),
        ):
            with self.subTest(previous=previous, locked=locked, hostiles=bool(hostiles)):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, training_checked_at=previous, growth_locked=locked)
                player.train = Mock(return_value=None)
                turn = player_turn([cell("here"), cell("east", 1)], actors=hostiles,
                                   hostiles=[actor["id"] for actor in hostiles], stats={"hit_dice": {"total": "2"}})
                turn.submit = Mock(return_value=Result(Outcome.CONFIRMED, None, None))
                session = Session(Mock())
                session.wait_for_decision = Mock(side_effect=[turn, player_turn([], combat={"terminal": True, "victory": False})])
                play(session, player, 1, lambda row: None)
                self.assertEqual(player.train.call_count, expected)

    def test_earned_training_waits_for_a_visible_landing(self):
        from unittest.mock import Mock
        from rogue_runner import play
        from tor_client_bot import Outcome, Result, Session

        stats = {"hit_dice": {"total": 2}}
        standing = player_turn([cell("here", down=True)], stats=stats)
        picture = json.loads(standing.recording())
        picture["state"]["observation"]["motion"] = {"velocity": [0, 0, -1]}
        falling = Turn(picture)
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, training_checked_at=1)
        player.train = Mock(return_value=None)
        confirmed = Result(Outcome.CONFIRMED, None, None)
        falling.submit = Mock(return_value=confirmed)
        standing.submit = Mock(return_value=confirmed)
        terminal = player_turn([], combat={"terminal": True, "victory": False, "dead": False})
        session = Session(Mock())
        session.wait_for_decision = Mock(side_effect=[falling, standing, terminal])
        trace = []
        play(session, player, 2, trace.append)
        self.assertEqual([row["reason"] for row in trace], ["settle_fall", "descend"])
        player.train.assert_called_once_with(standing, session.training)

    def test_training_reads_the_talent_options_once_when_choosing_its_second_preference(self):
        from unittest.mock import Mock
        from tor_client_bot import PointPool, Training, TrainingKind, TrainingOption

        kind = TrainingKind.TALENT
        training = Mock(spec=Training)
        training.overview.return_value = Mock(locked=False, unspent=1, starting_skills=())
        pool = PointPool(1, "growth", "Growth", 1, (kind,), ())
        talent = TrainingOption(kind, "dislocating_strike", "Dislocating strike", (), ())
        training.pools.return_value = (pool,)
        training.options.return_value = (talent,)
        training.preview.return_value = Mock(would_die=False)
        turn = player_turn([cell("here")])
        purchase, result = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY).train(turn, training)
        self.assertEqual(purchase, {"type": "purchase", "kind": kind.value, "id": talent.identity,
                                    "hit_die": pool.hit_die, "pool": pool.identity})
        self.assertIs(result, training.confirm.return_value)
        training.preview.assert_called_once_with(pool, talent)
        training.options.assert_called_once_with(kind)

    def test_training_saves_flexible_points_when_a_talent_only_pool_can_pay(self):
        from unittest.mock import Mock
        from rogue_training import purchase_training
        from tor_client_bot import PointPool, Training, TrainingKind, TrainingOption

        talent = TrainingKind.TALENT
        flexible = PointPool(1, "growth", "Growth", 1, (talent, TrainingKind.SKILL), ())
        dedicated = PointPool(2, "talents", "Talents", 1, (talent,), ())
        option = TrainingOption(talent, "rattling_blow", "Rattling blow", (), ())
        for dedicated_is_lethal in (False, True):
            with self.subTest(dedicated_is_lethal=dedicated_is_lethal):
                training = Mock(spec=Training)
                training.pools.return_value = (flexible, dedicated)
                training.options.return_value = (option,)
                safe = Mock(would_die=False)
                training.preview.side_effect = [Mock(would_die=True), safe] if dedicated_is_lethal else [safe]
                purchase, _ = purchase_training(training, ())
                expected_pool = flexible if dedicated_is_lethal else dedicated
                self.assertEqual(purchase["pool"], expected_pool.identity)
                self.assertEqual(training.preview.call_args_list[0].args, (dedicated, option))
                training.confirm.assert_called_once_with(safe)

    def test_training_skips_a_lethal_preview_and_confirms_only_the_safe_alternative(self):
        from unittest.mock import Mock
        from rogue_training import purchase_training
        from tor_client_bot import PointPool, Training, TrainingKind, TrainingOption

        kind = TrainingKind.TALENT
        training = Mock(spec=Training)
        pool = PointPool(1, "growth", "Growth", 1, (kind,), ())
        training.pools.return_value = (pool,)
        training.options.return_value = tuple(TrainingOption(kind, name, name, (), ())
                                              for name in ("rattling_blow", "dislocating_strike"))
        safe = Mock(would_die=False)
        training.preview.side_effect = [Mock(would_die=True), safe]
        purchase, _ = purchase_training(training, ())
        self.assertEqual(purchase["id"], "dislocating_strike")
        training.confirm.assert_called_once_with(safe)

    def test_training_skips_refused_previews_but_stops_on_stale_context(self):
        from unittest.mock import Mock
        from tor_client_bot import Outcome, PointPool, Result, Training, TrainingKind, TrainingOption, TrainingRefused

        kind = TrainingKind.TALENT
        pool = PointPool(1, "growth", "Growth", 1, (kind,), ())
        options = tuple(TrainingOption(kind, name, name, (), ()) for name in ("rattling_blow", "dislocating_strike"))
        for code in ("ineligible", "stale_context"):
            with self.subTest(code=code):
                training = Mock(spec=Training)
                training.overview.return_value = Mock(locked=False, unspent=1, starting_skills=())
                training.pools.return_value = (pool,)
                training.options.return_value = options
                refusal = TrainingRefused(Result(Outcome.REJECTED, None, None, code=code))
                safe_preview = Mock(would_die=False)
                training.preview.side_effect = [refusal, safe_preview]
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                turn = player_turn([cell("here")])
                if code == "stale_context":
                    with self.assertRaises(TrainingRefused):
                        player.train(turn, training)
                    training.preview.assert_called_once_with(pool, options[0])
                    training.confirm.assert_not_called()
                else:
                    purchase, _ = player.train(turn, training)
                    self.assertEqual(purchase["id"], options[1].identity)
                    training.confirm.assert_called_once_with(safe_preview)

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

    def test_descent_without_known_upstairs_records_time_without_searching_the_map(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        start = player_turn([cell("down", down=True)], tick=100)
        player.observe(start)
        player.admitted(start, Move(Direction.DOWN))
        with patch.object(player.maps[1], "distance", side_effect=AssertionError("no known upstairs")):
            player.observe(player_turn([cell("landing", up=True)], tick=200))
        self.assertEqual(player.travel_times.descent, {1: 100})
        self.assertEqual(player.travel_times.ascent, {})

    def test_same_tick_stair_arrival_preserves_travel_estimates(self):
        stairs = [cell("down", down=True), cell("up", 1, up=True)]
        for entered_at, descent in ((100, {}), (25, {2: 75})):
            with self.subTest(entered_at=entered_at):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
                player.observe(player_turn(stairs, tick=entered_at))
                start = player_turn(stairs, tick=100)
                player.observe(start)
                player.travel_times.ascent[2] = 200
                player.admitted(start, Move(Direction.DOWN))
                player.observe(player_turn([cell("landing", up=True)], tick=100))
                self.assertEqual(player.depth, 3)
                self.assertEqual(player.travel_times.ascent, {2: 200})
                self.assertEqual(player.travel_times.descent, descent)

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
        self.assertEqual(player.travel_times.ascent, {2: 300})
        self.assertEqual(player.travel_times.descent, {2: 200})

        # A faster second crossing updates the return estimate while preserving the first exploration time.
        lower = player_turn([cell("lower", up=True)], tick=250)
        player.observe(lower)
        player.admitted(lower, Move(Direction.UP))
        revisited = player_turn([cell("down", down=True)], tick=300)
        player.observe(revisited)
        player.admitted(revisited, Move(Direction.DOWN))
        player.observe(player_turn([cell("lower", up=True)], tick=350))
        self.assertEqual(player.travel_times.ascent, {2: 150})
        self.assertEqual(player.travel_times.descent, {2: 200})

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

    def test_observing_a_known_floor_reuses_its_map_without_allocating_another(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(player_turn([cell("here")]))
        memory = player.maps[player.depth]
        with patch("rogue_player.FloorMap", side_effect=AssertionError("unexpected new map")):
            player.observe(player_turn([cell("here"), cell("exit", 1, down=True)]))
        self.assertIn("exit", memory.stairs(Direction.DOWN))

    def test_stair_travel_prefers_a_clear_approach_and_keeps_the_only_exit(self):
        amulet = {"id": "prize", "name": AMULET, "class": "amulet", "identity": DEFAULT_OBJECTIVE_IDENTITY,
                  "identified": True, "quantity": 1}
        for returning in (False, True):
            for west_exit in (False, True):
                with self.subTest(returning=returning, west_exit=west_exit):
                    exits = {2, -3} if west_exit else {2}
                    cells = [cell(str(x), x, up=returning and x in exits, down=not returning and x in exits)
                             for x in range(-3, 3)]
                    player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
                    player.danger_until["1"] = 100
                    for tick, expected in ((99, Direction.WEST if west_exit else Direction.EAST),
                                           (100, Direction.EAST)):
                        turn = player_turn(cells, inventory=[amulet] if returning else [], tick=tick)
                        self.assertEqual(choose(player, turn), (Move(expected), "seek_stairs"))

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

    def test_stair_arrival_only_evades_an_unseen_attack_at_the_new_landing(self):
        from tor_client_bot import CombatReport

        origin = player_turn([cell("old-floor", down=True)])
        arrival = player_turn([cell("landing", up=True, down=True), cell("passage", 1)], tick=100)
        miss = {"type": "unseen_ability", "target": "player", "outcome": "miss"}
        for events, expected in (((), "descend"), ((miss,), "evade_unseen_attack")):
            with self.subTest(attacked_on_arrival=bool(events)):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observe(origin, CombatReport("player", (miss,)))
                player.admitted(origin, Move(Direction.DOWN))
                player.observe(arrival, CombatReport("player", events))
                self.assertEqual(player.depth, 2)
                self.assertEqual(player.choose(arrival)[1], expected)

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

    def test_failed_stair_retreat_does_not_mark_a_later_ordinary_landing_unsafe(self):
        from tor_client_bot import Result

        origin = player_turn([cell("stairs", down=True), cell("guard", 1)],
                             actors=[{"id": "orc", "name": "orc", "position": cell("guard", 1)["position"]}],
                             hostiles=["orc"])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(origin)
        down = Move(Direction.DOWN)
        player.admitted(origin, down, Result.from_reply({"status": "rejected"}), reason="retreat_downstairs")
        player.observe(origin)
        player.admitted(origin, down, reason="descend")
        player.observe(player_turn([cell("landing", up=True)], tick=100))
        self.assertEqual(player.depth, 2)
        self.assertNotIn(("landing", Direction.UP), player.unsafe_stairs)

    def test_failed_stair_is_retired_before_its_receipt_leaves_history(self):
        from tor_client_bot import Result

        origin = player_turn([cell("origin", down=True)])
        receipt = {"actor": 1, "branch": "game", "intention": "down", "phase": "queued", "type": "admitted"}
        accepted = Result.from_reply({"status": "confirmed", "receipt": receipt})
        for phase in ("failed", "cancelled"):
            with self.subTest(phase=phase):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observe(origin)
                player.admitted(origin, Move(Direction.DOWN), accepted)
                picture = json.loads(player_turn([], tick=10).recording())
                picture["readiness"] = {"admission": False}
                picture["finished_intentions"] = [dict(receipt, phase=phase)]
                player.observe(Turn(picture))
                self.assertIsNone(player.pending_stair)
                player.observe(player_turn([cell("unrelated", up=True)], tick=100))
                self.assertEqual(player.depth, 1)

    def test_visible_landing_is_counted_once_the_player_is_ready(self):
        origin = player_turn([cell("above", down=True)])
        landing = player_turn([cell("below", up=True)], tick=100)
        picture = json.loads(landing.recording())
        picture["readiness"] = {"admission": False}

        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(origin)
        player.admitted(origin, Move(Direction.DOWN))
        player.observe(Turn(picture))
        self.assertIsNotNone(player.pending_stair)
        self.assertEqual(player.depth, 1)
        player.observe(landing)
        self.assertIsNone(player.pending_stair)
        self.assertEqual(player.depth, 2)

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
        landed["finished_intentions"] = [dict(row, phase="started")]
        policy.observe(Turn(landed))
        self.assertEqual(policy.depth, 1)
        self.assertNotIn("below", policy.maps[1].cells)

        landed["finished_intentions"] = [dict(row, phase="resolved")]
        arrival = Turn(landed)
        policy.observe(arrival)
        policy.observe(arrival)
        self.assertEqual(policy.depth, 2)
        self.assertIn("below", policy.maps[2].cells)
        self.assertEqual(policy.travel_times.descent, {1: 100})

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

    def test_search_requires_a_ready_search_offer_before_travelling_to_a_site(self):
        site = [cell("here"), cell("site", 1), {**cell("wall", 2), "wall": True}]
        ability = {"id": "rogue.search", "name": "Search", "cost": None, "basic_attack": False,
                   "reach": {"kind": "self_only"}, "target_kind": "self_only"}
        for cooldown, ready in ((1, False), (0, True)):
            with self.subTest(cooldown=cooldown):
                turn = player_turn(site, abilities=[dict(ability, cooldown_remaining=str(cooldown))])
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observe(turn)
                expected = (Move(Direction.EAST), "search") if ready else None
                self.assertEqual(player.search(turn, player.maps[1], returning=False), expected)

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

    def test_sealed_landing_tries_a_scroll_between_the_first_and_second_wall_sweeps(self):
        cells = [cell("landing", up=True)]
        cells.extend(cell(f"wall-{x}-{y}", x, y, wall=True) for x, y in ((1, 0), (-1, 0), (0, 1), (0, -1)))
        search = {"id": "rogue.search", "name": "Search", "cooldown_remaining": "0", "cost": None,
                  "basic_attack": False, "reach": {"kind": "self_only"}, "target_kind": "self_only"}
        scroll = {"id": "roll", "name": "runed scroll", "class": "scroll", "quantity": 1, "identified": False}
        turn = player_turn(cells, abilities=[search], inventory=[scroll],
                           interactions={"inventory": [{"item": "roll", "readable": True}]})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
        player.observe(turn)
        for _ in range(3):
            action, reason = player.choose(turn)
            self.assertEqual(reason, "search")
            player.admitted(turn, action)
        action, reason = player.choose(turn)
        self.assertEqual((action.item.target, reason), ("roll", "test_unknown_magic"))
        player.admitted(turn, action, reason=reason)
        after_read = player_turn(cells, abilities=[search], tick=100)
        player.observe(after_read)
        for _ in range(3):
            action, reason = player.choose(after_read)
            self.assertEqual(reason, "search")
            player.admitted(after_read, action)
        self.assertEqual(player.choose(after_read), (None, "no_disclosed_route"))

    def test_navigation_report_counts_only_the_current_floor(self):
        from rogue_runner import trace_navigation

        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(player_turn([cell("start", down=True)]))
        player.depth = 2
        player.observe(player_turn([cell("landing", up=True), cell("exit", 1, down=True)]))
        player.search_history.admissions.update({"start": 4, "landing": 2})
        self.assertEqual(trace_navigation(player), {
            "floor": 2, "visited_cells": 1, "known_up_stairs": 1, "known_down_stairs": 1, "searches_admitted": 2,
        })

    def test_search_credits_disclosed_nearby_walls_on_the_current_floor(self):
        search = {"id": "rogue.search", "name": "Search", "cooldown_remaining": "0", "cost": None,
                  "basic_attack": False, "reach": {"kind": "self_only"}, "target_kind": "self_only"}
        room = [cell("here")]
        for key, x, y, z in (("diagonal", 1, 1, 0), ("diagonal", -1, 1, 0), ("middle", -3, 3, 0),
                             ("far", 4, 0, 0), ("above", 1, 1, 1)):
            room.append({**cell(key), "wall": True, "position": {"x": x, "y": y, "z": z}})
        turn = player_turn(room, abilities=[search])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        player.admitted(turn, UseAbility(turn.abilities[0]))
        self.assertEqual(dict(player.search_history.wall_searches), {"diagonal": 1, "middle": 1})

        # A second site contributes another attempt to the same visible wall.
        next_room = [cell("other"), {**cell("diagonal"), "wall": True,
                                     "position": {"x": 0, "y": 1, "z": 0}}]
        next_turn = player_turn(next_room, abilities=[search])
        player.observe(next_turn)
        player.admitted(next_turn, UseAbility(next_turn.abilities[0]))
        self.assertEqual(dict(player.search_history.admissions), {"here": 1, "other": 1})
        self.assertEqual(dict(player.search_history.wall_searches), {"diagonal": 2, "middle": 1})

    def test_resolved_search_updates_only_the_player_that_submitted_it(self):
        from tor_client_bot import Result

        turn = search_turn([cell("start"), cell("wall", 1, wall=True)])
        searching, idle = (Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY) for _ in range(2))
        searching.observe(turn)
        idle.observe(turn)
        receipt = Result.from_reply({"status": "confirmed", "receipt": search_activity()})
        searching.admitted(turn, UseAbility(turn.abilities[0]), receipt)
        finished = Turn({**json.loads(turn.recording()), "finished_intentions": [search_activity("resolved")]})
        idle.observe(finished)
        searching.observe(finished)
        self.assertEqual(searching.search_history.wall_searches["wall"], 1)
        self.assertEqual(idle.search_history.wall_searches["wall"], 0)

    def test_wall_coverage_counts_only_resolved_search_receipts_and_uses_the_original_picture(self):
        from tor_client_bot import Result

        turn = search_turn([cell("start"), cell("original-wall", 1, wall=True)])
        receipt = Result.from_reply({"status": "confirmed", "receipt": search_activity()})
        later = player_turn([cell("other"), cell("new-wall", 1, wall=True)])
        for phase in ("cancelled", "failed", "resolved"):
            with self.subTest(phase=phase):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observe(turn)
                player.admitted(turn, UseAbility(turn.abilities[0]), receipt)
                stale = search_activity("resolved", intention="earlier-search")
                player.observe(Turn({**json.loads(later.recording()), "finished_intentions": [stale]}))
                self.assertFalse(player.search_history.wall_searches)
                final = Turn({**json.loads(later.recording()), "finished_intentions": [search_activity(phase)]})
                player.observe(final)
                player.observe(final)
                self.assertEqual(player.search_history.attempts_at("start"), int(phase == "resolved"))
                self.assertEqual(dict(player.search_history.wall_searches), {"original-wall": 1} if phase == "resolved" else {})
                self.assertEqual(player.search_history.admissions["start"], 1)

    def test_a_reported_search_in_progress_survives_a_later_observation(self):
        from tor_client_bot import Result

        cells = [cell("here"), cell("wall", 1, wall=True)]
        turn = search_turn(cells)
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        receipt = Result.from_reply({"status": "confirmed", "receipt": search_activity()})
        player.admitted(turn, UseAbility(turn.abilities[0]), receipt)
        later = json.loads(player_turn(cells, tick=100).recording())
        player.observe(Turn({**later, "intentions": [search_activity("paused")]}))
        self.assertIsNotNone(player.search_history.pending_search)
        player.observe(Turn({**later, "finished_intentions": [search_activity("resolved")]}))
        self.assertEqual(player.search_history.wall_searches["wall"], 1)

    def test_a_repeated_ready_turn_waits_for_the_pending_search_before_exploring(self):
        from tor_client_bot import Result

        turn = search_turn([cell("end"), cell("hall", -1), cell("east", 1, wall=True),
                            cell("north", y=-1, wall=True), cell("south", y=1, wall=True)])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        action, reason = choose(player, turn)
        self.assertEqual(reason, "search")
        receipt = Result.from_reply({"status": "confirmed", "receipt": search_activity()})
        player.admitted(turn, action, receipt)
        self.assertEqual(choose(player, turn), (Wait(), "await_search_result"))
        self.assertEqual(player.search_history.wall_searches["east"], 0)
        finished = Turn({**json.loads(turn.recording()), "finished_intentions": [search_activity("resolved")]})
        self.assertEqual(choose(player, finished)[1], "search")
        self.assertEqual(player.search_history.wall_searches["east"], 1)

    def test_search_with_expired_history_keeps_only_its_admission_count(self):
        from tor_client_bot import Result

        cells = [cell("here"), cell("wall", 1, wall=True)]
        turn = search_turn(cells)
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        receipt = Result.from_reply({"status": "confirmed", "receipt": search_activity()})
        player.admitted(turn, UseAbility(turn.abilities[0]), receipt)
        history = player.search_history
        player.observe(turn)
        self.assertIsNotNone(history.pending_search)
        player.observe(player_turn(cells, tick=turn.tick + 100))
        self.assertIsNone(history.pending_search)
        self.assertEqual(history.attempts_at("here"), 1)
        self.assertFalse(history.wall_searches)
        self.assertFalse(history.completed_end_searches)

    def test_return_connection_sites_only_include_the_players_side_of_a_barrier(self):
        cells = [cell("here"), cell("west-wall", 1, wall=True), cell("stairs", 2, up=True),
                 cell("east-wall", 3, wall=True), cell("isolated", 4)]
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(search_turn(cells))
        memory = player.maps[player.depth]
        self.assertEqual(player.search_history.return_connection_sites(memory, "here", {"stairs"}), {"here"})

    def test_search_at_the_current_wall_needs_no_movement_route(self):
        turn = search_turn([cell("here"), cell("wall", 1, wall=True), cell("stairs", 2, up=True)])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        memory = player.maps[1]
        expected = (UseAbility(turn.abilities[0]), "search")
        with patch.object(memory, "toward", side_effect=AssertionError("already at the search site")):
            self.assertEqual(player.search(turn, memory, returning=False), expected)
            self.assertEqual(player.search_return_connection(turn, memory, {"stairs"}), expected)

    def test_exhausted_return_search_sites_do_not_request_a_route(self):
        from rogue_search import ORDINARY_WALL_SEARCHES

        turn = search_turn([cell("here"), cell("wall", 1, wall=True), cell("stairs", 2, up=True)])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        player.search_history.admissions["here"] = ORDINARY_WALL_SEARCHES
        memory = player.maps[player.depth]
        with patch.object(memory, "toward", side_effect=AssertionError("no eligible search destination")):
            self.assertIsNone(player.search_return_connection(turn, memory, {"stairs"}))

    def test_cancelled_return_search_can_be_retried_at_the_same_boundary(self):
        from tor_client_bot import Result

        cells = [cell("hall", -1), cell("here"), cell("wall", 1, wall=True), cell("stairs", 2, up=True)]
        turn = search_turn(cells)
        receipt = Result.from_reply({"status": "confirmed", "receipt": search_activity()})
        for phase in ("cancelled", "failed", "resolved"):
            with self.subTest(phase=phase):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observe(turn)
                player.search_history.admissions["here"] = 2
                search = UseAbility(turn.abilities[0])
                player.admitted(turn, search, receipt)
                final = Turn({**json.loads(turn.recording()), "finished_intentions": [search_activity(phase)]})
                player.observe(final)
                decision = player.search_return_connection(final, player.maps[player.depth], {"stairs"})
                self.assertEqual(decision, None if phase == "resolved" else (search, "search"))
                self.assertEqual(player.search_history.admissions["here"], 3)

    def test_search_prefers_two_distinct_walls_over_two_views_of_one_wall(self):
        turn = search_turn([cell("here"), cell("same-wall", y=-1, wall=True),
                            cell("same-wall", y=1, wall=True), cell("hall", 1),
                            cell("end-wall", 2, wall=True), cell("side-wall", 1, -1, wall=True)])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        self.assertEqual(player.search(turn, player.maps[player.depth], returning=False),
                         (Move(Direction.EAST), "search"))

    def test_equivalent_search_sites_are_ranked_independently_of_opaque_cell_keys(self):
        ability = {"id": "rogue.search", "name": "Search", "cooldown_remaining": "0", "cost": None,
                   "basic_attack": False, "reach": {"kind": "self_only"}, "target_kind": "self_only"}
        def decision(left, right, *, reverse_observation=False):
            player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
            cells = [cell("here"), cell(left, -1), cell(right, 1),
                     {**cell("left-wall", -2), "wall": True}, {**cell("right-wall", 2), "wall": True}]
            turn = player_turn(list(reversed(cells)) if reverse_observation else cells, abilities=[ability])
            player.observe(turn)
            return player.search(turn, player.maps[1], returning=False)

        expected = decision("a", "z")
        self.assertIsInstance(expected[0], Move)
        self.assertEqual(decision("z", "a"), expected)
        self.assertEqual(decision("z", "a", reverse_observation=True), expected)

    def test_search_limit_allows_one_last_attempt_toward_unseen_terrain(self):
        ability = {"id": "rogue.search", "name": "Search", "cooldown_remaining": 0, "cost": None,
                   "basic_attack": False, "reach": {"kind": "self_only"}, "target_kind": "self_only"}
        turn = player_turn([cell("here"), {**cell("wall", 1), "wall": True}], abilities=[ability])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        memory = player.maps[1]
        for limit in (3, 6):
            with self.subTest(limit=limit):
                player.search_history.admissions["here"] = limit
                self.assertIsNone(player.search(turn, memory, returning=False, allow_unseen=False,
                                                wall_limit=limit))
                self.assertEqual(player.search(turn, memory, returning=False, wall_limit=limit),
                                 (UseAbility(turn.abilities[0]), "search"))
                player.search_history.admissions["here"] += 1
                self.assertIsNone(player.search(turn, memory, returning=False, wall_limit=limit))

    def test_equivalent_wall_searches_prefer_a_route_outside_recent_danger(self):
        ability = {"id": "rogue.search", "name": "Search", "cooldown_remaining": 0, "cost": None,
                   "basic_attack": False, "reach": {"kind": "self_only"}, "target_kind": "self_only"}
        turn = player_turn([cell("here"), cell("west", -1), cell("east", 1),
                            cell("west-wall", -2, wall=True), cell("east-wall", 2, wall=True)], abilities=[ability])
        for hazard, escape in (("east", Direction.WEST), ("west", Direction.EAST)):
            with self.subTest(hazard=hazard):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observe(turn)
                player.danger_until[hazard] = 100
                self.assertEqual(player.search(turn, player.maps[1], returning=False), (Move(escape), "search"))

    def test_equivalent_wall_searches_prefer_the_route_without_a_disclosed_trap(self):
        ability = {"id": "rogue.search", "name": "Search", "cooldown_remaining": "0", "cost": None,
                   "basic_attack": False, "reach": {"kind": "self_only"}, "target_kind": "self_only"}
        pit = {"id": "pit", "family": "pit", "reachable": True}
        for trapped, safe_direction in ((-1, Direction.EAST), (1, Direction.WEST)):
            with self.subTest(trapped=trapped):
                cells = [cell("here")]
                for x in (-1, 1):
                    site = cell(str(x), x)
                    if x == trapped:
                        site["trap"] = pit
                    cells.extend([site, {**cell(f"wall-{x}", 2 * x), "wall": True}])
                turn = player_turn(cells, abilities=[ability])
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observe(turn)
                self.assertEqual(player.search(turn, player.maps[1], returning=False),
                                 (Move(safe_direction), "search"))

    def test_ground_loot_and_arrow_recovery_resume_when_levitation_ends(self):
        stairs, near = cell("stairs", down=True), cell("food", 1)
        bow = {"id": "bow", "identity": "item.weapon.shortbow", "name": "bow",
               "class": "weapon", "quantity": 1, "identified": True}
        food = {"id": "ration", "name": "ration", "class": "food", "quantity": 1, "identified": False}
        floating = {"condition": {"id": "rogue.levitation", "name": "Levitating"},
                    "phase": "active", "remaining_ticks": "100"}
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        for conditions, grounded in (([floating], False), ([], True)):
            with self.subTest(grounded=grounded):
                turn = player_turn([stairs, near], inventory=[bow], ground=[{**near, "item": food, "reachable": False}],
                                   stats={"conditions": conditions})
                goals = player.loot_goals(turn, turn.ground_items, returning=False)
                self.assertEqual(goals.essential, {"food"} if grounded else set())
                self.assertEqual(goals.recover_arrows, grounded)
                expected = (Move(Direction.EAST), "seek_item") if grounded else (Wait(), "wait_for_ground_contact")
                self.assertEqual(choose(player, turn), expected)

    def test_waits_on_the_stair_for_levitation_to_end(self):
        condition = {"condition": {"id": "rogue.levitation", "name": "Levitating"},
                     "phase": "active", "remaining_ticks": "1958"}
        policy = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        stairs = [cell("down", down=True)]
        floating = player_turn(stairs, stats={"conditions": [condition]})
        self.assertEqual(choose(policy, floating), (Wait(), "wait_for_ground_contact"))
        self.assertEqual(choose(policy, player_turn(stairs)), (Move(Direction.DOWN), "descend"))

    def test_wand_appearance_and_charge_count_govern_different_retries(self):
        from dataclasses import replace
        from rogue_trials import WandTrials

        inventory = [{"id": target, "name": "oak staff", "class": "wand", "quantity": 1, "identified": False}
                     for target in ("first", "second")]
        offers = [{"item": item["id"], "zappable": True, "device_activation": "direction"} for item in inventory]
        first, second = player_turn([cell("here")], inventory=inventory, interactions={"inventory": offers}).devices
        trials = WandTrials()
        self.assertTrue(trials.can_experiment(first))
        trials.admitted(first, experiment=True)
        self.assertFalse(trials.can_spend_charge(first))
        self.assertTrue(trials.can_spend_charge(second))
        self.assertFalse(trials.can_experiment(second))
        self.assertTrue(trials.can_spend_charge(replace(first, charges=1)))
        self.assertFalse(trials.can_spend_charge(replace(first, charges=0)))

    def test_readies_a_weapon_before_experimenting_with_an_unknown_potion(self):
        mace = {"id": "mace", "identity": "item.weapon.mace", "class": "weapon", "name": "mace",
                "quantity": 1, "identified": True}
        flask = {"id": "flask", "class": "potion", "name": "cloudy flask", "quantity": 1, "identified": False}
        slot = {"id": "weapon", "name": "Weapon"}
        for equipped, expected in ((False, "equip_melee"), (True, "test_unknown_potion")):
            with self.subTest(equipped=equipped):
                turn = player_turn([cell("stairs", down=True)], inventory=[flask, mace], interactions={
                    "slots": [slot], "inventory": [{"item": "flask", "drinkable": True},
                                                  {"item": "mace", "slot": slot, "equipped_slot": 0 if equipped else None}]})
                self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)[1], expected)

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

    def test_new_potion_appearance_waits_for_time_to_advance_after_any_admission(self):
        from tor_client_bot import Result

        potions = [{"id": name, "name": name, "class": "potion", "quantity": 1, "identified": False}
                   for name in ("smoky", "blue")]
        def scene(tick):
            return player_turn([cell("stairs", down=True)], tick=tick, inventory=potions, interactions={
                "inventory": [{"item": potion["id"], "drinkable": True} for potion in potions]})

        activity = {"type": "admitted", "actor": 1, "branch": "game", "intention": "drink", "phase": "queued"}
        confirmed = Result.from_reply({"status": "confirmed"})
        queued = Result.from_reply({"status": "confirmed", "receipt": activity})
        for receipt in (queued, confirmed, None):
            with self.subTest(receipt=receipt):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                before = scene(100)
                action, reason = choose(player, before)
                self.assertEqual(action.item.target, "smoky")
                player.admitted(before, action, receipt, reason=reason)
                self.assertEqual(player.potion_trials.sampled, set() if receipt is queued else {"smoky"})
                self.assertEqual(choose(player, before)[1], "descend")
                self.assertEqual(choose(player, scene(50))[1], "descend")
                action, reason = choose(player, scene(200))
                self.assertEqual((action.item.target, reason), ("blue", "test_unknown_potion"))

    def test_potion_trial_retries_only_when_a_failed_drink_left_the_stack_intact(self):
        from tor_client_bot import Result

        item = {"id": "flask", "name": "blue flask", "class": "potion", "quantity": 2, "identified": False}
        def scene(quantity, tick):
            return player_turn([cell("stairs", down=True)], inventory=[dict(item, quantity=quantity)], tick=tick,
                               interactions={"inventory": [{"item": "flask", "drinkable": True}]})
        receipt = {"type": "admitted", "actor": 1, "branch": "game", "intention": "drink", "phase": "queued"}
        for phase, quantity, expected in (("failed", 2, "test_unknown_potion"), ("cancelled", 2, "test_unknown_potion"),
                                          ("failed", 3, "test_unknown_potion"), ("resolved", 2, "descend"),
                                          ("failed", 1, "descend")):
            with self.subTest(phase=phase, quantity=quantity):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                before = scene(2, 50)
                action, reason = choose(player, before)
                player.admitted(before, action, Result.from_reply({"status": "confirmed", "receipt": receipt}), reason=reason)
                for tick, next_reason in ((50, "descend"), (25, "descend"), (100, expected)):
                    after = json.loads(scene(quantity, tick).recording())
                    after["finished_intentions"] = [dict(receipt, phase=phase)]
                    self.assertEqual(choose(player, Turn(after))[1], next_reason)

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

    def test_distant_healing_potion_is_worth_a_detour_until_two_are_packed(self):
        cells = [cell("stairs", down=True)] + [cell(str(x), x) for x in range(1, 5)]
        flask = {"id": "flask", "name": "healing", "class": "potion", "quantity": 1,
                 "identified": True, "identity": "item.potion.healing"}
        for packed, expected in ((1, (Move(Direction.EAST), "seek_item")), (2, (Move(Direction.DOWN), "descend"))):
            with self.subTest(packed=packed):
                turn = player_turn(cells, inventory=[{**flask, "id": "pack", "quantity": packed}],
                                   ground=[{**cells[-1], "item": flask, "reachable": False}])
                self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn), expected)

    def test_pickup_and_detour_count_separate_stacks_of_each_healing_kind(self):
        here, shelf = cell("stairs", down=True), cell("shelf", 1)
        healing = {"id": "heal", "name": "healing", "identity": "item.potion.healing",
                   "class": "potion", "identified": True, "quantity": 1}
        extra = {**healing, "id": "extra", "identity": "item.potion.extra-healing"}
        stock = [{**healing, "id": "first"}, {**healing, "id": "second"}]
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        ground = [{**here, "item": item, "reachable": True} for item in (healing, extra)]
        turn = player_turn([here, shelf], inventory=stock, ground=ground)
        self.assertEqual(player.choose_pickup(turn, turn.ground_items, returning=False).target, "extra")

        other_shelf = cell("other-shelf", -1)
        turn = player_turn([here, shelf, other_shelf], inventory=stock, ground=[
            {**shelf, "item": healing, "reachable": False}, {**other_shelf, "item": extra, "reachable": False}])
        self.assertEqual(player.loot_goals(turn, turn.ground_items, returning=False).wanted, {"other-shelf"})

    def test_empty_floor_preserves_arrow_recovery_without_counting_inventory_for_ground_loot(self):
        from unittest.mock import patch

        bow = {"id": "bow", "name": "bow", "identity": "item.weapon.shortbow", "class": "weapon",
               "quantity": 1, "identified": True}
        turn = player_turn([cell("here")], inventory=[bow])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        with patch("rogue_loot.CarriedLoot", side_effect=AssertionError("no ground stack needs a stock count")):
            goals = player.loot_goals(turn, turn.ground_items, returning=False)
        self.assertTrue(goals.recover_arrows)
        self.assertFalse(goals.essential or goals.weapons or goals.nearby or goals.supplies)

    def test_return_arrow_detour_requires_a_bow_low_stock_and_no_visible_foe(self):
        here, stash = cell("exit", up=True), cell("stash", 1)
        bow = {"id": "bow", "name": "bow", "identity": "item.weapon.shortbow",
               "class": "weapon", "quantity": 1, "identified": True}
        arrows = {**bow, "id": "arrows", "identity": "item.weapon.arrow"}
        ground = [{**stash, "item": arrows, "reachable": False}]
        foe = {"id": "foe", "name": "orc", "position": cell("orc", -1)["position"]}
        for has_bow, count, enemies, wanted in (
            (True, 7, [], True), (True, 8, [], False), (False, 7, [], False), (True, 7, [foe], False),
        ):
            with self.subTest(bow=has_bow, count=count, enemies=bool(enemies)):
                pack = [{**arrows, "id": "packed", "quantity": count}]
                if has_bow:
                    pack.append(bow)
                turn = player_turn([here, stash], ground=ground, inventory=pack, actors=enemies,
                                   hostiles=[enemy["id"] for enemy in enemies])
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                goals = player.loot_goals(turn, turn.ground_items, returning=True)
                self.assertEqual(goals.weapons, {"stash"} if wanted else set())
                self.assertFalse(goals.recover_arrows)

    def test_arrow_routes_require_a_bow_the_player_can_wield(self):
        bow = {"id": "bow", "name": "bow", "identity": "item.weapon.shortbow",
               "class": "weapon", "quantity": 1, "identified": True}
        mace = {**bow, "id": "mace", "identity": "item.weapon.mace"}
        arrow = {**bow, "id": "arrow", "identity": "item.weapon.arrow"}
        stash = cell("stash", 2)
        slot = {"id": "weapon", "name": "Weapon"}
        for worn, cursed, usable in ((None, "bow", False), ("bow", "bow", True),
                                    ("mace", "mace", False), ("mace", None, True)):
            with self.subTest(worn=worn, cursed=cursed):
                offers = [{"item": item["id"], "slot": slot, "equipped_slot": 0 if item["id"] == worn else None,
                           "known_cursed": item["id"] == cursed} for item in (bow, mace)]
                turn = player_turn([cell("here"), stash], inventory=[bow, mace], interactions={
                    "slots": [slot], "inventory": offers}, ground=[{**stash, "item": arrow, "reachable": False}])
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                self.assertEqual(player.loot_goals(turn, turn.ground_items, returning=False).recover_arrows, usable)
                self.assertEqual(player.loot_goals(turn, turn.ground_items, returning=True).wanted,
                                 {"stash"} if usable else set())

    def test_recovers_reachable_arrows_when_carrying_the_bow(self):
        policy = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        def weapon(name):
            return {"id": name, "name": name, "identity": f"item.weapon.{name}",
                    "identified": True, "quantity": 1, "class": "weapon"}
        ground = {"item": weapon("arrow"), "reachable": True, "position": {"x": 0, "y": 0, "z": 0}}
        turn = player_turn([cell("here", down=True)], inventory=[weapon("shortbow")], ground=[ground])
        self.assertEqual(choose(policy, turn), (Pickup(turn.ground_items[0].item), "take"))

    def test_guard_warning_allows_pickup_once_arrows_are_within_reach_and_the_guard_is_gone(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        here, stash, guard_cell = [cell(str(x), x) for x in range(3)]
        arrow = {"id": "arrows", "name": "arrows", "identity": "item.weapon.arrow",
                 "class": "weapon", "identified": True, "quantity": 4}
        guard = {"id": "guard", "name": "orc", "position": guard_cell["position"]}
        ground = {**stash, "item": arrow, "reachable": False}
        player.observe(player_turn([here, stash, guard_cell], ground=[ground], actors=[guard], hostiles=["guard"]))
        turn = player_turn([here, stash, guard_cell], ground=[{**ground, "reachable": True}], tick=100)
        self.assertEqual(choose(player, turn), (Pickup(turn.ground_items[0].item), "take"))
        self.assertIn(stash["key"], player.supplies_guarded_until)

    def test_collects_nearby_gold_before_a_distant_stack_of_arrows(self):
        bow = {"id": "bow", "name": "bow", "identity": "item.weapon.shortbow",
               "identified": True, "quantity": 1, "class": "weapon"}
        arrow = {**bow, "id": "arrows", "identity": "item.weapon.arrow", "quantity": 4}
        gold = {**bow, "id": "gold", "identity": "item.coin.gold", "class": "coin"}
        for distance, direction, reason in ((6, Direction.WEST, "recover_arrows"),
                                            (7, Direction.EAST, "seek_supplies"),
                                            (12, Direction.EAST, "seek_supplies")):
            with self.subTest(distance=distance):
                cells = [cell(str(i), i) for i in range(-distance, 3)]
                ground = [{**cells[0], "item": arrow, "reachable": False},
                          {**cells[-1], "item": gold, "reachable": False}]
                turn = player_turn(cells, inventory=[bow], ground=ground)
                self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn),
                                 (Move(direction), reason))

    def test_returning_archer_leaves_guarded_arrows_until_the_warning_expires(self):
        hall = [cell(str(x), x, up=x == 0) for x in range(6)]
        bow, arrow = [{"id": name, "name": name, "identity": f"item.weapon.{name}",
                       "class": "weapon", "identified": True, "quantity": 1} for name in ("shortbow", "arrow")]
        relic = {"id": "amulet", "name": AMULET, "identity": DEFAULT_OBJECTIVE_IDENTITY,
                 "class": "amulet", "identified": True, "quantity": 1}
        guard = {"id": "guard", "name": "orc", "position": hall[5]["position"]}
        ground = [{**hall[4], "item": arrow, "reachable": False}]
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
        player.observe(player_turn(hall, ground=ground, actors=[guard], hostiles=["guard"]))
        for tick, expected in ((1199, (Move(Direction.UP), "ascend")), (1200, (Move(Direction.EAST), "seek_item"))):
            with self.subTest(tick=tick):
                turn = player_turn(hall, inventory=[bow, relic], ground=ground, tick=tick)
                self.assertEqual(choose(player, turn), expected)

    def test_optional_loot_detours_respect_guard_warnings_on_the_way_to_a_clear_stash(self):
        hall = [cell("stairs", down=True), cell("passage", 1), cell("stash", 2)]
        bow = {"id": "bow", "name": "bow", "identity": "item.weapon.shortbow",
               "class": "weapon", "identified": True, "quantity": 1}
        for category, identity, reason in (("weapon", "item.weapon.arrow", "recover_arrows"),
                                           ("potion", None, "seek_supplies")):
            item = {"id": "loot", "name": "loot", "class": category, "identity": identity,
                    "identified": identity is not None, "quantity": 1}
            player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
            player.supplies_guarded_until["passage"] = 100
            for tick, expected in ((99, (Move(Direction.DOWN), "descend")), (100, (Move(Direction.EAST), reason))):
                with self.subTest(category=category, tick=tick):
                    turn = player_turn(hall, ground=[{**hall[2], "item": item, "reachable": False}],
                                       inventory=[bow], tick=tick)
                    self.assertEqual(choose(player, turn), expected)

    def test_remembered_arrow_detour_respects_guard_warning_until_expiry(self):
        hall = [cell(str(x), x, down=x == 0) for x in range(6)]
        bow, arrow = [{"id": name, "name": name, "identity": f"item.weapon.{name}",
                       "class": "weapon", "identified": True, "quantity": 1} for name in ("shortbow", "arrow")]
        guard = {"id": "guard", "name": "orc", "position": hall[5]["position"]}
        ground = [{**hall[4], "item": arrow, "reachable": False}]
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(player_turn(hall, inventory=[bow], actors=[guard], hostiles=["guard"], ground=ground, tick=0))
        for tick, expected in ((1199, (Move(Direction.DOWN), "descend")), (1200, (Move(Direction.EAST), "recover_arrows"))):
            with self.subTest(tick=tick):
                turn = player_turn(hall[:3], inventory=[bow], tick=tick)
                self.assertEqual(choose(player, turn), expected)

    def test_collects_visible_spent_arrows_beyond_an_immediate_detour(self):
        cells = [cell(str(i), i, down=i == 0) for i in range(5)]
        bow, arrow = [{"id": name, "name": name, "identity": f"item.weapon.{name}",
                       "identified": True, "quantity": 12, "class": "weapon"} for name in ("shortbow", "arrow")]
        ground = [{**cells[-1], "item": {**arrow, "id": "spent", "quantity": 1}, "reachable": False}]
        ground.append({**cells[0], "item": {**arrow, "id": "unreachable"}, "reachable": False})
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
        for technique in ("basic_melee", "rattling_blow"):
            with self.subTest(technique=technique):
                strike = {**melee, "id": technique, "basic_attack": technique == "basic_melee"}
                turn = player_turn([cell("here"), cell("enemy", 1)], actors=[foe], hostiles=["emu"],
                                   abilities=[strike, throw], inventory=[dart])
                target, = turn.hostiles
                shot = Shot(turn.abilities[1], target, (1, 0, 0), turn.inventory[0], None, 0)
                turn.ranged_attacks = Mock(return_value=[shot])
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual(reason, "melee")
                self.assertEqual((action.ability.identity, action.target), (technique, target))

    def test_uses_a_loaded_launcher_even_if_a_carried_throw_is_offered_first(self):
        from unittest.mock import Mock
        from tor_client_bot import ProjectileRange, Shot

        cells = [cell(str(x), x) for x in range(4)]
        foe = {"id": "emu", "name": "emu", "position": cells[-1]["position"]}
        abilities = [{"id": name, "name": name, "target_kind": "directional_projectile",
                      "reach": {"kind": "projectile"}, "cooldown_remaining": "0", "cost": None,
                      "basic_attack": False} for name in ("throw", "launch")]
        inventory = [{"id": name, "name": name, "class": "weapon", "quantity": 1, "identified": False}
                     for name in ("dart", "arrow", "bow")]
        turn = player_turn(cells, actors=[foe], hostiles=["emu"], abilities=abilities, inventory=inventory)
        dart, arrow, bow = turn.inventory
        target, = turn.hostiles
        throw = Shot(turn.abilities[0], target, (3, 0, 0), dart, None, 0)
        launch = Shot(turn.abilities[1], target, (3, 0, 0), arrow, bow, 0, range=ProjectileRange(3, 6))
        def offers_until_launch(first):
            if first is throw:
                yield throw
            yield launch
            self.fail("A usable short-range launch should end the projectile query")

        for first in (throw, launch):
            with self.subTest(first=first.ability.identity):
                turn.ranged_attacks = Mock(return_value=offers_until_launch(first))
                self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn),
                                 (launch, "launch_projectile"))

    def test_only_a_selected_throw_needs_to_compete_with_melee(self):
        from tor_client_bot import Shot

        here, adjacent = cell("here"), cell("near", 1)
        foe = {"id": "emu", "name": "emu", "position": adjacent["position"]}
        items = [{"id": name, "name": name, "class": "weapon", "quantity": 1, "identified": False}
                 for name in ("dart", "bow")]
        ability = {"id": "shoot", "name": "shoot", "target_kind": "directional_projectile",
                   "basic_attack": False, "reach": {"kind": "projectile"}, "cost": None, "cooldown_remaining": 0}
        turn = player_turn([here, adjacent], actors=[foe], hostiles=["emu"], inventory=items, abilities=[ability])
        target, = turn.hostiles
        dart, bow = turn.inventory
        launch = Shot(turn.abilities[0], target, (1, 0, 0), dart, bow, 0)
        throw = Shot(turn.abilities[0], target, (1, 0, 0), dart, None, 0)
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        for offers, expected in (([], None), ([throw, launch], (launch, "launch_projectile")), ([throw], None)):
            with self.subTest(offers=offers), patch.object(turn, "ranged_attacks", return_value=offers), \
                    patch.object(turn, "attack") as strike:
                self.assertEqual(player.ranged_combat(turn, [target]), expected)
                if offers == [throw]:
                    strike.assert_called_once_with(target, technique="rattling_blow")
                else:
                    strike.assert_not_called()

    def test_nearby_bystander_does_not_trigger_projectile_queries_for_a_distant_foe(self):
        from unittest.mock import patch

        cells = [cell(str(x), x) for x in range(10)]
        actors = [{"id": name, "name": name, "position": cells[x]["position"]}
                  for name, x in (("friend", 2), ("foe", 9))]
        ability = {"id": "shoot", "name": "shoot", "target_kind": "directional_projectile", "basic_attack": False,
                   "reach": {"kind": "projectile"}, "cooldown_remaining": "0", "cost": None}
        turn = player_turn(cells, actors=actors, hostiles=["foe"], abilities=[ability])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        with patch.object(turn, "_projectile_pages", return_value=()) as query:
            self.assertIsNone(player.ranged_combat(turn, []))
            query.assert_not_called()

    def test_projectile_selection_orders_delivery_then_range_and_preserves_ties(self):
        from dataclasses import replace
        from itertools import combinations
        from tor_client_bot import ProjectileRange, Shot

        near = cell("near", 2)
        far = {**cell("far"), "position": {"x": 1, "y": 0, "z": -4}}
        cells = [cell("here"), cell("lane", 1), near, far]
        foes = [{"id": tile["key"], "name": tile["key"], "position": tile["position"]} for tile in (far, near)]
        ability = {"id": "shoot", "name": "shoot", "target_kind": "directional_projectile",
                   "reach": {"kind": "projectile"}, "cooldown_remaining": "0", "basic_attack": False, "cost": None}
        pack = [{"id": name, "name": name, "class": "weapon", "quantity": 1, "identified": False} for name in ("dart", "bow")]
        turn = player_turn(cells, actors=foes, hostiles=["far", "near"], abilities=[ability], inventory=pack)
        dart, bow = turn.inventory
        targets = {foe.target: foe for foe in turn.hostiles}
        near_foe, far_foe = targets["near"], targets["far"]
        short = Shot(turn.abilities[0], near_foe, near_foe.cells[0], dart, bow, 0, range=ProjectileRange(2, 4))
        long = replace(short, target=far_foe, offset=far_foe.cells[0])
        unknown = replace(long, range=None)

        # List the policy preferences from best to worst. Each pair must work in either offer order.
        preferences = [
            ("short launch", short),
            ("unknown launch", unknown),
            ("long launch", long),
            ("short throw", replace(short, launcher=None)),
            ("unknown throw", replace(unknown, launcher=None)),
            ("long throw", replace(long, launcher=None)),
        ]

        def assert_selection(offers, expected):
            with patch.object(turn, "ranged_attacks", return_value=iter(offers)):
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
            self.assertIs(action, expected)
            self.assertEqual(reason, "launch_projectile" if expected.launcher else "throw_projectile")

        for (preferred_name, preferred), (other_name, other) in combinations(preferences, 2):
            with self.subTest(preferred=preferred_name, other=other_name):
                assert_selection([other, preferred], preferred)
                assert_selection([preferred, other], preferred)

        for name, shot in preferences:
            with self.subTest(equal_priority=name):
                alternative = replace(shot, socket=1)
                assert_selection([shot], shot)
                assert_selection([shot, alternative], shot)
                assert_selection([alternative, shot], alternative)

    def test_throw_range_ends_at_four_and_an_admitted_throw_waits_for_time_to_advance(self):
        from unittest.mock import Mock
        from tor_client_bot import Shot

        dart = {"id": "d", "name": "dart", "identity": "item.weapon.dart", "quantity": 2, "identified": True, "class": "weapon"}
        throw = {"id": "throw", "name": "Throw", "target_kind": "directional_projectile", "reach": {"kind": "projectile"},
                 "cooldown_remaining": "0", "cost": None, "basic_attack": False}
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        cases = (
            ((5,), 5, 10, False),
            ((4, 5), 5, 10, False),
            ((4, 5), 4, 10, True),
            ((4,), 4, 10, False),
            ((4,), 4, 9, False),
            ((4,), 4, 11, True),
        )
        for body, distance, tick, expected in cases:
            with self.subTest(body=body, aim=distance, tick=tick):
                cells = [cell(str(x), x) for x in range(max(body) + 1)]
                actors = [{"id": "emu", "name": "emu", "position": cells[x]["position"]} for x in body]
                turn = player_turn(cells, actors=actors, hostiles=["emu"], inventory=[dart], abilities=[throw], tick=tick)
                shot = Shot(turn.abilities[0], turn.hostiles[0], (distance, 0, 0), turn.inventory[0], None, 0)
                turn.ranged_attacks = Mock(return_value=[shot])
                action, reason = choose(player, turn)
                self.assertEqual(reason == "throw_projectile", expected)
                if expected:
                    self.assertEqual(action, shot)
                    player.admitted(turn, action, reason=reason)

    def test_distant_foe_prompts_the_best_weapon_the_player_can_equip(self):
        inventory = [{"id": name, "name": name, "identity": f"item.weapon.{name}", "class": "weapon",
                      "identified": True, "quantity": 1} for name in ("shortbow", "mace", "arrow")]
        slot = {"id": "weapon", "name": "Weapon"}
        foe = {"id": "enemy", "name": "orc", "position": cell("enemy", 5)["position"]}
        for offered, expected, target in (
            (["mace"], "equip_melee", "mace"),
            (["mace", "shortbow"], "equip_bow", "shortbow"),
            (["shortbow", "mace"], "equip_bow", "shortbow"),
        ):
            with self.subTest(offered=offered):
                turn = player_turn([cell("here")], actors=[foe], hostiles=["enemy"], inventory=inventory,
                                   interactions={"slots": [slot], "inventory": [
                                       {"item": name, "slot": slot} for name in offered]})
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual(reason, expected)
                self.assertEqual(action.item.target, target)

    def test_nearest_foe_sets_the_range_for_drawing_a_bow(self):
        inventory = [{"id": name, "name": name, "identity": f"item.weapon.{name}",
                      "identified": True, "quantity": 1} for name in ("shortbow", "arrow")]
        slot = {"id": "weapon", "name": "Weapon"}
        for nearest, expected in ((4, None), (5, "equip_bow")):
            with self.subTest(nearest=nearest):
                foes = [{"id": str(x), "name": "orc", "position": cell(str(x), x)["position"]}
                        for x in (8, nearest)]
                turn = player_turn([cell("here")], actors=foes, hostiles=[foe["id"] for foe in foes],
                                   inventory=inventory, interactions={"slots": [slot],
                                   "inventory": [{"item": "shortbow", "slot": slot}]})
                decision = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY).ranged_combat(turn, [])
                self.assertEqual(decision[1] if decision else None, expected)

    def test_loose_arrows_without_a_bow_allow_melee_preparation(self):
        slot = {"id": "weapon", "name": "Weapon"}
        inventory = [{"id": name, "name": name, "identity": f"item.weapon.{name}", "class": "weapon",
                      "quantity": 1, "identified": True} for name in ("mace", "arrow")]
        foe = {"id": "foe", "name": "orc", "position": cell("near", 3)["position"]}
        turn = player_turn([cell("here")], actors=[foe], hostiles=["foe"], inventory=inventory,
                           interactions={"slots": [slot], "inventory": [{"item": "mace", "slot": slot}]})
        action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
        self.assertEqual(reason, "equip_melee")
        self.assertEqual(action.item.target, "mace")

    def test_unoffered_bow_does_not_block_a_melee_weapon_at_medium_range(self):
        slot = {"id": "weapon", "name": "Weapon"}
        inventory = [{"id": name, "name": name, "identity": f"item.weapon.{name}", "class": "weapon",
                      "quantity": 1, "identified": True} for name in ("shortbow", "mace", "arrow")]
        foe = {"id": "foe", "name": "orc", "position": cell("near", 3)["position"]}
        turn = player_turn([cell("here")], actors=[foe], hostiles=["foe"], inventory=inventory,
                           interactions={"slots": [slot], "inventory": [{"item": "mace", "slot": slot}]})
        action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
        self.assertEqual(reason, "equip_melee")
        self.assertEqual(action.item.target, "mace")

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

    def test_removes_a_weapon_curse_in_safety_to_equip_an_available_upgrade(self):
        from tor_client_bot import ItemUse, UseItem

        slot = {"id": "weapon", "name": "Weapon"}
        bow = {"id": "bow", "identity": "item.weapon.shortbow", "class": "weapon",
               "name": "bow", "identified": True, "quantity": 1}
        mace = {**bow, "id": "mace", "name": "mace", "identity": "item.weapon.mace"}
        scroll = {**bow, "id": "cure", "class": "scroll", "identity": "item.scroll.remove-curse"}
        foe = {"id": "snake", "name": "snake", "position": cell("enemy", 3)["position"]}
        for upgrade, hostiles, readable, use_scroll in (
            (True, [], True, True), (False, [], True, False), (True, [foe], True, False), (True, [], False, False),
        ):
            with self.subTest(upgrade=upgrade, in_combat=bool(hostiles), readable=readable):
                offers = [{"item": "bow", "slot": slot, "equipped_slot": 0}, {"item": "cure", "readable": readable}]
                if upgrade:
                    offers.append({"item": "mace", "slot": slot})
                turn = player_turn([cell("here", down=True)], inventory=[bow, mace, scroll], actors=hostiles,
                                   hostiles=[enemy["id"] for enemy in hostiles],
                                   interactions={"slots": [slot], "inventory": offers, "curse_blocked": ["bow"]})
                decision = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                if use_scroll:
                    self.assertEqual(decision, (UseItem(turn.inventory[2], ItemUse.READ), "remove_weapon_curse"))
                else:
                    self.assertNotEqual(decision[1], "remove_weapon_curse")

    def test_readies_a_melee_weapon_when_a_foe_closes_even_with_arrows_left(self):
        from tor_client_bot import Unequip

        inventory = [{"id": name, "name": name, "identity": f"item.weapon.{name}",
                      "identified": True, "quantity": 1} for name in ("shortbow", "mace", "arrow")]
        slot = {"id": "weapon", "name": "Weapon"}
        offers = [{"item": "shortbow", "slot": slot, "equipped_slot": 0},
                  {"item": "mace", "slot": slot, "equipped_slot": None}]
        body = {"id": "player", "name": "you", "position": cell("body", 1)["position"]}
        for distance, extra_body, switch in ((2, [], True), (3, [], False), (3, [body], True)):
            with self.subTest(distance=distance, extended=bool(extra_body)):
                foe = {"id": "snake", "name": "snake", "position": cell("snake", distance)["position"]}
                turn = player_turn([cell("here"), cell("body", 1)], actors=[foe, *extra_body],
                                   hostiles=["snake"], inventory=inventory,
                                   interactions={"slots": [slot], "inventory": offers})
                decision = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual(decision == (Unequip(turn.equipment[0]), "change_weapon"), switch)

    def test_retreats_when_a_known_drainer_closes_on_any_part_of_the_player(self):
        cells = [cell("stairs", up=True), cell("body", 1), cell("hall", 2), cell("snake", 3)]
        snake = {"id": "snake", "name": "rattlesnake", "position": cells[3]["position"]}
        body = {"id": "player", "name": "you", "position": cells[1]["position"]}
        for actors, should_retreat in (([snake], False), ([body, snake], True)):
            with self.subTest(should_retreat=should_retreat):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
                player.opponents.draining_names.add("rattlesnake")
                turn = player_turn(cells, actors=actors, hostiles=["snake"])
                decision = choose(player, turn)
                self.assertEqual(decision == (Move(Direction.UP), "retreat_upstairs"), should_retreat)

    def test_stair_retreat_does_not_reverse_into_the_same_pursuit(self):
        snake = {"id": "snake", "name": "rattlesnake", "position": {"x": 2, "y": 0, "z": 0}}
        policy = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=6)
        policy.opponents.draining_names.add("rattlesnake")
        first = player_turn([cell("origin"), cell("detour", 1), cell("stairs", -1, up=True)],
                            actors=[snake], hostiles=["snake"])
        policy.observe(first)
        policy.admitted(first, Move(Direction.EAST), reason="retreat_to_stairs")
        later = player_turn([cell("detour"), cell("origin", -1), cell("stairs", -2, up=True)],
                            actors=[snake], hostiles=["snake"], tick=100)
        policy.observe(later)
        self.assertNotEqual(policy.combat(later), (Move(Direction.WEST), "retreat_to_stairs"))

    def test_retreat_prefers_adjacent_stairs_outside_melee_contact(self):
        north = cell("north")
        north["position"]["y"] = -1
        exits = [cell("west", -1, down=True), cell("east", 1, down=True)]
        for x, escape in ((-1, Direction.EAST), (1, Direction.WEST)):
            with self.subTest(enemy_x=x):
                enemy = cell("enemy", x)
                enemy["position"]["y"] = -1
                foe = {"id": "foe", "name": "orc", "position": enemy["position"]}
                turn = player_turn([cell("here"), north, enemy, *exits], actors=[foe], hostiles=["foe"],
                                   combat={"hp": 8, "max_hp": 20})
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.opponents.damage_by_target["foe"] = 2
                self.assertEqual(choose(player, turn), (Move(escape), "retreat_to_stairs"))

    def test_adjacent_stair_assessment_reports_the_trapped_exit_alongside_the_safe_move(self):
        from rogue_combat import OpponentHistory
        from rogue_retreat import assess_adjacent_retreat_stairs

        pit = {"id": "pit", "family": "pit", "reachable": True}
        cells = [cell("here"), cell("clear", -1, down=True), {**cell("trapped", 1, down=True), "trap": pit}]
        turn = player_turn(cells)
        step, rejected = assess_adjacent_retreat_stairs(turn, {"clear", "trapped"}, OpponentHistory())
        self.assertEqual(step, Move(Direction.WEST))
        self.assertEqual(rejected, {"trapped"})

    def test_unguarded_adjacent_retreat_stairs_favor_more_distance_from_pursuit(self):
        cells = [cell(str(x), x, down=abs(x) == 1) for x in range(-3, 4)]
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
        for foe_x, escape in ((-3, Direction.EAST), (3, Direction.WEST)):
            with self.subTest(foe_x=foe_x):
                foe = {"id": "foe", "name": "orc", "position": cell("enemy", foe_x)["position"]}
                turn = player_turn(cells, actors=[foe], hostiles=["foe"])
                player.observe(turn)
                self.assertEqual(player.retreat_to_stairs(turn, pressed=True), (Move(escape), "retreat_to_stairs"))

    def test_stair_exposure_uses_combined_damage_at_the_destination(self):
        cells = [cell("west", -1, down=True), cell("here"), cell("east", 1)]
        foes = []
        for x in (-1, 0, 1):
            tile = cell(f"north-{x}", x)
            tile["position"]["y"] = -1
            cells.append(tile)
            foes.append({"id": str(x), "name": "orc", "position": tile["position"]})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.opponents.damage_by_target.update({"-1": 1, "0": 2, "1": 5})
        for hp, expected in ((6, None), (7, Move(Direction.WEST))):
            with self.subTest(hp=hp):
                turn = player_turn(cells, actors=foes, hostiles=["-1", "0", "1"], combat={"hp": hp, "max_hp": 10})
                self.assertEqual(player.step_onto_retreat_stair(turn, {"west"}), expected)

    def test_retreat_chooses_lower_observed_damage_when_both_stairs_are_guarded(self):
        cells = [cell("here"), cell("west", -1, down=True), cell("east", 1, down=True)]
        foes = []
        for x in (-1, 0, 1):
            tile = cell(f"north-{x}", x)
            tile["position"]["y"] = -1
            cells.append(tile)
            if x:
                foes.append({"id": str(x), "name": "orc", "position": tile["position"]})
        turn = player_turn(cells, actors=foes, hostiles=["-1", "1"], combat={"hp": 8, "max_hp": 20})
        for safer, direction in (("-1", Direction.WEST), ("1", Direction.EAST)):
            with self.subTest(safer=safer):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.opponents.damage_by_target.update({"-1": 3, "1": 3, safer: 1})
                self.assertEqual(choose(player, turn), (Move(direction), "retreat_to_stairs"))

    def test_steps_onto_escape_stairs_before_two_open_approaches_reach_melee(self):
        for blocked, expected in ((False, "retreat_to_stairs"), (True, "combat_haste")):
            with self.subTest(blocked=blocked):
                cells = [cell("here"), cell("exit", 1, down=True), cell("north", y=-1),
                         cell("south", y=1, wall=blocked), cell("orc", y=-2), cell("bat", y=2)]
                foes = [{"id": tile["key"], "name": tile["key"], "position": tile["position"]} for tile in cells[-2:]]
                haste = {"id": "haste", "identity": "item.potion.haste", "class": "potion", "quantity": 1,
                         "identified": True, "name": "haste"}
                turn = player_turn(cells, actors=foes, hostiles=["orc", "bat"], inventory=[haste],
                                   interactions={"inventory": [{"item": "haste", "drinkable": True}]})
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                action, reason = choose(player, turn)
                self.assertEqual(reason, expected)
                if not blocked:
                    self.assertEqual(action, Move(Direction.EAST))

    def test_two_pursuers_sharing_one_doorway_need_a_second_open_approach(self):
        from rogue_retreat import converging_pursuers

        for west_open in (False, True):
            with self.subTest(west_open=west_open):
                cells = [cell("here"), cell("west", -1, wall=not west_open)]
                cells.extend(cell(f"{x},{y}", x, y) for x in (-1, 0, 1) for y in (-2, -1))
                foes = [{"id": str(x), "name": "orc", "position": {"x": x, "y": -2, "z": 0}} for x in (-1, 1)]
                turn = player_turn(cells, actors=foes, hostiles=["-1", "1"])
                distances = [(foe, turn.distance_to(foe)) for foe in turn.hostiles]
                self.assertEqual(converging_pursuers(turn, distances), west_open)
                self.assertFalse(converging_pursuers(turn, distances[:1]))

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
        policy.opponents.draining_names.add("rattlesnake")
        snake = {"id": "snake", "name": "rattlesnake", "position": {"x": 1, "y": 0, "z": 0}}
        turn = player_turn([cell("escape", down=True), cell("away", -1), cell("snake", 1)],
                           actors=[snake], hostiles=["snake"])
        self.assertEqual(choose(policy, turn), (Move(Direction.DOWN), "retreat_downstairs"))

    def test_first_floor_retreat_routes_to_distant_upstairs_when_carrying_the_amulet(self):
        rooms = [cell(str(x), x, up=x == 2) for x in range(-1, 3)]
        foe = {"id": "foe", "name": "orc", "position": rooms[0]["position"]}
        relic = {"id": "amulet", "name": AMULET, "identity": DEFAULT_OBJECTIVE_IDENTITY,
                 "identified": True, "quantity": 1}
        for pack, expected in (([], None), ([relic], (Move(Direction.EAST), "retreat_to_stairs"))):
            with self.subTest(carrying=bool(pack)):
                turn = player_turn(rooms, actors=[foe], hostiles=["foe"], inventory=pack)
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observe(turn)
                self.assertEqual(player.retreat_to_stairs(turn), expected)

    def test_carrier_routes_to_far_stairs_without_entering_contact_at_the_near_exit(self):
        cells = [cell(str(x), x, up=x in (-2, 1)) for x in range(-2, 3)]
        guard = {"id": "guard", "name": "orc", "position": cells[-1]["position"]}
        relic = {"id": "amulet", "name": AMULET, "identity": DEFAULT_OBJECTIVE_IDENTITY,
                 "identified": True, "quantity": 1}
        turn = player_turn(cells, actors=[guard], hostiles=["guard"], inventory=[relic])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        self.assertEqual(choose(player, turn), (Move(Direction.WEST), "retreat_to_stairs"))

    def test_outbound_retreat_keeps_rejected_downstairs_out_of_the_longer_route(self):
        cells = [cell(str(x), x, up=x == -2, down=x == 1) for x in range(-2, 3)]
        guard = {"id": "guard", "name": "griffin", "position": cells[-1]["position"]}
        turn = player_turn(cells, actors=[guard], hostiles=["guard"], combat={"hp": 4, "max_hp": 20})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
        self.assertEqual(choose(player, turn), (Move(Direction.WEST), "retreat_to_stairs"))

    def test_retreat_routes_to_the_farther_stair_until_the_nearer_one_is_safe(self):
        corridor = [cell(str(x), x, up=x in (-2, 3)) for x in range(-2, 4)]
        dragon = {"id": "foe", "name": "dragon", "position": {"x": 0, "y": 2, "z": 0}}
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
        player.unsafe_stairs["-2", Direction.UP] = 100
        for tick, direction in ((99, Direction.EAST), (100, Direction.WEST)):
            with self.subTest(tick=tick):
                turn = player_turn(corridor, actors=[dragon], hostiles=["foe"], tick=tick)
                player.observe(turn)
                self.assertEqual(player.retreat_to_stairs(turn), (Move(direction), "retreat_to_stairs"))

    def test_recontests_a_guarded_landing_only_with_a_health_margin(self):
        snake = {"id": "snake", "name": "rattlesnake", "position": cell("snake", 1)["position"]}
        strike = {"id": "strike", "name": "Strike", "cooldown_remaining": 0, "cost": None,
                  "basic_attack": True, "reach": {"kind": "melee"}, "target_kind": "actor"}
        rattle = {**strike, "id": "rattling_blow", "basic_attack": False}
        cases = [
            (21, 5, "snake", [strike, rattle], "rattling_blow"),  # Exactly three quarters of maximum health.
            (20, 5, "snake", [strike], "retreat_upstairs"),
            (24, 6, "snake", [strike], "retreat_upstairs"),  # Four estimated hits would be lethal.
            (24, 5, "snake", [strike, rattle], "rattling_blow"),
            (24, 5, "snake", [strike], "strike"),
            (24, 0, "snake", [strike], "retreat_upstairs"),
            (24, 5, "other", [strike], "retreat_upstairs"),
        ]
        for hp, damage, guard, abilities, expected in cases:
            player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
            player.opponents.draining_names.add("rattlesnake")
            player.opponents.damage_by_target["snake"] = damage
            player.last_guarded_landing = "landing", guard
            turn = player_turn([cell("landing", up=True), cell("snake", 1)], actors=[snake], hostiles=["snake"],
                               abilities=abilities, combat={"hp": str(hp), "max_hp": "28"})
            with self.subTest(hp=hp, damage=damage, guard=guard):
                action, reason = choose(player, turn)
                self.assertEqual(action.ability.identity if reason == "melee" else reason, expected)

    def test_levitating_bot_can_break_contact_with_a_remembered_stair_guard(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
        player.last_guarded_landing = "landing", "guard"
        player.opponents.draining_targets.add("guard")
        player.opponents.damage_by_target["guard"] = 1
        tiles = [cell("landing", up=True), cell("away", -1), cell("guard", 1)]
        guard = {"id": "guard", "name": "orc", "position": tiles[-1]["position"]}
        strike = {"id": "strike", "name": "Strike", "basic_attack": True, "reach": {"kind": "melee"},
                  "target_kind": "actor", "cost": None, "cooldown_remaining": 0}
        floating = {"condition": {"id": "rogue.levitation", "name": "Levitation"},
                    "phase": "active", "remaining_ticks": 100}
        turn = player_turn(tiles, actors=[guard], hostiles=["guard"], abilities=[strike],
                           combat={"hp": 24, "max_hp": 28}, stats={"conditions": [floating]})
        self.assertEqual(choose(player, turn), (Move(Direction.WEST), "retreat"))

    def test_explores_past_a_landing_when_a_known_drainer_is_distant(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
        player.opponents.draining_names.add("rattlesnake")
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
        player.opponents.draining_names.add("rattlesnake")
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

    def test_carries_the_amulet_past_an_ordinary_enemy_to_nearby_upstairs(self):
        amulet = {"id": "relic", "name": AMULET, "class": "amulet", "quantity": 1,
                  "identified": True, "identity": DEFAULT_OBJECTIVE_IDENTITY}
        foe = {"id": "orc", "name": "orc", "position": cell("orc", -1)["position"]}
        strike = {"id": "strike", "name": "Strike", "cooldown_remaining": 0, "cost": None,
                  "basic_attack": True, "reach": {"kind": "melee"}, "target_kind": "actor"}
        for here, exit_cell, expected in [
            (cell("here", up=True), cell("east", 1), (Move(Direction.UP), "retreat_upstairs")),
            (cell("here"), cell("east", 1, up=True), (Move(Direction.EAST), "retreat_to_stairs")),
        ]:
            turn = player_turn([here, cell("west", -1), exit_cell], actors=[foe], hostiles=["orc"],
                               inventory=[amulet], abilities=[strike])
            self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn), expected)

    def test_follows_the_last_amulet_sighting_until_its_cell_is_seen_empty(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=26)
        passage = [cell("here"), cell("west", -1), cell("west-end", -2), cell("east", 1)]
        alcove = cell("alcove", 2)
        amulet = {"id": "relic", "name": AMULET, "class": "amulet", "quantity": 1,
                  "identified": True, "identity": DEFAULT_OBJECTIVE_IDENTITY}
        ration = {"id": "food", "name": "ration", "class": "food", "quantity": 1, "identified": False}
        food = {**passage[2], "item": ration, "reachable": False}
        sighting = {**alcove, "item": amulet, "reachable": False}
        player.observe(player_turn([*passage, alcove], ground=[sighting]))
        hidden = player_turn(passage, ground=[food])
        self.assertEqual(choose(player, hidden), (Move(Direction.EAST), "seek_objective"))
        empty = player_turn([*passage, alcove], ground=[food])
        self.assertEqual(choose(player, empty), (Move(Direction.WEST), "seek_item"))

    def test_objective_route_skirts_a_recently_guarded_supply_until_its_warning_expires(self):
        room = [cell("here"), cell("guarded", 1), cell("goal", 2)]
        for x in range(3):
            row = cell(f"detour-{x}", x)
            row["position"]["y"] = 1
            room.append(row)
        relic = {"id": "relic", "name": AMULET, "class": "amulet", "quantity": 1,
                 "identified": True, "identity": DEFAULT_OBJECTIVE_IDENTITY}
        ground = [{**room[2], "item": relic, "reachable": False}]
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.supplies_guarded_until["guarded"] = 100
        for tick, tiles, expected in ((98, room[:3], Direction.EAST), (99, room, Direction.SOUTH_EAST),
                                      (100, room, Direction.EAST)):
            with self.subTest(tick=tick):
                turn = player_turn(tiles, ground=ground, tick=tick)
                self.assertEqual(choose(player, turn), (Move(expected), "seek_objective"))

    def test_objective_route_prefers_a_past_guard_to_a_known_trap(self):
        tiles = [cell("here"), cell("guarded", 1), cell("goal", 2)]
        for x in range(3):
            detour = cell(f"detour-{x}", x)
            detour["position"]["y"] = 1
            if x == 1:
                detour["trap"] = {"id": "pit", "family": None, "reachable": True}
            tiles.append(detour)
        relic = {"id": "relic", "name": AMULET, "class": "amulet", "quantity": 1,
                 "identified": True, "identity": DEFAULT_OBJECTIVE_IDENTITY}
        turn = player_turn(tiles, ground=[{**tiles[2], "item": relic, "reachable": False}])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.supplies_guarded_until["guarded"] = 100
        self.assertEqual(choose(player, turn), (Move(Direction.EAST), "seek_objective"))

    def test_guarded_supply_fallback_keeps_combat_sites_excluded_from_objective_pursuit(self):
        corridor = [cell("here"), cell("guarded", 1), cell("combat", 2), cell("goal", 3)]
        relic = {"id": "relic", "name": AMULET, "class": "amulet", "quantity": 1,
                 "identified": True, "identity": DEFAULT_OBJECTIVE_IDENTITY}
        turn = player_turn(corridor, ground=[{**corridor[-1], "item": relic, "reachable": False}])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        player.supplies_guarded_until["guarded"] = 100
        player.danger_until["combat"] = 200
        self.assertIsNone(player.step_toward_objective(turn, player.maps[player.depth]))

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

    def test_damage_estimate_requires_one_distinct_attacker_with_an_applied_hit(self):
        room = [cell("here"), cell("west", -1), cell("east", 1)]
        foes = [{"id": who, "name": who, "position": tile["position"]}
                for who, tile in zip(("orc", "bat"), room[1:])]
        for event_type, other, outcome, expected in (
            ("ability", "orc", "applied", {"orc": 6}),
            ("ability", "bat", "miss", {"orc": 6}),
            ("ability", "bat", "applied", {}),
            ("unseen_ability", None, "applied", {}),
            ("unseen_ability", None, "miss", {"orc": 6}),
        ):
            with self.subTest(event_type=event_type, other=other, outcome=outcome):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observe(player_turn(room, actors=foes, hostiles=["orc", "bat"]))
                hit = {"type": "ability", "target": "player", "caster": "orc", "outcome": "applied"}
                other_event = {"type": event_type, "target": "player", "outcome": outcome}
                if other is not None:
                    other_event["caster"] = other
                player.observe(player_turn(room, actors=foes, hostiles=["orc", "bat"],
                                           combat={"hp": 4, "events": [hit, other_event]}))
                self.assertEqual(dict(player.opponents.damage_by_target), expected)
                self.assertEqual(dict(player.opponents.damage_by_name), expected)

    def test_damage_from_an_out_of_sight_attacker_stays_specific_to_its_target(self):
        room = [cell("here"), cell("east", 1)]
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(player_turn(room))
        hit = {"type": "unseen_ability", "target": "player", "caster": "hunter", "outcome": "applied"}
        player.observe(player_turn(room, combat={"hp": 4, "events": [hit]}))
        for target, expected in (("hunter", 6), ("other", 0)):
            with self.subTest(target=target):
                foe = {"id": target, "name": "orc", "position": room[1]["position"]}
                sighting = player_turn(room, actors=[foe], hostiles=[target])
                self.assertEqual(player.opponents.damage_estimate(sighting.hostiles[0]), expected)

    def test_damage_learning_excludes_a_reflected_hit_in_the_same_interval(self):
        room = [cell("here"), cell("east", 1)]
        foe = {"id": "hunter", "name": "orc", "position": room[1]["position"]}
        for target, outcome, expected in (("player", "applied", {}), ("player", "miss", {"hunter": 6}),
                                          ("hunter", "applied", {"hunter": 6})):
            with self.subTest(target=target, outcome=outcome):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observe(player_turn(room, actors=[foe], hostiles=["hunter"]))
                events = [
                    {"type": "ability", "target": "player", "caster": "hunter", "outcome": "applied"},
                    {"type": "ability", "target": target, "caster": "player", "outcome": outcome,
                     "ability": {"id": "rogue.wand_lightning"}},
                ]
                player.observe(player_turn(room, actors=[foe], hostiles=["hunter"], combat={"hp": 4, "events": events}))
                self.assertEqual(dict(player.opponents.damage_by_target), expected)
                self.assertEqual(player.opponents.damage_by_name.get("orc"), expected.get("hunter"))

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
        ground = [{**stairs[2], "item": scroll, "reachable": False}]
        for elapsed, expected in ((1199, (Move(Direction.DOWN), "descend")),
                                  (1200, (Move(Direction.EAST), "seek_supplies"))):
            with self.subTest(elapsed=elapsed):
                later = player_turn(stairs, ground=ground, tick=turn.tick + elapsed)
                self.assertEqual(choose(player, later), expected)

    def test_recent_guard_suppresses_weapon_and_coin_detours_until_expiry(self):
        hall = [cell("here"), cell("stash", 1), cell("guard", 2)]
        guard = {"id": "guard", "name": "orc", "position": hall[2]["position"]}
        for category, identity in (("weapon", "item.weapon.dagger"), ("weapon", "item.weapon.dart"),
                                   ("weapon", "item.weapon.spear"), ("coin", "item.coin.gold")):
            with self.subTest(identity=identity):
                item = {"id": "loot", "class": category, "identity": identity, "name": identity,
                        "quantity": 1, "identified": True}
                ground = [{**hall[1], "item": item, "reachable": False}]
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observe(player_turn(hall, actors=[guard], hostiles=["guard"], ground=ground, tick=0))
                for tick in (1199, 1200):
                    turn = player_turn(hall, ground=ground, tick=tick)
                    player.observe(turn)
                    goals = player.loot_goals(turn, turn.ground_items, returning=False)
                    weapon_detour = identity in {"item.weapon.dart", "item.weapon.spear"}
                    self.assertEqual("stash" in goals.weapons, tick == 1200 and weapon_detour)
                    self.assertEqual("stash" in goals.nearby, tick == 1200)
                    melee_supply = identity in {"item.weapon.dagger", "item.weapon.spear"}
                    self.assertEqual("stash" in goals.supplies, tick == 1200 and melee_supply)

    def test_repeat_guard_sighting_refreshes_local_danger_without_extending_a_stair_warning(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        room = [cell("here"), cell("scroll", 1), cell("guard", 2)]
        scroll = {"id": "roll", "class": "scroll", "name": "scroll", "quantity": 1, "identified": False}
        guard = {"id": "guard", "name": "orc", "position": room[2]["position"]}
        stair = ("landing", Direction.DOWN)
        player.unsafe_stairs[stair] = 1200
        for tick in (0, 100):
            player.observe(player_turn(room, tick=tick, actors=[guard], hostiles=["guard"],
                                       ground=[{**room[1], "item": scroll, "reachable": False}]))
        player.observe(player_turn(room, tick=1200))
        self.assertNotIn(stair, player.unsafe_stairs)
        self.assertEqual(player.danger_until, {"here": 1300})
        self.assertEqual(player.supplies_guarded_until, {"scroll": 1300})
        player.observe(player_turn(room, tick=1300))
        self.assertFalse(player.danger_until)
        self.assertFalse(player.supplies_guarded_until)

    def test_guarded_scroll_uses_its_own_position_and_every_cell_of_the_guard(self):
        room = [cell(str(x), x, down=x == 0) for x in range(5)]
        scroll = {"id": "roll", "name": "scroll", "class": "scroll", "quantity": 1, "identified": False}
        ground = [{**room[2], "item": scroll, "reachable": False}]
        body = {"id": "player", "name": "you", "position": room[1]["position"]}
        for guard_positions, expected in (((4,), "seek_supplies"), ((4, 3), "descend")):
            with self.subTest(guard_positions=guard_positions):
                guard = [{"id": "guard", "name": "orc", "position": room[x]["position"]} for x in guard_positions]
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observe(player_turn(room, actors=[body, *guard], hostiles=["guard"], ground=ground))
                later = player_turn(room, actors=[body], ground=ground, tick=1)
                self.assertEqual(choose(player, later)[1], expected)

    def test_prefers_a_known_teleport_to_another_step_in_a_lethal_pursuit(self):
        from tor_client_bot import ItemUse, UseItem

        foe = {"id": "hunter", "name": "troll", "position": {"x": 1, "y": 0, "z": 0}}
        scroll = {"id": "escape", "name": "renamed scroll", "identity": "item.scroll.teleportation",
                  "class": "scroll", "identified": True, "quantity": 1}
        turn = player_turn([cell("here"), cell("away", -1), cell("foe", 1)], actors=[foe],
                           hostiles=["hunter"], inventory=[scroll], combat={"hp": 17, "max_hp": 59},
                           interactions={"inventory": [{"item": "escape", "readable": True}]})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.opponents.damage_by_target["hunter"] = 17
        self.assertEqual(choose(player, turn), (UseItem(turn.inventory[0], ItemUse.READ), "escape_pursuit"))

    def test_weak_player_chooses_between_food_and_escape_at_the_lethal_damage_threshold(self):
        from tor_client_bot import ItemUse, UseItem

        foe = {"id": "hunter", "name": "troll", "position": cell("east", 1)["position"]}
        food = {"id": "meal", "name": "ration", "class": "food", "identified": False, "quantity": 1}
        scroll = {"id": "escape", "name": "escape", "class": "scroll", "identified": True, "quantity": 1,
                  "identity": "item.scroll.teleportation"}
        turn = player_turn([cell("here"), cell("east", 1)], actors=[foe], hostiles=["hunter"],
                           inventory=[food, scroll], combat={"hp": 10, "max_hp": 10},
                           stats={"nutrition": {"hunger": "weak"}},
                           interactions={"inventory": [{"item": "meal", "edible": True},
                                                       {"item": "escape", "readable": True}]})
        meal, escape = turn.inventory
        for damage, expected in ((9, (UseItem(meal, ItemUse.EAT), "eat")),
                                 (10, (UseItem(escape, ItemUse.READ), "escape_pursuit"))):
            with self.subTest(damage=damage):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.opponents.damage_by_target["hunter"] = damage
                self.assertEqual(choose(player, turn), expected)

    def test_displaces_a_lethal_contact_before_retreating_to_available_stairs(self):
        from tor_client_bot import Zap

        foe = {"id": "fang", "name": "rattlesnake", "position": {"x": 1, "y": 0, "z": 0}}
        item = {"id": "staff", "class": "wand", "name": "displacing staff", "quantity": 1,
                "identified": True, "identity": "item.wand.wand-teleport-away"}
        offer = {"item": "staff", "zappable": True, "device_activation": "direction", "charges": None}
        turn = player_turn([cell("here"), cell("snake", 1), cell("stairs", -1, down=True)], actors=[foe], hostiles=["fang"],
                           inventory=[item], combat={"hp": 5, "max_hp": 39},
                           interactions={"inventory": [offer]})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.opponents.damage_by_target["fang"] = 7
        action, reason = choose(player, turn)
        self.assertEqual((action, reason), (Zap(turn.devices[0], turn.cell_at((1, 0, 0))), "displace_threat"))
        player.admitted(turn, action, reason=reason)
        self.assertEqual(player.choose(turn), (Move(Direction.WEST), "retreat_to_stairs"))

    def test_displaces_the_most_dangerous_contact_with_a_disclosed_target_cell(self):
        from tor_client_bot import Zap

        foes = [{"id": name, "name": name, "position": cell(name, x)["position"]}
                for name, x in (("east", 1), ("west", -1))]
        wand = {"id": "wand", "name": "staff", "class": "wand", "quantity": 1, "identified": True,
                "identity": "item.wand.wand-teleport-away"}
        offer = {"item": "wand", "zappable": True, "device_activation": "direction", "charges": 1}
        for tiles, target in (([cell("here"), cell("east", 1)], 1),
                              ([cell("here"), cell("east", 1), cell("west", -1)], -1)):
            with self.subTest(target=target):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.opponents.damage_by_target.update(west=8, east=3)
                turn = player_turn(tiles, actors=foes, hostiles=["west", "east"], inventory=[wand],
                                   interactions={"inventory": [offer]})
                self.assertTrue(all(turn.in_contact(foe) for foe in turn.hostiles))
                self.assertEqual(choose(player, turn), (Zap(turn.devices[0], turn.cell_at((target, 0, 0))),
                                                        "displace_threat"))

    def test_displacement_skips_an_off_axis_foe_touching_the_players_extended_body(self):
        from tor_client_bot import Zap

        cells = [cell("here"), cell("body", 1), cell("east", 2), cell("south", 1, 1),
                 cell("brute", 2, 1), cell("scout", -1)]
        actors = [{"id": actor, "name": actor, "position": tile["position"]}
                  for actor, tile in (("player", cells[1]), ("brute", cells[4]), ("scout", cells[5]))]
        wand = {"id": "rod", "class": "wand", "name": "rod", "quantity": 1, "identified": True,
                "identity": "item.wand.wand-teleport-away"}
        offer = {"item": "rod", "zappable": True, "device_activation": "direction", "charges": 1}
        turn = player_turn(cells, actors=actors, hostiles=["brute", "scout"], inventory=[wand],
                           interactions={"inventory": [offer]})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.opponents.damage_by_target.update(brute=8, scout=3)
        self.assertTrue(all(turn.in_contact(foe) for foe in turn.hostiles))
        self.assertEqual(choose(player, turn), (Zap(turn.devices[0], turn.cell_at((-1, 0, 0))), "displace_threat"))

    def test_equally_ranked_melee_targets_keep_their_observed_order(self):
        from rogue_combat import OpponentHistory, ranked_melee_targets

        tiles = [cell("here"), cell("east", 1), cell("west", -1)]
        foes = [{"id": target, "name": "orc", "position": tile["position"]}
                for target, tile in zip(("z", "a"), tiles[1:])]
        turn = player_turn(tiles, actors=foes, hostiles=["z", "a"])
        self.assertEqual(len(turn.hostiles), 2)
        targets = ranked_melee_targets(turn, OpponentHistory(), lambda foe: False)
        self.assertEqual(targets, list(turn.hostiles))

    def test_when_pinned_attacks_the_equally_healthy_foe_blocking_the_down_stair(self):
        melee = {"id": "basic_melee", "name": "Attack", "cooldown_remaining": "0", "cost": None,
                 "basic_attack": True, "reach": {"kind": "melee"}, "target_kind": "actor"}
        room = [cell("here"), cell("plain", 1), cell("exit", -1, down=True)]
        enemies = [{"id": name, "name": "rattlesnake", "position": tile["position"]}
                   for name, tile in zip(["other", "blocker"], room[1:])]
        for condition, warning, expected in ((None, 1, "other"), (None, 0, "blocker"),
                                             ("rogue.levitation", 0, "other"), ("rogue.bear_trap", 0, "other")):
            with self.subTest(condition=condition, warning=warning):
                conditions = [{"condition": {"id": condition, "name": condition},
                               "phase": "active", "remaining_ticks": 100}] if condition else []
                turn = player_turn(room, actors=enemies, hostiles=["other", "blocker"], abilities=[melee],
                                   combat={"hp": 7, "max_hp": 42}, stats={"conditions": conditions})
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=8)
                player.unsafe_stairs["exit", Direction.DOWN] = turn.tick + warning
                player.opponents.damage_by_target["other"] = 3
                action, reason = choose(player, turn)
                self.assertEqual((reason, action.target.target), ("melee", expected))

    def test_first_floor_upstairs_is_a_melee_priority_only_when_carrying_the_objective(self):
        relic = {"id": "amulet", "name": AMULET, "identity": DEFAULT_OBJECTIVE_IDENTITY,
                 "identified": True, "quantity": 1}
        tiles = [cell("here"), cell("plain", 1), cell("exit", -1, up=True)]
        enemies = [{"id": name, "name": "orc", "position": tile["position"]}
                   for name, tile in zip(("plain", "exit"), tiles[1:])]
        melee = {"id": "basic_melee", "name": "Attack", "cooldown_remaining": 0, "cost": None,
                 "basic_attack": True, "reach": {"kind": "melee"}, "target_kind": "actor"}
        for pack, expected in (([], "plain"), ([relic], "exit")):
            with self.subTest(expected=expected):
                turn = player_turn(tiles, inventory=pack, actors=enemies, hostiles=["plain", "exit"], abilities=[melee])
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.opponents.damage_by_target["plain"] = 1
                action, reason = choose(player, turn)
                self.assertEqual((reason, action.target.target), ("melee", expected))

    def test_melee_breaks_equal_body_distance_by_observed_damage(self):
        cells = [cell(str(x), x) for x in range(-1, 3)]
        actors = [{"id": name, "name": "troll", "position": cell(name, x)["position"]}
                  for name, x in (("west", -1), ("player", 1), ("east", 2))]
        melee = {"id": "basic_melee", "name": "Attack", "basic_attack": True,
                 "reach": {"kind": "melee"}, "target_kind": "actor", "cooldown_remaining": "0", "cost": None}
        trap = {"condition": {"id": "rogue.bear_trap", "name": "Bear trap"},
                "phase": "active", "remaining_ticks": 100}
        turn = player_turn(cells, actors=actors, hostiles=["west", "east"], abilities=[melee],
                           stats={"conditions": [trap]})
        for dangerous in ("east", "west", None):
            with self.subTest(dangerous=dangerous):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.opponents.damage_by_name["troll"] = 6
                if dangerous:
                    player.opponents.damage_by_target[dangerous] = 4
                attack, reason = choose(player, turn)
                self.assertEqual((reason, attack.target.target), ("melee", dangerous or "west"))

    def test_slow_wand_reacts_to_distance_from_the_players_nearest_body_cell(self):
        cells = [cell(str(x), x) for x in range(5)]
        foe = {"id": "troll", "name": "troll", "position": cells[4]["position"]}
        body = {"id": "player", "name": "you", "position": cells[1]["position"]}
        wand = {"id": "slow", "class": "wand", "name": "staff", "identity": "item.wand.wand-slow",
                "identified": True, "quantity": 1}
        offer = {"item": "slow", "zappable": True, "device_activation": "direction", "charges": 2}
        for actors, should_slow in (([foe], False), ([body, foe], True)):
            with self.subTest(should_slow=should_slow):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.opponents.damage_by_target["troll"] = 7
                turn = player_turn(cells, actors=actors, hostiles=["troll"], inventory=[wand],
                                   interactions={"inventory": [offer]})
                action, reason = choose(player, turn)
                self.assertEqual(reason == "slow_approach", should_slow)
                if should_slow:
                    self.assertEqual(action.target, turn.cell_at((4, 0, 0)))
                    player.admitted(turn, action, reason=reason)
                    self.assertIn("troll", player.attempted_slow)

    def test_control_a_dangerous_approach_or_step_to_an_exit(self):
        from tor_client_bot import Zap
        here, lane, distant = cell("here"), cell("lane", 1), cell("enemy", 2)
        foe = {"id": "troll", "name": "dragon", "position": distant["position"]}
        wand = {"id": "slow", "class": "wand", "identity": "item.wand.wand-slow",
                "identified": True, "name": "slow staff", "quantity": 1}
        offer = {"item": "slow", "zappable": True, "device_activation": "direction", "charges": 3}
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=4)
        player.opponents.damage_by_name["dragon"] = 7
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

    def test_slow_attempt_tracks_only_the_foe_reached_in_a_shared_lane(self):
        cells = [cell(str(x), x) for x in range(4)]
        foes = [{"id": name, "name": name, "position": cells[x]["position"]}
                for name, x in (("front", 2), ("rear", 3))]
        wand = {"id": "staff", "name": "staff", "class": "wand", "identity": "item.wand.wand-slow",
                "identified": True, "quantity": 1}
        offer = {"item": "staff", "zappable": True, "device_activation": "direction", "charges": 2}
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.opponents.damage_by_target.update(front=6, rear=9)
        for actors, expected, tick in ((foes, "front", 0), (foes[1:], "rear", 100)):
            turn = player_turn(cells, actors=actors, hostiles=[foe["id"] for foe in actors], tick=tick,
                               inventory=[wand], interactions={"inventory": [offer]})
            action, reason = choose(player, turn)
            self.assertEqual(reason, "slow_approach")
            target = next(foe for foe in turn.hostiles if foe.target == expected)
            self.assertIn(action.target, turn.cells_of(target))
            player.admitted(turn, action, reason=reason)
        self.assertEqual(player.attempted_slow, {"front", "rear"})

    def test_melee_exposure_counts_only_opponents_in_reach_of_the_candidate_square(self):
        from rogue_combat import OpponentHistory

        cells = [cell("here"), cell("west", -1), cell("east", 1), cell("exit", -2)]
        foes = [{"id": tile["key"], "name": "orc", "position": tile["position"]} for tile in cells[1:3]]
        turn = player_turn(cells, actors=foes, hostiles=["west", "east"])
        history = OpponentHistory()
        history.damage_by_name["orc"] = 2
        history.damage_by_target["west"] = 5
        self.assertEqual(history.melee_exposure(turn).damage, 7)
        at_exit = history.melee_exposure(turn, (-2, 0, 0))
        self.assertEqual([foe.target for foe in at_exit.foes], ["west"])
        self.assertEqual(at_exit.damage, 5)

    def test_control_targets_include_close_drainers_and_rank_only_nearby_threats(self):
        cells = [cell(str(x), x) for x in range(5)]
        foes = [{"id": name, "name": name, "position": cells[x]["position"]}
                for name, x in (("drainer", 1), ("brute", 2), ("scout", 3), ("distant", 4))]
        turn = player_turn(cells, actors=foes, hostiles=[foe["id"] for foe in foes])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.opponents.draining_names.add("drainer")
        player.opponents.damage_by_target.update(brute=5, scout=4, distant=20)
        targets = player.control_targets(turn, health=10)
        self.assertEqual([(foe.target, damage) for foe, damage in targets], [("brute", 5), ("drainer", 0)])

    def test_control_wands_respect_priority_and_fall_back_after_a_slow_attempt(self):
        cells = [cell("here"), cell("lane", 1), cell("enemy", 2)]
        foe = {"id": "dragon", "name": "dragon", "position": cells[2]["position"]}
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.opponents.damage_by_target["dragon"] = 7
        # The missile is offered first, but slowing this approach takes priority.
        names = ["magic-missile", "slow"]
        inventory = [{"id": name, "name": name, "class": "wand", "identified": True,
                      "identity": f"item.wand.wand-{name}", "quantity": 1} for name in names]
        offers = [{"item": name, "zappable": True, "device_activation": "direction", "charges": 2}
                  for name in names]
        turn = player_turn(cells, actors=[foe], hostiles=["dragon"], inventory=inventory,
                           interactions={"inventory": offers})
        action, reason = choose(player, turn)
        self.assertEqual((action.device.item.target, reason), ("slow", "slow_approach"))
        player.admitted(turn, action, reason=reason)
        action, reason = choose(player, turn)
        self.assertEqual((action.device.item.target, reason), ("magic-missile", "magic_missile"))

    def test_combat_wands_check_each_lane_with_the_first_directional_offer(self):
        from unittest.mock import patch

        identity = "item.wand.wand-cold"
        modes = ("self_target", "direction_or_self", "direction")
        wands = [{"id": mode, "name": mode, "class": "wand", "identity": identity, "identified": True,
                  "quantity": 1} for mode in modes]
        offers = [{"item": mode, "device_activation": mode, "zappable": True, "charges": 1} for mode in modes]
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        for blocked in (False, True):
            with self.subTest(blocked=blocked):
                cells = [cell(str(x), x, wall=blocked and x == 2) for x in range(7)]
                foe = {"id": "emu", "name": "emu", "position": cells[3]["position"]}
                turn = player_turn(cells, actors=[foe], hostiles=["emu"], inventory=wands,
                                   interactions={"inventory": offers})
                with patch.object(turn, "aim_device", wraps=turn.aim_device) as aim:
                    shot = player.aim_combat_wand(turn, turn.hostiles, {identity}, reflected_ray_range=6)
                if blocked:
                    self.assertIsNone(shot)
                else:
                    self.assertEqual(shot.device.item.target, "direction_or_self")
                aim.assert_called_once()

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

    def test_fires_a_charged_elemental_wand_before_switching_to_a_melee_weapon(self):
        cells = [cell(str(i), i) for i in range(7)]
        foe = {"id": "foe", "name": "hunter", "position": cells[2]["position"]}
        inventory = [{"id": name, "name": name, "identity": f"item.weapon.{name}", "identified": True,
                      "quantity": 1} for name in ("shortbow", "mace", "arrow")]
        inventory.append({"id": "rod", "name": "rod", "class": "wand", "identity": "item.wand.wand-cold",
                          "identified": True, "quantity": 1})
        slot = {"id": "weapon", "name": "Weapon"}
        for charges, expected in ((1, "elemental_ray"), (0, "change_weapon")):
            with self.subTest(charges=charges):
                offers = [{"item": "shortbow", "slot": slot, "equipped_slot": 0}, {"item": "mace", "slot": slot},
                          {"item": "rod", "zappable": True, "device_activation": "direction", "charges": charges}]
                turn = player_turn(cells, actors=[foe], hostiles=["foe"], inventory=inventory,
                                   interactions={"slots": [slot], "inventory": offers})
                self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)[1], expected)

    def test_elemental_ray_prioritizes_the_foe_with_greater_observed_damage(self):
        cells = [cell(str(x), x) for x in range(-6, 7)]
        foes = [{"id": name, "name": name, "position": cell(name, x)["position"]}
                for name, x in (("west", -4), ("east", 4))]
        wand = {"id": "rod", "class": "wand", "name": "rod", "quantity": 1, "identified": True,
                "identity": "item.wand.wand-cold"}
        offer = {"item": "rod", "zappable": True, "device_activation": "direction", "charges": 1}
        turn = player_turn(cells, actors=foes, hostiles=["west", "east"], inventory=[wand],
                           interactions={"inventory": [offer]})
        for target, offset in (("west", -4), ("east", 4)):
            with self.subTest(target=target):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.opponents.damage_by_target[target] = 5
                action, reason = choose(player, turn)
                self.assertEqual((reason, action.target), ("elemental_ray", turn.cell_at((offset, 0, 0))))

    def test_polymorph_is_reserved_for_a_dangerous_uninjured_approach(self):
        wand = {"id": "staff", "class": "wand", "identity": "item.wand.wand-polymorph",
                "identified": True, "name": "staff of polymorph", "quantity": 1}
        offer = {"item": "staff", "zappable": True, "device_activation": "direction_or_self", "charges": 3}
        cells = [cell("here"), cell("lane", 1), cell("foe", 2)]
        foe = {"id": "hunter", "name": "griffin", "position": cells[-1]["position"]}
        for damage, injury, expected in [(10, "healthy", True), (5, "healthy", False), (10, "near_death", False)]:
            with self.subTest(damage=damage, injury=injury):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.opponents.damage_by_name["griffin"] = damage
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
                player.opponents.damage_by_name["griffin"] = 10
                action, reason = choose(player, turn)
                self.assertEqual(reason, "polymorph_threat")
                player.admitted(turn, action, reason=reason)
                player.admitted(turn, action, reason=reason)
                report = CombatReport("player", ({**own_cast, **fields},))
                player.observe(turn, report)
                player.observe(turn, report)  # The same resistance still permits exactly one retry.
                action, reason = player.choose(turn)
                self.assertEqual(reason == "polymorph_threat", retry)
                if retry:
                    player.admitted(turn, action, reason=reason)
                    player.observe(turn, report)
                    self.assertNotEqual(player.choose(turn)[1], "polymorph_threat")

    def test_polymorph_resistance_only_updates_the_opponent_previously_targeted(self):
        from rogue_combat import PolymorphHistory, PolymorphState
        from tor_client_bot import AbilityOutcome, OutgoingAbility

        history = PolymorphHistory()
        history.admitted(["orc", "troll"])
        history.observe(tuple(OutgoingAbility(target, "rogue.wand_polymorph", AbilityOutcome.UNAFFECTED, None)
                              for target in ("troll", "griffin")))
        self.assertEqual(history.by_target, {"orc": PolymorphState.ATTEMPTED, "troll": PolymorphState.RESISTED})

    def test_resisted_polymorph_requires_disclosed_charges_even_on_a_fresh_wand(self):
        cells = [cell("here"), cell("enemy", 1)]
        foe = {"id": "hunter", "name": "griffin", "position": cells[1]["position"]}
        wand = {"id": "fresh", "class": "wand", "identity": "item.wand.wand-polymorph",
                "identified": True, "quantity": 1, "name": "staff"}
        offer = {"item": "fresh", "zappable": True, "device_activation": "direction"}
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.opponents.damage_by_name["griffin"] = 10
        from rogue_combat import PolymorphState
        player.polymorph.by_target["hunter"] = PolymorphState.RESISTED
        for charges, retry in ((None, False), (0, False), (1, True)):
            with self.subTest(charges=charges):
                turn = player_turn(cells, actors=[foe], inventory=[wand],
                                   interactions={"inventory": [dict(offer, charges=charges)]},
                                   combat={"actors": [{"actor": "hunter", "hostile": True, "injury": "healthy"}]})
                self.assertEqual(choose(player, turn)[1] == "polymorph_threat", retry)

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
                player.opponents.damage_by_name["troll"] = 19
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
        inventory = [dict(ration, id="unavailable"), ration, dict(ration, id="spare")]
        for foes in ([], [enemy]):
            with self.subTest(in_combat=bool(foes)):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, recovering_from_pursuit=True)
                turn = player_turn([cell("here"), cell("away", -1), cell("foe", 1)], actors=foes,
                                   hostiles=[foe["id"] for foe in foes], combat={"hp": "24", "max_hp": "35"},
                                   inventory=inventory, stats={"nutrition": {"hunger": "weak"}},
                                   interactions={"inventory": [{"item": "spare", "edible": True},
                                                               {"item": "food", "edible": True}]})
                self.assertEqual(choose(player, turn), (UseItem(turn.inventory[1], ItemUse.EAT), "eat"))

    def test_selects_the_most_useful_measured_meal_for_the_available_capacity(self):
        from tor_client_bot import ItemUse, UseItem

        meals = [{"id": name, "name": name, "identity": name, "class": "food", "identified": True,
                  "quantity": 1} for name in ("unknown", "snack", "feast", "ration")]
        foe = {"id": "foe", "name": "centaur", "position": cell("east", 1)["position"]}
        for reserve, expected in ((5, "feast"), (65, "ration"), (95, "snack")):
            for foes in ([], [foe]):
                with self.subTest(reserve=reserve, in_combat=bool(foes)):
                    player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                    player.meals.gains.update(snack=10, feast=80, ration=40)
                    turn = player_turn([cell("here"), cell("east", 1)], actors=foes, hostiles=["foe"] if foes else [],
                                       inventory=meals, interactions={"inventory": [
                                           {"item": meal["id"], "edible": True} for meal in meals]},
                                       stats={"nutrition": {"kind": "required", "hunger": "weak", "reserve": reserve,
                                                            "maximum": 100, "unit_ticks": 100, "elapsed": 0}})
                    item = next(item for item in turn.inventory if item.target == expected)
                    self.assertEqual(choose(player, turn), (UseItem(item, ItemUse.EAT), "eat"))

    def test_hungry_player_ranks_reachable_ground_food_by_usable_nutrition(self):
        here = cell("here")
        foods = [{"id": name, "name": name, "identity": name, "identified": True, "class": "food", "quantity": 1}
                 for name in ("unknown", "snack", "ration", "feast")]
        ground = [{**here, "item": item, "reachable": item["id"] != "feast"} for item in foods]
        for reserve, expected in ((5, "ration"), (95, "snack")):
            for order in (ground, ground[::-1]):
                with self.subTest(reserve=reserve, first=order[0]["item"]["id"]):
                    player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                    player.meals.gains.update(snack=10, ration=40, feast=80)
                    turn = player_turn([here], ground=order, stats={"nutrition": {
                        "kind": "required", "hunger": "weak", "reserve": reserve,
                        "maximum": 100, "unit_ticks": 100, "elapsed": 0}})
                    action, reason = choose(player, turn)
                    self.assertEqual((reason, action.item.target), ("take", expected))

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
                player.opponents.damage_by_name["centaur"] = 16
                cells = [cell("here"), cell("stairs", 1, up=True), {**cell("enemy"), "position": foe["position"]}]
                if side_open:
                    cells.append(cell("side", -1))
                turn = player_turn(cells, actors=[foe], hostiles=["centaur"],
                                   combat={"hp": str(health), "max_hp": "35"})
                player.observe(turn)
                player.recovering_from_pursuit = True
                self.assertEqual(player.retreat_to_stairs(turn)[0], Move(expected))

    def test_stair_approach_can_leave_a_lethal_attacker_while_a_weaker_one_remains(self):
        cells = [cell("here"), cell("stairs", 1, up=True), cell("side", -1), tile(1, 1), tile(0, 1), tile(-1, 1)]
        foes = [{"id": name, "name": name, "position": square["position"]}
                for name, square in zip(("griffin", "bat"), cells[3:])]
        turn = player_turn(cells, actors=foes, hostiles=["griffin", "bat"], combat={"hp": 10})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
        player.observe(turn)
        player.opponents.damage_by_target["griffin"] = 10
        for damage, expected in ((0, Direction.EAST), (4, Direction.WEST), (5, Direction.EAST)):
            with self.subTest(damage=damage):
                player.opponents.damage_by_target["bat"] = damage
                self.assertEqual(player.choose(turn)[0], Move(expected))
        player.opponents.damage_by_target["bat"] = 1
        player.opponents.draining_names.add("bat")
        self.assertEqual(player.choose(turn)[0], Move(Direction.EAST))

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
            player.opponents.damage_by_name["griffin"] = 20
            action, reason = choose(player, turn)
            if can_evade:
                self.assertEqual(reason, "retreat")
                self.assertFalse(turn.in_contact(turn.hostiles[0], from_offset=action.direction.offset))
            else:
                self.assertEqual((action, reason), (OpenDoor(turn.cell_at((1, 0, 0)).door), "retreat_to_stairs"))

    def test_retreat_breaks_contact_with_the_harder_hitter_on_either_flank(self):
        room = [tile(x, y) for x in (-1, 0, 1) for y in (0, 1)]
        foes = [{"id": name, "name": name, "position": tile(x, 1)["position"]}
                for name, x in (("west", -1), ("east", 1))]
        turn = player_turn(room, actors=foes, hostiles=["west", "east"])
        for dangerous, weaker in (("west", "east"), ("east", "west")):
            with self.subTest(dangerous=dangerous):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.opponents.damage_by_target.update({dangerous: 8, weaker: 1})
                action, reason = choose(player, turn)
                self.assertEqual(reason, "retreat")
                remaining = [foe.target for foe in turn.hostiles
                             if turn.in_contact(foe, from_offset=action.direction.offset)]
                self.assertEqual(remaining, [weaker])

    def test_one_unknown_damage_estimate_keeps_the_terrain_based_retreat_choice(self):
        from rogue_retreat import choose_retreat_step

        room = [tile(x, y) for x in (-1, 0, 1) for y in (0, 1)]
        foes = [{"id": name, "name": name, "position": tile(x, 1)["position"]}
                for name, x in (("west", -1), ("east", 1))]
        turn = player_turn(room, actors=foes, hostiles=["west", "east"])
        preferred = choose_retreat_step(turn)
        remaining, = [foe for foe in turn.hostiles if turn.in_contact(foe, from_offset=preferred.direction.offset)]
        self.assertEqual(choose_retreat_step(turn, lambda foe: 8 if foe == remaining else 0), preferred)

    def test_retreat_reuses_contact_checks_when_comparing_damage(self):
        room = [tile(x, y) for x in (-1, 0, 1) for y in (0, 1)]
        foes = [{"id": name, "name": name, "position": tile(x, 1)["position"]}
                for name, x in (("west", -1), ("east", 1))]
        turn = player_turn(room, actors=foes, hostiles=["west", "east"])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.opponents.damage_by_target.update(west=1, east=8)
        with patch.object(turn, "in_contact", wraps=turn.in_contact) as contact:
            action, reason = player.evade(turn)
        self.assertEqual((action.direction.offset[0], reason), (-1, "retreat"))
        checks = [(call.args[0].target, call.kwargs["from_offset"]) for call in contact.call_args_list]
        self.assertEqual(len(checks), len(set(checks)))

    def test_public_policy_steps_out_of_one_of_two_melee_contacts(self):
        room = [tile(x, y) for x, y in [(0, 0), (1, 0), (0, -1), (0, 1), (1, 1)]]
        foes = [{"id": name, "name": name, "position": {"x": x, "y": y, "z": 0}}
                for name, x, y in [("east", 1, 0), ("north", 0, -1)]]
        turn = player_turn(room, actors=foes, hostiles=["east", "north"], combat={"hp": 9, "max_hp": 58})
        self.assertEqual(sum(turn.in_contact(foe) for foe in turn.hostiles), 2)
        self.assertEqual(sum(turn.in_contact(foe, from_offset=(0, 1, 0)) for foe in turn.hostiles), 1)
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        self.assertEqual(choose(player, turn), (Move(Direction.SOUTH), "retreat"))

    def test_melee_breaks_a_tie_using_damage_observed_from_each_foe(self):
        cells = [cell("here"), cell("left", -1), cell("right", 1)]
        foes = [{"id": tile["key"], "name": "orc", "position": tile["position"]} for tile in cells[1:]]
        strike = {"id": "strike", "name": "Strike", "cooldown_remaining": 0, "cost": None,
                  "basic_attack": True, "reach": {"kind": "melee"}, "target_kind": "actor"}
        turn = player_turn(cells, actors=foes, hostiles=["left", "right"], abilities=[strike])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        preferred = turn.hostiles[1]
        player.opponents.damage_by_name[preferred.name] = 3
        player.opponents.damage_by_target[preferred.target] = 3
        action, reason = choose(player, turn)
        self.assertEqual((reason, action.target.target), ("melee", preferred.target))

    def test_prioritizes_a_disclosed_near_death_foe_when_cornered(self):
        north = cell("north", down=True)
        north["position"]["y"] = -1
        east = cell("east", 1)
        foes = [{"id": position["key"], "position": position["position"]} for position in (north, east)]
        melee = {"id": "strike", "name": "Strike", "cooldown_remaining": "0", "cost": None,
                 "basic_attack": True, "reach": {"kind": "melee"}, "target_kind": "actor"}
        push = {**melee, "id": "dislocating_strike", "basic_attack": False}
        for foe in foes:
            foe["name"] = "opponent"
        turn = player_turn([cell("here"), north, east, cell("beyond", 2)], actors=foes, abilities=[melee, push],
                           combat={"actors": [{"actor": "north", "injury": "healthy", "hostile": True},
                                              {"actor": "east", "injury": "near_death", "hostile": True}]})
        action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
        self.assertEqual(reason, "melee")
        self.assertEqual((action.target.target, action.ability.identity), ("east", "strike"))

    def test_waits_for_bear_trap_to_release_before_descending(self):
        trap = {"condition": {"id": "rogue.bear_trap", "name": "Bear trap"}, "phase": "active",
                "remaining_ticks": "100", "appearance": "status_indicator", "cue": "status"}
        tiles = [cell("stairs", down=True)]
        policy = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        self.assertEqual(choose(policy, player_turn(tiles, stats={"conditions": [trap]})),
                         (Wait(), "wait_for_restraint"))
        self.assertEqual(choose(policy, player_turn(tiles)), (Move(Direction.DOWN), "descend"))

        trap["remaining_ticks"] = None
        self.assertEqual(choose(policy, player_turn(tiles, stats={"conditions": [trap]})),
                         (Move(Direction.NORTH), "escape_bear_trap"))

        for phase, remaining in (("active", "0"), ("inactive", None)):
            with self.subTest(phase=phase, remaining=remaining):
                expired = {**trap, "phase": phase, "remaining_ticks": remaining}
                turn = player_turn(tiles, stats={"conditions": [expired]})
                self.assertEqual(choose(policy, turn), (Move(Direction.DOWN), "descend"))

    def test_inactive_untimed_trap_does_not_trigger_escape_from_a_timed_trap(self):
        timed = {"condition": {"id": "rogue.bear_trap", "name": "Bear trap"},
                 "phase": "active", "remaining_ticks": "100"}
        inactive = {**timed, "phase": "inactive", "remaining_ticks": None}
        for conditions in ([timed, inactive], [inactive, timed]):
            with self.subTest(conditions=conditions):
                turn = player_turn([cell("stairs", down=True)], stats={"conditions": conditions})
                self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn),
                                 (Wait(), "wait_for_restraint"))

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

    def test_cancelled_scroll_trial_preserves_other_scroll_choices(self):
        import json
        from tor_client_bot import ActivityChoice, Result

        scrolls = [{"id": name, "name": name, "class": "scroll", "identified": False, "quantity": 1}
                   for name in ("MIR VEL", "NAR")]
        turn = player_turn([cell("here")], inventory=scrolls, interactions={
            "inventory": [{"item": s["id"], "readable": True} for s in scrolls]})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        action, reason = choose(player, turn)
        self.assertEqual((action.item.target, reason), ("MIR VEL", "test_unknown_magic"))
        receipt = {"type": "admitted", "actor": 1, "branch": "game", "intention": "read", "phase": "queued"}
        result = Result.from_reply({"status": "confirmed", "receipt": receipt})
        player.admitted(turn, action, result, reason=reason)
        picture = json.loads(turn.recording())
        picture.update(can_cancel=True, readiness={"admission": False})
        for change in ({"phase": "started"}, {"intention": "other"}, {"branch": "other"}, {"actor": 2}):
            with self.subTest(change=change):
                stalled = Turn({**picture, "intentions": [{**receipt, **change}]})
                self.assertEqual(player.stalled(stalled, result), (None, "readiness_timeout"))
        stalled = Turn({**picture, "intentions": [receipt]})
        cancel, reason = player.stalled(stalled, result)
        self.assertEqual((cancel, reason), (ActivityChoice.CANCEL, "cancel_stalled_scroll"))
        player.admitted(stalled, cancel, reason=reason)
        action, reason = choose(player, turn)
        self.assertEqual((action.item.target, reason), ("NAR", "test_unknown_magic"))

        # The stall belongs to this floor. An identical inventory on another floor can retry.
        player.depth += 1
        action, reason = choose(player, turn)
        self.assertEqual((action.item.target, reason), ("MIR VEL", "test_unknown_magic"))

    def test_cancelled_first_read_leaves_the_appearance_available_for_stair_study(self):
        from tor_client_bot import ActivityChoice, Result

        turn = scroll_trial_turn([cell("stairs", down=True)], ("one", "runed scroll"), ("two", "runed scroll"))
        result = Result.from_reply({"status": "confirmed", "receipt": read_activity()})
        for previously_sampled in (False, True):
            with self.subTest(previously_sampled=previously_sampled):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
                if previously_sampled:
                    prior, reason = player.scroll_trial(turn, at_exit=True)
                    player.admitted(turn, prior, reason=reason)
                    self.assertIsNone(player.scroll_trial(turn, at_exit=True))
                action, reason = player.scroll_trial(turn)
                player.admitted(turn, action, result, reason=reason)
                player.admitted(turn, ActivityChoice.CANCEL, reason="cancel_stalled_scroll")
                retry = player.scroll_trial(turn, at_exit=True)
                if previously_sampled:
                    self.assertIsNone(retry)
                else:
                    self.assertEqual(retry[0].item.target, "two")

    def test_unresolved_scroll_trial_blocks_both_stair_study_and_route_experiments(self):
        import json
        from tor_client_bot import Result

        turn = scroll_trial_turn([cell("here")], ("first", "MIR"), ("second", "NAR"))
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        action, reason = choose(player, turn)
        player.admitted(turn, action, Result.from_reply({"status": "confirmed", "receipt": read_activity()}), reason=reason)
        self.assertFalse(player.scroll_trials.sampled)
        for phase in ("queued", "started", "paused", "suspended"):
            with self.subTest(phase=phase):
                active = Turn({**json.loads(turn.recording()), "intentions": [read_activity(phase)]})
                player.observe(active)
                self.assertIsNone(player.route_scroll(active, player.maps[player.depth], returning=False))
                self.assertIsNone(player.scroll_trial(active, at_exit=True))

    def test_scroll_receipt_outside_retained_history_does_not_block_future_stair_study(self):
        from tor_client_bot import Result

        def picture(tick):
            return scroll_trial_turn([cell("stairs", down=True)], ("first", "MIR"), ("second", "NAR"), tick=tick)

        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        before = picture(100)
        action, reason = player.scroll_trial(before, at_exit=True)
        player.admitted(before, action, Result.from_reply({"status": "confirmed", "receipt": read_activity()}), reason=reason)
        player.observe(before)
        self.assertIsNone(player.scroll_trial(before, at_exit=True))
        later = picture(200)
        player.observe(later)
        trial = player.scroll_trial(later, at_exit=True)
        self.assertIsNotNone(trial)
        self.assertEqual(trial[0].item.target, "second")

    def test_failed_scroll_trial_skips_the_failed_item_but_can_study_another_copy(self):
        import json
        from tor_client_bot import Result

        turn = scroll_trial_turn([cell("here")], ("one", "runed"), ("two", "runed"))
        result = Result.from_reply({"status": "confirmed", "receipt": read_activity()})
        for phase, expected in (("failed", "two"), ("cancelled", "two"), ("resolved", None)):
            with self.subTest(phase=phase):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                action, reason = choose(player, turn)
                player.admitted(turn, action, result, reason=reason)
                after = Turn({**json.loads(turn.recording()), "finished_intentions": [read_activity(phase)]})
                player.observe(after)
                choice = player.scroll_trial(after, at_exit=True)
                self.assertEqual(choice[0].item.target if choice else None, expected)

    def test_interrupted_read_keeps_the_appearance_sampled_if_a_scroll_is_missing(self):
        from tor_client_bot import Result

        for remaining in (0, 1, 2, 3):
            with self.subTest(remaining=remaining):
                turn = scroll_trial_turn([cell("here")], ("one", "runed"), ("two", "runed"))
                picture = json.loads(turn.recording())
                inventory = picture["state"]["observation"]["inventory"]
                inventory[0]["quantity"] = 2
                before = Turn(picture)
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                action, reason = choose(player, before)
                receipt = Result.from_reply({"status": "confirmed", "receipt": read_activity()})
                player.admitted(before, action, receipt, reason=reason)
                after = json.loads(before.recording())
                inventory = after["state"]["observation"]["inventory"]
                if remaining:
                    inventory[0]["quantity"] = remaining
                else:
                    inventory.pop(0)
                after["finished_intentions"] = [read_activity("failed")]
                observed = Turn(after)
                player.observe(observed)
                choice = player.scroll_trial(observed, at_exit=True)
                self.assertEqual(choice[0].item.target if choice else None, "two" if remaining >= 2 else None)

    def test_failed_emergency_read_allows_another_scroll_only_if_the_first_survived(self):
        import json
        from tor_client_bot import Result

        foe = {"id": "orc", "name": "orc", "position": cell("orc", 1)["position"]}
        turn = scroll_trial_turn([cell("here"), cell("orc", 1)], ("first", "MIR"), ("second", "NAR"),
                                 actors=[foe], hostiles=["orc"], combat={"hp": 1, "max_hp": 10})
        for consumed in (False, True):
            with self.subTest(consumed=consumed):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.opponents.damage_by_target["orc"] = 2
                action, reason = choose(player, turn)
                self.assertEqual((action.item.target, reason), ("first", "test_unknown_magic"))
                result = Result.from_reply({"status": "confirmed", "receipt": read_activity()})
                player.admitted(turn, action, result, reason=reason)
                picture = json.loads(turn.recording())
                if consumed:
                    picture["state"]["observation"]["inventory"].pop(0)
                after = Turn({**picture, "finished_intentions": [read_activity("failed")]})
                player.observe(after)
                retry = player.combat(after)
                if consumed:
                    self.assertFalse(retry and retry[1] == "test_unknown_magic")
                else:
                    self.assertIsNotNone(retry)
                    self.assertEqual((retry[0].item.target, retry[1]), ("second", "test_unknown_magic"))

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

    def test_stalled_stair_cancellation_is_limited_per_origin_and_direction(self):
        from tor_client_bot import ActivityChoice, Result

        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        attempts = [
            ("shared", Direction.DOWN, True), ("shared", Direction.UP, True),
            ("other", Direction.DOWN, True), ("shared", Direction.DOWN, False),
            ("shared", Direction.UP, False), ("other", Direction.DOWN, False),
        ]
        for index, (origin, direction, can_cancel) in enumerate(attempts):
            with self.subTest(origin=origin, direction=direction, can_cancel=can_cancel):
                landing = player_turn([cell(origin, up=True, down=True)])
                player.observe(landing)
                receipt = {"type": "admitted", "actor": 1, "branch": "game",
                           "intention": str(index), "phase": "queued"}
                result = Result.from_reply({"status": "confirmed", "receipt": receipt})
                player.admitted(landing, Move(direction), result)
                picture = json.loads(landing.recording())
                blocked = Turn({**picture, "can_cancel": True, "intentions": [receipt],
                                "readiness": {"admission": False}})
                expected = (ActivityChoice.CANCEL, "cancel_stalled_stair") if can_cancel else (None, "readiness_timeout")
                self.assertEqual(player.stalled(blocked, result), expected)
                if can_cancel:
                    player.admitted(blocked, ActivityChoice.CANCEL, reason="cancel_stalled_stair")
                    clear = Turn({**picture, "finished_intentions": [dict(receipt, phase="cancelled")]})
                    self.assertEqual(choose(player, clear), (Wait(), "wait_after_cancel"))
                    player.admitted(clear, Wait(), reason="wait_after_cancel")
                else:
                    failed = Turn({**picture, "finished_intentions": [dict(receipt, phase="failed")]})
                    player.observe(failed)
                self.assertIsNone(player.pending_stair)

    def test_witnessed_arrival_resets_the_departure_cancellation_allowance(self):
        from unittest.mock import Mock
        from rogue_player import PendingStair, StairCrossings
        from tor_client_bot import Result

        origin = player_turn([cell("stairs", down=True)])
        receipt = Result.from_reply({"status": "confirmed", "receipt": {
            "type": "admitted", "actor": 1, "branch": "game", "intention": "down", "phase": "queued"}})
        pending = PendingStair(origin.stair_attempt(Move(Direction.DOWN), receipt), ())
        crossings = StairCrossings(pending=pending)
        crossings.record_cancellation()
        self.assertFalse(crossings.can_cancel(receipt))
        landing = Mock(can_act=False, terminal=False)
        landing.stair_arrival.return_value = True
        self.assertIsNone(crossings.observe(landing))
        self.assertFalse(crossings.can_cancel(receipt))
        landing.can_act = True
        self.assertIs(crossings.observe(landing), pending)
        self.assertIsNone(crossings.observe(landing))
        crossings.pending = pending
        self.assertTrue(crossings.can_cancel(receipt))

    def test_stair_retry_wait_applies_only_to_the_most_recent_cancellation(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=6)
        foe = {"id": "dragon", "name": "dragon", "position": cell("enemy", 2)["position"]}
        turn = player_turn([cell("stairs", up=True, down=True), cell("enemy", 2)],
                           actors=[foe], hostiles=["dragon"])
        for direction, expected in ((Direction.UP, (Wait(), "wait_after_cancel")),
                                    (Direction.DOWN, (Move(Direction.UP), "retreat_upstairs"))):
            with self.subTest(cancelled=direction):
                player.admitted(turn, Move(direction))
                player.crossings.record_cancellation()
                player.crossings.pending = None
                self.assertEqual(choose(player, turn), expected)
                player.admitted(turn, Wait())

    def test_healing_after_a_stair_cancellation_spends_the_retry_wait(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        here, east = cell("here", down=True), cell("east", 1)
        foe = {"id": "orc", "name": "orc", "position": east["position"]}
        turn = player_turn([here, east], actors=[foe], hostiles=["orc"], inventory=healing_potions(),
                           interactions={"inventory": [{"item": "healing", "drinkable": True}]},
                           combat={"hp": 3, "max_hp": 10})
        player.crossings.deferred_departure = turn.cell.key, Direction.DOWN
        action, reason = choose(player, turn)
        self.assertEqual(reason, "heal")
        self.assertEqual(player.crossings.deferred_departure, (turn.cell.key, Direction.DOWN))
        player.admitted(turn, action, reason=reason)
        self.assertEqual(choose(player, player_turn([here])), (Move(Direction.DOWN), "descend"))

    def test_completed_crossing_retires_the_wait_intended_for_a_cancelled_departure(self):
        from tor_client_bot import ActivityChoice, Result

        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        origin = player_turn([cell("stairs", down=True)])
        player.observe(origin)
        receipt = Result.from_reply({"status": "confirmed", "receipt": {
            "type": "admitted", "actor": 1, "branch": "game", "intention": "down", "phase": "queued"}})
        player.admitted(origin, Move(Direction.DOWN), receipt)
        player.admitted(origin, ActivityChoice.CANCEL, reason="cancel_stalled_stair")
        landing = player_turn([cell("landing", up=True, down=True)], tick=100)
        action, reason = choose(player, landing)
        self.assertEqual(player.depth, 2)
        self.assertEqual((action, reason), (Move(Direction.DOWN), "descend"))

    def test_terminal_depth_counts_only_a_witnessed_final_descent(self):
        from unittest.mock import Mock
        from rogue_runner import play
        from tor_client_bot import Outcome, Result, Session

        for final_cells, depth in (([cell("landing", up=True)], 2), ([cell("origin", down=True)], 1), ([], 1)):
            with self.subTest(final_cells=final_cells):
                origin = player_turn([cell("origin", down=True)])
                origin.submit = Mock(return_value=Result(Outcome.CONFIRMED, None, None))
                end = player_turn(final_cells, tick=100, combat={"terminal": True, "victory": False, "dead": True})
                session = Session(Mock())
                session.wait_for_decision = Mock(side_effect=[origin, end])
                report = play(session, Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), 1, Mock())
                origin.submit.assert_called_once_with(Move(Direction.DOWN))
                self.assertEqual((report["max_depth"], report["actions"]), (depth, 1))

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

    def test_action_trace_preserves_admission_and_rejection_details(self):
        from unittest.mock import Mock
        from rogue_player import play
        from tor_client_bot import Result, Session

        receipt = {"type": "admitted", "actor": 1, "branch": "game", "intention": "step", "phase": "queued"}
        for reply, expected in (
            ({"status": "confirmed", "receipt": receipt, "request_id": "request-1"},
             {"status": "confirmed", "receipt": {**receipt, "failure": None, "failed_action": None}, "code": None,
              "message": None, "request_id": "request-1"}),
            ({"status": "rejected", "code": "blocked", "message": "No passage"},
             {"status": "rejected", "receipt": {"intention": None}, "code": "blocked", "message": "No passage"}),
        ):
            with self.subTest(status=reply["status"]):
                turn = player_turn([cell("stairs", down=True)])
                turn.submit = Mock(return_value=Result.from_reply(reply))
                session = Session(Mock())
                session.wait_for_decision = Mock(side_effect=[turn, player_turn([], combat={"terminal": True, "victory": False})])
                rows = []
                play(session, Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), 1, rows.append)
                self.assertEqual(json.loads(json.dumps(rows[0]["result"])), expected)

    def test_pickup_admission_tracks_ground_stacks_that_merge_into_one_carried_stack(self):
        from rogue_equipment_history import EquipmentHistory

        here = cell("here")
        ration = {"id": "pack", "name": "ration", "class": "food", "quantity": 1, "identified": False}
        piles = [{**here, "reachable": True, "item": dict(ration, id=key)} for key in ("one", "two")]
        turn = player_turn([here], inventory=[ration], ground=piles)
        history = EquipmentHistory()
        history.observe(turn)
        for pile in turn.ground_items:
            history.admitted(Pickup(pile.item))
        history.admitted(Pickup(turn.ground_items[0].item))
        history.observe(player_turn([here], inventory=[dict(ration, quantity=3)]))
        self.assertEqual(history.counts()["food"], {"seen_on_ground": 2, "pickup_admitted": 2, "carried": 1})

    def test_equip_admission_counts_each_item_once_before_it_is_observed_worn(self):
        from rogue_equipment_history import EquipmentHistory

        slot = {"id": "weapon", "name": "Weapon"}
        blade = {"id": "blade", "name": "blade", "class": "weapon", "quantity": 1, "identified": False}
        turn = player_turn([cell("here")], inventory=[blade], interactions={
            "slots": [slot], "inventory": [{"item": "blade", "slot": slot}]})
        equip, = turn.equip_choices()
        history = EquipmentHistory()
        history.observe(turn)
        history.admitted(equip)
        history.admitted(equip)
        self.assertEqual(history.counts(), {"weapon": {"carried": 1, "equip_admitted": 1}})

    def test_equipment_history_counts_the_items_in_admitted_shots(self):
        from rogue_equipment_history import EquipmentHistory
        from tor_client_bot import Shot

        target = cell("east", 1)
        foe = {"id": "foe", "name": "orc", "position": target["position"]}
        ability = {"id": "shoot", "name": "shoot", "target_kind": "directional_projectile",
                   "reach": {"kind": "projectile"}, "cooldown_remaining": "0", "basic_attack": False, "cost": None}
        items = [{"id": name, "name": name, "class": "weapon", "quantity": 1, "identified": False}
                 for name in ("dart", "arrow", "bow")]
        turn = player_turn([cell("here"), target], actors=[foe], hostiles=["foe"], abilities=[ability], inventory=items)
        dart, arrow, bow = turn.inventory
        history = EquipmentHistory()
        for projectile, launcher in ((dart, None), (arrow, bow), (arrow, bow)):
            history.admitted(Shot(turn.abilities[0], turn.hostiles[0], (1, 0, 0), projectile, launcher, 0))
        self.assertEqual(history.counts(), {"weapon": {"projectile_admitted": 2, "launcher_admitted": 1}})

    def test_equipment_report_counts_distinct_items_and_retains_last_device_offers(self):
        from rogue_equipment_history import EquipmentHistory

        wand = {"class": "wand", "identity": "item.wand.wand-light", "identified": True,
                "name": "staff of light", "quantity": 1}
        inventory = [dict(wand, id=target) for target in ("spent", "unknown-charges", "unavailable")]
        offer = {"zappable": True, "device_activation": "self_target"}
        turn = player_turn([cell("here")], inventory=inventory, tick=100,
                           interactions={"inventory": [dict(offer, item="spent", charges=0),
                                                       dict(offer, item="unknown-charges")]})
        history = EquipmentHistory()
        history.observe(turn)
        history.observe(turn)
        history.observe(player_turn([cell("here")], tick=200, combat={"terminal": True}))

        self.assertEqual(history.counts(), {"wand": {"carried": 3, "recognized": 3}})
        self.assertEqual([row["item"] for row in history.report()["last_carried_devices"]],
                         ["spent", "unknown-charges", "unavailable"])
        self.assertEqual(history.last_inventory_tick, 100)
        self.assertEqual([(row["activation_offered"], row["charges"]) for row in history.last_devices],
                         [(True, 0), (True, None), (False, None)])

    def test_older_inventory_contributes_to_counts_without_replacing_current_devices(self):
        from rogue_equipment_history import EquipmentHistory

        wand = {"id": "staff", "class": "wand", "name": "staff", "quantity": 1, "identified": False}
        ring = {"id": "ring", "class": "ring", "name": "ring", "quantity": 1, "identified": False}
        history = EquipmentHistory()
        history.observe(player_turn([cell("here")], inventory=[wand], tick=100))
        history.observe(player_turn([cell("here")], inventory=[ring], tick=50))
        report = history.report()
        self.assertEqual(report["last_inventory_tick"], 100)
        self.assertEqual([item["item"] for item in report["last_carried_devices"]], ["staff"])
        self.assertEqual(report["equipment_opportunities"], {"wand": {"carried": 1}, "ring": {"carried": 1}})

        history.observe(player_turn([cell("here")], inventory=[ring], tick=100))
        self.assertEqual(history.report()["last_carried_devices"], [])

    def test_equipment_report_distinguishes_empty_inventory_from_no_live_observation(self):
        from rogue_equipment_history import EquipmentHistory

        history = EquipmentHistory()
        history.observe(player_turn([], tick=300, combat={"terminal": True}))
        self.assertEqual(history.report(), {
            "equipment_opportunities": {}, "last_inventory_tick": None, "last_carried_devices": [],
        })

        wand = {"id": "wand", "class": "wand", "identified": False, "name": "staff", "quantity": 1}
        history.observe(player_turn([cell("here")], inventory=[wand], tick=400))
        history.observe(player_turn([cell("here")], tick=500))
        self.assertEqual(history.report(), {
            "equipment_opportunities": {"wand": {"carried": 1}},
            "last_inventory_tick": 500,
            "last_carried_devices": [],
        })

    def test_budget_report_snapshots_the_return_estimate_for_each_floor(self):
        from unittest.mock import Mock
        from rogue_runner import play
        from tor_client_bot import Session

        session = Session(Mock())
        session.wait_for_decision = Mock(return_value=player_turn([cell("here")]))
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.travel_times.ascent[1] = 100
        first = play(session, player, 0, self.fail)
        player.travel_times.ascent[2] = 250
        second = play(session, player, 0, self.fail)

        for report, expected in ((first, {1: 100}), (second, {1: 100, 2: 250})):
            self.assertEqual(report["estimated_ascent_ticks_by_floor"], expected)
            self.assertEqual(report["estimated_ascent_ticks"], sum(expected.values()))

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

    def test_terminal_picture_without_victory_or_death_keeps_a_neutral_outcome(self):
        from unittest.mock import Mock
        from rogue_runner import play
        from tor_client_bot import Session

        terminal = player_turn([], combat={"terminal": True, "dead": False, "victory": False})
        session = Session(Mock())
        session.wait_for_decision = Mock(return_value=terminal)
        report = play(session, Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), 0, self.fail)
        self.assertEqual((report["terminal"], report["actions"]), ("terminal", 0))
        self.assertEqual(report["events_since_previous_action"], [])
        self.assertNotIn("last_nutrition", report)

    def test_terminal_report_retains_the_attack_when_only_death_is_in_the_final_picture(self):
        from unittest.mock import Mock
        from rogue_runner import play
        from tor_client_bot import Session

        attack = {"type": "ability", "caster": "snake", "target": "player", "outcome": "applied",
                  "ability": {"id": "rogue.venomous_bite"}}
        death = {"type": "died", "actor": "player", "cause": "injury"}
        for snapshot, batches in (([death], [[attack], [death]]), ([attack, death], [[death]])):
            with self.subTest(snapshot=snapshot):
                terminal = player_turn([], combat={"terminal": True, "dead": True, "victory": False, "events": snapshot})
                picture = json.loads(terminal.recording())
                picture["combat_updates"] = [{"cursor": {"sequence": str(i)}, "events": events}
                                             for i, events in enumerate(batches)]
                session = Session(Mock())
                session.wait_for_decision = Mock(return_value=Turn(picture))
                report = play(session, Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), 0, self.fail)
                self.assertEqual(report["last_combat_events"], [attack, death])
                self.assertEqual(report["events_since_previous_action"], [e for batch in batches for e in batch])

    def test_death_report_preserves_disclosed_starvation_evidence(self):
        from unittest.mock import Mock
        from rogue_o_matic_cohort import death_cause
        from rogue_runner import play
        from tor_client_bot import Session

        for threshold, expected in ((-3, "starvation"), (-4, "undisclosed"), (None, "undisclosed")):
            with self.subTest(threshold=threshold):
                food = {"kind": "required", "reserve": -3}
                if threshold is not None:
                    food["starve_at"] = threshold
                turn = player_turn([], stats={"nutrition": food},
                                   combat={"terminal": True, "dead": True, "victory": False})
                session = Session(Mock())
                session.wait_for_decision = Mock(return_value=turn)
                report = play(session, Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), 0, self.fail)
                self.assertEqual(death_cause(report), expected)

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
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
                report = play(session, player, 0, self.fail)
                self.assertEqual(report["terminal"], expected)
                self.assertEqual(report["max_depth"], 3)

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
        for hp, maximum, contacts, healing in [(5, 12, 1, True), (6, 12, 1, False), (8, 12, 2, True),
                                                (9, 12, 2, False), (8, 11, 2, True), (9, 11, 2, False)]:
            with self.subTest(hp=hp, maximum=maximum, contacts=contacts):
                turn = player_turn([cell("here"), cell("west", -1), cell("east", 1)], actors=foes[:contacts],
                                   hostiles=[foe["id"] for foe in foes[:contacts]], inventory=[potion],
                                   combat={"hp": hp, "max_hp": maximum},
                                   interactions={"inventory": [{"item": "heal", "drinkable": True}]})
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual(reason == "heal", healing)
                if healing:
                    self.assertEqual(action.item.target, "heal")

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
                self.assertEqual(policy.opponents.damage_by_target["foe"], loss)

    def test_known_lethal_hit_prioritizes_extra_healing_in_either_inventory_order(self):
        foe = {"id": "foe", "name": "troll", "position": cell("east", 1)["position"]}
        potions = healing_potions()
        scroll = {"id": "escape", "name": "escape", "identity": "item.scroll.teleportation",
                  "class": "scroll", "identified": True, "quantity": 1}
        offers = [{"item": p["id"], "drinkable": True} for p in potions] + [{"item": "escape", "readable": True}]
        for inventory in ([scroll, *potions], [*reversed(potions), scroll]):
            with self.subTest(first=inventory[0]["id"]):
                turn = player_turn([cell("here"), cell("east", 1)], actors=[foe], hostiles=["foe"],
                                   inventory=inventory, combat={"hp": 6, "max_hp": 10},
                                   interactions={"inventory": offers})
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.opponents.damage_by_target["foe"] = 6
                action, reason = choose(player, turn)
                self.assertEqual((action.item.target, reason), ("extra-healing", "heal"))

    def test_injured_player_selects_a_dose_by_two_round_damage_in_either_inventory_order(self):
        foe = {"id": "foe", "name": "orc", "position": cell("east", 1)["position"]}
        doses = healing_potions()
        for inventory in (doses, doses[::-1]):
            with self.subTest(first=inventory[0]["id"]):
                turn = player_turn([cell("here"), cell("east", 1)], inventory=inventory, actors=[foe],
                                   hostiles=["foe"], combat={"hp": 4, "max_hp": 10},
                                   interactions={"inventory": [{"item": p["id"], "drinkable": True} for p in doses]})
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                for damage, expected in ((1, "healing"), (2, "extra-healing")):
                    player.opponents.damage_by_target["foe"] = damage
                    action, reason = choose(player, turn)
                    self.assertEqual((action.item.target, reason), (expected, "heal"))

    def test_unthreatened_healing_preserves_extra_healing_in_either_inventory_order(self):
        doses = healing_potions()
        for inventory in (doses, doses[::-1]):
            with self.subTest(first=inventory[0]["id"]):
                turn = player_turn([cell("here")], inventory=inventory, combat={"hp": 3, "max_hp": 10},
                                   interactions={"inventory": [{"item": p["id"], "drinkable": True} for p in doses]})
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual((action.item.target, reason), ("healing", "heal"))

    def test_healing_uses_the_first_drinkable_copy_of_the_preferred_dose(self):
        ordinary, extra = healing_potions()
        inventory = [ordinary, extra, {**ordinary, "id": "first"}, {**ordinary, "id": "second"}]
        offers = [{"item": item["id"], "drinkable": True} for item in inventory[1:]]
        turn = player_turn([cell("here")], inventory=inventory, combat={"hp": 3, "max_hp": 10},
                           interactions={"inventory": offers})
        action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
        self.assertEqual((action.item.target, reason), ("first", "heal"))

    def test_injured_blind_player_uses_ordinary_healing_above_half_health(self):
        doses = healing_potions()
        blindness = {"condition": {"id": "rogue.blindness", "name": "Blindness"},
                     "phase": "active", "remaining_ticks": 100}
        offers = [{"item": item["id"], "drinkable": True} for item in doses]
        for hp, expected in ((8, True), (10, False)):
            with self.subTest(hp=hp):
                turn = player_turn([cell("here", down=True)], inventory=doses[::-1], combat={"hp": hp, "max_hp": 10},
                                   stats={"conditions": [blindness]}, interactions={"inventory": offers})
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual(reason == "heal", expected)
                if expected:
                    self.assertEqual(action.item.identity, "item.potion.healing")

    def test_overlapping_blindness_and_hallucination_allow_an_ordinary_healing_fallback(self):
        doses = healing_potions()
        conditions = [{"condition": {"id": identity, "name": identity}, "phase": "active", "remaining_ticks": 100}
                      for identity in ("rogue.blindness", "rogue.hallucinating")]
        for extra_offered, expected in ((True, "extra-healing"), (False, "healing")):
            with self.subTest(extra_offered=extra_offered):
                offers = [{"item": "healing", "drinkable": True}, {"item": "extra-healing", "drinkable": extra_offered}]
                turn = player_turn([cell("here")], inventory=doses, combat={"hp": 8, "max_hp": 10},
                                   stats={"conditions": conditions}, interactions={"inventory": offers})
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual((action.item.target, reason), (expected, "heal"))

    def test_minor_injury_and_hallucination_require_a_dose_that_can_treat_both(self):
        doses = healing_potions()
        hallucination = {"condition": {"id": "rogue.hallucinating", "name": "Hallucinating"},
                         "phase": "active", "remaining_ticks": 100}
        for hp, extra_offered, should_heal in ((8, True, True), (8, False, False), (10, True, False)):
            with self.subTest(hp=hp, extra_offered=extra_offered):
                offers = [{"item": "healing", "drinkable": True}, {"item": "extra-healing", "drinkable": extra_offered}]
                turn = player_turn([cell("here", down=True)], inventory=doses, combat={"hp": hp, "max_hp": 10},
                                   stats={"conditions": [hallucination]}, interactions={"inventory": offers})
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual(reason == "heal", should_heal)
                if should_heal:
                    self.assertEqual(action.item.identity, "item.potion.extra-healing")

    def test_healing_while_hallucinating_prefers_a_dose_that_also_cures_the_condition(self):
        condition = {"condition": {"id": "rogue.hallucinating", "name": "Hallucinating"},
                     "phase": "active"}
        potions = healing_potions()
        for remaining, extra_offered, expected in ((100, True, "extra-healing"), (100, False, "healing"),
                                                   (0, True, "healing")):
            with self.subTest(remaining=remaining, extra_offered=extra_offered):
                offers = [{"item": "healing", "drinkable": True}, {"item": "extra-healing", "drinkable": extra_offered}]
                turn = player_turn([cell("here")], inventory=potions, combat={"hp": 3, "max_hp": 10},
                                   stats={"conditions": [dict(condition, remaining_ticks=remaining)]},
                                   interactions={"inventory": offers})
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual((action.item.target, reason), (expected, "heal"))

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

    def test_amulet_carrier_prefers_a_past_fight_to_a_known_pit_on_the_way_home(self):
        amulet = {"id": "artifact", "identity": DEFAULT_OBJECTIVE_IDENTITY, "name": AMULET,
                  "quantity": 1, "identified": True}
        pit = {"id": "pit", "family": "pit", "reachable": True}
        tiles = [cell("here"), cell("passage", 1), cell("exit", 2, up=True)]
        for x in range(3):
            detour = cell(f"detour-{x}", x)
            detour["position"]["y"] = 1
            if x == 1:
                detour["trap"] = pit
            tiles.append(detour)
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.danger_until["passage"] = 500
        turn = player_turn(tiles, inventory=[amulet])
        self.assertEqual(choose(player, turn), (Move(Direction.EAST), "seek_stairs"))

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
        ready.data.return_value = json.loads(player_turn([far], tick=100).recording())
        native.observe.side_effect = [paused, ready]
        native.resume.return_value = {"status": "confirmed"}
        trace = []
        report = play(Session(native), Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), 1, trace.append)
        native.resume.assert_called_once_with(paused)
        native.act.assert_not_called()
        self.assertEqual(report["terminal"], "action_budget")
        self.assertEqual(trace[0]["tick"], 0)
        self.assertEqual(report["tick"], 100)
        self.assertIsNone(trace[0]["previous_intention_phase"])
        self.assertEqual(trace[0]["action"], {"type": "resume"})
        self.assertEqual(trace[0]["reachable_pickups"], [
            {"item": "ration-a", "category": "food", "identity": None, "quantity": 2, "cell": "here"}])

    def test_identification_precedes_resume_only_while_the_prompt_accepts_an_answer(self):
        from tor_client_bot import ActivityChoice

        potion = {"id": "dose", "name": "blue potion", "class": "potion", "identified": False, "quantity": 1}
        prompt = {"choice": "lore", "classes": ["potion"], "items": ["dose"]}
        turn = player_turn([cell("here")], inventory=[potion], interactions={"identification": prompt})
        picture = json.loads(turn.recording())
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        for admission in (True, False):
            with self.subTest(admission=admission):
                paused = Turn({**picture, "can_resume": True, "readiness": {"admission": admission}})
                action, reason = player.choose(paused)
                if admission:
                    self.assertEqual((action.item.target, reason), ("dose", "identify"))
                else:
                    self.assertEqual((action, reason), (ActivityChoice.RESUME, "resume"))

    def test_identification_prefers_the_larger_unknown_potion_stack(self):
        potion = {"class": "potion", "identified": False, "quantity": 1}
        single = {**potion, "id": "single", "name": "blue flask"}
        stack = {**potion, "id": "stack", "name": "pink flask", "quantity": 3}
        prompt = {"choice": "lore", "classes": ["potion"], "items": ["single", "stack"]}
        for inventory in ([single, stack], [stack, single]):
            with self.subTest(first=inventory[0]["id"]):
                turn = player_turn([cell("here")], inventory=inventory, interactions={"identification": prompt})
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual((action.item.target, reason), ("stack", "identify"))

    def test_weapon_identification_checks_the_worn_weapon_before_an_unmeasured_spare(self):
        slot = {"id": "weapon", "name": "Weapon"}
        spare = {"id": "spare", "identity": "item.weapon.dagger", "name": "dagger",
                 "class": "weapon", "identified": True, "quantity": 1}
        worn = {**spare, "id": "worn", "identity": "item.weapon.shortbow", "name": "shortbow"}
        for measured, expected in ((False, "worn"), (True, "spare")):
            with self.subTest(measured=measured):
                offer = {"item": "worn", "slot": slot, "equipped_slot": 0}
                if measured:
                    offer["known_equipment"] = {"enchantment": 0}
                turn = player_turn([cell("here")], inventory=[spare, worn], interactions={
                    "slots": [slot], "inventory": [offer, {"item": "spare", "slot": slot}],
                    "identification": {"choice": "lore", "classes": ["weapon"], "items": ["spare", "worn"]}})
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual((action.item.target, reason), (expected, "identify"))

    def test_mixed_identification_prompt_weighs_stack_size_before_worn_status(self):
        slot = {"id": "weapon", "name": "Weapon"}
        mace = {"id": "mace", "identity": "item.weapon.mace", "class": "weapon",
                "name": "mace", "identified": True, "quantity": 1}
        potion = {"id": "flask", "class": "potion", "name": "flask", "identified": False}
        interactions = {
            "slots": [slot],
            "inventory": [{"item": "mace", "slot": slot, "equipped_slot": 0}],
            "identification": {"choice": "lore", "classes": ["potion", "weapon"], "items": ["flask", "mace"]},
        }
        for quantity, expected in ((1, "mace"), (2, "flask")):
            with self.subTest(quantity=quantity):
                turn = player_turn([cell("here")], inventory=[dict(potion, quantity=quantity), mace],
                                   interactions=interactions)
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual((action.item.target, reason), (expected, "identify"))

    def test_equally_useful_identification_targets_follow_inventory_order(self):
        potion = {"class": "potion", "identified": False, "quantity": 2}
        blue = {**potion, "id": "blue", "name": "blue flask"}
        red = {**potion, "id": "red", "name": "red flask"}
        prompt = {"choice": "lore", "classes": ["potion"], "items": ["red", "blue"]}
        for first, second in ((blue, red), (red, blue)):
            with self.subTest(first=first["id"]):
                turn = player_turn([cell("here")], inventory=[first, second], interactions={"identification": prompt})
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual((action.item.target, reason), (first["id"], "identify"))

    def test_identification_prompt_with_no_useful_offers_declines_the_choice(self):
        known = {"id": "band", "name": "band", "class": "ring", "identity": "item.ring.ring-searching",
                 "quantity": 1, "identified": True}
        unknown = {"id": "wand", "name": "staff", "class": "wand", "quantity": 1, "identified": False}
        for offered in ([], ["band"]):
            with self.subTest(offered=offered):
                turn = player_turn([cell("here")], inventory=[known, unknown], interactions={
                    "identification": {"choice": "current", "items": offered, "classes": ["ring", "wand"]}})
                action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
                self.assertEqual((action.choice, action.item, reason), ("current", None, "identify"))

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

    def test_identification_scroll_is_saved_for_usable_or_worn_armor(self):
        scroll = {"id": "scroll", "name": "robes", "class": "scroll", "identified": True,
                  "identity": "item.scroll.identify-armor", "quantity": 1}
        armor = {"id": "mail", "name": "mail", "class": "armor", "identified": True,
                 "identity": "item.armor.ring-mail", "quantity": 1}
        slot = {"id": "body_armor", "name": "Armor"}
        for equipped, reason in ((None, "descend"), (0, "identify_gear")):
            with self.subTest(equipped=equipped):
                offers = [{"item": "scroll", "readable": True},
                          {"item": "mail", "slot": slot, "known_cursed": True, "equipped_slot": equipped}]
                turn = player_turn([cell("exit", down=True)], inventory=[armor, scroll],
                                   interactions={"inventory": offers, "slots": [slot]})
                self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)[1], reason)

        spare = {**armor, "id": "spare"}
        prompt = {"choice": "identify", "items": ["mail", "spare"], "classes": ["armor"]}
        turn = player_turn([cell("here")], inventory=[armor, spare], interactions={
            "identification": prompt, "inventory": [{"item": "mail", "known_cursed": True}]})
        action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
        self.assertEqual((action.item.target, reason), ("spare", "identify"))

    def test_identification_uses_weapon_slot_evidence_for_scenario_gear(self):
        from rogue_gear import needs_identification

        slot = {"id": "weapon", "name": "Weapon"}
        blade = {"id": "blade", "identity": "fixture.blade", "class": "weapon", "name": "blade",
                 "quantity": 1, "identified": True}
        for offer, expected in (({}, False), ({"slot": slot}, True),
                                ({"slot": slot, "known_cursed": True}, False),
                                ({"slot": slot, "known_cursed": True, "equipped_slot": 0}, True)):
            with self.subTest(offer=offer):
                turn = player_turn([cell("here")], inventory=[blade], interactions={
                    "slots": [slot], "inventory": [{"item": "blade", **offer}]})
                self.assertEqual(needs_identification(turn.inventory[0], turn), expected)

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

    def test_failed_search_reopens_an_exhausted_site_for_the_final_unseen_search(self):
        from tor_client_bot import Result

        turn = search_turn([cell("here")])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        memory = player.maps[player.depth]
        history = player.search_history
        receipt = Result.from_reply({"status": "confirmed", "receipt": search_activity()})
        history.admit(turn, turn.cell, receipt, memory)
        def sites():
            return list(history.ranked_search_sites(turn, memory, returning=False, allow_unseen=True, wall_limit=0))

        self.assertEqual(sites(), [])
        history.observe(Turn({**json.loads(turn.recording()), "finished_intentions": [search_activity("failed")]}))
        self.assertEqual(sites(), [{"here"}])

    def test_resolved_stair_search_counts_as_an_attempt_without_spending_a_dead_end_search(self):
        from tor_client_bot import Result

        walls = [cell("north", y=-1, wall=True), cell("east", 1, wall=True), cell("south", y=1, wall=True)]
        ability = {"id": "rogue.search", "name": "Search", "cost": None, "cooldown_remaining": 0,
                   "basic_attack": False, "target_kind": "self_only", "reach": {"kind": "self_only"}}
        receipt = Result.from_reply({"status": "confirmed", "receipt": search_activity()})
        for up in (True, False):
            with self.subTest(up=up):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                turn = player_turn([cell("stairs", up=up, down=not up), cell("hall", -1), *walls], abilities=[ability])
                player.observe(turn)
                player.admitted(turn, UseAbility(turn.abilities[0]), receipt)
                player.observe(Turn({**json.loads(turn.recording()), "finished_intentions": [
                    search_activity("resolved")]}))
                self.assertEqual(player.search_history.admissions["stairs"], 1)
                self.assertEqual(player.search_history.completed_end_searches["stairs"], 0)

    def test_immediate_searches_exhaust_the_dead_end_allowance(self):
        from tor_client_bot import Result

        turn = search_turn([cell("end"), cell("hall", -1), cell("east", 1, wall=True),
                            cell("north", y=-1, wall=True), cell("south", y=1, wall=True)])
        for receipt in (None, Result.from_reply({"status": "confirmed"})):
            with self.subTest(receipt=receipt):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                for _ in range(6):
                    action, reason = choose(player, turn)
                    self.assertEqual(reason, "search")
                    player.admitted(turn, action, receipt, reason=reason)
                self.assertEqual(choose(player, turn)[1], "explore")
                self.assertEqual(player.search_history.wall_searches["east"], 6)

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
        outcomes = ["failed", "cancelled"] + ["resolved"] * 6
        for index, phase in enumerate(outcomes):
            action, reason = choose(player, turn)
            self.assertEqual(reason, "search")
            activity = {"type": "admitted", "actor": 1, "branch": "game", "intention": str(index), "phase": "queued"}
            receipt = Result.from_reply({"status": "confirmed", "receipt": activity})
            player.admitted(turn, action, receipt)
            picture = json.loads(turn.recording())
            picture["finished_intentions"] = [{**activity, "phase": phase}]
            turn = Turn(picture)
            # Reobserving the same outcome must not credit the dead end twice.
            player.observe(turn)
            player.observe(turn)
            self.assertEqual(player.search_history.admissions["end"], index + 1)
            self.assertEqual(player.search_history.completed_end_searches["end"], outcomes[:index + 1].count("resolved"))
        self.assertEqual(choose(player, turn)[1], "explore")
        player.search_history.completed_end_searches["end"] = 1
        walls[1]["wall"] = False
        self.assertEqual(choose(player, player_turn(room, abilities=[ability]))[1], "explore")

    def test_eats_ordinary_food_when_the_ruleset_omits_the_ration_tag(self):
        meal = {"id": "meal", "name": "fixture meal", "class": "food", "quantity": 1, "identified": False}
        stone = {**meal, "id": "stone", "name": "stone food"}
        turn = player_turn([cell("here"), cell("frontier", 1)], inventory=[stone, meal],
                           stats={"nutrition": {"hunger": "hungry"}}, interactions={"inventory": [
                               {"item": "stone", "edible": False}, {"item": "meal", "edible": True}]})
        action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
        self.assertEqual((action.item.target, reason), ("meal", "eat"))

    def test_only_urgent_hunger_interrupts_a_corridor_stand_for_a_meal(self):
        meal = {"id": "packed", "name": "ration", "class": "food", "quantity": 1, "identified": False}
        for hunger, reason in (("nourished", "hold_corridor"), ("hungry", "hold_corridor"),
                               ("weak", "eat"), ("empty", "eat")):
            with self.subTest(hunger=hunger):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, corridor_stand=("here", 100))
                turn = player_turn([cell("here")], inventory=[meal], stats={"nutrition": {"hunger": hunger}},
                                   interactions={"inventory": [{"item": "packed", "edible": True}]})
                self.assertEqual(choose(player, turn)[1], reason)

    def test_hungry_bot_eats_a_carried_meal_before_picking_up_another(self):
        here = cell("here")
        meal = {"id": "packed", "name": "meal", "class": "food", "quantity": 1, "identified": False}
        turn = player_turn([here], inventory=[meal],
                           ground=[{**here, "item": {**meal, "id": "spare"}, "reachable": True}],
                           stats={"nutrition": {"hunger": "hungry"}},
                           interactions={"inventory": [{"item": "packed", "edible": True}]})
        action, reason = choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn)
        self.assertEqual((action.item.target, reason), ("packed", "eat"))

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

    def test_unreachable_food_on_the_current_cell_does_not_hide_a_distant_ration(self):
        track = [cell("here"), cell("hall", 1), cell("ration", 2)]
        ground = [{**site, "item": {"id": site["key"], "name": "meal", "class": "food",
                                    "quantity": 1, "identified": False}, "reachable": False}
                  for site in (track[0], track[-1])]
        for hunger, reason in (("weak", "forage"), ("nourished", "seek_item")):
            with self.subTest(hunger=hunger):
                turn = player_turn(track, ground=ground, stats={"nutrition": {"hunger": hunger}})
                self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn),
                                 (Move(Direction.EAST), reason))

    def test_foraging_prefers_a_safe_ration_but_can_use_the_only_known_route(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        cells = [cell("near", -1), cell("here"), cell("passage", 1), cell("far", 2)]
        meal = {"id": "ration", "name": "ration", "class": "food", "quantity": 1, "identified": False}
        near = {**cells[0], "item": meal, "reachable": False}
        far = {**cells[-1], "item": {**meal, "id": "other"}, "reachable": False}
        hungry = {"nutrition": {"hunger": "weak"}}
        player.danger_until["near"] = 1200
        for ground, expected in (([near, far], Direction.EAST), ([near], Direction.WEST)):
            with self.subTest(direction=expected):
                turn = player_turn(cells, ground=ground, stats=hungry)
                self.assertEqual(choose(player, turn), (Move(expected), "forage"))

    def test_foraging_among_warned_food_cells_still_prefers_the_untrapped_route(self):
        cells = [cell("near", -1), cell("here"), cell("passage", 1), cell("far", 2)]
        cells[0]["trap"] = {"id": "pit", "family": None, "reachable": True}
        ground = [{**tile, "item": {"id": tile["key"], "name": "ration", "class": "food",
                                    "quantity": 1, "identified": False}, "reachable": False}
                  for tile in (cells[0], cells[-1])]
        turn = player_turn(cells, ground=ground, stats={"nutrition": {"hunger": "weak"}})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        player.danger_until.update(near=1200, far=1200)
        memory = player.maps[player.depth]
        with patch.object(memory, "toward", wraps=memory.toward) as route:
            self.assertEqual(player.choose(turn), (Move(Direction.EAST), "forage"))
            route.assert_called_once()

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

    def test_leaves_an_exposed_square_before_opening_a_door_to_recovery_shelter(self):
        from tor_client_bot import OpenDoor

        for height in (0, 1):
            with self.subTest(door_height=height):
                tiles = [cell(str(x), x) for x in range(-3, 3)]
                doorway = next(tile for tile in tiles if tile["key"] == "1")
                if height:
                    doorway = cell("upper-gate", 1)
                    doorway["position"]["z"] = height
                    tiles.append(doorway)
                doorway["door"] = {"id": "gate", "open": False, "reachable": True}
                body = {"id": "player", "name": "you", "position": {"x": 0, "y": 0, "z": 1}}
                turn = player_turn(tiles, actors=[body], combat={"hp": 6}, stats={"health_recovery": {
                    "period_ticks": 1000, "elapsed_ticks": 0, "reset_percent": 100, "stopped": False}})
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observe(turn)
                player.danger_until["0"] = 1200
                player.maps[1].visited.add("2")
                gate = turn.cell_at((1, 0, height)).door
                self.assertEqual(player.choose(turn), (OpenDoor(gate), "reposition_to_recover"))
                player.maps[1].visited.add("-3")
                self.assertEqual(player.choose(turn), (Move(Direction.WEST), "reposition_to_recover"))

    def test_recovery_walks_a_familiar_stair_route_around_a_closed_door(self):
        tiles = [cell(str(x), x, down=x in (-3, 2)) for x in range(-3, 3)]
        doorway = next(tile for tile in tiles if tile["key"] == "1")
        doorway["door"] = {"id": "gate", "open": False, "reachable": True}
        turn = player_turn(tiles, combat={"hp": 6}, stats={"health_recovery": {
            "period_ticks": 1000, "elapsed_ticks": 0, "reset_percent": 100, "stopped": False}})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        player.maps[1].visited.update({"1", "-1", "-2"})
        self.assertEqual(player.choose(turn), (Move(Direction.WEST), "recover_en_route"))
        player.danger_until["-1"] = 1200
        self.assertEqual(player.choose(turn), (Wait(), "recover"))

    def test_recovery_prefers_a_clear_route_to_shelter_over_a_shorter_dangerous_route(self):
        turn = player_turn([cell(str(x), x) for x in range(-3, 3)], combat={"hp": 6},
                           stats={"health_recovery": {"period_ticks": 1000, "elapsed_ticks": 0,
                                                      "reset_percent": 100, "stopped": False}})
        for clear_shelter, expected in ((True, Direction.WEST), (False, Direction.EAST)):
            with self.subTest(clear_shelter=clear_shelter):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observe(turn)
                player.maps[1].visited.add("2")
                if clear_shelter:
                    player.maps[1].visited.add("-3")
                player.danger_until.update({"0": 1200, "1": 1200})
                self.assertEqual(player.choose(turn), (Move(expected), "reposition_to_recover"))

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

    def test_recovery_avoids_a_known_pit_between_the_player_and_familiar_ground(self):
        pit = {"id": "pit", "family": None, "reachable": True}
        hall = [cell("here"), {**cell("passage", 1), "trap": pit}, cell("stairs", 2, down=True)]
        stats = {"health_recovery": {"period_ticks": 1000, "elapsed_ticks": 0, "reset_percent": 100,
                                     "stopped": False}}
        turn = player_turn(hall, combat={"hp": 6}, stats=stats)
        for exposed in (False, True):
            with self.subTest(exposed=exposed):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observe(turn)
                player.maps[1].visited.update({"passage", "stairs"})
                if exposed:
                    player.danger_until["here"] = 900
                self.assertEqual(player.choose(turn), (Wait(), "recover"))

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
        player.travel_times.ascent[1] = scarce.nutrition.ticks_until_empty
        self.assertIsNone(player.recovery_action(scarce))
        player.travel_times.ascent.clear()
        player.unsafe_stairs["stairs", Direction.DOWN] = turn.tick
        from unittest.mock import patch
        with patch.object(player, "can_afford_wait", side_effect=AssertionError("walking needs no rest budget")):
            self.assertEqual(player.choose(scarce), (Move(Direction.EAST), "recover_en_route"))
        player.danger_until["hall"] = 1200
        self.assertEqual(player.choose(turn), (Wait(), "recover"))
        on_stairs = player_turn([cell("stairs", down=True)], combat={"hp": 6}, stats=stats)
        self.assertEqual(choose(player, on_stairs), (Wait(), "recover"))

    def test_recovery_on_the_retrieval_floor_preserves_food_for_the_return(self):
        room = [cell("here", up=True), cell("down", 1, down=True)]
        stats = {
            "health_recovery": {"period_ticks": 1000, "elapsed_ticks": 0, "reset_percent": 100, "stopped": False},
            "nutrition": {"kind": "required", "reserve": 50, "maximum": 2000, "unit_ticks": 100,
                          "hunger": "nourished", "elapsed": 0},
        }
        turn = player_turn(room, combat={"hp": 6}, stats=stats)
        player = Adventurer(3, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
        self.assertEqual(choose(player, turn), (Wait(), "recover"))
        player.travel_times.ascent[2] = turn.nutrition.ticks_until_empty
        self.assertIsNone(player.recovery_action(turn))

    def test_finishes_recovery_before_exposing_itself_to_more_combat(self):
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        room = [cell("here"), cell("frontier", 1)]
        stats = {"health_recovery": {"period_ticks": 1000, "elapsed_ticks": 0, "reset_percent": 100,
                                     "stopped": False}}
        for hp in (4, 6, 9):
            turn = player_turn(room, combat={"hp": hp}, stats=stats)
            expected = (Wait(), "recover") if hp < 8 else (Move(Direction.EAST), "explore")
            self.assertEqual(choose(player, turn), expected)

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

    def test_meal_measurement_follows_the_admitted_eating_activity(self):
        from tor_client_bot import ItemUse, Result, UseItem

        ration = {"id": "ration", "identity": "fixture.ration", "name": "ration", "class": "food",
                  "quantity": 1, "identified": True}
        nutrition = {"kind": "required", "unit_ticks": 100, "elapsed": 0, "maximum": 100,
                     "reserve": 10, "hunger": "weak"}
        before = player_turn([cell("here")], inventory=[ration], stats={"nutrition": nutrition},
                             interactions={"inventory": [{"item": "ration", "edible": True}]})
        after = player_turn([cell("here")], tick=100, stats={"nutrition": dict(nutrition, reserve=40)})
        activity = {"type": "admitted", "actor": 1, "branch": "game", "intention": "eat", "phase": "queued"}
        receipt = Result.from_reply({"status": "confirmed", "receipt": activity})
        for phase in ("resolved", "failed", "cancelled", None):
            with self.subTest(phase=phase):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.admitted(before, UseItem(before.inventory[0], ItemUse.EAT), receipt)
                # The ready picture at admission can be repeated before the receipt becomes visible.
                player.observe(before)
                self.assertIsNotNone(player.meals.pending)
                paused = Turn({**json.loads(before.recording()), "intentions": [dict(activity, phase="paused")]})
                player.observe(paused)
                self.assertIsNotNone(player.meals.pending)
                history = [dict(activity, phase=phase)] if phase else []
                finished = Turn({**json.loads(after.recording()), "finished_intentions": history})
                player.observe(finished)
                self.assertIsNone(player.meals.pending)
                self.assertEqual(player.meals.gains, {"fixture.ration": 30} if phase == "resolved" else {})

    def test_meal_without_an_activity_survives_repeated_and_older_observations(self):
        from tor_client_bot import Result

        ration = {"id": "food", "identity": "fixture.ration", "name": "ration", "class": "food",
                  "quantity": 1, "identified": True}
        nutrition = {"kind": "required", "unit_ticks": 100, "elapsed": 0, "maximum": 100,
                     "reserve": 10, "hunger": "weak"}
        before = player_turn([cell("here")], tick=10, inventory=[ration], stats={"nutrition": nutrition},
                             interactions={"inventory": [{"item": "food", "edible": True}]})
        def consumed(tick, reserve):
            return player_turn([cell("here")], tick=tick, stats={"nutrition": dict(nutrition, reserve=reserve)})

        for receipt in (None, Result.from_reply({"status": "confirmed"})):
            with self.subTest(receipt=receipt):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                action, reason = choose(player, before)
                player.admitted(before, action, receipt, reason=reason)
                player.observe(consumed(9, 25))
                player.observe(before)
                self.assertEqual(player.meals.gains, {})
                player.observe(consumed(10, 40))
                self.assertEqual(player.meals.gains, {"fixture.ration": 30})

    def test_an_unidentified_meal_replaces_an_unfinished_measurement(self):
        from tor_client_bot import ItemUse, UseItem

        known = {"id": "ration", "identity": "fixture.ration", "name": "ration", "class": "food",
                 "quantity": 1, "identified": True}
        unknown = {"id": "fruit", "name": "fruit", "class": "food", "quantity": 1, "identified": False}
        nutrition = {"kind": "required", "unit_ticks": 100, "elapsed": 0, "maximum": 100,
                     "reserve": 10, "hunger": "weak"}
        turn = player_turn([cell("here")], inventory=[known, unknown], stats={"nutrition": nutrition},
                           interactions={"inventory": [{"item": key, "edible": True} for key in ("ration", "fruit")]})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        ration, fruit = turn.inventory
        player.admitted(turn, UseItem(ration, ItemUse.EAT))
        self.assertIsNotNone(player.meals.pending)
        player.admitted(turn, UseItem(fruit, ItemUse.EAT))
        player.observe(player_turn([cell("here")], stats={"nutrition": dict(nutrition, reserve=40)}))
        self.assertEqual(player.meals.gains, {})

    def test_resolved_meal_is_measured_before_recovery_drains_the_reserve(self):
        from tor_client_bot import ItemUse, Result, UseItem

        ration = {"id": "ration", "identity": "fixture.ration", "name": "ration", "class": "food",
                  "quantity": 1, "identified": True}
        nutrition = {"kind": "required", "unit_ticks": 100, "elapsed": 0, "maximum": 100,
                     "reserve": 10, "hunger": "weak"}
        before = player_turn([cell("here")], inventory=[ration], stats={"nutrition": nutrition},
                             interactions={"inventory": [{"item": "ration", "edible": True}]})
        activity = {"type": "admitted", "actor": 1, "branch": "game", "intention": "eat", "phase": "queued"}
        receipt = Result.from_reply({"status": "confirmed", "receipt": activity})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.admitted(before, UseItem(before.inventory[0], ItemUse.EAT), receipt)
        after = player_turn([cell("here")], tick=100, stats={"nutrition": dict(nutrition, reserve=40)})
        picture = json.loads(after.recording())
        picture["readiness"] = {"admission": False}
        picture["finished_intentions"] = [dict(activity, phase="resolved")]
        partial = json.loads(player_turn([cell("here")], tick=100).recording())
        partial.update(readiness=picture["readiness"], finished_intentions=picture["finished_intentions"])
        player.observe(Turn(partial))
        self.assertIsNotNone(player.meals.pending)
        # A later observation can disclose nutrition after the finished receipt has aged out.
        picture["finished_intentions"] = []
        player.observe(Turn(picture))
        player.observe(player_turn([cell("here")], tick=200, stats={"nutrition": dict(nutrition, reserve=39)}))
        self.assertEqual(player.meals.gains, {"fixture.ration": 30})

    def test_food_estimate_uses_the_lowest_observed_gain_after_consumption(self):
        from tor_client_bot import ItemUse, UseItem

        identity = "item.food.ration"
        ration = {"id": "ration", "name": "ration", "identity": identity, "class": "food", "identified": True}
        def picture(count, reserve, *, preparing=False):
            nutrition = {"kind": "required", "unit_ticks": 100, "elapsed": 0, "maximum": 1000,
                         "hunger": "nourished", "reserve": reserve}
            pack = [dict(ration, quantity=count)] if count else []
            offers = [{"item": "ration", "edible": True}] if count else []
            return player_turn([cell("here")], inventory=pack, stats={"nutrition": nutrition},
                               interactions={"inventory": offers}, combat={"preparation_remaining": 1 if preparing else None})

        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        observations = (
            (100, 2, 150, None),  # The stack did not shrink.
            (100, 0, 220, None),  # Losing two rations cannot establish the gain from one.
            (100, 1, 95, None),
            (990, 1, 1000, None),  # The reserve cap hides the full gain.
            (100, 1, 160, 60),
            (995, 1, 1000, 60),
            (100, 1, 140, 40),
            (100, 1, 170, 40),
        )
        for starting_reserve, count, reserve, expected in observations:
            with self.subTest(starting_reserve=starting_reserve, count=count, reserve=reserve):
                before = picture(2, starting_reserve)
                player.admitted(before, UseItem(before.inventory[0], ItemUse.EAT))
                previous = player.meals.gains.get(identity)
                player.observe(picture(1, starting_reserve + 1, preparing=True))
                self.assertEqual(player.meals.gains.get(identity), previous)
                player.observe(picture(count, reserve))
                self.assertEqual(player.meals.gains.get(identity), expected)

    def test_provisions_known_return_route_before_taking_stairs(self):
        nutrition = {"kind": "required", "unit_ticks": "100", "elapsed": "0",
                     "maximum": 1300, "hunger": "nourished"}
        ration = {"id": "ration", "class": "food", "name": "ration", "quantity": 1, "identified": False}
        relic = {"id": "objective", "identity": DEFAULT_OBJECTIVE_IDENTITY, "name": AMULET,
                 "identified": True, "quantity": 1}
        for carrying, direction, descent_history, required_reserve in (
            (True, Direction.UP, {1: 10000}, 400),
            (False, Direction.DOWN, {}, 400),
            (False, Direction.DOWN, {1: 10000}, 1300),
        ):
            with self.subTest(direction=direction, required_reserve=required_reserve):
                player = Adventurer(6, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
                player.travel_times.ascent = {1: 20000, 2: 20000}
                player.travel_times.descent = descent_history
                player.observe(player_turn([cell("passage"), cell("food", 1)], ground=[
                    {"item": ration, "reachable": False, "position": {"x": 1, "y": 0, "z": 0}},
                ]))
                for reserve, expected in [(required_reserve - 1, Direction.EAST), (required_reserve, direction)]:
                    turn = player_turn([cell("stairs", up=carrying, down=not carrying), cell("passage", 1)],
                                       inventory=[relic] if carrying else [],
                                       stats={"nutrition": dict(nutrition, reserve=reserve)})
                    self.assertEqual(choose(player, turn)[0], Move(expected))

    def test_provisions_for_a_previously_measured_floor_after_retreating(self):
        player = Adventurer(4, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
        player.travel_times.descent.update({1: 100, 2: 100, 3: 8000})
        player.travel_times.ascent.update({1: 100, 2: 100, 3: 100})
        for reserve, reason in ((82, "scout_for_food"), (83, "descend")):
            with self.subTest(reserve=reserve):
                turn = player_turn([cell("stairs", down=True), cell("passage", 1)], stats={"nutrition": {
                    "kind": "required", "reserve": reserve, "maximum": 1300, "unit_ticks": 100,
                    "hunger": "nourished", "elapsed": 0}})
                self.assertEqual(choose(player, turn)[1], reason)

    def test_journey_budget_combines_measured_floors_and_an_unexplored_floor(self):
        player = Adventurer(8, DEFAULT_OBJECTIVE_IDENTITY, depth=6)
        player.travel_times.descent.update({1: 9000, 2: 100, 3: 200, 4: 300, 5: 400, 6: 2000})
        player.travel_times.ascent.update({floor: 50 for floor in range(1, 7)})
        # Five shallower return legs, floor six's measured round trip, and
        # floor seven at the median round trip of the five deepest measured floors.
        self.assertEqual(player.estimated_journey_ticks(returning=False), 250 + 2050 + 350)
        self.assertEqual(player.estimated_journey_ticks(returning=True), 250)

    def test_round_trip_forecast_uses_available_timings_when_an_ascent_is_unknown(self):
        from rogue_food import estimate_round_trip_ticks

        # Of the two measured trips, only floor one has an observed return leg.
        # The lower median of 120 and 200 supplies the estimate for floor three.
        self.assertEqual(estimate_round_trip_ticks(2, 4, {1: 100, 2: 200}, {1: 20}), 200 + 120)
        self.assertEqual(estimate_round_trip_ticks(2, 4, {}, {1: 20}), 0)
        self.assertEqual(estimate_round_trip_ticks(4, 4, {1: 100}, {1: 20}), 0)

    def test_carried_meals_count_toward_the_scouting_budget(self):
        cells = [cell("stairs", down=True), cell("frontier", 1)]
        nutrition = {"kind": "required", "reserve": 100, "maximum": 1300,
                     "unit_ticks": 100, "elapsed": 0, "hunger": "nourished"}
        ration = {"id": "meal", "name": "ration", "class": "food", "identity": "item.food.ration",
                  "identified": True, "quantity": 2}
        for gain, elapsed, reason in (
            (49, 0, "scout_for_food"), (50, 0, "descend"), (50, 1, "scout_for_food"), (None, 0, "descend"),
        ):
            with self.subTest(gain=gain, elapsed=elapsed):
                player = Adventurer(6, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
                player.travel_times.ascent[2] = 20000
                if gain is not None:
                    player.meals.gains[ration["identity"]] = gain
                turn = player_turn(cells, stats={"nutrition": dict(nutrition, elapsed=elapsed)}, inventory=[ration],
                                   interactions={"inventory": [{"item": "meal", "edible": True}]})
                action, actual_reason = choose(player, turn)
                self.assertEqual(actual_reason, reason)
                if reason == "scout_for_food":
                    player.admitted(turn, action, reason=actual_reason)
                    self.assertEqual(player.food_scouts.by_floor[3].meal_baseline, 2)

    def test_food_scout_prefers_a_clear_frontier_over_a_recent_threat(self):
        cells = [cell("stairs", down=True), cell("near", -1), cell("hall", 1), cell("far", 2)]
        turn = player_turn(cells, tick=100, stats={"nutrition": {
            "kind": "required", "reserve": 400, "maximum": 2000, "unit_ticks": 100,
            "hunger": "nourished", "elapsed": 0}})
        player = Adventurer(6, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
        player.travel_times.ascent[2] = 50000
        player.observe(turn)
        player.maps[3].visited.add("hall")
        player.danger_until["near"] = 900
        self.assertEqual(player.choose(turn), (Move(Direction.EAST), "scout_for_food"))
        player.maps[3].visited.add("far")
        self.assertEqual(player.choose(turn), (Move(Direction.WEST), "scout_for_food"))

    def test_food_scout_stays_off_a_known_pit_on_the_way_to_an_unexplored_square(self):
        stairs, passage, frontier = cell("stairs", down=True), cell("passage", 1), cell("frontier", 2)
        nutrition = {"kind": "required", "reserve": 400, "maximum": 2000, "unit_ticks": 100,
                     "hunger": "nourished", "elapsed": 0}
        for trap, expected in ((None, (Move(Direction.EAST), "scout_for_food")),
                               ({"id": "pit", "family": None, "reachable": True}, (Move(Direction.DOWN), "descend"))):
            with self.subTest(trap=trap):
                turn = player_turn([stairs, {**passage, "trap": trap}, frontier], stats={"nutrition": nutrition})
                player = Adventurer(6, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
                player.travel_times.ascent[2] = 50000
                player.observe(turn)
                player.maps[3].visited.add("passage")
                action, reason = player.choose(turn)
                self.assertEqual((action, reason), expected)
                self.assertNotIn(3, player.food_scouts.by_floor)
                player.admitted(turn, action, reason=reason)
                self.assertEqual(3 in player.food_scouts.by_floor, trap is None)

    def test_injured_food_scout_keeps_moving_until_the_scouting_budget_expires(self):
        player = Adventurer(6, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
        cells = [cell("hall"), cell("stairs", -1, down=True), cell("frontier", 1)]
        stats = {"health_recovery": {"period_ticks": 1000, "elapsed_ticks": 0, "reset_percent": 100,
                                     "stopped": False},
                 "nutrition": {"kind": "required", "reserve": 400, "maximum": 2000, "unit_ticks": 100,
                               "hunger": "nourished", "elapsed": 0}}
        player.travel_times.ascent[2] = 50000
        player.food_scouts.by_floor[3] = FoodScout(deadline=1000, meal_baseline=0)
        for tick, expected in [(100, (Move(Direction.EAST), "scout_for_food")),
                               (1000, (Move(Direction.WEST), "recover_en_route"))]:
            with self.subTest(tick=tick):
                turn = player_turn(cells, tick=tick, combat={"hp": 6}, stats=stats)
                player.observe(turn)
                player.maps[3].visited.add("stairs")
                self.assertEqual(player.choose(turn), expected)

    def test_leaving_an_unscouted_floor_preserves_its_first_detour(self):
        scouts = FoodScoutHistory()
        stats = {"nutrition": {"kind": "required", "reserve": 4, "maximum": 100, "unit_ticks": 100,
                               "hunger": "weak", "elapsed": 0}}
        hall = player_turn([cell("hall")], tick=100, stats=stats)
        stairs = player_turn([cell("stairs", down=True)], tick=200, stats=stats)
        self.assertFalse(scouts.can_explore(3, hall))
        scouts.start_if_new(3, hall)
        scouts.finish(3)
        self.assertTrue(scouts.can_explore(3, stairs))
        self.assertNotIn(3, scouts.by_floor)

    def test_new_food_scout_requires_at_least_one_tick_of_detour_budget(self):
        for reserve_ticks in (3, 4, 7, 8):
            with self.subTest(reserve_ticks=reserve_ticks):
                nutrition = {"kind": "required", "reserve": 1, "maximum": 100, "unit_ticks": 10,
                             "hunger": "weak", "elapsed": 10 - reserve_ticks}
                turn = player_turn([cell("stairs", down=True)], tick=100, stats={"nutrition": nutrition})
                scouts = FoodScoutHistory()
                self.assertEqual(scouts.can_explore(3, turn), reserve_ticks >= 4)
                scouts.start_if_new(3, turn)
                if reserve_ticks == 3:
                    self.assertNotIn(3, scouts.by_floor)
                else:
                    self.assertEqual(scouts.by_floor[3].deadline, 100 + reserve_ticks // 4)

    def test_food_scout_counts_edible_portions_in_a_stack(self):
        scout = FoodScout(deadline=100, meal_baseline=2)
        for quantity, edible, finished in ((2, True, False), (3, False, False), (3, True, True)):
            with self.subTest(quantity=quantity, edible=edible):
                ration = {"id": "ration", "name": "ration", "class": "food",
                          "quantity": quantity, "identified": False}
                turn = player_turn([cell("here")], inventory=[ration], tick=99,
                                   interactions={"inventory": [{"item": "ration", "edible": edible}]})
                self.assertEqual(scout.finished(turn), finished)

    def test_scout_recognizes_a_ration_that_replaces_an_eaten_meal(self):
        player = Adventurer(6, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
        player.food_scouts.by_floor[3] = FoodScout(deadline=1000, meal_baseline=1)
        here = cell("stairs", down=True)
        ration = {"id": "ration", "class": "food", "name": "ration", "identified": False, "quantity": 1}
        empty = player_turn([here], ground=[{**here, "item": ration, "reachable": True}])
        player.observe(empty)
        player.admitted(empty, Pickup(empty.ground_items[0].item), reason="take")
        player.observe(empty)
        self.assertIsNotNone(player.food_scouts.by_floor[3])
        stocked = player_turn([here], tick=100, inventory=[ration],
                              interactions={"inventory": [{"item": "ration", "edible": True}]})
        player.observe(stocked)
        self.assertIsNone(player.food_scouts.by_floor[3])

    def test_food_scout_stays_finished_after_eating_the_ration_it_found(self):
        player = Adventurer(6, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
        player.travel_times.ascent[2] = 50000
        cells = [cell("stairs", down=True), cell("frontier", 1)]
        stats = {"nutrition": {"kind": "required", "reserve": 400, "maximum": 2000,
                               "unit_ticks": 100, "hunger": "nourished", "elapsed": 0}}
        ration = {"id": "ration", "name": "ration", "class": "food", "quantity": 1, "identified": False}
        before = player_turn(cells, stats=stats)
        action, reason = choose(player, before)
        self.assertEqual((action, reason), (Move(Direction.EAST), "scout_for_food"))
        player.admitted(before, action, reason=reason)
        stocked = player_turn(cells, stats=stats, tick=100, inventory=[ration],
                              interactions={"inventory": [{"item": "ration", "edible": True}]})
        player.observe(stocked)
        after = player_turn(cells, stats=stats, tick=200)
        self.assertEqual(choose(player, after), (Move(Direction.DOWN), "descend"))

    def test_a_stair_round_trip_does_not_revive_the_departed_floors_food_scout(self):
        player = Adventurer(6, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
        player.travel_times.ascent[2] = 50000
        player.food_scouts.by_floor[3] = FoodScout(deadline=1000, meal_baseline=0)
        stairs = [cell("stairs", down=True), cell("passage", 1)]
        origin = player_turn(stairs)
        down, reason = choose(player, origin)
        self.assertEqual((down, reason), (Move(Direction.DOWN), "descend"))
        player.admitted(origin, down, reason=reason)
        player.observe(origin)
        self.assertIsInstance(player.food_scouts.by_floor[3], FoodScout)

        below = player_turn([cell("below", up=True)], tick=100)
        player.observe(below)
        self.assertEqual(player.depth, 4)
        self.assertNotIn(4, player.food_scouts.by_floor)
        player.admitted(below, Move(Direction.UP), reason="retreat_upstairs")
        revisit = player_turn(stairs, tick=200, stats={"nutrition": {
            "kind": "required", "reserve": 400, "maximum": 2000, "unit_ticks": 100,
            "hunger": "nourished", "elapsed": 0}})
        self.assertEqual(choose(player, revisit), (Move(Direction.DOWN), "descend"))
        self.assertEqual(player.depth, 3)

    def test_finished_food_scout_can_forage_again_when_a_ration_is_located(self):
        player = Adventurer(6, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
        player.travel_times.ascent[2] = 50000
        stairs, hall, shelf = cell("stairs", down=True), cell("hall", 1), cell("shelf", 2)
        ration = {"id": "food", "name": "ration", "class": "food", "quantity": 1, "identified": False}
        food = {**shelf, "item": ration, "reachable": False}
        stats = {"nutrition": {"kind": "required", "reserve": 400, "maximum": 2000,
                               "unit_ticks": 100, "hunger": "nourished", "elapsed": 0}}
        for tick, ground, reason in ((0, [], "scout_for_food"), (10000, [], None), (10100, [food], "forage")):
            with self.subTest(tick=tick):
                turn = player_turn([stairs, hall, shelf], stats=stats, tick=tick, ground=ground)
                player.observe(turn)
                decision = player.provision(turn, player.maps[3], returning=False)
                self.assertEqual(decision, (Move(Direction.EAST), reason) if reason else None)
                if decision:
                    player.admitted(turn, decision[0], reason=decision[1])

    def test_food_scout_finishes_once_the_bot_heads_back_to_the_stairs(self):
        player = Adventurer(6, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
        player.travel_times.ascent[2] = 50000
        player.food_scouts.by_floor[3] = FoodScout(deadline=1000, meal_baseline=0)
        cells = [cell("hall"), cell("stairs", -1, down=True), cell("frontier", 1)]
        turn = player_turn(cells, tick=100, stats={"nutrition": {
            "kind": "required", "reserve": 400, "maximum": 2000, "unit_ticks": 100,
            "hunger": "nourished", "elapsed": 0}})
        player.observe(turn)
        player.maps[3].visited.add("stairs")
        self.assertEqual(player.choose(turn), (Move(Direction.EAST), "scout_for_food"))
        player.last_exploration_origin = "frontier"
        player.admitted(turn, Move(Direction.WEST), reason="seek_stairs")
        self.assertIsNone(player.last_exploration_origin)
        self.assertEqual(player.choose(turn), (Move(Direction.WEST), "seek_stairs"))

    def test_opening_a_door_on_the_stair_route_finishes_the_food_scout(self):
        from tor_client_bot import OpenDoor

        player = Adventurer(6, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
        player.food_scouts.by_floor[3] = FoodScout(deadline=1000, meal_baseline=0)
        player.travel_times.ascent[2] = 50000
        door = {"id": "gate", "open": False, "reachable": True}
        cells = [cell("stairs", -2, down=True), {**cell("gate", -1), "door": door}, cell("hall"), cell("frontier", 1)]
        nutrition = {"kind": "required", "reserve": 500, "maximum": 2000, "unit_ticks": 100,
                     "hunger": "nourished", "elapsed": 0}
        turn = player_turn(cells, stats={"nutrition": nutrition})
        player.observe(turn)
        player.maps[3].visited.update(("gate", "stairs"))
        action, reason = player.choose(turn)
        self.assertEqual((action, reason), (OpenDoor(turn.cell_at((-1, 0, 0)).door), "seek_stairs"))
        player.admitted(turn, action, reason=reason)
        door["open"] = True
        later = player_turn(cells, tick=100, stats={"nutrition": {**nutrition, "reserve": 499}})
        self.assertEqual(choose(player, later), (Move(Direction.WEST), "seek_stairs"))

    def test_recovers_along_a_visited_food_route_when_rest_is_also_affordable(self):
        player = Adventurer(6, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
        cells = [cell("here"), cell("stairs", -1, down=True), cell("passage", 1), cell("frontier", 2)]
        stats = {"health_recovery": {"period_ticks": 1000, "elapsed_ticks": 0, "reset_percent": 100,
                                     "stopped": False},
                 "nutrition": {"kind": "required", "reserve": 400, "maximum": 2000,
                               "unit_ticks": 100, "hunger": "nourished", "elapsed": 0}}
        player.travel_times.descent[2] = 50000
        player.food_scouts.by_floor[3] = FoodScout(deadline=1000, meal_baseline=0)
        turn = player_turn(cells, tick=100, combat={"hp": 6}, stats=stats)
        player.observe(turn)
        player.maps[3].visited.update({"stairs", "passage"})
        with patch.object(player, "safe_stairs", side_effect=AssertionError("food route already supplies the step")):
            self.assertEqual(player.choose(turn), (Move(Direction.EAST), "scout_for_food"))
        player.danger_until["passage"] = 900
        self.assertEqual(player.choose(turn), (Wait(), "recover"))

    def test_injured_scout_resumes_its_food_route_after_levitation_expires(self):
        player = Adventurer(6, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
        player.travel_times.descent[2] = 50000
        player.food_scouts.by_floor[3] = FoodScout(deadline=1000, meal_baseline=0)
        cells = [cell("hall"), cell("stairs", -1, down=True), cell("passage", 1), cell("frontier", 2)]
        recovery = {"period_ticks": 1000, "elapsed_ticks": 0, "reset_percent": 100, "stopped": False}
        nutrition = {"kind": "required", "reserve": 400, "maximum": 2000, "unit_ticks": 100,
                     "hunger": "nourished", "elapsed": 0}
        levitation = {"condition": {"id": "rogue.levitation", "name": "Levitation"}, "phase": "active"}
        for tick, remaining, expected in (
            (100, 100, (Move(Direction.WEST), "recover_en_route")),
            (200, 0, (Move(Direction.EAST), "scout_for_food")),
        ):
            with self.subTest(remaining=remaining):
                turn = player_turn(cells, tick=tick, combat={"hp": 6}, stats={
                    "health_recovery": recovery, "nutrition": nutrition,
                    "conditions": [{**levitation, "remaining_ticks": remaining}],
                })
                player.observe(turn)
                player.maps[3].visited.update({"stairs", "passage"})
                self.assertEqual(player.choose(turn), expected)

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
        player.search_history.admissions["search-site"] = 3
        self.assertEqual(player.choose(turn), (Move(Direction.SOUTH), "explore"))


    def test_return_search_uses_another_stair_passage_after_exhausting_one_boundary(self):
        cells = [cell(str(x), x, wall=abs(x) == 2, up=abs(x) == 3) for x in range(-3, 4)]
        ability = {"id": "rogue.search", "name": "Search", "cooldown_remaining": "0", "cost": None,
                   "basic_attack": False, "reach": {"kind": "personal"}, "target_kind": "self"}
        for exhausted, expected in (("-1", Direction.EAST), ("1", Direction.WEST)):
            with self.subTest(exhausted=exhausted):
                turn = player_turn(cells, abilities=[ability])
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.observe(turn)
                player.search_history.admissions[exhausted] = 3
                memory = player.maps[player.depth]
                self.assertEqual(player.search_return_connection(turn, memory, memory.stairs(Direction.UP)),
                                 (Move(expected), "search"))

    def test_return_route_keeps_a_sealed_boundary_until_six_searches_have_completed(self):
        cells = [cell("hall"), cell("end", 1), cell("barrier", 2, wall=True), cell("stairs", 3, up=True),
                 cell("north-wall", 1, -1, wall=True), cell("south-wall", 1, 1, wall=True)]
        ability = {"id": "rogue.search", "name": "Search", "cooldown_remaining": "0", "cost": None,
                   "basic_attack": False, "reach": {"kind": "personal"}, "target_kind": "self"}
        turn = player_turn(cells, abilities=[ability])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        memory = player.maps[player.depth]
        player.search_history.admissions["end"] = 7
        for completed, expected in ((5, (Move(Direction.EAST), "search")), (6, None)):
            with self.subTest(completed=completed):
                player.search_history.completed_end_searches["end"] = completed
                self.assertEqual(player.search_return_connection(turn, memory, {"stairs"}), expected)

    def test_stair_in_a_sealed_corridor_uses_the_ordinary_return_search_limit(self):
        cells = [cell("hall"), cell("landing", 1, down=True), cell("barrier", 2, wall=True), cell("exit", 3, up=True),
                 cell("north-wall", 1, -1, wall=True), cell("south-wall", 1, 1, wall=True)]
        turn = player_turn(cells)
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        memory = player.maps[player.depth]
        for attempts, expected in ((2, {"landing"}), (3, set())):
            with self.subTest(attempts=attempts):
                player.search_history.admissions["landing"] = attempts
                self.assertEqual(player.search_history.return_connection_sites(memory, "hall", {"exit"}), expected)

    def test_return_search_ignores_walls_around_an_already_connected_stair(self):
        cells = [cell("open-stair", -1, up=True), cell("here"), cell("hall", 1),
                 cell("wall", 2, wall=True), cell("lost-stair", 3, up=True),
                 cell("side-wall", -1, 1, wall=True), cell("other-side", 0, 1)]
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(player_turn(cells))
        memory = player.maps[player.depth]
        self.assertEqual(player.search_history.return_connection_sites(memory, "here", memory.stairs(Direction.UP)),
                         {"hall"})

    def test_return_search_traverses_a_shared_stair_passage_once(self):
        from unittest.mock import patch

        cells = [cell("here"), {**cell("wall", 1), "wall": True},
                 cell("first", 2, up=True), cell("second", 3, up=True),
                 cell("other-wall", 4, wall=True), cell("isolated", 5)]
        ability = {"id": "rogue.search", "name": "Search", "cooldown_remaining": "0", "cost": None,
                   "basic_attack": False, "reach": {"kind": "personal"}, "target_kind": "self"}
        turn = player_turn(cells, abilities=[ability])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        memory = player.maps[player.depth]
        upstairs = memory.stairs(Direction.UP)
        with patch.object(memory, "reachable", wraps=memory.reachable) as traverse:
            self.assertEqual(player.search_return_connection(turn, memory, upstairs),
                             (UseAbility(turn.abilities[0]), "search"))
            self.assertEqual(traverse.call_count, 1)
        self.assertEqual(upstairs, {"first", "second"})

    def test_returning_player_saves_teleport_when_the_current_upstairs_warning_will_expire(self):
        items = [{"id": "objective", "identity": DEFAULT_OBJECTIVE_IDENTITY, "name": AMULET,
                  "quantity": 1, "identified": True},
                 {"id": "scroll", "identity": "item.scroll.teleportation", "name": "Wayfarer",
                  "quantity": 1, "class": "scroll", "identified": True}]
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=4)
        player.unsafe_stairs["here", Direction.UP] = 100
        for tick, expected in ((99, (Wait(), "wait_for_safe_ascent")), (100, (Move(Direction.UP), "ascend"))):
            with self.subTest(tick=tick):
                turn = player_turn([cell("here", up=True)], tick=tick, inventory=items,
                                   interactions={"inventory": [{"item": "scroll", "readable": True}]})
                self.assertEqual(choose(player, turn), expected)

    def test_waiting_for_safe_ascent_reserves_food_for_the_return_trip(self):
        amulet = {"id": "objective", "identity": DEFAULT_OBJECTIVE_IDENTITY, "name": AMULET,
                  "quantity": 1, "identified": True}
        for elapsed, expected in ((0, (Wait(), "wait_for_safe_ascent")), (1, (Move(Direction.EAST), "explore"))):
            with self.subTest(elapsed=elapsed):
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=3)
                player.unsafe_stairs["stairs", Direction.UP] = 99
                player.travel_times.ascent[1] = 100
                nutrition = {"kind": "required", "reserve": 2, "maximum": 20, "unit_ticks": 100,
                             "elapsed": elapsed, "hunger": "nourished"}
                turn = player_turn([cell("stairs", up=True), cell("passage", 1)],
                                   inventory=[amulet], stats={"nutrition": nutrition})
                self.assertEqual(choose(player, turn), expected)

    def test_uses_known_teleportation_when_the_remembered_return_route_is_disconnected(self):
        items = [{"id": "objective", "identity": DEFAULT_OBJECTIVE_IDENTITY, "name": AMULET,
                  "quantity": 1, "identified": True},
                 {"id": "unknown", "name": "Faded parchment", "quantity": 1,
                  "class": "scroll", "identified": False},
                 {"id": "scroll", "name": "Wayfarer", "identity": "item.scroll.teleportation",
                  "quantity": 1, "class": "scroll", "identified": True}]
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=4)
        player.observe(player_turn([cell("lost-stair", up=True)]))
        def picture(tiles, readable=("unknown", "scroll")):
            offers = [{"item": name, "readable": True} for name in readable]
            return player_turn(tiles, inventory=items, interactions={"inventory": offers})
        turn = picture([cell("here")])
        action, reason = choose(player, turn)
        self.assertEqual((action.item.target, reason), ("scroll", "find_return_route"))
        action, reason = choose(player, picture([cell("here")], readable=("unknown",)))
        self.assertEqual((action.item.target, reason), ("unknown", "test_unknown_magic"))
        self.assertEqual(choose(player, picture([cell("here"), cell("frontier", 1)])), (Move(Direction.EAST), "explore"))

    def test_recovery_stair_step_distinguishes_unfamiliar_ground_from_recent_danger(self):
        from rogue_retreat import recovery_stair_step

        turn = player_turn([cell("here"), cell("passage", 1), cell("stairs", 2, down=True)])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        memory = player.maps[1]
        self.assertIsNone(recovery_stair_step(turn, memory, {"stairs"}, {"elsewhere"}))
        self.assertEqual(recovery_stair_step(turn, memory, {"stairs"}, {"passage"}), Move(Direction.EAST))

    def test_recovery_keeps_moving_toward_stairs_when_a_familiar_detour_is_clear(self):
        cells = [cell(str(x), x, down=x in (-3, 2)) for x in range(-3, 3)]
        turn = player_turn(cells, combat={"hp": 6}, stats={"health_recovery": {
            "period_ticks": 1000, "elapsed_ticks": 0, "reset_percent": 100, "stopped": False}})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        player.observe(turn)
        player.maps[1].visited.update(("-1", "-2"))
        self.assertEqual(player.choose(turn), (Move(Direction.WEST), "recover_en_route"))
        player.maps[1].visited.add("1")
        player.danger_until["1"] = 1200
        self.assertEqual(player.choose(turn), (Move(Direction.WEST), "recover_en_route"))
        player.danger_until["-1"] = 1200
        self.assertEqual(player.choose(turn), (Wait(), "recover"))

    def test_injured_player_attempts_escape_from_an_untimed_trap_before_resting(self):
        trap = {"condition": {"id": "rogue.bear_trap", "name": "Bear trap"}, "phase": "active",
                "remaining_ticks": None}
        tiles = [cell("here")]
        recovery = {"period_ticks": 1000, "elapsed_ticks": 0, "reset_percent": 100, "stopped": False}
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        for conditions, expected in (([trap], (Move(Direction.NORTH), "escape_bear_trap")),
                                     ([], (Wait(), "recover"))):
            with self.subTest(trapped=bool(conditions)):
                turn = player_turn(tiles, combat={"hp": 6}, stats={"conditions": conditions, "health_recovery": recovery})
                self.assertEqual(choose(player, turn), expected)

    def test_timed_bear_trap_keeps_recovery_in_place_and_preserves_the_travel_reserve(self):
        trap = {"condition": {"id": "rogue.bear_trap", "name": "Bear trap"}, "phase": "active",
                "remaining_ticks": "100"}
        tiles = [cell("here"), cell("stairs", 1, down=True)]
        for reserve, expected in ((32, "recover"), (31, "wait_for_restraint")):
            with self.subTest(reserve=reserve):
                turn = player_turn(tiles, combat={"hp": 5, "max_hp": 10}, stats={
                    "conditions": [trap], "health_recovery": {"period_ticks": 1000, "elapsed_ticks": 0,
                                                             "reset_percent": 100, "stopped": False},
                    "nutrition": {"kind": "required", "reserve": reserve, "maximum": 100,
                                  "unit_ticks": 100, "elapsed": 0, "hunger": "nourished"}})
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                player.travel_times.ascent[1] = 100
                self.assertEqual(choose(player, turn), (Wait(), expected))

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
                player.travel_times.ascent[1] = travel_ticks
                player.danger_until["hall"] = 1200
                self.assertEqual(player.choose(turn), expected)

    def test_recovery_deficit_rounds_fractional_targets_up(self):
        from rogue_player import recovery_deficit
        from tor_client_bot import Health

        for current, maximum, missing in ((7, 10, 1), (8, 10, 0), (8, 11, 1), (9, 11, 0), (11, 11, 0)):
            with self.subTest(current=current, maximum=maximum):
                self.assertEqual(recovery_deficit(Health(current, maximum)), missing)

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
        player.travel_times.ascent.update({1: 500, 2: 500, 3: 9000})
        self.assertEqual(choose(player, turn), (Wait(), "recover"))
        player.travel_times.ascent[1] = 1500
        self.assertEqual(player.choose(turn), (Move(Direction.UP), "ascend"))

    def test_spare_ticks_are_capped_by_the_food_clock_and_expose_a_journey_shortfall(self):
        from rogue_food import FoodEstimate
        from tor_client_bot import Nutrition

        nutrition = Nutrition(hunger="nourished", reserve=8, maximum=12, ticks_per_unit=100, elapsed_ticks=60)
        packed = FoodEstimate(known_reserve=3, has_unmeasured_meals=True)
        for travel, spare in ((0, 740), (299, 740), (300, 740), (301, 739), (1040, 0), (1041, -1)):
            with self.subTest(travel=travel):
                self.assertEqual(packed.spare_ticks_after_travel(nutrition, travel), spare)

    def test_carried_rations_can_cover_travel_while_rest_uses_current_nutrition(self):
        ration = {"id": "food", "identity": "item.food.ration", "name": "ration", "class": "food",
                  "identified": True, "quantity": 2}
        unknown = {"id": "other", "name": "mystery meal", "class": "food", "identified": False, "quantity": 3}
        for reserve, value, inventory, expected in (
            (20, 10, [ration], "recover"),
            (20, 10, [unknown, ration], "recover"),
            (20, 5, [ration, unknown], "descend"),
            (20, 5, [ration], "descend"),
            (10, 20, [ration], "descend"),
            (20, None, [ration], "descend"),
        ):
            with self.subTest(reserve=reserve, value=value, items=[item["id"] for item in inventory]):
                offers = [{"item": item["id"], "edible": True} for item in inventory]
                turn = player_turn([cell("exit", down=True)], inventory=inventory, interactions={"inventory": offers},
                                   combat={"hp": 7, "max_hp": 10}, stats={
                                       "nutrition": {"kind": "required", "reserve": reserve, "maximum": 1300,
                                                     "unit_ticks": 100, "elapsed": 0, "hunger": "nourished"},
                                       "health_recovery": {"period_ticks": 1000, "elapsed_ticks": 0,
                                                           "reset_percent": 100, "stopped": False}})
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
                player.travel_times.ascent[1] = 2000
                if value is not None:
                    player.meals.gains[ration["identity"]] = value
                self.assertEqual(choose(player, turn)[1], expected)

    def test_guarded_stair_keeps_its_travel_reserve_while_resting(self):
        turn = player_turn([cell("exit", down=True)], combat={"hp": 7, "max_hp": 10}, stats={
            "nutrition": {"kind": "required", "reserve": 20, "maximum": 1300, "unit_ticks": 100,
                          "elapsed": 0, "hunger": "nourished"},
            "health_recovery": {"period_ticks": 1000, "elapsed_ticks": 0,
                                "reset_percent": 100, "stopped": False}})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
        player.observe(turn)
        player.unsafe_stairs["exit", Direction.DOWN] = 100
        for travel, expected in ((999, (Wait(), "recover")), (1000, None)):
            with self.subTest(travel=travel):
                player.travel_times.ascent[1] = travel
                self.assertEqual(player.recovery_action(turn), expected)

    def test_unknown_wands_share_one_check_of_a_blocked_firing_lane(self):
        from unittest.mock import patch

        cells = [cell(str(x), x, wall=x == 2) for x in range(5)]
        foe = {"id": "emu", "name": "emu", "position": cells[3]["position"]}
        wands = [{"id": name, "name": name, "class": "wand", "identified": False, "quantity": 1}
                 for name in ("oak", "ash")]
        offers = [{"item": wand["id"], "device_activation": "direction", "zappable": True} for wand in wands]
        turn = player_turn(cells, actors=[foe], hostiles=["emu"], inventory=wands, interactions={"inventory": offers})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        with patch.object(turn, "aim_device", wraps=turn.aim_device) as aim:
            self.assertIsNone(player.try_unknown_wand(turn))
        aim.assert_called_once()

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

    def test_evasion_has_no_destination_when_the_last_pursuer_is_gone(self):
        turn = player_turn([cell("here"), cell("passage", -1)])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        self.assertIsNone(player.evade(turn))

    def test_retreat_recognizes_an_immediate_gain_without_an_onward_square(self):
        from rogue_retreat import choose_retreat_step

        foe = {"id": "hunter", "name": "hunter", "position": cell("foe", 1)["position"]}
        turn = player_turn([cell("here"), cell("foe", 1), cell("west", -1)], actors=[foe], hostiles=["hunter"])
        step = choose_retreat_step(turn)
        self.assertEqual(step.direction, Direction.WEST)
        self.assertTrue(step.improves_immediately)

    def test_level_first_step_requires_an_improving_continuation(self):
        foe = {"id": "hunter", "name": "hunter", "position": cell("foe", 0, 1)["position"]}
        cells = [cell("here"), cell("west", -1), cell("foe", 0, 1), cell("corner", -1, 1)]
        # The open corner keeps the hunter in contact after a step west.
        for onward in (False, True):
            with self.subTest(onward=onward):
                visible = cells + ([cell("exit", -2)] if onward else [])
                turn = player_turn(visible, actors=[foe], hostiles=["hunter"])
                expected = (Move(Direction.WEST), "break_contact") if onward else None
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                self.assertEqual(player.evade(turn), expected)

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
            player.opponents.damage_by_target["foe"] = learned
            turn = player_turn(room, actors=[foe], hostiles=["foe"], abilities=[melee], combat={"hp": hp})
            return choose(player, turn)
        self.assertEqual(decision(2), (Move(Direction.WEST), "break_contact"))
        foe["name"] = "black unicorn"
        self.assertEqual(decision(9, 4)[1], "melee")
        self.assertEqual(decision(8, 4)[1], "break_contact")
        escape["wall"] = True
        self.assertEqual(decision(2)[1], "melee")

    def test_when_two_steps_break_contact_choose_the_one_with_a_continuing_escape(self):
        foe = {"id": "foe", "name": "hunter", "position": cell("enemy", 1)["position"]}
        cells = [cell("here"), cell("enemy", 1)]
        cells.extend(cell(str(y), y=y) for y in (-1, 1, 2))
        cells.extend([cell("wall-north", 0, -2, wall=True), cell("wall-ne", 1, -1, wall=True),
                      cell("wall-se", 1, 1, wall=True)])
        for trapped, direction in ((False, Direction.SOUTH), (True, Direction.NORTH)):
            if trapped:
                next(tile for tile in cells if tile["key"] == "1")["trap"] = {
                    "id": "trap", "family": None, "reachable": True}
            turn = player_turn(cells, actors=[foe], hostiles=["foe"], combat={"hp": 2})
            with self.subTest(trapped=trapped):
                self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn),
                                 (Move(direction), "retreat"))

    def test_overlapping_escape_routes_evaluate_each_pursuer_once_per_destination(self):
        from unittest.mock import patch

        cells = [cell(f"{x},{y}", x, y) for x in range(-2, 3) for y in range(-2, 3)]
        foes = [{"id": name, "name": name, "position": {"x": 2, "y": y, "z": 0}}
                for name, y in (("orc", -1), ("troll", 1))]
        turn = player_turn(cells, actors=foes, hostiles=["orc", "troll"])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
        with patch.object(turn, "distance_to", wraps=turn.distance_to) as distance:
            step, reason = player.evade(turn)
        self.assertEqual((step.direction.offset[0], reason), (-1, "retreat"))
        evaluated = [(call.args[0].target, call.kwargs["from_offset"]) for call in distance.call_args_list]
        self.assertEqual(len(evaluated), len(set(evaluated)))

    def test_two_step_escape_prefers_clear_ground_over_one_more_square_of_clearance(self):
        from rogue_retreat import best_onward_escape

        for trapped in (-1, -2):
            with self.subTest(trapped=trapped):
                corridor = [cell(str(x), x) for x in range(-2, 3)]
                pit = next(site for site in corridor if site["key"] == str(trapped))
                pit["trap"] = {"id": "pit", "family": "pit", "reachable": False}
                turn = player_turn(corridor)

                def safety(offset):
                    return {2: (0, 1), -2: (0, 2)}.get(offset[0], (-1, 1))

                self.assertEqual(best_onward_escape(turn, [Direction.WEST, Direction.EAST], safety), Direction.EAST)
                self.assertEqual(best_onward_escape(turn, [Direction.WEST], safety), Direction.WEST)

    def test_retreat_prefers_a_clear_step_when_both_moves_break_contact(self):
        foe = {"id": "hunter", "name": "hunter", "position": tile(1, 0)["position"]}
        pit = {**tile(-1, 0), "trap": {"id": "pit", "family": "pit", "reachable": True}}
        for side, expected in (([], Direction.WEST), ([tile(0, -1)], Direction.NORTH)):
            with self.subTest(expected=expected):
                turn = player_turn([tile(0, 0), tile(1, 0), pit, *side], actors=[foe],
                                   hostiles=["hunter"], combat={"hp": 2})
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY)
                self.assertEqual(choose(player, turn), (Move(expected), "retreat"))

    def test_distant_pursuer_allows_a_clear_detour_before_a_trapped_escape(self):
        foe = {"id": "hunter", "name": "hunter", "position": tile(2, 1)["position"]}
        trap = {"id": "pit", "family": "pit", "reachable": True}
        west = {**tile(-1, 0), "trap": trap}
        for onward, expected in (([], (Move(Direction.WEST), "retreat")),
                                 ([{**tile(0, -2), "trap": trap}], (Move(Direction.WEST), "retreat")),
                                 ([tile(0, -2)], (Move(Direction.NORTH), "break_contact"))):
            with self.subTest(onward=onward):
                room = [tile(0, 0), west, tile(0, -1), tile(2, 1), *onward]
                turn = player_turn(room, actors=[foe], hostiles=["hunter"], combat={"hp": 2})
                self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn), expected)

    def test_equal_escape_routes_avoid_a_trap_on_either_step(self):
        foe = {"id": "foe", "name": "hunter", "position": cell("enemy", 1)["position"]}
        for trap_y, expected in ((-2, Direction.SOUTH), (-1, Direction.SOUTH), (1, Direction.NORTH), (2, Direction.NORTH)):
            with self.subTest(trap_y=trap_y):
                cells = [cell("here"), cell("enemy", 1)]
                for y in (-2, -1, 1, 2):
                    tile = cell(str(y), y=y)
                    if y == trap_y:
                        tile["trap"] = {"id": "pit", "family": None, "reachable": True}
                    cells.append(tile)
                turn = player_turn(cells, actors=[foe], hostiles=["foe"], combat={"hp": 2})
                self.assertEqual(choose(Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY), turn),
                                 (Move(expected), "retreat"))

    def test_stair_retreat_prefers_a_connected_exit_to_one_visible_across_a_wall(self):
        cells = [cell(str(x), x, up=x in (-4, 3), wall=x == 2) for x in range(-4, 4)]
        foe = {"id": "foe", "name": "wraith", "position": cell("enemy", y=-3)["position"]}
        turn = player_turn(cells, actors=[foe], hostiles=["foe"])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
        self.assertEqual(choose(player, turn), (Move(Direction.WEST), "retreat_to_stairs"))

    def test_stair_retreat_checks_remembered_traps_beyond_the_first_step(self):
        for has_clear_exit, expected in ((True, Direction.WEST), (False, Direction.EAST)):
            with self.subTest(has_clear_exit=has_clear_exit):
                corridor = [cell(str(x), x, up=x == 3 or (has_clear_exit and x == -4)) for x in range(-4, 4)]
                pit = next(tile for tile in corridor if tile["key"] == "2")
                pit["trap"] = {"id": "pit", "family": "pit", "reachable": False}
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
                player.observe(player_turn(corridor))
                # The stair is visible again, while the intervening pit is out of sight.
                foe = {"id": "foe", "name": "wraith", "position": cell("enemy", y=-3)["position"]}
                turn = player_turn([tile for tile in corridor if tile is not pit], actors=[foe], hostiles=["foe"])
                self.assertEqual(choose(player, turn), (Move(expected), "retreat_to_stairs"))

    def test_stair_retreat_prefers_an_open_exit_over_a_shorter_route_through_a_pursuer(self):
        cells = [cell(str(x), x, up=x in (-4, 3)) for x in range(-4, 4)]
        foe = {"id": "foe", "name": "wraith", "position": cell("enemy", 2)["position"]}
        turn = player_turn(cells, actors=[foe], hostiles=["foe"])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
        self.assertEqual(choose(player, turn), (Move(Direction.WEST), "retreat_to_stairs"))

    def test_stair_retreat_counts_diagonal_moves_when_choosing_an_exit(self):
        cells = [cell("west-exit", -3, up=True), cell("west-hall", -2), cell("west", -1)]
        cells.extend(cell(f"{x},{y}", x, y, up=(x, y) == (2, 2)) for x in range(3) for y in range(3))
        foe = {"id": "foe", "name": "wraith", "position": {"x": 0, "y": -3, "z": 0}}
        turn = player_turn(cells, actors=[foe], hostiles=["foe"])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
        self.assertEqual(choose(player, turn), (Move(Direction.SOUTH_EAST), "retreat_to_stairs"))

    def test_equally_close_stairs_favor_an_approach_away_from_the_pursuer(self):
        corridor = [cell(str(x), x, up=abs(x) == 3) for x in range(-3, 4)]
        for foe_x, escape in ((-2, Direction.EAST), (2, Direction.WEST)):
            foe = {"id": "foe", "name": "wraith", "position": {"x": foe_x, "y": 0, "z": 0}}
            for cells in (corridor, corridor[::-1]):
                with self.subTest(foe_x=foe_x, first=cells[0]["key"]):
                    turn = player_turn(cells, actors=[foe], hostiles=["foe"])
                    player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
                    self.assertEqual(choose(player, turn), (Move(escape), "retreat_to_stairs"))

    def test_equally_safe_stair_approaches_keep_the_same_direction_on_a_complete_tie(self):
        corridor = [cell(str(x), x, up=abs(x) == 3) for x in range(-3, 4)]
        foe = {"id": "foe", "name": "wraith", "position": cell("north", y=-2)["position"]}
        for cells in (corridor, corridor[::-1]):
            with self.subTest(first=cells[0]["key"]):
                turn = player_turn(cells, actors=[foe], hostiles=["foe"])
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
                self.assertEqual(choose(player, turn), (Move(Direction.WEST), "retreat_to_stairs"))

    def test_retreat_uses_the_safer_approach_to_two_views_of_the_same_stair(self):
        corridor = [cell("stair" if abs(x) == 3 else str(x), x, up=abs(x) == 3) for x in range(-3, 4)]
        for foe_x, expected in ((2, Direction.WEST), (-2, Direction.EAST)):
            foe = {"id": "foe", "name": "wraith", "position": cell("foe", foe_x, 1)["position"]}
            for cells in (corridor, corridor[::-1]):
                with self.subTest(foe_x=foe_x, first_x=cells[0]["position"]["x"]):
                    turn = player_turn(cells, actors=[foe], hostiles=["foe"])
                    player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
                    self.assertEqual(choose(player, turn), (Move(expected), "retreat_to_stairs"))

    def test_stair_retreat_rules_out_an_exit_occupied_by_any_part_of_a_pursuer(self):
        corridor = [cell(str(x), x, up=x in (-2, 3)) for x in range(-3, 4)]
        for footprint, expected in (((-3,), Direction.WEST), ((-3, -2), Direction.EAST)):
            with self.subTest(footprint=footprint):
                actors = [{"id": "foe", "name": "wraith", "position": cell("body", x)["position"]}
                          for x in footprint]
                turn = player_turn(corridor, actors=actors, hostiles=["foe"])
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
                self.assertEqual(choose(player, turn), (Move(expected), "retreat_to_stairs"))

    def test_remembered_stair_route_respects_the_previous_retreat_square(self):
        corridor = [cell(str(x), x, up=x in (-2, 3)) for x in range(-2, 4)]
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
        player.observe(player_turn(corridor))
        foe = {"id": "foe", "name": "wraith", "position": cell("north", y=-3)["position"]}
        turn = player_turn(corridor[1:4], actors=[foe], hostiles=["foe"])
        for previous, expected in ((None, Direction.WEST), ("-1", Direction.EAST)):
            with self.subTest(previous=previous):
                player.last_retreat_origin = previous
                self.assertEqual(player.route_to_retreat_stairs(turn, {"-2", "3"}),
                                 (Move(expected), "retreat_to_stairs"))

    def test_stair_retreat_takes_the_detour_around_a_known_blind_alley(self):
        cells = [cell("here"), cell("alley", 1), cell("barrier", 2, wall=True),
                 cell("north-wall", 1, -1, wall=True), cell("south-wall", 1, 1, wall=True),
                 cell("detour", 0, 1), cell("last-step", 3, 1), cell("stairs", 3, up=True)]
        cells.extend(cell(f"hall-{x}", x, 2) for x in range(4))
        foe = {"id": "foe", "name": "wraith", "position": cell("enemy", y=-3)["position"]}
        turn = player_turn(cells, actors=[foe], hostiles=["foe"])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
        self.assertEqual(choose(player, turn), (Move(Direction.SOUTH), "retreat_to_stairs"))

    def test_stair_retreat_skips_a_two_step_alley_that_requires_backtracking(self):
        cells = [cell("here"), cell("alley", 1), cell("alley-end", 2),
                 cell("barrier", 3, wall=True), cell("detour", 0, 1),
                 cell("last-step", 4, 1), cell("stairs", 4, up=True)]
        cells.extend(cell(f"wall-{x}-{y}", x, y, wall=True) for x in (1, 2) for y in (-1, 1))
        cells.extend(cell(f"hall-{x}", x, 2) for x in range(5))
        foe = {"id": "foe", "name": "wraith", "position": cell("enemy", y=-3)["position"]}
        turn = player_turn(cells, actors=[foe], hostiles=["foe"])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
        self.assertEqual(choose(player, turn), (Move(Direction.SOUTH), "retreat_to_stairs"))

    def test_stair_at_end_of_blind_alley_is_a_valid_retreat(self):
        cells = [cell("here"), cell("east-stair", 1, up=True), cell("west-stair", -1, up=True)]
        cells.extend(cell(f"wall-{x}-{y}", x, y, wall=True) for x, y in ((2, 0), (1, 1), (1, -1)))
        foe = {"id": "foe", "name": "wraith", "position": cell("enemy", -3)["position"]}
        turn = player_turn(cells, actors=[foe], hostiles=["foe"])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
        self.assertEqual(choose(player, turn), (Move(Direction.EAST), "retreat_to_stairs"))

    def test_stair_approach_can_gain_clearance_with_an_off_axis_step(self):
        cells = [cell(str(x), x, up=x == 3) for x in range(4)]
        cells.extend(cell(f"south-{x}", x, 1) for x in range(3))
        foe = {"id": "foe", "name": "wraith", "position": {"x": 0, "y": -2, "z": 0}}
        turn = player_turn(cells, actors=[foe], hostiles=["foe"])
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
        self.assertEqual(choose(player, turn), (Move(Direction.SOUTH_EAST), "retreat_to_stairs"))

    def test_stair_retreat_routes_to_a_clear_exit_beyond_a_trapped_exit(self):
        cells = [cell("clear", -2, down=True), cell("west", -1), cell("here"), cell("pit", 1, down=True)]
        cells[-1]["trap"] = {"id": "pit", "family": None, "reachable": True}
        foe = {"id": "foe", "name": "wraith", "position": {"x": 0, "y": 3, "z": 0}}
        turn = player_turn(cells, actors=[foe], hostiles=["foe"], combat={"hp": 8, "max_hp": 20})
        player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
        self.assertEqual(choose(player, turn), (Move(Direction.WEST), "retreat_to_stairs"))

    def test_stair_retreat_avoids_a_trapped_shortcut_with_equal_clearance(self):
        east = cell("east", 1)
        south = cell("south")
        diagonal = cell("shortcut", 1)
        stairs = cell("exit", 2, up=True)
        for tile in (south, diagonal, stairs):
            tile["position"]["y"] = 1
        cells = [cell("here"), east, south, diagonal, stairs, cell("enemy", -2)]
        foe = {"id": "foe", "name": "wraith", "position": cells[-1]["position"]}
        for trapped, expected in ((False, Direction.SOUTH_EAST), (True, Direction.EAST)):
            with self.subTest(trapped=trapped):
                if trapped:
                    diagonal["trap"] = {"id": "pit", "family": None, "reachable": True}
                turn = player_turn(cells, actors=[foe], hostiles=["foe"])
                player = Adventurer(26, DEFAULT_OBJECTIVE_IDENTITY, depth=2)
                self.assertEqual(choose(player, turn), (Move(expected), "retreat_to_stairs"))

    def test_retrieval_floor_changes_stair_priority_until_the_objective_is_carried(self):
        room = [cell("stairs", up=True, down=True), cell("frontier", 1)]
        prize = {"id": "prize", "name": AMULET, "class": "amulet", "identity": DEFAULT_OBJECTIVE_IDENTITY,
                 "quantity": 1, "identified": True}
        for depth, inventory, direction, reason in (
            (2, [], Direction.DOWN, "descend"),
            (2, [prize], Direction.UP, "ascend"),
            (3, [], Direction.EAST, "explore"),
            (4, [], Direction.EAST, "explore"),
            (4, [prize], Direction.UP, "ascend"),
            (3, [prize], Direction.UP, "ascend"),
        ):
            with self.subTest(depth=depth, reason=reason):
                player = Adventurer(3, DEFAULT_OBJECTIVE_IDENTITY, depth=depth)
                self.assertEqual(choose(player, player_turn(room, inventory=inventory)), (Move(direction), reason))
