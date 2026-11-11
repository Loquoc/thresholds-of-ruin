"""Fresh workflow execution replaces retired test-pass adoption plumbing.

Retired pass-adoption code has been removed. These integration contracts prove
the active workflow requires fresh results and preserves build-only transport.
"""
import os
from pathlib import Path
import re
import subprocess
import shlex
import sys
import tempfile
import unittest
from unittest.mock import patch

from test_verify import workflow_bash

ROOT = Path(__file__).resolve().parents[1]


def workflow_step(name):
    workflow = (ROOT / '.github/workflows/ci.yml').read_text()
    match = re.search(r'^      - name: ' + re.escape(name) + r'\n(.*?)(?=^      - |^  \w|\Z)', workflow, re.M | re.S)
    if match is None:
        raise AssertionError(f'Missing workflow step: {name}')
    return match[1]


def workflow_job(name):
    text = (ROOT / '.github/workflows/ci.yml').read_text()
    return re.search(r'^  ' + re.escape(name) + r':\n(.*?)(?=^  \w|\Z)', text, re.M | re.S)[1]


def consumer_permitted(name, *, producer='success', apps='true', mode='full', docs='false', cancelled=False,
                       event='pull_request', draft=False):
    expression = re.search(r'^    if: \$\{\{ (.+) \}\}$', workflow_job(name), re.M)[1]
    values = {'cancelled()': repr(cancelled), 'needs.scope.result': repr('success'),
              'needs.scope.outputs.usage_review': repr('success'),
              'github.event_name': repr(event), 'github.event.pull_request.draft': repr(draft), 'false': 'False',
              'needs.scope.outputs.mode': repr(mode), 'needs.scope.outputs.docs_only': repr(docs),
              'needs.scope.outputs.consumer_3d': repr('true'),
              'needs.scope.outputs.requires_apps': repr(apps),
              'needs.windows_binaries.result': repr(producer), 'needs.selected_rust.result': repr(producer),
              '&&': 'and', '||': 'or', '!': 'not', '==': '==', '!=': '!=', '(': '(', ')': ')'}
    tokens = re.findall(r"'[^']*'|[A-Za-z_][A-Za-z_0-9.]*(?:\(\))?|&&|\|\||!=|==|[!()]|\S", expression)
    translated = ' '.join(token if token.startswith("'") else values[token] for token in tokens)
    return eval(translated, {'__builtins__': {}})


class WorkflowAdoption(unittest.TestCase):
    def test_usage_review_runs_before_core_planning(self):
        job = self.workflows()['ci.yml']['jobs']['scope']
        commands = [step.get('run', '') for step in job['steps']]
        check = commands.index('python3 scripts/ci_usage_review.py')
        self.assertLess(check, next(i for i, cmd in enumerate(commands) if 'ci_test_plan.py' in cmd))
        result = subprocess.run([sys.executable, str(ROOT / 'scripts/ci_usage_review.py')], cwd=ROOT,
                                capture_output=True, text=True, timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_usage_review_detects_unreviewed_workflow_content(self):
        import ci_usage_review
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workflows = root / '.github/workflows'
            workflows.mkdir(parents=True)
            source = workflows / 'ci.yml'
            source.write_bytes(b'name: project verification\r\n')
            import hashlib
            reviewed = {'.github/workflows/ci.yml': hashlib.sha256(b'name: project verification\n').hexdigest()}
            self.assertEqual(ci_usage_review.differences(root, reviewed), [])
            source.write_bytes(b'name: changed workflow\n')
            self.assertIn('changed: .github/workflows/ci.yml', ci_usage_review.differences(root, reviewed))
            (workflows / 'extra.yaml').write_bytes(b'name: unreviewed workflow\n')
            self.assertIn('unreviewed: .github/workflows/extra.yaml', ci_usage_review.differences(root, reviewed))
            source.unlink()
            self.assertIn('missing: .github/workflows/ci.yml', ci_usage_review.differences(root, reviewed))

    def workflows(self):
        import yaml
        return {name: yaml.load((ROOT / '.github/workflows' / name).read_text(encoding='utf-8'),
                               Loader=yaml.BaseLoader) for name in ('ci.yml',)}

    def test_draft_transitions_trigger_the_single_automatic_workflow(self):
        workflows = self.workflows()
        self.assertFalse((ROOT / '.github/workflows/client-3d.yml').exists())
        for name, workflow in [('ci.yml', workflows['ci.yml'])]:
            with self.subTest(workflow=name):
                pull_request = workflow['on']['pull_request']
                self.assertIsInstance(pull_request, dict)
                self.assertIn('types', pull_request)
                self.assertEqual(set(pull_request['types']),
                                 {'opened', 'synchronize', 'reopened', 'ready_for_review', 'converted_to_draft'})

    def test_drafts_do_not_start_core_application_lanes(self):
        for name in ('profiles', 'windows_binaries', 'windows_python', 'selected_rust',
                     'selected_linux', 'selected_windows', 'selected_release', 'selected_consumer'):
            mode = 'selected' if name.startswith('selected_') else 'full'
            with self.subTest(job=name):
                self.assertFalse(consumer_permitted(name, mode=mode, draft=True))
                self.assertTrue(consumer_permitted(name, mode=mode, draft=False))
                for event in ('push', 'schedule', 'release', 'workflow_dispatch'):
                    self.assertTrue(consumer_permitted(name, mode=mode, event=event, draft=True))
                self.assertFalse(consumer_permitted(name, mode=mode, cancelled=True))

    def test_actual_admission_process_rejects_drafts_before_checkout(self):
        workflow = self.workflows()['ci.yml']
        for name in ('check',):
            step = workflow['jobs'][name]['steps'][0]
            self.assertEqual(step.get('name'), 'Require ready pull request for qualification')
            self.assertEqual(step['shell'], 'bash')
            self.assertEqual(step['env'], {'QUALIFICATION_EVENT': '${{ github.event_name }}',
                                           'QUALIFICATION_DRAFT': '${{ github.event.pull_request.draft }}'})
            command = shlex.split(step['run'])
            self.assertEqual(command[:2], ['python', '-c'])
            for event, draft, passed in (
                ('pull_request', 'false', True), ('pull_request', 'true', False),
                ('pull_request', '', False), ('pull_request', 'unknown', False),
                ('push', 'true', True), ('schedule', '', True),
                ('workflow_dispatch', 'true', True), ('release', '', True),
            ):
                with self.subTest(workflow=name, event=event, draft=draft):
                    result = subprocess.run([sys.executable, *command[1:]],
                                            env={**os.environ, 'QUALIFICATION_EVENT': event,
                                                 'QUALIFICATION_DRAFT': draft},
                                            capture_output=True, text=True, check=False)
                    self.assertEqual(result.returncode == 0, passed, result.stderr)
                    if not passed:
                        self.assertIn('Draft feedback is not merge qualification', result.stderr)

    def test_dependency_groups_preserve_individual_major_and_security_updates(self):
        import yaml
        config = yaml.load((ROOT / '.github/dependabot.yml').read_text(encoding='utf-8'), Loader=yaml.BaseLoader)
        self.assertEqual({entry['package-ecosystem'] for entry in config['updates']},
                         {'cargo', 'github-actions', 'pip'})
        for entry in config['updates']:
            with self.subTest(ecosystem=entry['package-ecosystem']):
                self.assertEqual(entry['schedule']['interval'], 'weekly')
                self.assertEqual(entry['open-pull-requests-limit'], '5')
                self.assertNotIn('ignore', entry)
                self.assertNotIn('allow', entry)
                self.assertIn('groups', entry)
                self.assertEqual(len(entry['groups']), 1)
                [group] = entry['groups'].values()
                self.assertEqual(group['applies-to'], 'version-updates')
                self.assertEqual(group['patterns'], ['*'])
                self.assertEqual(set(group['update-types']), {'minor', 'patch'})

    def test_native_logs_have_explicit_bounded_retention(self):
        workflow = self.workflows()['ci.yml']
        uploads = [step for step in workflow['jobs']['selected_consumer']['steps']
                   if step.get('uses', '').startswith('actions/upload-artifact@')]
        self.assertEqual(len(uploads), 2)
        self.assertTrue(all(step['with'].get('retention-days') == '7' for step in uploads))

    def test_active_workflow_has_no_retired_generic_pass_adoption(self):
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        for retired in ('python scripts/ci_adoption.py', 'python scripts/ci_head_reuse.py',
                        'python scripts/ci_qualification.py', '--cross-head-proof', 'steps.adoption'):
            self.assertNotIn(retired, workflow)
        self.assertIn('python scripts/verify.py full --ci-profile', workflow)
        self.assertIn('python scripts/ci_selected.py aggregate', workflow)

    def test_full_windows_retention_has_unique_success_only_role_outputs(self):
        cases = [('windows_binaries', 'Record successful producer attempt', 'producer',
                  [('debug', None), ('release', None)]),
                 ('windows_python', 'Record successful shard attempt', 'shard',
                  [(profile, index) for profile in ('debug', 'release') for index in (1, 2)])]
        for job_name, step_name, prefix, roles in cases:
            job = workflow_job(job_name)
            for profile, index in roles:
                key = f'{prefix}_{profile}' + (f'_{index}' if index else '')
                self.assertIn(f'{key}: ${{{{ steps.execution_attempt.outputs.{key} }}}}', job)
            step = workflow_step(step_name)
            self.assertIn('if: success() && !cancelled()', step)
            self.assertIn('id: execution_attempt', step)
            self.assertGreater(job.index(step_name), job.index('uses: actions/upload-artifact@'))
            command = re.search(r'^        run: (.+)$', step, re.M)[1]
            for profile, index in roles:
                rendered = command.replace('${{ matrix.profile }}', profile).replace('${{ matrix.shard }}', str(index))
                with tempfile.TemporaryDirectory() as directory:
                    output = Path(directory) / 'output'
                    result = subprocess.run([workflow_bash(), '-e', '-c', rendered],
                        env={**os.environ, 'GITHUB_OUTPUT': output.as_posix(), 'GITHUB_RUN_ATTEMPT': '7'},
                        capture_output=True, text=True, timeout=10)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    key = f'{prefix}_{profile}' + (f'_{index}' if index else '')
                    self.assertEqual(output.read_text(), f'{key}=7\n')
        gate = workflow_job('check')
        self.assertIn('id: windows_cohort', gate)
        self.assertIn('shell: bash', workflow_step('Select successful full Windows execution cohort'))
        self.assertIn('WINDOWS_BINARIES: ${{ toJSON(needs.windows_binaries) }}', gate)
        self.assertIn('WINDOWS_PYTHON: ${{ toJSON(needs.windows_python) }}', gate)
        self.assertLess(gate.index('Select successful full Windows execution cohort'),
                        gate.index('Require both complete Windows debug Python shards'))
        for profile in ('debug', 'release'):
            step = workflow_step(f'Require both complete Windows {profile} Python shards')
            self.assertIn('--origin-attempt "${{ steps.windows_cohort.outputs.origin_attempt }}"', step)
            self.assertNotIn('github.run_attempt', step)

    def test_cache_writes_stay_successful_and_same_repository_ref_scoped(self):
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        expressions = re.findall(r'^          save-if: \$\{\{ (.+) \}\}$', workflow, re.M)
        caches = re.findall(r'uses: Swatinem/rust-cache@', workflow)
        self.assertEqual(len(expressions), len(caches))
        self.assertEqual(workflow.count('          cache-on-failure: false'), len(caches))
        self.assertGreater(len(caches), 0)
        for expression in expressions:
            main_only = expression == "job.status == 'success' && github.event_name != 'pull_request_target' && github.ref == 'refs/heads/main'"
            for passed in (False, True):
                for event in ('push', 'pull_request', 'pull_request_target', 'workflow_dispatch', 'schedule'):
                    for ref in ('refs/heads/main', 'refs/heads/topic', 'refs/pull/188/merge'):
                        for head in ('owner/repo', 'fork/repo', ''):
                            values = {'job.status': repr('success' if passed else 'failure'), 'github.ref': repr(ref),
                                      'github.event_name': repr(event), 'github.repository': repr('owner/repo'),
                                      'github.event.pull_request.head.repo.full_name': repr(head),
                                      '&&': 'and', '||': 'or', '==': '==', '!=': '!=', '(': '(', ')': ')'}
                            tokens = re.findall(r"'[^']*'|[A-Za-z_.]+(?:\(\))?|&&|\|\||!=|==|[()]|\S", expression)
                            program = ' '.join(token if token.startswith("'") else values[token] for token in tokens)
                            actual = eval(program, {'__builtins__': {}})
                            expected = passed and event != 'pull_request_target' and (ref == 'refs/heads/main' or not main_only and event == 'pull_request' and head == 'owner/repo')
                            with self.subTest(event=event, ref=ref, head=head, passed=passed):
                                self.assertEqual(actual, expected)
        # No cross-PR cache namespace override or untrusted-trigger expansion.
        self.assertNotIn('pull_request_target:', workflow)
        self.assertNotIn('cache-on-failure: true', workflow)

    def test_full_windows_consumers_never_start_without_successful_producer(self):
        for producer in ('success', 'failure', 'cancelled', 'skipped', '', 'unknown'):
            for cancelled in (False, True):
                for docs in ('false', 'true'):
                    with self.subTest(producer=producer, cancelled=cancelled, docs=docs):
                        self.assertEqual(consumer_permitted('windows_python', producer=producer,
                                                            cancelled=cancelled, docs=docs),
                                         producer == 'success' and not cancelled and docs == 'false')
        self.assertFalse(consumer_permitted('windows_python', mode='selected'))
        # Independent Rust/Linux partitions must not acquire a producer barrier.
        self.assertNotIn('windows_binaries', workflow_job('profiles'))
        # Required full qualification still rejects failed producer/skipped shards.
        gate = workflow_job('check')
        self.assertIn("windows == ['success', 'success']", gate)
        self.assertIn("WINDOWS_BINARIES_RESULT: ${{ needs.windows_binaries.result }}", gate)
        self.assertIn("WINDOWS_PYTHON_RESULT: ${{ needs.windows_python.result }}", gate)

    def test_native_artifact_contracts_run_before_any_producer_compile(self):
        producer = workflow_job('windows_binaries')
        self.assertIn('run: python -m unittest discover -s scripts -p test_ci_binary_artifact.py', producer)
        self.assertLess(producer.index('Check artifact transport contracts before compiling'),
                        producer.index('Build exact-run ordinary applications'))
        self.assertNotIn('continue-on-error', producer)

    def test_producer_smokes_fresh_transport_before_uploading_pristine_bundle(self):
        producer = workflow_job('windows_binaries')
        stage = workflow_step('Validate fresh producer transport and application runtime')
        self.assertLess(producer.index('Build exact-run ordinary applications'),
                        producer.index('Validate fresh producer transport and application runtime'))
        self.assertLess(producer.index('Validate fresh producer transport and application runtime'),
                        producer.index('Publish binaries, not test evidence'))
        self.assertIn('shell: bash', stage)
        self.assertIn('python scripts/ci_binary_artifact.py validate .local/producer-validation '
                      '--profile ${{ matrix.profile }} --source-sha "$GITHUB_SHA" --smoke', stage)
        self.assertNotIn('cargo ', stage)
        self.assertNotIn('continue-on-error', stage)
        upload = workflow_step('Publish binaries, not test evidence')
        self.assertIn('.local/application-artifact/manifest.json', upload)
        self.assertIn('.local/application-artifact/executables.tar', upload)
        self.assertNotIn('producer-validation', upload)
        # Execute the actual copy program: only immutable transport files move,
        # archive modes simulate artifact download, and an old destination fails.
        match = re.search(r"python - <<'PY'\n(.*?)\n          PY", stage, re.S)
        self.assertIsNotNone(match)
        program = '\n'.join(line[10:] for line in match[1].splitlines())
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original = root / '.local/application-artifact'
            original.mkdir(parents=True)
            for name in ('manifest.json', 'executables.tar'):
                path = original / name
                path.write_bytes(('immutable:' + name).encode())
                path.chmod(0o755)
            (original / 'not-transport.txt').write_text('must not be copied')
            before = {path.name: (path.read_bytes(), path.stat().st_mode) for path in original.iterdir()}
            subprocess.run([sys.executable, '-c', program], cwd=root, check=True, timeout=10)
            copied = root / '.local/producer-validation'
            self.assertEqual({path.name for path in copied.iterdir()}, {'manifest.json', 'executables.tar'})
            for path in copied.iterdir():
                self.assertEqual(path.read_bytes(), before[path.name][0])
                if os.name != 'nt':
                    self.assertEqual(path.stat().st_mode & 0o777, 0o644)
            self.assertEqual({path.name: (path.read_bytes(), path.stat().st_mode) for path in original.iterdir()}, before)
            rejected = subprocess.run([sys.executable, '-c', program], cwd=root,
                                      capture_output=True, timeout=10)
            self.assertNotEqual(rejected.returncode, 0)

    def test_selected_consumer_keeps_pure_tooling_independent_of_app_producer(self):
        for producer in ('success', 'failure', 'cancelled', 'skipped', '', 'unknown'):
            with self.subTest(producer=producer):
                self.assertEqual(consumer_permitted('selected_linux', producer=producer, mode='selected'),
                                 producer == 'success')
                self.assertTrue(consumer_permitted('selected_linux', producer=producer, mode='selected', apps='false'))
                self.assertFalse(consumer_permitted('selected_linux', producer=producer, mode='selected',
                                                    apps='false', cancelled=True))
        self.assertFalse(consumer_permitted('selected_linux', mode='full', apps='false'))

    def test_complete_partition_is_fresh_and_uses_explicit_shell(self):
        block = workflow_step('Verify complete profile partition')
        self.assertIn('shell: bash', block)
        self.assertIn('if: success() && !cancelled()', block)
        self.assertNotIn('cache-hit', block)
        self.assertNotIn('continue-on-error', block)

    def test_partition_and_shard_gates_reject_failure_cancellation_and_skip(self):
        for name in ('Require fresh complete partition execution', 'Require fresh complete shard execution'):
            block = workflow_step(name)
            self.assertIn('if: always()', block)
            self.assertIn('shell: bash', block)
            program = re.search(r'^        run: (.+)$', block, re.M)[1]
            for outcome in ('success', 'failure', 'cancelled', 'skipped', '', 'unknown'):
                result = subprocess.run([workflow_bash(), '--noprofile', '--norc', '-c', program],
                    env={**os.environ, 'VERIFICATION_OUTCOME': outcome}, capture_output=True, timeout=5)
                self.assertEqual(result.returncode == 0, outcome == 'success', (name, outcome))

    def test_full_producer_creates_parent_on_clean_checkout_without_replacing_bundle(self):
        import ci_binary_artifact as artifact
        block = workflow_step('Prepare ordinary application artifact parent')
        command = shlex.split(re.search(r'^        run: (.+)$', block, re.M)[1])
        self.assertEqual(command[:2], ['python', '-c'])
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        self.assertLess(workflow.index('Prepare ordinary application artifact parent'),
                        workflow.index('Build exact-run ordinary applications'))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            executables = {}
            for name in artifact.BINARIES:
                path = root / (name + '.exe')
                path.write_bytes(b'fixture executable')
                executables[name] = path
            output = root / '.local/application-artifact'
            with self.assertRaises(OSError):
                artifact.package(executables, {'platform': 'Windows'}, output)
            subprocess.run([sys.executable, *command[1:]], cwd=root, check=True, timeout=10)
            self.assertTrue(output.parent.is_dir())
            self.assertFalse(output.exists())
            artifact.package(executables, {'platform': 'Windows'}, output)
            self.assertTrue((output / 'manifest.json').is_file())
            with self.assertRaises(FileExistsError):
                artifact.package(executables, {'platform': 'Windows'}, output)

    def test_no_hosted_workflow_runs_on_a_timer(self):
        import yaml
        workflows = {path.name: yaml.load(path.read_text(encoding='utf-8'), Loader=yaml.BaseLoader)
                     for path in (ROOT / '.github/workflows').glob('*.yml')}
        core = workflows['ci.yml']
        self.assertNotIn('schedule', core['on'], 'The migration must not schedule the exhaustive matrix')
        self.assertEqual(set(core['on']), {'push', 'pull_request', 'workflow_dispatch', 'release'})
        self.assertEqual(core['on']['push']['branches'], ['main'])
        self.assertEqual(core['on']['release']['types'], ['published'])
        scheduled = {name: workflow['on']['schedule'] for name, workflow in workflows.items()
                     if 'schedule' in workflow['on']}
        self.assertEqual(scheduled, {})

    def advisory_workflow(self):
        import yaml
        path = ROOT / '.github/workflows/dependency-advisories.yml'
        self.assertTrue(path.is_file(), 'Keep the explicit advisory-only scan available')
        return yaml.load(path.read_text(encoding='utf-8'), Loader=yaml.BaseLoader)

    def test_advisory_scan_is_one_bounded_blocking_job_with_no_test_qualification(self):
        workflow = self.advisory_workflow()
        self.assertEqual(set(workflow['on']), {'workflow_dispatch'})
        self.assertEqual(workflow['permissions'], {'contents': 'read'})
        self.assertEqual(set(workflow['jobs']), {'advisories'})
        job = workflow['jobs']['advisories']
        self.assertEqual(job['runs-on'], 'ubuntu-latest')
        self.assertEqual(job['timeout-minutes'], '10')
        self.assertNotIn('continue-on-error', job)
        self.assertNotIn('strategy', job)
        self.assertNotIn('permissions', job)
        steps = job['steps']
        self.assertEqual(len(steps), 4)
        self.assertEqual(steps[0]['name'], 'State advisory-only scope')
        self.assertEqual(steps[1]['uses'], 'actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1')
        self.assertEqual(steps[1]['with'], {'ref': '${{ github.sha }}', 'persist-credentials': 'false'})
        self.assertEqual(steps[2]['run'], 'python3 scripts/ci_usage_review.py')
        self.assertEqual(steps[3]['uses'], 'EmbarkStudios/cargo-deny-action@3c6349835b2b7b196a839186cb8b78e02f7b5f25')
        self.assertEqual(steps[3]['with'], {'command': 'check advisories'})
        for step in steps:
            self.assertNotIn('continue-on-error', step)
            self.assertNotIn('if', step)
        self.assertEqual(workflow['concurrency']['cancel-in-progress'], 'true')
        self.assertIn('${{ github.ref }}', workflow['concurrency']['group'])

    def test_actual_advisory_scope_step_accepts_only_scan_events_and_never_qualifies_tests(self):
        step = self.advisory_workflow()['jobs']['advisories']['steps'][0]
        self.assertEqual(step['env'], {'ADVISORY_EVENT': '${{ github.event_name }}'})
        self.assertEqual(step['shell'], 'bash')
        for event in ('schedule', 'workflow_dispatch', 'push', 'pull_request', 'release', '', 'unknown'):
            with self.subTest(event=event), tempfile.TemporaryDirectory() as directory:
                summary = Path(directory) / 'summary'
                result = subprocess.run([workflow_bash(), '-e', '-c', step['run']],
                    env={**os.environ, 'ADVISORY_EVENT': event, 'GITHUB_STEP_SUMMARY': str(summary)},
                    capture_output=True, text=True, timeout=10)
                if event == 'workflow_dispatch':
                    self.assertEqual(result.returncode, 0, result.stderr)
                    text = summary.read_text(encoding='utf-8')
                    self.assertIn('Dependency advisories only', text)
                    self.assertIn('does not qualify tests or a merge', text)
                    self.assertIn('No application builds or test suites', text)
                else:
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn('Unsupported advisory event', result.stderr)
                    self.assertFalse(summary.exists())

    def test_advisory_success_cannot_supply_full_qualification(self):
        import ci_postmerge
        advisory = self.advisory_workflow()['jobs']['advisories']
        row = {'name': advisory['name'], 'run_id': 123, 'head_sha': 'a' * 40,
               'run_attempt': 1, 'status': 'completed', 'conclusion': 'success'}
        with self.assertRaisesRegex(ValueError, 'canonical full job roles'):
            ci_postmerge.validate_jobs([row], {'id': 123, 'head_sha': 'a' * 40, 'run_attempt': 1})
        for event in ('schedule', 'workflow_dispatch'):
            with self.subTest(event=event), patch.dict(os.environ, {'GITHUB_EVENT_NAME': event}), \
                    self.assertRaisesRegex(ValueError, 'only full PR coverage'):
                ci_postmerge.seal({}, {}, {})

    def test_artifact_validation_is_not_test_result_reuse(self):
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        self.assertIn('--ci-binaries .local/application-artifact', workflow)
        self.assertIn('--ci-python-shard ${{ matrix.shard }}', workflow)
        self.assertIn('Verify complete profile partition', workflow)
        self.assertNotIn('adopted=true', workflow)




class LinuxDebugShardWorkflow(unittest.TestCase):
    def test_isolated_debug_owners_keep_cap_and_exact_union_gate(self):
        profiles = workflow_job('profiles')
        self.assertIn("matrix.partition == 'python' && 40 || 60 }}", profiles)
        for index in (1, 2):
            self.assertIn(f'partition: python\n            shard: {index}', profiles)
            self.assertIn(f'linux_debug_shard_{index}:', profiles)
        self.assertIn("format('--ci-python-shard {0}', matrix.shard)", profiles)
        gate = workflow_step('Require both complete Linux debug Python shards')
        self.assertIn("matrix.os == 'ubuntu-latest'", gate)
        self.assertIn("needs.scope.outputs.mode == 'full'", gate)
        self.assertIn("needs.scope.outputs.docs_only == 'false'", gate)
        for index in (1, 2):
            self.assertIn(f'verification-ubuntu-latest-debug-python-shard-{index}-attempt-', gate)
        self.assertIn('--profile debug --directory .local/shard-coverage/linux-debug', gate)
        self.assertIn('--origin-attempt "${{ steps.linux_cohort.outputs.origin_attempt }}"', gate)


if __name__ == '__main__':
    unittest.main()
