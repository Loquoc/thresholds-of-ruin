"""Run eight assertion-checked retention workloads in 24 isolated release processes.

Head-only diagnostic, not a before/after speedup or full CI qualification. Logs,
source/binary/toolchain/machine provenance and failed receipts remain in output.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import time

import feedback
import perf_ledger

ROOT = Path(__file__).resolve().parents[1]
CASES = [(history, owners) for owners in (1, 8) for history in (0, 100, 1000, 10000)]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate(sample, history, owners):
    if (sample.get('version'), sample.get('profile'), sample.get('history'), sample.get('attachments')) != (2, 'release', history, owners):
        raise ValueError('wrong sample identity/profile')
    expected_records = history + 32 * owners + 128 + 1
    recovery = sample['recovery']
    if recovery['records_loaded'] != expected_records or recovery['records_replayed'] != 0:
        raise ValueError('wrong recovery coverage')
    stages = sample['stages']
    if [row['phase'] for row in stages] != ['seeded', 'rewind_expired', 'capabilities_released']:
        raise ValueError('missing or duplicate stage')
    roots = [min(history + 1, 128), 128 + 32 * owners, 128]
    handles = [0, 32 * owners, 0]
    records = [history, history + 32 * owners + 128, history + 32 * owners + 128]
    for row, root_count, handle_count, record_count in zip(stages, roots, handles, records):
        if (row['unique_boundary_roots'], row['presentation_handles'], row['records']) != (root_count, handle_count, record_count):
            raise ValueError('wrong ownership/record counts')
        if row['snapshot_batch_root_limit'] != 257 or row['snapshot_batches'] != (root_count + 256) // 257:
            raise ValueError('wrong checkpoint batching')
        for key in ('rss_bytes', 'snapshot_batch_json_bytes', 'boundary_inline_bytes'):
            if type(row[key]) is not int or row[key] <= 0:
                raise ValueError(f'invalid {key}')
    for key in ('rss_before_engine', 'checkpoint_json_bytes', 'saved_bytes'):
        if type(sample[key]) is not int or sample[key] <= 0:
            raise ValueError(f'invalid {key}')
    duration = sample['restart_ms']
    if type(duration) not in (int, float) or not math.isfinite(duration) or duration < 0:
        raise ValueError('invalid restart duration')
    return sample


def decode(text, history, owners):
    rows = [line.removeprefix('retention-sample ') for line in text.splitlines() if line.startswith('retention-sample ')]
    if len(rows) != 1:
        raise ValueError('expected exactly one sample')
    return validate(json.loads(rows[0]), history, owners)


def distribution(values):
    ordered = sorted(values)
    return {'n': len(values), 'p50': perf_ledger.nearest_rank(ordered, 50),
            'p95': perf_ledger.nearest_rank(ordered, 95), 'max': ordered[-1], 'min': ordered[0]}


def summarize(rows):
    if len(rows) != 24 or {(r['round'], r['history'], r['attachments']) for r in rows} != {(n, h, c) for n in range(3) for h, c in CASES}:
        raise ValueError('incomplete or duplicate matrix')
    result = []
    for history, owners in CASES:
        cases = [r for r in rows if (r['history'], r['attachments']) == (history, owners)]
        item = {'history': history, 'attachments': owners}
        for key in ('restart_ms', 'rss_before_engine', 'checkpoint_json_bytes', 'saved_bytes'):
            item[key] = distribution([r[key] for r in cases])
        item['stages'] = {}
        for index, phase in enumerate(('seeded', 'rewind_expired', 'capabilities_released')):
            item['stages'][phase] = {key: distribution([r['stages'][index][key] for r in cases])
                                     for key in cases[0]['stages'][index] if key != 'phase'}
        result.append(item)
    return result


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('--output', type=Path, required=True)
    args = cli.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    if any(key.startswith('CARGO_PROFILE_RELEASE_') or key in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS') for key in os.environ):
        raise ValueError('release overrides are not accepted for this normal-release diagnostic')
    env = {**os.environ, 'CARGO_BUILD_JOBS': '1', 'CARGO_INCREMENTAL': '0'}
    target = ROOT / '.local/retention-target'
    env['CARGO_TARGET_DIR'] = str(target)
    before = feedback.snapshot(ROOT)
    metadata = {'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                'source_tree': subprocess.check_output(['git', 'rev-parse', 'HEAD^{tree}'], cwd=ROOT, text=True).strip(),
                'status': subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True),
                'rustc': subprocess.check_output(['rustc', '-Vv'], text=True),
                'cargo': subprocess.check_output(['cargo', '-V'], text=True),
                'machine': perf_ledger.probe_machine(Path(env.get('TMPDIR', '/tmp'))),
                'build': {'profile': 'release', 'jobs': 1, 'incremental': False, 'release_overrides': False},
                'workflow': {key: os.environ.get(key) for key in ('GITHUB_SHA', 'GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_WORKFLOW_REF', 'RETENTION_PR_HEAD')}}
    (out / 'inputs-before.json').write_text(json.dumps(before, indent=2) + '\n')
    (out / 'metadata.json').write_text(json.dumps(metadata, indent=2) + '\n')

    def execute(name, command):
        started = time.monotonic()
        log = out / f'{name}.log'
        timed_out = False
        with log.open('w') as stream:
            try:
                completed = subprocess.run(command, cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT, timeout=3600)
                code = completed.returncode
            except subprocess.TimeoutExpired:
                timed_out, code = True, None
        receipt = {'command': command, 'exit_code': code, 'timed_out': timed_out, 'elapsed_seconds': time.monotonic() - started,
                   'log': log.name, 'log_sha256': digest(log)}
        (out / f'{name}.receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
        if code != 0:
            raise RuntimeError(f'{name} failed; see {log}')
        return log, receipt

    common = ['-p', 'tor-server', '--example', 'retention_bench', '--features', 'retention-profile', '--release', '--locked']
    try:
        execute('preflight', ['cargo', 'check', *common])
        execute('build', ['cargo', 'build', *common])
        log, _ = execute('arguments', ['cargo', 'test', *common])
        if '1 passed; 0 failed' not in log.read_text():
            raise ValueError('argument validation test did not run')
        binary = target / 'release/examples/retention_bench'
        metadata['binary'] = {'path': str(binary), 'sha256': digest(binary)}
        (out / 'metadata.json').write_text(json.dumps(metadata, indent=2) + '\n')
        rows = []
        for round_index in range(3):
            for history, owners in (CASES if round_index != 1 else list(reversed(CASES))):
                name = f'round-{round_index}-h{history}-c{owners}'
                log, receipt = execute(name, [str(binary), str(history), str(owners)])
                sample = decode(log.read_text(), history, owners)
                sample.update({'round': round_index, 'receipt': receipt})
                rows.append(sample)
                with (out / 'samples.jsonl').open('a') as stream:
                    stream.write(json.dumps(sample, sort_keys=True) + '\n')
                print(name, 'passed', flush=True)
        if digest(binary) != metadata['binary']['sha256']:
            raise ValueError('measurement binary changed')
        (out / 'summary.json').write_text(json.dumps(summarize(rows), indent=2) + '\n')
    except Exception as error:
        (out / 'completion.json').write_text(json.dumps({'success': False, 'error': str(error)}) + '\n')
        raise
    else:
        (out / 'completion.json').write_text(json.dumps({'success': True, 'samples': 24}) + '\n')
    finally:
        after = feedback.snapshot(ROOT)
        (out / 'inputs-after.json').write_text(json.dumps(after, indent=2) + '\n')
        (out / 'source-integrity.json').write_text(json.dumps({'unchanged': before == after}) + '\n')
        if before != after:
            raise ValueError('source inputs changed')


if __name__ == '__main__':
    main()
