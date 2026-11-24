"""Readers of the cube M4 reads, on frozen pod records (mtg-kernel-cube-pod-record/v1).

D7:  read (b)  candidate-built decks of learned drafters against A1.5's dealt opponent. A verdict.
     read (c)  per pool, candidate build minus stated build against C(P). Non-inferiority. A verdict.
     read (b') the stated build of each learned pool against C(P), from (c)'s games. No verdict.
D7a: read (a)  a 2 by 2 of picks (learned or uniform) by builder (candidate or stated), on A4.6's
               role-swapped pairs. The verdict is the candidate-builder contrast; the stated-builder
               contrast and the difference of the two are reported. Also the early pod's read (a).
     dose      current builder minus the run's initial builder, against C(P). Report only.
     aa        read (a)'s A/A: exact symmetry of identical policies on role-swapped pairs.

Every constant that A6 fixes (levels, thresholds, margins, sizing inputs and caps, extension size,
resample count, namespaces) comes from a declaration file the reader takes as input
(`mtg-kernel-cube-read-declaration/v1`), validated strictly, with no defaults.

Statistics:
- Unit: the draft for (b), (b'), (c) and dose reads; the pair for (a) and its A/A.
- Bootstrap: percentile, resampling units with replacement. Root seed
  seed([namespace, "bootstrap", read, look]) [proposed for A6]; the t-th unit index of resample b is
  seed([root, b, t]) mod n (deal.seed, pure Python). One resampling serves every statistic and every
  halt scoring of a report. Percentiles by linear interpolation at position p (B - 1) on the sorted
  resample statistics, as B.4 does.
- Sums are exactly rounded (math.fsum), so a sum depends on its terms only, on every platform and
  Python version.
- Halts (A2.4): a read never scores a halted game. Every read reports three scorings: `drop`
  (halted games left out), `halts_against` (each halted game imputed to lower the statistic) and
  `halts_for` (each imputed to raise it). The imputations are signed per statistic, following the
  sign of the contrast (design note 9h). A verdict read is void if the imputation against its
  verdict could flip it.
- Looks: at most two per verdict read, at the declared level each. Look 1 reads the first n units;
  look 2 reads the pooled units of both looks, the first n + extension_factor x n.
- Verdict word, exactly one of: `win` (proceed), `loss` (the attempt ends), `extend` (inconclusive
  at look 1: one read-only extension), `not_won` (inconclusive at look 2: the attempt ends), `void`.
"""

from __future__ import annotations

import argparse
from decimal import Decimal, ROUND_CEILING, localcontext
import hashlib
import json
import math
from pathlib import Path
import sys
from typing import Any, Callable

sys.path.insert(0, str(Path(__file__).resolve().parent))

import deal  # noqa: E402
import pod_record  # noqa: E402
import read_plans  # noqa: E402

DECLARATION_SCHEMA = "mtg-kernel-cube-read-declaration/v1"
REPORT_SCHEMA = "mtg-kernel-cube-read-report/v1"
TEST_NAMESPACE = "test"
A_SCALE = "learned_minus_uniform"

VERDICT_READS = ("a", "a_early", "b", "c")
READS = ("a", "a_early", "b", "b_prime", "c", "dose", "aa")
SCORINGS = ("drop", "halts_against", "halts_for")

DECL_KEYS = {"schema", "interfaces", "a6_sha256", "draw_score", "bootstrap", "reads"}
BOOTSTRAP_KEYS = {"resamples"}
SECTION_KEYS = {"a", "a_early", "b", "c", "dose", "aa"}
VERDICT_KEYS = {"namespace", "run", "levels", "win_if_lower_above", "loss_if_upper_below", "extension_factor",
                "sizing"}
A_KEYS = VERDICT_KEYS | {"scale"}
SIZING_KEYS = {"spread", "effect", "z_level", "z_power", "floor", "cap"}
DOSE_KEYS = {"namespace", "run", "panel_drafts", "level"}
AA_KEYS = {"namespace", "run", "pairs", "level"}

MAX_LOOKS = 2


class DeclarationError(ValueError):
    """A declaration file that does not follow mtg-kernel-cube-read-declaration/v1."""


class ReadError(ValueError):
    """Inputs a reader refuses (wrong records, a third look, an unsigned declaration on read data)."""


# ---------------------------------------------------------------------------------------------
# The declaration


def _dfail(path: str, message: str) -> None:
    raise DeclarationError(f"declaration {path}: {message}")


def _keys(value: Any, path: str, keys: set[str]) -> dict[str, Any]:
    if not isinstance(value, dict):
        _dfail(path, "must be an object")
    actual = set(value.keys())
    if actual != keys:
        _dfail(path, f"keys differ: missing {sorted(keys - actual)}, extra {sorted(actual - keys)}")
    return value


def _number(value: Any, path: str) -> Decimal:
    if type(value) is int:
        return Decimal(value)
    if isinstance(value, Decimal) and value.is_finite():
        return value
    _dfail(path, f"must be a number, got {value!r}")
    raise AssertionError  # unreachable


def _int(value: Any, path: str, minimum: int) -> int:
    if not (type(value) is int and value >= minimum):
        _dfail(path, f"must be an integer >= {minimum}, got {value!r}")
    return value


def _level(value: Any, path: str) -> Decimal:
    level = _number(value, path)
    if not Decimal(0) < level < Decimal("0.5"):
        _dfail(path, f"a one-sided level lies in (0, 0.5), got {level}")
    return level


def _name(value: Any, path: str) -> str:
    if not (isinstance(value, str) and pod_record.NAME_RE.match(value)):
        _dfail(path, f"must match [a-z0-9-]+, got {value!r}")
    return value


def _validate_sizing(value: Any, path: str) -> dict[str, Any]:
    sizing = _keys(value, path, SIZING_KEYS)
    out: dict[str, Any] = {}
    if sizing["spread"] is None:
        out["spread"] = None
    else:
        out["spread"] = _number(sizing["spread"], f"{path}.spread")
        if out["spread"] <= 0:
            _dfail(f"{path}.spread", "must be positive or null (not yet recorded)")
    out["effect"] = _number(sizing["effect"], f"{path}.effect")
    if out["effect"] <= 0:
        _dfail(f"{path}.effect", "must be positive")
    out["z_level"] = _number(sizing["z_level"], f"{path}.z_level")
    out["z_power"] = _number(sizing["z_power"], f"{path}.z_power")
    if out["z_level"] <= 0 or out["z_power"] < 0:
        _dfail(path, "z_level must be positive and z_power non-negative")
    out["floor"] = _int(sizing["floor"], f"{path}.floor", 1)
    out["cap"] = _int(sizing["cap"], f"{path}.cap", out["floor"])
    return out


def _validate_verdict_section(value: Any, path: str, keys: set[str]) -> dict[str, Any]:
    section = _keys(value, path, keys)
    out: dict[str, Any] = {
        "namespace": _name(section["namespace"], f"{path}.namespace"),
        "run": _name(section["run"], f"{path}.run"),
    }
    levels = section["levels"]
    if not (isinstance(levels, list) and len(levels) == MAX_LOOKS):
        _dfail(f"{path}.levels", f"must list one level per look, {MAX_LOOKS} looks")
    out["levels"] = [_level(lv, f"{path}.levels[{i}]") for i, lv in enumerate(levels)]
    out["win_if_lower_above"] = _number(section["win_if_lower_above"], f"{path}.win_if_lower_above")
    out["loss_if_upper_below"] = _number(section["loss_if_upper_below"], f"{path}.loss_if_upper_below")
    if out["loss_if_upper_below"] > out["win_if_lower_above"]:
        _dfail(path, "loss_if_upper_below must not exceed win_if_lower_above")
    out["extension_factor"] = _int(section["extension_factor"], f"{path}.extension_factor", 1)
    out["sizing"] = _validate_sizing(section["sizing"], f"{path}.sizing")
    if "scale" in keys:
        if section["scale"] != A_SCALE:
            _dfail(f"{path}.scale", f"read (a)'s one scale is {A_SCALE!r}, got {section['scale']!r}")
        out["scale"] = A_SCALE
    return out


def validate_declaration(doc: Any) -> dict[str, Any]:
    """Validate a parsed declaration (numbers parsed as Decimal) and return its normalised form."""
    top = _keys(doc, "", DECL_KEYS)
    if top["schema"] != DECLARATION_SCHEMA:
        _dfail("schema", f"unknown version {top['schema']!r}")
    if top["interfaces"] != pod_record.INTERFACES_VERSION:
        _dfail("interfaces", f"unknown version {top['interfaces']!r}")
    a6 = top["a6_sha256"]
    if a6 is not None and not (isinstance(a6, str) and pod_record.SHA256_RE.match(a6)):
        _dfail("a6_sha256", "must be the signed A6's sha256 or null")
    if _number(top["draw_score"], "draw_score") != Decimal("0.5"):
        _dfail("draw_score", "the frozen interface scores a natural draw 0.5")
    bootstrap = _keys(top["bootstrap"], "bootstrap", BOOTSTRAP_KEYS)
    resamples = _int(bootstrap["resamples"], "bootstrap.resamples", 1)
    reads = _keys(top["reads"], "reads", SECTION_KEYS)
    sections: dict[str, Any] = {
        "a": _validate_verdict_section(reads["a"], "reads.a", A_KEYS),
        "a_early": _validate_verdict_section(reads["a_early"], "reads.a_early", A_KEYS),
        "b": _validate_verdict_section(reads["b"], "reads.b", VERDICT_KEYS),
        "c": _validate_verdict_section(reads["c"], "reads.c", VERDICT_KEYS),
    }
    dose = _keys(reads["dose"], "reads.dose", DOSE_KEYS)
    sections["dose"] = {
        "namespace": _name(dose["namespace"], "reads.dose.namespace"),
        "run": _name(dose["run"], "reads.dose.run"),
        "panel_drafts": _int(dose["panel_drafts"], "reads.dose.panel_drafts", 1),
        "level": _level(dose["level"], "reads.dose.level"),
    }
    aa = _keys(reads["aa"], "reads.aa", AA_KEYS)
    sections["aa"] = {
        "namespace": _name(aa["namespace"], "reads.aa.namespace"),
        "run": _name(aa["run"], "reads.aa.run"),
        "pairs": _int(aa["pairs"], "reads.aa.pairs", 1),
        "level": _level(aa["level"], "reads.aa.level"),
    }
    # Namespaces are single-use (A4.8): (b) and (c) share the read namespace by design (9c); every
    # other read has its own. The fixture namespace `test` is exempt.
    owners: dict[str, str] = {}
    for read in ("a", "a_early", "b", "dose", "aa"):
        ns = sections[read]["namespace"]
        if ns != TEST_NAMESPACE and ns in owners:
            _dfail(f"reads.{read}.namespace", f"{ns!r} is already read {owners[ns]}'s")
        owners[ns] = read
    ns_c = sections["c"]["namespace"]
    if ns_c != TEST_NAMESPACE and ns_c != sections["b"]["namespace"] and ns_c in owners:
        _dfail("reads.c.namespace", "(c) shares (b)'s namespace or has its own")
    return {"a6_sha256": a6, "resamples": resamples, "reads": sections}


def load_declaration(path: str | Path) -> tuple[dict[str, Any], str]:
    data = Path(path).read_bytes()
    try:
        doc = json.loads(data.decode("utf-8"), parse_float=Decimal)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise DeclarationError(f"declaration is not UTF-8 JSON: {exc}") from exc
    return validate_declaration(doc), hashlib.sha256(data).hexdigest()


# ---------------------------------------------------------------------------------------------
# Sizing


def first_look_units(sizing: dict[str, Any]) -> int:
    """n = ceil(((z_level + z_power) s / effect)^2), at least the floor and at most the cap.
    Exact decimal arithmetic, so the rounding never depends on binary floating point."""
    if sizing["spread"] is None:
        raise ReadError("the read's spread is not recorded yet (it comes from D14's sizing run)")
    with localcontext() as ctx:
        ctx.prec = 60
        raw = ((sizing["z_level"] + sizing["z_power"]) * sizing["spread"] / sizing["effect"]) ** 2
        n = int(raw.to_integral_value(rounding=ROUND_CEILING))
    return min(sizing["cap"], max(sizing["floor"], n))


def look_units(section: dict[str, Any], look: int) -> int:
    """Units a verdict read's look reads: n at look 1, n + extension_factor x n at look 2."""
    if not (type(look) is int and 1 <= look <= MAX_LOOKS):
        raise ReadError(f"a read has at most {MAX_LOOKS} looks; look {look!r} is refused")
    n = first_look_units(section["sizing"])
    return n if look == 1 else n + section["extension_factor"] * n


# ---------------------------------------------------------------------------------------------
# Bootstrap


def bootstrap_root(namespace: str, read: str, look: int) -> int:
    return deal.seed([namespace, "bootstrap", read, look])


def resample_indices(root: int, resample: int, n: int) -> list[int]:
    """seed([root, resample, t]) mod n for t = 0 to n - 1. The compact JSON of three integers is
    "[root,resample,t]", so the digest is taken incrementally; test_readers checks it equals
    deal.seed."""
    base = hashlib.sha256(f"[{root},{resample},".encode("ascii"))
    out = []
    for t in range(n):
        h = base.copy()
        h.update(f"{t}]".encode("ascii"))
        out.append((int.from_bytes(h.digest()[:8], "little") & deal.MASK) % n)
    return out


def _mean(values: list[float | None]) -> float | None:
    scored = [v for v in values if v is not None]
    return math.fsum(scored) / len(scored) if scored else None


def _spread(values: list[float | None]) -> float | None:
    scored = [v for v in values if v is not None]
    if len(scored) < 2:
        return None
    m = math.fsum(scored) / len(scored)
    return math.sqrt(math.fsum((v - m) ** 2 for v in scored) / (len(scored) - 1))


def percentile(sorted_values: list[float], p: float) -> float:
    """Linear interpolation at position p (B - 1) of B sorted values (B.4)."""
    position = p * (len(sorted_values) - 1)
    low = math.floor(position)
    high = min(low + 1, len(sorted_values) - 1)
    frac = position - low
    return sorted_values[low] + (sorted_values[high] - sorted_values[low]) * frac


def bootstrap_many(series: dict[Any, list[float | None]], root: int, resamples: int, level: float
                   ) -> dict[Any, dict[str, Any]]:
    """One shared resampling of n units for several unit-statistic series of length n. Returns, per
    series, the estimate, the interval at (level, 1 - level) and the unit count and spread. A series
    whose resample has no scored unit gets interval None (undefined)."""
    n = len(next(iter(series.values())))
    if any(len(v) != n for v in series.values()):
        raise ReadError("statistic series of unequal length")
    if n == 0:
        raise ReadError("no units to read")
    dense = {key: all(v is not None for v in values) for key, values in series.items()}
    draws: dict[Any, list[float] | None] = {key: [] for key in series}
    for b in range(resamples):
        idx = resample_indices(root, b, n)
        for key, values in series.items():
            acc = draws[key]
            if acc is None:
                continue
            if dense[key]:
                acc.append(math.fsum(map(values.__getitem__, idx)) / n)
                continue
            picked = [values[i] for i in idx if values[i] is not None]
            if not picked:
                draws[key] = None
                continue
            acc.append(math.fsum(picked) / len(picked))
    out: dict[Any, dict[str, Any]] = {}
    for key, values in series.items():
        acc = draws[key]
        interval = None
        if acc is not None:
            acc.sort()
            interval = [percentile(acc, level), percentile(acc, 1.0 - level)]
        out[key] = {
            "estimate": _mean(values),
            "interval": interval,
            "units_scored": sum(1 for v in values if v is not None),
            "unit_spread": _spread(values),
        }
    return out


def bootstrap_interval(values: list[float], root: int, resamples: int, level: float) -> list[float] | None:
    """The percentile interval of the mean of `values` (one series, no halts)."""
    return bootstrap_many({"x": list(values)}, root, resamples, level)["x"]["interval"]


# ---------------------------------------------------------------------------------------------
# Units: per-game outcomes taken from validated records


def _impute(wf: float | None, scoring: str, against_value: float) -> float | None:
    """A halted game (wf None) under a scoring: dropped, or imputed to `against_value` (lowering
    the statistic) or its complement 1 - against_value (raising it)."""
    if wf is not None:
        return wf
    if scoring == "drop":
        return None
    if scoring == "halts_against":
        return against_value
    return 1.0 - against_value


def _arm_mean(games: list[float | None], scoring: str, against_value: float) -> float | None:
    return _mean([_impute(wf, scoring, against_value) for wf in games])


def _read_b_unit(record: dict[str, Any]) -> list[list[float | None]]:
    """Read (b): per candidate deck (the built deck of each read_b game), its game outcomes."""
    games_by_deck: dict[int, list[float | None]] = {}
    for game in record["games"]:
        if game["role"] != "read_b":
            raise ReadError(f"read (b) record holds a {game['role']!r} game")
        built = [d for d in game["decks"] if record["decks"][d]["source"] == "build"]
        if len(built) != 1:
            raise ReadError(f"read (b) game {game['game']} must hold exactly one built deck")
        games_by_deck.setdefault(built[0], []).append(pod_record.win_fraction(game, built[0]))
    if len(games_by_deck) != pod_record.SEATS:
        raise ReadError(f"read (b) record has {len(games_by_deck)} candidate decks, not {pod_record.SEATS}")
    for deck_id, outcomes in games_by_deck.items():
        if len(outcomes) != deal.READ_B_GAMES:
            raise ReadError(f"read (b) deck {deck_id} has {len(outcomes)} games, not {deal.READ_B_GAMES}")
    return [games_by_deck[d] for d in sorted(games_by_deck)]


def _paired_unit(record: dict[str, Any], kind: str) -> list[tuple[list[float | None], list[float | None]]]:
    """Paired reads: per pool, (x outcomes, y outcomes) on matched games."""
    pools = []
    by_id = {g["game"]: g for g in record["games"]}
    entries = record["paired_builds"]
    if len(entries) != pod_record.SEATS or any(e["kind"] != kind for e in entries):
        raise ReadError(f"a {kind} record holds {pod_record.SEATS} {kind} comparisons, one per seat")
    for entry in entries:
        xs = [pod_record.win_fraction(by_id[g], entry["deck_x"]) for g in entry["games_x"]]
        ys = [pod_record.win_fraction(by_id[g], entry["deck_y"]) for g in entry["games_y"]]
        pools.append((xs, ys))
    return pools


def _stat_b(unit: list[list[float | None]], scoring: str) -> float | None:
    return _mean([_arm_mean(games, scoring, 0.0) for games in unit])


def _stat_paired_difference(unit: list[tuple[list, list]], scoring: str) -> float | None:
    diffs = []
    for xs, ys in unit:
        mx = _arm_mean(xs, scoring, 0.0)  # halts against x: x loses
        my = _arm_mean(ys, scoring, 1.0)  # halts against the difference: y wins
        diffs.append(None if mx is None or my is None else mx - my)
    return _mean(diffs)


def _stat_paired_y(unit: list[tuple[list, list]], scoring: str) -> float | None:
    return _mean([_arm_mean(ys, scoring, 0.0) for _, ys in unit])


def _learning_chairs(record: dict[str, Any]) -> tuple[int, ...]:
    return tuple(c["chair"] for c in record["chairs"] if c["role"] == "learning")


EVEN = (0, 2, 4, 6)
ODD = (1, 3, 5, 7)


def _read_a_pod(record: dict[str, Any]) -> dict[str, dict[int, tuple[dict[str, Any], int]]]:
    """A read (a) pod: per builder arm, per pairing, the cross game and its learned deck."""
    learning = _learning_chairs(record)
    if learning not in (EVEN, ODD):
        raise ReadError("a read (a) pod has its 4 learning chairs in the even or in the odd chairs")
    if any(c["role"] != "fixture" for c in record["chairs"] if c["chair"] not in learning):
        raise ReadError("a read (a) pod's other 4 chairs have role 'fixture'")
    arms: dict[str, dict[int, tuple[dict[str, Any], int]]] = {"candidate": {}, "stated": {}}
    per_chair: dict[tuple[int, str], int] = {}
    for deck in record["decks"]:
        if deck["source"] != "build":
            raise ReadError("a read (a) pod holds built decks only")
        arm = "stated" if deck["builder"]["kind"] == "stated" else "candidate"
        key = (deck["chair"], arm)
        if key in per_chair:
            raise ReadError(f"chair {deck['chair']} has two {arm} builds")
        per_chair[key] = deck["deck"]
    if len(per_chair) != 2 * pod_record.SEATS:
        raise ReadError("every chair of a read (a) pod is built once by each builder")
    for game in record["games"]:
        if game["role"] != "cross" or game["pairing"] is None:
            raise ReadError(f"read (a) game {game['game']} is not a cross game with a pairing")
        deck_arms = {"stated" if record["decks"][d]["builder"]["kind"] == "stated" else "candidate"
                     for d in game["decks"]}
        if len(deck_arms) != 1:
            raise ReadError(f"read (a) game {game['game']} mixes builders")
        arm = deck_arms.pop()
        learned = [d for d, c in zip(game["decks"], game["chairs"]) if c in learning]
        if len(learned) != 1:
            raise ReadError(f"read (a) game {game['game']} is not learned seat against uniform seat")
        if game["pairing"] in arms[arm]:
            raise ReadError(f"read (a) pairing {game['pairing']} is played twice under one builder")
        arms[arm][game["pairing"]] = (game, learned[0])
    for arm, games in arms.items():
        if len(games) != 16:
            raise ReadError(f"a read (a) pod plays the 16 cross games under each builder; {arm} has {len(games)}")
    return arms


def _read_a_pair(pods: list[dict[str, Any]]) -> tuple[dict[str, list[float | None]], list[dict[str, Any]]]:
    """A pair's learned-seat outcomes per builder arm, and its two pods ordered (even, odd)."""
    if len(pods) != 2:
        raise ReadError("a read (a) pair is two pods")
    ordered = sorted(pods, key=lambda r: _learning_chairs(r))
    if _learning_chairs(ordered[0]) != EVEN or _learning_chairs(ordered[1]) != ODD:
        raise ReadError("a read (a) pair puts the learned seats in the even chairs, then in the odd chairs")
    if ordered[0]["seeds"] != ordered[1]["seeds"]:
        raise ReadError("the two pods of a read (a) pair share their pack and pod seeds")
    outcomes: dict[str, list[float | None]] = {"candidate": [], "stated": []}
    for record in ordered:
        arms = _read_a_pod(record)
        for arm in ("candidate", "stated"):
            for pairing in sorted(arms[arm]):
                game, learned = arms[arm][pairing]
                outcomes[arm].append(pod_record.win_fraction(game, learned))
    return outcomes, ordered


def _contrast(games: list[float | None], scoring: str, against_value: float) -> float | None:
    """Learned minus uniform win fraction: 2 wf - 1 per cross game (a draw is 0)."""
    m = _arm_mean(games, scoring, against_value)
    return None if m is None else 2.0 * m - 1.0


def _stat_a_candidate(unit: dict[str, list], scoring: str) -> float | None:
    return _contrast(unit["candidate"], scoring, 0.0)


def _stat_a_stated(unit: dict[str, list], scoring: str) -> float | None:
    return _contrast(unit["stated"], scoring, 0.0)


def _stat_a_difference(unit: dict[str, list], scoring: str) -> float | None:
    # Lower bound (9h): halted candidate-builder games a loss for the learned seat, halted
    # stated-builder games a win for it; the upper bound the reverse.
    cand = _contrast(unit["candidate"], scoring, 0.0)
    stated = _contrast(unit["stated"], scoring, 1.0)
    return None if cand is None or stated is None else cand - stated


# ---------------------------------------------------------------------------------------------
# Collecting a read's records


def _collect(records: list[dict[str, Any]], use: str, namespace: str, run: str, count: int,
             per_unit: int) -> dict[int, list[dict[str, Any]]]:
    """Group records by pod_index; exactly `per_unit` records for each index 0 to count - 1."""
    units: dict[int, list[dict[str, Any]]] = {}
    for record in records:
        where = f"record {record['namespace']}/{record['run']}/{record['pod_index']}"
        if record["use"] != use:
            raise ReadError(f"{where}: use {record['use']!r}, this read reads {use!r}")
        if record["namespace"] != namespace or record["run"] != run:
            raise ReadError(f"{where}: not in the declared namespace {namespace!r} and run {run!r}")
        if record["status"] != "complete":
            raise ReadError(f"{where}: status {record['status']!r}; a read pod records halted games and completes")
        if record["pod_index"] >= count:
            raise ReadError(f"{where}: unit {record['pod_index']} is beyond this look's {count} units")
        units.setdefault(record["pod_index"], []).append(record)
    missing = [i for i in range(count) if len(units.get(i, [])) != per_unit]
    if missing:
        raise ReadError(f"units without exactly {per_unit} record(s): {missing[:10]}")
    return units


def _attach_plans(units: dict[int, list[dict[str, Any]]], plans: list[dict[str, Any]], use: str) -> None:
    by_index: dict[int, dict[str, Any]] = {}
    for plan in plans:
        if plan["use"] != use:
            continue
        if plan["pod_index"] in by_index:
            raise ReadError(f"two plans for unit {plan['pod_index']}")
        by_index[plan["pod_index"]] = plan
    for index, records in units.items():
        if index not in by_index:
            raise ReadError(f"no plan for unit {index}")
        read_plans.check_record_against_plan(records[0], by_index[index])


def _halted(records: list[dict[str, Any]]) -> int:
    return sum(1 for r in records for g in r["games"] if g["scores"] is None)


def _verdict(interval: list[float] | None, win: float, loss: float) -> str:
    if interval is None:
        return "undefined"
    if interval[0] > win:
        return "win"
    if interval[1] < loss:
        return "loss"
    return "inconclusive"


def _void_test(verdicts: dict[str, str]) -> dict[str, Any]:
    """A2.4 with 9h's signs: a win is tested under halts_against, a loss under halts_for, and an
    inconclusive result under both (void if either imputation reaches a verdict) [proposed for A6].
    A drop scoring with no scored unit in some resample is void."""
    drop = verdicts["drop"]
    if drop == "undefined":
        return {"tested": [], "void": True, "reason": "no scored unit in some resample"}
    if drop == "win":
        tested = ["halts_against"]
        void = verdicts["halts_against"] != "win"
    elif drop == "loss":
        tested = ["halts_for"]
        void = verdicts["halts_for"] != "loss"
    else:
        tested = ["halts_for", "halts_against"]
        void = verdicts["halts_for"] == "win" or verdicts["halts_against"] == "loss"
    return {"tested": tested, "void": void, "reason": None}


def _verdict_word(drop: str, void: bool, look: int) -> str:
    if void:
        return "void"
    if drop in ("win", "loss"):
        return drop
    return "extend" if look == 1 else "not_won"


def _f(value: Decimal) -> float:
    return float(value)


def _require_signed(declaration: dict[str, Any], namespace: str) -> None:
    if declaration["a6_sha256"] is None and namespace != TEST_NAMESPACE:
        raise ReadError(f"namespace {namespace!r} is read data: the declaration names no signed A6")


def _statistics_block(units: list[Any], stats: dict[str, Callable[[Any, str], float | None]], root: int,
                      resamples: int, level: float) -> dict[str, dict[str, Any]]:
    series = {(name, scoring): [fn(u, scoring) for u in units] for name, fn in stats.items() for scoring in SCORINGS}
    boot = bootstrap_many(series, root, resamples, level)
    return {name: {scoring: boot[(name, scoring)] for scoring in SCORINGS} for name in stats}


def _report_head(read: str, look: int, decl_sha: str, declaration: dict[str, Any], namespace: str, run: str,
                 record_shas: list[str], plan_shas: list[str], unit_kind: str, count: int, level: float,
                 root: int) -> dict[str, Any]:
    return {
        "schema": REPORT_SCHEMA,
        "interfaces": pod_record.INTERFACES_VERSION,
        "read": read,
        "look": look,
        "declaration_sha256": decl_sha,
        "a6_sha256": declaration["a6_sha256"],
        "namespace": namespace,
        "run": run,
        "inputs": {"records": sorted(record_shas), "plans": sorted(plan_shas)},
        "units": {"kind": unit_kind, "count": count, "indices": [0, count - 1]},
        "bootstrap": {"resamples": declaration["resamples"], "root_seed": root, "level": level},
    }


def _verdict_tail(report: dict[str, Any], stat_name: str, section: dict[str, Any], look: int) -> None:
    win = _f(section["win_if_lower_above"])
    loss = _f(section["loss_if_upper_below"])
    block = report["statistics"][stat_name]
    verdicts = {s: _verdict(block[s]["interval"], win, loss) for s in SCORINGS}
    void = _void_test(verdicts)
    report["thresholds"] = {"statistic": stat_name, "win_if_lower_above": win, "loss_if_upper_below": loss}
    report["verdicts_by_scoring"] = verdicts
    report["void_test"] = void
    report["verdict"] = _verdict_word(verdicts["drop"], void["void"], look)


def run_read(declaration: dict[str, Any], decl_sha: str, read: str, look: int,
             record_files: list[tuple[list[dict[str, Any]], str]],
             plan_files: list[tuple[dict[str, Any], str]]) -> dict[str, Any]:
    """Compute one read's report for one look (for a dose read, `look` is the dose ordinal)."""
    if read not in READS:
        raise ReadError(f"unknown read {read!r}; reads are {list(READS)}")
    records = [r for recs, _ in record_files for r in recs]
    record_shas = [sha for _, sha in record_files]
    plans = [p for p, _ in plan_files]
    plan_shas = [sha for _, sha in plan_files]
    resamples = declaration["resamples"]
    sections = declaration["reads"]

    if read in ("b", "c", "b_prime"):
        section = sections["c" if read == "b_prime" else read]
        namespace, run = section["namespace"], section["run"]
        _require_signed(declaration, namespace)
        count = look_units(section, look)
        level = _f(section["levels"][look - 1])
        use = "read_b" if read == "b" else "paired_build"
        grouped = _collect(records, use, namespace, run, count, 1)
        _attach_plans(grouped, plans, use)
        ordered = [grouped[i][0] for i in range(count)]
        root = bootstrap_root(namespace, read, look)
        report = _report_head(read, look, decl_sha, declaration, namespace, run, record_shas, plan_shas,
                              "draft", count, level, root)
        if read == "b":
            units = [_read_b_unit(r) for r in ordered]
            stats = {"candidate_win_fraction": _stat_b}
        elif read == "c":
            units = [_paired_unit(r, "candidate_stated") for r in ordered]
            stats = {"candidate_minus_stated": _stat_paired_difference}
        else:
            units = [_paired_unit(r, "candidate_stated") for r in ordered]
            stats = {"stated_win_fraction": _stat_paired_y}
        report["games"] = {"total": sum(len(r["games"]) for r in ordered), "halted": _halted(ordered)}
        report["statistics"] = _statistics_block(units, stats, root, resamples, level)
        if read == "b_prime":
            report["verdict"] = None
        else:
            _verdict_tail(report, next(iter(stats)), section, look)
        return report

    if read == "dose":
        section = sections["dose"]
        namespace, run = section["namespace"], section["run"]
        _require_signed(declaration, namespace)
        if not (type(look) is int and look >= 0):
            raise ReadError("a dose read's look is its ordinal, an integer >= 0")
        count = section["panel_drafts"]
        level = _f(section["level"])
        grouped = _collect(records, "dose", namespace, run, count, 1)
        _attach_plans(grouped, plans, "dose")
        ordered = [grouped[i][0] for i in range(count)]
        root = bootstrap_root(namespace, read, look)
        report = _report_head(read, look, decl_sha, declaration, namespace, run, record_shas, plan_shas,
                              "draft", count, level, root)
        units = [_paired_unit(r, "current_initial") for r in ordered]
        report["games"] = {"total": sum(len(r["games"]) for r in ordered), "halted": _halted(ordered)}
        report["statistics"] = _statistics_block(units, {"current_minus_initial": _stat_paired_difference},
                                                 root, resamples, level)
        report["verdict"] = None
        return report

    # Read (a), the early pod's read (a) and the A/A: units are pairs of two pods.
    section = sections[read]
    namespace, run = section["namespace"], section["run"]
    _require_signed(declaration, namespace)
    if read == "aa":
        if look != 1:
            raise ReadError("the A/A has one look")
        count = section["pairs"]
        level = _f(section["level"])
    else:
        count = look_units(section, look)
        level = _f(section["levels"][look - 1])
    if plans:
        raise ReadError("read (a) pods follow their layout; they take no explicit plan")
    grouped = _collect(records, "read_a", namespace, run, count, 2)
    root = bootstrap_root(namespace, read, look)
    report = _report_head(read, look, decl_sha, declaration, namespace, run, record_shas, plan_shas,
                          "pair", count, level, root)
    units = []
    pairs = []
    for i in range(count):
        outcomes, ordered_pods = _read_a_pair(grouped[i])
        units.append(outcomes)
        pairs.append(ordered_pods)
    all_pods = [p for pair in pairs for p in pair]
    report["games"] = {"total": sum(len(r["games"]) for r in all_pods), "halted": _halted(all_pods)}
    report["statistics"] = _statistics_block(
        units,
        {
            "candidate_contrast": _stat_a_candidate,
            "stated_contrast": _stat_a_stated,
            "contrast_difference": _stat_a_difference,
        },
        root,
        resamples,
        level,
    )
    if read == "aa":
        report["symmetry"] = check_aa_symmetry(pairs)
        report["verdict"] = "symmetric" if report["symmetry"]["symmetric"] else "asymmetric"
    else:
        _verdict_tail(report, "candidate_contrast", section, look)
    return report


# ---------------------------------------------------------------------------------------------
# The A/A: exact symmetry on role-swapped pairs


def _game_signature(record: dict[str, Any], game: dict[str, Any]) -> list[Any]:
    """Everything a game's play depends on or produces, with deck ids replaced by their cards."""
    return [
        game["pairing"],
        game["chairs"],
        game["seat_rule"],
        game["game_seed"],
        game["episode"],
        [record["decks"][d]["cards"] for d in game["decks"]],
        game["attempts"],
        game["scores"],
    ]


def check_aa_symmetry(pairs: list[list[dict[str, Any]]]) -> dict[str, Any]:
    """Identical policies on a role-swapped pair play the same games with the roles exchanged. So
    each cross game of the even pod equals, field for field (decks by their cards, attempts and
    scores included), the game of the same pairing and builder in the odd pod, and the learned
    seat's results in the two pods sum to exactly one win per game. Any difference is reported."""
    mismatches: list[str] = []
    checked = 0
    for index, (even, odd) in enumerate(pairs):
        arms_even, arms_odd = _read_a_pod(even), _read_a_pod(odd)
        for arm in ("candidate", "stated"):
            for pairing in sorted(arms_even[arm]):
                checked += 1
                g_even, learned_even = arms_even[arm][pairing]
                if pairing not in arms_odd[arm]:
                    mismatches.append(f"pair {index} {arm} pairing {pairing}: missing in the odd pod")
                    continue
                g_odd, learned_odd = arms_odd[arm][pairing]
                if _game_signature(even, g_even) != _game_signature(odd, g_odd):
                    mismatches.append(f"pair {index} {arm} pairing {pairing}: games differ")
                    continue
                a = pod_record.win_fraction(g_even, learned_even)
                b = pod_record.win_fraction(g_odd, learned_odd)
                if (a is None) != (b is None) or (a is not None and a + b != 1.0):
                    mismatches.append(f"pair {index} {arm} pairing {pairing}: results are not role-swapped")
    return {"games_checked": checked, "mismatches": mismatches[:20], "mismatch_count": len(mismatches),
            "symmetric": not mismatches}


# ---------------------------------------------------------------------------------------------
# Output


def serialize_report(report: dict[str, Any]) -> str:
    return json.dumps(report, indent=2, ensure_ascii=True, allow_nan=False) + "\n"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Read cube M4 pod records into one read report.")
    parser.add_argument("--declaration", required=True, type=Path)
    parser.add_argument("--read", required=True, choices=READS)
    parser.add_argument("--look", required=True, type=int, help="1 or 2; a dose read's ordinal")
    parser.add_argument("--records", required=True, nargs="+", type=Path)
    parser.add_argument("--plans", nargs="*", default=[], type=Path)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args(argv)
    declaration, decl_sha = load_declaration(args.declaration)
    record_files = [pod_record.load_records(p) for p in args.records]
    plan_files = [read_plans.load_plan(p) for p in args.plans]
    report = run_read(declaration, decl_sha, args.read, args.look, record_files, plan_files)
    text = serialize_report(report)
    if args.out is None:
        sys.stdout.write(text)
    else:
        args.out.write_bytes(text.encode("utf-8"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
