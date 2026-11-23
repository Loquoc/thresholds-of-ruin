"""Report conventions for fixed bot cohorts."""

import json
import unittest

from rogue_o_matic_cohort import summarize


class CohortSummaryTests(unittest.TestCase):
    def test_binding_snapshot_preserves_the_selected_python_and_native_code(self):
        import hashlib
        import os
        import tempfile
        from pathlib import Path
        from importlib.machinery import EXTENSION_SUFFIXES
        from unittest.mock import Mock, patch
        from rogue_o_matic_cohort import play_on_server, snapshot_bindings

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, policy = root / "installed/tor_client_bot", root / "policy"
            source.mkdir(parents=True)
            policy.mkdir()
            native = "_native" + EXTENSION_SUFFIXES[0]
            contents = {"__init__.py": b"", "game.py": b"current = True", native: b"binary fixture"}
            for name, data in contents.items():
                (source / name).write_bytes(data)
            (source / "_native.pyi").write_text("stub only")
            hashes = snapshot_bindings(source, policy)
            (source / "game.py").write_text("current = False")
            for name, data in contents.items():
                self.assertEqual((policy / "tor_client_bot" / name).read_bytes(), data)
            self.assertEqual(hashes, {name: hashlib.sha256(data).hexdigest() for name, data in contents.items()})
            entry = policy / "player.py"
            terminal = json.dumps({"terminal": "action_budget"})
            entry.write_text("from tor_client_bot.game import current\nassert current\n"
                             f"print({terminal!r})\nraise SystemExit(1)\n")
            server = Mock()
            server.until.return_value = '{"address":"localhost:1"}'
            with patch.dict(os.environ, {"PYTHONPATH": str(source.parent), "PYTHONSAFEPATH": "1"}):
                result = play_on_server(server, "test", 1, 1, 15, root / "turns.jsonl", 26, "amulet", entry, False)
            self.assertEqual(result["terminal"], "action_budget")

    def test_startup_failure_counts_each_scheduled_seed_in_the_cli_report(self):
        from pathlib import Path
        import subprocess
        import sys
        import tempfile

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "scenario.toml").write_text("id = 'fixture'\n", encoding="utf-8")
            output = root / "campaign"
            # Python exits immediately on the server's --listen argument.
            result = subprocess.run([sys.executable, str(Path(__file__).with_name("rogue_o_matic_cohort.py")),
                                     "--server", sys.executable, "--scenario", str(root), "--output", str(output),
                                     "47", "49"], capture_output=True, text=True, timeout=40)
            self.assertEqual(result.returncode, 1, result.stderr)
            report = json.loads((output / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(report["runs"], 2, result.stderr)
            self.assertEqual([(r["seed"], r["terminal"]) for r in report["results"]],
                             [(47, "server_startup_error"), (49, "server_startup_error")])
            self.assertEqual(report["victories"], 0)
            self.assertEqual(report["deaths_by_cause"], {})

    def test_shutdown_timeout_preserves_the_observed_campaign_result(self):
        import subprocess
        import tempfile
        from pathlib import Path
        from unittest.mock import patch

        from rogue_o_matic_cohort import play

        def client(*args, stdout, **kwargs):
            print(json.dumps({"terminal": "action_budget", "actions": 1, "max_depth": 3}), file=stdout)
            return subprocess.CompletedProcess(args, 1)

        with tempfile.TemporaryDirectory() as temp, patch("rogue_o_matic_cohort.Process") as process:
            server = process.return_value
            server.until.return_value = '{"address":"localhost:1"}'
            server.stop.side_effect = subprocess.TimeoutExpired("server", 10)
            with patch("rogue_o_matic_cohort.subprocess.run", side_effect=client):
                result = play(47, "server", "scenario", 1, 1, 60, Path(temp), 26, "amulet", "bot.py")
        self.assertEqual(result["terminal"], "action_budget")
        self.assertEqual(result["max_depth"], 3)
        self.assertEqual(result["server_cleanup"], "timeout")

    def test_campaign_inputs_identify_scenario_and_policy(self):
        from pathlib import Path
        import tempfile

        from rogue_o_matic_cohort import campaign_inputs

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scenario = root / "scenario"
            scenario.mkdir()
            (scenario / "scenario.toml").write_text("id = 'example'\n", encoding="utf-8")
            policy = root / "policy.py"
            policy.write_text("pass\n", encoding="utf-8")
            server = root / "server"
            rules = scenario / "rules.toml"
            certificate = scenario / "validation.json"
            server.write_bytes(b"old server")
            rules.write_text("import = 'old'\n", encoding="utf-8")
            certificate.write_text("{}", encoding="utf-8")
            before = campaign_inputs(server, scenario, 3, policy)
            server.write_bytes(b"new server")
            rules.write_text("import = 'new'\n", encoding="utf-8")
            certificate.write_text('{"ruleset": "new"}', encoding="utf-8")
            policy.write_text("print(1)\n", encoding="utf-8")
            after = campaign_inputs(server, scenario, 3, policy)
        self.assertEqual(before["actor"], 3)
        self.assertEqual(before["scenario"], str(scenario))
        self.assertEqual(before["scenario_manifest_sha256"], after["scenario_manifest_sha256"])
        self.assertNotEqual(before["policy_sha256"], after["policy_sha256"])
        for name in ("server_sha256", "scenario_rules_sha256", "scenario_validation_sha256"):
            self.assertNotEqual(before[name], after[name])

    def test_policy_snapshot_preserves_sources_after_workspace_edits(self):
        import hashlib
        from pathlib import Path
        import tempfile

        from rogue_o_matic_cohort import POLICY_FILES, snapshot_policy

        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source"
            source.mkdir()
            output = Path(directory) / "output"
            output.mkdir()
            for name in POLICY_FILES:
                (source / name).write_bytes(f"# Original {name}\r\n".encode())
            script, hashes = snapshot_policy(source, output)
            for name in POLICY_FILES:
                (source / name).write_text("# Edited", encoding="utf-8")
                data = (script.parent / name).read_bytes()
                self.assertEqual(data, f"# Original {name}\r\n".encode())
                self.assertEqual(hashes[name], hashlib.sha256(data).hexdigest())
            self.assertEqual(script.name, "rogue_o_matic.py")

    def test_fresh_victory_verification_requires_the_configured_identity(self):
        from types import SimpleNamespace
        from unittest.mock import patch

        from rogue_o_matic_cohort import verify_victory

        from unittest.mock import MagicMock

        session = MagicMock()
        turn = session.__enter__.return_value.observe.return_value
        turn.victorious = True
        turn.inventory = (SimpleNamespace(identity="item.amulet.fixture.seal"),)
        extension = SimpleNamespace(Session=SimpleNamespace(connect=lambda *args: session))
        with patch.dict("sys.modules", {"tor_client_bot": extension}):
            self.assertTrue(verify_victory("example", "token", 1, "item.amulet.fixture.seal"))
            self.assertFalse(verify_victory("example", "token", 1, "item.amulet.fixture.lookalike"))
        self.assertEqual(session.__exit__.call_count, 2)

    def test_cli_reports_readiness_timeout_as_failure_and_keeps_the_report(self):
        from contextlib import redirect_stdout
        from io import StringIO
        from pathlib import Path
        import tempfile
        from unittest.mock import patch

        from rogue_o_matic_cohort import main

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "scenario.toml").write_text("id = 'fixture'\n", encoding="utf-8")
            (root / "server").write_bytes(b"fixture server")
            args = ["cohort", "--server", str(root / "server"), "--scenario", str(root),
                    "--output", str(root / "output"), "50"]
            result = {"seed": 50, "terminal": "readiness_timeout", "max_depth": 11}
            def play(*_):
                initial = json.loads((output / "summary.json").read_text(encoding="utf-8"))
                self.assertEqual(initial["inputs"]["seeds"], [50])
                self.assertEqual(initial["results"], [])
                return result

            for terminal in ("readiness_timeout", "unknown_outcome"):
                result["terminal"] = terminal
                output = root / terminal
                args[-2] = str(output)
                with self.subTest(terminal=terminal), patch("sys.argv", args), \
                        patch("rogue_o_matic_cohort.play", side_effect=play), redirect_stdout(StringIO()):
                    self.assertEqual(main(), 1)
                summary = json.loads((output / "summary.json").read_text(encoding="utf-8"))
                self.assertEqual(summary["results"], [result])

    def test_counts_victory_and_distinguishes_disclosed_and_undisclosed_deaths(self):
        rows = [
            {"seed": 42, "terminal": "victory", "max_depth": 26, "amulet_acquired": True,
             "victory_verified": True},
            {"seed": 43, "terminal": "death", "max_depth": 19, "last_combat_events": [
                {"type": "ability", "target": "observer", "outcome": "applied",
                 "ability": {"id": "rogue.energy_drain"}},
                {"type": "died", "actor": "observer"},
            ]},
            {"seed": 44, "terminal": "death", "max_depth": 15},
            {"seed": 45, "terminal": "wall_timeout"},
        ]
        result = summarize(rows)
        self.assertEqual(result["victories"], 1)
        self.assertEqual(result["amulets_acquired"], 1)
        self.assertEqual(result["max_depth"], 26)
        self.assertEqual(result["deaths_by_cause"], {"rogue.energy_drain": 1, "undisclosed": 1})
        self.assertEqual(result["results"], rows)

    def test_attributes_injury_through_the_players_interrupted_intention(self):
        from rogue_o_matic_cohort import death_cause

        events = [
            {"type": "effect", "ability": {"id": "rogue.venomous_bite"}},
            {"type": "ability", "caster": "snake", "target": "player", "outcome": "applied",
             "ability": {"id": "rogue.venomous_bite"}},
            {"type": "interrupted", "actor": "player"},
            {"type": "died", "actor": "player", "cause": "injury"},
        ]
        self.assertEqual(death_cause({"last_combat_events": events}), "rogue.venomous_bite")
        self.assertEqual(death_cause({"last_combat_events": events[-1:]}), "injury")
        events.append({"type": "died", "actor": "other", "cause": "starvation"})
        self.assertEqual(death_cause({"last_combat_events": events, "self_target": "player"}),
                         "rogue.venomous_bite")

    def test_only_the_players_interruptions_can_bridge_an_attack_and_death(self):
        from rogue_o_matic_cohort import death_cause

        attack = {"type": "ability", "caster": "snake", "target": "hero", "outcome": "applied",
                  "ability": {"id": "rogue.venomous_bite"}}
        death = {"type": "died", "actor": "hero", "cause": "injury"}
        for actor, expected in (("hero", "rogue.venomous_bite"), ("other", "injury")):
            with self.subTest(actor=actor):
                events = [attack, {"type": "interrupted", "actor": actor},
                          {"type": "interrupted", "actor": "hero"}, death]
                self.assertEqual(death_cause({"self_target": "hero", "last_combat_events": events}), expected)

    def test_attributes_an_applied_unseen_attack_without_inventing_its_identity(self):
        from rogue_o_matic_cohort import death_cause

        attack = {"type": "unseen_ability", "caster": None, "target": "hero", "outcome": "applied"}
        death = {"type": "died", "actor": "hero", "cause": "injury"}
        report = {"self_target": "hero", "last_combat_events": [
            attack, {"type": "interrupted", "actor": "hero"}, death,
        ]}
        self.assertEqual(death_cause(report), "unseen_ability")
        attack["outcome"] = "miss"
        self.assertEqual(death_cause(report), "injury")
        attack.update(outcome="applied", target="other")
        self.assertEqual(death_cause(report), "injury")

    def test_reports_starvation_instead_of_the_last_self_targeted_search(self):
        events = [{"type": "ability", "caster": "player", "target": "player",
                   "outcome": "applied", "ability": {"id": "rogue.search"}},
                  {"type": "died", "actor": "player"}]
        rows = [
            {"terminal": "death", "last_combat_events": events,
             "last_nutrition": {"kind": "required", "reserve": -3, "starve_at": -3}},
            {"terminal": "death", "last_combat_events": events},
            {"terminal": "death", "last_combat_events": [
                {**events[0], "ability": {"id": "rogue.wand_lightning"}}, events[1]]},
        ]
        self.assertEqual(summarize(rows)["deaths_by_cause"],
                         {"starvation": 1, "undisclosed": 1, "rogue.wand_lightning": 1})

    def test_closest_return_floor_excludes_outbound_travel(self):
        from pathlib import Path
        import tempfile

        from rogue_o_matic_cohort import trace_report

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "turns.jsonl"
            rows = []
            for floor, acquired, expected in [(1, False, None), (26, True, 26),
                                              (12, True, 12), (13, True, 12)]:
                rows.append({"floor": floor, "amulet_acquired": acquired, "action": {"type": "wait"}})
                path.write_text("\n".join(map(json.dumps, rows)), encoding="utf-8")
                _, report = trace_report(path)
                self.assertEqual(report["closest_return_floor"], expected)

    def test_unverified_claim_does_not_count_as_a_victory(self):
        self.assertEqual(summarize([{"terminal": "victory"}])["victories"], 0)

    def test_counts_confirmed_search_and_travel_and_records_reserve_by_floor(self):
        from pathlib import Path
        import tempfile

        from rogue_o_matic_cohort import trace_report

        def turn(floor, action, status, reserve):
            return {"floor": floor, "action": action, "result": {"status": status}, "nutrition_reserve": reserve}

        rows = [turn(1, {"type": "use_ability", "id": "rogue.search"}, "confirmed", 300),
                turn(1, {"type": "use_ability", "id": "rogue.search"}, "rejected", 280),
                turn(1, {"type": "move", "direction": "down"}, "confirmed", 250),
                turn(2, {"type": "move", "direction": "west"}, "confirmed", 240),
                turn(1, {"type": "wait"}, "confirmed", 310)]
        with tempfile.TemporaryDirectory() as directory:
            trace = Path(directory) / "turns.jsonl"
            trace.write_text("".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8")
            _, progress = trace_report(trace)
        self.assertEqual(progress["floors"], {
            "1": {"searches_admitted": 1, "searches_resolved": 0, "searches_failed": 0, "moves_admitted": 1, "first_reserve": 300,
                  "last_reserve": 310, "lowest_reserve": 250},
            "2": {"searches_admitted": 0, "searches_resolved": 0, "searches_failed": 0, "moves_admitted": 1, "first_reserve": 240,
                  "last_reserve": 240, "lowest_reserve": 240},
        })
        self.assertEqual(progress["stair_transitions"], [
            {"from": 1, "to": 2, "before_reserve": 250, "after_reserve": 240},
            {"from": 2, "to": 1, "before_reserve": 240, "after_reserve": 310},
        ])

    def test_prices_resolved_supply_steps_and_counts_reversals_without_new_terrain(self):
        from pathlib import Path
        import tempfile
        from rogue_o_matic_cohort import trace_report

        rows = [{"floor": 1, "tick": tick, "cell": square, "known_cells": size,
                 "action": {"type": "move"}, "reason": reason, "nutrition_reserve": reserve,
                 "result": {"status": "confirmed", "receipt": {"intention": str(tick)}},
                 "previous_intention_id": str(tick - 100), "previous_intention_phase": "resolved"}
                for tick, square, size, reason, reserve in [
                    (0, "a", 9, "seek_supplies", 20), (100, "b", 9, "explore", 18),
                    (200, "a", 9, "explore", 17), (300, "b", 10, "explore", 16)]]
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "turns.jsonl"
            log.write_text("".join(json.dumps(r) + "\n" for r in rows), encoding="utf-8")
            _, progress = trace_report(log)
        self.assertEqual(progress["supply_detours"], {"ticks": 100, "reserve_spent": 2, "resolved_steps": 1})
        self.assertEqual(progress["reversals_without_new_terrain"], 1)

    def test_correlates_search_resolution_with_its_admission_floor(self):
        from pathlib import Path
        import tempfile

        from rogue_o_matic_cohort import trace_report

        def search(intention):
            return {"floor": 1, "action": {"type": "use_ability", "id": "rogue.search"},
                    "result": {"status": "confirmed", "receipt": {"intention": intention}}}

        rows = [search("first"), search("second"), search("third"),
                {"floor": 1, "action": {"type": "wait"}, "previous_intention_id": "first",
                 "previous_intention_phase": "started"},
                {"floor": 1, "action": {"type": "wait"}, "previous_intention_id": "second",
                 "previous_intention_phase": "queued"},
                {"floor": 2, "action": {"type": "wait"}, "previous_intention_id": "first",
                 "previous_intention_phase": "resolved"},
                {"terminal": "action_budget", "previous_intention_id": "second",
                 "previous_intention_phase": "failed"}]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "turns.jsonl"
            path.write_text("".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8")
            _, progress = trace_report(path)
        floor = progress["floors"]["1"]
        self.assertEqual((floor["searches_admitted"], floor["searches_resolved"],
                          floor["searches_failed"]), (3, 1, 1))

    def test_partial_trace_preserves_depth_and_actions_when_the_last_row_is_incomplete(self):
        from pathlib import Path
        import tempfile
        from rogue_o_matic_cohort import trace_report

        with tempfile.TemporaryDirectory() as directory:
            trace = Path(directory) / "turns.jsonl"
            trace.write_text(
                '{"action":{"type":"wait"},"floor":1}\n'
                '{"action":{"type":"move"},"floor":3,"amulet_acquired":true}\n'
                '{"action":', encoding="utf-8")
            terminal, progress = trace_report(trace)
        self.assertIsNone(terminal)
        self.assertEqual((progress["actions"], progress["max_depth"]), (2, 3))
        self.assertTrue(progress["amulet_acquired"])
        self.assertIsNone(progress["floors"]["1"]["lowest_reserve"])
