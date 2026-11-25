"""Run a fixed seed cohort through the installed Python bot and a real server."""

import argparse
import hashlib
from collections import Counter
from dataclasses import dataclass
from importlib.machinery import EXTENSION_SUFFIXES
from importlib.util import find_spec
import json
import os
from pathlib import Path
import secrets
import shutil
import subprocess
import sys
import tomllib

from process_harness import Process, ROOT
from rogue_o_matic import DEFAULT_OBJECTIVE_IDENTITY, DEFAULT_RETRIEVAL_DEPTH, player_exit_code


POLICY_FILES = (
    "rogue_o_matic.py",
    "rogue_player.py",
    "rogue_runner.py",
    "rogue_gear.py",
    "rogue_food.py",
    "rogue_retreat.py",
    "rogue_loot.py",
    "rogue_training.py",
)
SELF_DAMAGE_ABILITIES = {"rogue.wand_fire", "rogue.wand_cold", "rogue.wand_lightning", "rogue.breath_weapon"}


def preceding_attack(events_before_death, actor):
    """Read newest first. Only the dying actor's interruptions can follow the attack."""
    for event in events_before_death:
        kind = event["type"]
        if kind == "interrupted" and event.get("actor") == actor:
            continue
        if (kind not in {"ability", "unseen_ability"} or event.get("target") != actor
                or event.get("outcome") != "applied"):
            return None
        self_inflicted = event.get("caster") == actor
        if kind == "unseen_ability" and not self_inflicted:
            return "unseen_ability"
        ability = (event.get("ability") or {}).get("id")
        if self_inflicted and ability not in SELF_DAMAGE_ABILITIES:
            return None
        return ability or "unidentified_ability"
    return None


def death_cause(report):
    history = report.get("last_combat_events", [])
    player = report.get("self_target")
    if player is None:
        # Older reports omit the player target. Their events are usable only for a single victim.
        victims = {event["actor"] for event in history if event["type"] == "died"}
        if len(victims) == 1:
            player = victims.pop()
    events = reversed(history)
    for event in events:
        if event["type"] != "died" or player is None or event["actor"] != player:
            continue
        if event.get("cause") == "starvation":
            return "starvation"
        if attack := preceding_attack(events, event["actor"]):
            return attack
        if event.get("cause") == "injury":
            return "injury"
        break
    nutrition = report.get("last_nutrition")
    if isinstance(nutrition, dict) and nutrition.get("kind") == "required":
        reserve = nutrition.get("reserve")
        threshold = nutrition.get("starve_at")
        if type(reserve) is int and type(threshold) is int and reserve <= threshold:
            return "starvation"
    return "undisclosed"


def snapshot_policy(source, output):
    directory = output / "policy"
    directory.mkdir()
    hashes = {}
    for name in POLICY_FILES:
        data = (source / name).read_bytes()
        (directory / name).write_bytes(data)
        hashes[name] = hashlib.sha256(data).hexdigest()
    return directory / "rogue_o_matic.py", hashes


def snapshot_bindings(source, policy_directory):
    """Put the runnable package beside the policy so every seed imports the same bytes."""
    directory = policy_directory / "tor_client_bot"
    directory.mkdir()
    files = sorted(source.glob("*.py"))
    native = next((source / ("_native" + suffix) for suffix in EXTENSION_SUFFIXES
                   if (source / ("_native" + suffix)).is_file()), None)
    if native:
        files.append(native)
    hashes = {}
    for path in files:
        target = directory / path.name
        shutil.copyfile(path, target)
        with target.open("rb") as copied:
            hashes[path.name] = hashlib.file_digest(copied, "sha256").hexdigest()
    return hashes


def campaign_inputs(server, scenario, actor, policy):
    with server.open("rb") as executable:
        server_hash = hashlib.file_digest(executable, "sha256").hexdigest()
    inputs = {
        "server": str(server),
        "server_sha256": server_hash,
        "scenario": str(scenario),
        "actor": actor,
        "scenario_manifest_sha256": hashlib.sha256((scenario / "scenario.toml").read_bytes()).hexdigest(),
        "policy_sha256": hashlib.sha256(policy.read_bytes()).hexdigest(),
    }
    for key, name in (("scenario_rules_sha256", "rules.toml"),
                      ("scenario_validation_sha256", "validation.json")):
        path = scenario / name
        if path.is_file():
            inputs[key] = hashlib.sha256(path.read_bytes()).hexdigest()
    return inputs


def summarize(rows):
    return {
        "runs": len(rows),
        "victories": sum(row["terminal"] == "victory" and row.get("victory_verified") is True
                         for row in rows),
        "amulets_acquired": sum(bool(row.get("amulet_acquired")) for row in rows),
        "max_depth": max((row.get("max_depth", 0) for row in rows), default=0),
        "deaths_by_cause": dict(sorted(Counter(
            death_cause(row) for row in rows if row["terminal"] == "death").items())),
        "results": rows,
    }


@dataclass(frozen=True)
class TraceSite:
    floor: int
    known_cells: int
    cell: str


@dataclass(frozen=True)
class AdmittedMove:
    intention: str
    origin: TraceSite
    reason: str | None
    tick: int
    reserve: int | None

    @classmethod
    def from_trace(cls, row: dict, origin: TraceSite):
        result = row.get("result", {})
        if row.get("action", {}).get("type") != "move" or result.get("status") != "confirmed":
            return None
        receipt = result.get("receipt") or {}
        if intention := receipt.get("intention"):
            return cls(intention, origin, row.get("reason"), row["tick"], row.get("nutrition_reserve"))
        return None

    def resolved_at(self, row, site: TraceSite) -> bool:
        return (row.get("previous_intention_id") == self.intention
                and row.get("previous_intention_phase") == "resolved"
                and self.origin.floor == site.floor and self.origin.cell != site.cell)


class FloorVisits:
    """Accumulate travel, nutrition, and search outcomes by floor across repeat visits."""

    def __init__(self) -> None:
        self.floors: dict[str, dict[str, int | None]] = {}
        self.transitions: list[dict[str, int | None]] = []
        self.previous_floor: int | None = None
        self.previous_reserve: int | None = None
        self.pending_searches: dict[str, str] = {}

    def resolve_search(self, row: dict) -> None:
        phase = row.get("previous_intention_phase")
        if phase in {"resolved", "failed", "cancelled"}:
            search_floor = self.pending_searches.pop(row.get("previous_intention_id"), None)
            if search_floor is not None and phase != "cancelled":
                self.floors[search_floor][f"searches_{phase}"] += 1

    def record_observation(self, row: dict) -> None:
        floor = row["floor"]
        reserve = row.get("nutrition_reserve")
        metrics = self.floors.setdefault(str(floor), {
            "searches_admitted": 0, "searches_resolved": 0, "searches_failed": 0,
            "moves_admitted": 0,
            "first_reserve": None, "last_reserve": None, "lowest_reserve": None,
        })
        if reserve is not None:
            if metrics["first_reserve"] is None:
                metrics["first_reserve"] = reserve
            metrics["last_reserve"] = reserve
            low = metrics["lowest_reserve"]
            metrics["lowest_reserve"] = reserve if low is None else min(low, reserve)
        if "action" in row and row.get("result", {}).get("status") == "confirmed":
            action = row["action"]
            if action["type"] == "move":
                metrics["moves_admitted"] += 1
            elif action["type"] == "use_ability" and action.get("id") == "rogue.search":
                metrics["searches_admitted"] += 1
                receipt = row["result"].get("receipt") or {}
                if intention := receipt.get("intention"):
                    self.pending_searches[intention] = str(floor)
        if self.previous_floor is not None and self.previous_floor != floor:
            self.transitions.append({
                "from": self.previous_floor, "to": floor,
                "before_reserve": self.previous_reserve, "after_reserve": reserve,
            })
        self.previous_floor = floor
        self.previous_reserve = reserve


def trace_rows(log):
    """Yield an action or terminal report per nonblank line, with None marking a gap."""
    for line in log:
        if not line.strip():
            continue
        try:
            row = json.loads(line.decode("utf-8"))
        except ValueError:
            row = None
        if isinstance(row, dict):
            if "terminal" in row or (isinstance(row.get("action"), dict) and row.get("floor") is not None):
                yield row
                continue
        yield None


def trace_report(path):
    progress = {"actions": 0, "max_depth": 0, "amulet_acquired": False,
                "closest_return_floor": None}
    visits = FloorVisits()
    terminal = None
    last_move = None
    previous_origin = None
    with path.open("rb") as log:
        for row in trace_rows(log):
            terminal = None
            if row is None:
                # Costs and transitions need consecutive observations. Totals from intact rows survive.
                last_move = previous_origin = None
                visits.previous_floor = None
                visits.previous_reserve = None
                continue
            if row.get("floor") is not None and row.get("cell") is not None and row.get("known_cells") is not None:
                detours = progress.setdefault("supply_detours", {"ticks": 0, "reserve_spent": 0, "resolved_steps": 0})
                progress.setdefault("reversals_without_new_terrain", 0)
                site = TraceSite(row["floor"], row["known_cells"], row["cell"])
                if last_move and last_move.resolved_at(row, site):
                    if previous_origin == site and last_move.origin.known_cells == site.known_cells:
                        progress["reversals_without_new_terrain"] += 1
                    previous_origin = last_move.origin
                    if last_move.reason == "seek_supplies":
                        detours["resolved_steps"] += 1
                        detours["ticks"] += row["tick"] - last_move.tick
                        if row.get("nutrition_reserve") is not None and last_move.reserve is not None:
                            detours["reserve_spent"] += max(0, last_move.reserve - row["nutrition_reserve"])
                else:
                    previous_origin = None
                last_move = AdmittedMove.from_trace(row, site)
            else:
                last_move = previous_origin = None
            visits.resolve_search(row)
            if "terminal" in row:
                terminal = row
                # Older summaries have no observed position. Their max_depth is a run total.
                if row.get("cell") is None or row.get("floor") is None:
                    continue
            else:
                progress["actions"] += 1
            visits.record_observation(row)
            floor = row["floor"]
            progress["max_depth"] = max(progress["max_depth"], floor)
            progress["amulet_acquired"] |= row.get("amulet_acquired") is True
            if progress["amulet_acquired"]:
                closest = progress["closest_return_floor"]
                progress["closest_return_floor"] = floor if closest is None else min(closest, floor)
    progress.update(floors=visits.floors, stair_transitions=visits.transitions)
    return terminal, progress


def verify_victory(address, token, actor, objective_identity):
    from tor_client_bot import Session

    with Session.connect(address, token, actor) as game:
        turn = game.observe()
        return turn.victorious and any(item.identity == objective_identity for item in turn.inventory)


def play(seed, server_path, scenario, actor, budget, timeout, output, retrieval_depth,
         objective_identity, policy_path, record_decisions=False):
    directory = output / f"seed-{seed}"
    directory.mkdir()
    token = secrets.token_urlsafe(32)
    server = Process(server_path,
                     ["--listen", "127.0.0.1:0", "--save", directory / "campaign.db",
                      "--scenario", scenario, "--character", str(actor), "--seed", str(seed)],
                     token=token, diagnostics=directory / "server")
    trace = directory / "turns.jsonl"
    report = {}
    try:
        report = play_on_server(server, token, actor, budget, timeout, trace, retrieval_depth,
                                objective_identity, policy_path, record_decisions)
    finally:
        try:
            server.stop(save=False)
        except subprocess.TimeoutExpired:
            report["server_cleanup"] = "timeout"
    return {"seed": seed, **report}


def play_on_server(server, token, actor, budget, timeout, trace, retrieval_depth,
                   objective_identity, policy_path, record_decisions):
    try:
        line = server.until(lambda text: text.startswith("{") and '"address"' in text, include_transcript=False)
        address = json.loads(line)["address"]
    except (AssertionError, ValueError, KeyError) as error:
        return {"terminal": "server_startup_error", "startup_error": str(error), "actions": 0}
    try:
        with trace.open("w", encoding="utf-8") as log, trace.with_name("bot.stderr").open("w", encoding="utf-8") as errors:
            policy_imports = [str(Path(policy_path).resolve().parent)]
            if inherited := os.environ.get("PYTHONPATH"):
                policy_imports.append(inherited)
            result = subprocess.run(
                [sys.executable, str(policy_path), "--connect", address,
                 "--actor", str(actor), "--budget", str(budget), "--retrieval-depth", str(retrieval_depth),
                 "--objective-identity", objective_identity, *(["--record-decisions"] if record_decisions else [])],
                cwd=ROOT, stdout=log, stderr=errors, timeout=timeout,
                env={**os.environ, "PYTHONPATH": os.pathsep.join(policy_imports), "TOR_SERVER_TOKEN": token},
            )
        report, progress = trace_report(trace)
        if report is None or result.returncode != player_exit_code(report["terminal"]):
            return {"terminal": "client_error", "exit_code": result.returncode, **progress}
        report = {**progress, **report}
        if report["terminal"] != "victory":
            return report
        try:
            if verify_victory(address, token, actor, objective_identity):
                return {**report, "victory_verified": True}
            message = "Fresh player observation did not confirm victory with the retrieval item"
        except RuntimeError as error:
            message = str(error)
        return {**report, "terminal": "client_error", "victory_verified": False, "verification_error": message}
    except subprocess.TimeoutExpired:
        _, progress = trace_report(trace)
        return {"terminal": "wall_timeout", **progress}


def write_summary(output, inputs, rows):
    summary = {"inputs": inputs, **summarize(rows)}
    draft = output / "summary.json.tmp"
    draft.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    draft.replace(output / "summary.json")
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", type=Path, required=True)
    parser.add_argument("--scenario", type=Path, default=ROOT / "scenarios/rogue")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--actor", type=int, default=1)
    parser.add_argument("--budget", type=int, default=20_000)
    parser.add_argument("--timeout", type=int, default=900)
    parser.add_argument("--objective-identity", default=DEFAULT_OBJECTIVE_IDENTITY)
    parser.add_argument("seeds", type=int, nargs="+")
    parser.add_argument("--record-decisions", action="store_true",
                        help="save replayable player observations in each seed trace")
    args = parser.parse_args()
    if len(args.seeds) != len(set(args.seeds)) or min(args.seeds) < 0:
        parser.error("Seeds must be unique nonnegative integers")
    if args.budget < 1 or args.timeout < 1:
        parser.error("Budget and timeout must be positive")

    output = args.output.resolve()
    output.mkdir(parents=True)
    server, scenario = args.server.resolve(), args.scenario.resolve()
    policy_path, policy_hashes = snapshot_policy(ROOT / "scripts", output)
    inputs = campaign_inputs(server, scenario, args.actor, policy_path)
    inputs["policy_sources_sha256"] = policy_hashes
    bindings = find_spec("tor_client_bot")
    inputs["bot_bindings_sha256"] = (
        snapshot_bindings(Path(bindings.origin).parent, policy_path.parent)
        if bindings and bindings.origin else None
    )
    with (scenario / "scenario.toml").open("rb") as manifest:
        objective = tomllib.load(manifest).get("unique_retrieval", {})
    retrieval_depth = objective.get("minimum_depth", DEFAULT_RETRIEVAL_DEPTH)
    inputs.update(retrieval_depth=retrieval_depth, objective_identity=args.objective_identity,
                  seeds=args.seeds, action_budget=args.budget, timeout_seconds=args.timeout,
                  record_decisions=args.record_decisions)
    rows = []
    write_summary(output, inputs, rows)
    for seed in args.seeds:
        rows.append(play(seed, server, scenario, args.actor, args.budget, args.timeout, output,
                         retrieval_depth, args.objective_identity, policy_path, args.record_decisions))
        summary = write_summary(output, inputs, rows)
    print(json.dumps(summary))
    return int(any(row.get("server_cleanup") or row["terminal"] in
                   {"client_error", "server_startup_error", "unknown_outcome", "wall_timeout", "readiness_timeout"}
                   for row in rows))


if __name__ == "__main__":
    raise SystemExit(main())
