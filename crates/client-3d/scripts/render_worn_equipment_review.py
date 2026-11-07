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
               'binary_sha256': digest(binary), 'scenes': [],
               'environment': {k: os.environ[k] for k in ('WGPU_BACKEND', 'CARGO_PROFILE_DEV_DEBUG', 'CARGO_INCREMENTAL') if k in os.environ}}
    receipt_path = output / 'receipt.json'
    try:
        for source in sorted(fixtures.glob('*.json')):
            if source.name == 'index.json':
                continue
            script = json.loads(source.read_text())
            target = output / source.stem
            started = time.monotonic()
            command = [str(binary), '--scene-script', str(source), '--capture', str(target), '--offscreen']
            with (output / f'{source.stem}.log').open('w') as log:
                result = subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, timeout=900, check=False)
            if result.returncode:
                raise RuntimeError(f'{source.name} exited {result.returncode}')
            manifest = json.loads((target / 'manifest.json').read_text())
            validate_manifest(script, manifest, target)
            receipt['scenes'].append({'script': source.name, 'sha256': digest(source),
                                      'seconds': time.monotonic() - started,
                                      'captures': [{'file': f'{source.stem}/{shot["name"]}.png',
                                                    'sha256': digest(target / f'{shot["name"]}.png')}
                                                   for shot in manifest['captures']]})
            receipt_path.write_text(json.dumps(receipt, indent=2) + '\n')
        receipt['status'] = 'complete'
    except Exception as error:
        receipt['status'] = 'failed'
        receipt['error'] = str(error)
        raise
    finally:
        receipt_path.write_text(json.dumps(receipt, indent=2) + '\n')


if __name__ == '__main__':
    main()
