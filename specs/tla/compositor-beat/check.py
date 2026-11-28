"""Run the compositor beat model and its deliberate failure/reachability checks."""
import argparse
from pathlib import Path
import re
import subprocess
import tempfile


HERE = Path(__file__).resolve().parent
NAME = "CompositorBeat"


def run(jar, config, expected=None):
    with tempfile.TemporaryDirectory(prefix="tor-compositor-tlc-") as directory:
        root = Path(directory)
        source = HERE / f"{NAME}.tla"
        (root / source.name).write_bytes(source.read_bytes())
        (root / f"{NAME}.cfg").write_text(config, encoding="utf-8")
        result = subprocess.run(
            ["java", "-Xmx384m", "-XX:+UseParallelGC", "-cp", str(jar),
             "tlc2.TLC", "-workers", "1", "-metadir", "states",
             "-config", f"{NAME}.cfg", NAME],
            cwd=root, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True, check=False, timeout=120)
    if expected is None:
        count = re.search(r"(\d+) distinct states found, 0 states left on queue", result.stdout)
        if count and result.returncode == 0 and "No error has been found" in result.stdout:
            return f"{int(count[1]):,} distinct states, no violations"
    elif result.returncode == 12 and f"Invariant {expected} is violated" in result.stdout:
        return f"expected counterexample to {expected}"
    raise SystemExit(f"Unexpected TLC result ({result.returncode}):\n{result.stdout}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("jar", type=Path, help="local path to tla2tools.jar")
    args = parser.parse_args()
    jar = args.jar.resolve(strict=True)
    config = (HERE / f"{NAME}.cfg").read_text(encoding="utf-8")
    for steps, lead in [(2, 1), (3, 1), (3, 0)]:
        alternate = config.replace("Steps = 2", f"Steps = {steps}").replace("Lead = 1", f"Lead = {lead}")
        print(f"{steps} steps, lead {lead}: {run(jar, alternate)}", flush=True)
    for invariant in (
        "ReceiptFirstAbsent", "OutcomeFirstAbsent", "SkipWithoutPermissionAbsent",
        "SecondStepWhilePlayingAbsent", "OrdinaryReturnAbsent", "ReplacementReturnAbsent",
    ):
        print(invariant + ": " + run(jar, config + f"INVARIANT {invariant}\n", invariant), flush=True)
    for variant, invariant in (
        ("receipt-only", "AuthorizedContinuation"),
        ("playback-only", "AuthorizedContinuation"),
        ("retain-old-plan", "ReplacementStopsPlan"),
        ("retain-old-frames", "ReplacementFlushes"),
        ("return-before-drain", "InputAfterDrain"),
    ):
        print(variant + ": " + run(jar, config.replace('"normal"', f'"{variant}"'), invariant), flush=True)


if __name__ == "__main__":
    main()
