"""Run the bounded ordering experiments and check their controls."""
import argparse
from pathlib import Path
import re
import subprocess
import tempfile


HERE = Path(__file__).resolve().parent

CASES = {
    "WieldOrdering": [
        ("immediate update before receipt", None, "ImmediateOrderAbsent"),
        ("delayed update after receipt", None, "DelayedOrderAbsent"),
        ("occupied socket refusal", None, "RefusalAbsent"),
        ("cleared socket filled before continuation", None, "ReoccupiedAbsent"),
        ("replacement unavailable at continuation", None, "UnavailableAbsent"),
        ("eligible replacement is submitted", None, "EquipAbsent"),
        ("ignore receipt phase", "ignore-receipt-phase", "TerminalNotLost"),
        ("ignore occupied socket", "ignore-slot", "EquipUsesCurrentEmptySlot"),
        ("ignore replacement availability", "ignore-incoming", "EquipUsesAvailableItem"),
    ],
    "SupplyClaims": [
        ("pause while the free unit is taken", None, "PartialTakeAbsent"),
        ("reclaim the exact unit after cancellation", None, "ReclaimAbsent"),
        ("allow overlapping claims", "allow-double-hold", "ExclusiveClaim"),
        ("consume an unpaid cancellation", "consume-on-cancel", "ConservesStock"),
        ("charge an already paid retry", "pay-retry", "PaidAtMostOnce"),
    ],
}


def run(jar, module, config, expected=None):
    with tempfile.TemporaryDirectory(prefix="tor-tlc-ordering-") as directory:
        dest = Path(directory)
        source = HERE / f"{module}.tla"
        (dest / source.name).write_bytes(source.read_bytes())
        (dest / f"{module}.cfg").write_text(config, encoding="utf-8")
        result = subprocess.run(
            ["java", "-Xmx384m", "-XX:+UseParallelGC", "-cp", str(jar),
             "tlc2.TLC", "-workers", "1", "-metadir", "states",
             "-config", f"{module}.cfg", module],
            cwd=dest, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True, check=False, timeout=120)
    if expected is None:
        states = re.search(r"(\d+) distinct states found, 0 states left on queue", result.stdout)
        if result.returncode == 0 and states and "No error has been found" in result.stdout:
            return f"{int(states[1]):,} distinct states, no violations"
    elif result.returncode == 12 and f"Invariant {expected} is violated" in result.stdout:
        return f"expected counterexample to {expected}"
    raise SystemExit(f"Unexpected TLC result ({result.returncode}):\n{result.stdout}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("jar", type=Path, help="local path to tla2tools.jar")
    args = parser.parse_args()
    jar = args.jar.resolve(strict=True)
    for module, cases in CASES.items():
        config = (HERE / f"{module}.cfg").read_text(encoding="utf-8")
        print(f"{module}: {run(jar, module, config)}", flush=True)
        if module == "SupplyClaims":
            for units in ('{"unit-1"}', '{"unit-1", "unit-2", "unit-3"}'):
                alternate = config.replace('{"unit-1", "unit-2"}', units)
                print(f"  {units}: {run(jar, module, alternate)}", flush=True)
        for description, variant, invariant in cases:
            if variant is None:
                control = config + f"INVARIANT {invariant}\n"
            else:
                control = config.replace('"normal"', f'"{variant}"')
            print(f"  {description}: {run(jar, module, control, invariant)}", flush=True)


if __name__ == "__main__":
    main()
