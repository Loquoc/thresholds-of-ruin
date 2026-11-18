"""Owner-only advancement choices backed by the common client's preview session."""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import TYPE_CHECKING, Iterator

from .models import Health, Outcome, Result

if TYPE_CHECKING:
    from . import _native


class TrainingKind(str, Enum):
    SKILL = "skill_rank"
    TALENT = "talent"
    EXPERTISE = "expertise"


@dataclass(frozen=True, slots=True)
class TrainingOverview:
    locked: bool
    unspent: int
    starting_skills: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class PointPool:
    hit_die: int
    identity: str
    name: str
    available: int
    allowed: tuple[TrainingKind, ...]
    _basis: tuple = field(repr=False)


@dataclass(frozen=True, slots=True)
class TrainingOption:
    kind: TrainingKind
    identity: str
    name: str
    forms: tuple[str, ...]
    _basis: tuple = field(repr=False)


@dataclass(frozen=True, slots=True)
class PurchasePreview:
    pool: PointPool
    option: TrainingOption
    form: str | None
    resulting_health: Health
    would_die: bool


class TrainingRefused(RuntimeError):
    def __init__(self, result: Result):
        super().__init__(result.reason or result.status.value)
        self.result = result


def _basis(page: dict) -> tuple:
    basis = page["basis"]
    return (
        basis["actor"],
        basis["branch"],
        basis["stream"],
        int(basis["reset"]),
        int(basis["build_epoch"]),
    )


class Training:
    """Browse one player's build and confirm a specific preview at most once."""

    def __init__(self, connection: _native.Session):
        self._connection = connection
        self._preview: PurchasePreview | None = None
        self._opened = False

    def _page(self, command: str) -> dict:
        self._preview = None
        reply = self._connection.advancement(command)
        result = Result.from_reply(reply)
        if result.status is not Outcome.CONFIRMED:
            raise TrainingRefused(result)
        page = reply.get("page")
        if page is None:
            raise ValueError("Confirmed training request returned no page")
        return page

    def overview(self) -> TrainingOverview:
        page = self._page("overview")
        self._opened = True
        row = page["entries"][0]["value"]
        return TrainingOverview(
            row["locked"],
            int(row["points"]["unspent"]),
            tuple(skill["id"] for skill in row["starting_skills"]),
        )

    def _rows(self, section: str) -> Iterator[tuple[tuple, dict]]:
        if not self._opened:
            self.overview()
        page = self._page(section)
        basis = _basis(page)
        while True:
            if _basis(page) != basis:
                raise TrainingRefused(Result(Outcome.STALE, None, "Build changed during browsing"))
            for row in page["entries"]:
                yield basis, row["value"]
            if page["next"] is None:
                return
            page = self._page("next")

    def pools(self) -> tuple[PointPool, ...]:
        return tuple(
            PointPool(
                int(row["owner"]) + 1,
                row["pool"]["id"],
                row["pool"]["name"],
                row["unspent"],
                tuple(TrainingKind(k) for k in row["allowed"]),
                basis,
            )
            for basis, row in self._rows("balances")
        )

    def options(self, kind: TrainingKind) -> tuple[TrainingOption, ...]:
        section = {
            TrainingKind.SKILL: "skills",
            TrainingKind.TALENT: "talents",
            TrainingKind.EXPERTISE: "expertises",
        }[kind]
        field_name = {
            TrainingKind.SKILL: "skill",
            TrainingKind.TALENT: "talent",
            TrainingKind.EXPERTISE: "expertise",
        }[kind]
        return tuple(
            TrainingOption(
                kind,
                row["selection"][field_name],
                row["name"],
                tuple(form["id"] for form in row["form_choices"]),
                basis,
            )
            for basis, row in self._rows(section)
        )

    def preview(
        self, pool: PointPool, option: TrainingOption, *, form: str | None = None
    ) -> PurchasePreview:
        if pool._basis != option._basis:
            raise TrainingRefused(Result(Outcome.STALE, None, "Choose from the same build"))
        if option.kind not in pool.allowed or pool.available == 0:
            raise ValueError("Choose an available point pool for this purchase")
        if (form is None and option.forms) or (form is not None and form not in option.forms):
            raise ValueError("Choose a form offered for this purchase")
        verb = "skill" if option.kind is TrainingKind.SKILL else option.kind.value
        command = f"purchase {pool.hit_die} {pool.identity} {verb} {option.identity}"
        page = self._page(command if form is None else f"{command} form {form}")
        if _basis(page) != pool._basis:
            raise TrainingRefused(Result(Outcome.STALE, None, "Build changed before preview"))
        health = page["entries"][0]["value"]["health"]
        self._preview = PurchasePreview(
            pool, option, form, Health(int(health["current"]), int(health["maximum"])), health["dead"]
        )
        return self._preview

    def confirm(self, preview: PurchasePreview) -> Result:
        if preview is not self._preview or self._preview is None:
            return Result(Outcome.STALE, None, "Preview this purchase again before confirming")
        self._preview = None
        self._opened = False
        return Result.from_reply(self._connection.advancement("confirm"))
