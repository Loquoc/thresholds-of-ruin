"""Exact-tree selected PR seals plus fresh main integration."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import time
import zipfile

import ci_postmerge as prior
import ci_selected
import ci_test_plan as planner

ROOT = planner.ROOT
MATCH = ('repository', 'source_tree', 'parents', 'workflow_blob', 'policy_version', 'inventory_sha256', 'smoke_ids')
SELECTION = ('policy_version', 'inventory_sha256', 'requires_apps', 'requires_rust', 'consumer_3d', 'contracts', 'lanes')
require = prior.require


def build_policy():
    import ci_binary_artifact as artifacts
    require(not any(os.environ.get(key) for key in artifacts.FORBIDDEN_ENV), 'unreviewed compiler override')
    require(not any((key.startswith('CARGO_TARGET_') and key.endswith(('_RUSTFLAGS', '_LINKER', '_RUNNER')))
                    or (key.startswith('CARGO_BUILD_') and key != 'CARGO_BUILD_JOBS')
                    for key, value in os.environ.items() if value), 'unreviewed target override')
    require(not any(key.startswith('CARGO_PROFILE_') and key not in artifacts.BUILD_ENV for key in os.environ),
            'unreviewed profile override')
    result = {'cargo_config': artifacts.cargo_configuration(),
              'profile': {key: os.environ.get(key) for key in artifacts.BUILD_ENV}}
    validate_configuration(result)
    return result


def validate_configuration(value):
    import ci_binary_artifact as artifacts
    require(isinstance(value, dict) and set(value) == {'cargo_config', 'profile'}
            and isinstance(value['profile'], dict)
            and set(value['profile']) == set(artifacts.BUILD_ENV)
            and all(item in (None, '0') for item in value['profile'].values())
            and isinstance(value['cargo_config'], list), 'unreviewed build configuration')


def selection(plan):
    require(plan.get('mode') == 'selected', 'only fresh selected plans qualify')
    return {key: plan[key] for key in SELECTION}


def job_name(key, workflow=prior.WORKFLOW):
    text = (ROOT / workflow).read_text(encoding='utf-8')
    match = re.search(r'^  ' + re.escape(key) + r':\n(.*?)(?=^  \w|\Z)', text, re.M | re.S)
    require(match is not None, 'missing canonical job')
    return re.search(r'^    name: (.+)$', match[1], re.M)[1]


class Contract:
    def __init__(self, kind, plan=None):
        require(kind == 'selected', 'unknown proof kind')
        self.kind, self.plan = kind, plan
        self.workflow = prior.WORKFLOW
        self.artifact = kind + '-pr-qualification'
        self.assurance = 'fresh-' + kind + '-pr'

    def groups(self):
        apps, consumer = self.plan['requires_apps'], self.plan['consumer_3d']
        return {**dict.fromkeys(('scope', 'tooling', 'dependencies', 'check', 'selected_linux', 'selected_windows'), 'success'),
                'selected_rust': 'success' if apps else 'skipped',
                'selected_release': 'success' if apps else 'skipped',
                'selected_consumer': 'success' if consumer else 'skipped'}

    def validate(self, qualified, current):
        require(type(qualified.get('version')) is int and qualified['version'] == prior.VERSION
                and isinstance(qualified.get('source_sha'), str) and planner.SHA.fullmatch(qualified['source_sha'])
                and re.fullmatch(r'[1-9][0-9]{0,18}', qualified.get('run_id', ''))
                and re.fullmatch(r'[1-9][0-9]{0,8}', qualified.get('attempt', ''))
                and type(qualified.get('pr_number')) is int and qualified['pr_number'] > 0,
                'invalid qualification origin/schema')
        require(qualified.get('assurance') == self.assurance and qualified.get('groups') == self.groups(),
                'incorrect qualification kind/groups')
        require(all(qualified.get(key) == current[key] for key in MATCH), 'qualification inputs differ')
        expected = selection(self.plan)
        require(qualified.get('selection') == expected, 'selected contracts changed')
        roles = {row['id'] for row in self.plan['lanes']}
        fingerprints = qualified.get('toolchains')
        require(isinstance(fingerprints, dict) and set(fingerprints) == roles, 'missing tested compiler role')
        normalized = [prior.toolchain(value) for value in fingerprints.values()]
        require(normalized and len({planner.digest(value) for value in normalized}) == 1, 'tested compilers disagree')
        require(isinstance(qualified.get('configurations'), dict)
                and set(qualified['configurations']) == roles, 'missing tested build configuration')
        for value in qualified['configurations'].values():
            validate_configuration(value)
    def job_roles(self, qualified):
        success = {'Change scope and documentation', 'Repository tooling lint', 'Dependencies (bans licenses sources)',
                   'Dependencies (advisories)', 'Rust (windows-latest)', 'Rust (ubuntu-latest)', 'Selected PR qualification'}
        success |= {'3D compatibility (linux-3d-consumer)' if row['id'] == 'linux-3d-consumer'
                    else 'Selected (' + row['id'] + ')' for row in self.plan['lanes']}
        skipped = {job_name(key) for key in ('profiles', 'windows_binaries', 'windows_python', 'full_qualification', 'postmerge')}
        skipped |= {job_name(key) for key, status in self.groups().items() if status == 'skipped'}
        return success, skipped

    def validate_jobs(self, jobs, run, qualified):
        require(all(isinstance(row, dict) for row in jobs), 'malformed contract job metadata')
        success, skipped = self.job_roles(qualified)
        names = [row.get('name') for row in jobs]
        require(len(names) == len(set(names)) and set(names) == success | skipped, 'canonical contract job roles')
        for row in jobs:
            require(row.get('run_id') == run['id'] and row.get('head_sha') == run['head_sha']
                    and type(row.get('run_attempt')) is int and row['run_attempt'] == run['run_attempt']
                    and row.get('status') == 'completed', 'partial or foreign execution cohort')
            allowed = {'skipped'} if row['name'] in skipped else {'success'}
            if row['name'] == 'Dependencies (advisories)':
                allowed |= {'failure'}  # Main enforcement is independently blocking.
            require(row.get('conclusion') in allowed, 'required contract job failed')


def compiler_for(proof):
    configuration = build_policy()
    require(all(value['cargo_config'] == configuration['cargo_config']
                for value in proof['qualification']['configurations'].values()), 'Cargo configuration changed')
    qualified = next(iter(proof['qualification']['toolchains'].values()))
    release = prior.toolchain(qualified)['release']
    subprocess.run(['rustup', 'toolchain', 'install', release, '--profile', 'minimal'], check=True,
                   timeout=120, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    actual = prior.toolchain(environment={**os.environ, 'RUSTUP_TOOLCHAIN': release})
    require(actual == qualified, 'current compiler differs from qualified compiler')
    return actual


def resolve(normal, event, root=ROOT):
    try:
        require(normal.get('mode') == 'selected' and os.environ.get('GITHUB_EVENT_NAME') == 'push', 'not eligible')
        current = prior.context(root)
        proof = prior.fetch_candidate(current, event, prior.Api(current['repository']), contract=Contract('selected', normal))
        proof['current_toolchain'] = compiler_for(proof)
        return {**normal, 'mode': 'postmerge', 'assurance': 'prior-exact-tree-selected-plus-fresh-main',
                'lanes': [], 'consumer_3d': False, 'postmerge_proof': proof,
                'normal_selection': normal, 'reasons': ['exact tree has complete selected PR coverage']}
    except (OSError, ValueError, TypeError, KeyError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        return {**normal, 'postmerge_optimization': {'status': 'declined', 'reason': str(error)[:200]}}


def validate_selected(plan, root=ROOT):
    require(plan.get('mode') == 'postmerge' and os.environ.get('GITHUB_EVENT_NAME') == 'push'
            and plan.get('assurance') == 'prior-exact-tree-selected-plus-fresh-main', 'not a retained main run')
    normal = planner.normal_plan(root=root)
    require(normal == plan.get('normal_selection') and plan['lanes'] == [] and plan['consumer_3d'] is False,
            'normal selected requirement differs')
    proof = plan['postmerge_proof']
    require(len(proof['main']['parents']) == 2, 'not an ordinary merge')
    require(proof['main'] == prior.context(root) and plan['source_sha'] == proof['main']['source_sha']
            and plan['source_tree'] == proof['main']['source_tree'], 'current source/run differs')
    require(0 <= time.time() - prior.timestamp(proof['qualified_at']) <= prior.MAX_AGE, 'expired qualification')
    Contract('selected', normal).validate(proof['qualification'], proof['main'])
    require(prior.toolchain(proof['current_toolchain']) == next(iter(proof['qualification']['toolchains'].values())),
            'resolved compiler differs')
    return proof


def pr_context(event):
    require(os.environ.get('GITHUB_EVENT_NAME') == 'pull_request', 'only fresh PR execution can seal')
    current = prior.context()
    pr = event['pull_request']
    require(pr.get('draft') is False and pr['base']['ref'] == 'main'
            and pr['head']['repo']['full_name'] == pr['base']['repo']['full_name'] == current['repository']
            and current['parents'] == [pr['base']['sha'], pr['head']['sha']], 'foreign/draft PR or changed integration base')
    return current


def seal_selected(directory, event, needs):
    current = pr_context(event)
    plan = planner.normal_plan()
    contract = Contract('selected', plan)
    require(set(needs) == set(contract.groups()) and {key: row['result'] for key, row in needs.items()} == contract.groups(),
            'selected prerequisite groups incomplete')
    paths = sorted(directory.glob('*/receipt.json'))
    rows = [ci_selected.read_json(path) for path in paths]
    ci_selected.validate_receipts(plan, rows)
    for row, path in zip(rows, paths):
        ci_selected.validate_files(plan, row, path.parent)
    result = {'version': prior.VERSION, **current, 'pr_number': event['number'], 'assurance': contract.assurance,
              'selection': selection(plan), 'groups': contract.groups(),
              'toolchains': {row['lane']: prior.toolchain(row['toolchain']) for row in rows},
              'configurations': {row['lane']: row['build_configuration'] for row in rows}}
    contract.validate(result, current)
    return result


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    require(not path.exists(), 'refusing to replace proof evidence')
    path.write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    selected = sub.add_parser('seal-selected')
    selected.add_argument('--directory', type=Path, required=True)
    selected.add_argument('--output', type=Path, required=True)
    args = parser.parse_args(argv)
    if args.command == 'seal-selected':
        result = seal_selected(args.directory, prior.decode(Path(os.environ['GITHUB_EVENT_PATH']).read_bytes()),
                               prior.decode(os.environ['QUALIFICATION_NEEDS']))
        write(args.output, result)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
