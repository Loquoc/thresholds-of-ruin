"""Selected proof boundaries use independently mutated receipts."""
from copy import deepcopy
import importlib
import base64
import contextlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import ci_postmerge
import ci_test_plan as planner
import test_ci_postmerge as postmerge_tests


class ContractReuseTests(unittest.TestCase):
    def module(self):
        self.assertIsNotNone(importlib.util.find_spec('ci_contract_reuse'), 'Missing exact-tree contract reuse')
        return importlib.import_module('ci_contract_reuse')

    def fixture(self, kind='selected', *, smoke=False):
        module = self.module()
        base = postmerge_tests.PostmergeTests()
        base.fixture()
        plan = {'mode': 'selected', 'policy_version': planner.POLICY_VERSION, 'inventory_sha256': '9' * 64,
                'requires_apps': False, 'requires_rust': False, 'consumer_3d': False, 'contracts': ['tooling'],
                'lanes': [{'id': prefix + '-tooling', 'os': system, 'profile': 'debug', 'partition': 'tooling',
                           'test_ids': ['fixture.Pure.test_a']}
                          for prefix, system in [('linux', 'ubuntu-latest'), ('windows', 'windows-latest')]]}
        if smoke:
            plan['consumer_3d'] = True
            plan['contracts'].append('native')
            plan['lanes'].append({'id': 'linux-3d-consumer', 'os': 'ubuntu-latest', 'profile': 'debug',
                                  'partition': 'consumer', 'test_ids': []})
        contract = module.Contract(kind, plan)
        roles = [lane['id'] for lane in plan['lanes']]
        base.qualified.update(assurance=contract.assurance,
                              selection=module.selection(plan),
                              groups=contract.groups(),
                              toolchains={role: dict(base.compiler) for role in roles})
        base.qualified['configurations'] = {role: {'cargo_config': [], 'profile': {
            'CARGO_INCREMENTAL': None, 'CARGO_PROFILE_DEV_DEBUG': '0', 'CARGO_PROFILE_RELEASE_DEBUG': None}}
            for role in roles}
        successful, skipped = contract.job_roles(base.qualified)
        base.jobs = [{'name': name, 'run_id': 123, 'head_sha': 'b' * 40, 'run_attempt': 1,
                      'status': 'completed', 'conclusion': 'skipped' if name in skipped else 'success'}
                     for name in sorted(successful | skipped)]
        responses = base.responses()
        runs_key = next(key for key in responses if key.startswith('/actions/workflows/'))
        responses[runs_key]['workflow_runs'][0]['path'] = contract.workflow
        responses[runs_key.replace('ci.yml', Path(contract.workflow).name)] = responses.pop(runs_key)
        responses['/actions/runs/123/artifacts?per_page=100']['artifacts'][0]['name'] = contract.artifact + '-attempt-1'
        class Api:
            def get(self, path):
                return responses[path]
            def raw(self, path):
                return responses[path]
        return module, contract, base, responses, Api()

    def test_selected_accepts_only_the_exact_tree_and_canonical_lane_union(self):
        kind = 'selected'
        module, contract, base, responses, api = self.fixture(kind)
        proof = ci_postmerge.fetch_candidate(base.current, base.event, api, now=base.now, contract=contract)
        self.assertEqual(proof['qualification']['assurance'], 'fresh-' + kind + '-pr')
        self.assertEqual(proof['main']['source_tree'], proof['qualification']['source_tree'])
        self.assertEqual(len(proof['qualification']['toolchains']), 2)

    def test_selected_smoke_proof_rejects_missing_failed_and_retired_authority(self):
        module, contract, base, _, api = self.fixture(smoke=True)
        proof = ci_postmerge.fetch_candidate(base.current, base.event, api, now=base.now, contract=contract)
        self.assertIn('linux-3d-consumer', proof['qualification']['toolchains'])
        smoke_name = '3D compatibility (linux-3d-consumer)'
        for conclusion in ('failure', 'cancelled', 'skipped', None):
            jobs = [dict(job) for job in base.jobs if job['name'] != smoke_name or conclusion is not None]
            for job in jobs:
                if job['name'] == smoke_name:
                    job['conclusion'] = conclusion
            with self.subTest(conclusion=conclusion), self.assertRaises(ValueError):
                contract.validate_jobs(jobs, base.run, base.qualified)
        with self.assertRaisesRegex(ValueError, 'unknown proof kind'):
            module.Contract('3d', contract.plan)
        with self.assertRaisesRegex(ValueError, 'incorrect qualification kind'):
            contract.validate(dict(base.qualified, assurance='fresh-3d-pr'), base.current)

    def test_mutated_merge_run_artifact_and_job_metadata_fail_closed(self):
        kind = 'selected'
        for mutation in ('direct', 'base', 'fork', 'newer-failure', 'expired', 'digest', 'partial', 'missing-lane'):
            with self.subTest(kind=kind, mutation=mutation):
                module, contract, base, responses, api = self.fixture(kind)
                if mutation == 'direct': base.current['parents'] = ['a' * 40]
                elif mutation == 'base': base.event['before'] = '0' * 40
                elif mutation == 'fork': base.pr['head']['repo']['full_name'] = 'foreign/repo'
                elif mutation == 'newer-failure':
                    runs = responses[next(key for key in responses if key.startswith('/actions/workflows/'))]
                    runs['workflow_runs'].append(dict(base.run, id=124, run_number=100, conclusion='failure'))
                    runs['total_count'] = 2
                elif mutation == 'expired': responses['/actions/runs/123/artifacts?per_page=100']['artifacts'][0]['expired'] = True
                elif mutation == 'digest': responses['/actions/runs/123/artifacts?per_page=100']['artifacts'][0]['digest'] = 'sha256:' + '0' * 64
                elif mutation == 'partial': base.jobs[0]['run_attempt'] = 2
                else: base.jobs.pop()
                with self.assertRaises(ValueError):
                    ci_postmerge.fetch_candidate(base.current, base.event, api, now=base.now, contract=contract)

    def test_contract_rejects_selection_inventory_compiler_and_full_promotion(self):
        kind = 'selected'
        module, contract, base, _, _ = self.fixture(kind)
        contract.validate(base.qualified, base.current)
        for mutation in ('selection', 'compiler', 'role', 'groups', 'inventory', 'assurance', 'version', 'configuration', 'profile'):
            row = deepcopy(base.qualified)
            if mutation == 'selection': row['selection'] = {}
            elif mutation == 'compiler': next(iter(row['toolchains'].values()))['commit'] = '2' * 40
            elif mutation == 'role': row['toolchains'].pop(next(iter(row['toolchains'])))
            elif mutation == 'groups': row['groups'] = {}
            elif mutation == 'inventory': row['inventory_sha256'] = '0' * 64
            elif mutation == 'assurance': row['assurance'] = 'fresh-full-pr'
            elif mutation == 'version': row['version'] = True
            elif mutation == 'configuration': row['configurations'] = list(row['configurations'])
            else: next(iter(row['configurations'].values()))['profile'] = ['CARGO_INCREMENTAL', 'CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_RELEASE_DEBUG']
            with self.subTest(kind=kind, mutation=mutation), self.assertRaises(ValueError):
                contract.validate(row, base.current)


    def test_pure_tooling_main_integration_has_no_compilation_or_native_requirement(self):
        self.module()
        steps = ci_postmerge.smoke_steps(2, applications=False)
        self.assertEqual([name for name, _ in steps], ['python-postmerge'])
        self.assertNotIn('cargo', str(steps))

    def test_selected_planner_uses_only_the_selected_resolver(self):
        module = self.module()
        normal = {'mode': 'selected', 'source_sha': 'a' * 40}
        with patch.object(planner, 'normal_plan', return_value=normal), \
                patch.object(planner.ci_scope, 'classify', return_value={'docs_only': False}), \
                patch.object(module, 'resolve', return_value=normal) as resolve, \
                patch.object(ci_postmerge, 'resolve') as full:
            self.assertIs(planner.current_plan(event_name='push', event={}), normal)
            resolve.assert_called_once_with(normal, {}, planner.ROOT)
            full.assert_not_called()

    def test_selected_network_or_compiler_failure_keeps_normal_selected_execution(self):
        module, contract, base, _, _ = self.fixture()
        normal = {**contract.plan, 'source_sha': base.current['source_sha'], 'source_tree': base.current['source_tree']}
        for failure in ('network', 'compiler'):
            proof = {'qualification': base.qualified, 'main': base.current}
            with self.subTest(failure=failure), patch.dict(os.environ, {'GITHUB_EVENT_NAME': 'push'}), \
                    patch.object(ci_postmerge, 'context', return_value=base.current), \
                    patch.object(ci_postmerge, 'fetch_candidate', side_effect=ValueError('network') if failure == 'network' else None,
                                 return_value=proof), \
                    patch.object(module, 'compiler_for', side_effect=ValueError('compiler')):
                result = module.resolve(normal, base.event)
                self.assertEqual(result['mode'], 'selected')
                self.assertEqual(result['lanes'], normal['lanes'])
                self.assertEqual(result['postmerge_optimization']['status'], 'declined')

    def test_qualified_numeric_compiler_is_installed_and_rechecked_without_stable_rollover(self):
        module, _, base, _, _ = self.fixture()
        with patch.object(module.subprocess, 'run') as run, \
                patch.object(ci_postmerge, 'toolchain', wraps=ci_postmerge.toolchain) as fingerprint:
            fingerprint.side_effect = lambda value=None, **kwargs: value if value is not None else dict(base.compiler)
            self.assertEqual(module.compiler_for({'qualification': base.qualified}), base.compiler)
            self.assertEqual(run.call_args.args[0], ['rustup', 'toolchain', 'install', '1.99.1', '--profile', 'minimal'])
            self.assertEqual(fingerprint.call_args.kwargs['environment']['RUSTUP_TOOLCHAIN'], '1.99.1')

    def rows(self, ids, profile='debug'):
        return [{'kind': 'selection', 'schema_version': 1, 'profile': profile, 'test_ids': ids},
                *[{'kind': 'test', 'schema_version': 1, 'profile': profile, 'test_id': identity, 'outcome': 'passed',
                   'failures': 0, 'errors': 0, 'subtests_failed': 0, 'subtests_skipped': 0} for identity in ids],
                {'kind': 'suite', 'schema_version': 1, 'profile': profile, 'successful': True, 'tests_run': len(ids),
                 'failures': 0, 'errors': 0, 'skipped': 0, 'waived': 0, 'expected_failures': 0, 'unexpected_successes': 0}]

    def test_selected_seal_reads_actual_bound_results_and_rejects_tampering(self):
        import ci_no_build
        import ci_selected
        module, contract, base, _, _ = self.fixture()
        current = {**base.current, 'source_sha': 'e' * 40, 'run_id': '123'}
        plan = {**contract.plan, 'source_sha': 'e' * 40, 'source_tree': 'd' * 40}
        event = {'number': 7, 'pull_request': {'draft': False,
                 'base': {'ref': 'main', 'sha': 'a' * 40, 'repo': {'full_name': 'owner/repo'}},
                 'head': {'sha': 'b' * 40, 'repo': {'full_name': 'owner/repo'}}}}
        with tempfile.TemporaryDirectory() as temporary, \
                patch.dict(os.environ, {'GITHUB_EVENT_NAME': 'pull_request', 'GITHUB_RUN_ID': '123', 'GITHUB_RUN_ATTEMPT': '1'}), \
                patch.object(ci_postmerge, 'context', return_value=current), patch.object(planner, 'normal_plan', return_value=plan):
            directory = Path(temporary)
            for lane in plan['lanes']:
                target = directory / lane['id']
                target.mkdir()
                (target / 'python-selected.log').write_text('fixture lifecycle\n')
                (target / 'python-selected-tests.jsonl').write_text(''.join(json.dumps(row) + '\n' for row in self.rows(lane['test_ids'])))
                guard = target / 'no-build'
                env = ci_no_build.prepare(guard, executables={'cargo': sys.executable, 'rustc': sys.executable}, python_guard=True)
                ci_no_build.probe(guard, env)
                receipt = {'policy_version': planner.POLICY_VERSION, 'assurance': 'selected-pr-contracts',
                    'source_sha': 'e' * 40, 'source_tree': 'd' * 40, 'plan_sha256': planner.digest(plan),
                    'lane': lane['id'], 'run_id': '123', 'run_attempt': '1', 'passed': True, 'evidence_origin': 'fresh',
                    'toolchain': base.compiler, 'build_configuration': base.qualified['configurations'][lane['id']],
                    'test_ids': lane['test_ids'], 'steps': [{'step': 'python-selected', 'code': 0,
                        'record': ci_selected.binding(target / 'python-selected.log')}],
                    'python_record': ci_selected.binding(target / 'python-selected-tests.jsonl'),
                    'no_build_records': {key: ci_selected.binding(guard / name)
                                        for key, name in [('manifest', 'manifest.json'), ('audit', 'audit.jsonl')]}}
                (target / 'receipt.json').write_text(json.dumps(receipt))
            needs = {key: {'result': value} for key, value in contract.groups().items()}
            seal = module.seal_selected(directory, event, needs)
            self.assertEqual(seal['assurance'], 'fresh-selected-pr')
            self.assertEqual(set(seal['toolchains']), {'linux-tooling', 'windows-tooling'})
            path = directory / 'linux-tooling/python-selected-tests.jsonl'
            path.write_text('green but not the bound test lifecycle')
            with self.assertRaisesRegex(ValueError, 'bytes|digest'):
                module.seal_selected(directory, event, needs)
            needs['check']['result'] = 'skipped'
            with self.assertRaisesRegex(ValueError, 'prerequisite'):
                module.seal_selected(directory, event, needs)



    def test_actual_transport_process_accepts_only_complete_bound_selected_proofs(self):
        kind = 'selected'
        module, _, base, responses, _ = self.fixture(kind)
        encoded = {key: {'binary': base64.b64encode(value).decode()} if isinstance(value, bytes) else {'json': value}
                   for key, value in responses.items()}
        with self.subTest(kind=kind), tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            data = directory / 'responses.json'; data.write_text(json.dumps(encoded))
            shim = directory / 'transport.py'
            shim.write_text('import base64,json,sys\nfrom pathlib import Path\n'
                'command=json.loads(sys.argv[1]); assert command[:4]==["gh","api","--method","GET"]\n'
                f'row=json.loads(Path({str(data)!r}).read_text())[command[4].removeprefix("/repos/owner/repo")]\n'
                'sys.stdout.buffer.write(base64.b64decode(row["binary"]) if "binary" in row else json.dumps(row["json"]).encode())\n')
            payload = {'current': base.current, 'event': base.event, 'now': base.now,
                       'plan': self.fixture()[1].plan}
            path = directory / 'input.json'; path.write_text(json.dumps(payload))
            code = '''import json,sys
from pathlib import Path
sys.path.insert(0,'scripts')
import ci_postmerge as p, ci_contract_reuse as r
data=json.loads(Path(sys.argv[1]).read_text()); original=p.bounded_capture
p.bounded_capture=lambda command,timeout: original([sys.executable,sys.argv[2],json.dumps(command)],timeout=timeout)
proof=p.fetch_candidate(data['current'],data['event'],p.Api('owner/repo'),now=data['now'],contract=r.Contract(sys.argv[3],data['plan']))
print(proof['qualification']['assurance'])'''
            result = subprocess.run([sys.executable, '-c', code, str(path), str(shim), kind], cwd=module.ROOT,
                                    capture_output=True, text=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), 'fresh-' + kind + '-pr')
            encoded['/actions/runs/123/jobs?filter=latest&per_page=100']['json']['jobs'][0]['conclusion'] = 'failure'
            data.write_text(json.dumps(encoded))
            result = subprocess.run([sys.executable, '-c', code, str(path), str(shim), kind], cwd=module.ROOT,
                                    capture_output=True, text=True, timeout=30)
            self.assertNotEqual(result.returncode, 0)



    def test_positive_selected_resolution_is_distinct_and_revalidated_against_normal_scope(self):
        module, contract, base, _, _ = self.fixture()
        normal = {**contract.plan, 'source_sha': base.current['source_sha'], 'source_tree': base.current['source_tree']}
        proof = {'qualification': base.qualified, 'main': base.current, 'qualified_at': base.run['updated_at']}
        with patch.dict(os.environ, {'GITHUB_EVENT_NAME': 'push'}), \
                patch.object(ci_postmerge, 'context', return_value=base.current), \
                patch.object(ci_postmerge, 'fetch_candidate', return_value=proof), \
                patch.object(module, 'compiler_for', return_value=base.compiler), \
                patch.object(planner, 'normal_plan', return_value=normal), patch.object(module.time, 'time', return_value=base.now):
            plan = module.resolve(normal, base.event)
            self.assertEqual(plan['assurance'], 'prior-exact-tree-selected-plus-fresh-main')
            self.assertEqual(plan['mode'], 'postmerge')
            self.assertEqual(plan['lanes'], [])
            self.assertFalse(plan['consumer_3d'])
            self.assertEqual(module.validate_selected(plan), proof)
            changed = deepcopy(plan); changed['normal_selection']['requires_apps'] = True
            with self.assertRaisesRegex(ValueError, 'normal selected'):
                module.validate_selected(changed)


    def test_selected_seal_waits_for_required_smoke_without_a_dependency_cycle(self):
        import yaml
        module = self.module()
        core = yaml.load((module.ROOT / '.github/workflows/ci.yml').read_text(), Loader=yaml.BaseLoader)
        seal = core['jobs']['selected_qualification']
        check = core['jobs']['check']
        self.assertIn('check', seal['needs'])
        self.assertIn('selected_consumer', seal['needs'])
        self.assertIn('selected_consumer', check['needs'])
        self.assertNotIn('selected_qualification', check['needs'])
        self.assertIn("needs.scope.outputs.mode == 'selected'", seal['if'])


    def test_fresh_worker_dispatch_never_requeries_proof_availability(self):
        import ci_selected
        self.module()
        normal = {'mode': 'selected'}
        with tempfile.TemporaryDirectory() as temporary, patch.object(planner, 'normal_plan', return_value=normal), \
                patch.object(planner, 'current_plan', side_effect=AssertionError('Late proof query')), \
                patch.object(ci_selected, 'run', return_value=0) as run:
            self.assertEqual(ci_selected.main(['run', '--lane', 'linux-tooling', '--output', temporary]), 0)
            self.assertEqual(run.call_args.args[0], normal)

    def test_actual_fresh_tooling_worker_finishes_one_test_under_guard_without_proof_queries(self):
        import ci_selected
        module = self.module()
        identity = 'test_ci_contract_reuse.ContractReuseTests.test_pure_tooling_main_integration_has_no_compilation_or_native_requirement'
        lane = {'id': 'windows-tooling' if sys.platform == 'win32' else 'linux-tooling',
                'os': 'windows-latest' if sys.platform == 'win32' else 'ubuntu-latest',
                'profile': 'debug', 'partition': 'tooling', 'test_ids': [identity]}
        plan = {'mode': 'selected', 'policy_version': planner.POLICY_VERSION, 'source_sha': 'a' * 40,
                'source_tree': 'b' * 40, 'lanes': [lane]}
        # Only the local source/lock authority is a fixture: the runner and
        # inherited no-build guard execute for real, with empty applications.
        with tempfile.TemporaryDirectory() as temporary, patch.object(ci_selected, 'clean_source'), \
                patch.object(ci_selected.verify, 'running_builds', return_value=[]), \
                patch.object(ci_selected.verify, 'Lock', return_value=contextlib.nullcontext()), \
                patch.object(planner, 'normal_plan', return_value=plan), \
                patch.object(planner, 'current_plan', side_effect=AssertionError('Late proof query')):
            directory = Path(temporary) / 'lane'
            self.assertEqual(ci_selected.run(plan, lane['id'], directory), 0)
            receipt = ci_selected.read_json(directory / 'receipt.json')
            ci_selected.validate_receipts(plan, [receipt])
            ci_selected.validate_files(plan, receipt, directory)
            self.assertEqual(receipt['test_ids'], [identity])
            self.assertTrue(receipt['passed'])
            self.assertEqual(receipt['build_configuration'], module.build_policy())

    def test_build_override_policy_rejects_unreviewed_compiler_target_and_profile_inputs(self):
        module = self.module()
        for key, value in [('RUSTFLAGS', '--cfg unreviewed'), ('CARGO_TARGET_FAKE_LINKER', 'unreviewed'),
                           ('CARGO_PROFILE_RELEASE_LTO', 'true')]:
            with self.subTest(key=key), patch.dict(os.environ, {key: value}), self.assertRaisesRegex(ValueError, 'unreviewed'):
                module.build_policy()



if __name__ == '__main__':
    unittest.main()
