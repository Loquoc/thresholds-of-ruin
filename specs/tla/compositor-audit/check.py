"""Run the bounded compositor audits; optionally retain TLC's exact traces."""
import argparse
from pathlib import Path
import re
import subprocess
import tempfile


HERE = Path(__file__).resolve().parent
AUDITS = {
    "SettleAudit": (
        None,
        ("StartedPermissionAbsent", "QueuedYieldAbsent", "TimedOutAfterReceiptAbsent", "RetainedFailureAbsent"),
        {
            "reuse-started-permission": "DoneHasFreshBoundary",
            "accept-old-permission": "DoneHasFreshBoundary",
            "accept-other-failure": "CorrectResult",
            "yield-after-known-failure": "DoNotYieldFailure",
            "snapshot-is-done": "SnapshotIsNotSuccess",
            "terminal-receipt": "CorrectResult",
        },
    ),
    "ViewQueueAudit": (
        ("Views = 3", "Views = 4"),
        ("FoldAbsent", "ReadyHeldByLastViewAbsent", "CaptureBacklogAbsent", "ResetBetweenViewsAbsent"),
        {
            "ready-on-read-ahead": "ReadyFollowsAppliedViews",
            "merge-epochs": "DoNotFoldAcrossBoundaries",
            "merge-generations": "DoNotFoldAcrossBoundaries",
            "merge-own-step": "DoNotFoldAcrossBoundaries",
            "drop-on-fold": "PreservesEveryViewInOrder",
            "consume-during-capture": "CaptureHoldsPresentation",
        },
    ),
    "TimelineAudit": (
        ("Events = 4", "Events = 5"),
        ("ParallelSpansAbsent", "AmbiguousFlightsAbsent", "ResetAfterHitAbsent"),
        {
            "cross-actor-close": "ExactSpanBoundary",
            "no-death-close": "ExactSpanBoundary",
            "ignore-caster": "LinksMatchPriorFacts",
            "ignore-tick": "LinksMatchPriorFacts",
            "retain-reactions": "FrameAndScope",
        },
    ),
}


def run(jar, module, config, name, traces, expected=None):
    with tempfile.TemporaryDirectory(prefix="tor-compositor-audit-") as directory:
        root = Path(directory)
        source = HERE / f"{module}.tla"
        (root / source.name).write_bytes(source.read_bytes())
        (root / f"{module}.cfg").write_text(config, encoding="utf-8")
        result = subprocess.run(
            ["java", "-Xmx384m", "-XX:+UseParallelGC", "-cp", str(jar), "tlc2.TLC",
             "-workers", "1", "-metadir", "states", "-config", f"{module}.cfg", module],
            cwd=root, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True, check=False, timeout=120,
        )
    if traces:
        traces.joinpath(f"{module}-{name}.log").write_text(result.stdout, encoding="utf-8")
    if expected is None:
        count = re.search(r"(\d+) distinct states found, 0 states left on queue", result.stdout)
        if count and result.returncode == 0 and "No error has been found" in result.stdout:
            return f"{int(count[1]):,} distinct states, no violations"
    elif result.returncode == 12 and f"Invariant {expected} is violated" in result.stdout:
        return f"expected counterexample to {expected}"
    raise SystemExit(f"{module}/{name}: unexpected TLC result ({result.returncode}):\n{result.stdout}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("jar", type=Path, help="local path to the pinned tla2tools.jar")
    parser.add_argument("--traces", type=Path, help="save the full TLC output per case")
    args = parser.parse_args()
    jar = args.jar.resolve(strict=True)
    if args.traces:
        args.traces.mkdir(parents=True, exist_ok=True)
    for module, (wider, witnesses, variants) in AUDITS.items():
        config = (HERE / f"{module}.cfg").read_text(encoding="utf-8")
        print(module + ":", run(jar, module, config, "baseline", args.traces), flush=True)
        if wider:
            print("  wider:", run(jar, module, config.replace(*wider), "wider", args.traces), flush=True)
        for label in witnesses:
            modified = config + f"INVARIANT {label}\n"
            print(f"  {label}:", run(jar, module, modified, label, args.traces, label), flush=True)
        for label, expected in variants.items():
            modified = config.replace('"normal"', f'"{label}"')
            print(f"  {label}:", run(jar, module, modified, label, args.traces, expected), flush=True)


if __name__ == "__main__":
    main()
