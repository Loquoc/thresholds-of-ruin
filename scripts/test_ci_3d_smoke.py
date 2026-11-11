import json
import tempfile
import unittest
from pathlib import Path

import ci_selected
import ci_test_plan


class SmokeContract(unittest.TestCase):
    def test_3d_only_edits_do_not_require_full_core_verification(self):
        test_id = 'test_tools.Tools.test_fixture'
        scope = {'full_required': False, 'consumer_3d': True, 'contracts': ['native'],
                 'requires_apps': False, 'requires_rust': False, 'reasons': [],
                 'linux_debug': [test_id], 'windows_debug': [test_id], 'linux_release': []}
        plan = ci_test_plan.make_plan('pull_request', 'a' * 40, 'b' * 40,
                                      ['crates/client-3d/src/main.rs'], [test_id], scope)
        self.assertEqual(plan['mode'], 'selected')
        self.assertEqual([lane['id'] for lane in plan['lanes']],
                         ['linux-tooling', 'windows-tooling', 'linux-3d-consumer'])

    def test_consumer_builds_and_runs_only_the_native_smoke(self):
        lane = {'id': 'linux-3d-consumer', 'profile': 'debug', 'partition': 'consumer', 'test_ids': []}
        steps = ci_selected.steps_for(lane, Path('.local/smoke-test'), 1)
        self.assertEqual([name for name, _, _ in steps],
                         ['consumer-server', 'consumer-client', 'consumer-smoke'])
        for _, command, _ in steps[:2]:
            self.assertEqual(command[:2], ['cargo', 'build'])
            self.assertIn('--locked', command)
        self.assertEqual(steps[2][1][-1],
                         'test_client_process.Native3DProcesses.test_ci_loads_and_accepts_native_input')

    def test_hosted_workflows_have_one_3d_owner(self):
        workflows = ci_selected.ROOT / '.github/workflows'
        self.assertFalse((workflows / 'client-3d.yml').exists())
        ci = (workflows / 'ci.yml').read_text(encoding='utf-8')
        for retired in ('native_verify:', 'native_scope:', 'native_qualification:',
                        'native_integration:', 'tor-client-3d-model', 'client-3d/scripts/verify.py'):
            self.assertNotIn(retired, ci)
        self.assertEqual(ci.count('ci_selected.py run --lane linux-3d-consumer'), 1)
        self.assertEqual(ci.count('ci_selected.py smoke-3d --output'), 1)

    def test_selected_smoke_requires_bound_evidence_of_one_clean_native_test(self):
        lane = {'id': 'linux-3d-consumer', 'partition': 'consumer', 'test_ids': []}
        profile = {'profile': 'debug', 'schema_version': 1}
        test_id = ci_selected.SMOKE_ID
        rows = [
            dict(profile, kind='selection', test_ids=[test_id]),
            dict(profile, kind='test', test_id=test_id, outcome='passed', failures=0,
                 errors=0, subtests_failed=0, subtests_skipped=0),
            dict(profile, kind='suite', successful=True, tests_run=1, failures=0, errors=0,
                 skipped=0, waived=0, expected_failures=0, unexpected_successes=0),
        ]
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            path = directory / 'consumer-smoke-tests.jsonl'

            def write():
                path.write_text(''.join(json.dumps(row) + '\n' for row in rows), encoding='utf-8')

            write()
            receipt = {'lane': lane['id'], 'steps': [], 'smoke_record': ci_selected.binding(path)}
            ci_selected.validate_files({'lanes': [lane]}, receipt, directory)
            path.write_bytes(path.read_bytes() + b'\n')
            with self.assertRaises(ValueError):
                ci_selected.validate_files({'lanes': [lane]}, receipt, directory)
            rows[1]['outcome'] = 'skipped'
            rows[-1]['skipped'] = 1
            write()
            with self.assertRaisesRegex(ValueError, 'did not pass cleanly'):
                ci_selected.smoke_records(directory)
