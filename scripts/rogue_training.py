"""Select and preview earned purchases for the Rogue player."""

from collections.abc import Iterator

from tor_client_bot import Outcome, PointPool, Result, Training, TrainingKind, TrainingOption, TrainingRefused


def training_choices(training: Training, skill: str) -> Iterator[tuple[PointPool, TrainingOption]]:
    priorities = (
        (TrainingKind.TALENT, ("rattling_blow", "dislocating_strike")),
        (TrainingKind.SKILL, (skill,)),
    )
    # Spend restricted points first so flexible pools remain available for later purchases.
    pools = sorted(training.pools(), key=lambda pool: len(pool.allowed))
    for kind, identities in priorities:
        eligible = [pool for pool in pools if pool.available and kind in pool.allowed]
        if not eligible:
            continue
        options = training.options(kind)
        for identity in identities:
            option = next((item for item in options if item.identity == identity and not item.forms), None)
            if option is not None:
                for pool in eligible:
                    yield pool, option


def purchase_training(training: Training, starting_skills: tuple[str, ...]) -> tuple[dict, Result] | None:
    """Confirm the first safe preview. A stale refusal requires a fresh observation."""
    skill = "spellcasting" if "spellcasting" in starting_skills else "heavy_weaponry"
    for pool, option in training_choices(training, skill):
        try:
            preview = training.preview(pool, option)
        except TrainingRefused as error:
            if error.result.status is Outcome.REJECTED and not error.result.stale:
                continue
            raise
        if not preview.would_die:
            purchase = {"type": "purchase", "kind": option.kind.value, "id": option.identity,
                        "hit_die": pool.hit_die, "pool": pool.identity}
            return purchase, training.confirm(preview)
    return None
