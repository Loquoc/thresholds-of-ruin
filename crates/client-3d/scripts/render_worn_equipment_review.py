"""Render the bounded worn-equipment pack and retain exact provenance.

Run with an already-built native client. Never builds, blesses images, or treats
successful rendering as human visual acceptance.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
from render_pixels import decode_png

ROOT = Path(__file__).resolve().parents[3]


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def validate_manifest(script, manifest, output):
    if manifest.get('status') != 'complete' or manifest.get('capture_mode') != 'offscreen':
        raise ValueError('capture did not complete in offscreen mode')
    expected = [(s['name'], s['tick']) for s in script['captures']]
    actual = [(s['name'], s['tick']) for s in manifest['captures']]
    if actual != expected:
        raise ValueError('captured names/ticks do not match requested review')
    for shot in manifest['captures']:
        if not shot.get('renderer', {}).get('name'):
            raise ValueError('missing actual renderer identity')
        png = output / f'{shot["name"]}.png'
        if not png.is_file():
            raise ValueError(f'missing image {png.name}')
        width, height, _, _ = decode_png(png.read_bytes())
        if [width, height] != script['viewport']:
            raise ValueError(f'wrong image dimensions {png.name}')


def capture_batches(script):
    """Bound readback work per process without dropping poses or changing actors."""
    batches = []
    for i in range(0, len(script['captures']), 5):
        captures = script['captures'][i:i + 5]
        end = max(c['tick'] for c in captures)
        # The native schema rejects future events after the final capture.
        # Keep the complete causal prefix for every requested pose.
        events = [event for event in script['events'] if event['tick'] <= end]
        batches.append(dict(script, captures=captures, events=events))
    return batches


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    binary, output = args.binary.resolve(), args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    fixtures = ROOT / 'crates/client-3d/scripts/fixtures/worn-equipment-review'
    receipt = {'status': 'running', 'visual_acceptance': 'not_reviewed',
               'commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
               'tree': subprocess.check_output(['git', 'rev-parse', 'HEAD^{tree}'], cwd=ROOT, text=True).strip(),
               'binary_sha256': digest(binary), 'scenes': [], 'failures': [],
               'environment': {k: os.environ[k] for k in ('WGPU_BACKEND', 'CARGO_PROFILE_DEV_DEBUG', 'CARGO_INCREMENTAL') if k in os.environ}}
    receipt_path = output / 'receipt.json'
    try:
        for source in sorted(fixtures.glob('*.json')):
            if source.name == 'index.json':
                continue
            script = json.loads(source.read_text())
            for part, batch in enumerate(capture_batches(script)):
                name = f'{source.stem}-part-{part + 1:02}'
                target = output / name
                prepared = output / f'{name}.json'
                prepared.write_text(json.dumps(batch, indent=2) + '\n')
                started = time.monotonic()
                command = [str(binary), '--scene-script', str(prepared), '--capture', str(target), '--offscreen']
                with (output / f'{name}.log').open('w') as log:
                    result = subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, timeout=900, check=False)
                if result.returncode:
                    receipt['failures'].append({'script': source.name, 'batch': prepared.name,
                                                'error': f'native process exited {result.returncode}',
                                                'seconds': time.monotonic() - started})
                    receipt_path.write_text(json.dumps(receipt, indent=2) + '\n')
                    # Every batch owns a new process and output directory. Keep
                    # independent evidence, but never turn partial work green.
                    continue
                manifest = json.loads((target / 'manifest.json').read_text())
                validate_manifest(batch, manifest, target)
                receipt['scenes'].append({'script': source.name, 'sha256': digest(source), 'batch': prepared.name,
                                          'batch_sha256': digest(prepared),
                                          'seconds': time.monotonic() - started,
                                          'captures': [{'file': f'{name}/{shot["name"]}.png',
                                                        'sha256': digest(target / f'{shot["name"]}.png')}
                                                       for shot in manifest['captures']]})
                receipt_path.write_text(json.dumps(receipt, indent=2) + '\n')
        if receipt['failures']:
            raise RuntimeError(f"{len(receipt['failures'])} capture batches failed; see retained manifests")
        receipt['status'] = 'complete'
    except Exception as error:
        receipt['status'] = 'failed'
        receipt['error'] = str(error)
        raise
    finally:
        receipt_path.write_text(json.dumps(receipt, indent=2) + '\n')


if __name__ == '__main__':
    main()
