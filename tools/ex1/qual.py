"""EX1 design v3.3 section 7 item 7: qualification runs of the EX1 engine (collection hook, adapter, update).

Builds RT1 run configs (`native_expanded_training_run_v1`) from an OH1/OH2 base and a dose parent, with episode ids
and deal seeds in a qualification namespace, and summarizes finished runs. Nothing here is a Stage P or Stage F run.

  qual.py config --base BASE.json --parent DOSE.json --out CONFIG.json --root RUN_DIR --run RUN --offset U
                 --updates N [--workers W] [--ex1 --coefficient C --kappa K] [--namespace NS]
  qual.py summarize RUN_DIR [RUN_DIR ...]      per-update CPU, wall, rows, searches, strata, cap receipts
  qual.py strata RUN_DIR [RUN_DIR ...]         realized decisions and triggers per stratum against n(S) and p(S)
  qual.py calibrate RUN_DIR [RUN_DIR ...]      c* = r* / median raw ratio over updates with rows (B2 section 3)
  qual.py check RUN_DIR [RUN_DIR ...]          the check run's median applied ratio against [r*/3, 3 r*]
  qual.py mirror RUN_DIR [RUN_DIR ...]         every recorded target against the Python mirror, bit for bit
  qual.py compare RUN_A RUN_B                  same states, trajectories and EX1 records (timings aside)
  qual.py hookoff RUN_X RUN_O                  identical trajectories with the hook on and off
"""
import argparse
import copy
import hashlib
import json
import math
import statistics
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import target_ref  # noqa: E402

N_S = {"target": 6.46, "cast_or_activate": 32.17, "other": 38.30}
P_S = {"target": 0.155, "cast_or_activate": 0.0311, "other": 0.0261}
R_STAR = 0.3
KAPPA = 1.0
MASK = (1 << 63) - 1


ZSTD_MAGIC = b"\x28\xb5\x2f\xfd"
RECORD_CAP = 512 * 1024 * 1024  # the trainer's per-file bound (MAX_FILE_BYTES)


def zstd_frame_content_size(data, cap=RECORD_CAP):
    """The declared content size of `data`, which must be exactly one zstd frame (the record_envelope_v1 rule):
    the frame header must declare a content size of at most `cap`, and walking the frame's block headers (and its
    checksum, when flagged) must end exactly at the end of `data`. Raises ValueError otherwise."""
    if not data.startswith(ZSTD_MAGIC) or len(data) < 6:
        raise ValueError("not a zstd frame")
    descriptor = data[4]
    fcs_flag, single_segment, checksum, dict_flag = descriptor >> 6, (descriptor >> 5) & 1, (descriptor >> 2) & 1, \
        descriptor & 3
    if descriptor & 0x08:
        raise ValueError("zstd frame header has its reserved bit set")
    at = 5 + (0 if single_segment else 1) + (0, 1, 2, 4)[dict_flag]
    size_bytes = (1 if single_segment else 0, 2, 4, 8)[fcs_flag]
    if size_bytes == 0:
        raise ValueError("a record envelope declares its content size")
    if len(data) < at + size_bytes:
        raise ValueError("truncated zstd frame header")
    size = int.from_bytes(data[at:at + size_bytes], "little") + (256 if size_bytes == 2 else 0)
    if size > cap:
        raise ValueError("record envelope content exceeds its size limit")
    at += size_bytes
    while True:
        if len(data) < at + 3:
            raise ValueError("truncated zstd block header")
        header = int.from_bytes(data[at:at + 3], "little")
        last, kind, block = header & 1, (header >> 1) & 3, header >> 3
        if kind == 3:
            raise ValueError("reserved zstd block type")
        at += 3 + (1 if kind == 1 else block)
        if last:
            break
    at += 4 if checksum else 0
    if at != len(data):
        raise ValueError("a record envelope is exactly one zstd frame")
    return size


def open_record(data, cap=RECORD_CAP):
    """The record bytes of a published file: a sealed record (mtg-kernel `record_envelope_v1`, one zstd frame of
    the record's exact bytes) opened, anything else as it is. Fails closed like the Rust reader: exactly one frame,
    a declared content size within `cap`, output of exactly that length, content not itself sealed. Decodes with
    Python 3.14's compression.zstd, else the zstandard package, else the zstd command line tool."""
    if not data.startswith(ZSTD_MAGIC):
        return data
    size = zstd_frame_content_size(data, cap)
    try:
        from compression import zstd
        decoder = zstd.ZstdDecompressor()
        content = decoder.decompress(data, max_length=size + 1)
        if not decoder.eof or decoder.unused_data:
            raise ValueError("a record envelope is exactly one zstd frame")
    except ImportError:
        try:
            import zstandard
            content = zstandard.ZstdDecompressor().decompress(data, max_output_size=size)
        except ImportError:
            with tempfile.TemporaryDirectory() as tmp:
                source = Path(tmp) / "record.zst"
                source.write_bytes(data)
                proc = subprocess.Popen(["zstd", "-q", "-d", "-c", str(source)], stdout=subprocess.PIPE,
                                        stderr=subprocess.DEVNULL)
                content = proc.stdout.read(size + 1)  # never more than one byte past the declared size
                if len(content) > size:
                    proc.kill()
                proc.stdout.close()
                if proc.wait() != 0 and len(content) <= size:
                    raise ValueError("zstd could not decode the record envelope")
    if len(content) != size:
        raise ValueError("record envelope content length differs from its frame")
    if content.startswith(ZSTD_MAGIC):
        raise ValueError("a record envelope holds unsealed record bytes only")
    return content


def read(path):
    return json.loads(open_record(Path(path).read_bytes()).decode("utf-8"))


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def seed(parts):
    text = json.dumps(list(parts), separators=(",", ":"))
    return int.from_bytes(hashlib.sha256(text.encode()).digest()[:8], "little") & MASK


KEEP_OFFSET_SHAPES = ("linear-v1",)
KEEP_OFFSET_KEYS = frozenset({"b0", "end_update", "shape"})
KEEP_OFFSET_MAX = 1.0e6  # the trainer's london_keep_offset_v1::MAX_OFFSET_V1
KEEP_OFFSET_MAX_END = 1 << 53  # (T - u) stays exact in binary64
KEEP_OFFSET_OPENING = "london-v1"  # the one opening protocol the offset belongs to
KEEP_OFFSET_MODES = ("learned", "keep_seven")
KEEP_OFFSET_COUNTS = 7  # a per-count lean vector: entry k applies to an announcement with k mulligans already taken
PREGAME_ADVANTAGE_MODES = ("terminal_v1",)  # the trainer's loss_selection.pregame_advantage; absent = unchanged GAE
# Run options the trainer refuses together with an offset ("london_keep_offset trains only on the plain CPU GAE
# update"): a base that sets any of them would collect games and then fail at its first update.
KEEP_OFFSET_REFUSED_OPTIONS = ("line_b", "e_supported", "soft_q", "search_targets", "ex1")


def _is_number(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def _float32(value):
    """`value` rounded once to the nearest IEEE binary32, as the Python float equal to it."""
    return struct.unpack("<f", struct.pack("<f", value))[0]


def validate_keep_offset(keep_offset, base=None):
    """The checked, normalized keep offset `{"b0": float or [7 floats], "end_update": int, "shape": "linear-v1"}` (a
    new dict). Raises ValueError for anything that is not exactly that form: a non-mapping, missing or unknown keys,
    end_update not an int in [1, 2^53] (a bool is neither), or an unknown shape.

    `b0` is either one number (the scalar lean: finite, in (0, 1e6]) or a list of exactly 7 numbers, entry k being the
    lean of an announcement with k mulligans already taken by the acting seat (each finite, in [0, 1e6], at least one
    above 0; a zero entry means no lean at that count): the trainer's own validation of the stamped values.

    With `base` (an OH1/OH2 base), also raises unless the base is one the trainer can train with an offset (see
    `check_london_base`): every episode of every iteration has the London opening and its mulligan modes (the
    trainer refuses an update batch that mixes offset and no offset), no episode already carries a
    `london_keep_offset` (the schedule is the only writer of that field), and the update is the plain CPU GAE one
    (no line_b, e_supported, soft_q, search_targets or ex1 block)."""
    if not isinstance(keep_offset, dict):
        raise ValueError("keep_offset must be an object {b0, end_update, shape}")
    unknown = sorted(str(k) for k in set(keep_offset) - KEEP_OFFSET_KEYS)
    if unknown:
        raise ValueError(f"keep_offset has unknown keys {unknown}")
    missing = sorted(KEEP_OFFSET_KEYS - set(keep_offset))
    if missing:
        raise ValueError(f"keep_offset lacks {missing}")
    b0, end, shape = keep_offset["b0"], keep_offset["end_update"], keep_offset["shape"]
    # The range test comes first: it is exact for any int, while isfinite(10**400) overflows instead of refusing.
    if isinstance(b0, list):
        if len(b0) != KEEP_OFFSET_COUNTS:
            raise ValueError(f"keep_offset b0 as a list needs exactly {KEEP_OFFSET_COUNTS} entries (one per number of "
                             f"mulligans taken), got {len(b0)}")
        for k, entry in enumerate(b0):
            if not (_is_number(entry) and 0 <= entry <= KEEP_OFFSET_MAX and math.isfinite(entry)):
                raise ValueError(f"keep_offset b0[{k}] must be a finite number in [0, {KEEP_OFFSET_MAX:g}], "
                                 f"got {entry!r}")
        if not any(entry > 0 for entry in b0):
            raise ValueError("keep_offset b0 needs at least one entry above 0")
        b0 = [float(entry) + 0.0 for entry in b0]  # + 0.0 turns -0.0 into 0.0 (the trainer refuses a negative zero)
    elif _is_number(b0) and 0 < b0 <= KEEP_OFFSET_MAX and math.isfinite(b0):
        b0 = float(b0)
    else:
        raise ValueError(f"keep_offset b0 must be a finite number in (0, {KEEP_OFFSET_MAX:g}] or a list of "
                         f"{KEEP_OFFSET_COUNTS} numbers in [0, {KEEP_OFFSET_MAX:g}], got {b0!r}")
    if not (type(end) is int and 1 <= end <= KEEP_OFFSET_MAX_END):
        raise ValueError(f"keep_offset end_update must be an integer in [1, 2^53], got {end!r}")
    if shape not in KEEP_OFFSET_SHAPES:
        raise ValueError(f"keep_offset shape must be one of {KEEP_OFFSET_SHAPES}, got {shape!r}")
    if base is not None:
        check_london_base(base)
    return {"b0": b0, "end_update": end, "shape": shape}


def validate_pregame_advantage(mode, base=None):
    """`mode` (the arm's `pregame_advantage`: "terminal_v1", or None for the trainer's unchanged GAE advantage),
    checked. The mode is the trainer's `loss_selection.pregame_advantage`: the advantage and value target of the
    pregame decisions come from the game's own terminal result, in-game rows unchanged. Raises ValueError for any
    other string ("gae" is spelled by leaving the key out, so one setting has one spelling). With `base`, also
    raises unless the trainer can use it: every episode has the London opening (`check_london_base`), the loss is
    `gae_advantage_value_v1` with gamma exactly 1.0 (the trainer refuses any other gamma), and the base's own
    loss_selection names no pregame_advantage (the arm is the only writer of the key)."""
    if mode is None:
        return None
    if mode not in PREGAME_ADVANTAGE_MODES:
        raise ValueError(f"pregame_advantage must be one of {PREGAME_ADVANTAGE_MODES} or absent, got {mode!r}")
    if base is not None:
        check_london_base(base)
        loss = base["loss_selection"]
        if "pregame_advantage" in loss:
            raise ValueError("the base's loss_selection already names pregame_advantage; the arm writes it")
        check_base_names_no_pregame_scale(base)
        gamma = loss.get("gamma")
        if not (_is_number(gamma) and gamma == 1.0):
            raise ValueError(f"pregame_advantage terminal_v1 needs gamma exactly 1.0 in the base's loss_selection, "
                             f"got {gamma!r}")
    return mode


# The earlier, pre-normalisation key. The trainer never took it on a release branch; the tools refuse it by name so an
# arm written for it cannot run unscaled by accident.
RETIRED_PREGAME_SCALE_KEY = "pregame_advantage" + "_scale"


def retired_pregame_scale_message(where):
    return (f"{where} names {RETIRED_PREGAME_SCALE_KEY}, which no trainer takes; the key is "
            "pregame_announcement_step_scale (a scale on the announcement rows' optimizer step, 0 < s < 1)")


def check_base_names_no_pregame_scale(base):
    """Raises ValueError when the base's own loss_selection names a pregame scale key (the arm is the only writer of
    `pregame_announcement_step_scale`; the retired key is never valid). Checked for every config, whatever the arm's
    pregame_advantage is, so a base that carries one cannot pass it through an arm that names none."""
    loss = base.get("loss_selection")
    if not isinstance(loss, dict):
        return
    if "pregame_announcement_step_scale" in loss:
        raise ValueError("the base's loss_selection already names pregame_announcement_step_scale; the arm writes it")
    if RETIRED_PREGAME_SCALE_KEY in loss:
        raise ValueError(retired_pregame_scale_message("the base's loss_selection"))


def validate_pregame_announcement_step_scale(scale, mode):
    """`scale` (the arm's `pregame_announcement_step_scale`, or None), checked: the trainer's loss_selection key, a
    number s with 0 < s < 1 that scales the pregame announcement rows' part of the optimizer step after Adam's
    normalisation (their policy gradient is kept in a second buffer a beside the rest r; first moment from r + s a,
    second moment from (r + a)^2). It exists only with `pregame_advantage` "terminal_v1" (`mode`); absent leaves every
    config, arm file and recipe identity exactly as it was. Raises ValueError for a non-number (a bool is not one), a
    non-finite value, a value outside (0, 1) once read as the trainer's f32 (1.0 is refused: absent is the unscaled
    update), or a scale without the terminal mode."""
    if scale is None:
        return None
    if mode != "terminal_v1":
        raise ValueError("pregame_announcement_step_scale needs pregame_advantage terminal_v1")
    ok = False
    if _is_number(scale):
        try:  # the trainer reads the number as an f32: refuse what it would round to 0 or to 1, and what overflows
            ok = 0.0 < _float32(float(scale)) < 1.0
        except OverflowError:
            ok = False
    if not ok:
        raise ValueError(f"pregame_announcement_step_scale must be a number in (0, 1) exclusive, got {scale!r}")
    return scale


def check_london_base(base):
    """Raises ValueError unless `base` can carry a keep offset: the trainer reads one offset per update batch (one
    iteration), refusing a batch that mixes offset and no offset, so EVERY episode of EVERY iteration needs the
    London opening (`london-v1`) with its `mulligan_modes` (the learner seat's mode is `learned`), none may carry a
    `london_keep_offset` already, a reset start or a search opponent, and the base's loss and update backend must be
    the plain CPU GAE update the offset trains on (an absent loss_selection is the trainer's other default loss)."""
    for i, item in enumerate(base["iterations"]):
        for j, entry in enumerate(item["episodes"]):
            episode = entry["episode"]
            where = f"iteration {i} episode {j}"
            if "london_keep_offset" in episode:
                raise ValueError(f"{where} already carries london_keep_offset; the keep_offset schedule writes it")
            if episode.get("opening") is None:
                raise ValueError(f"keep_offset needs an opening on every episode of the base ({where} has none)")
            if episode["opening"] != KEEP_OFFSET_OPENING:
                raise ValueError(f"{where}: opening {episode['opening']!r} is not {KEEP_OFFSET_OPENING!r}")
            modes, seat = episode.get("mulligan_modes"), episode.get("learner_seat")
            if not (isinstance(modes, list) and len(modes) == 2 and all(m in KEEP_OFFSET_MODES for m in modes)):
                raise ValueError(f"{where}: an opening needs mulligan_modes, two of {KEEP_OFFSET_MODES}, got {modes!r}")
            if not (type(seat) is int and seat in (0, 1) and modes[seat] == "learned"):
                raise ValueError(f"{where}: the learner seat {seat!r} needs the learned mulligan mode, got {modes!r}")
            for key in ("reset", "opponent_search"):
                if episode.get(key) is not None:
                    raise ValueError(f"{where}: an opening is refused with {key}")
    loss, backend = base.get("loss_selection"), base.get("update_backend", {"kind": "cpu"})
    if not (isinstance(loss, dict) and loss.get("kind") == "gae_advantage_value_v1"):
        raise ValueError("keep_offset trains only on the gae_advantage_value_v1 loss (the base's loss_selection)")
    if not (isinstance(backend, dict) and backend.get("kind") == "cpu"):
        raise ValueError("keep_offset trains only on the CPU update backend (the base's update_backend)")
    for key in KEEP_OFFSET_REFUSED_OPTIONS:
        if base.get(key) is not None:
            raise ValueError(f"keep_offset trains only on the plain CPU GAE update: the base's {key} is refused")


def keep_offset_value(keep_offset, u):
    """The `london_keep_offset` an opening episode carries at fork update `u`, or None from `end_update` on (no
    field at all). b(u) = float32(float64(b0) * (T - u) / T), computed in binary64 and rounded once to the nearest
    binary32, returned as the Python float equal to it (so it round-trips through struct 'f' unchanged). With a
    vector `b0` the result is the list of its 7 entries so computed (an entry of 0 stays 0.0). `u` counts
    updates from the fork point: the chain tools number a chain's updates from 0 at its first segment, whatever
    update its parent had reached, so no start offset exists. Raises ValueError for a negative or non-integer `u`,
    or for a value the trainer would refuse (a scalar not finite and positive after rounding, a vector entry above
    0 that rounds to 0, or any value above 1e6)."""
    keep_offset = validate_keep_offset(keep_offset)
    if type(u) is not int or u < 0:
        raise ValueError(f"keep_offset update must be a non-negative integer, got {u!r}")
    end = keep_offset["end_update"]
    if u >= end:
        return None
    b0 = keep_offset["b0"]
    if isinstance(b0, list):
        values = [_float32(entry * (end - u) / end) for entry in b0]
        for k, (entry, value) in enumerate(zip(b0, values)):
            if not (math.isfinite(value) and 0.0 <= value <= KEEP_OFFSET_MAX and (value > 0.0 or entry == 0.0)):
                raise ValueError(f"keep_offset entry {k} value {value!r} at update {u} is outside the trainer's "
                                 "accepted range (finite, at most 1e6 in binary32, above 0 where its b0 entry is)")
        return values
    value = _float32(b0 * (end - u) / end)
    if not (math.isfinite(value) and 0.0 < value <= KEEP_OFFSET_MAX):
        raise ValueError(f"keep_offset value {value!r} at update {u} is outside the trainer's accepted range "
                         "(finite, positive, at most 1e6 in binary32)")
    return value


PASS_OFFSET_MAX = 1.0e6  # the trainer's priority_pass_offset_v1::MAX_PASS_OFFSET_V1
# The pinned destination pair in data/flat_policy_v3/feature_identity.rs, also used by the native loader.
FLAT_V3_TRANSFER = {
    "expected_feature_contract_digest": "9319fbd41e6b42ec90d565c13f3b4f75a3898dafe3816387453c08419ba9cc68",
    "expected_feature_encoding_digest": "c4662291ca9a75525b51b51f3b5d512671340c05b69fd33fb0827c0c8af70a2b",
}
ACTION_WINDOW_RULES = ("action-windows-v1", "action-windows-end-step-v1")  # the trainer's action-window seat rules
WINDOW_RULES = ("harness-v2",) + ACTION_WINDOW_RULES


def validate_pass_offset(pass_offset, base=None):
    """The checked, normalized priority pass offset schedule `{"b0": float, "end_update": int, "shape":
    "linear-v1"}` (a new dict), the form of a scalar `keep_offset`: b0 a finite number in (0, 1e6], end_update an int
    in [1, 2^53], shape "linear-v1", no other key. With `base`, also raises unless the trainer can train the base with
    the offset (`check_windows_base`)."""
    if not isinstance(pass_offset, dict):
        raise ValueError("pass_offset must be an object {b0, end_update, shape}")
    unknown = sorted(str(k) for k in set(pass_offset) - KEEP_OFFSET_KEYS)
    if unknown:
        raise ValueError(f"pass_offset has unknown keys {unknown}")
    missing = sorted(KEEP_OFFSET_KEYS - set(pass_offset))
    if missing:
        raise ValueError(f"pass_offset lacks {missing}")
    b0, end, shape = pass_offset["b0"], pass_offset["end_update"], pass_offset["shape"]
    # The range test comes first: it is exact for any int, while isfinite(10**400) overflows instead of refusing.
    if not (_is_number(b0) and 0 < b0 <= PASS_OFFSET_MAX and math.isfinite(b0)):
        raise ValueError(f"pass_offset b0 must be a finite number in (0, {PASS_OFFSET_MAX:g}], got {b0!r}")
    if not (type(end) is int and 1 <= end <= KEEP_OFFSET_MAX_END):
        raise ValueError(f"pass_offset end_update must be an integer in [1, 2^53], got {end!r}")
    if shape not in KEEP_OFFSET_SHAPES:
        raise ValueError(f"pass_offset shape must be one of {KEEP_OFFSET_SHAPES}, got {shape!r}")
    if base is not None:
        check_windows_base(base)
    return {"b0": float(b0), "end_update": end, "shape": shape}


def check_windows_base(base):
    """Raises ValueError unless `base` can carry a priority pass offset: the trainer reads one value per update batch
    and refuses a batch that mixes offset and no offset, so EVERY episode of EVERY iteration needs `priority_windows`
    (two seat rules) with at least one seat on an action-window rule; none may carry a `priority_pass_offset` already
    (the schedule is the only writer of that field), a reset start or a search opponent (the trainer refuses priority
    windows with either), and the update must be the plain CPU GAE one."""
    for i, item in enumerate(base["iterations"]):
        for j, entry in enumerate(item["episodes"]):
            episode = entry["episode"]
            where = f"iteration {i} episode {j}"
            if "priority_pass_offset" in episode:
                raise ValueError(f"{where} already carries priority_pass_offset; the pass_offset schedule writes it")
            windows = episode.get("priority_windows")
            if windows is None:
                raise ValueError(f"pass_offset needs priority_windows on every episode of the base ({where} has none)")
            if not (isinstance(windows, list) and len(windows) == 2 and all(w in WINDOW_RULES for w in windows)):
                raise ValueError(f"{where}: priority_windows must be two of {WINDOW_RULES}, got {windows!r}")
            if not any(w in ACTION_WINDOW_RULES for w in windows):
                raise ValueError(f"{where}: pass_offset needs a seat on an action-window rule, got {windows!r}")
            for key in ("reset", "opponent_search"):
                if episode.get(key) is not None:
                    raise ValueError(f"{where}: priority_windows are refused with {key}")
    loss, backend = base.get("loss_selection"), base.get("update_backend", {"kind": "cpu"})
    if not (isinstance(loss, dict) and loss.get("kind") == "gae_advantage_value_v1"):
        raise ValueError("pass_offset trains only on the gae_advantage_value_v1 loss (the base's loss_selection)")
    if not (isinstance(backend, dict) and backend.get("kind") == "cpu"):
        raise ValueError("pass_offset trains only on the CPU update backend (the base's update_backend)")
    for key in KEEP_OFFSET_REFUSED_OPTIONS:
        if base.get(key) is not None:
            raise ValueError(f"pass_offset trains only on the plain CPU GAE update: the base's {key} is refused")


def check_pass_offset_source(pass_offset, source, update):
    """Reject active offsets when source metadata establishes an unsupported learner.

    Generation preflight uses the declared transfer. The native collector checks the loaded policy.
    """
    if pass_offset is None or update >= pass_offset["end_update"]:
        return
    if source.get("s_learner") is not None:
        raise ValueError("priority pass offset is unsupported by the S learner")
    if source.get("feature_transfer") == FLAT_V3_TRANSFER:
        raise ValueError("priority pass offset is unsupported by FlatV3: it requires a V4-generation policy")


def pass_offset_value(pass_offset, u):
    """The `priority_pass_offset` an episode with `priority_windows` carries at fork update `u`, or None from
    `end_update` on (no field at all): float32(float64(b0) * (T - u) / T), computed as `keep_offset_value` computes a
    scalar. Raises ValueError for a negative or non-integer `u`, or a value the trainer would refuse (not finite,
    negative or above 1e6 in binary32; the trainer accepts zero, which changes no logit)."""
    pass_offset = validate_pass_offset(pass_offset)
    if type(u) is not int or u < 0:
        raise ValueError(f"pass_offset update must be a non-negative integer, got {u!r}")
    end = pass_offset["end_update"]
    if u >= end:
        return None
    value = _float32(pass_offset["b0"] * (end - u) / end)
    if not (math.isfinite(value) and 0.0 <= value <= PASS_OFFSET_MAX):
        raise ValueError(f"pass_offset value {value!r} at update {u} is outside the trainer's accepted range "
                         "(finite, at least 0, at most 1e6 in binary32)")
    return value


POSTBOARD_TAG = "postboard-exposure-v1"  # the domain of every draw below
POSTBOARD_KEYS = frozenset({"share", "max_swaps", "rule", "seats", "keep_ids"})
POSTBOARD_RULES = ("uniform-swaps-v1",)
POSTBOARD_SEATS = ("learner-and-current-v1",)
POSTBOARD_MAIN, POSTBOARD_SIDE = 60, 15  # the trainer's exact registration (sideboard.rs)
POSTBOARD_MAX_DENOMINATOR = 1 << 20
POSTBOARD_MAX_CARD_ID = (1 << 16) - 1  # card ids are u16 in the trainer's deck lists
POSTBOARD_OPPONENT_KINDS = ("current", "fixed", "initial")  # the seat rule reads the kind, so it must be one it knows


def _is_index(value, low, high):
    return type(value) is int and low <= value <= high


def validate_postboard_exposure(spec, base=None):
    """The checked, normalized post-board exposure `{"share": [n, d], "max_swaps": k, "rule": "uniform-swaps-v1",
    "seats": "learner-and-current-v1", "keep_ids": [card ids]}` (a new dict). Raises ValueError for anything that is
    not exactly that form: a non-mapping, missing or unknown keys, a share that is not two ints with
    1 <= n <= d <= 2^20 (a bool is not an int), max_swaps not an int in [1, 15], an unknown rule or seat rule, or
    keep_ids that is not a strictly increasing list of card ids (each an int in [0, 65535]; it may be empty).

    The meaning (`postboard_exposure_episode`): a share n / d of the episodes are drawn for exposure; in such a game
    each exposed seat exchanges k' mainboard positions for k' sideboard positions of its registered 75, k' uniform in
    [1, max_swaps]. `keep_ids` are the card ids that are never taken out of a mainboard (the declared protected set:
    the lands, so a random exchange cannot remove a deck's mana). The exchange counts positions, not card names: a
    position exchanged for a copy of the same card changes nothing, so where a card id is in both lists the number of
    cards that change can be below k', and a drawn game whose exchanges all cancel stays a game one. The share of
    post-board games is therefore at most n / d, and equal to it when no deck has a card id in both lists.

    With `base` (an OH1/OH2 base), also raises unless every episode can be a post-board game (`check_postboard_base`)."""
    if not isinstance(spec, dict):
        raise ValueError("postboard_exposure must be an object {share, max_swaps, rule, seats, keep_ids}")
    unknown = sorted(str(k) for k in set(spec) - POSTBOARD_KEYS)
    if unknown:
        raise ValueError(f"postboard_exposure has unknown keys {unknown}")
    missing = sorted(POSTBOARD_KEYS - set(spec))
    if missing:
        raise ValueError(f"postboard_exposure lacks {missing}")
    share, swaps, rule, seats, keep = (spec[k] for k in ("share", "max_swaps", "rule", "seats", "keep_ids"))
    if not (isinstance(share, list) and len(share) == 2 and _is_index(share[1], 1, POSTBOARD_MAX_DENOMINATOR)
            and _is_index(share[0], 1, share[1])):
        raise ValueError("postboard_exposure share must be [n, d], two integers with 1 <= n <= d <= 2^20, "
                         f"got {share!r}")
    if not _is_index(swaps, 1, POSTBOARD_SIDE):
        raise ValueError(f"postboard_exposure max_swaps must be an integer in [1, {POSTBOARD_SIDE}], got {swaps!r}")
    if rule not in POSTBOARD_RULES:
        raise ValueError(f"postboard_exposure rule must be one of {POSTBOARD_RULES}, got {rule!r}")
    if seats not in POSTBOARD_SEATS:
        raise ValueError(f"postboard_exposure seats must be one of {POSTBOARD_SEATS}, got {seats!r}")
    if not (isinstance(keep, list) and all(_is_index(i, 0, POSTBOARD_MAX_CARD_ID) for i in keep)
            and all(a < b for a, b in zip(keep, keep[1:]))):
        raise ValueError("postboard_exposure keep_ids must be a strictly increasing list of card ids in [0, 65535]")
    normalized = {"share": list(share), "max_swaps": swaps, "rule": rule, "seats": seats, "keep_ids": list(keep)}
    if base is not None:
        check_postboard_base(base, normalized)
    return normalized


def check_postboard_base(base, spec=None):
    """Raises ValueError unless every episode of `base` can be turned into a post-board game: it is a game one today
    (`postboard` false, `selected` equal to `registered`), both seats register exactly 60 and 15 card ids (the
    trainer's own registration), it has no reset start and no search opponent (a recorded prefix or a search
    descriptor belongs to the registered 60), and its entry names an opponent kind the seat rule knows. With
    `spec` (a normalized exposure), also raises unless every registration has at least max_swaps mainboard positions
    outside keep_ids, so the number of positions exchanged is never cut short and stays uniform."""
    keep = set(spec["keep_ids"]) if spec is not None else None
    for i, item in enumerate(base["iterations"]):
        for j, entry in enumerate(item["episodes"]):
            episode = entry["episode"]
            where = f"iteration {i} episode {j}"
            if episode.get("postboard") is not False:
                raise ValueError(f"{where}: postboard_exposure needs a game-one base (postboard false); the exposure "
                                 "is the only writer of a post-board game")
            registered, selected = episode.get("registered"), episode.get("selected")
            if not (isinstance(registered, list) and len(registered) == 2 and registered == selected):
                raise ValueError(f"{where}: postboard_exposure needs two registrations with selected equal to "
                                 "registered")
            for deck in registered:
                main, side = (deck.get(k) if isinstance(deck, dict) else None for k in ("mainboard", "sideboard"))
                if not (isinstance(main, list) and len(main) == POSTBOARD_MAIN and isinstance(side, list)
                        and len(side) == POSTBOARD_SIDE
                        and all(_is_index(c, 0, POSTBOARD_MAX_CARD_ID) for c in main + side)):
                    raise ValueError(f"{where}: postboard_exposure needs registrations of exactly {POSTBOARD_MAIN} "
                                     f"mainboard and {POSTBOARD_SIDE} sideboard card ids")
                if keep is not None and sum(card not in keep for card in main) < spec["max_swaps"]:
                    raise ValueError(f"{where}: postboard_exposure needs at least max_swaps ({spec['max_swaps']}) "
                                     f"mainboard cards outside keep_ids in every registration "
                                     f"({deck.get('label')!r} has fewer)")
            if not _is_index(episode.get("learner_seat"), 0, 1):
                raise ValueError(f"{where}: postboard_exposure needs a learner seat of 0 or 1")
            for key in ("reset", "opponent_search"):
                if episode.get(key) is not None:
                    raise ValueError(f"{where}: postboard_exposure is refused with {key}")
            kind = (entry.get("opponent") or {}).get("kind")
            if kind not in POSTBOARD_OPPONENT_KINDS:
                raise ValueError(f"{where}: postboard_exposure needs an opponent kind in {POSTBOARD_OPPONENT_KINDS}, "
                                 f"got {kind!r}")


def _postboard_key(parts):
    """A 256-bit integer from the canonical JSON of `parts`: a draw is a remainder of it (the bias of a remainder
    below 2^20 is under 2^-235) or the order of several of them (a uniform subset without replacement)."""
    text = json.dumps(list(parts), separators=(",", ":"))
    return int.from_bytes(hashlib.sha256(text.encode()).digest(), "big")


def postboard_exposure_seats(entry):
    """The physical seats the exposure may change under `learner-and-current-v1`: the learner seat, and the other
    seat when the entry's opponent is the current policy. A fixed (or initial) opponent seat keeps its registered 60:
    it plays the surface its checkpoint was trained on."""
    learner = entry["episode"]["learner_seat"]
    return [learner, 1 - learner] if entry["opponent"]["kind"] == "current" else [learner]


def postboard_exposure_swap(deck, spec, key):
    """`deck` (a registration `{label, mainboard, sideboard}`) with k' positions exchanged, as a new dict with both
    lists sorted, or None when the exchange leaves the mainboard as it was (every position was exchanged for a copy
    of the same card). k' = 1 + draw mod max_swaps; a base admitted by `check_postboard_base` always has that many
    movable positions (a direct caller with fewer gets as many as there are). The positions out are the k' mainboard
    positions (of those whose card id is not in keep_ids) with the smallest keys; the positions in are the k'
    sideboard positions with the smallest keys. `key` is the list that names this seat's draws. `deck` is not
    modified."""
    main, side = list(deck["mainboard"]), list(deck["sideboard"])
    keep = set(spec["keep_ids"])
    movable = [i for i, card in enumerate(main) if card not in keep]
    count = min(1 + _postboard_key(key + ["count"]) % spec["max_swaps"], len(movable), len(side))
    outs = sorted(movable, key=lambda i: _postboard_key(key + ["out", i]))[:count]
    ins = sorted(range(len(side)), key=lambda i: _postboard_key(key + ["in", i]))[:count]
    new_main = sorted([c for i, c in enumerate(main) if i not in set(outs)] + [side[i] for i in ins])
    new_side = sorted([c for i, c in enumerate(side) if i not in set(ins)] + [main[i] for i in outs])
    if new_main == sorted(main):
        return None
    return dict(deck, mainboard=new_main, sideboard=new_side)


def postboard_exposure_episode(entry, spec, namespace, run, update, slot):
    """Applies the exposure to one config entry in place and returns whether it became a post-board game. The game is
    drawn when sha256([tag, namespace, run, update, slot, "gate"]) mod d < n; then each exposed seat
    (`postboard_exposure_seats`) gets its own exchange (`postboard_exposure_swap`). When at least one mainboard
    changed, `selected` holds the result (new lists; a seat that did not change keeps its registration's values) and
    `postboard` is true. `registered` never changes, so every selected 60 and 15 conserves its registered 75. A
    drawn game whose exchanges all cancel stays a game one, byte for byte (possible only when a card id is in both
    lists of a deck)."""
    episode = entry["episode"]
    key = [POSTBOARD_TAG, namespace, run, update, slot]
    numerator, denominator = spec["share"]
    if _postboard_key(key + ["gate"]) % denominator >= numerator:
        return False
    selected = [dict(deck) for deck in episode["selected"]]
    changed = False
    for seat in postboard_exposure_seats(entry):
        swapped = postboard_exposure_swap(episode["registered"][seat], spec, key + ["seat", seat])
        if swapped is not None:
            selected[seat] = swapped
            changed = True
    if changed:
        episode["selected"] = selected
        episode["postboard"] = True
    return changed


def build_config(base, dose, root, run, namespace, offset, updates, workers, coefficient=None, kappa=None,
                 prep_workers=None, keep_offset=None, pregame_advantage=None, pregame_announcement_step_scale=None,
                 pass_offset=None, postboard_exposure=None):
    """An RT1 run config from an OH1/OH2 base and a dose record: the base's schedule from global update `offset`,
    episode ids and deal seeds keyed by (namespace, run, update, slot), and EX1 when `coefficient` is given.

    `keep_offset` (`{"b0", "end_update", "shape": "linear-v1"}`, see `keep_offset_value`) stamps every episode that
    has an `opening` with `london_keep_offset = b(u)` for the iteration's global update u = offset + i, while
    u < end_update, and no such key from there on (a number, or with a vector b0 the list of 7). A chain's global
    update counts from 0 at the fork point (its first segment's offset), so u is the fork update. Without it every
    episode is left exactly as the base has it.

    `pass_offset` (`{"b0", "end_update", "shape": "linear-v1"}`, see `pass_offset_value`) stamps every episode that
    has `priority_windows` with `priority_pass_offset = b(u)` the same way, and no such key from end_update on.

    `pregame_advantage` ("terminal_v1", see `validate_pregame_advantage`) adds that key to the config's
    `loss_selection`; without it the loss_selection is exactly the base's. `pregame_announcement_step_scale`
    (`validate_pregame_announcement_step_scale`) adds that key too, only with the terminal mode.

    `postboard_exposure` (`validate_postboard_exposure`) turns a declared share of the episodes into post-board
    games: `selected` differs from `registered` on the exposed seats and `postboard` is true
    (`postboard_exposure_episode`), drawn per (namespace, run, update, slot), so a segment's games do not depend on
    where the segment starts. Without it every episode is left exactly as the base has it."""
    check_base_names_no_pregame_scale(base)
    pregame_announcement_step_scale = validate_pregame_announcement_step_scale(pregame_announcement_step_scale, pregame_advantage)
    if pregame_advantage is not None:
        pregame_advantage = validate_pregame_advantage(pregame_advantage, base)
        if coefficient is not None:
            raise ValueError("pregame_advantage is refused with an EX1 coefficient (the plain CPU GAE update only)")
    if keep_offset is not None:
        keep_offset = validate_keep_offset(keep_offset, base)
        if offset < 0:
            raise ValueError(f"keep_offset needs a non-negative global update offset, got {offset}")
        if coefficient is not None:
            raise ValueError("keep_offset is refused with an EX1 coefficient (the trainer refuses the offset with ex1)")
    if pass_offset is not None:
        pass_offset = validate_pass_offset(pass_offset, base)
        if offset < 0:
            raise ValueError(f"pass_offset needs a non-negative global update offset, got {offset}")
        if coefficient is not None:
            raise ValueError("pass_offset is refused with an EX1 coefficient (the trainer refuses the offset with ex1)")
    if postboard_exposure is not None:
        postboard_exposure = validate_postboard_exposure(postboard_exposure, base)
        if offset < 0:
            raise ValueError(f"postboard_exposure needs a non-negative global update offset, got {offset}")
        if coefficient is not None:
            raise ValueError("postboard_exposure is refused with an EX1 coefficient (search roots and placements "
                             "belong to the registered 60)")
    source = dose["source"]
    check_pass_offset_source(pass_offset, source, offset)
    # An S source starting from its initial train state (W5's XS parent, S0) has no checkpoint; its pin is
    # `s_learner.initial` (s_learner_v1.rs). A fresh net-8 start (a dose record that says `"fresh": true`, e.g. a
    # G(i) fresh-init lineage) has no checkpoint either: the trainer builds the model from the play import's
    # fresh-initialization parameters with a fresh Adam, so the play import is the pin.
    if source.get("checkpoint") is not None:
        pinned = source["checkpoint"]
    elif dose.get("fresh") is True:
        assert not source.get("s_learner") and not source.get("w_learner"),             "a fresh net-8 parent has no S or wide learner"
        pinned = source.get("play_import")
    else:
        # An S or wide (net-8w128, w_learner_v1.rs) learner without a checkpoint: its starting train state.
        pinned = (source.get("s_learner") or source.get("w_learner") or {}).get("initial")
    assert pinned is not None, "the parent has neither a checkpoint nor an S initial state"
    assert sha(pinned["path"]) == pinned["sha256"], "parent differs from its dose pin"
    plan = base["iterations"]
    iterations = []
    for i in range(updates):
        update = offset + i
        item = copy.deepcopy(plan[update % len(plan)])
        for j, entry in enumerate(item["episodes"]):
            episode = entry["episode"]
            episode["id"] = f"{namespace}-{run}-u{update}-j{j}-s{episode['learner_seat']}"
            episode["seed"] = seed([namespace, run, update, j])
            if keep_offset is not None and episode.get("opening") is not None:
                value = keep_offset_value(keep_offset, update)
                if value is not None:
                    episode["london_keep_offset"] = value
            if pass_offset is not None and episode.get("priority_windows") is not None:
                value = pass_offset_value(pass_offset, update)
                if value is not None:
                    episode["priority_pass_offset"] = value
            if postboard_exposure is not None:
                postboard_exposure_episode(entry, postboard_exposure, namespace, run, update, j)
        iterations.append(item)
    cfg = {k: copy.deepcopy(v) for k, v in base.items() if k != "iterations"}
    if pregame_advantage is not None:
        cfg["loss_selection"] = dict(cfg["loss_selection"], pregame_advantage=pregame_advantage)
        if pregame_announcement_step_scale is not None:
            cfg["loss_selection"]["pregame_announcement_step_scale"] = pregame_announcement_step_scale
    cfg["initial_source"] = source
    cfg["iterations"] = iterations
    cfg["collection_workers"] = workers
    cfg["record_process_cpu"] = True
    cfg["output_directory"] = Path(root).as_posix()
    if coefficient is not None:
        cfg["preparation_workers"] = 1 if prep_workers is None else int(prep_workers)
        cfg["ex1"] = {"run": run, "update_offset": offset, "coefficient": coefficient,
                      "cap": {"mode": "off"} if kappa is None else {"mode": "ratio", "kappa": kappa}}
    return cfg


def config(args):
    cfg = build_config(read(args.base), read(args.parent), args.root, args.run, args.namespace, args.offset,
                       args.updates, args.workers, args.coefficient if args.ex1 else None, args.kappa,
                       args.prep_workers)
    Path(args.out).write_text(json.dumps(cfg, indent=1) + "\n", encoding="utf-8")


def iterations(root):
    for complete in sorted(Path(root, "iterations").glob("*/complete.json")):
        receipt = read(complete)
        update = read(receipt["update"]["path"])
        collection = read(receipt["collection"]["path"])
        yield receipt, update, collection


def ex1_receipt(update):
    pin = (update.get("ex1") or {}).get("receipt")
    return read(pin["path"]) if pin else None


def summarize(args):
    out = []
    for root in args.roots:
        previous = None
        for receipt, update, collection in iterations(root):
            cpu = receipt.get("driver_process_cpu_seconds")
            row = {"run": Path(root).name, "iteration": receipt["iteration"],
                   "cpu_seconds": None if cpu is None or previous is None else cpu - previous,
                   "collection_wall": collection.get("collection_elapsed_seconds"),
                   "update_wall": update.get("update_elapsed_seconds"),
                   "after_state_sha256": update["after_state_sha256"]}
            previous = cpu
            ex1 = ex1_receipt(update)
            if ex1:
                exposure = ex1["gradient_exposure"]
                row.update({"searches": ex1["searches"], "searches_failed": ex1["searches_failed"],
                            "search_wall": ex1["search_wall_seconds"], "rows": ex1["rows"],
                            "rows_eligible": ex1["rows_eligible"], "rows_dropped": ex1["rows_dropped"],
                            "cap": exposure.get("cap"), "adam_steps": exposure.get("adam_steps")})
            out.append(row)
    print(json.dumps(out, indent=1))
    return out


def strata(args):
    totals = {s: {"decisions": 0, "triggers": 0, "searches_failed": 0, "root_rows": 0, "burst_rows": 0,
                  "no_target_support": 0, "no_target_mass": 0} for s in N_S}
    episodes = 0
    for root in args.roots:
        for _, update, _ in iterations(root):
            ex1 = ex1_receipt(update)
            if not ex1:
                continue
            episodes += len(ex1["episodes"])
            for stratum, counts in ex1["strata"].items():
                for key in totals[stratum]:
                    totals[stratum][key] += counts.get(key, 0)
    report = {"episodes": episodes, "strata": {}}
    for s, t in totals.items():
        per_episode = t["decisions"] / episodes if episodes else None
        report["strata"][s] = dict(t, decisions_per_episode=per_episode, n_s=N_S[s], p_s=P_S[s],
                                   expected_triggers=t["decisions"] * P_S[s],
                                   triggers_per_episode=t["triggers"] / episodes if episodes else None)
    print(json.dumps(report, indent=1))
    return report


def ratios(roots, key):
    values = []
    for root in roots:
        for _, update, _ in iterations(root):
            ex1 = ex1_receipt(update)
            if not ex1 or ex1["rows"] == 0:
                continue
            value = (ex1["gradient_exposure"].get("cap") or {}).get(key)
            if not isinstance(value, (int, float)):
                raise SystemExit(f"stop: an update with rows has a missing or null {key} ({root})")
            values.append(value)
    return values


def calibrate(args):
    raw = ratios(args.roots, "raw_ratio")
    if len(raw) < 8:
        raise SystemExit(f"stop: only {len(raw)} updates have rows")
    median = statistics.median(raw)
    if not median > 0:
        raise SystemExit("stop: the median raw ratio is not positive")
    c_star = float(f"{R_STAR / median:.3g}")
    print(json.dumps({"r_star": R_STAR, "updates_with_rows": len(raw), "median_raw_ratio": median,
                      "c_star": c_star, "raw": raw}, indent=1))


def check(args):
    applied = ratios(args.roots, "applied_ratio")
    if len(applied) < 8:
        raise SystemExit(f"stop: only {len(applied)} updates have rows")
    median = statistics.median(applied)
    ok = R_STAR / 3 <= median <= 3 * R_STAR
    print(json.dumps({"r_star": R_STAR, "kappa": KAPPA, "updates_with_rows": len(applied),
                      "median_applied_ratio": median, "band": [R_STAR / 3, 3 * R_STAR], "pass": ok,
                      "applied": applied}, indent=1))
    if not ok:
        raise SystemExit("stop: the median applied ratio is outside [r*/3, 3 r*]")


TIMING_KEYS = {"wall", "search_wall_seconds", "teacher_seconds", "seconds"}


PATH_KEYS = TIMING_KEYS | {"path", "sha256", "checkpoint_sha256"}


def strip(value, keys=TIMING_KEYS):
    if isinstance(value, dict):
        return {k: strip(v, keys) for k, v in value.items() if k not in keys}
    if isinstance(value, list):
        return [strip(v, keys) for v in value]
    return value


def ex1_records(collection):
    return [read(pin["path"]) for pin in (collection.get("ex1") or {}).get("episodes", [])]


def mirror(args):
    checked = 0
    for root in args.roots:
        for _, _, collection in iterations(root):
            trajectories = [read(pin["path"]) for pin in collection["trajectories"]]
            for record in ex1_records(collection):
                trajectory = trajectories[record["slot"]]
                by_step = {d["step"]: d for d in trajectory["decisions"]}
                for row in record["attempts"][-1]["rows"]:
                    logits = by_step[row["step"]]["logits"]
                    result = target_ref.target(logits, row["n"], row["wins"], row["draws"])
                    assert not isinstance(result, tuple), f"mirror gives no target at {root} {row['step']}"
                    q_bits = [target_ref.bits64(x) for x in result["q"]]
                    assert q_bits == row["q_bits"], f"mirror differs at {root} slot {record['slot']} {row['step']}"
                    checked += 1
    print(json.dumps({"rows_checked": checked, "bit_equal": True}))
    return checked


def compare(args):
    a, b = list(iterations(args.roots[0])), list(iterations(args.roots[1]))
    assert len(a) == len(b), "different iteration counts"
    for (ra, ua, ca), (rb, ub, cb) in zip(a, b):
        assert ua["after_state_sha256"] == ub["after_state_sha256"], f"state differs at {ra['iteration']}"
        # Later trajectories name the run's own previous checkpoint file (run-local path and bytes); the model
        # state is compared above, so file identities are left out here.
        ta = [strip(read(p["path"]), PATH_KEYS) for p in ca["trajectories"]]
        tb = [strip(read(p["path"]), PATH_KEYS) for p in cb["trajectories"]]
        assert ta == tb, f"trajectories differ at {ra['iteration']}"
        assert strip(ex1_records(ca)) == strip(ex1_records(cb)), f"ex1 records differ at {ra['iteration']}"
        ea, eb = ex1_receipt(ua), ex1_receipt(ub)
        keys = ("strata", "searches", "searches_failed", "rows", "rows_eligible", "rows_dropped", "gradient_exposure")
        assert (ea is None) == (eb is None) and (ea is None or all(strip(ea[k]) == strip(eb[k]) for k in keys))
    print(json.dumps({"iterations": len(a), "identical": True,
                      "states": [u["after_state_sha256"] for _, u, _ in a]}))


def hookoff(args):
    x, o = list(iterations(args.roots[0])), list(iterations(args.roots[1]))
    n = min(len(x), len(o))
    assert n >= 1
    for (_, _, cx), (_, _, co) in zip(x[:n], o[:n]):
        assert [p["sha256"] for p in cx["trajectories"]] == [p["sha256"] for p in co["trajectories"]]
    print(json.dumps({"iterations": n, "trajectories_identical": True,
                      "x_rows": [ex1_receipt(u)["rows"] for _, u, _ in x[:n]],
                      "x_state": [u["after_state_sha256"] for _, u, _ in x[:n]],
                      "o_state": [u["after_state_sha256"] for _, u, _ in o[:n]]}))


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("config")
    c.add_argument("--base", required=True)
    c.add_argument("--parent", required=True)
    c.add_argument("--out", required=True)
    c.add_argument("--root", required=True)
    c.add_argument("--run", required=True)
    c.add_argument("--offset", type=int, required=True)
    c.add_argument("--updates", type=int, required=True)
    c.add_argument("--workers", type=int, default=10)
    c.add_argument("--prep-workers", type=int, default=None)
    c.add_argument("--namespace", default="ex1-qual-20261029")
    c.add_argument("--ex1", action="store_true")
    c.add_argument("--coefficient", type=float, default=1.0)
    c.add_argument("--kappa", type=float, default=None)
    for name in ("summarize", "strata", "calibrate", "mirror", "compare", "hookoff"):
        s = sub.add_parser(name)
        s.add_argument("roots", nargs="+")
    k = sub.add_parser("check")
    k.add_argument("roots", nargs="+")
    args = ap.parse_args(argv)
    {"config": config, "summarize": summarize, "strata": strata, "calibrate": calibrate, "check": check,
     "mirror": mirror, "compare": compare, "hookoff": hookoff}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
