"""Interpret activity receipts for decisions made by the Rogue player."""

from tor_client_bot import ActivityPhase, Turn


def receipt_history_expired(turn: Turn, phase: ActivityPhase | None, submitted_at: int | None) -> bool:
    """Only a later ready observation can retire a receipt with no reported phase."""
    return phase is None and submitted_at is not None and turn.tick > submitted_at and turn.can_act
