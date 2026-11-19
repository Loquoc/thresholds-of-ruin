"""Outcome scripts for generated scenario packages.

The engine decides no victory or defeat itself: packages end runs with Luau
scripts. Generated fixtures use the same small canonical scripts as the
authored packages, copied into each package because scripts must live inside
it. `death` loses the run when the selected character dies; `elimination`
ends an arena when at most one faction still has a living participant.
"""
from pathlib import Path

CANONICAL = Path(__file__).resolve().parent / "outcome_scripts"
# The Rogue exploration package, whose manifest many generators start from.
ROGUE_EXPLORATION = Path(__file__).resolve().parent.parent / "scenarios/rogue-exploration"

HOOKS = {
    "death": [{"on": "died", "actor": "selected", "handler": "outcome.died"}],
    "elimination": [{"on": "died", "actors": "character", "handler": "outcome.died"}],
}


def script(kind):
    """The canonical script text for `kind`."""
    if kind not in HOOKS:
        raise ValueError(f"Unknown outcome script {kind!r}")
    return (CANONICAL / f"{kind}.luau").read_text(encoding="utf-8")


def manifest_section(kind):
    """The `[scripting]` TOML that binds `kind`'s handler."""
    if kind not in HOOKS:
        raise ValueError(f"Unknown outcome script {kind!r}")
    lines = ["", "[scripting]", 'scripts = ["scripts/outcome.luau"]']
    for hook in HOOKS[kind]:
        lines += ["", "[[scripting.hooks]]"]
        lines += [f'{key} = "{value}"' for key, value in hook.items()]
    return "\n".join(lines) + "\n"


def add_outcome_script(package, kind):
    """Write `kind`'s script into `package` and bind it in its manifest."""
    package = Path(package)
    manifest = package / "scenario.toml"
    source = manifest.read_text(encoding="utf-8")
    if any(line.strip() == "[scripting]" for line in source.splitlines()):
        raise ValueError(f"{manifest} already declares scripts")
    (package / "scripts").mkdir(exist_ok=True)
    (package / "scripts" / "outcome.luau").write_text(script(kind), encoding="utf-8", newline="\n")
    manifest.write_text(source.rstrip("\n") + "\n" + manifest_section(kind), encoding="utf-8", newline="\n")
    return package


def apply(manifest, package, kind):
    """Give a generated manifest dictionary `kind`'s outcome script, replacing
    any it inherited, and write the script into `package`."""
    if kind not in HOOKS:
        raise ValueError(f"Unknown outcome script {kind!r}")
    package = Path(package)
    manifest["scripting"] = {"scripts": ["scripts/outcome.luau"],
                             "hooks": [dict(hook) for hook in HOOKS[kind]]}
    (package / "scripts").mkdir(parents=True, exist_ok=True)
    (package / "scripts" / "outcome.luau").write_text(script(kind), encoding="utf-8", newline="\n")
    return manifest


def copy_declared(manifest, package, source):
    """Copy the scripts a manifest dictionary declares from its `source`
    package, for generators that rewrite a scripted production manifest."""
    scripts = manifest.get("scripting", {}).get("scripts", [])
    if not scripts:
        # A manifest rebuilt from selected keys dropped its scripting; copying
        # nothing would silently remove the package's outcome.
        raise ValueError("The manifest declares no scripts to copy")
    for relative in scripts:
        target = Path(package) / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes((Path(source) / relative).read_bytes())
