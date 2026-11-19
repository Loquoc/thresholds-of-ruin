"""Run a fixed seed cohort through the installed Python bot and a real server."""

import argparse
import hashlib
from collections import Counter
import json
import os
from pathlib import Path
import secrets
import subprocess
import sys
import tomllib

from process_harness import Process, ROOT
from rogue_o_matic import DEFAULT_OBJECTIVE_IDENTITY, DEFAULT_RETRIEVAL_DEPTH


POLICY_FILES = ("rogue_o_matic.py", "rogue_player.py")
SELF_DAMAGE_ABILITIES = {"rogue.wand_fire", "rogue.wand_cold", "rogue.wand_lightning", "rogue.breath_weapon"}


def death_cause(report):
    events = report.get("last_combat_events", [])
    player = report.get("self_target")
    for index in range(len(events) - 1, -1, -1):
        event = events[index]
        if event["type"] != "died" or (player is not None and event["actor"] != player):
            continue
        actor = event["actor"]
        if event.get("cause") == "starvation":
            return "starvation"
        for prior in reversed(events[:index]):
            if prior["type"] == "interrupted" and prior.get("actor") == actor:
                continue
            if (prior["type"] in {"ability", "unseen_ability"} and prior.get("target") == actor
                    and prior.get("outcome") == "applied"):
                if prior["type"] == "unseen_ability" and prior.get("caster") != actor:
                    return "unseen_ability"
                ability = (prior.get("ability") or {}).get("id")
                if prior.get("caster") != actor or ability in SELF_DAMAGE_ABILITIES:
                    return ability or "unidentified_ability"
            break
        if event.get("cause") == "injury":
            return "injury"
        break
    nutrition = report.get("last_nutrition", {})
    if nutrition.get("kind") == "required" and nutrition["reserve"] <= nutrition["starve_at"]:
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


def trace_report(path):
    progress = {"actions": 0, "max_depth": 0, "amulet_acquired": False,
                "floors": {}, "stair_transitions": []}
    previous = None
    terminal = None
    pending_searches = {}
    with path.open(encoding="utf-8") as log:
        for line in log:
            if not line.strip():
                continue
            terminal = None
            try:
                row = json.loads(line)
            except ValueError:
                continue
            if not isinstance(row, dict):
                continue
            phase = row.get("previous_intention_phase")
            if phase and (search_floor := pending_searches.pop(row.get("previous_intention_id"), None)) is not None:
                if phase in {"resolved", "failed"}:
                    progress["floors"][search_floor][f"searches_{phase}"] += 1
            if "terminal" in row:
                terminal = row
            elif "action" in row:
                progress["actions"] += 1
                progress["amulet_acquired"] |= row.get("amulet_acquired") is True
                floor = row["floor"]
                progress["max_depth"] = max(progress["max_depth"], floor)
                reserve = row.get("nutrition_reserve")
                metrics = progress["floors"].setdefault(str(floor), {
                    "searches_admitted": 0, "searches_resolved": 0, "searches_failed": 0,
                    "moves_admitted": 0,
                    "first_reserve": reserve, "last_reserve": reserve, "lowest_reserve": reserve,
                })
                if reserve is not None:
                    metrics["last_reserve"] = reserve
                    low = metrics["lowest_reserve"]
                    metrics["lowest_reserve"] = reserve if low is None else min(low, reserve)
                if row.get("result", {}).get("status") == "confirmed":
                    action = row["action"]
                    if action["type"] == "move":
                        metrics["moves_admitted"] += 1
                    elif action["type"] == "use_ability" and action.get("id") == "rogue.search":
                        metrics["searches_admitted"] += 1
                        receipt = row["result"].get("receipt") or {}
                        if intention := receipt.get("intention"):
                            pending_searches[intention] = str(floor)
                if previous is not None and previous[0] != floor:
                    progress["stair_transitions"].append({
                        "from": previous[0], "to": floor,
                        "before_reserve": previous[1], "after_reserve": reserve,
                    })
                previous = floor, reserve
    return terminal, progress


def verify_victory(address, token, actor, objective_identity):
    from tor_client_bot import Session

    with Session.connect(address, token, actor) as game:
        turn = game.observe()
        return turn.victorious and any(item.identity == objective_identity for item in turn.inventory)


def play(seed, server_path, scenario, actor, budget, timeout, output, retrieval_depth,
         objective_identity, policy_path):
    directory = output / f"seed-{seed}"
    directory.mkdir()
    token = secrets.token_urlsafe(32)
    server = Process(server_path,
                     ["--listen", "127.0.0.1:0", "--save", directory / "campaign.db",
                      "--scenario", scenario, "--character", str(actor), "--seed", str(seed)],
                     token=token, diagnostics=directory / "server")
    trace = directory / "turns.jsonl"
    try:
        line = server.until(lambda text: text.startswith("{") and '"address"' in text)
        address = json.loads(line)["address"]
        with trace.open("w", encoding="utf-8") as log, (directory / "bot.stderr").open("w", encoding="utf-8") as errors:
            result = subprocess.run(
                [sys.executable, str(policy_path), "--connect", address,
                 "--actor", str(actor), "--budget", str(budget), "--retrieval-depth", str(retrieval_depth),
                 "--objective-identity", objective_identity],
                cwd=ROOT, stdout=log, stderr=errors, timeout=timeout,
                env={**os.environ, "TOR_SERVER_TOKEN": token},
            )
        report, progress = trace_report(trace)
        if report is None or result.returncode != int(report["terminal"] != "victory"):
            return {"seed": seed, "terminal": "client_error", "exit_code": result.returncode, **progress}
        report = {**progress, **report}
        if report["terminal"] == "victory":
            try:
                verified = verify_victory(address, token, actor, objective_identity)
            except RuntimeError as error:
                return {"seed": seed, **report, "terminal": "client_error",
                        "victory_verified": False, "verification_error": str(error)}
            if not verified:
                return {"seed": seed, **report, "terminal": "client_error",
                        "victory_verified": False,
                        "verification_error": "Fresh player observation did not confirm victory with the retrieval item"}
            report["victory_verified"] = True
        return {"seed": seed, **report}
    except subprocess.TimeoutExpired:
        _, progress = trace_report(trace)
        return {"seed": seed, "terminal": "wall_timeout", **progress}
    finally:
        server.stop(save=False)


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
    with (scenario / "scenario.toml").open("rb") as manifest:
        objective = tomllib.load(manifest).get("unique_retrieval", {})
    retrieval_depth = objective.get("minimum_depth", DEFAULT_RETRIEVAL_DEPTH)
    inputs.update(retrieval_depth=retrieval_depth, objective_identity=args.objective_identity,
                  seeds=args.seeds)
    rows = []
    write_summary(output, inputs, rows)
    for seed in args.seeds:
        rows.append(play(seed, server, scenario, args.actor, args.budget, args.timeout, output,
                         retrieval_depth, args.objective_identity, policy_path))
        summary = write_summary(output, inputs, rows)
    print(json.dumps(summary))
    return int(any(row["terminal"] in {"client_error", "wall_timeout", "readiness_timeout"} for row in rows))


if __name__ == "__main__":
    raise SystemExit(main())
