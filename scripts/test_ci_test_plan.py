"""Independent risk-plan expectations: malformed scope must never narrow CI."""
import contextlib
import io
import json
import os
from pathlib import Path
import tempfile
import subprocess
import sys
import unittest
from unittest.mock import patch

import ci_test_plan as plan


class PlanTests(unittest.TestCase):
    def retained_plan_at_identify_scale(self, name_length):
        normal = {**plan.full_plan('a' * 40, 'b' * 40, []), 'mode': 'selected',
                  'assurance': 'selected-pr-contracts', 'requires_apps': True}
        normal['lanes'] = [
            {'id': lane, 'os': system, 'profile': 'debug', 'partition': 'python',
             'test_ids': [f'test_replay.Processes.test_{index:04d}_' + 'x' * name_length
                          for index in range(count)]}
            for lane, system, count in (('linux-debug-python', 'ubuntu-latest', 3171),
                                        ('windows-debug-python', 'windows-latest', 3164))
        ]
        retained = {**normal, 'mode': 'postmerge', 'consumer_3d': False, 'lanes': [],
                    'assurance': 'prior-exact-tree-selected-plus-fresh-main',
                    'normal_selection': normal,
                    'postmerge_proof': {'qualification': {'selection': {'lanes': normal['lanes']}},
                                        'current_toolchain': {'release': '1.100.0'}}}
        return normal, retained

    def publish_retained_candidate(self, directory, normal, retained):
        output = directory / 'plan.json'
        job_outputs = directory / 'github-output'
        event = directory / 'event.json'
        event.write_text('{}', encoding='utf-8')
        with (patch.object(plan, 'normal_plan', return_value=normal),
              patch.object(plan.ci_scope, 'classify', return_value={'docs_only': False}),
              patch('ci_contract_reuse.resolve', return_value=retained),
              patch.dict(os.environ, {'GITHUB_EVENT_NAME': 'push', 'GITHUB_OUTPUT': str(job_outputs),
                                      'GITHUB_EVENT_PATH': str(event)}),
              contextlib.redirect_stdout(io.StringIO())):
            self.assertEqual(plan.main(['--output', str(output)]), 0)
        return output, job_outputs.read_text(encoding='utf-8').splitlines()

    def test_real_size_retained_plan_reaches_both_postmerge_consumers(self):
        import ci_postmerge as postmerge
        normal, retained = self.retained_plan_at_identify_scale(122)
        self.assertGreater(len(json.dumps(retained, indent=2).encode()), postmerge.MAX_BYTES)
        with tempfile.TemporaryDirectory() as folder:
            directory = Path(folder)
            output, decisions = self.publish_retained_candidate(directory, normal, retained)
            self.assertIn('mode=postmerge', decisions)
            with patch.object(postmerge, 'smoke') as smoke:
                self.assertEqual(postmerge.main(['smoke', '--plan', str(output)]), 0)
            smoke.assert_called_once_with(retained)
            with patch.object(postmerge, 'validate_smoke') as aggregate, patch.dict(os.environ, {'GITHUB_STEP_SUMMARY': ''}):
                self.assertEqual(postmerge.main(['aggregate', '--plan', str(output), '--directory', folder]), 0)
            aggregate.assert_called_once_with(retained, directory)

    def test_retained_plan_above_wire_budget_leaves_fresh_selected_jobs_enabled(self):
        import ci_postmerge as postmerge
        normal, retained = self.retained_plan_at_identify_scale(150)
        self.assertGreater(len(json.dumps(retained, separators=(',', ':')).encode()), postmerge.MAX_BYTES)
        with tempfile.TemporaryDirectory() as folder:
            output, decisions = self.publish_retained_candidate(Path(folder), normal, retained)
            self.assertIn('mode=selected', decisions)
            self.assertIn('postmerge_release=', decisions)
            fresh = postmerge.decode(output.read_bytes())
            self.assertEqual(fresh.pop('postmerge_optimization')['status'], 'declined')
            self.assertEqual(fresh, normal)
            self.assertNotIn('postmerge_proof', fresh)

    def complete_selection(self, paths):
        import ci_test_contracts as contracts
        inventory, metadata = plan.discover_inventory()
        selected = contracts.select(paths, inventory, root=plan.ROOT, test_metadata=metadata)
        return inventory, metadata, selected

    def test_reviewed_catalog_preserves_assertions_and_resource_boundaries(self):
        import ci_test_contracts as contracts
        catalog = json.loads((plan.ROOT / 'scripts/testdata/ci-ownership/review-20261110.json').read_text())
        rows = catalog['owners']
        self.assertEqual(len(rows), 53)
        self.assertEqual(len({row['owner'] for row in rows}), 53)
        self.assertEqual(sum(row['resource'] == 'application' for row in rows), 3)
        self.assertEqual(sum(row['resource'] == 'tooling' for row in rows), 50)
        inventory, metadata = plan.discover_inventory()
        for row in rows:
            with self.subTest(owner=row['owner']):
                module, cls = row['owner'].split('.')
                self.assertTrue(row['assertion_review'])
                self.assertTrue(row['witness_methods'])
                self.assertIn(cls, contracts.REVIEWED_OWNERS.get(module, ()))
                for method in row['witness_methods']:
                    identity = row['owner'] + '.' + method
                    self.assertIn(identity, inventory)
                    self.assertEqual(metadata[identity], {
                        'process': row['resource'] == 'application', 'graphical': False})

    def test_model_diagnostic_owners_preserve_tooling_and_fail_closed_boundaries(self):
        import ci_test_contracts as contracts
        import model_performance_report
        catalog = json.loads((plan.ROOT / 'scripts/testdata/ci-ownership/review-20261116.json').read_text())
        inventory, metadata = plan.discover_inventory()
        self.assertEqual(len(catalog['owners']), 2)
        for row in catalog['owners']:
            module, owner = row['owner'].split('.')
            identities = {row['owner'] + '.' + method for method in row['witness_methods']}
            self.assertEqual(len(identities), 3)
            self.assertEqual({i for i in inventory if i.startswith(row['owner'] + '.')}, identities)
            self.assertEqual(row['resource'], 'tooling')
            self.assertTrue(row['assertion_review'])
            self.assertIn(owner, contracts.REVIEWED_OWNERS[module])
            for identity in identities:
                self.assertEqual(metadata[identity], {'process': False, 'graphical': False})
            for path in ('scripts/' + module + '.py',
                         'scripts/' + module.removeprefix('test_') + '.py'):
                selected = contracts.select([path], inventory, root=plan.ROOT, test_metadata=metadata)
                self.assertFalse(selected['full_required'], selected['reasons'])
                self.assertFalse(selected['requires_apps'])
                for lane in ('linux_debug', 'windows_debug'):
                    self.assertTrue(identities <= set(selected[lane]), (path, lane))
            # Shared validator reaches broader literal CLI edges. Preserve
            # fail-closed full coverage until those edges receive their own review.
            shared = contracts.select(['scripts/' + model_performance_report.__name__ + '.py'],
                                      inventory, root=plan.ROOT, test_metadata=metadata)
            self.assertTrue(shared['full_required'])
            self.assertIn('cannot resolve Python source dependencies', shared['reasons'])
            unknown = module + '.UnreviewedBoundary.test_requires_review'
            selected = contracts.select(['scripts/' + module + '.py'], inventory + [unknown],
                                        root=plan.ROOT, test_metadata={**metadata, unknown: {
                                            'process': False, 'graphical': False}})
            self.assertTrue(selected['full_required'])
            self.assertIn('unreviewed owner: ' + module + '.UnreviewedBoundary', selected['reasons'])

    def test_complete_inventory_supports_tooling_without_application_builds(self):
        import rogue_species_tuning
        import test_model_performance_report
        for module in (rogue_species_tuning, test_model_performance_report):
            path = 'scripts/' + module.__name__ + '.py'
            with self.subTest(path=path):
                inventory, metadata, selected = self.complete_selection([path])
                self.assertFalse(selected['full_required'], selected['reasons'][:3])
                self.assertFalse(selected['requires_apps'])
                self.assertFalse(selected['requires_rust'])
                self.assertEqual(selected['linux_release'], [])
                result = plan.make_plan('pull_request', 'a' * 40, 'b' * 40, [path], inventory, selected)
                self.assertEqual(result['mode'], 'selected', result['reasons'])
                self.assertEqual([lane['id'] for lane in result['lanes']], ['linux-tooling', 'windows-tooling'])
                for lane in result['lanes']:
                    self.assertTrue(lane['test_ids'])
                    self.assertTrue(all(not metadata[identity]['process'] for identity in lane['test_ids']))

    def test_complete_inventory_keeps_applications_and_fail_closed_escalation(self):
        import ci_cost_report
        import ci_test_contracts as contracts
        inventory, metadata, selected = self.complete_selection(['crates/simulation/src/lib.rs'])
        self.assertFalse(selected['full_required'], selected['reasons'][:3])
        self.assertTrue(selected['requires_apps'])
        self.assertTrue(selected['requires_rust'])
        for prefix in ('test_rogue_item_stages_process.RogueItemStageProcesses.',
                       'test_save_backpressure_process.SaveBackpressure.',
                       'test_scenario_capabilities_process.ScenarioCapabilityProcesses.'):
            ids = [identity for identity in inventory if identity.startswith(prefix)]
            self.assertTrue(ids, prefix)
            self.assertTrue(set(ids) <= set(selected['linux_debug']))
        self.assertTrue(set(contracts.SMOKE_WINDOWS) <= set(selected['windows_debug']))
        self.assertTrue(set(contracts.SMOKE_RELEASE) <= set(selected['linux_release']))
        for path in ('.github/workflows/ci.yml', '.github/workflows/dependency-advisories.yml',
                     'Cargo.toml', '../escape', 'scripts/missing-reviewed-source.py',
                     'scripts/testdata/ci-ownership/review-20261110.json'):
            with self.subTest(path=path):
                result = contracts.select([path], inventory, root=plan.ROOT, test_metadata=metadata)
                self.assertTrue(result['full_required'])
        for path in ('scripts/ci_cost_report.py', 'scripts/test_ci_test_plan.py'):
            result = plan.make_plan('pull_request', 'a' * 40, 'b' * 40, [path], inventory, selected)
            self.assertEqual(result['mode'], 'full', 'Verification controls must never take the tooling fast path')
        unknown = 'test_ci_cost_report.UnreviewedBoundary.test_requires_review'
        result = contracts.select(['scripts/' + ci_cost_report.__name__ + '.py'], inventory + [unknown],
                                  root=plan.ROOT, test_metadata={**metadata, unknown: {
                                      'process': False, 'graphical': False}})
        self.assertTrue(result['full_required'])
        self.assertIn('unreviewed owner: test_ci_cost_report.UnreviewedBoundary', result['reasons'])

    def test_actual_planner_process_selects_reviewed_tooling(self):
        code = '''
import json, sys
sys.path.insert(0, 'scripts')
import ci_test_contracts, ci_test_plan
inventory, metadata = ci_test_plan.discover_inventory()
paths = ['scripts/' + sys.argv[1] + '.py']
selection = ci_test_contracts.select(paths, inventory, root=ci_test_plan.ROOT, test_metadata=metadata)
result = ci_test_plan.make_plan('pull_request', 'a' * 40, 'b' * 40, paths, inventory, selection)
print(json.dumps({'mode': result['mode'], 'apps': result.get('requires_apps'),
                  'lanes': [lane['id'] for lane in result['lanes']]}))
'''
        for module in ('rogue_species_tuning', 'test_model_performance_report'):
            with self.subTest(module=module):
                result = subprocess.run([sys.executable, '-c', code, module], cwd=plan.ROOT,
                                        capture_output=True, text=True, check=True)
                self.assertEqual(json.loads(result.stdout), {'mode': 'selected', 'apps': False,
                                                             'lanes': ['linux-tooling', 'windows-tooling']})

    def test_actual_planner_selects_api_and_live_bot_tests_on_both_platforms(self):
        code = """
import sys
sys.path.insert(0, 'scripts')
from ci_test_plan import ROOT, discover_inventory, make_plan
from ci_test_contracts import select
inventory, metadata = discover_inventory()
for path in sys.argv[1:]:
    selection = select([path], inventory, root=ROOT, test_metadata=metadata)
    plan = make_plan('pull_request', 'a' * 40, 'b' * 40, [path], inventory, selection)
    assert plan['mode'] == 'selected', (path, plan['reasons'])
    lanes = {lane['id']: lane for lane in plan['lanes']}
    assert plan['requires_apps'] and not plan['requires_rust']
    modules = {'test_rogue_bot_process', 'test_rogue_o_matic', 'test_rogue_o_matic_cohort'}
    if path.startswith('crates/'):
        modules.add('test_bot_api')
    required = {identity for identity in inventory if identity.split('.')[0] in modules}
    for lane in ('linux-debug-python', 'windows-debug-python'):
        assert required <= set(lanes[lane]['test_ids']), (path, lane)
path = 'scripts/rogue_policy_fixture.py'
selection = select([path], inventory, root=ROOT, test_metadata=metadata)
fixture_plan = make_plan('pull_request', 'a' * 40, 'b' * 40, [path], inventory, selection)
assert fixture_plan['mode'] == 'selected', fixture_plan['reasons']
lanes = {lane['id']: set(lane['test_ids']) for lane in fixture_plan['lanes']}
for module in ('test_rogue_o_matic', 'test_rogue_o_matic_reference'):
    tests = {identity for identity in inventory if identity.split('.')[0] == module}
    assert tests and all(tests <= lanes[lane] for lane in ('linux-tooling', 'windows-tooling')), module
"""
        paths = ['crates/client-bot/python/tor_client_bot/game.py', 'scripts/' + 'rogue_player' + '.py']
        run = subprocess.run([sys.executable, '-c', code, *paths], cwd=plan.ROOT, capture_output=True, text=True)
        self.assertEqual(run.returncode, 0, run.stderr)

    def test_reviewed_reporter_cli_edges_cover_actual_validator_routes(self):
        import ci_test_contracts as contracts
        import perf_compare
        for workload, module in (('equipment', 'equipment_performance_report'),
                                  ('models', 'model_performance_report')):
            with self.subTest(workload=workload):
                self.assertEqual(perf_compare.WORKLOADS[workload].validator, 'scripts/' + module + '.py')
                self.assertIn('perf_compare', contracts.RUNTIME_SCRIPT_EDGES.get(module, ()))

    def test_composed_application_owners_are_discovered_as_process_tests(self):
        inventory, metadata = plan.discover_inventory()
        owners = ('test_rogue_campaign.CampaignProcess.',
                  'test_rogue_encounter_study.EncounterProcess.')
        for prefix in owners:
            identities = [identity for identity in inventory if identity.startswith(prefix)]
            self.assertTrue(identities, prefix)
            for identity in identities:
                self.assertTrue(metadata[identity]['process'], identity)
        pure = [identity for identity in inventory
                if identity.startswith('test_rogue_encounter_study.EncounterPackages.')]
        self.assertTrue(pure)
        self.assertTrue(all(not metadata[identity]['process'] for identity in pure))

    def test_tooling_selection_excludes_composed_application_owners(self):
        import ci_cost_report
        import ci_test_contracts as contracts
        import test_rogue_campaign
        import test_rogue_encounter_study
        inventory, metadata = plan.discover_inventory()
        # Exercise complete discovery; newly unknown owners must still escalate.
        result = contracts.select(['scripts/' + ci_cost_report.__name__ + '.py'], inventory,
                                  root=plan.ROOT, test_metadata=metadata)
        self.assertFalse(result['full_required'], result['reasons'])
        self.assertFalse(result['requires_apps'])
        for lane in ('linux_debug', 'windows_debug'):
            self.assertFalse([identity for identity in result[lane] if identity.startswith(
                ('test_rogue_campaign.CampaignProcess.', 'test_rogue_encounter_study.EncounterProcess.'))])
        for module, owner in (
            (test_rogue_campaign, 'CampaignProcess'),
            (test_rogue_encounter_study, 'EncounterProcess'),
        ):
            path, prefix = 'scripts/' + module.__name__ + '.py', module.__name__ + '.' + owner + '.'
            result = contracts.select([path], inventory, root=plan.ROOT, test_metadata=metadata)
            self.assertFalse(result['full_required'], result['reasons'])
            self.assertTrue(result['requires_apps'], path)
            for lane in ('linux_debug', 'windows_debug'):
                self.assertTrue(any(identity.startswith(prefix) for identity in result[lane]), lane)

    def selection(self, **changes):
        value = {"full_required": False, "reasons": ["reviewed"], "contracts": ["wire"],
                 "linux_debug": ["test_wire_process.Wire.test_roundtrip"],
                 "windows_debug": ["test_native_process.Native.test_input"],
                 "linux_release": ["test_wire_process.Wire.test_roundtrip"], "consumer_3d": False, "requires_apps": True, "requires_rust": True}
        return {**value, **changes}

    def test_non_pr_events_are_exhaustive_even_when_paths_are_known(self):
        for event in ("schedule", "workflow_dispatch", "release", "unknown"):
            with self.subTest(event=event):
                result = plan.make_plan(event, "a" * 40, "b" * 40, ["crates/protocol/src/lib.rs"],
                                        ["test_wire_process.Wire.test_roundtrip"], self.selection())
                self.assertEqual(result["mode"], "full")
                self.assertEqual(result["lanes"], [])
                self.assertTrue(result["consumer_3d"])

    def test_selected_assurance_is_not_full_and_contains_platform_release_owners(self):
        result = plan.make_plan("pull_request", "a" * 40, "b" * 40,
                                ["crates/protocol/src/lib.rs"],
                                ["test_wire_process.Wire.test_roundtrip", "test_native_process.Native.test_input"], self.selection())
        self.assertEqual(result["mode"], "selected")
        self.assertEqual([row["id"] for row in result["lanes"]],
                         ["linux-debug-rust", "linux-debug-python", "windows-debug-python", "linux-release-python"])
        self.assertEqual(result["assurance"], "selected-pr-contracts")
        self.assertEqual(result["policy_version"], plan.POLICY_VERSION)

    def test_consumer_is_separate_from_full_renderer(self):
        result = plan.make_plan("pull_request", "a" * 40, "b" * 40, ["crates/protocol/src/lib.rs"],
            ["test_wire_process.Wire.test_roundtrip", "test_native_process.Native.test_input"], self.selection(consumer_3d=True))
        self.assertEqual(result["lanes"][-1]["id"], "linux-3d-consumer")
        self.assertNotIn("render", str(result))

    def test_control_and_toolchain_changes_cannot_choose_their_own_fast_gate(self):
        for path in ("scripts/ci_test_plan.py", "scripts/ci_test_contracts.py", "scripts/ci_test_inventory.json",
                     ".github/workflows/ci.yml", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml"):
            result = plan.make_plan("pull_request", "a" * 40, "b" * 40, [path], [], self.selection())
            self.assertEqual(result["mode"], "full", path)
            self.assertTrue(result["consumer_3d"])

    def test_unreviewed_or_invalid_selection_falls_back(self):
        for selection in (self.selection(full_required=True), self.selection(linux_debug=[]),
                          self.selection(linux_debug=["not.a.test"]), self.selection(consumer_3d="false")):
            result = plan.make_plan("pull_request", "a" * 40, "b" * 40, ["crates/protocol/src/lib.rs"], [], selection)
            self.assertEqual(result["mode"], "full")
            self.assertTrue(result["consumer_3d"])

    def test_raw_changes_keep_both_deleted_and_added_names(self):
        raw = b":100644 000000 " + b"a" * 40 + b" " + b"0" * 40 + b" D\0scripts/old.py\0"
        raw += b":000000 100644 " + b"0" * 40 + b" " + b"b" * 40 + b" A\0scripts/new.py\0"
        self.assertEqual(plan.diff_paths(raw), ["scripts/new.py", "scripts/old.py"])

    def test_raw_diff_rejects_symlinks_duplicates_and_truncated_input(self):
        row = b":100644 100644 " + b"a" * 40 + b" " + b"b" * 40 + b" M\0scripts/thing.py\0"
        for raw in (row[:-1], row + row, row.replace(b":100644", b":120000"), row.replace(b"scripts/thing.py", b"../escape")):
            with self.subTest(raw=raw), self.assertRaises(ValueError):
                plan.diff_paths(raw)

    def test_missing_scope_is_full_not_empty_selected(self):
        with patch.object(plan, "scope_paths", side_effect=ValueError("bad evidence")):
            result = plan.current_plan(event_name="pull_request", event={}, source_sha="a" * 40)
        self.assertEqual(result["mode"], "full")
        self.assertIn("scope", result["reasons"][0])

    def test_digest_binds_selection_source_and_policy(self):
        value = {"policy_version": 2, "source_sha": "a" * 40, "ids": ["a", "b"]}
        self.assertEqual(plan.digest(value), plan.digest(dict(reversed(list(value.items())))))
        self.assertNotEqual(plan.digest(value), plan.digest({**value, "ids": ["a"]}))

    def test_pure_tooling_plan_has_two_no_application_lanes(self):
        selection = self.selection(requires_rust=False, requires_apps=False, linux_release=[])
        inventory = ["test_wire_process.Wire.test_roundtrip", "test_native_process.Native.test_input"]
        result = plan.make_plan("pull_request", "a" * 40, "b" * 40, ["scripts/report.py"], inventory, selection)
        self.assertEqual([lane["id"] for lane in result["lanes"]], ["linux-tooling", "windows-tooling"])
        self.assertFalse(result["requires_apps"])
        self.assertFalse(result["requires_rust"])

    def test_process_only_plan_builds_apps_without_redundant_rust_suite(self):
        selection = self.selection(requires_rust=False)
        inventory = ["test_wire_process.Wire.test_roundtrip", "test_native_process.Native.test_input"]
        result = plan.make_plan("push", "a" * 40, "b" * 40, ["scripts/test_wire_process.py"], inventory, selection)
        self.assertEqual(result["lanes"][0]["id"], "linux-debug-build")
        self.assertEqual(result["mode"], "selected")

    def test_main_scope_requires_complete_nonforced_ancestral_change(self):
        source, before = "a" * 40, "b" * 40
        raw = b":100644 100644 " + b"c" * 40 + b" " + b"d" * 40 + b" M\0scripts/report.py\0"
        def git(root, *args):
            if args == ("rev-parse", "--is-shallow-repository"):
                return b"false\n"
            if args == ("rev-parse", "HEAD"):
                return source.encode()
            if args[0] == "diff":
                return raw
            self.assertEqual(args, ("merge-base", "--is-ancestor", before, source))
            return b""
        event = {"before": before, "after": source, "created": False, "deleted": False,
                 "forced": False, "ref": "refs/heads/main"}
        with patch.object(plan.ci_scope, "git", side_effect=git):
            self.assertEqual(plan.scope_paths(plan.ROOT, "push", event, source), ["scripts/report.py"])
            for field, value in (("forced", True), ("created", True), ("deleted", True),
                                 ("before", "0" * 40), ("ref", "refs/heads/other"), ("after", "f" * 40)):
                with self.subTest(field=field), self.assertRaises(ValueError):
                    plan.scope_paths(plan.ROOT, "push", {**event, field: value}, source)


if __name__ == "__main__":
    unittest.main()
