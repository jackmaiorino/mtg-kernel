"""The B width pre-screen's single read (collab-lead LANES/b-width-prescreen-20261027/SCREEN-NOTE.md v1.2,
frozen at 12aab0f2).

Standard library only. Every constant comes from the driver's config, whose ``analyzer.sha256`` must equal this
file's own SHA-256; the analyzer refuses to run otherwise. Inputs are the driver's records under one run root
(``mtg-kernel`` ``expanded_deck_training_v1::b_width_prescreen_v1``), the parent files the config pins, and the SHA-256
of ``data/SECTION7.json`` as the note's section 7 records it. A different hash, or any expected input that is
missing or cannot be read (a premature run, a host without the parents, a drive error), is refused before anything
is written; only a content or hash mismatch makes the read VOID.

The checks run in the note's section 6 order (data, the 48-hour wall cap, attrition, fits, pairing, function
preservation, numerical validity, sensitivity control, decision). Any failure makes the read VOID; an invalid
analysis is never "not funded".

Inputs: the required inputs (LAUNCH.json, SECTION7.json, each chain's data, collection, store and preservation
records, each fit's report or refusal, the torn attempt records each collection record lists, and each parent's
pinned files: the checkpoint, the import and the import's direct pins, discovered from the import's own hashed
bytes) plus every other file found under the root's record folders are read in full and hashed before the
analysis, which consumes exactly those bytes. A required input that is absent refuses; so does any file or folder
that cannot be listed or read. The read's fingerprint is built from those hashes, SECTION7.json's must be the one
section 7 records, and every input is inventoried and hashed again before publication (any change refuses).

The read is bound to the run root: it is written once to ``<root>/READ.json`` (``--out`` only exports a copy), and
any later run on the same root is refused unless ``--rerender`` re-renders that read from identical inputs, at its
recorded read time, to the same read (every field but ``root``, so a moved copy of the root re-renders too). An
exclusive lock, ``<root>/.READ.lock``, serializes the runs that may publish a read; a re-render writes nothing in
the root, takes no lock and so also runs from a read-only copy. The read is written in full to a temporary file in
the root, synced, and published by a hard link that never replaces an existing READ.json, so a crash leaves no read
or a complete one; the run root's volume must support hard links (NTFS, POSIX file systems), and the analyzer
refuses rather than fall back to a replacing rename. A later run that finds a complete unpublished read (a crash
between writing and publishing) re-renders it at its recorded read time and publishes it only if the inputs and the
bytes are the same, so there are never two completed reads to choose between. A temporary that cannot be read
refuses the run and is kept; one that was read in full and is not a whole read is removed.

Exit status: 0 when a read (any verdict) was written or re-rendered (a lock or export that could not be completed
after publication is reported as a warning), 2 when the analyzer refused to run.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import stat
import struct
import sys
import time
from pathlib import Path

READ_SCHEMA = "mtg-kernel-b-width-prescreen-read/v1"
CONFIG_SCHEMA = "mtg-kernel-b-width-prescreen-config/v1"
DATA_SCHEMA = "mtg-kernel-b-width-prescreen-data/v1"
COLLECTION_SCHEMA = "mtg-kernel-b-width-prescreen-collection/v1"
SECTION7_SCHEMA = "mtg-kernel-b-width-prescreen-section7/v1"
PRESERVATION_SCHEMA = "mtg-kernel-b-width-prescreen-preservation/v1"
FIT_SCHEMA = "mtg-kernel-b-width-prescreen-fit/v1"
LAUNCH_SCHEMA = "mtg-kernel-b-width-prescreen-launch/v1"
STORE_MAGIC = b"mtg-kernel-b-width-prescreen-store/v1\n"
DRAW_ORDER = "occurrence-seeds-then-pairs-v1"
READ_NAME = "READ.json"
LOCK_NAME = ".READ.lock"
RECORD_FOLDERS = ("data", "preserve", "fits", "collect")
ARMS = ("w64", "w128", "w32")
ARCHITECTURES = {"w64": "kernel-policy-value-net-8", "w128": "kernel-policy-value-net-8w128",
                 "w32": "kernel-policy-value-net-8"}
CONSTRUCTIONS = {"w64": "identity", "w128": "net8-to-net8w128-function-preserving-widening-v1",
                 "w32": "net8-w32-upper-half-fan-out-zeroed-control-v1"}
# The driver's interrupted publication: ``.<name>.tmp-<pid>``.
TEMPORARY = re.compile(r"^\.(.+)\.tmp-(\d+)$")
# This analyzer's own unpublished read.
READ_TEMPORARY = re.compile(r"^\.READ\.json\.tmp-\d+$")
# The fingerprint's value for a folder under the record folders (its files are listed by their own paths).
DIRECTORY = "directory"
SECTION7_REFUSAL = "data/SECTION7.json does not have the SHA-256 section 7 records; nothing was read"
MASK64 = (1 << 64) - 1
CONFIG_KEYS = {
    "": {"schema", "note", "input_generation", "feature_contract_digest", "feature_encoding_digest", "decks",
         "collection", "chains", "arms", "fitting", "qualification", "analysis", "analyzer"},
    "collection": {"games_per_pair", "fit_indices", "holdout_indices", "seed_domain", "focal_seat",
                   "starting_player_rule", "seat_rule_basis", "max_physical_decisions", "max_policy_steps",
                   "runtime_rules", "postboard", "collection_sampler", "training_math"},
    "fitting": {"value_target_source", "learning_rate", "value_coefficient", "batch_groups", "epochs",
                "heldout_curve_points", "fitting_seeds", "minibatch_seed_domain", "widen_seed_domain"},
    "analysis": {"bootstrap_draws", "bootstrap_seed_domain", "bootstrap_seed_index", "bootstrap_draw_order",
                 "interval_lower_index", "interval_upper_index", "fund_lower_bound_above", "fund_min_point_r",
                 "t_critical_df7", "max_pooled_nonnatural_holdout_fraction", "max_chain_nonnatural_holdout",
                 "max_chain_nonnatural_fit", "min_holdout_games_per_pair", "wall_cap_hours"},
    "analyzer": {"path", "sha256"},
}


def current_time() -> float:
    """The read time (seconds since the epoch). Tests replace it."""
    return time.time()


class Refused(Exception):
    """The analyzer does not run (no read is written)."""


class Void(Exception):
    """A section 6 check failed: the read is VOID."""

    def __init__(self, check: str, detail: object) -> None:
        super().__init__(f"{check}: {detail}")
        self.check = check
        self.detail = detail


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def domain_seed(domain: str, *parts: int) -> int:
    """First 8 bytes, little-endian, of SHA-256 of the UTF-8 string ``domain|p1|p2...`` (decimal integers)."""
    text = "|".join([domain, *(str(part) for part in parts)])
    return int.from_bytes(hashlib.sha256(text.encode("utf-8")).digest()[:8], "little")


class SplitMix64:
    def __init__(self, seed: int) -> None:
        self.state = seed & MASK64

    def next_u64(self) -> int:
        self.state = (self.state + 0x9E3779B97F4A7C15) & MASK64
        z = self.state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK64
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK64
        return z ^ (z >> 31)

    def index(self, n: int) -> int:
        """An index in [0, n): ``(next_u64 * n) >> 64``."""
        return (self.next_u64() * n) >> 64


def f32_bits(value: float) -> int:
    return struct.unpack("<I", struct.pack("<f", value))[0]


def bits_view(value: object) -> object:
    """A JSON value with every float replaced by its exact hex form, so equality is bit equality (sign of zero
    included)."""
    if isinstance(value, float):
        return ("f64", value.hex())
    if isinstance(value, list):
        return [bits_view(item) for item in value]
    if isinstance(value, dict):
        return {key: bits_view(item) for key, item in value.items()}
    return value


def finite(value: object) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def number(value: float) -> object:
    """A float for the read file: non-finite values become strings, so the file stays valid JSON."""
    return value if math.isfinite(value) else repr(value)


def require(ok: bool, check: str, detail: object) -> None:
    if not ok:
        raise Void(check, detail)


def open_input(path: Path):
    """Opens a file for reading: the analyzer's one I/O primitive (tests replace it to simulate unreadable files)."""
    return open(path, "rb")


def read_input(path: Path) -> bytes:
    """A file's bytes. A file that cannot be read is not a data outcome: the analyzer refuses (nothing is written)
    instead of reading VOID."""
    try:
        with open_input(path) as handle:
            return handle.read()
    except OSError as error:
        raise Refused(f"{path} cannot be read: {error}") from None


def hash_input(path: Path) -> str:
    """A file's SHA-256, streamed, refusing on any I/O error."""
    digest = hashlib.sha256()
    try:
        with open_input(path) as handle:
            for chunk in iter(lambda: handle.read(1 << 20), b""):
                digest.update(chunk)
    except OSError as error:
        raise Refused(f"{path} cannot be read: {error}") from None
    return digest.hexdigest()


def scan_directory(directory: Path) -> list:
    """A directory's entries, sorted by name. Any listing error refuses: a scan never skips what it cannot read."""
    try:
        with os.scandir(directory) as iterator:
            return sorted(iterator, key=lambda entry: entry.name)
    except OSError as error:
        raise Refused(f"{directory} cannot be listed: {error}") from None


def path_kind(path: Path) -> str | None:
    """``"file"``, ``"directory"``, or None if nothing is there. Any other error, or another kind of entry,
    refuses."""
    try:
        mode = os.stat(path).st_mode
    except (FileNotFoundError, NotADirectoryError):
        return None
    except OSError as error:
        raise Refused(f"{path} cannot be examined: {error}") from None
    if stat.S_ISREG(mode):
        return "file"
    if stat.S_ISDIR(mode):
        return "directory"
    raise Refused(f"{path} is neither a file nor a folder")


def walk_folder(root: Path, directory: Path, found: dict[str, Path | None]) -> None:
    """Every entry under ``directory``, by root-relative label: a file by its path, a folder (label ending in
    ``/``) as None. Any error, or an entry that is neither a regular file nor a folder, refuses."""
    for entry in scan_directory(directory):
        path = Path(entry.path)
        label = path.relative_to(root).as_posix()
        try:
            is_folder = entry.is_dir(follow_symlinks=False)
            is_file = not is_folder and entry.is_file(follow_symlinks=False)
        except OSError as error:
            raise Refused(f"{path} cannot be examined: {error}") from None
        if is_folder:
            found[label + "/"] = None
            walk_folder(root, path, found)
        elif is_file:
            found[label] = path
        else:
            raise Refused(f"{path} is neither a regular file nor a folder")


def check_config(config: dict) -> None:
    for section, keys in CONFIG_KEYS.items():
        actual = config if section == "" else config.get(section)
        if not isinstance(actual, dict) or set(actual) != keys:
            raise Refused(f"config section {section or 'top level'!r} does not have exactly the expected fields")
    if config["schema"] != CONFIG_SCHEMA:
        raise Refused("config schema differs")
    analysis = config["analysis"]
    if analysis["bootstrap_draw_order"] != DRAW_ORDER:
        raise Refused("unsupported bootstrap draw order")
    if not 0 <= analysis["interval_lower_index"] < analysis["interval_upper_index"] < analysis["bootstrap_draws"]:
        raise Refused("invalid interval indices")
    if config["arms"]["names"] != list(ARMS):
        raise Refused("arms must be w64, w128 and w32")
    fitting = config["fitting"]
    if fitting["batch_groups"] < 1 or fitting["epochs"] < 1 or fitting["heldout_curve_points"] < 1:
        raise Refused("invalid fitting constants")


def pairs_of(config: dict) -> list[tuple[int, int]]:
    decks = len(config["decks"])
    return [(focal, opposing) for focal in range(decks) for opposing in range(decks)]


def expected_schedule(config: dict, k: int) -> list[tuple[str, str, int, int]]:
    """A chain's scheduled games in the driver's schedule order: (id, split, focal, opposing)."""
    collection = config["collection"]
    schedule = []
    for focal, opposing in pairs_of(config):
        for index in range(collection["games_per_pair"]):
            split = "fit" if index in collection["fit_indices"] else "holdout"
            schedule.append((f"bwps-k{k}-f{focal}-o{opposing}-i{index}", split, focal, opposing))
    return schedule


def step_rule(fitting: dict, fit_groups: int) -> tuple[int, int]:
    """Section 4: steps = ceil(epochs * fit_groups / batch_groups), held-out every floor(steps / points)."""
    steps = -(-(fitting["epochs"] * fit_groups) // fitting["batch_groups"])
    return steps, steps // fitting["heldout_curve_points"]


def evaluation_steps(steps: int, every: int) -> list[int]:
    """The fit entry's held-out evaluation points: before the first update, every ``every`` updates, and after
    the last."""
    points = [0] + list(range(every, steps + 1, every))
    if points[-1] != steps:
        points.append(steps)
    return points


def nested_pins(value: object) -> list[dict]:
    """Every ``{"path", "sha256"}`` object inside one JSON document, as the driver's staging check reads them."""
    if isinstance(value, dict):
        if set(value) == {"path", "sha256"} and isinstance(value["path"], str):
            return [value]
        return [pin for item in value.values() for pin in nested_pins(item)]
    if isinstance(value, list):
        return [pin for item in value for pin in nested_pins(item)]
    return []


def direct_pins(import_bytes: bytes) -> list[dict]:
    """The import's direct pins: every ``{path, sha256}`` object inside the import document. With the checkpoint
    and the import itself they are what the engine loads and the driver's staging check verifies. Files those
    direct pins name in turn (a manifest's historical registry, say) are provenance the engine never opens, so they
    are neither required, read nor bound. An import that does not parse has none (its pinned hash then fails)."""
    try:
        return nested_pins(json.loads(import_bytes))
    except ValueError:
        return []


def torn_attempt_pattern(k: int) -> re.Pattern:
    """A chain's retained torn attempt record (the driver's ``collect/torn-attempts/k<k>/torn-attempt-<i>.<n>``)."""
    return re.compile(rf"^collect/torn-attempts/k{k}/torn-attempt-(\d{{4}})\.\d+$")


def torn_attempt_paths(collection_bytes: bytes, k: int) -> list[str]:
    """The retained torn attempt records a chain's collection record lists (well-formed paths only; anything else
    is the data check's to reject)."""
    try:
        value = json.loads(collection_bytes)
    except ValueError:
        return []
    rows = value.get("torn_attempts") if isinstance(value, dict) else None
    if not isinstance(rows, list):
        return []
    pattern = torn_attempt_pattern(k)
    return [row["path"] for row in rows
            if isinstance(row, dict) and isinstance(row.get("path"), str) and pattern.match(row["path"])]


class Inventory:
    """Every file a read binds, enumerated without skipping anything: the required inputs, built explicitly (the
    run's records, each fit's report or refusal, the torn attempt records each collection record lists, and each
    parent's checkpoint, import and direct pins, the pins discovered from the import's own hashed bytes), and every
    other file or folder found under the root's record folders. A required input that is absent, or anything that
    cannot be listed or read, refuses."""

    def __init__(self, config: dict, root: Path) -> None:
        discovered: dict[str, Path | None] = {}
        for folder in RECORD_FOLDERS:
            kind = path_kind(root / folder)
            if kind == "directory":
                walk_folder(root, root / folder, discovered)
            elif kind == "file":
                raise Refused(f"{root / folder} is not a folder")
        required: dict[str, Path] = {}
        missing: list[str] = []
        # Files read whole here (imports and collection records): their hashes are of these very bytes, and what
        # they name is derived from the same bytes.
        self.digests: dict[str, str] = {}
        self.pins: dict[int, list[dict]] = {}
        import_pins: dict[str, list[dict]] = {}

        def need(label: str, path: Path) -> bool:
            if path_kind(path) == "file":
                required[label] = path
                return True
            missing.append(label)
            return False

        def read_whole(label: str, path: Path) -> bytes:
            data = read_input(path)
            self.digests[label] = sha256_bytes(data)
            return data

        need("LAUNCH.json", root / "LAUNCH.json")
        need("data/SECTION7.json", root / "data" / "SECTION7.json")
        for chain in config["chains"]:
            k = chain["index"]
            for relative in (f"data/k{k}.json", f"collect/k{k}/games.bin", f"preserve/k{k}.json"):
                need(relative, root / relative)
            relative = f"collect/k{k}/collection.json"
            if need(relative, root / relative):
                for torn in torn_attempt_paths(read_whole(relative, root / relative), k):
                    need(torn, root / torn)
            for s in config["fitting"]["fitting_seeds"]:
                for arm in ARMS:
                    name = f"k{k}-s{s}-{arm}"
                    forms = [label for label in discovered if label in (f"fits/{name}.json",
                                                                         f"fits/{name}.refused.json")
                             or (label.startswith(f"fits/.{name}.refused.json.tmp-")
                                 and TEMPORARY.match(label[len("fits/"):]))]
                    for label in forms:
                        need(label, root / label)
                    if not forms:
                        missing.append(f"fits/{name}.json")
            source = chain["source"]
            pins = [source["checkpoint"], source["play_import"]]
            path = source["play_import"]["path"]
            if need(path, Path(path)):
                if path not in import_pins:
                    import_pins[path] = direct_pins(read_whole(path, Path(path)))
                pins += import_pins[path]
            unique: dict[str, dict] = {}
            for pin in pins:
                unique.setdefault(pin["path"], pin)
            self.pins[k] = list(unique.values())
            for pin in self.pins[k]:
                need(pin["path"], Path(pin["path"]))
        if missing:
            # A premature run (a stage unfinished), a host without the parents, or a lost file is not a data
            # outcome: refuse without writing anything.
            missing = list(dict.fromkeys(missing))
            raise Refused(f"{len(missing)} expected inputs are missing, so nothing was read: "
                          + ", ".join(missing[:20]))
        self.paths: dict[str, Path | None] = {**discovered, **required}

    def hashes(self) -> dict[str, str]:
        """Every bound file's SHA-256, each read in full (any I/O error refuses), and each folder's marker."""
        return {label: DIRECTORY if path is None else self.digests.get(label) or hash_input(path)
                for label, path in sorted(self.paths.items())}


def fingerprint(config_sha256: str, analyzer_sha256: str, section7_sha256: str, hashes: dict[str, str]) -> str:
    document = {"config": config_sha256, "analyzer": analyzer_sha256, "section7": section7_sha256,
                "files": sorted(hashes.items())}
    return sha256_bytes(json.dumps(document, sort_keys=True).encode("utf-8"))


def fingerprint_of(config: dict, root: Path, config_sha256: str, analyzer_sha256: str, section7_sha256: str) -> str:
    return fingerprint(config_sha256, analyzer_sha256, section7_sha256, Inventory(config, root).hashes())


class Inputs:
    """The files the analysis consumes. Each is read in full and must have the SHA-256 the inventory recorded
    before the analysis began (else the input changed during the read: refused), so the fingerprint is bound to
    the bytes analyzed."""

    def __init__(self, root: Path, hashes: dict[str, str]) -> None:
        self.root = root
        self.hashes = hashes
        self.files: dict[str, str] = {}

    def _bytes(self, label: str, path: Path) -> bytes:
        data = read_input(path)
        if self.hashes.get(label) != sha256_bytes(data):
            raise Refused(f"{label} changed during the read (or is not in its inventory); nothing was written")
        return data

    def read(self, relative: str, check: str) -> tuple[dict, bytes]:
        data = self.read_bytes(relative, check)
        try:
            return json.loads(data), data
        except json.JSONDecodeError as error:
            raise Void(check, f"{relative}: {error}") from None

    def read_bytes(self, relative: str, check: str) -> bytes:
        data = self._bytes(relative, self.root / relative)
        self.files[relative] = sha256_bytes(data)
        return data

    def read_pin(self, pin: dict, check: str, what: str) -> bytes:
        data = self._bytes(pin["path"], Path(pin["path"]))
        require(sha256_bytes(data) == pin["sha256"], check, f"{what} {pin['path']} differs from its pinned SHA-256")
        return data


def game_error(row: dict, check: str) -> float:
    groups = row.get("groups")
    total = row.get("value_squared_error_sum")
    require(isinstance(groups, int) and groups > 0, check, f"{row.get('game_id')}: group count {groups!r}")
    require(finite(total), check, f"{row.get('game_id')}: value_squared_error_sum {total!r}")
    value = total / groups
    require(math.isfinite(value), check, f"{row.get('game_id')}: per-game error {value!r}")
    return value


def pair_weighted_loss(errors: dict[str, float], pair_games: dict[tuple[int, int], list[str]]) -> float:
    """Equal-weight mean over ordered pairs of the mean per-game error of each pair's retained holdout games."""
    return sum(sum(errors[game] for game in games) / len(games) for games in pair_games.values()) / len(pair_games)


def check_torn_attempts(k: int, collection: dict, inputs: Inputs) -> None:
    """A chain's retained torn attempt records (attempt records a crash tore while they were first written; the
    driver set them aside and played those games normally). They are audit evidence, not analysis inputs: the
    files under ``collect/torn-attempts/k<k>/`` must be exactly the ones the collection record lists, with its
    sizes and hashes."""
    rows = collection.get("torn_attempts")
    require(isinstance(rows, list), "data", f"chain {k}: the collection record has no torn attempt list")
    pattern = torn_attempt_pattern(k)
    listed = []
    for row in rows:
        path = row.get("path") if isinstance(row, dict) else None
        match = pattern.match(path) if isinstance(path, str) else None
        require(match is not None and row.get("index") == int(match.group(1)), "data",
                f"chain {k}: torn attempt entry {row!r} is not a retained record of its game")
        data = inputs.read_bytes(path, "data")
        require(len(data) == row.get("bytes") and sha256_bytes(data) == row.get("sha256"), "data",
                f"chain {k}: {path} differs from its collection record entry")
        listed.append(path)
    prefix = f"collect/torn-attempts/k{k}/"
    present = sorted(label for label in inputs.hashes if label.startswith(prefix) and label != prefix)
    require(present == sorted(listed) and len(listed) == len(set(listed)), "data",
            {"chain": k, "torn_attempts_listed": sorted(listed), "torn_attempts_present": present})


def check_data(config: dict, config_sha256: str, inputs: Inputs, section7: dict,
               pins: dict[int, list[dict]]) -> dict[int, dict]:
    """Section 6.1, reading the pinned bytes themselves: the parents and their imports, each chain's store, its
    collection and data records, and section 7."""
    chains = [chain["index"] for chain in config["chains"]]
    require(section7.get("schema") == SECTION7_SCHEMA and section7.get("config_sha256") == config_sha256
            and section7.get("analyzer") == config["analyzer"] and isinstance(section7.get("engine_commit"), str),
            "data", "SECTION7.json is not this config's and analyzer's")
    frozen = {entry["chain"]: entry for entry in section7.get("chains", [])}
    require(sorted(frozen) == sorted(chains) and len(frozen) == len(section7.get("chains", [])), "data",
            "SECTION7.json does not list exactly the config's chains")
    # One build throughout: the launch's engine commit and binary are section 7's (and every record's, below).
    launch, _ = inputs.read("LAUNCH.json", "data")
    build = (section7.get("engine_commit"), section7.get("binary_sha256"))
    require(isinstance(build[1], str) and (launch.get("engine_commit"), launch.get("binary_sha256")) == build,
            "data", "LAUNCH.json and SECTION7.json name different builds")
    fitting = config["fitting"]
    chain_data: dict[int, dict] = {}
    for chain in config["chains"]:
        k = chain["index"]
        entry = frozen[k]
        source = chain["source"]
        for pin in pins[k]:
            inputs.read_pin(pin, "data", f"chain {k} parent, import or import input")
        record, record_bytes = inputs.read(f"data/k{k}.json", "data")
        collection, collection_bytes = inputs.read(f"collect/k{k}/collection.json", "data")
        store = inputs.read_bytes(f"collect/k{k}/games.bin", "data")
        parent = source["checkpoint"]["sha256"]
        require(record.get("schema") == DATA_SCHEMA and record.get("config_sha256") == config_sha256
                and record.get("chain") == k, "data", f"chain {k}: data record is not this config's")
        require(sha256_bytes(record_bytes) == entry["data_record_sha256"], "data",
                f"chain {k}: data record differs from section 7")
        require(sha256_bytes(collection_bytes) == entry["collection_record_sha256"]
                == record["collection_record_sha256"], "data", f"chain {k}: collection record differs from section 7")
        require(collection.get("schema") == COLLECTION_SCHEMA and collection.get("config_sha256") == config_sha256
                and collection.get("chain") == k, "data", f"chain {k}: collection record is not this config's")
        require((collection.get("engine_commit"), collection.get("binary_sha256")) == build, "data",
                f"chain {k}: collection built by another engine build")
        check_torn_attempts(k, collection, inputs)
        require(parent == record["parent_checkpoint_sha256"] == entry["parent_checkpoint_sha256"]
                == collection["source"]["checkpoint"]["sha256"], "data", f"chain {k}: parent hash differs")
        require(collection["parent_state_sha256"] == record["parent_state_sha256"], "data",
                f"chain {k}: parent state differs")
        require(collection["feature_contract_digest"] == config["feature_contract_digest"]
                and collection["feature_encoding_digest"] == config["feature_encoding_digest"], "data",
                f"chain {k}: feature digests differ from section 1")
        # The store's bytes, and every group count read from the store itself.
        require(sha256_bytes(store) == collection["store_sha256"] == record["store_sha256"]
                == entry["store_sha256"] and store.startswith(STORE_MAGIC), "data",
                f"chain {k}: the store differs from its records")
        schedule = expected_schedule(config, k)
        games = collection["games"]
        require([game["plan"]["id"] for game in games] == [row[0] for row in schedule], "data",
                f"chain {k}: the collection's games are not the scheduled games")
        attempted = {"fit": [], "holdout": []}
        natural = {"fit": [], "holdout": []}
        excluded = []
        groups_of = {}
        substeps = 0
        for game, (game_id, split, _, _) in zip(games, schedule):
            attempted[split].append(game_id)
            require(game["plan"]["split"] == split, "data", f"{game_id}: split differs from the config")
            if game["sha256"] is None:
                require(not game["natural"], "data", f"{game_id}: natural game without stored decisions")
                excluded.append(game_id)
                continue
            require(game["natural"], "data", f"{game_id}: stored decisions of an excluded game")
            start, end = game["offset"], game["offset"] + game["length"]
            require(len(STORE_MAGIC) <= start and end <= len(store) and game["length"] >= 4, "data",
                    f"{game_id}: outside the store")
            require(sha256_bytes(store[start:end]) == game["sha256"], "data", f"{game_id}: stored bytes differ")
            groups = int.from_bytes(store[start:start + 4], "little")
            require(groups > 0 and groups == game["groups"], "data", f"{game_id}: group count differs from the store")
            require(isinstance(game["substeps"], int) and game["substeps"] >= groups, "data",
                    f"{game_id}: fewer substeps than groups")
            natural[split].append(game_id)
            groups_of[game_id] = groups
            substeps += game["substeps"]
        require(record["attempted"] == attempted, "data", f"chain {k}: attempted ids differ from the config")
        require(record["natural"] == natural, "data", f"chain {k}: retained ids differ from the collection")
        require(sorted(row["id"] for row in record["excluded"]) == sorted(excluded), "data",
                f"chain {k}: excluded ids differ from the collection")
        for split in ("fit", "holdout"):
            retained = set(natural[split])
            dropped = set(attempted[split]) & set(excluded)
            require(retained.isdisjoint(dropped) and retained | dropped == set(attempted[split])
                    and len(retained) + len(dropped) == len(attempted[split]), "data",
                    f"chain {k}: natural plus excluded is not attempted ({split})")
        fit_groups = sum(groups_of[game] for game in natural["fit"])
        holdout_groups = sum(groups_of[game] for game in natural["holdout"])
        require(record["fit_groups"] == fit_groups and record["holdout_groups"] == holdout_groups, "data",
                f"chain {k}: group totals differ from the store")
        require([(game["id"], game["groups"]) for game in record["holdout_games"]]
                == [(game, groups_of[game]) for game in natural["holdout"]], "data",
                f"chain {k}: holdout games differ from the store")
        steps, every = step_rule(fitting, fit_groups)
        require(record["steps"] == steps and record["heldout_every_steps"] == every, "data",
                f"chain {k}: steps {record['steps']}/{record['heldout_every_steps']} differ from the section 4 "
                f"rule's {steps}/{every}")
        for field in ("attempted", "natural", "excluded", "outcome_source_sha256", "decision_source_sha256",
                      "fit_groups", "steps"):
            require(record[field] == entry[field], "data", f"chain {k}: {field} differs from section 7")
        chain_data[k] = {"record": record, "sha256": sha256_bytes(record_bytes), "store_groups": groups_of,
                         "substeps": substeps,
                         "schedule": {row[0]: row for row in schedule}}
    return chain_data


def check_wall_cap(config: dict, config_sha256: str, inputs: Inputs, section7: dict, read_at: float) -> dict:
    launch, launch_bytes = inputs.read("LAUNCH.json", "wall_cap")
    frozen = section7.get("launch", {})
    require(launch.get("schema") == LAUNCH_SCHEMA and launch.get("config_sha256") == config_sha256
            and sha256_bytes(launch_bytes) == frozen.get("sha256")
            and isinstance(launch.get("launched_at_unix"), int)
            and launch["launched_at_unix"] == frozen.get("launched_at_unix"), "wall_cap",
            "LAUNCH.json is not the launch section 7 records")
    elapsed = read_at - launch["launched_at_unix"]
    cap = config["analysis"]["wall_cap_hours"] * 3600.0
    require(0 <= elapsed <= cap, "wall_cap",
            f"{elapsed / 3600.0:.3f} h from the collection launch to the read; the cap is {cap / 3600.0} h")
    return {"launched_at_unix": launch["launched_at_unix"], "elapsed_hours": elapsed / 3600.0}


def analyze(config: dict, config_sha256: str, root: Path, read_at: float, hashes: dict[str, str],
            pins: dict[int, list[dict]]) -> tuple[dict, dict]:
    """Returns (read body without verdict fields, input file hashes); raises Void on a failed check."""
    inputs = Inputs(root, hashes)
    checks: list[dict] = []
    body: dict = {"checks": checks}
    chains = [chain["index"] for chain in config["chains"]]
    seeds = config["fitting"]["fitting_seeds"]
    analysis = config["analysis"]
    fitting = config["fitting"]
    pairs = pairs_of(config)
    section7, _ = inputs.read("data/SECTION7.json", "data")
    build = (section7.get("engine_commit"), section7.get("binary_sha256"))

    # 1. Data.
    chain_data = check_data(config, config_sha256, inputs, section7, pins)
    data = {k: chain_data[k]["record"] for k in chains}
    data_sha = {k: chain_data[k]["sha256"] for k in chains}
    checks.append({"check": "data", "pass": True})

    # The 48-hour cap (section 6, reruns).
    body["wall_cap"] = check_wall_cap(config, config_sha256, inputs, section7, read_at)
    checks.append({"check": "wall_cap", "pass": True})

    # 2. Attrition, against the scheduled counts.
    excluded_rows = []
    holdout_per_chain = len(pairs) * len(config["collection"]["holdout_indices"])
    attempted_holdout = len(chains) * holdout_per_chain
    excluded_holdout = 0
    pair_games: dict[int, dict[tuple[int, int], list[str]]] = {}
    for k in chains:
        record = data[k]
        schedule = chain_data[k]["schedule"]
        by_split = {"fit": 0, "holdout": 0}
        for row in record["excluded"]:
            _, split, focal, opposing = schedule[row["id"]]
            by_split[split] += 1
            excluded_rows.append({"chain": k, "id": row["id"], "split": split, "pair": [focal, opposing],
                                  "terminal_classification": row["terminal_classification"],
                                  "terminal_reason": row["terminal_reason"]})
        excluded_holdout += by_split["holdout"]
        require(by_split["holdout"] <= analysis["max_chain_nonnatural_holdout"], "attrition",
                f"chain {k}: {by_split['holdout']} non-natural holdout games")
        require(by_split["fit"] <= analysis["max_chain_nonnatural_fit"], "attrition",
                f"chain {k}: {by_split['fit']} non-natural fit games")
        groups = {pair: [] for pair in pairs}
        for game in record["natural"]["holdout"]:
            _, _, focal, opposing = schedule[game]
            groups[(focal, opposing)].append(game)
        for pair, games in groups.items():
            require(len(games) >= max(1, analysis["min_holdout_games_per_pair"]), "attrition",
                    f"chain {k}: ordered pair {pair} keeps {len(games)} holdout games")
        pair_games[k] = groups
    require(excluded_holdout <= analysis["max_pooled_nonnatural_holdout_fraction"] * attempted_holdout, "attrition",
            f"{excluded_holdout} of {attempted_holdout} holdout games are non-natural")
    body["exclusions"] = excluded_rows
    checks.append({"check": "attrition", "pass": True, "excluded_holdout": excluded_holdout,
                   "attempted_holdout": attempted_holdout})

    # 3. Fits.
    expected = {f"k{k}-s{s}-{arm}.json" for k in chains for s in seeds for arm in ARMS}
    # The fits folder's entries as the inventory recorded them (bound to the fingerprint). A folder inside it
    # (named with a trailing "/") is never a report: it is unexpected.
    present = sorted({name.split("/")[0] + ("/" if "/" in name else "")
                      for name in (label[len("fits/"):] for label in hashes if label.startswith("fits/"))})
    temporaries = [name for name in present if TEMPORARY.match(name)]
    reports = [name for name in present if name not in temporaries]
    # A refusal file, or an interrupted refusal publication, is an entry refusal: VOID.
    refusals = [name for name in reports if name.endswith(".refused.json")] + [
        name for name in temporaries if TEMPORARY.match(name).group(1).endswith(".refused.json")]
    require(not refusals, "fits", {"entry_refusals": refusals})
    require(set(reports) == expected and len(reports) == len(expected), "fits",
            {"missing": sorted(expected - set(reports)), "unexpected": sorted(set(reports) - expected)})
    body["ignored_interrupted_publications"] = temporaries
    lr_bits, vc_bits = f32_bits(fitting["learning_rate"]), f32_bits(fitting["value_coefficient"])
    fits: dict[tuple[int, int, str], dict] = {}
    for k in chains:
        record = data[k]
        retained = record["natural"]["holdout"]
        steps, every = step_rule(fitting, record["fit_groups"])
        require(every >= 1, "fits", f"chain {k}: held-out cadence {every} is below 1")
        points = evaluation_steps(steps, every)
        for s in seeds:
            for arm in ARMS:
                fit, _ = inputs.read(f"fits/k{k}-s{s}-{arm}.json", "fits")
                where = f"chain {k} seed {s} {arm}"
                report = fit.get("report", {})
                cfg = report.get("config", {})
                require(fit.get("schema") == FIT_SCHEMA and fit.get("config_sha256") == config_sha256
                        and fit.get("data_record_sha256") == data_sha[k] and fit.get("chain") == k
                        and fit.get("seed") == s and fit.get("arm") == arm, "fits", f"{where}: not this fit's record")
                require((fit.get("engine_commit"), fit.get("binary_sha256")) == build, "fits",
                        f"{where}: built by another engine build")
                minibatch_seed = domain_seed(fitting["minibatch_seed_domain"], k, s)
                require(cfg.get("value_target_source") == fitting["value_target_source"]
                        == report.get("value_target_source")
                        and cfg.get("steps") == steps and cfg.get("batch_groups") == fitting["batch_groups"]
                        and report.get("learning_rate_bits") == lr_bits
                        and report.get("value_coefficient_bits") == vc_bits
                        and cfg.get("minibatch_seed") == minibatch_seed == fit.get("minibatch_seed")
                        and cfg.get("heldout_every_steps") == every
                        and report.get("initial_adam_step") == 0
                        and report.get("architecture") == ARCHITECTURES[arm]
                        and report.get("input_generation") == config["input_generation"], "fits",
                        f"{where}: config differs from section 4")
                require(report.get("fit_group_count") == record["fit_groups"]
                        and report.get("heldout_group_count") == record["holdout_groups"], "fits",
                        f"{where}: group counts differ from the data record")
                heldout = report.get("heldout", [])
                require([evaluation.get("after_step") for evaluation in heldout] == points
                        and len(report.get("steps", [])) == steps, "fits",
                        f"{where}: evaluations or updates differ from the step rule, or the final one is missing")
                for evaluation in heldout:
                    ids = [row.get("game_id") for row in evaluation.get("games", [])]
                    require(len(ids) == len(set(ids)) and sorted(ids) == sorted(retained), "fits",
                            f"{where}: held-out rows at step {evaluation.get('after_step')} are not the retained "
                            "holdout games, each once")
                transform = fit.get("transform", {})
                parent_state = record["parent_state_sha256"]
                require(report.get("initial_state_sha256") == fit.get("fresh_adam_state_sha256"), "fits",
                        f"{where}: the fit did not start from the recorded fresh-Adam state")
                require(transform.get("construction_version") == CONSTRUCTIONS[arm]
                        and transform.get("parent_state_sha256") == fit.get("parent_state_sha256") == parent_state
                        and transform.get("child_state_sha256") == fit.get("transformed_state_sha256"), "fits",
                        f"{where}: state hash chain broken")
                if arm == "w64":
                    require(fit["transformed_state_sha256"] == parent_state and fit.get("widen_seed") is None,
                            "fits", f"{where}: w64 is not the parent")
                elif arm == "w128":
                    widen_seed = domain_seed(fitting["widen_seed_domain"], k, s)
                    require(fit.get("widen_seed") == widen_seed == transform.get("seed"), "fits",
                            f"{where}: widen seed differs from section 3")
                fits[(k, s, arm)] = fit
    checks.append({"check": "fits", "pass": True, "reports": len(fits)})

    # 4. Pairing.
    for k in chains:
        reference = fits[(k, seeds[0], "w64")]["report"]
        for s in seeds:
            order = {fits[(k, s, arm)]["report"]["minibatch_order_sha256"] for arm in ARMS}
            require(len(order) == 1, "pairing", f"chain {k} seed {s}: minibatch orders differ across arms")
            for arm in ARMS:
                report = fits[(k, s, arm)]["report"]
                for field in ("fit_game_ids", "heldout_game_ids", "outcome_source_sha256",
                              "decision_source_sha256"):
                    require(report[field] == reference[field], "pairing",
                            f"chain {k} seed {s} {arm}: {field} differs within the chain")
            require(bits_view(fits[(k, s, "w64")]["report"]["heldout"][0])
                    == bits_view(reference["heldout"][0]), "pairing",
                    f"chain {k} seed {s}: w64's step-0 report differs across seeds")
        for arm in ("w64", "w32"):
            fresh = {fits[(k, s, arm)]["fresh_adam_state_sha256"] for s in seeds}
            require(len(fresh) == 1, "pairing", f"chain {k}: {arm}'s fresh-Adam state differs across seeds")
        require(reference["fit_game_ids"] == data[k]["natural"]["fit"]
                and reference["heldout_game_ids"] == data[k]["natural"]["holdout"]
                and reference["outcome_source_sha256"] == data[k]["outcome_source_sha256"]
                and reference["decision_source_sha256"] == data[k]["decision_source_sha256"], "pairing",
                f"chain {k}: fit inputs differ from the data record")
    checks.append({"check": "pairing", "pass": True})

    # 5. Function preservation.
    for k in chains:
        preservation, _ = inputs.read(f"preserve/k{k}.json", "function_preservation")
        # The driver's forward comparison consumes every substep of every retained fit and holdout game.
        substeps = chain_data[k]["substeps"]
        require(preservation.get("schema") == PRESERVATION_SCHEMA
                and preservation.get("config_sha256") == config_sha256
                and preservation.get("data_record_sha256") == data_sha[k] and preservation.get("chain") == k
                and (preservation.get("engine_commit"), preservation.get("binary_sha256")) == build
                and preservation.get("pass") is True and isinstance(preservation.get("inputs"), int)
                and preservation["inputs"] == substeps, "function_preservation",
                f"chain {k}: no passed forward comparison over every recorded input")
        by_seed = {row.get("seed"): row for row in preservation.get("seeds", [])}
        require(sorted(by_seed) == sorted(seeds) and len(by_seed) == len(preservation.get("seeds", [])),
                "function_preservation", f"chain {k}: seeds differ")
        for s in seeds:
            row, fit = by_seed[s], fits[(k, s, "w128")]
            require(row.get("mismatches") == 0 and row.get("inputs_compared") == preservation["inputs"]
                    and row.get("widen_seed") == fit["widen_seed"]
                    and row.get("child_state_sha256") == fit["transformed_state_sha256"], "function_preservation",
                    f"chain {k} seed {s}: forward outputs not bit-identical on every input")
            require(bits_view(fit["report"]["heldout"][0]["games"])
                    == bits_view(fits[(k, s, "w64")]["report"]["heldout"][0]["games"]), "function_preservation",
                    f"chain {k} seed {s}: step-0 per-game held-out reports differ")
    checks.append({"check": "function_preservation", "pass": True})

    # Per-game errors and losses (section 5), with their numerical validity.
    errors: dict[tuple[int, int, str], dict[str, float]] = {}
    losses: dict[tuple[int, int, str], float] = {}
    for key, fit in fits.items():
        final = fit["report"]["heldout"][-1]
        groups = chain_data[key[0]]["store_groups"]
        rows = {}
        for row in final["games"]:
            require(row.get("groups") == groups.get(row.get("game_id")), "numerical_validity",
                    f"{key}: {row.get('game_id')} group count differs from the store")
            rows[row["game_id"]] = game_error(row, "numerical_validity")
        errors[key] = rows
        losses[key] = pair_weighted_loss(rows, pair_games[key[0]])
        require(math.isfinite(losses[key]), "numerical_validity", f"{key}: loss {losses[key]!r}")
    pooled = {arm: sum(losses[(k, s, arm)] for k in chains for s in seeds) / (len(chains) * len(seeds))
              for arm in ARMS}
    require(all(math.isfinite(value) for value in pooled.values()) and pooled["w64"] > 0, "numerical_validity",
            {arm: number(value) for arm, value in pooled.items()})
    r_point = 1.0 - pooled["w128"] / pooled["w64"]
    require(math.isfinite(r_point), "numerical_validity", f"R {r_point!r}")
    per_chain = {}
    for k in chains:
        mean = {arm: sum(losses[(k, s, arm)] for s in seeds) / len(seeds) for arm in ("w64", "w128")}
        require(math.isfinite(mean["w64"]) and mean["w64"] > 0, "numerical_validity",
                f"chain {k}: w64 loss {mean['w64']!r}")
        per_chain[k] = 1.0 - mean["w128"] / mean["w64"]
        require(math.isfinite(per_chain[k]), "numerical_validity", f"chain {k}: R {per_chain[k]!r}")
    checks.append({"check": "numerical_validity", "pass": True, "stage": "observed"})

    # 6. Sensitivity control.
    require(pooled["w32"] > pooled["w64"], "sensitivity_control",
            {"L_w32": number(pooled["w32"]), "L_w64": number(pooled["w64"])})
    checks.append({"check": "sensitivity_control", "pass": True})

    # 7. Decision: the bootstrap.
    draws = bootstrap(config, chains, seeds, pairs, pair_games, errors)
    ordered = sorted(draws)
    interval = [ordered[analysis["interval_lower_index"]], ordered[analysis["interval_upper_index"]]]
    require(all(math.isfinite(value) for value in interval), "numerical_validity", "interval endpoint")
    checks.append({"check": "numerical_validity", "pass": True, "stage": "bootstrap", "draws": len(draws)})
    body.update({
        "L": {arm: pooled[arm] for arm in ARMS},
        "R": r_point,
        "interval": interval,
        "bootstrap_draws": len(draws),
        "descriptive": descriptive(config, chains, seeds, data, fits, pair_games, per_chain, pooled, interval),
    })
    return body, inputs.files


def bootstrap(config: dict, chains: list[int], seeds: list[int], pairs: list[tuple[int, int]],
              pair_games: dict[int, dict[tuple[int, int], list[str]]],
              errors: dict[tuple[int, int, str], dict[str, float]]) -> list[float]:
    """The note's section 5 bootstrap: R for each draw, in draw order. Each draw draws its chain occurrences; then
    per occurrence, in order, its seed indices and then its per-pair game draws."""
    analysis = config["analysis"]
    stream = SplitMix64(domain_seed(analysis["bootstrap_seed_domain"], analysis["bootstrap_seed_index"]))
    # Per chain, the retained holdout games in pair order, and each arm's per-game errors in that order.
    order = {k: [game for pair in pairs for game in pair_games[k][pair]] for k in chains}
    vectors = {(k, s, arm): [errors[(k, s, arm)][game] for game in order[k]]
               for k in chains for s in seeds for arm in ("w64", "w128")}
    results = []
    for _ in range(analysis["bootstrap_draws"]):
        occurrences = [chains[stream.index(len(chains))] for _ in chains]
        totals = {"w64": 0.0, "w128": 0.0}
        for k in occurrences:
            drawn_seeds = [seeds[stream.index(len(seeds))] for _ in seeds]
            weights = []
            for pair in pairs:
                games = pair_games[k][pair]
                counts = [0] * len(games)
                for _ in games:
                    counts[stream.index(len(games))] += 1
                scale = 1.0 / (len(games) * len(pairs))
                weights.extend(count * scale for count in counts)
            cache: dict[tuple[int, str], float] = {}
            for s in drawn_seeds:
                for arm in ("w64", "w128"):
                    if (s, arm) not in cache:
                        cache[(s, arm)] = math.fsum(w * e for w, e in zip(weights, vectors[(k, s, arm)]))
                    totals[arm] += cache[(s, arm)]
        count = len(occurrences) * len(seeds)
        w64, w128 = totals["w64"] / count, totals["w128"] / count
        require(math.isfinite(w64) and w64 > 0, "numerical_validity", f"bootstrap draw {len(results)}: L_w64 {w64!r}")
        value = 1.0 - w128 / w64
        require(math.isfinite(value), "numerical_validity", f"bootstrap draw {len(results)}: R {value!r}")
        results.append(value)
    return results


def descriptive(config: dict, chains: list[int], seeds: list[int], data: dict, fits: dict, pair_games: dict,
                per_chain: dict[int, float], pooled: dict[str, float], interval: list[float]) -> dict:
    """Section 6's descriptive items. None of them enters the verdict."""
    analysis = config["analysis"]
    values = [per_chain[k] for k in chains]
    t_interval = None
    if len(values) == 8:
        mean = sum(values) / len(values)
        sd = math.sqrt(sum((value - mean) ** 2 for value in values) / (len(values) - 1))
        half = analysis["t_critical_df7"] * sd / math.sqrt(len(values))
        t_interval = {"mean": mean, "lower": mean - half, "upper": mean + half}
    curves = {}
    curve_minimum = {arm: [] for arm in ARMS}
    for (k, s, arm), fit in fits.items():
        points = []
        for evaluation in fit["report"]["heldout"]:
            rows = {row["game_id"]: row["value_squared_error_sum"] / row["groups"] for row in evaluation["games"]}
            points.append([evaluation["after_step"], pair_weighted_loss(rows, pair_games[k])])
        curves[f"k{k}-s{s}-{arm}"] = points
        curve_minimum[arm].append(min(point[1] for point in points))
    minima = {arm: sum(values) / len(values) for arm, values in curve_minimum.items()}

    def rank(losses: dict[str, float]) -> list[str]:
        return sorted(ARMS, key=lambda arm: losses[arm])

    group_mse = {arm: sum(fits[(k, s, arm)]["report"]["heldout"][-1]["value_mse"] for k in chains for s in seeds)
                 / (len(chains) * len(seeds)) for arm in ARMS}
    policy_nll = {arm: sum(fits[(k, s, arm)]["report"]["heldout"][-1]["policy_nll_mean"] for k in chains
                           for s in seeds) / (len(chains) * len(seeds)) for arm in ARMS}
    fit_loss = {}
    for arm in ARMS:
        total, count = 0.0, 0
        for k in chains:
            epoch = -(-data[k]["fit_groups"] // config["fitting"]["batch_groups"])
            for s in seeds:
                steps = fits[(k, s, arm)]["report"]["steps"][-epoch:]
                total += sum(step["loss"] for step in steps)
                count += len(steps)
        fit_loss[arm] = total / count
    bootstrap_excludes_zero = interval[0] > 0 or interval[1] < 0
    t_excludes_zero = t_interval is not None and (t_interval["lower"] > 0 or t_interval["upper"] < 0)
    return {
        "per_chain_R": {str(k): per_chain[k] for k in chains},
        "per_chain_t_interval_df7": t_interval,
        "bootstrap_and_t_agree_on_excluding_zero": bootstrap_excludes_zero == t_excludes_zero,
        "endpoint_sensitivity": {
            "final_minus_curve_minimum": {arm: pooled[arm] - minima[arm] for arm in ARMS},
            "curve_minimum": minima,
            "ordering_final": rank(pooled),
            "ordering_curve_minima": rank(minima),
            "ordering_differs": rank(pooled) != rank(minima),
            "curve_minimum_rule": "per fit, the minimum pair-weighted held-out loss over its evaluation points; "
                                  "pooled as the mean over chains and seeds",
        },
        "heldout_curves": curves,
        "fit_set_loss_last_epoch": fit_loss,
        "group_weighted_heldout_mse": group_mse,
        "heldout_policy_nll": policy_nll,
    }


def verdict_of(config: dict, body: dict) -> tuple[str, str]:
    """Section 6.7: fund B iff the bootstrap lower bound is above the threshold and R is at least the minimum."""
    analysis = config["analysis"]
    if body["interval"][0] > analysis["fund_lower_bound_above"] and body["R"] >= analysis["fund_min_point_r"]:
        return "FUND_B", "Fund B: advance B to its reviewed study design (scope: the note's top section)."
    return ("NOT_FUNDED", "B is not funded now; this diagnostic gave no positive evidence at this budget. "
            "This does not mean width cannot help continued RL.")


def render(config: dict, config_sha256: str, analyzer_sha256: str, root: Path, section7_sha256: str,
           read_at: float, hashes: dict[str, str], pins: dict[int, list[dict]]) -> dict:
    """The read of the inputs whose SHA-256s are ``hashes`` (the analysis consumes exactly those bytes)."""
    read: dict = {"schema": READ_SCHEMA, "analyzer_sha256": analyzer_sha256, "config_sha256": config_sha256,
                  "section7_sha256": section7_sha256, "root": str(root), "read_at_unix": read_at}
    try:
        body, files = analyze(config, config_sha256, root, read_at, hashes, pins)
    except (Void, KeyError, TypeError, AttributeError, IndexError, ValueError, ZeroDivisionError) as error:
        void = error if isinstance(error, Void) else Void("malformed_input", f"{type(error).__name__}: {error}")
        read.update({"verdict": "VOID", "void_check": void.check, "void_detail": void.detail,
                     "statement": "VOID: a section 6 check failed; this is not a 'not funded' reading."})
        files = {}
    else:
        verdict, statement = verdict_of(config, body)
        read.update({"verdict": verdict, "statement": statement, "R": body["R"], "interval": body["interval"]})
        read.update({key: value for key, value in body.items() if key not in ("R", "interval")})
    read["inputs"] = files
    read["inputs_sha256"] = fingerprint(config_sha256, analyzer_sha256, section7_sha256, hashes)
    return read


def sanitized(value: object) -> object:
    """The read with every non-finite float as a string, so the file stays valid JSON."""
    if isinstance(value, float):
        return number(value)
    if isinstance(value, list):
        return [sanitized(item) for item in value]
    if isinstance(value, dict):
        return {key: sanitized(item) for key, item in value.items()}
    return value


def read_bytes(read: dict) -> bytes:
    return (json.dumps(sanitized(read), indent=1, allow_nan=False) + "\n").encode("utf-8")


def sync_directory(directory: Path) -> None:
    """Makes a directory entry durable where the platform supports syncing a directory (not on Windows)."""
    if os.name != "posix":
        return
    descriptor = os.open(directory, os.O_RDONLY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def check_hard_links(root: Path) -> None:
    """Publication needs hard links on the run root's volume (NTFS and POSIX file systems have them; exFAT and
    some network shares do not). Without them the analyzer refuses: it never falls back to a replacing rename."""
    probe = root / f".READ.link-probe-{os.getpid()}"
    twin = root / f".READ.link-probe-{os.getpid()}.link"
    try:
        with open(probe, "xb"):
            pass
        os.link(probe, twin)
    except OSError as error:
        raise Refused(f"the run root's volume must support hard links to publish the read: {error}") from None
    finally:
        for path in (twin, probe):
            try:
                path.unlink()
            except FileNotFoundError:
                pass


def publish_read(root: Path, canonical: Path, data: bytes) -> None:
    """Writes the full read to a temporary file in the root and syncs it, then publishes it without replacing:
    a hard link to READ.json (refused if it exists, on NTFS as on POSIX) and the temporary's removal. A crash
    leaves either no READ.json or a complete one."""
    check_hard_links(root)
    temporary = root / f".{READ_NAME}.tmp-{os.getpid()}"
    with open(temporary, "xb") as handle:
        handle.write(data)
        handle.flush()
        os.fsync(handle.fileno())
    publish_temporary(temporary, canonical)


def publish_temporary(temporary: Path, canonical: Path) -> None:
    os.link(temporary, canonical)
    os.unlink(temporary)
    sync_directory(canonical.parent)


def unpublished_reads(root: Path) -> tuple[list[tuple[Path, bytes, dict]], list[Path]]:
    """Reads a crashed run left unpublished in the root: (complete, incomplete) temporary files. The root is listed
    without skipping anything (a listing error refuses), and each temporary is read in full first; one that cannot
    be read refuses the run and is kept. A complete one is a whole read in this analyzer's exact output form; only a
    fully read file that is not one is incomplete."""
    complete, incomplete = [], []
    for entry in scan_directory(root):
        if not READ_TEMPORARY.match(entry.name):
            continue
        path = Path(entry.path)
        data = read_input(path)
        try:
            value = json.loads(data)
            whole = isinstance(value, dict) and value.get("schema") == READ_SCHEMA and read_bytes(value) == data
        except ValueError:
            whole = False
        if whole:
            complete.append((path, data, value))
        else:
            incomplete.append(path)
    return complete, incomplete


def without_root(data: bytes) -> bytes:
    """A read's bytes with its ``root`` field removed (every other field, content and hash binding kept)."""
    value = json.loads(data)
    value.pop("root", None)
    return read_bytes(value)


def inside(path: Path, folder: Path) -> bool:
    try:
        path.relative_to(folder)
        return True
    except ValueError:
        return False


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--section7-sha256", required=True, help="SHA-256 of data/SECTION7.json from the note")
    parser.add_argument("--out", type=Path, help="also export the read to this path (outside the record folders)")
    parser.add_argument("--rerender", action="store_true",
                        help="re-render the root's existing read from identical inputs; it must reproduce its bytes "
                             "(takes no lock and writes nothing in the root, so it runs from a read-only copy)")
    arguments = parser.parse_args(argv)
    try:
        config_bytes = arguments.config.read_bytes()
        config = json.loads(config_bytes)
        check_config(config)
        analyzer_sha256 = sha256_bytes(Path(__file__).read_bytes())
        if config["analyzer"]["sha256"] != analyzer_sha256:
            raise Refused("this analyzer's SHA-256 is not the one the config pins")
        config_sha256 = sha256_bytes(config_bytes)
        root = arguments.root.resolve()
        section7 = root / "data" / "SECTION7.json"
        # An early check, so a wrong hash refuses before anything else; the inventory's own hash of SECTION7.json
        # (the bytes the analysis consumes) is checked again below.
        if path_kind(section7) != "file" or sha256_bytes(read_input(section7)) != arguments.section7_sha256:
            raise Refused(SECTION7_REFUSAL)
        canonical = root / READ_NAME
        out = arguments.out.resolve() if arguments.out else None
        if out is not None and any(inside(out, root / folder) for folder in RECORD_FOLDERS):
            raise Refused("--out must not be inside the root's record folders")
        if out == canonical:
            out = None
        if arguments.rerender:
            # A re-render writes nothing in the root: no lock, so it also runs from a read-only copy.
            summary = read_root(config, config_sha256, analyzer_sha256, root, arguments.section7_sha256,
                                canonical, out, True)
        else:
            lock = acquire_lock(root)
            try:
                summary = read_root(config, config_sha256, analyzer_sha256, root, arguments.section7_sha256,
                                    canonical, out, False)
            finally:
                warning = release_lock(lock)
                if warning:
                    print(warning, file=sys.stderr)
    except (Refused, OSError, ValueError, KeyError, TypeError) as error:
        print(f"b width prescreen read refused: {error}", file=sys.stderr)
        return 2
    print(json.dumps(summary))
    return 0


def acquire_lock(root: Path) -> Path:
    """The run root's exclusive analyzer lock, ``<root>/.READ.lock``, held from before recovery, rendering and
    publication until the run exits, so no two runs can both complete a read. If it exists the run refuses and
    leaves it. If this run created it but cannot initialize it, it removes that lock (never one that was already
    there). Removing a lock left by a crashed run is an operator step: first confirm that no analyzer process is
    running on this root."""
    lock = root / LOCK_NAME
    try:
        handle = open(lock, "xb")
    except FileExistsError:
        raise Refused(f"{lock} exists: another analyzer holds this root, or one exited without releasing it. "
                      "Remove it only after confirming that no analyzer process is running on this root.") from None
    try:
        with handle:
            handle.write(json.dumps({"pid": os.getpid(), "started_at_unix": time.time()}).encode("utf-8"))
            handle.flush()
            os.fsync(handle.fileno())
    except BaseException as error:
        warning = release_lock(lock)
        if isinstance(error, OSError):
            raise Refused(f"{lock} could not be initialized: {error}" + (f"; {warning}" if warning else "")) from None
        raise
    return lock


def release_lock(lock: Path) -> str | None:
    """Removes this run's lock. Returns a warning naming the lock if it could not be removed."""
    try:
        os.unlink(lock)
    except OSError as error:
        return (f"warning: {lock} could not be removed ({error}). This run has ended and holds it no longer: "
                "remove it before the next run on this root.")
    return None


def verify_unchanged(config: dict, root: Path, hashes: dict[str, str], pins: dict[int, list[dict]]) -> None:
    """Inventories and re-hashes every bound input; any change since the first inventory refuses (nothing is
    written), and so does a required input that has since disappeared."""
    inventory = Inventory(config, root)
    if inventory.hashes() != hashes or inventory.pins != pins:
        raise Refused("an input changed while the read was rendered; nothing was written")


def export(out: Path | None, data: bytes) -> None:
    """Writes the read's ``--out`` copy (a copy that exists must hold the same bytes)."""
    if out is None:
        return
    if path_kind(out) is not None:
        if read_input(out) != data:
            raise Refused(f"{out} holds a different read")
        return
    out.parent.mkdir(parents=True, exist_ok=True)
    with open(out, "xb") as handle:
        handle.write(data)


def exported(summary: dict, out: Path | None, data: bytes) -> dict:
    """The summary of a published read, after its ``--out`` copy. The read is published either way, so a copy that
    cannot be written is a warning (a later ``--rerender --out`` writes it), not a refusal."""
    try:
        export(out, data)
    except (Refused, OSError) as error:
        summary["export_warning"] = f"the read is published, but its --out copy was not written: {error}"
        print(f"warning: {summary['export_warning']}", file=sys.stderr)
    return summary


def summary_of(read: dict, canonical: Path) -> dict:
    return {"verdict": read["verdict"], "R": read.get("R"), "interval": read.get("interval"), "read": str(canonical)}


def read_root(config: dict, config_sha256: str, analyzer_sha256: str, root: Path, section7_sha256: str,
              canonical: Path, out: Path | None, rerender: bool) -> dict:
    """The read (under the root's lock unless it is a re-render). Returns the printed summary."""
    if rerender and path_kind(canonical) is None:
        raise Refused("--rerender needs the root's existing read")
    if not rerender:
        if path_kind(canonical) is not None:
            raise Refused(f"{canonical} exists: this run root has been read (use --rerender only to re-render it)")
        if out is not None and path_kind(out) is not None:
            raise Refused(f"{out} exists")
    # Every bound input is read in full and hashed before the analysis; the analysis consumes exactly those bytes.
    inventory = Inventory(config, root)
    hashes, pins = inventory.hashes(), inventory.pins
    if hashes["data/SECTION7.json"] != section7_sha256:
        raise Refused(SECTION7_REFUSAL)
    current = fingerprint(config_sha256, analyzer_sha256, section7_sha256, hashes)
    if rerender:
        # A re-render may run from a moved copy of the root (an archive): every field but the root itself must be
        # reproduced.
        previous_bytes = read_input(canonical)
        previous = json.loads(previous_bytes)
        if previous.get("inputs_sha256") != current:
            raise Refused("the inputs differ from the existing read's; a re-render needs identical inputs")
        read = render(config, config_sha256, analyzer_sha256, root, section7_sha256, previous["read_at_unix"],
                      hashes, pins)
        data = read_bytes(read)
        verify_unchanged(config, root, hashes, pins)
        if without_root(previous_bytes) != without_root(data):
            raise Refused("the re-rendered read differs from the existing one; nothing was written")
        export(out, data)
        return summary_of(read, canonical)
    # A run that crashed after writing its read but before publishing it already rendered the read. It is
    # re-rendered at its recorded read time and published only if the inputs and the bytes are the same, so there
    # are never two completed reads to choose between. A fully read temporary that is not a whole read is removed;
    # one that cannot be read refuses the run and is kept.
    complete, incomplete = unpublished_reads(root)
    if len(complete) > 1:
        raise Refused(f"more than one unpublished read in {root}: {[entry[0].name for entry in complete]}")
    if complete:
        temporary, previous_bytes, previous = complete[0]
        if previous.get("inputs_sha256") != current or previous.get("config_sha256") != config_sha256:
            raise Refused(f"{temporary.name} is an unpublished read of other inputs")
        rendered = read_bytes(render(config, config_sha256, analyzer_sha256, root, section7_sha256,
                                     previous["read_at_unix"], hashes, pins))
        if without_root(rendered) != without_root(previous_bytes):
            raise Refused(f"{temporary.name} does not re-render from its inputs; nothing was published")
        verify_unchanged(config, root, hashes, pins)
        check_hard_links(root)
        publish_temporary(temporary, canonical)
        summary = summary_of(previous, canonical)
        summary["published_interrupted_read"] = temporary.name
        return exported(summary, out, previous_bytes)
    for path in incomplete:
        path.unlink()
    read = render(config, config_sha256, analyzer_sha256, root, section7_sha256, int(current_time()), hashes, pins)
    data = read_bytes(read)
    verify_unchanged(config, root, hashes, pins)
    publish_read(root, canonical, data)
    return exported(summary_of(read, canonical), out, data)


if __name__ == "__main__":
    sys.exit(main())
