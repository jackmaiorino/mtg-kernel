"""Readers of the cube M4 reads (D7 and D7a), built to the signed A6 (part 1), m4-interfaces/v2 and
read plan v3.

Reads: acceptance (a) and early (a) (2 by 2 of picks by builder; the candidate row gives the verdict),
(b) (A1.5 opponent), (b') (stated builds against C(P), from (c)'s games), (c) (non-inferiority of
candidate minus stated), the A/A (A6.12, exact symmetry), dose reads and D13 (report only), the
companions of (a) (A4.7, report only) and the D14 sizing probes (A6.18).

Rules, all from A6:
- Scores (A6.3): natural win 1, draw 1/2, loss 0 for the focal side. A halt is dropped from its
  pool's or row's completed-game denominator. Means need a completed game; a difference needs both
  means; a unit averages its defined pool statistics with equal weight; units with no defined
  statistic are omitted from the drop statistic and counted.
- Intervals (A6.4): 10,000 resamples; resample b draws unit t as seed(["bootstrap", namespace, b, t])
  mod n, shared by every statistic and halt treatment of the read and look; (b') uses (c)'s draws.
  Percentiles 1/40 and 39/40 by linear interpolation at p (B - 1). All arithmetic is exact rational
  (fractions.Fraction), interpolation included; floats appear only for display. An undefined
  observed statistic or resample makes the drop interval undefined.
- Halt completions (A6.5): lower substitutes a loss for a halt that enters with a positive
  coefficient and a win for one that enters negatively; upper reverses. They use every planned game.
- Verdicts (A6.6): win if the lower percentile is strictly above the cut, loss (or inferiority) if
  the upper is strictly below; otherwise inconclusive. The gate is void if the drop, lower and upper
  verdicts differ. Signed disposition: at look 1 a verdict-flip void with a defined drop interval
  schedules the extension, an undefined drop interval ends the attempt; at look 2 a void ends it.
"""

from __future__ import annotations

import argparse
from decimal import Decimal
from fractions import Fraction
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
import read_sets  # noqa: E402

DECLARATION_SCHEMA = "mtg-kernel-cube-read-declaration/v2"
REPORT_SCHEMA = "mtg-kernel-cube-read-report/v2"
TEST = read_plans.TEST
SCORINGS = ("drop", "lower", "upper")
GATES = ("a", "a_early", "b", "c")
READ_FAMILY = {"a": "a", "a_early": "a_early", "b": "b", "c": "c", "b_prime": "c", "aa": "aa", "dose": "dose",
               "d13": "d13", "a_one": "a_one", "a_early_one": "a_early_one", "a_all": "a_all",
               "a_early_all": "a_early_all", "size_a": "size_a", "size_a_early": "size_a_early",
               "size_b": "size_b", "size_c": "size_c"}
READS = tuple(READ_FAMILY)
SIZED_GATE = {"size_a": "a", "size_a_early": "a_early", "size_b": "b", "size_c": "c"}
COMPANION_PARENT = {"a_one": "a", "a_all": "a", "a_early_one": "a_early", "a_early_all": "a_early"}
WIN = {1: Fraction(1), 0: Fraction(1, 2), -1: Fraction(0)}

A6_PART1_SHA256 = "fd189cb58321d6b8401b1eaab244759ee122f438d02948584716a18506d4fbd1"
A6_PART1_HEADING = b"## Signature, part 1"
A6_SIGNED_RESAMPLES = 10_000
A6_SIGNED_LEVEL = Fraction(1, 40)
A6_SIGNED_EXTENSION_FACTOR = 2
A6_SIGNED_Z_SUM = Fraction(14, 5)
A6_SIGNED_FLOOR = 128
A6_SIGNED_CAP = 512

A6_SIGNED_SIZING = {
    "a": {"delta": Fraction(3, 50), "z_sum": A6_SIGNED_Z_SUM, "floor": A6_SIGNED_FLOOR, "cap": A6_SIGNED_CAP},
    "a_early": {"delta": Fraction(3, 50), "z_sum": A6_SIGNED_Z_SUM, "floor": A6_SIGNED_FLOOR, "cap": A6_SIGNED_CAP},
    "b": {"delta": Fraction(3, 100), "z_sum": A6_SIGNED_Z_SUM, "floor": A6_SIGNED_FLOOR, "cap": A6_SIGNED_CAP},
    "c": {"delta": Fraction(1, 50), "z_sum": A6_SIGNED_Z_SUM, "floor": A6_SIGNED_FLOOR, "cap": A6_SIGNED_CAP},
}

A6_SIGNED_RULES: dict[str, dict[str, Any]] = {
    "a": {"levels": [A6_SIGNED_LEVEL, A6_SIGNED_LEVEL], "cut": Fraction(1, 25),
          "extension_factor": A6_SIGNED_EXTENSION_FACTOR, "sizing": A6_SIGNED_SIZING["a"]},
    "a_early": {"levels": [A6_SIGNED_LEVEL, A6_SIGNED_LEVEL], "cut": Fraction(1, 25),
                "extension_factor": A6_SIGNED_EXTENSION_FACTOR, "sizing": A6_SIGNED_SIZING["a_early"]},
    "b": {"levels": [A6_SIGNED_LEVEL, A6_SIGNED_LEVEL], "cut": Fraction(13, 25),
          "extension_factor": A6_SIGNED_EXTENSION_FACTOR, "sizing": A6_SIGNED_SIZING["b"]},
    "c": {"levels": [A6_SIGNED_LEVEL, A6_SIGNED_LEVEL], "cut": Fraction(-1, 50),
          "extension_factor": A6_SIGNED_EXTENSION_FACTOR, "sizing": A6_SIGNED_SIZING["c"]},
    "aa": {"pairs": 4, "level": A6_SIGNED_LEVEL},
    "dose": {"panel_drafts": 32, "level": A6_SIGNED_LEVEL},
    "d13": {"panel_drafts": 32, "level": A6_SIGNED_LEVEL},
    "a_one": {"level": A6_SIGNED_LEVEL},
    "a_early_one": {"level": A6_SIGNED_LEVEL},
    "a_all": {},
    "a_early_all": {},
    "size_a": {"units": 48, "level": A6_SIGNED_LEVEL},
    "size_a_early": {"units": 48, "level": A6_SIGNED_LEVEL},
    "size_b": {"units": 96, "level": A6_SIGNED_LEVEL},
    "size_c": {"units": 48, "level": A6_SIGNED_LEVEL},
}

DECL_KEYS = ("schema", "interfaces", "read_plan", "a6_sha256", "resamples", "pins", "inputs", "reads")
GATE_KEYS = ("namespace", "levels", "cut", "extension_factor", "n", "sizing")
SIZING_KEYS = ("delta", "z_sum", "floor", "cap")
SECTION_KEYS = {
    "a": GATE_KEYS, "a_early": GATE_KEYS, "b": GATE_KEYS, "c": GATE_KEYS,
    "aa": ("namespace", "pairs", "level"),
    "dose": ("namespace", "panel_drafts", "level", "panel_sha256"),
    "d13": ("namespace", "panel_drafts", "level", "panel_sha256"),
    "a_one": ("namespace", "level"), "a_early_one": ("namespace", "level"),
    "a_all": ("namespace",), "a_early_all": ("namespace",),
    "size_a": ("namespace", "units", "level"), "size_a_early": ("namespace", "units", "level"),
    "size_b": ("namespace", "units", "level"), "size_c": ("namespace", "units", "level"),
}


class DeclarationError(ValueError):
    """A declaration file that does not follow mtg-kernel-cube-read-declaration/v2."""


class ReadError(ValueError):
    """Inputs a reader refuses."""


# ---------------------------------------------------------------------------------------------
# The declaration: A6's constants, transcribed, exact


def _dfail(path: str, message: str) -> None:
    raise DeclarationError(f"declaration {path}: {message}")


def _exact(value: Any, path: str) -> Fraction:
    if type(value) is int:
        return Fraction(value)
    if isinstance(value, Decimal) and value.is_finite():
        return Fraction(value)
    _dfail(path, f"must be a number, got {value!r}")
    raise AssertionError


def _count(value: Any, path: str, minimum: int = 1) -> int:
    if not (type(value) is int and value >= minimum):
        _dfail(path, f"must be an integer >= {minimum}, got {value!r}")
    return value


def _level(value: Any, path: str) -> Fraction:
    level = _exact(value, path)
    if not 0 < level < Fraction(1, 2):
        _dfail(path, "a one-sided level lies in (0, 1/2)")
    return level


def verify_countersigned_a6(a6_source: Path | bytes, expected_sha256: str) -> bytes:
    """Verify the actual countersigned A6 declaration file against its pinned sha256, signed part 1
    sha256, and part 2 countersignature."""
    if not read_plans._is_sha(expected_sha256):
        raise ReadError("expected A6 sha256 must be a lowercase sha256 hex string")
    if isinstance(a6_source, (str, Path)):
        try:
            raw = Path(a6_source).read_bytes()
        except OSError as exc:
            raise ReadError(f"cannot read countersigned A6 file {a6_source}: {exc}") from exc
    elif isinstance(a6_source, bytes):
        raw = a6_source
    else:
        raise ReadError("a6_file must be a path or bytes")
    actual_sha = hashlib.sha256(raw).hexdigest()
    if actual_sha != expected_sha256:
        raise ReadError(f"A6 file sha256 {actual_sha} differs from declared a6_sha256 {expected_sha256}")
    normalized = raw.replace(b"\r\n", b"\n")
    idx = normalized.find(A6_PART1_HEADING)
    if idx < 0:
        raise ReadError("A6 file lacks the signed part 1 signature block")
    part1_bytes = normalized[:idx].rstrip(b"\n") + b"\n"
    part1_sha = hashlib.sha256(part1_bytes).hexdigest()
    if part1_sha != A6_PART1_SHA256:
        raise ReadError(
            f"A6 part 1 text sha256 {part1_sha} differs from signed part 1 {A6_PART1_SHA256}"
        )
    tail = normalized[idx:].decode("utf-8", errors="replace")
    part2_marker = "## Signature, part 2"
    p2_idx = tail.find(part2_marker)
    if p2_idx < 0:
        raise ReadError("A6 file has no countersigned part 2 signature section")
    p2_block = tail[p2_idx:]
    if "NOT signed yet" in p2_block or "TO FILL AT SIGNING" in p2_block:
        raise ReadError("A6 part 2 is not countersigned (contains unsigned placeholder text)")
    if "countersign" not in p2_block.lower():
        raise ReadError("A6 part 2 section lacks a countersignature statement")
    return raw


def _check_signed_section_rules(name: str, sec: dict[str, Any], resamples: int, fail_fn: Callable[[str], None]) -> None:
    """Enforce signed A6 part 1 constants on any non-test (live) read section."""
    if resamples != A6_SIGNED_RESAMPLES:
        fail_fn(f"live read requires signed resamples {A6_SIGNED_RESAMPLES}, got {resamples}")
    signed = A6_SIGNED_RULES[name]
    for key, expected in signed.items():
        if sec.get(key) != expected:
            fail_fn(f"live read {name}.{key} must equal signed A6 value {expected!r}, got {sec.get(key)!r}")


def _validate_pins_field(value: Any, path: str) -> dict[str, str] | None:
    if value is None:
        return None
    if not isinstance(value, dict) or tuple(value) != pod_record.PINS_KEYS:
        _dfail(path, f"keys must be {list(pod_record.PINS_KEYS)} in order or null")
    if not all(read_plans._is_sha(v) for v in value.values()):
        _dfail(path, "every pin must be a non-null sha256 string")
    return dict(value)


def _validate_inputs_field(value: Any, path: str) -> dict[str, str] | None:
    if value is None:
        return None
    if not isinstance(value, dict) or tuple(value) != read_plans.INPUT_KEYS:
        _dfail(path, f"keys must be {list(read_plans.INPUT_KEYS)} in order or null")
    if not all(read_plans._is_sha(v) for v in value.values()):
        _dfail(path, "every input pin must be a non-null sha256 string")
    return dict(value)


def validate_declaration(doc: Any) -> dict[str, Any]:
    if not isinstance(doc, dict) or tuple(doc) != DECL_KEYS:
        _dfail("", f"keys must be {list(DECL_KEYS)} in order")
    if doc["schema"] != DECLARATION_SCHEMA or doc["interfaces"] != pod_record.INTERFACES_VERSION \
            or doc["read_plan"] != read_plans.PLAN_SCHEMA:
        _dfail("schema", "unknown schema, interfaces or read plan version")
    a6 = doc["a6_sha256"]
    if a6 is not None and not read_plans._is_sha(a6):
        _dfail("a6_sha256", "is the countersigned A6 file's sha256 or null")
    resamples = _count(doc["resamples"], "resamples")
    pins = _validate_pins_field(doc["pins"], "pins")
    inputs = _validate_inputs_field(doc["inputs"], "inputs")
    reads = doc["reads"]
    if not isinstance(reads, dict) or tuple(reads) != tuple(SECTION_KEYS):
        _dfail("reads", f"sections must be {list(SECTION_KEYS)} in order")
    out: dict[str, Any] = {}
    has_live = False
    for name, keys in SECTION_KEYS.items():
        sec = reads[name]
        path = f"reads.{name}"
        if not isinstance(sec, dict) or tuple(sec) != keys:
            _dfail(path, f"keys must be {list(keys)} in order")
        ns = sec["namespace"]
        family = READ_FAMILY[name]
        if ns != TEST and ns not in read_plans.FAMILIES[family]["namespaces"]:
            _dfail(f"{path}.namespace", f"{ns!r} is not a namespace of family {family}")
        norm: dict[str, Any] = {"namespace": ns}
        if keys == GATE_KEYS:
            levels = sec["levels"]
            if not (isinstance(levels, list) and len(levels) == 2):
                _dfail(f"{path}.levels", "one level per look, two looks")
            norm["levels"] = [_level(v, f"{path}.levels[{i}]") for i, v in enumerate(levels)]
            norm["cut"] = _exact(sec["cut"], f"{path}.cut")
            norm["extension_factor"] = _count(sec["extension_factor"], f"{path}.extension_factor")
            norm["n"] = None if sec["n"] is None else _count(sec["n"], f"{path}.n")
            sz = sec["sizing"]
            if not isinstance(sz, dict) or tuple(sz) != SIZING_KEYS:
                _dfail(f"{path}.sizing", f"keys must be {list(SIZING_KEYS)} in order")
            norm["sizing"] = {"delta": _exact(sz["delta"], f"{path}.sizing.delta"),
                              "z_sum": _exact(sz["z_sum"], f"{path}.sizing.z_sum"),
                              "floor": _count(sz["floor"], f"{path}.sizing.floor"),
                              "cap": _count(sz["cap"], f"{path}.sizing.cap")}
            if norm["sizing"]["delta"] <= 0 or norm["sizing"]["z_sum"] <= 0 or \
                    norm["sizing"]["cap"] < norm["sizing"]["floor"]:
                _dfail(f"{path}.sizing", "delta and z_sum are positive and cap >= floor")
        for key in keys:
            if key in ("namespace",) + GATE_KEYS:
                continue
            if key == "level":
                norm[key] = _level(sec[key], f"{path}.level")
            elif key == "panel_sha256":
                if sec[key] is not None and not read_plans._is_sha(sec[key]):
                    _dfail(f"{path}.panel_sha256", "is the pinned panel digest or null")
                norm[key] = sec[key]
            else:
                norm[key] = _count(sec[key], f"{path}.{key}")
        if ns != TEST:
            has_live = True
            _check_signed_section_rules(name, norm, resamples, lambda msg, p=path: _dfail(p, msg))
        out[name] = norm
    if has_live and pins is None:
        _dfail("pins", "a live declaration must bind the five input pins")
    return {"a6_sha256": a6, "resamples": resamples, "pins": pins, "inputs": inputs, "reads": out}


def load_declaration(path: str | Path) -> tuple[dict[str, Any], str]:
    data = Path(path).read_bytes()
    try:
        doc = json.loads(data.decode("utf-8"), parse_float=Decimal)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise DeclarationError(f"declaration is not UTF-8 JSON: {exc}") from exc
    return validate_declaration(doc), hashlib.sha256(data).hexdigest()


# ---------------------------------------------------------------------------------------------
# Sizing (A6.18)


def size_from_probe(values: list[Fraction | None], sizing: dict[str, Any]) -> dict[str, Any]:
    """D14's count from a probe's drop unit statistics. n_raw = z_sum^2 s^2 / delta^2 from the exact
    sample variance (denominator m - 1), rounded up, clamped to [floor, cap]. Zero spread, or some
    undefined units with at least two defined, uses the cap and flags it. Fewer than two defined
    units supply no count."""
    defined = [v for v in values if v is not None]
    m = len(defined)
    out: dict[str, Any] = {"units_defined": m, "units_planned": len(values), "n_raw": None, "n": None, "flag": None}
    if m < 2:
        out["flag"] = "fewer than two defined units: no count"
        return out
    mean = sum(defined, Fraction(0)) / m
    variance = sum(((v - mean) ** 2 for v in defined), Fraction(0)) / (m - 1)
    out["s_squared"] = _num(variance)
    if variance == 0:
        out.update(n=sizing["cap"], flag="zero spread: the cap")
        return out
    if m < len(values):
        out.update(n=sizing["cap"], flag="some units undefined: the cap")
        return out
    n_raw = sizing["z_sum"] ** 2 * variance / sizing["delta"] ** 2
    out["n_raw"] = _num(n_raw)
    n = math.ceil(n_raw)
    clamped = min(sizing["cap"], max(sizing["floor"], n))
    out["n"] = clamped
    if clamped != n:
        out["flag"] = "floor binds" if clamped > n else "cap binds"
    return out


def look_units(section: dict[str, Any], look: int) -> int:
    if not (type(look) is int and look in (1, 2)):
        raise ReadError(f"a gate has at most 2 looks; look {look!r} is refused")
    n = section["n"]
    if n is None:
        raise ReadError("the read's count n is not recorded yet (after D14, before the holdout)")
    return n if look == 1 else n + section["extension_factor"] * n


# ---------------------------------------------------------------------------------------------
# Bootstrap (A6.4), exact


def resample_indices(namespace: str, resample: int, n: int) -> list[int]:
    """seed(["bootstrap", namespace, resample, t]) mod n for t = 0 to n - 1. The compact JSON is
    '["bootstrap","<namespace>",b,t]' (namespaces are [a-z0-9-]+), digested incrementally; a test
    checks it equals deal.seed."""
    if not pod_record.NAME_RE.match(namespace):
        raise ReadError(f"namespace {namespace!r}")
    base = hashlib.sha256(f'["bootstrap","{namespace}",{resample},'.encode("ascii"))
    out = []
    for t in range(n):
        h = base.copy()
        h.update(f"{t}]".encode("ascii"))
        out.append((int.from_bytes(h.digest()[:8], "little") & deal.MASK) % n)
    return out


def percentile(sorted_values: list[Fraction], p: Fraction) -> Fraction:
    position = p * (len(sorted_values) - 1)
    low = math.floor(position)
    high = min(low + 1, len(sorted_values) - 1)
    return sorted_values[low] + (sorted_values[high] - sorted_values[low]) * (position - low)


def _num(value: Fraction | None) -> dict[str, Any] | None:
    if value is None:
        return None
    return {"exact": f"{value.numerator}/{value.denominator}", "approx": float(value)}


def _mean(values: list[Fraction | None]) -> Fraction | None:
    defined = [v for v in values if v is not None]
    return sum(defined, Fraction(0)) / len(defined) if defined else None


def bootstrap(series: dict[Any, list[Fraction | None]], namespace: str, resamples: int, level: Fraction
              ) -> dict[Any, dict[str, Any]]:
    """One shared resampling of n clusters for every series. Each resampled statistic is the mean of
    the defined resampled unit values, multiplicities kept, computed as integers over a common
    denominator."""
    n = len(next(iter(series.values())))
    if n == 0:
        raise ReadError("no units")
    prepared = {}
    for key, values in series.items():
        dens = [v.denominator for v in values if v is not None]
        common = math.lcm(*dens) if dens else 1
        nums = [None if v is None else v.numerator * (common // v.denominator) for v in values]
        prepared[key] = (nums, common, all(v is not None for v in values), [])
    for b in range(resamples):
        idx = resample_indices(namespace, b, n)
        for key, (nums, common, dense, acc) in prepared.items():
            if acc is None:
                continue
            if dense:
                acc.append(Fraction(sum(map(nums.__getitem__, idx)), n * common))
                continue
            picked = [nums[i] for i in idx if nums[i] is not None]
            if not picked:
                prepared[key] = (nums, common, dense, None)
                continue
            acc.append(Fraction(sum(picked), len(picked) * common))
    out = {}
    for key, values in series.items():
        acc = prepared[key][3]
        estimate = _mean(values)
        interval = None
        if acc is not None and estimate is not None:
            acc.sort()
            interval = [percentile(acc, level), percentile(acc, 1 - level)]
        out[key] = {"estimate": estimate, "interval": interval,
                    "units_defined": sum(v is not None for v in values), "units_planned": n}
    return out


def _render(block: dict[str, Any]) -> dict[str, Any]:
    return {"estimate": _num(block["estimate"]),
            "interval": None if block["interval"] is None else [_num(v) for v in block["interval"]],
            "units_defined": block["units_defined"], "units_planned": block["units_planned"]}


# ---------------------------------------------------------------------------------------------
# Statistics. A game outcome is the focal win fraction (Fraction) or None for a halt.


def _complete(wf: Fraction | None, scoring: str, positive: bool) -> Fraction | None:
    """A6.5: under `lower` a halt entering with a positive coefficient is a loss, a negative one a
    win; `upper` reverses; `drop` leaves it out."""
    if wf is not None or scoring == "drop":
        return wf
    loss = (scoring == "lower") == positive
    return Fraction(0) if loss else Fraction(1)


def _arm(games: list[Fraction | None], scoring: str, positive: bool) -> Fraction | None:
    return _mean([_complete(w, scoring, positive) for w in games])


def stat_pools(unit: list[list[Fraction | None]], scoring: str) -> Fraction | None:
    """(b), (b'): the mean of the defined pool means."""
    return _mean([_arm(g, scoring, True) for g in unit])


def stat_paired(unit: list[tuple[list, list]], scoring: str) -> Fraction | None:
    """(c), dose, D13: the mean of the defined pool differences x minus y."""
    diffs = []
    for xs, ys in unit:
        mx, my = _arm(xs, scoring, True), _arm(ys, scoring, False)
        diffs.append(None if mx is None or my is None else mx - my)
    return _mean(diffs)


def stat_y(unit: list[tuple[list, list]], scoring: str) -> Fraction | None:
    """(b'): the stated build's mean, its halts completed for the stated side."""
    return _mean([_arm(ys, scoring, True) for _, ys in unit])


def _edge(games: list[Fraction | None], scoring: str, positive: bool) -> Fraction | None:
    m = _arm(games, scoring, positive)
    if m is None:
        return None
    return 2 * m - 1


def stat_a_candidate(unit: dict[str, list], scoring: str) -> Fraction | None:
    return _edge(unit["candidate"], scoring, True)


def stat_a_stated(unit: dict[str, list], scoring: str) -> Fraction | None:
    return _edge(unit["stated"], scoring, True)


def stat_a_interaction(unit: dict[str, list], scoring: str) -> Fraction | None:
    cand = _edge(unit["candidate"], scoring, True)
    stated = _edge(unit["stated"], scoring, False)
    return None if cand is None or stated is None else cand - stated


def stat_edge(unit: list[Fraction | None], scoring: str) -> Fraction | None:
    return _edge(unit, scoring, True)


def _wf(game: dict[str, Any], deck: int) -> Fraction | None:
    if game["scores"] is None:
        return None
    return WIN[game["scores"][game["decks"].index(deck)]]


# ---------------------------------------------------------------------------------------------
# Units from records and plans


def units_b(records: dict[int, dict[str, Any]], units: range) -> list[list[list[Fraction | None]]]:
    out = []
    for i in units:
        games = records[i]["games"]
        out.append([[_wf(games[4 * s + j], 2 * s) for j in range(4)] for s in range(pod_record.SEATS)])
    return out


def units_paired(records: dict[int, dict[str, Any]], units: range, per_opponent: bool = False,
                 q: int | None = None) -> list[list[tuple[list, list]]]:
    """Per unit, per paired build (and, with per_opponent, per opponent as D13 keeps them):
    (x outcomes, y outcomes) on matched games."""
    out = []
    for i in units:
        rec = records[i]
        by_id = {g["game"]: g for g in rec["games"]}
        pools = []
        for pair in rec["paired_builds"]:
            if q is not None and pair["q"] != q:
                continue
            matched = list(zip(pair["games_x"], pair["games_y"]))
            if per_opponent:
                def faces(gx: int, pair: dict[str, Any] = pair) -> int:
                    decks = by_id[gx]["decks"]
                    return decks[1 - decks.index(pair["deck_x"])]
                groups = [[(gx, gy) for gx, gy in matched if faces(gx) == opp] for opp in pair["opponents"]]
            else:
                groups = [matched]
            for group in groups:
                pools.append(([_wf(by_id[gx], pair["deck_x"]) for gx, _ in group],
                              [_wf(by_id[gy], pair["deck_y"]) for _, gy in group]))
        out.append(pools)
    return out


def units_a(records: dict[int, dict[str, Any]], plans: dict[int, dict[str, Any]], units: range
            ) -> list[dict[str, list[Fraction | None]]]:
    out = []
    for i in units:
        rows: dict[str, list[Fraction | None]] = {"candidate": [], "stated": []}
        for pod in (2 * i, 2 * i + 1):
            learning = {c["chair"] for c in plans[pod]["chairs"] if c["role"] == "learning"}
            for game in records[pod]["games"]:
                row = "candidate" if game["game"] % 2 == 0 else "stated"
                deck = next(d for d, c in zip(game["decks"], game["chairs"]) if c in learning)
                rows[row].append(_wf(game, deck))
        out.append(rows)
    return out


def units_one(records: dict[int, dict[str, Any]], units: range) -> list[list[Fraction | None]]:
    out = []
    for i in units:
        games: list[Fraction | None] = []
        for r in range(pod_record.SEATS):
            for game in records[8 * i + r]["games"]:
                games.append(_wf(game, game["decks"][game["chairs"].index(r)]))
        out.append(games)
    return out


# ---------------------------------------------------------------------------------------------
# Verdicts and dispositions (A6.5, A6.6 and the signed decision 2)


def verdict_of(interval: list[Fraction] | None, cut: Fraction) -> str:
    if interval is None:
        return "undefined"
    if interval[0] > cut:
        return "win"
    if interval[1] < cut:
        return "loss"
    return "inconclusive"


def disposition(verdicts: dict[str, str], look: int) -> dict[str, Any]:
    """The verdict word and the next action. win -> proceed; loss -> the attempt ends; inconclusive at
    look 1 -> extend (2n fresh clusters), at look 2 -> not_won (the attempt ends). A verdict flip
    is void: at look 1 it extends (its drop interval being defined), at look 2 it ends the attempt.
    An undefined drop interval is void and ends the attempt at either look."""
    drop = verdicts["drop"]
    if drop == "undefined":
        return {"verdict": "void", "void": "undefined drop interval", "next": "attempt_ends"}
    if len(set(verdicts.values())) > 1:
        if look == 1:
            return {"verdict": "extend", "void": "verdicts differ", "next": "extension"}
        return {"verdict": "void", "void": "verdicts differ", "next": "attempt_ends"}
    if drop == "win":
        return {"verdict": "win", "void": None, "next": "proceed"}
    if drop == "loss":
        return {"verdict": "loss", "void": None, "next": "attempt_ends"}
    if look == 1:
        return {"verdict": "extend", "void": None, "next": "extension"}
    return {"verdict": "not_won", "void": None, "next": "attempt_ends"}


# ---------------------------------------------------------------------------------------------
# The A/A exact test (A6.12)


def aa_symmetry(records: dict[int, dict[str, Any]], units: range) -> dict[str, Any]:
    """Across the two mirrors of each pair: equal pools and decks, and for every game equal chairs,
    seeds and natural outcome in session-player order; a halt passes only if the same game halts in
    both mirrors with the same stop kind, last-action source and decision counts. Then the learned
    scores are complementary, each row's edge is exactly zero and so is the interaction; a row with
    no completed mirrored game cannot certify the pair."""
    problems: list[str] = []
    for i in units:
        a, b = records[2 * i], records[2 * i + 1]
        if a["draft"]["packs"] != b["draft"]["packs"] or a["draft"]["pools"] != b["draft"]["pools"]:
            problems.append(f"pair {i}: packs or pools differ")
        if [d["cards"] for d in a["decks"]] != [d["cards"] for d in b["decks"]]:
            problems.append(f"pair {i}: decks differ")
        completed = {"candidate": 0, "stated": 0}
        for ga, gb in zip(a["games"], b["games"]):
            where = f"pair {i} game {ga['game']}"
            if (ga["chairs"], ga["game_seed"], ga["episode"]) != (gb["chairs"], gb["game_seed"], gb["episode"]):
                problems.append(f"{where}: schedule differs")
                continue
            aa_, ab_ = ga["attempts"][0], gb["attempts"][0]
            if aa_["halted"] != ab_["halted"]:
                problems.append(f"{where}: unmirrored halt")
                continue
            if aa_["halted"]:
                keys = ("stop_kind", "last_action_source", "decisions", "physical_decisions", "policy_steps")
                if any(aa_[k] != ab_[k] for k in keys):
                    problems.append(f"{where}: mirrored halt differs in stop kind, source or counts")
                continue
            if aa_["end"] != ab_["end"] or ga["scores"] != gb["scores"]:
                problems.append(f"{where}: natural outcome differs")
                continue
            completed["candidate" if ga["game"] % 2 == 0 else "stated"] += 1
        for row, count in completed.items():
            if count == 0:
                problems.append(f"pair {i}: the {row} row has no completed mirrored game")
    return {"symmetric": not problems, "problems": problems[:20], "problem_count": len(problems)}


# ---------------------------------------------------------------------------------------------
# Running a read


def _require_signed(declaration: dict[str, Any], sec_name: str, namespace: str,
                    a6_file: Path | bytes | None) -> None:
    if namespace == TEST:
        return
    if declaration["a6_sha256"] is None:
        raise ReadError(f"namespace {namespace!r} is read data: the declaration names no countersigned A6")
    if a6_file is None:
        raise ReadError(f"namespace {namespace!r} is read data: the countersigned A6 file must be verified")
    verify_countersigned_a6(a6_file, declaration["a6_sha256"])
    _check_signed_section_rules(
        sec_name, declaration["reads"][sec_name], declaration["resamples"],
        lambda msg: (_ for _ in ()).throw(ReadError(msg)),
    )
    if declaration.get("pins") is None:
        raise ReadError(f"namespace {namespace!r} is read data: the declaration binds no input pins")
    family = READ_FAMILY[sec_name]
    if read_plans.FAMILIES[family]["layout"] in ("b", "c", "d13") and declaration.get("inputs") is None:
        raise ReadError(f"namespace {namespace!r} is read data: dealer family {family} binds no pool/constants inputs")


def _check_declared_pins(declaration: dict[str, Any], family: str, sets: list[dict[str, Any]]) -> None:
    pins = declaration.get("pins")
    inputs = declaration.get("inputs")
    a6_sha = declaration.get("a6_sha256")
    dealer_family = read_plans.FAMILIES[family]["layout"] in ("b", "c", "d13")
    for s in sets:
        man = s["manifest"]
        if man["namespace"] != TEST and man["declaration_sha256"] != a6_sha:
            raise ReadError(
                f"set {man['set']} declaration_sha256 differs from countersigned A6 {a6_sha}"
            )
        for pid, plan in s["plans"].items():
            if pins is not None and plan["pins"] != pins:
                raise ReadError(f"set {man['set']} plan {pid} pins differ from declared pins")
            if inputs is not None and dealer_family and plan["inputs"] != inputs:
                raise ReadError(f"set {man['set']} plan {pid} dealer inputs differ from declared inputs")


def _set_info(opened: dict[str, Any]) -> dict[str, Any]:
    return {"set": opened["manifest"]["set"], "plan_set_sha256": opened["manifest"]["plan_set_sha256"],
            "execution": opened["execution"], "expired_games": opened["expired"]}


BASICS_BANDS: tuple[tuple[str, int, int], ...] = (
    ("0-14", 0, 14),
    ("15-19", 15, 19),
    ("20-35", 20, 35),
    ("36-40", 36, 40),
)
DECISION_CAP_STOP_KINDS = frozenset({
    "decision_cap",
    "driver_decision_cap",
    "terminal:max_physical_decisions",
    "terminal:max_policy_steps",
})


def _games(records: dict[int, dict[str, Any]]) -> dict[str, int]:
    games = [g for r in records.values() for g in r["games"]]
    return {"played": len(games), "completed": sum(g["scores"] is not None for g in games),
            "halted": sum(g["scores"] is None for g in games)}


def _scoped_games(read: str, records: dict[int, dict[str, Any]]) -> dict[int, list[dict[str, Any]]]:
    """Games belonging to `read`'s own denominator. For (b'), only the stated-arm games (games_y)."""
    if read != "b_prime":
        return {pid: list(rec["games"]) for pid, rec in records.items()}
    out: dict[int, list[dict[str, Any]]] = {}
    for pid, rec in records.items():
        stated_ids = {gy for pair in rec["paired_builds"] for gy in pair["games_y"]}
        out[pid] = [g for g in rec["games"] if g["game"] in stated_ids]
    return out


def _games_from_scoped(scoped: dict[int, list[dict[str, Any]]]) -> dict[str, int]:
    games = [g for gs in scoped.values() for g in gs]
    return {"played": len(games), "completed": sum(g["scores"] is not None for g in games),
            "halted": sum(g["scores"] is None for g in games)}


def _game_arm_and_play_draw(read: str, rec: dict[str, Any], plan: dict[str, Any],
                            game: dict[str, Any]) -> tuple[str, str]:
    layout = read_plans.FAMILIES[plan["family"]]["layout"]
    if layout == "a":
        arm = "candidate" if game["game"] % 2 == 0 else "stated"
        learning = {c["chair"] for c in plan["chairs"] if c["role"] == "learning"}
        pos = next((idx for idx, ch in enumerate(game["chairs"]) if ch in learning), 0)
        return arm, ("play" if pos == 0 else "draw")
    if layout == "one":
        rot = plan["rotation"]
        pos = game["chairs"].index(rot) if rot in game["chairs"] else 0
        return "single_seat", ("play" if pos == 0 else "draw")
    if layout == "all":
        return "all_learned", "play"
    if layout == "b":
        focal = next((d for d in game["decks"] if plan["decks"][d]["source"] == "build"), game["decks"][0])
        return "candidate", ("play" if game["decks"].index(focal) == 0 else "draw")
    # Paired dealer layouts (c, b_prime, dose, d13)
    focal = next((d for d in game["decks"] if plan["decks"][d]["source"] == "build"), game["decks"][0])
    arm = plan["decks"][focal]["row"]
    return arm, ("play" if game["decks"].index(focal) == 0 else "draw")


def _halts_and_lengths(read: str, records: dict[int, dict[str, Any]], plans: dict[int, dict[str, Any]],
                       scoped: dict[int, list[dict[str, Any]]]) -> tuple[dict[str, Any], bool]:
    """A2.3 halt and game-length breakdowns and A2.1 >0.1% over-8192 warning."""
    by_stop_kind: dict[str, int] = {}
    by_last_source: dict[str, int] = {}
    by_card_present: dict[int, list[int]] = {}
    by_card_charged: dict[int, int] = {}
    by_play_draw: dict[str, dict[str, int]] = {
        "play": {"completed": 0, "halted": 0},
        "draw": {"completed": 0, "halted": 0},
    }
    by_arm: dict[str, dict[str, int]] = {}
    completed_decisions: list[Fraction] = []
    over_8192 = 0
    for pid, gs in sorted(scoped.items()):
        rec = records[pid]
        plan = plans[pid]
        deck_cards = {d["deck"]: set(d["cards"]) for d in rec["decks"]}
        for g in gs:
            att = g["attempts"][0]
            halted = g["scores"] is None
            arm, pd = _game_arm_and_play_draw(read, rec, plan, g)
            by_play_draw[pd]["halted" if halted else "completed"] += 1
            arm_bucket = by_arm.setdefault(arm, {"completed": 0, "halted": 0})
            arm_bucket["halted" if halted else "completed"] += 1
            cards_in_game = deck_cards.get(g["decks"][0], set()) | deck_cards.get(g["decks"][1], set())
            for cid in cards_in_game:
                slot = by_card_present.setdefault(cid, [0, 0])
                slot[1 if halted else 0] += 1
            if halted:
                sk = att["stop_kind"] or "unknown"
                by_stop_kind[sk] = by_stop_kind.get(sk, 0) + 1
                src = att["last_action_source"]
                src_key = "null" if src is None else str(src)
                by_last_source[src_key] = by_last_source.get(src_key, 0) + 1
                if src is not None:
                    by_card_charged[src] = by_card_charged.get(src, 0) + 1
            else:
                dec = att["decisions"]
                completed_decisions.append(Fraction(dec))
                if dec > 8192:
                    over_8192 += 1
    completed_decisions.sort()
    if completed_decisions:
        pcts = {
            "p50": _num(percentile(completed_decisions, Fraction(1, 2))),
            "p99": _num(percentile(completed_decisions, Fraction(99, 100))),
            "p99_9": _num(percentile(completed_decisions, Fraction(999, 1000))),
        }
    else:
        pcts = {"p50": None, "p99": None, "p99_9": None}
    n_comp = len(completed_decisions)
    warning = bool(n_comp > 0 and Fraction(over_8192, n_comp) > Fraction(1, 1000))
    breakdown = {
        "by_stop_kind": dict(sorted(by_stop_kind.items())),
        "by_last_action_source": dict(sorted(by_last_source.items())),
        "by_card_charged": {str(k): v for k, v in sorted(by_card_charged.items())},
        "by_card_present_in_halts": {
            str(k): {"completed": v[0], "halted": v[1]}
            for k, v in sorted(by_card_present.items()) if v[1] > 0
        },
        "by_play_draw": by_play_draw,
        "by_arm": dict(sorted(by_arm.items())),
        "decision_percentiles_completed": pcts,
        "completed_over_8192": over_8192,
    }
    return breakdown, warning


def _denominators(read: str, layout: str, unit_list: list[Any], raw: dict[str, dict[str, Any]],
                  scoped: dict[int, list[dict[str, Any]]]) -> dict[str, dict[str, int]]:
    """A6.3 per-statistic exact denominators: units, pools/rows and games."""
    out: dict[str, dict[str, int]] = {}
    g_tot = _games_from_scoped(scoped)
    if layout == "a":
        for name, row_key in (("candidate_edge", "candidate"), ("stated_edge", "stated")):
            rows_planned = len(unit_list)
            rows_empty = sum(all(w is None for w in u[row_key]) for u in unit_list)
            gp = sum(len(u[row_key]) for u in unit_list)
            gc = sum(sum(w is not None for w in u[row_key]) for u in unit_list)
            out[name] = {
                "units_planned": len(unit_list),
                "units_defined": raw[name]["drop"]["units_defined"],
                "units_omitted": len(unit_list) - raw[name]["drop"]["units_defined"],
                "rows_planned": rows_planned,
                "rows_complete": rows_planned - rows_empty,
                "rows_empty": rows_empty,
                "games_planned": gp,
                "games_completed": gc,
                "games_halted": gp - gc,
            }
        both_complete = sum(
            (not all(w is None for w in u["candidate"])) and (not all(w is None for w in u["stated"]))
            for u in unit_list
        )
        out["interaction"] = {
            "units_planned": len(unit_list),
            "units_defined": raw["interaction"]["drop"]["units_defined"],
            "units_omitted": len(unit_list) - raw["interaction"]["drop"]["units_defined"],
            "rows_planned": len(unit_list),
            "rows_complete": both_complete,
            "rows_empty": len(unit_list) - both_complete,
            "games_planned": g_tot["played"],
            "games_completed": g_tot["completed"],
            "games_halted": g_tot["halted"],
        }
        return out
    if layout == "b":
        pools = [p for u in unit_list for p in u]
        pools_empty = sum(all(w is None for w in p) for p in pools)
        name = next(iter(raw))
        out[name] = {
            "units_planned": len(unit_list),
            "units_defined": raw[name]["drop"]["units_defined"],
            "units_omitted": len(unit_list) - raw[name]["drop"]["units_defined"],
            "pools_planned": len(pools),
            "pools_complete": len(pools) - pools_empty,
            "pools_empty": pools_empty,
            "games_planned": g_tot["played"],
            "games_completed": g_tot["completed"],
            "games_halted": g_tot["halted"],
        }
        return out
    if read == "b_prime":
        pools = [y for u in unit_list for _, y in u]
        pools_empty = sum(all(w is None for w in p) for p in pools)
        name = next(iter(raw))
        out[name] = {
            "units_planned": len(unit_list),
            "units_defined": raw[name]["drop"]["units_defined"],
            "units_omitted": len(unit_list) - raw[name]["drop"]["units_defined"],
            "pools_planned": len(pools),
            "pools_complete": len(pools) - pools_empty,
            "pools_empty": pools_empty,
            "games_planned": g_tot["played"],
            "games_completed": g_tot["completed"],
            "games_halted": g_tot["halted"],
        }
        return out
    if layout == "c":
        pools = [pair for u in unit_list for pair in u]
        pools_defined = sum(
            (not all(w is None for w in x)) and (not all(w is None for w in y)) for x, y in pools
        )
        pools_empty = sum(all(w is None for w in x + y) for x, y in pools)
        name = next(iter(raw))
        out[name] = {
            "units_planned": len(unit_list),
            "units_defined": raw[name]["drop"]["units_defined"],
            "units_omitted": len(unit_list) - raw[name]["drop"]["units_defined"],
            "pools_planned": len(pools),
            "pools_complete": pools_defined,
            "pools_empty": pools_empty,
            "games_planned": g_tot["played"],
            "games_completed": g_tot["completed"],
            "games_halted": g_tot["halted"],
        }
        return out
    if layout == "one":
        name = next(iter(raw))
        out[name] = {
            "units_planned": len(unit_list),
            "units_defined": raw[name]["drop"]["units_defined"],
            "units_omitted": len(unit_list) - raw[name]["drop"]["units_defined"],
            "games_planned": g_tot["played"],
            "games_completed": g_tot["completed"],
            "games_halted": g_tot["halted"],
        }
        return out
    return out


def _aa_pair_by_row(unit_list: list[dict[str, list[Fraction | None]]], units: range) -> list[dict[str, Any]]:
    """A6.12: report drop scores and both signed completions by pair and builder row."""
    rows = []
    for pair_idx, u in zip(units, unit_list):
        cand_n = len(u["candidate"])
        cand_c = sum(w is not None for w in u["candidate"])
        stat_n = len(u["stated"])
        stat_c = sum(w is not None for w in u["stated"])
        rows.append({
            "pair": pair_idx,
            "candidate": {
                "drop": _num(stat_a_candidate(u, "drop")),
                "lower": _num(stat_a_candidate(u, "lower")),
                "upper": _num(stat_a_candidate(u, "upper")),
                "games_planned": cand_n,
                "games_completed": cand_c,
                "games_halted": cand_n - cand_c,
            },
            "stated": {
                "drop": _num(stat_a_stated(u, "drop")),
                "lower": _num(stat_a_stated(u, "lower")),
                "upper": _num(stat_a_stated(u, "upper")),
                "games_planned": stat_n,
                "games_completed": stat_c,
                "games_halted": stat_n - stat_c,
            },
            "interaction": {
                "drop": _num(stat_a_interaction(u, "drop")),
                "lower": _num(stat_a_interaction(u, "lower")),
                "upper": _num(stat_a_interaction(u, "upper")),
            },
        })
    return rows


def _basics_band(count: int) -> str:
    for label, lo, hi in BASICS_BANDS:
        if lo <= count <= hi:
            return label
    raise ReadError(f"basics count {count} outside [0, 40]")


def _dose_first_basics_bands(records: dict[int, dict[str, Any]], plans: dict[int, dict[str, Any]],
                             timing: dict[int, dict[int, dict[str, Any]]],
                             basic_ids: list[int]) -> dict[str, Any]:
    """A6.17 & D0.20 first-dose timing and decision-cap hit rates in the four signed basics bands."""
    basics_set = set(basic_ids)
    buckets: dict[str, dict[str, Any]] = {
        label: {"games": 0, "completed": 0, "halted": 0, "decision_cap_hits": 0, "ms": []}
        for label, _, _ in BASICS_BANDS
    }
    all_bucket: dict[str, Any] = {"games": 0, "completed": 0, "halted": 0, "decision_cap_hits": 0, "ms": []}
    for pid, rec in sorted(records.items()):
        plan = plans[pid]
        pod_timing = timing.get(pid)
        if pod_timing is None:
            raise ReadError(f"first dose missing timing side file for pod {pid}")
        decks_by_id = {d["deck"]: d for d in rec["decks"]}
        for g in rec["games"]:
            gid = g["game"]
            if gid not in pod_timing:
                raise ReadError(f"first dose missing timing entry for pod {pid} game {gid}")
            elapsed = pod_timing[gid]["elapsed_ms"]
            focal = next((d for d in g["decks"] if plan["decks"][d]["source"] == "build"), None)
            if focal is None:
                raise ReadError(f"dose pod {pid} game {gid} has no built focal deck")
            b_count = sum(1 for cid in decks_by_id[focal]["cards"] if cid in basics_set)
            band = _basics_band(b_count)
            att = g["attempts"][0]
            halted = g["scores"] is None
            cap_hit = halted and (att["stop_kind"] in DECISION_CAP_STOP_KINDS)
            for b in (buckets[band], all_bucket):
                b["games"] += 1
                if halted:
                    b["halted"] += 1
                else:
                    b["completed"] += 1
                if cap_hit:
                    b["decision_cap_hits"] += 1
                b["ms"].append(elapsed)

    def summarize(b: dict[str, Any]) -> dict[str, Any]:
        ms_sorted = sorted(Fraction(x) for x in b["ms"])
        n = len(ms_sorted)
        if n > 0:
            total = sum(b["ms"])
            timing_summary = {
                "count": n,
                "total_ms": total,
                "mean_ms": _num(Fraction(total, n)),
                "p50_ms": _num(percentile(ms_sorted, Fraction(1, 2))),
                "p99_ms": _num(percentile(ms_sorted, Fraction(99, 100))),
                "max_ms": max(b["ms"]),
            }
        else:
            timing_summary = {"count": 0, "total_ms": 0, "mean_ms": None, "p50_ms": None,
                              "p99_ms": None, "max_ms": None}
        rate = _num(Fraction(b["decision_cap_hits"], b["games"])) if b["games"] > 0 else None
        return {
            "games": b["games"],
            "completed": b["completed"],
            "halted": b["halted"],
            "decision_cap_hits": b["decision_cap_hits"],
            "decision_cap_hit_rate": rate,
            "timing": timing_summary,
        }

    return {
        "bands": {label: summarize(buckets[label]) for label, _, _ in BASICS_BANDS},
        "all": summarize(all_bucket),
    }


def run_read(declaration: dict[str, Any], decl_sha: str, read: str, look: int,
             sets: list[dict[str, Any]], *, a6_file: Path | bytes | None = None,
             peer_sets: list[dict[str, Any]] | None = None, expected_pilot: Any = None,
             expected_picker: Any = None, expected_builders: dict[str, Any] | None = None,
             d0_sha256: str | None = None,
             shakedown_manifest: tuple[Path | bytes, str] | None = None) -> dict[str, Any]:
    """One read's report. `sets` are read_sets.open_plan_set results: one set, or for a gate's look 2
    the look-1 and look-2 sets. For dose and D13 the look is the mark ordinal (D13 may take one set
    per pilot)."""
    if read not in READS:
        raise ReadError(f"unknown read {read!r}")
    family = READ_FAMILY[read]
    sec_name = "c" if read == "b_prime" else read
    section = declaration["reads"][sec_name]
    namespace = section["namespace"]
    _require_signed(declaration, sec_name, namespace, a6_file)
    for s in sets:
        man = s["manifest"]
        if man["family"] != family or man["namespace"] != namespace:
            raise ReadError(f"set {man['set']} is family {man['family']} in {man['namespace']}, "
                            f"not {family} in {namespace}")
        if s.get("plans"):
            b = read_sets._set_bindings(s["plans"])
            if expected_pilot is not None and b["pilot"] != expected_pilot:
                raise ReadError(f"set {man['set']} pilot differs from declared pilot")
            if expected_picker is not None and b["picker"] != expected_picker:
                raise ReadError(f"set {man['set']} picker differs from declared picker")
            if expected_builders is not None and b["builders"] != expected_builders:
                raise ReadError(f"set {man['set']} builders differ from declared builders")
        read_sets.verify_set_restarts(
            s, n_b=declaration["reads"]["b"]["n"], n_c=declaration["reads"]["c"]["n"],
        )
    _check_declared_pins(declaration, family, sets)
    if peer_sets is not None:
        if read not in ("b", "c", "b_prime"):
            raise ReadError(f"peer_sets is only valid for reads b, c, or b_prime (not {read!r})")
        peer_family = "c" if read == "b" else "b"
        peer_ns = declaration["reads"][peer_family]["namespace"]
        for ps in peer_sets:
            pman = ps["manifest"]
            if pman["family"] != peer_family or pman["namespace"] != peer_ns:
                raise ReadError(f"peer set {pman['set']} is family {pman['family']} in {pman['namespace']}, "
                                f"not {peer_family} in {peer_ns}")
            read_sets.verify_set_restarts(
                ps, n_b=declaration["reads"]["b"]["n"], n_c=declaration["reads"]["c"]["n"],
            )
        _check_declared_pins(declaration, peer_family, peer_sets)
        if read == "b":
            read_sets.check_shared_bc_prefix(sets, peer_sets)
        else:
            read_sets.check_shared_bc_prefix(peer_sets, sets)
    report: dict[str, Any] = {
        "schema": REPORT_SCHEMA, "interfaces": pod_record.INTERFACES_VERSION, "read_plan": read_plans.PLAN_SCHEMA,
        "read": read, "family": family, "look": look, "declaration_sha256": decl_sha,
        "a6_sha256": declaration["a6_sha256"], "namespace": namespace, "sets": [_set_info(s) for s in sets],
    }
    voids = [s["void"] for s in sets if s["void"]]
    if voids:
        reason = voids[0]
        next_step = "attempt_ends" if "infrastructure failure" in reason else "technical recovery (A6.7)"
        records_all: dict[int, dict[str, Any]] = {}
        planned_games = 0
        for s in sets:
            records_all.update(s["records"])
            planned_games += sum(len(p["games"]) for p in s["plans"].values())
        g_counts = _games(records_all)
        g_counts["planned"] = planned_games
        g_counts["unplayed"] = planned_games - g_counts["played"]
        report.update(verdict="void", void=reason, next=next_step, games=g_counts)
        return report

    resamples = declaration["resamples"]
    level: Fraction | None = None
    if read in GATES or read == "b_prime":
        n = look_units(section, look)
        expected_sets = 1 if look == 1 else 2
        if len(sets) != expected_sets:
            raise ReadError(f"look {look} reads {expected_sets} set(s)")
        read_sets.check_look_allocation(sets[0]["manifest"], 1, section["n"])
        if look == 2:
            read_sets.check_look_allocation(sets[1]["manifest"], 2, section["n"], sets[0]["manifest"])
            read_sets.check_look_bindings(sets[0], sets[1])
        first = sets[0]["manifest"]["units"][0]
        units = range(first, first + n)
        level = section["levels"][look - 1]
    elif read == "aa":
        if look != 1 or len(sets) != 1:
            raise ReadError("the A/A has one fixed look and one set")
        units = range(*sets[0]["manifest"]["units"])
        if len(units) != section["pairs"]:
            raise ReadError(f"the A/A has {section['pairs']} pairs")
        level = section["level"]
    elif read in ("dose", "d13"):
        units = range(*sets[0]["manifest"]["units"])
        if len(units) != section["panel_drafts"]:
            raise ReadError(f"the panel has {section['panel_drafts']} drafts")
        marks = {p["mark"]["ordinal"] for s in sets for p in s["plans"].values()}
        if read == "dose" and (len(sets) != 1 or marks != {look}):
            raise ReadError("a dose read is one set of plans at the look's mark ordinal")
        level = section["level"]
    elif read in COMPANION_PARENT:
        parent = declaration["reads"][COMPANION_PARENT[read]]
        if parent["n"] is None or len(sets) != 1:
            raise ReadError("a companion reads its parent's first-look units, in one set")
        units = range(*sets[0]["manifest"]["units"])
        if len(units) != parent["n"]:
            raise ReadError(f"a companion has the parent's {parent['n']} first-look units")
        if read_plans.FAMILIES[family]["layout"] == "one":
            level = section["level"]
    else:
        units = range(*sets[0]["manifest"]["units"])
        if len(units) != section["units"] or len(sets) != 1:
            raise ReadError(f"the probe has {section['units']} units in one set")
        level = section["level"]

    records: dict[int, dict[str, Any]] = {}
    plans: dict[int, dict[str, Any]] = {}
    for s in sets:
        records.update(s["records"])
        plans.update(s["plans"])
    report["units"] = {"kind": "pair" if read_plans.FAMILIES[family]["layout"] == "a" else "cluster",
                       "first": units.start, "end": units.stop}
    if level is not None:
        report["bootstrap"] = {"namespace": namespace, "resamples": resamples, "level": _num(level)}
    scoped = _scoped_games(read, records)
    report["games"] = _games_from_scoped(scoped)
    a2_3, a2_1_warn = _halts_and_lengths(read, records, plans, scoped)
    report["a2_3"] = a2_3
    report["a2_1_warning"] = a2_1_warn
    all_halted = report["games"]["played"] > 0 and report["games"]["completed"] == 0

    def block(stats: dict[str, Callable], unit_list: list[Any]) -> dict[str, dict[str, Any]]:
        series = {(name, sc): [fn(u, sc) for u in unit_list] for name, fn in stats.items() for sc in SCORINGS}
        boot = bootstrap(series, namespace, resamples, level)
        return {name: {sc: boot[(name, sc)] for sc in SCORINGS} for name in stats}

    layout = read_plans.FAMILIES[family]["layout"]
    if layout == "a":
        unit_list = units_a(records, plans, units)
        stats = {"candidate_edge": stat_a_candidate, "stated_edge": stat_a_stated,
                 "interaction": stat_a_interaction}
    elif layout == "b":
        unit_list = units_b(records, units)
        stats = {"candidate_score": stat_pools}
    elif read == "b_prime":
        unit_list = units_paired(records, units)
        stats = {"stated_score": stat_y}
    elif layout == "c":
        unit_list = units_paired(records, units)
        stats = {"current_minus_initial" if family == "dose" else "candidate_minus_stated": stat_paired}
    elif layout == "one":
        unit_list = units_one(records, units)
        stats = {"single_seat_edge": stat_edge}
    elif layout == "all":
        report["denominators"] = {
            "completion": {
                "units_planned": len(units),
                "units_defined": sum(any(g["scores"] is not None for g in records[i]["games"]) for i in units),
                "units_omitted": sum(all(g["scores"] is None for g in records[i]["games"]) for i in units),
                "games_planned": report["games"]["played"],
                "games_completed": report["games"]["completed"],
                "games_halted": report["games"]["halted"],
            }
        }
        report["verdict"] = "void" if all_halted else None
        if all_halted:
            report["void"] = "all games halted"
        report["completion"] = {"games": report["games"],
                                "mean_decisions_completed": _num(_mean([Fraction(g["attempts"][0]["decisions"])
                                                                        for r in records.values()
                                                                        for g in r["games"]
                                                                        if g["scores"] is not None]))}
        return report
    else:
        return _read_d13(report, sets, units, namespace, resamples, level, all_halted=all_halted)

    raw = block(stats, unit_list)
    report["statistics"] = {name: {sc: _render(raw[name][sc]) for sc in SCORINGS} for name in raw}
    report["denominators"] = _denominators(read, layout, unit_list, raw, scoped)
    if layout in ("b", "c") and read != "dose":
        pools = [p for u in unit_list for p in (u if layout == "b" else [y for _, y in u] if read == "b_prime"
                                                else [x + y for x, y in u])]
        report["pools"] = {"planned": len(pools), "empty": sum(all(w is None for w in p) for p in pools)}

    if read == "dose":
        if d0_sha256 is not None and not read_plans._is_sha(d0_sha256):
            raise ReadError("d0_sha256 must be a sha256 hex digest")
        if namespace == "dose-d8" and d0_sha256 is None:
            raise ReadError("dose-d8 requires D0 provenance (d0_sha256)")
        man0 = sets[0]["manifest"]
        expected_panel = read_sets.resolve_pinned_panel(man0, section.get("panel_sha256"), shakedown_manifest)
        if man0["panel_sha256"] != expected_panel:
            raise ReadError(
                f"dose set panel_sha256 {man0['panel_sha256']!r} differs from pinned panel {expected_panel!r} "
                f"for run {man0['run']}"
            )
        mark0 = next(iter(plans.values()))["mark"]
        prov = {
            "label": "shakedown, under random play, provisional pins" if namespace in ("dose-d8", TEST) else "m5",
            "d0_sha256": d0_sha256,
            "panel_sha256": man0["panel_sha256"],
            "panel_run": man0["run"],
            "mark": mark0,
            "panel_pairing_note": "marks on different panels are not paired",
        }
        if shakedown_manifest is not None:
            prov["shakedown_manifest_sha256"] = shakedown_manifest[1]
        report["provenance"] = prov
        if look == 0:
            timing0 = sets[0].get("timing")
            basic_ids0 = sets[0].get("basic_ids")
            if timing0 is None:
                raise ReadError("first dose (ordinal 0) requires timing side files exec-<e>/timing/pod-<pod_index>.jsonl")
            if basic_ids0 is None:
                raise ReadError("first dose (ordinal 0) requires pool basic land ids for basics bands")
            report["first_dose_basics_bands"] = _dose_first_basics_bands(records, plans, timing0, basic_ids0)

    if read in GATES:
        primary = next(iter(stats))
        verdicts = {sc: verdict_of(raw[primary][sc]["interval"], section["cut"]) for sc in SCORINGS}
        report["cut"] = _num(section["cut"])
        report["verdicts_by_scoring"] = verdicts
        report.update(disposition(verdicts, look))
        if report["next"] == "extension":
            n0 = section["n"]
            report["extension"] = {"fresh_units": [first + n0, first + 3 * n0], "pooled_units": 3 * n0}
    elif read == "aa":
        report["symmetry"] = aa_symmetry(records, units)
        report["pair_by_row"] = _aa_pair_by_row(unit_list, units)
        zero = all(raw[name]["drop"]["estimate"] == 0 for name in raw)
        report["verdict"] = "symmetric" if report["symmetry"]["symmetric"] and zero else "failed"
        report["next"] = "symmetry receipt" if report["verdict"] == "symmetric" else "to the lead (A6.12)"
    elif read in SIZED_GATE:
        gate = declaration["reads"][SIZED_GATE[read]]
        primary = next(iter(stats))
        report["sizing"] = {sc: size_from_probe([stats[primary](u, sc) for u in unit_list], gate["sizing"])
                            for sc in SCORINGS}
        report["count"] = report["sizing"]["drop"]
        if read == "size_c":
            report["diagnostics"] = _c_diagnostics(unit_list)
        report["verdict"] = "void" if all_halted else None
        if all_halted:
            report["void"] = "all games halted"
    else:
        report["drop_interval_undefined"] = any(raw[name]["drop"]["interval"] is None for name in raw)
        if all_halted:
            report["verdict"] = "void"
            report["void"] = "all games halted"
        else:
            report["verdict"] = None
    return report


def _c_diagnostics(unit_list: list[list[tuple[list, list]]]) -> dict[str, Any]:
    """(c) probe diagnostics (A6.18): pool and draft spreads, within-draft correlation, and the
    descriptive spread of the stated half for (b')."""
    def variance(vals: list[Fraction]) -> Fraction | None:
        if len(vals) < 2:
            return None
        m = sum(vals, Fraction(0)) / len(vals)
        return sum(((v - m) ** 2 for v in vals), Fraction(0)) / (len(vals) - 1)

    pools = [d for u in unit_list for d in (stat_paired([p], "drop") for p in u) if d is not None]
    drafts = [v for v in (stat_paired(u, "drop") for u in unit_list) if v is not None]
    stated = [v for v in (stat_y(u, "drop") for u in unit_list) if v is not None]
    vp, vd = variance(pools), variance(drafts)
    rho = None
    if vp and vd is not None:
        rho = (8 * vd / vp - 1) / 7
    return {"pool_variance": _num(vp), "draft_variance": _num(vd), "within_draft_correlation": _num(rho),
            "stated_draft_variance": _num(variance(stated))}


def _read_d13(report: dict[str, Any], sets: list[dict[str, Any]], units: range, namespace: str, resamples: int,
              level: Fraction, *, all_halted: bool = False) -> dict[str, Any]:
    """D13: per contrast q, the mean of pool-opponent differences x minus y, draft-clustered, with
    both completions, per pilot set; and cross-pilot sign agreement on jointly defined differences."""
    per_pilot = []
    report["pilots"] = []
    any_undef = False
    for s in sets:
        contrasts = {}
        diffs = {}
        denoms = {}
        for q, kind, _, _ in read_plans.D13_CONTRASTS:
            unit_list = units_paired(s["records"], units, per_opponent=True, q=q)
            boot = bootstrap({sc: [stat_paired(u, sc) for u in unit_list] for sc in SCORINGS}, namespace,
                             resamples, level)
            if boot["drop"]["interval"] is None:
                any_undef = True
            contrasts[kind] = {sc: _render(boot[sc]) for sc in SCORINGS}
            diffs[q] = [stat_paired([p], "drop") for u in unit_list for p in u]
            pools = [pair for u in unit_list for pair in u]
            pools_def = sum(
                (not all(w is None for w in x)) and (not all(w is None for w in y)) for x, y in pools
            )
            pools_empty = sum(all(w is None for w in x + y) for x, y in pools)
            gp = sum(len(x) + len(y) for x, y in pools)
            gc = sum(sum(w is not None for w in x + y) for x, y in pools)
            denoms[kind] = {
                "units_planned": len(unit_list),
                "units_defined": boot["drop"]["units_defined"],
                "units_omitted": len(unit_list) - boot["drop"]["units_defined"],
                "pools_planned": len(pools),
                "pools_complete": pools_def,
                "pools_empty": pools_empty,
                "games_planned": gp,
                "games_completed": gc,
                "games_halted": gp - gc,
            }
        mark = next(iter(s["plans"].values()))["mark"]
        report["pilots"].append({"mark": mark, "contrasts": contrasts, "denominators": denoms})
        per_pilot.append(diffs)
    agreement = []
    for a in range(len(per_pilot)):
        for b in range(a + 1, len(per_pilot)):
            for q, kind, _, _ in read_plans.D13_CONTRASTS:
                pairs = [(x, y) for x, y in zip(per_pilot[a][q], per_pilot[b][q]) if x is not None and y is not None]
                same = sum((x > 0) - (x < 0) == (y > 0) - (y < 0) for x, y in pairs)
                agreement.append({"pilots": [a, b], "contrast": kind, "same_sign": same, "jointly_defined": len(pairs)})
    report["agreement"] = agreement
    report["drop_interval_undefined"] = any_undef
    if all_halted:
        report["verdict"] = "void"
        report["void"] = "all games halted"
    else:
        report["verdict"] = None
    return report


# ---------------------------------------------------------------------------------------------
# Output and the command line


def serialize_report(report: dict[str, Any]) -> str:
    return json.dumps(report, indent=2, ensure_ascii=True, allow_nan=False) + "\n"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Read cube M4 read sets into one read report.")
    parser.add_argument("--declaration", required=True, type=Path)
    parser.add_argument("--read", required=True, choices=READS)
    parser.add_argument("--look", required=True, type=int, help="1 or 2 for a gate; the mark ordinal for dose")
    parser.add_argument("--sets", required=True, nargs="+", type=Path, help="set directories, look 1 first")
    parser.add_argument("--peer-sets", nargs="+", type=Path,
                        help="peer (b)/(c) set directories for shared-prefix verification")
    parser.add_argument("--a6-file", type=Path, help="countersigned A6 declaration file (required for live reads)")
    parser.add_argument("--d0-sha256", help="D0 declaration sha256 for dose-d8 provenance")
    parser.add_argument("--shakedown-manifest", type=Path, help="D0.3b shakedown manifest for per-run panel digests")
    parser.add_argument("--shakedown-manifest-sha256", help="D0.3b sha256 pin for --shakedown-manifest")
    parser.add_argument("--pool", type=Path)
    parser.add_argument("--constants", type=Path)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args(argv)
    if (args.shakedown_manifest is None) != (args.shakedown_manifest_sha256 is None):
        raise ReadError("--shakedown-manifest and --shakedown-manifest-sha256 must be given together")
    sm = (args.shakedown_manifest, args.shakedown_manifest_sha256) if args.shakedown_manifest else None
    declaration, decl_sha = load_declaration(args.declaration)
    pool = read_plans.load_json(args.pool) if args.pool else None
    constants = read_plans.load_json(args.constants) if args.constants else None
    sec = declaration["reads"]["c" if args.read == "b_prime" else args.read]
    want_decl = declaration["a6_sha256"] or decl_sha
    sets = [read_sets.open_plan_set(d, want_decl, pool=pool, constants=constants,
                                    pinned_panel=sec.get("panel_sha256"),
                                    expected_pins=declaration.get("pins"),
                                    shakedown_manifest=sm) for d in args.sets]
    peer_sets = ([read_sets.open_plan_set(d, want_decl, pool=pool, constants=constants,
                                          expected_pins=declaration.get("pins")) for d in args.peer_sets]
                 if args.peer_sets else None)
    text = serialize_report(run_read(declaration, decl_sha, args.read, args.look, sets,
                                     a6_file=args.a6_file, peer_sets=peer_sets,
                                     d0_sha256=args.d0_sha256, shakedown_manifest=sm))
    if args.out is None:
        sys.stdout.write(text)
    else:
        args.out.write_bytes(text.encode("utf-8"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
