"""Combat evidence shared by the Rogue player and its cohort reports."""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from tor_client_bot import Creature, IncomingAbility, OutgoingAbility


SELF_DAMAGE_ABILITIES = {"rogue.wand_fire", "rogue.wand_cold", "rogue.wand_lightning", "rogue.breath_weapon"}


def includes_self_damage(outgoing: tuple[OutgoingAbility, ...], player: str | None) -> bool:
    """Whether the player's own ability could account for the observed health loss."""
    from tor_client_bot import AbilityOutcome

    return player is not None and any(
        effect.target == player and effect.ability in SELF_DAMAGE_ABILITIES and effect.outcome is AbilityOutcome.APPLIED
        for effect in outgoing
    )


def witnessed_unseen_attack(incoming: tuple[IncomingAbility, ...], hostiles: tuple[Creature, ...],
                            *, injured: bool) -> bool:
    """Recognize a hostile source, a missed attack, or an applied effect accompanied by injury."""
    from tor_client_bot import AbilityOutcome

    for event in incoming:
        if not event.unseen:
            continue
        if event.outcome is AbilityOutcome.MISS or (injured and event.outcome is AbilityOutcome.APPLIED):
            return True
        if any(foe.target == event.caster for foe in hostiles):
            return True
    return False


def sole_applied_attacker(incoming: tuple[IncomingAbility, ...]) -> str | None:
    """Attribute an injury only when every applied hit came from one known source."""
    from tor_client_bot import AbilityOutcome

    sources = {event.caster for event in incoming if event.outcome is AbilityOutcome.APPLIED}
    return sources.pop() if len(sources) == 1 else None
