"""Run the bounded TLC pilot plus optional model-only sanity checks."""
import argparse
from pathlib import Path
import re
import subprocess
import tempfile


HERE = Path(__file__).resolve().parent
MODULE = "RetryDurability"


def replace_once(text, old, new):
    if text.count(old) != 1:
        raise ValueError(f"expected one model fragment: {old!r}")
    return text.replace(old, new)


def run(jar, spec, config, expectation=None):
    with tempfile.TemporaryDirectory(prefix="tor-tlc-") as directory:
        root = Path(directory)
        (root / f"{MODULE}.tla").write_text(spec, encoding="utf-8")
        (root / f"{MODULE}.cfg").write_text(config, encoding="utf-8")
        result = subprocess.run(
            [
                "java", "-Xmx384m", "-XX:+UseParallelGC", "-cp", str(jar), "tlc2.TLC",
                "-workers", "1", "-metadir", "states", "-config", f"{MODULE}.cfg", MODULE,
            ],
            cwd=root,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
            timeout=120,
        )
    if expectation is None:
        match = re.search(r"(\d+) distinct states found, 0 states left on queue", result.stdout)
        if result.returncode == 0 and match and "No error has been found" in result.stdout:
            return f"{int(match[1]):,} distinct states, no violations"
    elif result.returncode == 12 and f"Invariant {expectation} is violated" in result.stdout:
        return f"observed expected counterexample to {expectation}"
    raise SystemExit(f"Unexpected TLC result ({result.returncode}):\n{result.stdout}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("jar", type=Path, help="local path to the upstream tla2tools.jar")
    parser.add_argument("--sanity", action="store_true", help="also check mutant and witness cases")
    args = parser.parse_args()
    jar = args.jar.resolve(strict=True)
    spec = (HERE / f"{MODULE}.tla").read_text(encoding="utf-8")
    config = (HERE / f"{MODULE}.cfg").read_text(encoding="utf-8")
    print("Baseline:", run(jar, spec, config))
    if not args.sanity:
        return

    mutants = (
        ("Premature save reply", "    /\\ IsPrefix(save.target, durable)\n", "", "SavePromiseSurvives"),
        ("Skip receipt lookup", "kind == IF known", "kind == IF FALSE /\\ known", "OneAdmissionPerId"),
        ("Admit stale input", 'IF request.basis # epoch THEN "stale"', 'IF FALSE THEN "stale"', "ReplyMatchesAdmission"),
    )
    for name, old, new, invariant in mutants:
        print(f"{name}:", run(jar, replace_once(spec, old, new), config, invariant))

    witnesses = (
        (
            "Durable receipt on a later attachment",
            'last.kind = "receipt" /\\ last.request.basis < epoch\n'
            '    /\\ HasExact(durable, last.request)',
        ),
        (
            "Saved prefix with a later volatile entry",
            "~save.active /\\ Len(promised) > 0 /\\ Len(durable) = Len(promised)\n"
            "    /\\ Len(admitted) > Len(durable)",
        ),
    )
    for name, state in witnesses:
        # Refuting the absence of a state proves that the scenario is reachable.
        witnessed = replace_once(
            spec, "=============================================================================",
            f"WitnessAbsent == ~(\n    {state}\n)\n=============================================================================",
        )
        witness_cfg = config.replace("CHECK_DEADLOCK", "    WitnessAbsent\nCHECK_DEADLOCK")
        print(f"{name}:", run(jar, witnessed, witness_cfg, "WitnessAbsent"))


if __name__ == "__main__":
    main()
