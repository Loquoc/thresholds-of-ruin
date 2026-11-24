"""Run the Rogue player session and report decisions and outcomes."""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import asdict

from rogue_gear import EquipmentHistory
from rogue_player import Adventurer
from tor_client_bot import Direction, Outcome, Result, Session, TrainingRefused, Turn, describe_action


def trace_result(result: Result) -> dict:
    receipt = {"intention": result.intention}
    if result.activity:
        receipt.update(type="admitted", **asdict(result.activity))
    record = {"status": result.status.value, "receipt": receipt,
              "code": result.code, "message": result.reason}
    if result.request_id:
        record["request_id"] = result.request_id
    return record


def trace_position(turn: Turn, player: Adventurer) -> dict:
    memory = player.maps.get(player.depth)
    return {
        "floor": player.depth,
        "cell": turn.cell.key if turn.cell else None,
        "known_cells": len(memory.cells) if memory is not None else None,
        "nutrition_reserve": turn.food_reserve,
    }


def play(game: Session, player: Adventurer, budget: int, emit: Callable[[dict], None],
         *, record_decisions: bool = False) -> dict:
    """Observe the outcome after the last budgeted submission, including victory."""
    count = 0
    deepest = player.depth
    acquired = False
    detail: dict[str, object] = {}
    equipment = EquipmentHistory()
    for decision in game.decisions(include_stalls=True):
        turn, combat = decision.turn, decision.combat
        equipment.observe(turn)
        if player.training_checked_at is None:
            player.training_checked_at = turn.hit_dice
        previous = decision.previous
        events = combat.records()
        acquired |= player.carrying_objective(turn)
        if turn.terminal:
            reason = "terminal"
            if turn.victorious:
                reason = "victory"
            elif turn.dead:
                reason = "death"
            detail.update(
                last_combat_events=turn.recent_combat.records() or events,
                events_since_previous_action=events,
                self_target=turn.self_target,
            )
            break
        player.observe(turn, combat)
        deepest = max(deepest, player.depth)
        if count >= budget:
            reason = "action_budget"
            break
        purchase = None
        action = None
        if player.ready_to_train(turn, stalled=decision.stalled):
            try:
                purchase = player.train(turn, game.training)
            except TrainingRefused as error:
                purchase = {"type": "training_preview"}, error.result
        if purchase:
            command, result = purchase
            reason = "earned_training"
        else:
            action, reason = player.stalled(turn, previous) if decision.stalled else player.choose(turn)
            if action is None:
                if decision.stalled:
                    detail.update(health=asdict(turn.health) if turn.health else None,
                                  conditions=[asdict(condition) for condition in turn.conditions])
                elif reason == "no_disclosed_route":
                    memory = player.maps[player.depth]
                    detail["navigation"] = {
                        "floor": player.depth,
                        "visited_cells": len(memory.visited),
                        "known_down_stairs": len(memory.stairs(Direction.DOWN)),
                        "searches_admitted": sum(player.searches[key] for key in memory.cells),
                    }
                break
            result = turn.submit(action)
            command = describe_action(action)
        count += 1
        emit(
            {
                **({"recording": turn.recording()} if record_decisions else {}),
                **trace_position(turn, player),
                "tick": turn.tick,
                "reachable_pickups": [
                    {"item": ground.item.target, "category": ground.item.category,
                     "identity": ground.item.identity, "quantity": ground.item.quantity,
                     "cell": ground.cell.key if ground.cell else None}
                    for ground in turn.ground_items if ground.reachable
                ],
                **({"stalled": True} if decision.stalled else {}),
                "reason": reason,
                "action": command,
                "result": trace_result(result),
                "previous_intention_id": previous.intention if previous else None,
                "previous_intention_phase": decision.previous_phase,
                "health_recovery": asdict(turn.recovery) if turn.recovery else None,
                "events_since_previous_action": events,
                "amulet_acquired": acquired,
            }
        )
        if result.status is Outcome.CONFIRMED:
            if action is not None:
                player.admitted(turn, action, result, reason=reason)
                equipment.admitted(action)
        elif not result.stale:
            previous = result
            reason = result.reason or result.status.value
            break
    else:
        raise RuntimeError("The decision stream ended before a terminal picture")
    if not turn.terminal:
        detail.update(trace_position(turn, player))
    detail.update(
        **equipment.report(),
        estimated_ascent_ticks_by_floor=player.ascent_ticks_by_floor,
        estimated_ascent_ticks=sum(player.ascent_ticks_by_floor.values()),
    )
    if record_decisions:
        detail["recording"] = turn.recording()
    if previous:
        detail.update(
            previous_intention_id=previous.intention, previous_intention_phase=turn.outcome_of(previous)
        )
    return {
        "terminal": reason,
        "actions": count,
        "max_depth": deepest,
        "amulet_acquired": acquired,
        "tick": turn.tick,
        **detail,
    }
