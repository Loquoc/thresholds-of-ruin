"""Public decisions can be tested without loading the native client or opening a socket."""

from pathlib import Path
import sys
import unittest
from unittest.mock import Mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "crates/client-bot/python"))
from tor_client_bot import Direction, FloorMap, ItemUse, Move, OpenDoor, Outcome, Turn, UseItem


def attached(session, snapshot):
    from tor_client_bot import Session

    session.observe.return_value = snapshot
    return Session(session).observe()


class BotApiTests(unittest.TestCase):
    def test_clear_horizontal_device_aim_can_choose_another_visible_body_cell(self):
        def tile(x, y, wall=False):
            return {"key": f"{x},{y}", "position": {"x": x, "y": y, "z": 0}, "wall": wall,
                    "stairs_up": False, "stairs_down": False}

        wand = {"id": "w", "name": "staff", "quantity": 1, "class": "wand", "identified": False}
        offer = {"item": "w", "zappable": True, "device_activation": "direction"}
        for side_open in (True, False):
            cells = [tile(1, 0, True), tile(2, 0), tile(0, 1, not side_open), tile(1, 1), tile(2, 1), tile(2, 2)]
            actors = [{"id": "foe", "name": "large creature", "position": c["position"]} for c in (cells[1], cells[-1])]
            turn = Turn({"readiness": {}, "state": {"observation": {"self_target": "self", "visible_cells": cells,
                        "visible_actors": actors, "inventory": [wand], "interactions": {"inventory": [offer]}}}})
            device, = turn.devices
            foe, = turn.creatures
            self.assertEqual(turn.aim_device(device, foe).target.offset, (2, 0, 0))
            aimed = turn.aim_device(device, foe, clear_horizontal_ray=True)
            self.assertEqual(aimed.target.offset if aimed else None, (2, 2, 0) if side_open else None)

    def test_rejected_training_preview_reopens_before_another_selection(self):
        from tor_client_bot import Session, TrainingKind, TrainingRefused
        basis = {"actor": 1, "stream": "s", "branch": "b", "reset": "0", "build_epoch": "4"}
        def page(value):
            return {"status": "confirmed", "page": {"basis": basis, "next": None, "entries": [{"value": value}]}}
        overview = page({"locked": False, "points": {"unspent": "1"}, "starting_skills": []})
        transport = Mock()
        transport.advancement.side_effect = [
            overview, page({"owner": "1", "pool": {"id": "xp", "name": "XP"}, "unspent": 1, "allowed": ["talent"]}),
            page({"name": "Strike", "selection": {"talent": "strike"}, "form_choices": []}),
            {"status": "rejected", "code": "invalid_purchase"}, overview,
            page({"health": {"current": "8", "maximum": "9", "dead": False}}),
        ]
        training = Session(transport).training
        pool, = training.pools()
        choice, = training.options(TrainingKind.TALENT)
        with self.assertRaises(TrainingRefused):
            training.preview(pool, choice)
        self.assertFalse(training.preview(pool, choice).would_die)
        self.assertEqual(transport.advancement.call_args_list[-2].args, ("overview",))

    def test_result_recognizes_local_and_server_stale_contexts(self):
        from tor_client_bot import Result
        for status, code, expected in [
            ("stale_observation", None, True), ("rejected", "stale_context", True),
            ("rejected", "invalid_target", False), ("unknown_outcome", None, False),
        ]:
            result = Result.from_reply({"status": status, "code": code})
            self.assertEqual(result.stale, expected)

    def test_device_offers_preserve_hidden_charges_and_bind_the_aim_to_its_observation(self):
        from tor_client_bot import Zap, describe_action

        wand = {"id": "device", "name": "ash wand", "identified": False, "quantity": 1, "class": "wand"}
        target = {"key": "east", "position": {"x": 1, "y": 0, "z": 0}, "wall": False,
                  "stairs_up": False, "stairs_down": False}
        offer = {"item": "device", "zappable": True, "device_activation": "direction", "charges": None}
        enemy = {"id": "foe", "name": "serpent", "position": target["position"]}
        view = {"inventory": [wand], "visible_cells": [target], "interactions": {"inventory": [offer]},
                "visible_actors": [enemy], "self_target": "player"}
        snap, transport = Mock(), Mock()
        snap.data.return_value = {"state": {"observation": view}, "readiness": {}}
        transport.act.return_value = {"status": "confirmed"}
        old = attached(transport, snap)
        new = attached(transport, snap)
        device, = new.devices
        self.assertIs(device.item, new.inventory[0])
        self.assertIsNone(device.item.identity)
        self.assertIsNone(device.charges)
        with self.assertRaises(ValueError):
            Zap(device, None)
        current = new.cell_at((1, 0, 0))
        self.assertIs(new.submit(Zap(old.devices[0], current)).status, Outcome.STALE)
        self.assertIs(new.submit(Zap(device, old.cell_at((1, 0, 0)))).status, Outcome.STALE)
        transport.act.assert_not_called()
        action = Zap(device, current)
        self.assertEqual(new.aim_device(device, new.creatures[0]), action)
        self.assertIsNone(new.aim_device(device, old.creatures[0]))
        expected = {"type": "zap", "item": "device", "aim": {"kind": "cell", "offset": [1, 0, 0]}}
        self.assertEqual(describe_action(action), expected)
        self.assertIs(new.submit(action).status, Outcome.CONFIRMED)
        transport.act.assert_called_once_with(expected, snap)
        offer.update(device_activation="self_target", charges="0")
        self_only = attached(transport, snap).devices[0]
        self.assertEqual(self_only.charges, 0)
        self.assertEqual(describe_action(Zap(self_only, None))["aim"], {"kind": "self_target"})
        with self.assertRaises(ValueError):
            Zap(self_only, Direction.EAST)
        offer.update(device_activation="direction_or_self", charges="2")
        flexible = attached(transport, snap).devices[0]
        self.assertEqual(describe_action(Zap(flexible, Direction.EAST))["aim"],
                         {"kind": "direction", "direction": "east"})
        self.assertEqual(describe_action(Zap(flexible, None))["aim"], {"kind": "self_target"})

    def test_readiness_timeout_retains_the_last_observed_turn(self):
        from tor_client_bot import ReadinessTimeout, Session

        transport = Mock()
        transport.observe.return_value.data.return_value = {
            "state": {"observation": {"ready": False, "tick": "17"}},
            "readiness": {"admission": False},
        }
        with self.assertRaises(ReadinessTimeout) as caught:
            Session(transport).wait_for_decision(timeout=0)
        self.assertIsInstance(caught.exception, TimeoutError)
        self.assertEqual(caught.exception.turn.tick, 17)
        self.assertFalse(caught.exception.turn.can_act)

    def test_optional_stalled_decision_preserves_receipt_and_continues_the_same_iterator(self):
        from tor_client_bot import ActivityPhase, ReadinessTimeout, Session, Wait

        receipt = {"actor": 1, "branch": "game", "intention": "down", "phase": "queued"}
        transport = Mock()
        transport.act.return_value = {"status": "confirmed", "receipt": {"type": "admitted", **receipt}}
        def frame(ready, phase):
            return Turn({"state": {"observation": {"tick": 17, "ready": ready}},
                         "readiness": {"admission": ready}, "intentions": [dict(receipt, phase=phase)]},
                        connection=transport, snapshot=Mock())
        first, blocked, following = frame(True, "queued"), frame(False, "queued"), frame(True, "cancelled")
        session = Session(Mock())
        session.wait_for_decision = Mock(side_effect=[first, ReadinessTimeout(blocked), following])
        decisions = session.decisions(include_stalls=True)
        initial = next(decisions)
        initial.turn.submit(Wait())
        stall = next(decisions)
        self.assertTrue(stall.stalled)
        self.assertIs(stall.turn, blocked)
        self.assertIs(stall.previous_phase, ActivityPhase.QUEUED)
        self.assertFalse(next(decisions).stalled)

    def test_identification_accepts_only_items_from_its_own_offer(self):
        from dataclasses import replace
        from tor_client_bot import Identification, Item

        item = Item("carried", "scroll", None, 1, (), "scroll")
        prompt = Identification("choose", (item,))
        self.assertIs(prompt.select(item).item, item)
        self.assertIsNone(prompt.select(None).item)
        with self.assertRaises(ValueError):
            prompt.select(replace(item))

    def test_decision_iterator_carries_the_previous_receipt_into_the_terminal_picture(self):
        from tor_client_bot import ActivityPhase, Session, Wait
        transport = Mock()
        receipt = dict(actor=1, branch="game", intention="7", phase="queued")
        transport.act.return_value = {"status": "confirmed", "receipt": {"type": "admitted", **receipt}}
        def snapshot(terminal):
            snap = Mock()
            snap.data.return_value = {
                "readiness": {"admission": True}, "state": {"observation": {
                    "ready": True, "combat": {"terminal": terminal}, "tick": 0}},
                "finished_intentions": [dict(receipt, phase="resolved")] if terminal else [],
            }
            return snap
        transport.observe.side_effect = [snapshot(False), snapshot(True)]
        decisions = Session(transport).decisions()
        first = next(decisions)
        self.assertIsNone(first.previous)
        result = first.turn.submit(Wait())
        last = next(decisions)
        self.assertIs(last.previous, result)
        self.assertIs(last.previous_phase, ActivityPhase.RESOLVED)
        self.assertTrue(last.turn.terminal)
        with self.assertRaises(StopIteration):
            next(decisions)

    def test_decline_from_an_earlier_prompt_cannot_answer_a_fresh_prompt(self):
        snap = Mock()
        snap.data.return_value = {"state": {"observation": {
            "inventory": [], "interactions": {"identification": {"choice": "offer", "items": []}},
        }}, "readiness": {}}
        session = Mock()
        session.act.return_value = {"status": "confirmed"}
        first = attached(session, snap)
        current = attached(session, snap)
        self.assertIs(current.submit(first.identification.select(None)).status, Outcome.STALE)
        session.act.assert_not_called()
        self.assertIs(current.submit(current.identification.select(None)).status, Outcome.CONFIRMED)

    def test_condition_query_distinguishes_active_expired_and_protected(self):
        def condition(identity, phase, remaining):
            return {"condition": {"id": identity, "name": identity}, "phase": phase,
                    "remaining_ticks": remaining}
        rows = [condition("timed", "active", "2"), condition("persistent", "active", None),
                condition("expired", "active", "0"), condition("protected", "protection", "3")]
        view = {"combat": {"own_stats": {"conditions": rows}}}
        turn = Turn({"state": {"observation": view}, "readiness": {}})
        self.assertTrue(turn.has_condition("timed"))
        self.assertTrue(turn.has_condition("persistent"))
        for key in ("expired", "protected", "undisclosed"):
            self.assertFalse(turn.has_condition(key))

    def test_action_description_uses_player_visible_operands(self):
        from tor_client_bot import Wait, describe_action

        session, snapshot = Mock(), Mock()
        snapshot.data.return_value = {"state": {"observation": {}}, "readiness": {"admission": True}}
        session.act.return_value = {"status": "confirmed"}
        turn = attached(session, snapshot)
        for action, expected in [(Move(Direction.DOWN), {"type": "move", "direction": "down"}),
                                 (Wait(), {"type": "wait"})]:
            with self.subTest(action=action):
                self.assertEqual(describe_action(action), expected)
                self.assertIs(turn.submit(action).status, Outcome.CONFIRMED)
                session.act.assert_called_with(expected, snapshot)

    def test_journal_interprets_incoming_abilities_without_inventing_a_hidden_caster(self):
        from tor_client_bot import AbilityOutcome, CombatJournal

        rows = [
            {"type": "unseen_ability", "target": "me", "caster": None, "outcome": "miss"},
            {
                "type": "ability",
                "target": "me",
                "caster": "foe",
                "ability": {"id": "sting"},
                "outcome": "applied",
                "strike": 0,
            },
            {"type": "anonymous_ability", "target": None, "caster": None, "outcome": "unaffected"},
            {"type": "ability", "target": "me", "caster": "me", "outcome": "applied"},
        ]
        data = {
            "state": {"observation": {"self_target": "me"}},
            "readiness": {},
            "stream": "one",
            "combat_updates": [{"cursor": {"sequence": "1"}, "events": rows}],
        }
        journal = CombatJournal()
        report = journal.read(Turn(data))
        unseen, sting = report.incoming
        self.assertEqual(
            (unseen.unseen, unseen.caster, unseen.ability, unseen.outcome),
            (True, None, None, AbilityOutcome.MISS),
        )
        self.assertEqual(
            (sting.caster, sting.ability, sting.outcome), ("foe", "sting", AbilityOutcome.APPLIED)
        )
        self.assertEqual(journal.read(Turn(data)).incoming, ())
        self.assertEqual(journal.read(Turn({**data, "stream": "two"})).incoming, report.incoming)

    def test_activity_lookup_tracks_phase_only_for_receipts_from_same_actor_and_branch(self):
        from tor_client_bot import ActivityPhase, Result

        row = dict(actor=1, branch="main", intention="7", entry_id="initial", phase="queued")
        result = Result.from_reply({"status": "confirmed", "receipt": {"type": "admitted", **row}})
        self.assertIs(result.activity.phase, ActivityPhase.QUEUED)

        def observe(active=(), finished=()):
            return Turn(
                {
                    "state": {"observation": {}},
                    "readiness": {},
                    "intentions": active,
                    "finished_intentions": finished,
                }
            )

        paused = observe([dict(row, phase="paused")])
        self.assertIs(paused.outcome_of(result), ActivityPhase.PAUSED)
        final = observe(finished=[dict(row, phase="resolved", entry_id="later")])
        self.assertIs(Turn.replay(final.recording()).outcome_of(result), ActivityPhase.RESOLVED)
        wire = Result.from_reply({"status": "confirmed", "receipt": {
            "type": "admitted", **row, "actor": "1"}})
        self.assertEqual(wire.activity.actor, 1)
        self.assertIs(final.outcome_of(wire), ActivityPhase.RESOLVED)
        for other in (dict(row, branch="other"), dict(row, actor=2)):
            self.assertIsNone(observe([other]).outcome_of(result))
        self.assertIsNone(observe().outcome_of(result))
        refused = Result.from_reply({"status": "rejected", "code": "unavailable", "message": "busy"})
        self.assertEqual(refused.code, "unavailable")
        self.assertIsNone(paused.outcome_of(refused))

    def test_decision_uses_relative_origin_and_submits_with_original_attachment(self):
        session = Mock()
        snapshot = session.observe()
        cells = [
            {
                "key": str(x),
                "position": {"x": x, "y": 0, "z": 0},
                "stairs_down": x == 0,
                "stairs_up": False,
                "wall": False,
            }
            for x in (1, 0)
        ]
        snapshot.data.return_value = {
            "state": {
                "observation": {
                    "ready": True,
                    "visible_cells": cells,
                    "combat": {"hp": "7", "max_hp": "12", "terminal": False},
                }
            },
            "readiness": {"admission": True, "resume": [], "cancel": []},
        }
        session.act.return_value = {"status": "confirmed", "receipt": {"intention": "12"}}
        turn = attached(session, snapshot)
        self.assertEqual((turn.health.current, turn.health.maximum), (7, 12))
        self.assertTrue(turn.cell.stairs_down)
        self.assertTrue(turn.can_act)
        action = Move(Direction.DOWN)
        session.act.assert_not_called()
        result = turn.submit(action)
        self.assertEqual(result.status, Outcome.CONFIRMED)
        self.assertEqual(result.intention, "12")
        session.act.assert_called_once_with({"type": "move", "direction": "down"}, snapshot)
        snapshot.data.assert_called_once_with()

    def test_groups_body_cells_and_distinguishes_undisclosed_from_nonhostile(self):
        def body(target, x):
            return {"id": target, "name": target, "position": {"x": x, "y": 0, "z": 0}}

        view = {
            "self_target": "self",
            "visible_actors": [body("self", 0), body("ogre", 3), body("ogre", 2), body("shade", 1)],
            "combat": {
                "actors": [{"actor": "ogre", "hostile": True, "allied": False, "injury": "wounded"}]
            },
        }
        snapshot = Mock()
        snapshot.data.return_value = {"state": {"observation": view}, "readiness": {}}
        turn = attached(Mock(), snapshot)
        ogre, shade = turn.creatures
        self.assertEqual(turn.hostiles, (ogre,))
        self.assertEqual((ogre.distance, ogre.injury, len(ogre.cells)), (2, "wounded", 2))
        self.assertIsNone(shade.hostile)
        self.assertIsNone(shade.injury)

    def test_item_offers_join_by_target_and_hide_unknown_identity(self):
        snap = Mock()
        carried = [
            {
                "id": "a",
                "name": "flask",
                "quantity": "3",
                "identity": "item.potion.unknown",
                "identified": False,
            },
            {
                "id": "b",
                "name": "ration",
                "quantity": "1",
                "identity": "item.food.ration",
                "identified": True,
            },
        ]
        view = {
            "inventory": carried,
            "interactions": {
                "inventory": [{"item": "b", "edible": True}, {"item": "a", "drinkable": True}]
            },
        }
        snap.data.return_value = {"state": {"observation": view}, "readiness": {}}
        session = Mock()
        turn = attached(session, snap)
        flask, food = turn.inventory
        self.assertEqual((flask.identity, flask.quantity, flask.uses), (None, 3, (ItemUse.DRINK,)))
        with self.assertRaises(ValueError):
            UseItem(flask, ItemUse.EAT)
        session.act.return_value = {"status": "rejected", "message": "out of reach"}
        result = turn.submit(UseItem(food, ItemUse.EAT))
        self.assertEqual((result.status, result.reason), (Outcome.REJECTED, "out of reach"))
        session.act.assert_called_once_with({"type": "eat", "item": "b"}, snap)

    def test_horizontal_move_rejects_an_overhang_at_head_height(self):
        def position(x, z):
            return {"x": x, "y": 0, "z": z}

        cells = [
            {
                "key": f"{x}-{z}",
                "position": position(x, z),
                "wall": x == z == 1,
                "stairs_up": False,
                "stairs_down": False,
            }
            for x in (0, 1)
            for z in (0, 1)
        ]
        body = [{"id": "me", "name": "you", "position": position(0, z)} for z in (0, 1)]
        turn = Turn(
            {
                "state": {
                    "observation": {"visible_cells": cells, "visible_actors": body, "self_target": "me"}
                },
                "readiness": {},
            }
        )
        self.assertIsNone(turn.step(Direction.EAST))

    def test_uses_the_reachable_part_of_a_door_that_spans_two_heights(self):
        rows = [
            {
                "key": str(z),
                "position": {"x": -1, "y": 0, "z": z},
                "wall": False,
                "stairs_up": False,
                "stairs_down": False,
                "door": {"id": "entrance", "open": False, "reachable": z == 0},
            }
            for z in (0, 1)
        ]
        view = {
            "self_target": "me",
            "visible_cells": rows,
            "visible_actors": [{"id": "me", "position": {"x": 0, "y": 0, "z": 1}}],
        }
        turn = Turn({"state": {"observation": view}, "readiness": {}})
        self.assertEqual(turn.step(Direction.WEST), OpenDoor(turn.visible_cells[0].door))
        self.assertIs(turn.visible_cells[0].door, turn.visible_cells[1].door)
        # A different unreachable door has its own reachability.
        rows[1]["door"]["id"] = "other"
        self.assertIsNone(Turn({"state": {"observation": view}, "readiness": {}}).step(Direction.WEST))
        rows[1]["door"] = None
        rows[0]["door"] = None
        rows[0]["wall"] = True
        self.assertIsNone(Turn({"state": {"observation": view}, "readiness": {}}).step(Direction.WEST))

    def test_navigation_opens_reachable_door_and_never_cuts_unknown_corner(self):
        def tile(key, x, y, door=None):
            return {
                "key": key,
                "position": {"x": x, "y": y, "z": 0},
                "wall": False,
                "stairs_up": False,
                "stairs_down": False,
                "door": door,
            }

        snap = Mock()
        door = {"id": "oak", "open": False, "reachable": True}
        view = {
            "visible_cells": [tile("home", 0, 0), tile("exit", 1, 0, door), tile("corner", 1, 1)],
            "visible_actors": [],
            "self_target": "me",
        }
        snap.data.return_value = {"state": {"observation": view}, "readiness": {}}
        turn = attached(Mock(), snap)
        memory = FloorMap()
        memory.observe(turn)
        first = memory.toward(turn, {"exit"})
        self.assertIsInstance(first, OpenDoor)
        self.assertEqual(first.door.target, "oak")
        self.assertIsNone(memory.toward(turn, {"corner"}, max_steps=1))
        self.assertEqual(memory.toward(turn, {"corner"}, max_steps=2), first)
        self.assertIsNone(memory.toward(turn, {"exit"}, max_steps=0))
        for prefer_safe in (True, False):
            self.assertIsNone(memory.toward(turn, {"corner"}, avoid={"exit"}, prefer_safe=prefer_safe))
        self.assertIsNone(turn.step(Direction.SOUTH_EAST))
        self.assertIsNone(turn.health)
        self.assertEqual(memory.visited, {"home"})

    def test_projectile_options_page_past_empty_matches_and_retain_exact_launcher(self):
        snap, session = Mock(), Mock()
        view = {
            "visible_actors": [{"id": "foe", "name": "kobold", "position": {"x": 2, "y": 0, "z": 0}}],
            "self_target": "me",
            "visible_cells": [],
            "inventory": [],
            "combat": {
                "actors": [{"actor": "foe", "hostile": True}],
                "own_stats": {
                    "abilities": [
                        {
                            "id": "shoot",
                            "name": "Shoot",
                            "target_kind": "directional_projectile",
                            "basic_attack": False,
                            "reach": {"kind": "projectile"},
                            "cost": None,
                            "cooldown_remaining": "0",
                        }
                    ]
                },
            },
        }
        for x in range(3):
            view["visible_cells"].append(
                {
                    "key": str(x),
                    "position": {"x": x, "y": 0, "z": 0},
                    "stairs_down": False,
                    "stairs_up": False,
                    "wall": False,
                }
            )
        for height in (1, 3):
            position = {"x": 2, "y": 0, "z": height}
            view["visible_actors"].append({"id": "foe", "name": "kobold", "position": position})
            view["visible_cells"].append(
                {**view["visible_cells"][-1], "key": f"upper-{height}", "position": position}
            )
        for key in ("bow", "arrow"):
            view["inventory"].append(
                {"id": key, "name": key, "quantity": "1", "identity": key, "identified": True}
            )
        snap.data.return_value = {"state": {"observation": view}, "readiness": {}}
        launcher = {"item": "bow", "socket": 3}
        session.projectile_choices.side_effect = [
            {"choices": [], "next_offset": 8},
            {"choices": [{"kind": "launcher", "launcher": launcher}], "next_offset": None},
            {"choices": [{"kind": "ammunition", "item": "arrow"}], "next_offset": None},
        ]
        turn = attached(session, snap)
        shot, upper = turn.ranged_attacks(max_distance=2)
        replay = Turn.replay(turn.recording())
        self.assertEqual(tuple(replay.ranged_attacks(max_distance=2)), (shot, upper))
        self.assertIs(shot.target, upper.target)
        self.assertEqual(upper.offset, (2, 0, 1))
        self.assertEqual(session.projectile_choices.call_count, 3)
        self.assertEqual(
            (shot.target.name, shot.launcher.name, shot.projectile.name), ("kobold", "bow", "arrow")
        )
        session.act.return_value = {"status": "confirmed"}
        turn.submit(shot)
        command, source = session.act.call_args.args
        self.assertIs(source, snap)
        self.assertEqual(
            command["invocation"]["aim"],
            {
                "kind": "cell_projectile",
                "offset": [2, 0, 0],
                "delivery": {"kind": "launch", "launcher": launcher, "ammunition": "arrow"},
            },
        )

    def test_policy_eats_an_offered_item_when_hungry(self):
        from rogue_player import Adventurer

        snap = Mock()
        view = {
            "ready": True,
            "self_target": "me",
            "visible_actors": [],
            "combat": {
                "hp": "8",
                "max_hp": "8",
                "terminal": False,
                "actors": [],
                "own_stats": {"nutrition": {"hunger": "hungry"}},
            },
            "visible_cells": [
                {
                    "key": "home",
                    "position": {"x": 0, "y": 0, "z": 0},
                    "stairs_up": False,
                    "stairs_down": False,
                    "wall": False,
                }
            ],
            "interactions": {"inventory": [{"item": "food", "edible": True}]},
            "tick": "100",
            "inventory": [
                {
                    "id": "food",
                    "name": "ration",
                    "identity": "item.food.ration",
                    "identified": True,
                    "quantity": "2",
                }
            ],
        }
        snap.data.return_value = {
            "state": {"observation": view},
            "readiness": {"admission": True, "resume": [], "cancel": []},
        }
        turn = attached(Mock(), snap)
        player = Adventurer(26, "relic")
        player.observe(turn)
        action, reason = player.choose(turn)
        self.assertEqual((action, reason), (UseItem(turn.inventory[0], ItemUse.EAT), "eat"))

    def test_action_from_another_snapshot_cannot_retarget_an_identical_item(self):
        session, snap = Mock(), Mock()
        item = {"id": "same-token", "identity": "food", "identified": True, "name": "meal", "quantity": "1"}
        view = {
            "inventory": [item],
            "interactions": {"inventory": [{"item": "same-token", "edible": True}]},
        }
        snap.data.return_value = {"state": {"observation": view}, "readiness": {}}
        first, second = attached(session, snap), attached(session, snap)
        result = second.submit(UseItem(first.inventory[0], ItemUse.EAT))
        self.assertIs(result.status, Outcome.STALE)
        session.act.assert_not_called()

    def test_chart_coordinates_remain_in_the_original_component(self):
        from tor_client_bot import ChartKind

        session, snap = Mock(), Mock()
        snap.data.return_value = {
            "state": {
                "observation": {
                    "charts": [{"kind": "mapping", "chart": "token"}, {"kind": "monsters", "chart": None}]
                }
            },
            "readiness": {},
        }
        session.chart.return_value = {
            "mode": "historical",
            "acquired_at": "120",
            "components": [{"component": "4", "alignment": "aligned"}],
            "rows": [
                {
                    "component": "4",
                    "position": ["5", "-1", "0"],
                    "mark": {"kind": "mapping", "fact": {"kind": "secret_door"}},
                }
            ],
        }
        turn = attached(session, snap)
        self.assertEqual(turn.available_charts, (ChartKind.MAPPING,))
        chart = turn.chart(ChartKind.MAPPING)
        (mark,) = chart.marks
        self.assertEqual(
            (chart.acquired_at, mark.component, mark.position, mark.feature),
            (120, 4, (5, -1, 0), "secret_door"),
        )
        self.assertEqual(Turn.replay(turn.recording()).chart(ChartKind.MAPPING), turn.chart(ChartKind.MAPPING))
        session.chart.assert_called_once_with(snap, "mapping")

    def test_replay_runs_the_same_local_policy_without_a_connection(self):
        from tor_client_bot import Wait

        snap = Mock()
        view = {"ready": True, "combat": {"terminal": False, "hp": "4", "max_hp": "9"}}
        snap.data.return_value = {"state": {"observation": view}, "readiness": {"admission": True}}
        live = attached(Mock(), snap)
        replay = Turn.replay(live.recording())
        self.assertEqual(replay.health, live.health)
        self.assertTrue(replay.can_act)
        with self.assertRaisesRegex(RuntimeError, "live player connection"):
            replay.submit(Wait())

    def test_training_reports_a_missing_confirmed_page(self):
        from unittest.mock import Mock
        from tor_client_bot import Session

        transport = Mock()
        transport.advancement.return_value = {"status": "confirmed", "page": None}
        with self.assertRaisesRegex(ValueError, "no page"):
            Session(transport).training.overview()

    def test_training_pages_and_one_use_purchase_confirmation(self):
        from tor_client_bot import Session, TrainingKind

        basis = {"actor": 1, "stream": "fixture", "branch": "a", "reset": "0", "build_epoch": "1"}

        def page(rows, successor=None):
            return {
                "status": "confirmed",
                "page": {"basis": basis, "next": successor, "entries": [{"value": row} for row in rows]},
            }

        overview = page([{"locked": False, "points": {"unspent": "1"}, "starting_skills": []}])
        balance = page(
            [
                {
                    "owner": "2",
                    "pool": {"id": "trained", "name": "Training"},
                    "unspent": 1,
                    "allowed": ["skill_rank"],
                }
            ]
        )
        catalog = page([{"name": "Defense", "selection": {"skill": "defense"}, "form_choices": []}])
        summary = page([{"health": {"current": "10", "maximum": "12", "dead": False}}])
        transport = Mock()
        transport.advancement.side_effect = [
            overview,
            balance,
            page([], 8),
            catalog,
            summary,
            overview,
            summary,
            {"status": "confirmed"},
        ]
        training = Session(transport).training
        (pool,) = training.pools()
        (choice,) = training.options(TrainingKind.SKILL)
        first = training.preview(pool, choice)
        self.assertEqual((pool.hit_die, first.resulting_health.current), (3, 10))
        training.overview()
        self.assertIs(training.confirm(first).status, Outcome.STALE)
        second = training.preview(pool, choice)
        self.assertIs(training.confirm(second).status, Outcome.CONFIRMED)
        self.assertIs(training.confirm(second).status, Outcome.STALE)
        self.assertEqual(
            [call.args[0] for call in transport.advancement.call_args_list],
            [
                "overview",
                "balances",
                "skills",
                "next",
                "purchase 3 trained skill defense",
                "overview",
                "purchase 3 trained skill defense",
                "confirm",
            ],
        )


if __name__ == "__main__":
    unittest.main()
