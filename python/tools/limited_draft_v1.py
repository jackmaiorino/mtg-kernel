"""Trusted-host draft of supplied packs, with separate private actor views.

No packs are generated. Rounds and initial seat order come from the caller;
round zero passes to seat+1, then direction alternates. A pass happens only
after every seat has picked. The host record contains hidden inputs and must
never be sent to a drafter. A completed pool is an ordinary ImportedDeck.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass
import hashlib
import json
from typing import Any
import uuid

import fdn_booster_pool_v1 as target
import limited_decks_v1 as limited


@dataclass(frozen=True)
class _DraftCard:
    # (round, original pack seat, copy ordinal within that supplied pack).
    copy_id: tuple[int, int, int]
    entry: limited.DeckEntry


@dataclass(frozen=True)
class DraftPrompt:
    session_id: str
    generation: int
    seat: int
    round: int
    pick: int


@dataclass(frozen=True)
class DraftView:
    seat: int
    complete: bool
    prompt: DraftPrompt | None
    pack: tuple[limited.DeckEntry, ...]
    picked: tuple[limited.DeckEntry, ...]


def _integer(value: Any, label: str) -> int:
    if type(value) is not int or value < 0:
        raise ValueError(f"{label}: expected a nonnegative integer")
    return value


def _object(value: Any, keys: set[str], label: str) -> dict:
    if type(value) is not dict or set(value) != keys:
        raise ValueError(f"{label}: invalid record fields")
    return value


class DraftSession:
    def __init__(self, packs: tuple[tuple[limited.ImportedDeck, ...], ...], *,
                 seed: int | None = None, provenance: str | None = None):
        if seed is not None and type(seed) is not int:
            raise ValueError("seed must be an integer or None; it does not generate packs")
        if provenance is not None and type(provenance) is not str:
            raise ValueError("provenance must be a string or None")
        if not isinstance(packs, (tuple, list)) or not packs:
            raise ValueError("expected at least one supplied round")
        if not isinstance(packs[0], (tuple, list)) or len(packs[0]) < 2:
            raise ValueError("a draft requires at least two seats")
        self._seat_count = len(packs[0])
        document = limited.load_json(target.TARGET.read_bytes())
        target.validate(document)
        names = set(document["card_names"])
        supplied = []
        for round_packs in packs:
            if not isinstance(round_packs, (tuple, list)) or len(round_packs) != self.seat_count:
                raise ValueError("every round must supply exactly one pack per seat")
            normalized = tuple(self._pack(pack, names) for pack in round_packs)
            sizes = [sum(entry.count for entry in (*pack.mainboard, *pack.sideboard)) for pack in normalized]
            if not sizes[0] or len(set(sizes)) != 1:
                raise ValueError("packs must have the same positive pickable-card count within each round")
            supplied.append(normalized)
        self._supplied = tuple(supplied)
        self._seed = seed
        self._provenance = provenance
        self._input_sha256 = hashlib.sha256(json.dumps(
            {"packs": self._pack_records(), "seed": seed, "provenance": provenance},
            ensure_ascii=False, sort_keys=True, separators=(",", ":"),
        ).encode()).hexdigest()
        self._session_id = uuid.uuid4().hex
        self._round = 0
        self._pick = 0
        self._generation = 0
        self._packs = self._open_round(0)
        self._picked: list[tuple[_DraftCard, ...]] = [() for _ in range(self.seat_count)]
        self._answered: set[int] = set()
        self._picks: list[dict] = []

    @staticmethod
    def _pack(pack: limited.ImportedDeck, names: set[str]) -> limited.ImportedDeck:
        if not isinstance(pack, limited.ImportedDeck):
            raise ValueError("supplied packs must be ImportedDeck values")
        if pack.name is not None and type(pack.name) is not str:
            raise ValueError("pack name must be a string or None")
        if not isinstance(pack.mainboard, (tuple, list)) or not isinstance(pack.sideboard, (tuple, list)):
            raise ValueError("pack zones must be row sequences")
        for entry in (*pack.mainboard, *pack.sideboard):
            if not isinstance(entry, limited.DeckEntry) or type(entry.count) is not int or entry.count <= 0:
                raise ValueError("pack rows require positive integer DeckEntry counts")
            if type(entry.name) is not str or entry.name not in names:
                raise ValueError("pack card is outside the frozen FDN names; unknown cards and tokens are refused")
            if entry.printing is not None and (type(entry.printing) is not str or not entry.printing.strip()):
                raise ValueError("invalid pack printing annotation")
        return limited.ImportedDeck(pack.name, tuple(pack.mainboard), tuple(pack.sideboard))

    def _open_round(self, round_index: int) -> tuple[tuple[_DraftCard, ...], ...]:
        result = []
        for seat, pack in enumerate(self._supplied[round_index]):
            copies = [limited.DeckEntry(1, entry.name, entry.printing)
                      for entry in (*pack.mainboard, *pack.sideboard) for _ in range(entry.count)]
            result.append(tuple(_DraftCard((round_index, seat, i), entry) for i, entry in enumerate(copies)))
        return tuple(result)

    @property
    def seat_count(self) -> int:
        return self._seat_count

    @property
    def complete(self) -> bool:
        return self._round == len(self._supplied)

    def _seat(self, seat: int) -> int:
        if _integer(seat, "seat") >= self.seat_count:
            raise ValueError("seat is outside this draft")
        return seat

    def view(self, seat: int) -> DraftView:
        """Route only this immutable value to the authenticated seat."""
        seat = self._seat(seat)
        prompt = None
        if not self.complete and seat not in self._answered:
            prompt = DraftPrompt(self._session_id, self._generation, seat, self._round, self._pick)
        # Original physical ordinals can disclose unseen initial pack order.
        # Current tuple indexes bind choices; actors need only visible cards.
        pack = () if self.complete else tuple(card.entry for card in self._packs[seat])
        picked = tuple(card.entry for card in self._picked[seat])
        return DraftView(seat, self.complete, prompt, pack, picked)

    def pick(self, seat: int, prompt: DraftPrompt, index: int) -> DraftView:
        """Reject stale/foreign/duplicate/index-invalid input before any change."""
        seat = self._seat(seat)
        if (type(prompt) is not DraftPrompt or type(prompt.session_id) is not str
                or any(type(value) is not int for value in (prompt.generation, prompt.seat, prompt.round, prompt.pick))
                or self.view(seat).prompt is None or prompt != self.view(seat).prompt):
            raise ValueError("stale, foreign or already answered draft prompt")
        index = _integer(index, "pick index")
        if index >= len(self._packs[seat]):
            raise ValueError("pick index is outside the issued pack")
        card = self._packs[seat][index]
        remaining = self._packs[seat][:index] + self._packs[seat][index + 1:]
        packs = list(self._packs)
        packs[seat] = remaining
        self._packs = tuple(packs)
        self._picked[seat] += (card,)
        self._answered.add(seat)
        self._picks.append({"round": self._round, "pick": self._pick, "seat": seat,
                            "index": index, "copy_id": list(card.copy_id)})
        if len(self._answered) == self.seat_count:
            self._generation += 1
            self._answered.clear()
            if not self._packs[0]:
                self._round += 1
                self._pick = 0
                self._packs = () if self.complete else self._open_round(self._round)
            else:
                direction = 1 if self._round % 2 == 0 else -1
                self._packs = tuple(self._packs[(receiver - direction) % self.seat_count]
                                    for receiver in range(self.seat_count))
                self._pick += 1
        return self.view(seat)

    def acquired_pool(self, seat: int) -> limited.ImportedDeck:
        """Completed picks in order, directly usable by limited_pool_v1."""
        seat = self._seat(seat)
        if not self.complete:
            raise ValueError("draft must complete before exporting an acquired pool")
        return limited.ImportedDeck(f"Draft seat {seat}", tuple(card.entry for card in self._picked[seat]), ())

    def _pack_records(self) -> list:
        return [[asdict(pack) for pack in round_packs] for round_packs in self._supplied]

    def record(self) -> dict:
        """Trusted-host checkpoint/replay data, including all hidden packs."""
        # JSON roundtrip produces detached, JSON-native containers throughout.
        return json.loads(json.dumps({
            "schema": "kernel_limited_supplied_draft/v1", "packs": self._pack_records(),
            "seed": self._seed, "provenance": self._provenance, "input_sha256": self._input_sha256,
            "picks": self._picks,
        }))

    @classmethod
    def replay(cls, record: dict) -> DraftSession:
        """Reissue fresh session prompts and replay exact original copy choices."""
        record = _object(record, {"schema", "packs", "seed", "provenance", "input_sha256", "picks"}, "draft")
        if record["schema"] != "kernel_limited_supplied_draft/v1" or type(record["packs"]) is not list:
            raise ValueError("invalid supplied-draft record")
        supplied = []
        for round_packs in record["packs"]:
            if type(round_packs) is not list:
                raise ValueError("record round must be a pack array")
            packs = []
            for pack in round_packs:
                pack = _object(pack, {"name", "mainboard", "sideboard"}, "pack")
                zones = []
                for zone in ("mainboard", "sideboard"):
                    if type(pack[zone]) is not list:
                        raise ValueError("record pack zone must be a row array")
                    zones.append(tuple(limited.DeckEntry(**_object(row, {"count", "name", "printing"}, "pack row"))
                                       for row in pack[zone]))
                packs.append(limited.ImportedDeck(pack["name"], *zones))
            supplied.append(tuple(packs))
        session = cls(tuple(supplied), seed=record["seed"], provenance=record["provenance"])
        if type(record["input_sha256"]) is not str or record["input_sha256"] != session._input_sha256:
            raise ValueError("supplied-draft input identity changed")
        if type(record["picks"]) is not list:
            raise ValueError("record picks must be an array")
        for pick in record["picks"]:
            pick = _object(pick, {"round", "pick", "seat", "index", "copy_id"}, "pick")
            for field in ("round", "pick", "seat", "index"):
                _integer(pick[field], "record " + field)
            view = session.view(pick["seat"])
            if view.prompt is None or (pick["round"], pick["pick"]) != (view.prompt.round, view.prompt.pick):
                raise ValueError("record pick is outside its original draft step")
            copy_id = pick["copy_id"]
            if type(copy_id) is not list or len(copy_id) != 3 or any(type(value) is not int for value in copy_id):
                raise ValueError("invalid recorded physical copy identity")
            physical_pack = session._packs[pick["seat"]]
            if pick["index"] >= len(physical_pack) or tuple(copy_id) != physical_pack[pick["index"]].copy_id:
                raise ValueError("record index and physical copy disagree")
            session.pick(pick["seat"], view.prompt, pick["index"])
        return session
