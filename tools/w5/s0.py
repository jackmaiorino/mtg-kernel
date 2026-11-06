"""W5 E4: S0, XS's parent (collab LANES/lead/W5-DESIGN.md v1.3 section 6.2; section 8 freezes it).

S0 is a distillation of b512 into S. This tool covers every step except the trainer (tools/w5/s0_train.py):

  s0.py plan-record --parent SOURCE.json --out ROOT [--attempt first|retry] [--games N] [--chunk-games 256]
                    [--store-codec raw|zstd-record/v1]
      the recording plan: b512 self-play games under W5's training runtime (resolution-boundary-v1, mulligans
      off, the training limits, the nine-deck pool), recorded with teacher logits by `regret_census_v1
      mode=s4a-tensor-dump` (mtg-kernel#480). 100k games is the frozen size; the one retry records 2x in place
      of the first store, in its own seed namespace. With `--store-codec zstd-record/v1` (opt-in; the raw store
      is unchanged without it) each recorded chunk is packed (`pack-chunk`): one zstd frame per record, the
      packed file read back and decoded to the recorder's bytes (SHA-256) before the raw file is removed. The
      index keeps raw coordinates and decoded block hashes and adds each record's frame; the trainer reads a
      record with one positioned read and one decode.
  s0.py qualify-record --plan ROOT/plan.json --binary EXE --parallel 1,2,4 [--games 64]
      the recorder's throughput check (COMPUTE-POLICY): completed games per second at each process count, on
      throwaway chunks; `record` runs a production plan only with a receipt for this binary and host.
  s0.py record --plan ROOT/plan.json --binary EXE [--parallel P] [--limit-chunks N]
      runs every chunk without a finished log (one census process per chunk, one worker each: s4a threads
      contend), keeping a failed attempt's files under an attempt suffix.
  s0.py index --plan ROOT/plan.json
      the canonical store index (ROOT/index.npz, index.games.json, manifest.json): every chunk finished, every
      game exactly once, each row on the plan's runtime and limits, block and file hashes. Every recorded
      decision has a policy target; only a naturally finished game has a value target (counted).
  s0.py config --plan ROOT/plan.json --out CFG.json --grad-clip-norm {none|X} [--smoke ...]
      the frozen S0 recipe (section 6.2) bound to the store's hashes; s0_train.py trains from it.
  s0.py export --config CFG.json --run RUN_DIR --binary s_transformer_v1 [--parity-decisions 10000]
      the native S (fp32, dropout 0) from the run's best state, and the export parity receipt: native
      `logits-dump` against the PyTorch state dict scored in float64 (s_transformer_reference_v1.py
      torch-logits) on the first 10,000 validation decisions, max absolute logit difference at most 1e-4.
  s0.py freeze-start --export RUN_DIR/export/export.json --binary s_transformer_v1 --out XS-START.json
      XS's frozen start (6.2): S0's weights, the reset Adam (zero moments, step 0), and the RNG states and data
      cursors (W5's chain seed roots and update 0), hashed by `s_transformer_v1 starting-state`
      (`STrainStateV1::starting_state_sha256_v1`). Written before the floor can be planned.
  s0.py source --shell SHELL.json --container S0.bin --out S0-SOURCE.json
      the RT1 S source (`w5-s-learner/v1`) on the FlatV4 shell, for the floor and XS.
  s0.py rebind-h --panel-def H-PANEL.json --import IMPORT.json --envelope ENVELOPE.json --head SHA --out DIR
      Step C's H rebound to the floor evaluator's build (lead ruling 11-05 E4 item 4: both panels on one S-capable
      build). H's V3 transfer envelope records its destination build commit and loads only in that build, so the
      envelope changes in that one field and the import and opponent only in the pin below them (D1, OH2, Step C).
  s0.py floor-plan --s0-source S --b512-source B --panel-def H-PANEL.json --xs-start XS-START.json
                   --evaluator EXE --out DIR [--h-rebind DIR/h-rebind.json]
  s0.py floor-run --floor DIR/floor.json --evaluator EXE [--workers 1,2,4,8]
  s0.py floor-read --floor DIR/floor.json [--out RECEIPT.json]
      the qualification floor: native S0 and b512 each play the frozen H panel (Step C namespace a: 81 ordered
      deck pairs, 12 families, 2 seats, 1,944 games) against Step C's reference H on one evaluator build; the
      paired difference S0 minus b512 in signed score over the eight non-Spy decks must be at least -0.03. With
      --h-rebind the plan re-derives the rebound H from Step C's pinned files byte for byte.

Pinned choices (design 6.2 names them or defers to the W7 pilot, collab LANES/bignet-pilot-20261031):
- recording: `regret_census_v1 mode=s4a-tensor-dump` (#480) with `OPPONENTS=self=<parent>` (both seats b512,
  self-play), base seed = the first 8 bytes (little-endian) of SHA256("w5/s0/record/<attempt>") (the pilot's
  rule, code/gen_chunk.sh line 4), chunks of 256 games (code/gen_queue.sh); runtime = W5's training runtime
  (tools/search_policy_pilot/schedule.py: resolution-boundary-v1 and the nine-deck baseline's limits,
  python/tools/nine_deck_baseline_v1.py MAX_PHYSICAL_DECISIONS and MAX_POLICY_STEPS; mulligans off). Each
  decision with two or more legal actions is recorded for the acting seat (both seats), with b512's logits.
- split: game g is in validation iff the first 8 bytes (big-endian) of SHA256(json(["w5/s0/split", g])),
  over 2^64, are below 0.05 (the placement rule's `SHA256(json([...]))` form, section 3).
- outcome: the acting seat's natural win (1), loss (0) or natural draw (0.5). A game that did not finish
  naturally (a limit) has no outcome: its decisions keep their policy target (design 6.2, "every decision") and
  carry no value target (`has_value` false), so they are never in the value subsample or the validation value.
  The pilot dropped such games whole (code/index_data.py lines 74 to 79); its store had none.
The trainer's choices are listed in s0_train.py.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shutil
import struct
import subprocess
import sys
import time
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]

PLAN_SCHEMA = "w5-s0-record-plan/v1"
INDEX_SCHEMA = "w5-s0-store-index/v1"
CONFIG_SCHEMA = "w5-s0-config/v1"
QUAL_SCHEMA = "w5-s0-record-qualification/v1"
EXPORT_SCHEMA = "w5-s0-export/v1"
START_SCHEMA = "w5-xs-start/v1"
FLOOR_SCHEMA = "w5-s0-floor/v1"
FLOOR_RECEIPT_SCHEMA = "w5-s0-floor-receipt/v1"

FROZEN_GAMES = {"first": 100_000, "retry": 200_000}   # 6.2: 100k; one retry with 2x data in its place
CHUNK_GAMES = 256
DECKS = list(range(9))                                 # the lineage's nine-deck pool (data/runtime_decks_v1.json)
RUNTIME_RULES = "resolution-boundary-v1"               # tools/search_policy_pilot/schedule.py RUNTIME_RULES
MAX_PHYSICAL_DECISIONS = 100_000                       # python/tools/nine_deck_baseline_v1.py
MAX_POLICY_STEPS = 200_000
VALIDATION_FRACTION = 0.05
SPLIT_TAG = "w5/s0/split"
RECORD_TAG = "w5/s0/record"
PARENT_LABEL = "b512"
PARITY_DECISIONS = 10_000
PARITY_BAR = 1e-4
FLOOR = -0.03
H_NAMESPACE = "w4c-panel-20261031a"
# XS's chains (design section 8: seed roots `w5/chain<i>`, at most 6 per arm) and its first update.
XS_SEED_ROOTS = tuple(f"w5/chain{i}" for i in range(1, 7))
XS_FIRST_UPDATE = 0
# Production plans need a record qualification receipt; smaller ones are smoke runs.
SMOKE_GAMES_MAX = 4096
# Opt-in packed store (`plan-record --store-codec zstd-record/v1`): each record one zstd frame, so a record is read
# with one positioned read and one decode; the index keeps raw coordinates and adds packed ones.
STORE_CODEC = "zstd-record/v1"
ZSTD_LEVEL = 3
PACK_SCHEMA = "w5-s0-chunk-pack/v1"

RECORD_FIELDS = (  # name, dtype, per-row width, count key (stage4a/tensor_dump.rs record layout)
    ("state", np.float32, 1, "s"),
    ("object_features", np.float32, 98, "n"),
    ("object_card_ids", np.int32, 1, "n"),
    ("object_groups", np.int32, 1, "n"),
    ("object_node_ids", np.int32, 1, "n"),
    ("edge_features", np.float32, 41, "e"),
    ("edge_source", np.int32, 1, "e"),
    ("edge_target", np.int32, 1, "e"),
    ("action_features", np.float32, 195, "a"),
    ("action_ref_features", np.float32, 25, "r"),
    ("action_ref_card_ids", np.int32, 1, "r"),
    ("action_ref_action_indices", np.int32, 1, "r"),
    ("action_ref_node_indices", np.int32, 1, "r"),
    ("logits", np.float32, 1, "a"),
    ("value", np.float32, 1, "one"),
)
STATE_DIM = 219


class S0Error(Exception):
    pass


def require(ok: bool, message: str) -> None:
    if not ok:
        raise S0Error(message)


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(1 << 22):
            h.update(chunk)
    return h.hexdigest()


def pin(path) -> dict:
    path = Path(path)
    return {"path": path.as_posix(), "sha256": sha256_file(path)}


def checked(ref: dict, what: str) -> Path:
    path = Path(ref["path"])
    require(path.exists() and sha256_file(path) == ref["sha256"], f"{what} differs from its pin: {path}")
    return path


def read_json(path) -> dict:
    return json.loads(Path(path).read_text(encoding="utf-8"))


def write_json(path, doc) -> dict:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(doc, indent=1, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
    return pin(path)


# ---------------------------------------------------------------------------------------------------------------
# Seeds and split


def base_seed(attempt: str) -> int:
    return int.from_bytes(hashlib.sha256(f"{RECORD_TAG}/{attempt}".encode()).digest()[:8], "little")


def split_u(game: int) -> float:
    digest = hashlib.sha256(json.dumps([SPLIT_TAG, int(game)]).encode()).digest()
    return int.from_bytes(digest[:8], "big") / 2.0 ** 64


def is_validation(game: int) -> bool:
    return split_u(game) < VALIDATION_FRACTION


# ---------------------------------------------------------------------------------------------------------------
# Records (mtg-kernel stage4a/tensor_dump.rs layout; the pilot's code/td_format.py)


def decode_record(buf, pos: int = 0) -> tuple[dict, int]:
    n, e, a, r, s, selected, legal, actor = (int(x) for x in np.frombuffer(buf, np.uint32, 8, pos))
    pos += 32
    counts = {"n": n, "e": e, "a": a, "r": r, "s": s, "one": 1}
    out = {"selected": selected, "legal": legal, "actor": actor}
    for name, dtype, width, key in RECORD_FIELDS:
        count = counts[key] * width
        arr = np.frombuffer(buf, dtype, count, pos)
        pos += count * 4
        out[name] = arr.reshape(-1, width) if width > 1 else arr
    out["value"] = float(out["value"][0])
    if s != STATE_DIM or legal != a or a < 2 or selected >= a:
        raise S0Error(f"bad record: state {s}, legal {legal}, actions {a}, selected {selected}")
    return out, pos


def record_size(buf, pos: int = 0) -> int:
    """A record's byte length from its header alone (the layout decode_record reads)."""
    n, e, a, r, s = (int(x) for x in np.frombuffer(buf, np.uint32, 5, pos))
    counts = {"n": n, "e": e, "a": a, "r": r, "s": s, "one": 1}
    return 32 + 4 * sum(width * counts[key] for _, _, width, key in RECORD_FIELDS)


def encode_record(rec: dict) -> bytes:
    """The inverse of decode_record (tests and fixtures)."""
    n, e = len(rec["object_card_ids"]), len(rec["edge_source"])
    a, r = len(rec["logits"]), len(rec["action_ref_card_ids"])
    parts = [struct.pack("<8I", n, e, a, r, STATE_DIM, rec["selected"], a, rec["actor"])]
    for name, dtype, width, key in RECORD_FIELDS:
        value = [rec["value"]] if name == "value" else rec[name]
        parts.append(np.asarray(value, dtype).reshape(-1).astype("<f4" if dtype == np.float32 else "<i4").tobytes())
    return b"".join(parts)


# ---------------------------------------------------------------------------------------------------------------
# Packed store (opt-in): one zstd frame per record


def zstd_api():
    """(library, version, compress(data, level), decompress(frame, size)) from the standard library's
    compression.zstd (Python 3.14) or the zstandard package; both write and read standard zstd frames."""
    try:
        from compression import zstd as z
        return ("compression.zstd", ".".join(map(str, z.zstd_version_info)),
                lambda data, level: z.compress(data, level=level), lambda frame, size: z.decompress(frame))
    except ImportError:
        pass
    try:
        import zstandard as z
    except ImportError:
        raise S0Error("a packed store needs zstd: Python 3.14 (compression.zstd) or the zstandard package") from None
    dctx = z.ZstdDecompressor()
    return ("zstandard", str(z.ZSTD_VERSION if isinstance(z.ZSTD_VERSION, str) else ".".join(map(str, z.ZSTD_VERSION))),
            lambda data, level: z.ZstdCompressor(level=level).compress(data),
            lambda frame, size: dctx.decompress(frame, max_output_size=size))


def frames_path(stem) -> Path:
    return Path(f"{stem}.frames.npy")


def chunk_lock(stem):
    """An exclusive OS lock on STEM.lock, or None while another process holds it. The OS releases it when the
    holder exits, so a recorder or packer left running by a killed driver keeps its chunk locked."""
    f = open(f"{stem}.lock", "a+b")
    try:
        if os.name == "nt":
            import msvcrt
            f.seek(0)
            msvcrt.locking(f.fileno(), msvcrt.LK_NBLCK, 1)
        else:
            import fcntl
            fcntl.flock(f.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
    except OSError:
        f.close()
        return None
    return f


def pack_chunk(stem, level: int = ZSTD_LEVEL) -> dict:
    """Packs a finished chunk's tensor file record by record (STORE_CODEC): STEM.binz holds one zstd frame per
    record in file order, STEM.frames.npy the table (raw offset, raw length, packed offset, packed length). The
    packed file is read back and every frame decoded; the decoded bytes must hash to the raw file's SHA-256 before
    the receipt (STEM.pack.json) is written and the raw file removed. Rerunning on a packed chunk only removes a
    raw file left behind, after checking the packed files against the receipt and the raw file against its decoded
    hash. The packer holds the chunk lock throughout, so a resumed recording cannot set the chunk aside under it."""
    stem = Path(stem)
    lock = chunk_lock(stem)
    require(lock is not None, f"{stem.name} is locked by another recorder or packer")
    try:
        return _pack_locked(stem, level)
    finally:
        lock.close()


def _pack_locked(stem: Path, level: int) -> dict:
    raw, packed, frames_file, receipt_path = (Path(f"{stem}.bin"), Path(f"{stem}.binz"), frames_path(stem),
                                              Path(f"{stem}.pack.json"))
    if receipt_path.exists():
        receipt = read_json(receipt_path)
        check_pack(stem, receipt.get("codec"))   # the packed outputs exist and match their pins
        if raw.exists():
            require(sha256_file(raw) == receipt["decoded"]["sha256"], f"{raw.name} differs from its pack receipt")
            raw.unlink()
        return receipt
    require(raw.exists(), f"{raw.name} is missing")
    library, version, compress, decompress = zstd_api()
    tmp = Path(f"{packed}.tmp")
    table, raw_hash, pos, zpos = [], hashlib.sha256(), 0, 0
    with open(raw, "rb") as fin, open(tmp, "wb") as fout:
        while header := fin.read(32):
            require(len(header) == 32, f"{raw.name}: a short record header at {pos}")
            size = record_size(header)
            body = fin.read(size - 32)
            require(len(body) == size - 32, f"{raw.name}: a short record at {pos}")
            data = header + body
            raw_hash.update(data)
            frame = compress(data, level)
            fout.write(frame)
            table.append((pos, size, zpos, len(frame)))
            pos += size
            zpos += len(frame)
    table = np.asarray(table, np.int64).reshape(-1, 4)
    require(pos == raw.stat().st_size, f"{raw.name}: changed while packing")
    decoded_hash = hashlib.sha256()
    with open(tmp, "rb") as f:   # the bytes on disk decode to the raw file
        for _, size, zoff, zlen in table:
            f.seek(int(zoff))
            data = decompress(f.read(int(zlen)), int(size))
            require(len(data) == size, f"{packed.name}: a frame decodes to {len(data)} bytes, not {size}")
            decoded_hash.update(data)
    require(decoded_hash.hexdigest() == raw_hash.hexdigest(), f"{packed.name}: decoded bytes differ from the raw file")
    os.replace(tmp, packed)
    with open(frames_file, "wb") as f:
        np.save(f, table, allow_pickle=False)
    receipt = {"schema": PACK_SCHEMA, "codec": {"kind": STORE_CODEC, "level": int(level)},
               "library": {"name": library, "zstd": version}, "records": int(len(table)),
               "decoded": {"bytes": pos, "sha256": raw_hash.hexdigest()}, "packed": pin(packed),
               "frames": pin(frames_file)}
    tmp_receipt = Path(f"{receipt_path}.tmp")
    write_json(tmp_receipt, receipt)
    os.replace(tmp_receipt, receipt_path)
    raw.unlink()
    return receipt


def check_pack(stem, codec: dict) -> tuple[dict, np.ndarray]:
    """A packed chunk's receipt, codec and pins; returns the receipt and its frame table."""
    receipt_path = Path(f"{stem}.pack.json")
    require(receipt_path.exists(), f"{Path(stem).name} has not been packed")
    receipt = read_json(receipt_path)
    require(receipt.get("schema") == PACK_SCHEMA and receipt.get("codec") == codec,
            f"{Path(stem).name}: packed with another codec")
    for key in ("packed", "frames"):
        path = Path(f"{stem}.binz") if key == "packed" else frames_path(stem)
        require(path.exists() and sha256_file(path) == receipt[key]["sha256"],
                f"{path.name} differs from its pack receipt")
    table = np.load(frames_path(stem), allow_pickle=False)
    require(table.ndim == 2 and table.shape[1] == 4 and len(table) == receipt["records"]
            and int(table[:, 1].sum()) == receipt["decoded"]["bytes"], f"{Path(stem).name}: bad frame table")
    return receipt, table


class RawChunk:
    """Reads from a raw chunk tensor file (the interface PackedChunk shares)."""

    def __init__(self, path):
        self.f = open(path, "rb")

    def close(self):
        self.f.close()

    def read(self, offset: int, length: int) -> bytes:
        self.f.seek(offset)
        return self.f.read(length)


def open_chunk(root: Path, manifest_chunk: dict, codec: dict | None):
    """A reader of one indexed chunk in raw coordinates; a packed chunk's pins are checked against the manifest."""
    stem = root / "store" / manifest_chunk["name"]
    if codec is None:
        return RawChunk(f"{stem}.bin")
    receipt, table = check_pack(stem, codec)
    require(receipt["packed"]["sha256"] == manifest_chunk["packed"]["sha256"]
            and receipt["frames"]["sha256"] == manifest_chunk["frames"]["sha256"],
            f"{manifest_chunk['name']}: pack differs from the manifest")
    return PackedChunk(f"{stem}.binz", table)


class PackedChunk:
    """Raw-coordinate reads from a packed chunk: a read starts and ends on record (frame) boundaries."""

    def __init__(self, path, table: np.ndarray):
        self.f, self.table = open(path, "rb"), table
        self.total = int(table[:, 1].sum())
        self.decompress = zstd_api()[3]

    def close(self):
        self.f.close()

    def read(self, offset: int, length: int) -> bytes:
        i = int(np.searchsorted(self.table[:, 0], offset))
        if length == 0:   # a game with no recorded decision: an empty block at a record boundary or the end
            require(offset == self.total or (i < len(self.table) and int(self.table[i, 0]) == offset),
                    f"offset {offset} is not a record boundary")
            return b""
        require(i < len(self.table) and int(self.table[i, 0]) == offset, f"offset {offset} is not a record start")
        out, end = [], offset + length
        while offset < end:
            require(i < len(self.table), f"read past the chunk at {offset}")
            _, size, zoff, zlen = (int(x) for x in self.table[i])
            self.f.seek(zoff)
            data = self.decompress(self.f.read(zlen), size)
            require(len(data) == size, f"frame {i} decodes to {len(data)} bytes, not {size}")
            out.append(data)
            offset += size
            i += 1
        require(offset == end, "a read ends inside a record")
        return b"".join(out)

    def decoded_sha256(self) -> str:
        h = hashlib.sha256()
        for _, size, zoff, zlen in self.table:
            self.f.seek(int(zoff))
            data = self.decompress(self.f.read(int(zlen)), int(size))
            require(len(data) == size, "a frame decodes to the wrong size")
            h.update(data)
        return h.hexdigest()


# ---------------------------------------------------------------------------------------------------------------
# Recording plan, qualification and runner


def plan_record(parent: Path, out: Path, attempt: str = "first", games: int | None = None,
                chunk_games: int = CHUNK_GAMES, store_codec: str | None = None) -> dict:
    require(attempt in FROZEN_GAMES, "attempt is first or retry")
    require(store_codec in (None, "raw", STORE_CODEC), f"store codec is raw or {STORE_CODEC}")
    games = FROZEN_GAMES[attempt] if games is None else int(games)
    require(games > 0 and chunk_games > 0, "games and chunk size must be positive")
    source = read_json(parent)
    require(set(source) >= {"checkpoint", "feature_transfer", "play_import"} and source.get("s_learner") is None,
            "the parent is a net-8 source (checkpoint, feature transfer, play import)")
    checkpoint = checked(source["checkpoint"], "parent checkpoint")
    out = Path(out)
    require(not (out / "plan.json").exists(), f"{out} already holds a plan")
    chunks = [{"name": f"chunk-{first:06d}", "first_game": first, "games": min(chunk_games, games - first)}
              for first in range(0, games, chunk_games)]
    plan = {"schema": PLAN_SCHEMA, "design": "W5-DESIGN.md v1.3 section 6.2", "attempt": attempt, "games": games,
            "frozen_games": FROZEN_GAMES[attempt], "production": games == FROZEN_GAMES[attempt],
            "chunk_games": chunk_games, "base_seed": base_seed(attempt),
            "seed_rule": f"first 8 bytes little-endian of sha256('{RECORD_TAG}/{attempt}'); game seeds by "
                         "regret_census_v1 game_setup (mix(base_seed ^ mix(game)))",
            "parent": {"label": PARENT_LABEL, "source": pin(parent), "checkpoint_sha256": sha256_file(checkpoint)},
            "runtime": {"runtime_rules": RUNTIME_RULES, "max_physical_decisions": MAX_PHYSICAL_DECISIONS,
                        "max_policy_steps": MAX_POLICY_STEPS, "mulligans": "off (keep seven)",
                        "opponent": "self-play (both seats the parent)"},
            "decks": DECKS, "recorder": "regret_census_v1 mode=s4a-tensor-dump (mtg-kernel#480)",
            "split": {"tag": SPLIT_TAG, "validation_fraction": VALIDATION_FRACTION,
                      "rule": "sha256(json([tag, game]))[:8] big-endian / 2^64 < fraction"},
            "chunks": chunks, "s0_tool_sha256": sha256_file(__file__)}
    if store_codec == STORE_CODEC:   # absent: the raw store, as before
        zstd_api()
        plan["store_codec"] = {"kind": STORE_CODEC, "level": ZSTD_LEVEL}
    write_json(out / "plan.json", plan)
    return plan


def load_plan(path) -> tuple[dict, Path]:
    plan = read_json(path)
    require(plan.get("schema") == PLAN_SCHEMA, "not an S0 recording plan")
    return plan, Path(path).parent


def launcher(binary) -> list[str]:
    """A native binary, or a Python test double."""
    return [sys.executable, str(binary)] if str(binary).endswith(".py") else [str(binary)]


def chunk_command(plan: dict, binary: Path, out_stem: Path, first: int, games: int) -> tuple[list, dict]:
    source = plan["parent"]["source"]["path"]
    env = dict(os.environ, OPPONENTS=f"self={source}", S4A_MODEL=PARENT_LABEL, S4A_RUNTIME=RUNTIME_RULES,
               S4A_GAME_LIMITS=f"{MAX_PHYSICAL_DECISIONS},{MAX_POLICY_STEPS}", TD_BIN=str(out_stem) + ".bin")
    cmd = [*launcher(binary), f"source={source}", f"out={out_stem}.jsonl", "mode=s4a-tensor-dump",
           f"first_game={first}", f"games={games}", "workers=1", f"base_seed={plan['base_seed']}",
           "decks=" + ",".join(str(d) for d in plan["decks"])]
    return cmd, env


def roster_complete(stem: Path, first: int, games: int) -> bool:
    """The chunk's rows are exactly its games, each a recorded game (the recorder writes a per-game failure as an
    `error` row and still exits 0)."""
    jsonl = Path(f"{stem}.jsonl")
    if not jsonl.exists():
        return False
    seen = []
    for line in jsonl.read_text(encoding="utf-8", errors="replace").splitlines():
        if not line.strip():
            continue
        try:
            row = json.loads(line)
        except json.JSONDecodeError:
            return False
        if row.get("kind") != "td_game":
            return False
        seen.append(row.get("game"))
    return sorted(seen) == list(range(first, first + games))


def finished(stem: Path, first: int | None = None, games: int | None = None, codec: dict | None = None) -> bool:
    """The log ends `rc=0` and, given the chunk's range, the rows are exactly its recorded games; a packed store's
    chunk also has its pack receipt (written after the packed bytes decoded to the raw file)."""
    log = Path(str(stem) + ".log")
    if not log.exists():
        return False
    lines = log.read_text(encoding="utf-8", errors="replace").strip().splitlines()
    if not (bool(lines) and lines[-1].startswith("rc=0 ")):
        return False
    if codec is not None and not Path(f"{stem}.pack.json").exists():
        return False
    return first is None or roster_complete(stem, first, games)


def check_parent(plan: dict) -> None:
    """The teacher is the plan's: its source file and checkpoint still hash to the plan's pins."""
    source = checked(plan["parent"]["source"], "parent source")
    require(sha256_file(checked(read_json(source)["checkpoint"], "parent checkpoint"))
            == plan["parent"]["checkpoint_sha256"], "the parent checkpoint differs from the plan's")


def set_aside(stem: Path) -> None:
    """A previous attempt's files are kept under an attempt suffix (ARTIFACT-LAW clause 3; code/gen_chunk.sh)."""
    present = [s for s in ("log", "jsonl", "bin", "binz", "binz.tmp", "frames.npy", "pack.json")
               if Path(f"{stem}.{s}").exists()]
    if not present:
        return
    n = 1
    while Path(f"{stem}.attempt-{n}.log").exists() or Path(f"{stem}.attempt-{n}.jsonl").exists():
        n += 1
    for suffix in present:
        os.replace(f"{stem}.{suffix}", f"{stem}.attempt-{n}.{suffix}")


def start_chunk(plan: dict, binary: Path, stem: Path, first: int, games: int):
    """Starts the chunk's recorder. A packed store's chunk is locked first (held by this driver while it records,
    then by the packer), so a packer left running by a killed driver stops the resume instead of racing it."""
    check_parent(plan)
    stem.parent.mkdir(parents=True, exist_ok=True)
    lock = None
    if plan.get("store_codec") is not None:
        lock = chunk_lock(stem)
        require(lock is not None, f"{stem.name} is locked by a recorder or packer still running; stop it first")
    set_aside(stem)
    cmd, env = chunk_command(plan, binary, stem, first, games)
    log = open(str(stem) + ".log", "w", encoding="utf-8")
    return subprocess.Popen(cmd, env=env, stdout=log, stderr=subprocess.STDOUT), log, time.time(), lock


def close_census(proc, log, stem: Path, first: int, games: int) -> int:
    rc = proc.wait()
    log.close()
    if rc == 0 and not roster_complete(stem, first, games):
        rc = -100   # exited 0 with failed or missing games
    return rc


def start_pack(stem: Path, codec: dict):
    """Packs the chunk in its own process (the slot stays busy), its output appended to the chunk's log."""
    log = open(str(stem) + ".log", "a", encoding="utf-8")
    cmd = [sys.executable, "-B", str(Path(__file__).resolve()), "pack-chunk", "--stem", str(stem),
           "--level", str(codec["level"])]
    return subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT), log


def end_chunk(stem: Path, first: int, games: int, rc: int) -> None:
    note = {-100: " (failed or missing games)", -101: " (packing failed)"}.get(rc, "")
    with open(str(stem) + ".log", "a", encoding="utf-8") as f:
        f.write(f"rc={rc} first={first} games={games}{note}\n")


def run_chunks(plan: dict, binary: Path, items: list[tuple[Path, int, int]], parallel: int) -> list[dict]:
    """Runs (stem, first, games) items, at most `parallel` processes at once; returns per-chunk results. With the
    plan's store codec, a recorded chunk is packed in the same slot before its `rc` line is written."""
    codec = plan.get("store_codec")
    pending, running, results = list(items), [], []
    try:
        while pending or running:
            while pending and len(running) < parallel:
                stem, first, games = pending.pop(0)
                proc, log, t0, lock = start_chunk(plan, binary, stem, first, games)
                running.append({"proc": proc, "log": log, "stem": stem, "first": first, "games": games, "t0": t0,
                                "phase": "record", "lock": lock})
            time.sleep(0.05)
            for job in list(running):
                if job["proc"].poll() is None:
                    continue
                stem, first, games = job["stem"], job["first"], job["games"]
                if job["phase"] == "record":
                    rc = close_census(job["proc"], job["log"], stem, first, games)
                    if job["lock"] is not None:   # the packer takes the lock over
                        job["lock"].close()
                        job["lock"] = None
                    if rc == 0 and codec is not None:
                        job["proc"], job["log"] = start_pack(stem, codec)
                        job["phase"] = "pack"
                        continue
                else:
                    rc = job["proc"].wait()
                    job["log"].close()
                    rc = 0 if rc == 0 else -101
                running.remove(job)
                end_chunk(stem, first, games, rc)
                results.append({"chunk": stem.name, "rc": rc, "games": games, "seconds": time.time() - job["t0"]})
    except BaseException:
        # A refused start (a locked chunk, a changed teacher) or an interrupt: stop and reap every child before the
        # driver's locks are released, so no recorder or packer outlives the run unprotected. Their chunks have no
        # `rc` line and rerun.
        for job in running:
            if job["proc"].poll() is None:
                job["proc"].kill()
            job["proc"].wait()
            job["log"].close()
            if job["lock"] is not None:
                job["lock"].close()
        raise
    return results


def host_identity() -> dict:
    return {"host": platform.node(), "machine": platform.machine(), "system": platform.system(),
            "logical_cpus": os.cpu_count()}


def qualify_record(plan_path, binary, parallel: list[int], games: int = 64) -> dict:
    """Completed games per second for each process count, one fresh chunk of `games` per process, on game
    indices past the plan (never in the store)."""
    plan, root = load_plan(plan_path)
    binary = Path(binary)
    qroot = root / "qualification"
    trials = []
    offset = plan["games"] + 1_000_000
    for count in parallel:
        require(count >= 1, "process counts are positive")
        items = [(qroot / f"p{count}" / f"q-{i}", offset + i * games, games) for i in range(count)]
        t0 = time.time()
        results = run_chunks(plan, binary, items, count)
        wall = time.time() - t0
        require(all(r["rc"] == 0 for r in results), f"qualification chunk failed at {count} processes")
        trials.append({"processes": count, "games": count * games, "wall_seconds": wall,
                       "games_per_second": count * games / wall})
        offset += count * games
    best = max(trials, key=lambda t: t["games_per_second"])
    receipt = {"schema": QUAL_SCHEMA, "plan_sha256": sha256_file(plan_path), "binary_sha256": sha256_file(binary),
               "host": host_identity(), "games_per_chunk": games, "trials": trials,
               "selected_processes": best["processes"], "selected_games_per_second": best["games_per_second"]}
    write_json(qroot / "receipt.json", receipt)
    return receipt


def record(plan_path, binary, parallel: int | None = None, limit_chunks: int | None = None) -> dict:
    plan, root = load_plan(plan_path)
    binary = Path(binary)
    if plan["games"] > SMOKE_GAMES_MAX:
        qual = root / "qualification" / "receipt.json"
        require(qual.exists(), "a production recording needs `qualify-record` first (COMPUTE-POLICY)")
        receipt = read_json(qual)
        require(receipt["binary_sha256"] == sha256_file(binary) and receipt["plan_sha256"] == sha256_file(plan_path)
                and receipt["host"]["host"] == platform.node(),
                "the record qualification covers another binary, plan or host")
        parallel = receipt["selected_processes"] if parallel is None else parallel
    parallel = parallel or 1
    check_parent(plan)
    codec = plan.get("store_codec")
    todo = [c for c in plan["chunks"]
            if not finished(root / "store" / c["name"], c["first_game"], c["games"], codec)]
    if limit_chunks is not None:
        todo = todo[:limit_chunks]
    t0 = time.time()
    results = run_chunks(plan, binary, [(root / "store" / c["name"], c["first_game"], c["games"]) for c in todo],
                         parallel)
    summary = {"chunks_run": len(results), "failed": [r["chunk"] for r in results if r["rc"] != 0],
               "games": sum(r["games"] for r in results), "wall_seconds": time.time() - t0, "parallel": parallel,
               "remaining": sum(1 for c in plan["chunks"]
                                if not finished(root / "store" / c["name"], c["first_game"], c["games"], codec))}
    if summary["wall_seconds"] > 0:
        summary["games_per_second"] = summary["games"] / summary["wall_seconds"]
    with open(root / "record-log.jsonl", "a", encoding="utf-8") as f:
        f.write(json.dumps({"binary_sha256": sha256_file(binary), "host": host_identity(), **summary}) + "\n")
    return summary


# ---------------------------------------------------------------------------------------------------------------
# Index


def index_store(plan_path) -> dict:
    """The canonical index: games in game order, decisions in game order. Every chunk must have finished and
    every planned game appears exactly once on the plan's runtime and limits."""
    plan, root = load_plan(plan_path)
    store = root / "store"
    codec = plan.get("store_codec")
    cols = {k: [] for k in ("fid", "offset", "length", "outcome", "has_value", "game", "actor", "turn",
                            "actions") + (("zoffset", "zlength") if codec else ())}
    games_out, files, chunks_out = [], [], []
    value_targets = {"not_natural": 0, "natural_draws": 0}
    decisions_total = 0
    for fid, chunk in enumerate(plan["chunks"]):
        stem = store / chunk["name"]
        require(finished(stem, chunk["first_game"], chunk["games"], codec), f"{chunk['name']} has not finished")
        bin_path, jsonl = Path(f"{stem}.bin"), Path(f"{stem}.jsonl")
        rows = {}
        for line in jsonl.read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            row = json.loads(line)
            require(row.get("kind") == "td_game", f"{jsonl.name}: a non-game row: {line[:160]}")
            g = int(row["game"])
            require(g not in rows, f"{jsonl.name}: game {g} twice")
            rows[g] = row
        want = set(range(chunk["first_game"], chunk["first_game"] + chunk["games"]))
        require(set(rows) == want, f"{chunk['name']}: games differ from the plan's")
        if codec is None:
            files.append(f"store/{chunk['name']}.bin")
            chunks_out.append({"name": chunk["name"], "bin": pin(bin_path), "jsonl": pin(jsonl),
                               "log": pin(Path(f"{stem}.log")), "bytes": bin_path.stat().st_size})
            fb = RawChunk(bin_path)
        else:
            # The packed file decodes to the recorder's bytes (the pack receipt's raw SHA-256), checked again here;
            # the game blocks below are hashed from the decoded bytes.
            receipt, table = check_pack(stem, codec)
            fb = PackedChunk(f"{stem}.binz", table)
            require(fb.decoded_sha256() == receipt["decoded"]["sha256"],
                    f"{chunk['name']}: decoded bytes differ from the recorded file")
            files.append(f"store/{chunk['name']}.binz")
            chunks_out.append({"name": chunk["name"], "packed": receipt["packed"], "frames": receipt["frames"],
                               "pack": pin(Path(f"{stem}.pack.json")), "decoded": receipt["decoded"],
                               "jsonl": pin(jsonl), "log": pin(Path(f"{stem}.log")),
                               "bytes": receipt["decoded"]["bytes"],
                               "packed_bytes": Path(f"{stem}.binz").stat().st_size})
            frame_of = {int(o): i for i, o in enumerate(table[:, 0])}
        try:
            for g in sorted(rows):
                row = rows[g]
                require(row.get("runtime_rules") == RUNTIME_RULES and row.get("limits") == {
                    "max_physical_decisions": MAX_PHYSICAL_DECISIONS, "max_policy_steps": MAX_POLICY_STEPS},
                    f"game {g} was not recorded on the training runtime and limits")
                block = fb.read(row["bin_offset"], row["bin_bytes"])
                require(len(block) == row["bin_bytes"], f"game {g}: short block")
                natural = row["classification"] == "Natural"
                pos, decoded = 0, []
                for d in row["decisions"]:
                    step, actor, legal, selected, turn, start, length = d
                    require(start == pos, f"game {g}: record offsets are not contiguous")
                    rec, end = decode_record(block, start)
                    require(end - start == length and rec["actor"] == actor and rec["legal"] == legal
                            and rec["selected"] == selected, f"game {g}: record header differs from its row")
                    decoded.append((actor, turn, start, length, legal))
                    pos = end
                require(pos == len(block), f"game {g}: trailing bytes in its block")
                val = is_validation(g)
                games_out.append({"game": g, "decks": row["decks"], "focal_seat": row["focal_seat"],
                                  "starting_player": row["starting_player"], "winner": row["winner"],
                                  "classification": row["classification"], "decisions": len(decoded),
                                  "block_sha256": sha256_bytes(block), "validation": val, "has_value": natural})
                decisions_total += len(decoded)
                if not natural:
                    value_targets["not_natural"] += 1
                elif row["winner"] is None:
                    value_targets["natural_draws"] += 1
                for actor, turn, start, length, legal in decoded:
                    cols["fid"].append(fid)
                    cols["offset"].append(row["bin_offset"] + start)
                    cols["length"].append(length)
                    cols["has_value"].append(natural)
                    cols["outcome"].append(0.0 if not natural else 0.5 if row["winner"] is None
                                           else 1.0 if actor == row["winner"] else 0.0)
                    cols["game"].append(g)
                    cols["actor"].append(actor)
                    cols["turn"].append(turn)
                    cols["actions"].append(legal)
                    if codec:
                        i = frame_of[row["bin_offset"] + start]
                        require(int(table[i, 1]) == length, f"game {g}: a record is not one frame")
                        cols["zoffset"].append(int(table[i, 2]))
                        cols["zlength"].append(int(table[i, 3]))
        finally:
            fb.close()
    dtypes = {"fid": np.int32, "offset": np.int64, "length": np.int32, "outcome": np.float32, "has_value": bool,
              "game": np.int64,
              "actor": np.uint8, "turn": np.int32, "actions": np.int32, "zoffset": np.int64, "zlength": np.int32}
    arrays = {k: np.asarray(v, dtypes[k]) for k, v in cols.items()}
    arrays["validation"] = np.asarray([is_validation(int(g)) for g in arrays["game"]], bool)
    arrays["files"] = np.asarray(files)
    np.savez(root / "index.npz", **arrays)
    games_pin = write_json(root / "index.games.json", {"games": games_out})
    val_games = sum(1 for g in games_out if g["validation"])
    manifest = {"schema": INDEX_SCHEMA, "plan": pin(plan_path), "index": pin(root / "index.npz"),
                "games_file": games_pin, "chunks": chunks_out,
                "counts": {"games": len(games_out), "games_with_value": len(games_out) - value_targets["not_natural"],
                           "validation_games": val_games, "decisions_recorded": decisions_total,
                           "decisions_indexed": int(arrays["game"].size),
                           "decisions_with_value": int(arrays["has_value"].sum()),
                           "validation_decisions": int(arrays["validation"].sum()),
                           "store_bytes": sum(c["bytes"] for c in chunks_out)},
                "value_targets": value_targets}
    if codec:
        manifest["codec"] = codec
        manifest["counts"]["packed_bytes"] = sum(c["packed_bytes"] for c in chunks_out)
    write_json(root / "manifest.json", manifest)
    return manifest


def verify_store(manifest_path) -> dict:
    """Every chunk's tensor file against its manifest pin, in the index's file order (once, before training)."""
    index, manifest, root = load_index(manifest_path)
    codec = manifest.get("codec")
    suffix, key = ("bin", "bin") if codec is None else ("binz", "packed")
    require([str(f) for f in index["files"]] == [f"store/{c['name']}.{suffix}" for c in manifest["chunks"]],
            "index files differ from the manifest")
    for chunk, name in zip(manifest["chunks"], index["files"]):
        path = root / str(name)   # the file the trainer reads, wherever the store now lives
        require(path.exists() and sha256_file(path) == chunk[key]["sha256"],
                f"{chunk['name']} tensors differ from the manifest")
    out = {"chunks": len(manifest["chunks"]), "bytes": sum(c["bytes"] for c in manifest["chunks"])}
    if codec is not None:   # the packed bytes, which the index decoded to the recorded ones
        out["packed_bytes"] = sum(c["packed_bytes"] for c in manifest["chunks"])
        out["codec"] = codec
    return out


def verify_games(index: dict, root: Path, rows) -> int:
    """The blocks of the games holding `rows` against their recorded block hashes (index.games.json)."""
    games = {g["game"]: g for g in read_json(root / "index.games.json")["games"]}
    plan = read_json(root / "plan.json")
    chunk_pins = {c["name"]: c for c in read_json(root / "manifest.json")["chunks"]}
    codec = plan.get("store_codec")
    wanted = sorted({int(index["game"][r]) for r in rows})
    rows_of = {}
    for chunk in plan["chunks"]:
        if any(chunk["first_game"] <= g < chunk["first_game"] + chunk["games"] for g in wanted):
            for line in (root / "store" / f"{chunk['name']}.jsonl").read_text(encoding="utf-8").splitlines():
                if line.strip():
                    row = json.loads(line)
                    rows_of[row["game"]] = (chunk["name"], row)
    by_chunk = {}
    for g in wanted:
        by_chunk.setdefault(rows_of[g][0], []).append(g)
    for name, chunk_games in by_chunk.items():   # one reader (one pin check) per chunk
        reader = open_chunk(root, chunk_pins[name], codec)
        try:
            for g in chunk_games:
                row = rows_of[g][1]
                require(sha256_bytes(reader.read(row["bin_offset"], row["bin_bytes"])) == games[g]["block_sha256"],
                        f"game {g}: block differs from the index")
        finally:
            reader.close()
    return len(wanted)


class RecordReader:
    """Indexed records (rows of index.npz) with one positioned read each; a packed store's record is its zstd
    frame (the index's zoffset and zlength) decoded to `length` bytes."""

    def __init__(self, index: dict, root: Path, codec: dict | None):
        self.ix, self.root, self.codec, self._fd = index, Path(root), codec, {}
        self.decompress = None
        if codec is not None:
            require(codec.get("kind") == STORE_CODEC and "zoffset" in index and "zlength" in index,
                    f"a packed store is {STORE_CODEC} with packed coordinates in its index")
            self.decompress = zstd_api()[3]

    def close(self):
        for fd in self._fd.values():
            os.close(fd)
        self._fd = {}

    def __del__(self):
        self.close()

    def fd(self, f: int):
        if f not in self._fd:
            path = self.root / str(self.ix["files"][f])
            self._fd[f] = os.open(path, os.O_RDONLY | getattr(os, "O_BINARY", 0) | getattr(os, "O_RANDOM", 0))
        return self._fd[f]

    def read(self, r: int) -> bytes:
        ix = self.ix
        fd, n = self.fd(int(ix["fid"][r])), int(ix["length"][r])
        if self.codec is None:
            os.lseek(fd, int(ix["offset"][r]), os.SEEK_SET)
            return os.read(fd, n)
        zl = int(ix["zlength"][r])
        os.lseek(fd, int(ix["zoffset"][r]), os.SEEK_SET)
        frame = os.read(fd, zl)
        require(len(frame) == zl, f"record {r}: short frame")
        return self.decompress(frame, n)


def load_index(manifest_path, verify: bool = True) -> tuple[dict, dict, Path]:
    manifest = read_json(manifest_path)
    require(manifest.get("schema") == INDEX_SCHEMA, "not an S0 store manifest")
    root = Path(manifest_path).parent
    if verify:   # the index file that is loaded, beside the manifest
        require(sha256_file(root / "index.npz") == manifest["index"]["sha256"], "store index differs from its pin")
    z = np.load(root / "index.npz", allow_pickle=False)
    return {k: z[k] for k in z.files}, manifest, root


# ---------------------------------------------------------------------------------------------------------------
# Frozen config


def make_config(plan_path, out, grad_clip_norm, smoke: bool = False, eval_every: int | None = None,
                max_steps: int | None = None) -> dict:
    import s0_recipe as R
    plan, root = load_plan(plan_path)
    manifest_path = root / "manifest.json"
    require(manifest_path.exists(), "index the store first")
    manifest = read_json(manifest_path)
    checked(manifest["plan"], "plan")
    checked(manifest["index"], "index")
    checked(manifest["games_file"], "games file")
    require(smoke or plan["production"], "a production config needs the frozen data size (100k, or 200k on retry)")
    require(smoke or (eval_every is None and max_steps is None), "evaluation and step overrides are smoke only")
    clip = None if grad_clip_norm in (None, "none") else float(grad_clip_norm)
    recipe = R.recipe(clip)
    if smoke:
        recipe["smoke_overrides"] = {"eval_every": eval_every, "max_steps": max_steps}
    config = {"schema": CONFIG_SCHEMA, "design": "W5-DESIGN.md v1.3 section 6.2", "smoke": bool(smoke),
              "store": {"manifest": pin(manifest_path), "plan": manifest["plan"], "index": manifest["index"],
                        "games_file": manifest["games_file"]},
              "recipe": recipe}
    write_json(out, config)
    return config


# ---------------------------------------------------------------------------------------------------------------
# Export and parity


REFERENCE = REPO / "python" / "tools" / "s_transformer_reference_v1.py"


def run_json(cmd: list, env=None) -> dict:
    done = subprocess.run(cmd, capture_output=True, text=True, env=env)
    require(done.returncode == 0, f"{Path(cmd[0]).name} {cmd[1] if len(cmd) > 1 else ''} failed: "
                                  f"{done.stderr.strip()[-600:] or done.stdout.strip()[-600:]}")
    return json.loads(done.stdout)


def parity_states(index: dict, root: Path, count: int, out: Path, codec: dict | None = None) -> dict:
    """The first `count` validation decisions in index order, concatenated as a tensor-dump file."""
    rows = np.nonzero(index["validation"])[0][:count]
    require(rows.size > 0, "no validation decisions")
    reader = RecordReader(index, root, codec)
    try:
        with open(out, "wb") as f:
            for r in rows:
                buf = reader.read(int(r))
                require(len(buf) == int(index["length"][r]), f"record {r}: short read")
                f.write(buf)
    finally:
        reader.close()
    return {"decisions": int(rows.size), "first_row": int(rows[0]), "last_row": int(rows[-1]), **pin(out)}


def export(config_path, run_dir, binary, parity_decisions: int = PARITY_DECISIONS) -> dict:
    config = read_json(config_path)
    require(config.get("schema") == CONFIG_SCHEMA, "not an S0 config")
    require(config["smoke"] or parity_decisions == PARITY_DECISIONS,
            f"a production export checks exactly {PARITY_DECISIONS} validation decisions")
    run_dir = Path(run_dir)
    train = read_json(run_dir / "train.json")
    require(train["config_sha256"] == sha256_file(config_path), "the run was trained from another config")
    require(train["status"] in ("complete", "stopped"), f"the run did not finish: {train['status']}")
    best = run_dir / "best.pt"
    require(best.exists() and sha256_file(best) == train["best_pt_sha256"], "best.pt differs from the run receipt")
    index, manifest, root = load_index(config["store"]["manifest"]["path"])
    require(sha256_file(Path(config["store"]["manifest"]["path"])) == config["store"]["manifest"]["sha256"],
            "store manifest differs from the config")
    out = run_dir / "export"
    out.mkdir(exist_ok=True)
    py = sys.executable
    values = run_json([py, str(REFERENCE), "export-values", "--state-dict", str(best), "--out-dir", str(out)])
    container = out / "s0.bin"
    imported = run_json([*launcher(binary), "import-values", f"values={out / 'values.bin'}",
                         f"names={out / 'names.json'}", f"out={container}"])
    require(imported["adam_step"] == 0, "the native S0 must carry a fresh optimizer")
    states = parity_states(index, root, parity_decisions, out / "parity-states.td", manifest.get("codec"))
    states["game_blocks_verified"] = verify_games(index, root, np.nonzero(index["validation"])[0][:parity_decisions])
    native = run_json([*launcher(binary), "logits-dump", f"out={out / 'native.logits'}",
                       f"model=s-checkpoint:{container}", f"states=td:{out / 'parity-states.td'}"])
    torch_side = run_json([py, str(REFERENCE), "torch-logits", "--state-dict", str(best),
                           "--states", str(out / "parity-states.td"), "--out", str(out / "torch.logits")])
    done = subprocess.run([py, str(REFERENCE), "compare-logits", str(out / "native.logits"),
                           str(out / "torch.logits"), "--receipt", str(out / "parity.json"),
                           "--bar", str(PARITY_BAR)], capture_output=True, text=True)
    parity = read_json(out / "parity.json")
    plan = read_json(root / "plan.json")
    require(sha256_file(root / "plan.json") == manifest["plan"]["sha256"], "store plan differs from its pin")
    receipt = {"schema": EXPORT_SCHEMA, "design": "W5-DESIGN.md v1.3 section 6.2 (export)",
               "teacher": {"label": plan["parent"]["label"], "checkpoint_sha256": plan["parent"]["checkpoint_sha256"]},
               "config": pin(config_path), "smoke": bool(config["smoke"]), "run": {"train": pin(run_dir / "train.json"), "best_pt": pin(best),
                                                    "best_step": train["best_step"]},
               "values": values, "import": imported, "container": pin(container),
               "parity": {"bar": PARITY_BAR, "decisions_required": parity_decisions, "states": states,
                          "native": native, "torch": torch_side, "receipt": pin(out / "parity.json"),
                          "max_abs_logit_difference": parity["max_abs_logit_difference"],
                          "max_abs_value_difference": parity["max_abs_value_difference"],
                          "argmax_mismatches": parity["argmax_mismatches"],
                          "torch_side": "the trained state dict scored in float64 "
                                        "(s_transformer_reference_v1.py torch-logits)"},
               "passed": bool(done.returncode == 0 and parity["max_abs_logit_difference"] <= PARITY_BAR
                              and states["decisions"] == parity_decisions
                              and parity["states"] == states["decisions"]),
               "binary_sha256": sha256_file(binary), "reference_sha256": sha256_file(REFERENCE)}
    if states["decisions"] < parity_decisions:
        receipt["short"] = f"only {states['decisions']} validation decisions exist"
    write_json(out / "export.json", receipt)
    return receipt


# ---------------------------------------------------------------------------------------------------------------
# XS's frozen start


def start_extras(seed_roots=XS_SEED_ROOTS, first_update: int = XS_FIRST_UPDATE) -> list[list[str]]:
    """XS's RNG states and data cursors at the switch to native RL: every RNG stream of an RT1 chain is derived
    from its seed root (deals, seats, sampling and search seeds), and the data cursor is the first global
    update. Named, in this order."""
    return [["rng/seed_roots", json.dumps(list(seed_roots), separators=(",", ":")).encode().hex()],
            ["cursor/first_update", struct.pack("<Q", first_update).hex()]]


def freeze_start(export_path, binary, out, smoke: bool = False) -> dict:
    out = Path(out)
    require(not out.exists(), f"{out} exists: XS's start is frozen once")
    exp = read_json(export_path)
    require(exp.get("schema") == EXPORT_SCHEMA and exp["passed"], "a passing S0 export receipt is required")
    require(exp["smoke"] == smoke, "a smoke export is frozen only with --smoke, a production one only without it")
    require(smoke or exp["parity"]["states"]["decisions"] == PARITY_DECISIONS,
            f"a production start needs the {PARITY_DECISIONS}-decision parity")
    container = checked(exp["container"], "S0 container")
    extras = out.with_suffix(".extras.json")
    extras.write_text(json.dumps(start_extras()), encoding="utf-8", newline="\n")
    native = run_json([*launcher(binary), "starting-state", f"checkpoint={container}", f"extras={extras}"])
    require(native["checkpoint_sha256"] == exp["container"]["sha256"] and native["adam_step"] == 0,
            "starting state does not read the exported S0")
    doc = {"schema": START_SCHEMA, "design": "W5-DESIGN.md v1.3 sections 6.2 and 8 (XS's complete starting state)",
           "export": pin(export_path), "container": exp["container"], "extras": pin(extras), "teacher": exp["teacher"],
           "extras_meaning": {"rng/seed_roots": "XS chain seed roots w5/chain1..6 (JSON, UTF-8)",
                              "cursor/first_update": "the first global update, u64 little-endian"},
           "adam": "reset: zero first and second moments, step 0 (ruling 6)", "native": native,
           "starting_state_sha256": native["starting_state_sha256"], "binary_sha256": sha256_file(binary),
           "smoke": bool(smoke),
           "frozen_at": time.strftime("%Y-%m-%dT%H:%M:%S%z")}
    write_json(out, doc)
    return doc


def make_source(shell_path, container, out) -> dict:
    """The RT1 S source on the plain FlatV4 shell (s_learner_v1.rs): the shell's play import and feature
    transfer, no checkpoint, and S0 as `s_learner.initial`."""
    shell = read_json(shell_path)
    require(shell.get("s_learner") is None, "the shell is a plain FlatV4 source")
    source = {k: v for k, v in shell.items() if k not in ("checkpoint", "s_learner")}
    source["checkpoint"] = None
    source["s_learner"] = {"schema": "w5-s-learner/v1", "initial": pin(container)}
    write_json(out, source)
    return source


# ---------------------------------------------------------------------------------------------------------------
# Qualification floor


def _panel_tools():
    sys.path.insert(0, str(REPO / "tools" / "spy_reset"))
    import panel as panel_tool
    import pilot
    return panel_tool, pilot


def anchor_floor_panel(panel: dict) -> None:
    """The floor's opponent and decks are the frozen H panel's, which Step C's pinned manifest pins."""
    sys.path.insert(0, str(HERE))
    from h_look import sha256_values
    require(panel.get("schema") == "w5-h-panel/v1" and panel["namespace"] == H_NAMESPACE,
            "the floor plays the frozen H panel (Step C namespace a)")
    manifest = read_json(checked(panel["step_c_manifest"], "Step C panel manifest"))
    pinned = sha256_values(manifest)
    for key in ("opponent", "decks"):
        require(panel[key]["sha256"].lower() in pinned, f"the panel's {key} is not pinned by Step C's manifest")


H_REBIND_SCHEMA = "w5-s0-h-rebind/v1"
H_HEAD_FIELD = "destination_build_git_head"


def rebound_h(opponent_bytes: bytes, import_bytes: bytes, envelope_bytes: bytes, head: str, envelope_path,
              import_path) -> tuple[bytes, bytes, bytes]:
    """Step C's H rebound to the build `head` (the D1, OH2 and Step C rebind): the V3 transfer envelope records its
    destination build commit and the loader reproduces the envelope, so H loads only in a binary of that commit. The
    envelope changes in that one field (one occurrence of the old head in its bytes); the import and the opponent
    change only in the pin (path and hash) of the file below them. Returns the rebound (envelope, import, opponent)."""
    require(len(head) == 40 and all(c in "0123456789abcdef" for c in head), "the build head must be a full commit")
    opponent, imp, envelope = json.loads(opponent_bytes), json.loads(import_bytes), json.loads(envelope_bytes)
    require(opponent["source"]["play_import"]["sha256"] == sha256_bytes(import_bytes), "the import is not H's")
    require(imp["transfer_envelope"]["sha256"] == sha256_bytes(envelope_bytes), "the envelope is not H's import's")
    old = envelope["receipt"][H_HEAD_FIELD]
    require(len(old) == 40 and old != head, "the envelope is already bound to this build")
    text = envelope_bytes.decode("utf-8")
    field = f'"{H_HEAD_FIELD}":"{old}"'
    field = field if text.count(field) else field.replace(":", ": ")
    require(text.count(field) == 1 and text.count(old) == 1, "expected exactly one occurrence of the old head")
    new_envelope = text.replace(field, field.replace(old, head), 1).encode("utf-8")
    parsed = json.loads(new_envelope)
    require(parsed["receipt"][H_HEAD_FIELD] == head, "the rebind did not set the head")
    parsed["receipt"][H_HEAD_FIELD] = old
    require(parsed == envelope, "parsed contents differ beyond the one field")
    imp = dict(imp, transfer_envelope={"path": Path(envelope_path).as_posix(), "sha256": sha256_bytes(new_envelope)})
    new_import = (json.dumps(imp, indent=2) + "\n").encode("utf-8")
    opponent = dict(opponent, source=dict(opponent["source"], play_import={
        "path": Path(import_path).as_posix(), "sha256": sha256_bytes(new_import)}))
    new_opponent = (json.dumps(opponent, indent=1) + "\n").encode("utf-8")
    return new_envelope, new_import, new_opponent


def rebind_h(panel_def, import_file, envelope_file, head: str, out) -> dict:
    """H rebound to the floor build's `head`, written under `out` with its record. `import_file` and `envelope_file`
    are Step C's H files at their current location (moved trees), checked against the pins in the panel's opponent."""
    out = Path(out)
    require(not (out / "h-rebind.json").exists(), f"{out} already holds an H rebind")
    panel = read_json(panel_def)
    anchor_floor_panel(panel)
    files = {"opponent": checked(panel["opponent"], "opponent H"), "import": Path(import_file),
             "envelope": Path(envelope_file)}
    raw = {k: p.read_bytes() for k, p in files.items()}
    targets = {"envelope": out / f"h-v3-transfer-envelope-{head[:8]}.json",
               "import": out / f"h-v3-play-import-source-{head[:8]}.json",
               "opponent": out / f"h-opponent-{head[:8]}.json"}
    rebound = rebound_h(raw["opponent"], raw["import"], raw["envelope"], head, targets["envelope"], targets["import"])
    require(not any(p.exists() for p in targets.values()), f"{out} already holds rebound H files")
    out.mkdir(parents=True, exist_ok=True)
    for key, data in zip(("envelope", "import", "opponent"), rebound):
        targets[key].write_bytes(data)
    record = {"schema": H_REBIND_SCHEMA,
              "design": "W5-DESIGN.md v1.3 section 6.2; lead ruling 2026-11-05 E4 item 4 (both floor panels on one "
                        "S-capable build, so Step C's H is rebound to that build as D1, OH2 and Step C did)",
              "field": f"receipt.{H_HEAD_FIELD}", "old": json.loads(raw["envelope"])["receipt"][H_HEAD_FIELD],
              "new": head, "from": {k: pin(p) for k, p in files.items()},
              "to": {k: pin(p) for k, p in targets.items()}, "panel_def": pin(panel_def),
              "s0_tool_sha256": sha256_file(__file__)}
    write_json(out / "h-rebind.json", record)
    return record


def admit_h_rebind(panel: dict, rebind_path) -> dict:
    """The floor's opponent on a build other than Step C's: H rebound from the panel's pinned opponent, re-derived here
    byte for byte from Step C's files."""
    record = read_json(rebind_path)
    require(record.get("schema") == H_REBIND_SCHEMA, "not an H rebind record")
    require(record["from"]["opponent"]["sha256"] == panel["opponent"]["sha256"],
            "the rebind does not start from the panel's opponent")
    raw = {k: checked(record["from"][k], f"Step C H {k}").read_bytes() for k in ("opponent", "import", "envelope")}
    to = {k: checked(record["to"][k], f"rebound H {k}") for k in ("envelope", "import", "opponent")}
    expected = rebound_h(raw["opponent"], raw["import"], raw["envelope"], record["new"], to["envelope"], to["import"])
    for key, data in zip(("envelope", "import", "opponent"), expected):
        require(to[key].read_bytes() == data, f"rebound H {key} differs from its re-derivation")
    return read_json(to["opponent"])


def floor_plan(s0_source, b512_source, panel_def, xs_start, evaluator, out, h_rebind=None) -> dict:
    """With `h_rebind` (a `rebind-h` record), the floor plays H rebound to the evaluator's build; without it, the
    panel's opponent bytes as Step C pinned them, which load only in Step C's build."""
    out = Path(out)
    require(not (out / "floor.json").exists(), f"{out} already holds a floor plan")
    start = read_json(xs_start)
    require(start.get("schema") == START_SCHEMA, "XS's frozen start must be written before the floor")
    require(start.get("smoke") is False, "the floor qualifies a production S0 only, not a smoke start")
    s0 = read_json(s0_source)
    require(s0.get("checkpoint") is None, "the floor plays S0 itself: an S source without a checkpoint record")
    require(s0.get("s_learner", {}).get("initial", {}).get("sha256") == start["container"]["sha256"],
            "the S0 source does not load the frozen start's container")
    checked(s0["s_learner"]["initial"], "S0 container")
    b512 = read_json(b512_source)
    require(b512.get("s_learner") is None, "the b512 source is net-8")
    require(sha256_file(checked(b512["checkpoint"], "b512 checkpoint")) == start["teacher"]["checkpoint_sha256"],
            "the floor's comparator is not S0's distillation teacher")
    panel = read_json(panel_def)
    anchor_floor_panel(panel)
    opponent = read_json(checked(panel["opponent"], "opponent H")) if h_rebind is None else \
        admit_h_rebind(panel, h_rebind)
    decks = read_json(checked(panel["decks"], "decks"))
    panel_tool, _ = _panel_tools()
    plans = {}
    for role, source in (("s0", s0), ("b512", b512)):
        annotations = {"w5_s0_floor": {"role": role, "xs_start_sha256": sha256_file(xs_start),
                                       "panel_def_sha256": sha256_file(panel_def)}}
        panel_tool.build(f"w5-s0-floor-{role}", source, "H", opponent, decks, out / role, int(panel["shards"]),
                         namespace=H_NAMESPACE, annotations=annotations)
        plans[role] = pin(out / role / "plan.json")
    floor = {"schema": FLOOR_SCHEMA, "design": "W5-DESIGN.md v1.3 section 6.2 (qualification floor)",
             "namespace": H_NAMESPACE, "floor": FLOOR, "xs_start": pin(xs_start),
             "starting_state_sha256": start["starting_state_sha256"], "s0_source": pin(s0_source),
             "b512_source": pin(b512_source), "panel_def": pin(panel_def), "evaluator": pin(evaluator),
             "plans": plans, "s0_tool_sha256": sha256_file(__file__)}
    if h_rebind is not None:
        floor["h_rebind"] = pin(h_rebind)
    write_json(out / "floor.json", floor)
    return floor


RUNNER = REPO / "python" / "tools" / "nine_deck_public_panel_v1.py"


def runner(*args: str) -> None:
    done = subprocess.run([sys.executable, str(RUNNER), *args])
    require(done.returncode == 0, f"panel runner {args[0]} failed")


FLOOR_WORKERS = "1,2,4,8"
FLOOR_QUALIFY_SAMPLE = 16


def floor_run(floor_path, evaluator, workers: str = FLOOR_WORKERS) -> dict:
    """Qualify, then run, both panels on the floor's one evaluator; start inside the host's held reservation, as
    the panel runner's `qualify` and `run` require (tools/w5/h_look.py run). `workers` is the qualification's
    worker-count ladder; the runner selects the count from its measured throughput."""
    floor = read_json(floor_path)
    require(floor.get("schema") == FLOOR_SCHEMA, "not an S0 floor")
    require(sha256_file(evaluator) == floor["evaluator"]["sha256"], "the evaluator is not the floor's")
    ladder = [int(x) for x in workers.split(",")]
    require(ladder and ladder[0] == 1 and all(a < b for a, b in zip(ladder, ladder[1:]))
            and all(FLOOR_QUALIFY_SAMPLE % k == 0 for k in ladder),
            f"the worker ladder starts at 1, increases and divides the {FLOOR_QUALIFY_SAMPLE}-game sample")
    for role in ("s0", "b512"):
        plan_path = checked(floor["plans"][role], f"{role} plan")
        root = plan_path.parent
        receipt = root / "qualification" / "receipt.json"
        if not receipt.exists():
            runner("qualify", "--plan", str(plan_path), "--sample", str(FLOOR_QUALIFY_SAMPLE), "--workers", workers,
                   "--root", str(root / "qualification"), "--executable", str(evaluator))
        require(receipt.exists(), f"{role}: qualification wrote no receipt")
        runner("run", "--plan", str(plan_path), "--receipt", str(receipt), "--executable", str(evaluator))
    return {"ran": str(floor_path)}


def panel_cases(rows_path: Path, panel_name: str, namespace: str) -> dict:
    """{(own, other, repeat, seat): score or None} with the exact frozen roster and seeds."""
    _, pilot = _panel_tools()
    expected = {(o, t, k, s) for o in range(pilot.DECKS) for t in range(pilot.DECKS)
                for k in range(pilot.PANEL_REPEATS) for s in (0, 1)}
    cases = {}
    for line in Path(rows_path).read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        r = json.loads(line)
        key = (r["own"], r["other"], r["repeat"], r["seat"])
        require(all(type(v) is int for v in key) and key in expected and key not in cases,
                f"{rows_path}: duplicate or unknown case {key}")
        require(r.get("panel") == panel_name and r.get("seed") == pilot.panel_seed(*key[:3], namespace),
                f"{rows_path}: panel or seed differs at {key}")
        complete = r.get("complete")
        require(type(complete) is bool and ((complete and r.get("score") in (0, 0.5, 1))
                                            or (not complete and r.get("score") is None)),
                f"{rows_path}: invalid completion or score at {key}")
        cases[key] = float(r["score"]) if complete else None
    require(set(cases) == expected, f"{rows_path}: the case roster differs from the frozen panel")
    return cases


def signed_score(cases: dict, fill: float, decks: list[int]) -> tuple[float, dict]:
    """D1's signed score 2 x (win + draw / 2) - 1, equal weight over focal decks (STEP-C-DESIGN.md section 3),
    missing games at `fill`."""
    per = {}
    for o in decks:
        vals = [fill if v is None else v for (own, _, _, _), v in cases.items() if own == o]
        per[o] = 2.0 * sum(vals) / len(vals) - 1.0
    return sum(per.values()) / len(per), per


def read_floor_rows(s0_rows, b512_rows, s0_panel: str, b512_panel: str, namespace: str = H_NAMESPACE) -> dict:
    _, pilot = _panel_tools()
    eight = [d for d in range(pilot.DECKS) if d != pilot.SPY]
    s0 = panel_cases(Path(s0_rows), s0_panel, namespace)
    b = panel_cases(Path(b512_rows), b512_panel, namespace)
    fills = {"least_favorable": (0.0, 1.0), "most_favorable": (1.0, 0.0)}   # (S0 missing, b512 missing)
    out = {}
    for name, (fs, fb) in fills.items():
        ss, sper = signed_score(s0, fs, eight)
        bs, bper = signed_score(b, fb, eight)
        out[name] = {"s0": ss, "b512": bs, "difference": ss - bs,
                     "per_deck_difference": {str(d): sper[d] - bper[d] for d in eight}}
    decided = out["least_favorable"]["difference"]
    return {"schema": FLOOR_RECEIPT_SCHEMA, "design": "W5-DESIGN.md v1.3 section 6.2 (qualification floor)",
            "namespace": namespace, "scored_decks": eight, "games_each": len(s0),
            "missing": {"s0": sum(v is None for v in s0.values()), "b512": sum(v is None for v in b.values())},
            "fills": out, "floor": FLOOR,
            "rule": "S0 minus b512 signed score over the eight non-Spy focal decks at least the floor; missing games "
                    "at the contrast's least favorable fill (S0 lost, b512 won), Step C's both-fills rule",
            "difference": decided, "passed": bool(decided >= FLOOR),
            "rows": {"s0": pin(s0_rows), "b512": pin(b512_rows)}}


def floor_read(floor_path, out=None) -> dict:
    floor = read_json(floor_path)
    require(floor.get("schema") == FLOOR_SCHEMA, "not an S0 floor")
    rows = {}
    for role in ("s0", "b512"):
        plan_path = checked(floor["plans"][role], f"{role} plan")
        rows[role] = plan_path.parent / "rows.jsonl"
        runner("rows", "--plan", str(plan_path), "--out", str(rows[role]))
    plans = {role: read_json(floor["plans"][role]["path"]) for role in ("s0", "b512")}
    receipt = read_floor_rows(rows["s0"], rows["b512"], plans["s0"]["panel"], plans["b512"]["panel"],
                              floor["namespace"])
    receipt.update(floor=floor["floor"], floor_plan=pin(floor_path), evaluator_sha256=floor["evaluator"]["sha256"],
                   starting_state_sha256=floor["starting_state_sha256"])
    receipt["passed"] = bool(receipt["difference"] >= floor["floor"])
    write_json(out or Path(floor_path).parent / "floor-receipt.json", receipt)
    return receipt


# ---------------------------------------------------------------------------------------------------------------


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="command", required=True)
    p = sub.add_parser("plan-record")
    p.add_argument("--parent", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--attempt", choices=list(FROZEN_GAMES), default="first")
    p.add_argument("--games", type=int, default=None, help="smoke size; the frozen size when omitted")
    p.add_argument("--chunk-games", type=int, default=CHUNK_GAMES)
    p.add_argument("--store-codec", choices=["raw", STORE_CODEC], default="raw",
                   help=f"{STORE_CODEC}: each chunk packed record by record after it is recorded (zstd level "
                        f"{ZSTD_LEVEL}), decoded bytes checked against the recorded file before it is removed")
    p = sub.add_parser("pack-chunk", help="(run by `record`) pack one recorded chunk")
    p.add_argument("--stem", type=Path, required=True)
    p.add_argument("--level", type=int, default=ZSTD_LEVEL)
    p = sub.add_parser("qualify-record")
    p.add_argument("--plan", type=Path, required=True)
    p.add_argument("--binary", type=Path, required=True)
    p.add_argument("--parallel", default="1,2,4")
    p.add_argument("--games", type=int, default=64,
                   help="games per process; the census start-up (about 6 s) dominates smaller chunks")
    p = sub.add_parser("record")
    p.add_argument("--plan", type=Path, required=True)
    p.add_argument("--binary", type=Path, required=True)
    p.add_argument("--parallel", type=int, default=None)
    p.add_argument("--limit-chunks", type=int, default=None)
    p = sub.add_parser("index")
    p.add_argument("--plan", type=Path, required=True)
    p = sub.add_parser("config")
    p.add_argument("--plan", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--grad-clip-norm", required=True,
                   help="open lead decision (design silent): 'none' or the pilot's 1.0")
    p.add_argument("--smoke", action="store_true")
    p.add_argument("--eval-every", type=int, default=None)
    p.add_argument("--max-steps", type=int, default=None)
    p = sub.add_parser("export")
    p.add_argument("--config", type=Path, required=True)
    p.add_argument("--run", type=Path, required=True)
    p.add_argument("--binary", type=Path, required=True)
    p.add_argument("--parity-decisions", type=int, default=PARITY_DECISIONS)
    p = sub.add_parser("freeze-start")
    p.add_argument("--smoke", action="store_true", help="freeze a smoke export (the floor never admits it)")
    p.add_argument("--export", type=Path, required=True)
    p.add_argument("--binary", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p = sub.add_parser("source")
    p.add_argument("--shell", type=Path, required=True)
    p.add_argument("--container", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p = sub.add_parser("rebind-h")
    for name in ("--panel-def", "--import", "--envelope", "--out"):
        p.add_argument(name, type=Path, required=True)
    p.add_argument("--head", required=True, help="the floor evaluator's full build commit")
    p = sub.add_parser("floor-plan")
    for name in ("--s0-source", "--b512-source", "--panel-def", "--xs-start", "--evaluator", "--out"):
        p.add_argument(name, type=Path, required=True)
    p.add_argument("--h-rebind", type=Path, default=None, help="a rebind-h record: H rebound to the evaluator's build")
    p = sub.add_parser("floor-run")
    p.add_argument("--floor", type=Path, required=True)
    p.add_argument("--evaluator", type=Path, required=True)
    p.add_argument("--workers", default=FLOOR_WORKERS, help="qualification worker-count ladder")
    p = sub.add_parser("floor-read")
    p.add_argument("--floor", type=Path, required=True)
    p.add_argument("--out", type=Path, default=None)
    a = ap.parse_args(argv)
    sys.path.insert(0, str(HERE))
    try:
        if a.command == "plan-record":
            result = plan_record(a.parent, a.out, a.attempt, a.games, a.chunk_games, a.store_codec)
            result = {k: v for k, v in result.items() if k != "chunks"}
        elif a.command == "pack-chunk":
            result = pack_chunk(a.stem, a.level)
        elif a.command == "qualify-record":
            result = qualify_record(a.plan, a.binary, [int(x) for x in a.parallel.split(",")], a.games)
        elif a.command == "record":
            result = record(a.plan, a.binary, a.parallel, a.limit_chunks)
        elif a.command == "index":
            result = index_store(a.plan)
        elif a.command == "config":
            result = make_config(a.plan, a.out, a.grad_clip_norm, a.smoke, a.eval_every, a.max_steps)
        elif a.command == "export":
            result = export(a.config, a.run, a.binary, a.parity_decisions)
        elif a.command == "freeze-start":
            result = freeze_start(a.export, a.binary, a.out, a.smoke)
        elif a.command == "source":
            result = make_source(a.shell, a.container, a.out)
        elif a.command == "rebind-h":
            result = rebind_h(a.panel_def, getattr(a, "import"), a.envelope, a.head, a.out)
        elif a.command == "floor-plan":
            result = floor_plan(a.s0_source, a.b512_source, a.panel_def, a.xs_start, a.evaluator, a.out, a.h_rebind)
        elif a.command == "floor-run":
            result = floor_run(a.floor, a.evaluator, a.workers)
        else:
            result = floor_read(a.floor, a.out)
    except S0Error as exc:
        print(f"refused: {exc}", file=sys.stderr)
        return 2
    print(json.dumps(result, indent=1, sort_keys=True))
    if a.command == "export" and not result["passed"]:
        return 1
    if a.command == "floor-read" and not result["passed"]:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
