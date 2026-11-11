"""Check that hosted workflows still match the repository's usage review."""
import hashlib
import json
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
REVIEW = ROOT / 'scripts/testdata/ci-usage-review.json'


def digest(path):
    return hashlib.sha256(path.read_bytes().replace(b'\r\n', b'\n')).hexdigest()


def differences(root, reviewed):
    workflows = {path.relative_to(root).as_posix()
                 for path in (root / '.github/workflows').iterdir()
                 if path.suffix.lower() in {'.yml', '.yaml'}}
    issues = [f'unreviewed: {name}' for name in sorted(workflows - reviewed.keys())]
    for name, expected in sorted(reviewed.items()):
        path = root / name
        if not path.is_file() or path.is_symlink():
            issues.append(f'missing: {name}')
        elif digest(path) != expected:
            issues.append(f'changed: {name}')
    return issues


def main():
    issues = differences(ROOT, json.loads(REVIEW.read_text(encoding='utf-8')))
    if issues:
        print('Review the changed CI inputs against docs/development/ci-usage.md before updating the review record.',
              file=sys.stderr)
        print('\n'.join(issues), file=sys.stderr)
        return 1
    print('Reviewed CI inputs match the local usage review.')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
