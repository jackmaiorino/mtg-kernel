"""Focused independent tests for the native full-episode trajectory V2 goldens.

Frozen contract: collab TO-CLAUDE.md Phase A with its adversarial-review,
authority-audit, and precedence/closure correction addenda.

Framing is reimplemented locally throughout. The primary byte-count and hash
assertions use the locally rebuilt streams, and the generator's own builder is
only compared against them afterwards, so a framing bug cannot agree with
itself.
"""

from __future__ import annotations

import sys

sys.dont_write_bytecode = True

import copy  # noqa: E402
import hashlib  # noqa: E402
import importlib.util  # noqa: E402
import json  # noqa: E402
import subprocess  # noqa: E402
import unittest  # noqa: E402
from pathlib import Path  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[2]
TOOLS = REPO_ROOT / "python" / "tools"
DATA = REPO_ROOT / "data"
GENERATOR_PATH = TOOLS / "generate_native_full_episode_trajectory_v2_goldens.py"
ARTIFACT_PATH = DATA / "native_full_episode_trajectory_v2_goldens.json"
V1_ARTIFACT_PATH = DATA / "native_full_episode_trajectory_v1_goldens.json"

# Re-baselined once per the owner ruling on record (collab CLAUDE #236,
# 2026-08-14; landed on this branch as commit 40a0fad, "Re-baseline
# native_full_episode_trajectory_v2's catalog-identity pins"): the
# runtime-decks-nine catalog landing is one of the three accepted
# determinism-epoch causes, so the golden generator's own
# RUNTIME_DECK_CATALOG_FILE_SHA256 moved and the golden was regenerated
# (case counts and byte lengths unchanged; only the hash cascade moved).
# The Rust side already carries these exact values
# (NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_FILE_SHA256_V2 /
# ..._GOLDEN_STREAM_SHA256_V2 in native_full_episode_trajectory_v2.rs and
# native_full_episode_trajectory_v2_goldens.rs); only this Python-side pin
# had not been updated to match. Byte counts are unaffected (same
# ARTIFACT_BYTES/STREAM_BYTES), and .gitattributes pins `*.json text
# eol=lf`, so both hashes below are confirmed identical on ubuntu-24.04
# and windows-2025 CI (no platform/EOL divergence to reconcile).
ARTIFACT_BYTES = 251658
ARTIFACT_SHA256 = "1a79f8153fe8adb7d984609d5e510f8e9f2f9e37358ed42744873ae4fd743672"
STREAM_BYTES = 226765
STREAM_SHA256 = "554003d75532a26d50ff6599cd909223176926773dce21efcd9895e41e51cb8e"

V1_ARTIFACT_SHA256 = "502a1b4ba296fdc4b2f4e8fd61cc5b4d64f152c9b84b4e11a85967f76c3bde8b"
V1_STREAM_SHA256 = "f5230cbbc0b87735e7aa14c89ce31e41ce769de3f4292cafe63dad4733168d7a"
V1_STREAM_IDENTITY = (
    "mtg-kernel-native-full-episode-trajectory-golden-vector-stream-sha256-v1"
)

NATIVE_ROOT = 5_293_664_275_683_392_565
U64_MAX = (1 << 64) - 1
MAX_ARTIFACT_BYTES = 4 * 1024 * 1024

# Live dependency files hashed against the pinned source-authority metadata.
DEPENDENCY_PINS = (
    (V1_ARTIFACT_PATH, ("inner_trajectory", "goldens_raw_file_sha256")),
    (
        TOOLS / "environment_randomization_v2_reference.py",
        ("environment_randomization", "python_reference_raw_file_sha256"),
    ),
    (
        DATA / "environment_randomization_v2" / "goldens_v1.json",
        ("environment_randomization", "kdf_goldens_raw_file_sha256"),
    ),
    (
        DATA / "environment_randomization_v2" / "reset_physical_trajectory_goldens_v1.json",
        ("reset_trajectory", "goldens_raw_file_sha256"),
    ),
    (
        DATA / "native_trainer_schedule_v1_goldens.json",
        ("trainer_schedule", "goldens_raw_file_sha256"),
    ),
    (
        DATA / "runtime_decks_v1.json",
        ("runtime_deck_catalog", "catalog_raw_file_sha256"),
    ),
)

ENVELOPE_TAGS = (
    "domain",
    "inner_trajectory_identity_utf8",
    "inner_trajectory_goldens_schema_utf8",
    "inner_trajectory_goldens_generator_identity_utf8",
    "inner_trajectory_golden_stream_identity_utf8",
    "inner_trajectory_goldens_file_sha256_raw32",
    "inner_trajectory_golden_stream_sha256_raw32",
    "environment_randomization_identity_utf8",
    "environment_randomization_namespace_utf8",
    "environment_randomization_kdf_goldens_schema_utf8",
    "environment_randomization_kdf_goldens_file_sha256_raw32",
    "reset_trajectory_goldens_schema_utf8",
    "reset_trajectory_generator_identity_utf8",
    "reset_trajectory_physical_projection_identity_utf8",
    "reset_trajectory_vector_stream_identity_utf8",
    "reset_trajectory_goldens_file_sha256_raw32",
    "reset_trajectory_vector_stream_sha256_raw32",
    "trainer_schedule_identity_utf8",
    "trainer_seed_version_utf8",
    "trainer_schedule_goldens_file_sha256_raw32",
    "runtime_deck_catalog_schema_utf8",
    "runtime_deck_protocol_utf8",
    "runtime_deck_materialization_protocol_utf8",
    "runtime_deck_hash_algorithm_utf8",
    "runtime_deck_catalog_file_sha256_raw32",
    "episode_index_u64be",
    "pair_index_u64be",
    "pair_environment_seed_u64be",
    "deck_p0_id_utf8",
    "deck_p0_hash_u64be",
    "deck_p1_id_utf8",
    "deck_p1_hash_u64be",
    "learner_seat_u8",
    "inner_trajectory_sha256_raw32",
)


def _import(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


GEN = _import("v2_goldens_generator_under_test", GENERATOR_PATH)
ContractRejection = GEN.ContractRejection
ShapeError = GEN.ShapeError


# ------------------------------------------------- local independent framing


def local_atom(tag: str, payload: bytes) -> bytes:
    encoded = tag.encode("utf-8")
    return (
        len(encoded).to_bytes(4, "big")
        + encoded
        + len(payload).to_bytes(8, "big")
        + payload
    )


def local_cj_no_lf(value) -> bytes:
    return json.dumps(
        value, sort_keys=True, separators=(",", ":"), ensure_ascii=True, allow_nan=False
    ).encode("ascii")


def local_cj_with_lf(value) -> bytes:
    return local_cj_no_lf(value) + b"\n"


def parse_atoms(stream: bytes) -> list[tuple[str, bytes]]:
    atoms: list[tuple[str, bytes]] = []
    offset = 0
    while offset < len(stream):
        tag_len = int.from_bytes(stream[offset : offset + 4], "big")
        offset += 4
        tag = stream[offset : offset + tag_len].decode("utf-8")
        offset += tag_len
        payload_len = int.from_bytes(stream[offset : offset + 8], "big")
        offset += 8
        payload = stream[offset : offset + payload_len]
        if len(payload) != payload_len:
            raise AssertionError("truncated atom payload")
        offset += payload_len
        atoms.append((tag, payload))
    if offset != len(stream):
        raise AssertionError("trailing bytes in atom stream")
    return atoms


def local_v2_semantic_stream(artifact: dict) -> bytes:
    """Full independent rebuild in the exact frozen 13-step order."""
    parts = [
        local_atom("domain", artifact["vector_stream_identity"].encode("ascii")),
        local_atom("schema_utf8", artifact["schema"].encode("utf-8")),
        local_atom("generator_identity_utf8", artifact["generator_identity"].encode("utf-8")),
        local_atom("trajectory_identity_utf8", artifact["trajectory_identity"].encode("utf-8")),
        local_atom(
            "source_authorities_canonical_json_utf8",
            local_cj_no_lf(artifact["source_authorities"]),
        ),
        local_atom(
            "positive_case_count_u32be", len(artifact["positive_cases"]).to_bytes(4, "big")
        ),
    ]
    for case in artifact["positive_cases"]:
        parts.append(local_atom("positive_case_name_ascii", case["name"].encode("ascii")))
        parts.append(
            local_atom(
                "positive_case_input_canonical_json_utf8", local_cj_no_lf(case["input"])
            )
        )
        parts.append(
            local_atom("positive_case_inner_stream_raw", bytes.fromhex(case["inner_stream_hex"]))
        )
        parts.append(
            local_atom("positive_case_inner_sha256_raw32", bytes.fromhex(case["inner_sha256"]))
        )
        parts.append(
            local_atom("positive_case_v2_stream_raw", bytes.fromhex(case["v2_stream_hex"]))
        )
        parts.append(
            local_atom("positive_case_v2_sha256_raw32", bytes.fromhex(case["v2_sha256"]))
        )
    parts.append(
        local_atom(
            "pair_positive_case_count_u32be",
            len(artifact["pair_positive_cases"]).to_bytes(4, "big"),
        )
    )
    for case in artifact["pair_positive_cases"]:
        parts.append(local_atom("pair_positive_case_name_ascii", case["name"].encode("ascii")))
        parts.append(
            local_atom(
                "pair_positive_case_input_canonical_json_utf8", local_cj_no_lf(case["input"])
            )
        )
        parts.append(
            local_atom(
                "pair_positive_even_trajectory_sha256_raw32",
                bytes.fromhex(case["even_trajectory_sha256"]),
            )
        )
        parts.append(
            local_atom(
                "pair_positive_odd_trajectory_sha256_raw32",
                bytes.fromhex(case["odd_trajectory_sha256"]),
            )
        )
    parts.append(
        local_atom(
            "trajectory_reject_case_count_u32be",
            len(artifact["trajectory_reject_cases"]).to_bytes(4, "big"),
        )
    )
    for case in artifact["trajectory_reject_cases"]:
        parts.append(
            local_atom("trajectory_reject_case_name_ascii", case["name"].encode("ascii"))
        )
        parts.append(
            local_atom(
                "trajectory_reject_case_input_canonical_json_utf8",
                local_cj_no_lf(case["input"]),
            )
        )
        parts.append(
            local_atom(
                "trajectory_reject_expected_code_ascii", case["expected_code"].encode("ascii")
            )
        )
    parts.append(
        local_atom(
            "pair_reject_case_count_u32be",
            len(artifact["pair_reject_cases"]).to_bytes(4, "big"),
        )
    )
    for case in artifact["pair_reject_cases"]:
        parts.append(local_atom("pair_reject_case_name_ascii", case["name"].encode("ascii")))
        parts.append(
            local_atom(
                "pair_reject_case_input_canonical_json_utf8", local_cj_no_lf(case["input"])
            )
        )
        parts.append(
            local_atom("pair_reject_expected_code_ascii", case["expected_code"].encode("ascii"))
        )
    return b"".join(parts)


def local_v1_semantic_stream(v1_artifact: dict) -> bytes:
    """Independent reconstruction of the frozen V1 golden vector stream. The V1
    input atom carries one final LF, unlike the V2 authority atom."""
    parts = [
        local_atom("domain", V1_STREAM_IDENTITY.encode("ascii")),
        local_atom(
            "positive_case_count_u64be",
            len(v1_artifact["positive_cases"]).to_bytes(8, "big"),
        ),
    ]
    for case in v1_artifact["positive_cases"]:
        parts.append(
            local_atom(
                "positive_case",
                local_atom("name_utf8", case["name"].encode("ascii"))
                + local_atom("input_canonical_json", local_cj_with_lf(case["input"]))
                + local_atom("stream_bytes", bytes.fromhex(case["stream_hex"]))
                + local_atom("expected_sha256", bytes.fromhex(case["expected_sha256"])),
            )
        )
    parts.append(
        local_atom(
            "reject_case_count_u64be", len(v1_artifact["reject_cases"]).to_bytes(8, "big")
        )
    )
    for case in v1_artifact["reject_cases"]:
        parts.append(
            local_atom(
                "reject_case",
                local_atom("name_utf8", case["name"].encode("ascii"))
                + local_atom("input_canonical_json", local_cj_with_lf(case["input"]))
                + local_atom(
                    "expected_rejection_ascii", case["expected_rejection"].encode("ascii")
                ),
            )
        )
    return b"".join(parts)


def load_artifact() -> dict:
    return GEN.strict_json_loads(ARTIFACT_PATH.read_bytes(), "artifact")


class CanonicalBytesTest(unittest.TestCase):
    def test_size_and_sha256_are_pinned(self) -> None:
        raw = ARTIFACT_PATH.read_bytes()
        self.assertEqual(len(raw), ARTIFACT_BYTES)
        self.assertEqual(hashlib.sha256(raw).hexdigest(), ARTIFACT_SHA256)

    def test_canonical_byte_shape(self) -> None:
        raw = ARTIFACT_PATH.read_bytes()
        self.assertTrue(raw.isascii())
        self.assertFalse(raw.startswith(b"\xef\xbb\xbf"))
        self.assertTrue(raw.endswith(b"\n"))
        self.assertEqual(raw.count(b"\n"), 1)
        body = raw[:-1]
        self.assertNotIn(b"\n", body)
        self.assertFalse(any(byte < 0x20 or byte == 0x7F for byte in body))
        self.assertLessEqual(len(raw), MAX_ARTIFACT_BYTES)

    def test_exact_canonical_regeneration(self) -> None:
        self.assertEqual(
            GEN.canonical_file_bytes(GEN.build_artifact()), ARTIFACT_PATH.read_bytes()
        )

    def test_real_cli_check_path_via_subprocess(self) -> None:
        result = subprocess.run(
            [sys.executable, "-B", str(GENERATOR_PATH), "--out", str(ARTIFACT_PATH), "--check"],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("CHECK OK", result.stdout)
        self.assertIn(ARTIFACT_SHA256, result.stdout)
        self.assertIn(STREAM_SHA256, result.stdout)

    def test_cli_check_fails_on_a_mutated_artifact(self) -> None:
        import tempfile

        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "mutated.json"
            target.write_bytes(ARTIFACT_PATH.read_bytes().replace(b"draw", b"drav", 1))
            result = subprocess.run(
                [sys.executable, "-B", str(GENERATOR_PATH), "--out", str(target), "--check"],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertNotEqual(result.returncode, 0)


class SemanticStreamTest(unittest.TestCase):
    def setUp(self) -> None:
        self.artifact = load_artifact()

    def test_locally_rebuilt_stream_pins(self) -> None:
        local = local_v2_semantic_stream(self.artifact)
        self.assertEqual(len(local), STREAM_BYTES)
        self.assertEqual(hashlib.sha256(local).hexdigest(), STREAM_SHA256)

    def test_generator_stream_matches_the_local_rebuild(self) -> None:
        self.assertEqual(GEN.semantic_stream(self.artifact), local_v2_semantic_stream(self.artifact))

    def test_stream_tag_order_and_no_lf_authority_atom(self) -> None:
        atoms = parse_atoms(local_v2_semantic_stream(self.artifact))
        tags = [tag for tag, _ in atoms]
        self.assertEqual(
            tags[:6],
            [
                "domain",
                "schema_utf8",
                "generator_identity_utf8",
                "trajectory_identity_utf8",
                "source_authorities_canonical_json_utf8",
                "positive_case_count_u32be",
            ],
        )
        authority = atoms[4][1]
        self.assertFalse(authority.endswith(b"\n"))
        self.assertEqual(authority, local_cj_no_lf(self.artifact["source_authorities"]))
        counts = {
            "positive_case_count_u32be": len(self.artifact["positive_cases"]),
            "pair_positive_case_count_u32be": len(self.artifact["pair_positive_cases"]),
            "trajectory_reject_case_count_u32be": len(self.artifact["trajectory_reject_cases"]),
            "pair_reject_case_count_u32be": len(self.artifact["pair_reject_cases"]),
        }
        for tag, expected in counts.items():
            payload = [p for t, p in atoms if t == tag]
            self.assertEqual(len(payload), 1, tag)
            self.assertEqual(len(payload[0]), 4, tag)
            self.assertEqual(int.from_bytes(payload[0], "big"), expected, tag)

    def test_independent_v1_stream_reconstruction_matches_the_frozen_pin(self) -> None:
        raw = V1_ARTIFACT_PATH.read_bytes()
        self.assertEqual(hashlib.sha256(raw).hexdigest(), V1_ARTIFACT_SHA256)
        v1 = json.loads(raw.decode("ascii"))
        rebuilt = local_v1_semantic_stream(v1)
        self.assertEqual(hashlib.sha256(rebuilt).hexdigest(), V1_STREAM_SHA256)


class EnvelopeTest(unittest.TestCase):
    def setUp(self) -> None:
        self.artifact = load_artifact()
        self.cases = {c["name"]: c for c in self.artifact["positive_cases"]}

    def test_exact_thirty_four_tag_order_without_collapsing(self) -> None:
        self.assertEqual(len(ENVELOPE_TAGS), 34)
        for case in self.artifact["positive_cases"]:
            atoms = parse_atoms(bytes.fromhex(case["v2_stream_hex"]))
            self.assertEqual(len(atoms), 34, case["name"])
            self.assertEqual(tuple(tag for tag, _ in atoms), ENVELOPE_TAGS, case["name"])

    def test_envelope_wraps_the_owned_inner_digest(self) -> None:
        for case in self.artifact["positive_cases"]:
            inner, envelope = GEN.evaluate_trajectory(case["input"])
            self.assertEqual(inner.hex(), case["inner_stream_hex"], case["name"])
            self.assertEqual(envelope.hex(), case["v2_stream_hex"], case["name"])
            atoms = parse_atoms(envelope)
            self.assertEqual(atoms[-1][0], "inner_trajectory_sha256_raw32")
            self.assertEqual(atoms[-1][1], hashlib.sha256(inner).digest(), case["name"])

    def test_envelope_has_no_self_or_artifact_hash(self) -> None:
        for case in self.artifact["positive_cases"]:
            envelope = bytes.fromhex(case["v2_stream_hex"])
            self.assertNotIn(bytes.fromhex(case["v2_sha256"]), envelope)
            self.assertNotIn(bytes.fromhex(ARTIFACT_SHA256), envelope)
            self.assertNotIn(bytes.fromhex(STREAM_SHA256), envelope)

    def test_dependency_closure_pins_absent_from_envelope(self) -> None:
        for case in self.artifact["positive_cases"]:
            envelope = bytes.fromhex(case["v2_stream_hex"])
            self.assertNotIn(
                bytes.fromhex(
                    "9dd7e5357d98ff5a7ac302d285da91fb56cf0d422c5aef6bc9b53f2a5d822024"
                ),
                envelope,
            )
            self.assertNotIn(b"mtg_kernel_native_trainer_schedule_goldens/v1", envelope)

    def test_pair_index_parity_and_full_width_root(self) -> None:
        for case in self.artifact["positive_cases"]:
            atoms = dict(parse_atoms(bytes.fromhex(case["v2_stream_hex"])))
            episode = int.from_bytes(atoms["episode_index_u64be"], "big")
            self.assertEqual(int.from_bytes(atoms["pair_index_u64be"], "big"), episode // 2)
            self.assertEqual(atoms["learner_seat_u8"], bytes((episode % 2,)))
        wide = self.cases["episode-3-root-u64-max-learner-p1-p0-win"]
        atoms = dict(parse_atoms(bytes.fromhex(wide["v2_stream_hex"])))
        self.assertEqual(atoms["pair_environment_seed_u64be"], b"\xff" * 8)
        self.assertEqual(int.from_bytes(atoms["pair_environment_seed_u64be"], "big"), U64_MAX)

    def test_root_red_pair_differs_only_by_root(self) -> None:
        left = copy.deepcopy(self.cases["episode-2-root-940000-draw-red-mate"])
        right = self.cases["episode-2-root-940001-draw"]
        self.assertNotEqual(
            left["input"]["pair_environment_seed_u64_hex"],
            right["input"]["pair_environment_seed_u64_hex"],
        )
        left["input"]["pair_environment_seed_u64_hex"] = right["input"][
            "pair_environment_seed_u64_hex"
        ]
        self.assertEqual(left["input"], right["input"])
        self.assertNotEqual(left["inner_sha256"], right["inner_sha256"])
        self.assertNotEqual(left["v2_sha256"], right["v2_sha256"])


class PositiveCoverageTest(unittest.TestCase):
    def setUp(self) -> None:
        self.artifact = load_artifact()

    def test_both_actor_roles_present(self) -> None:
        roles = set()
        for case in self.artifact["positive_cases"]:
            for row in case["input"]["decisions"]:
                roles.add(row["actor_role"])
        self.assertEqual(roles, {"learner", "opponent"})

    def test_all_three_natural_outcomes_present(self) -> None:
        outcomes = {c["input"]["terminal"]["outcome"] for c in self.artifact["positive_cases"]}
        self.assertEqual(outcomes, {"p0-win", "p1-win", "draw"})
        for case in self.artifact["positive_cases"]:
            terminal = case["input"]["terminal"]
            self.assertEqual(terminal["classification"], "natural")
            self.assertEqual(terminal["terminal_code"], "natural-game-over")

    def test_multi_substep_physical_group_present(self) -> None:
        widths = set()
        for case in self.artifact["positive_cases"]:
            for row in case["input"]["decisions"]:
                widths.add(row["substep_count_u32"])
        self.assertTrue({w for w in widths if w >= 2}, "no multi-substep group present")
        self.assertIn(3, widths)

    def test_both_learner_seats_present(self) -> None:
        seats = {c["input"]["learner_seat"] for c in self.artifact["positive_cases"]}
        self.assertEqual(seats, {"p0", "p1"})


class StrictLoaderTest(unittest.TestCase):
    def test_duplicate_keys_rejected_at_every_depth(self) -> None:
        with self.assertRaises(ShapeError):
            GEN.strict_json_loads(b'{"a":1,"a":2}', "probe")
        with self.assertRaises(ShapeError):
            GEN.strict_json_loads(b'{"o":{"a":1,"a":2}}', "probe")

    def test_float_and_non_finite_rejected(self) -> None:
        for payload in (b'{"a":1.0}', b'{"a":1e999}', b'{"a":NaN}', b'{"a":Infinity}', b'{"a":-Infinity}'):
            with self.assertRaises(ShapeError, msg=repr(payload)):
                GEN.strict_json_loads(payload, "probe")

    def test_boolean_in_integer_position_rejected(self) -> None:
        for value in (True, False):
            with self.assertRaises(ShapeError):
                GEN.require_bounded_int(value, 0, 10, "probe")
            with self.assertRaises(ShapeError):
                GEN.require_u32(value, "probe")
        self.assertEqual(GEN.require_bounded_int(5, 0, 10, "probe"), 5)

    def test_carriage_return_and_non_ascii_rejected(self) -> None:
        with self.assertRaises(ShapeError):
            GEN.strict_json_loads(b'{"a":1}\r', "probe")
        with self.assertRaises(ShapeError):
            GEN.strict_json_loads(b'{"a":"\xc3\xbc"}', "probe")

    def test_hex_width_and_case_rejections(self) -> None:
        with self.assertRaises(ShapeError):
            GEN.require_lower_hex("AB" * 32, 64, "probe")
        with self.assertRaises(ShapeError):
            GEN.require_lower_hex("ab" * 31, 64, "probe")
        with self.assertRaises(ShapeError):
            GEN.require_lower_hex("ab" * 33, 64, "probe")
        with self.assertRaises(ShapeError):
            GEN.require_lower_hex(None, 64, "probe")
        GEN.require_lower_hex("ab" * 32, 64, "probe")
        with self.assertRaises(ShapeError):
            GEN.parse_fixed_hex("FFFFFFFFFFFFFFFF", 16, "probe")


class ClosedArtifactValidationTest(unittest.TestCase):
    def setUp(self) -> None:
        self.artifact = load_artifact()

    def test_stored_artifact_validates(self) -> None:
        GEN.audit_artifact(copy.deepcopy(self.artifact))

    def test_top_level_unknown_and_missing_fields_rejected(self) -> None:
        extra = copy.deepcopy(self.artifact)
        extra["unexpected_top_level"] = 1
        with self.assertRaises(ShapeError):
            GEN.audit_artifact(extra)
        for key in sorted(GEN.ARTIFACT_FIELDS):
            missing = copy.deepcopy(self.artifact)
            del missing[key]
            with self.assertRaises(ShapeError, msg=key):
                GEN.audit_artifact(missing)

    def test_frozen_identity_drift_rejected(self) -> None:
        for key in ("schema", "generator_identity", "trajectory_identity", "vector_stream_identity"):
            drifted = copy.deepcopy(self.artifact)
            drifted[key] = drifted[key] + "-drift"
            with self.assertRaises(ShapeError, msg=key):
                GEN.audit_artifact(drifted)

    def test_nested_case_unknown_and_missing_fields_rejected(self) -> None:
        for key, fields in (
            ("positive_cases", GEN.POSITIVE_CASE_FIELDS),
            ("pair_positive_cases", GEN.PAIR_POSITIVE_CASE_FIELDS),
            ("trajectory_reject_cases", GEN.REJECT_CASE_FIELDS),
            ("pair_reject_cases", GEN.REJECT_CASE_FIELDS),
        ):
            extra = copy.deepcopy(self.artifact)
            extra[key][0]["unexpected_nested"] = 1
            with self.assertRaises(ShapeError, msg=f"{key} unknown"):
                GEN.audit_artifact(extra)
            for field in sorted(fields - {"name"}):
                missing = copy.deepcopy(self.artifact)
                del missing[key][0][field]
                with self.assertRaises(ShapeError, msg=f"{key}/{field}"):
                    GEN.audit_artifact(missing)

    def test_nested_input_unknown_field_rejected(self) -> None:
        drifted = copy.deepcopy(self.artifact)
        drifted["positive_cases"][0]["input"]["unexpected_input_field"] = 1
        with self.assertRaises(ShapeError):
            GEN.audit_artifact(drifted)

    def test_stored_stream_or_digest_drift_rejected(self) -> None:
        for field in ("inner_stream_hex", "v2_stream_hex", "inner_sha256", "v2_sha256"):
            drifted = copy.deepcopy(self.artifact)
            value = drifted["positive_cases"][0][field]
            flipped = ("1" if value[0] != "1" else "2") + value[1:]
            drifted["positive_cases"][0][field] = flipped
            with self.assertRaises(ShapeError, msg=field):
                GEN.audit_artifact(drifted)

    def test_pair_digest_drift_rejected(self) -> None:
        for field in ("even_trajectory_sha256", "odd_trajectory_sha256"):
            drifted = copy.deepcopy(self.artifact)
            value = drifted["pair_positive_cases"][0][field]
            drifted["pair_positive_cases"][0][field] = ("1" if value[0] != "1" else "2") + value[1:]
            with self.assertRaises(ShapeError, msg=field):
                GEN.audit_artifact(drifted)

    def test_reject_record_code_drift_rejected(self) -> None:
        for key in ("trajectory_reject_cases", "pair_reject_cases"):
            drifted = copy.deepcopy(self.artifact)
            drifted[key][0]["expected_code"] = "authority-mismatch"
            with self.assertRaises(ShapeError, msg=key):
                GEN.audit_artifact(drifted)
            outside = copy.deepcopy(self.artifact)
            outside[key][0]["expected_code"] = "not-a-real-code"
            with self.assertRaises(ShapeError, msg=key):
                GEN.audit_artifact(outside)

    def test_name_grammar_order_and_cap(self) -> None:
        bad = copy.deepcopy(self.artifact)
        bad["positive_cases"][0]["name"] = "Bad-Name"
        with self.assertRaises(ShapeError):
            GEN.audit_artifact(bad)
        unsorted = copy.deepcopy(self.artifact)
        unsorted["positive_cases"] = list(reversed(unsorted["positive_cases"]))
        with self.assertRaises(ShapeError):
            GEN.audit_artifact(unsorted)
        empty = copy.deepcopy(self.artifact)
        empty["pair_reject_cases"] = []
        with self.assertRaises(ShapeError):
            GEN.audit_artifact(empty)
        overlong = copy.deepcopy(self.artifact)
        overlong["positive_cases"][0]["name"] = "a" * 129
        with self.assertRaises(ShapeError):
            GEN.audit_artifact(overlong)


class AuthorityTest(unittest.TestCase):
    def setUp(self) -> None:
        self.artifact = load_artifact()
        self.authorities = self.artifact["source_authorities"]

    def test_covers_all_five_dependencies(self) -> None:
        self.assertEqual(
            sorted(self.authorities),
            [
                "environment_randomization",
                "inner_trajectory",
                "reset_trajectory",
                "runtime_deck_catalog",
                "trainer_schedule",
            ],
        )

    def test_both_closure_pins_present(self) -> None:
        self.assertEqual(
            self.authorities["environment_randomization"]["python_reference_raw_file_sha256"],
            "9dd7e5357d98ff5a7ac302d285da91fb56cf0d422c5aef6bc9b53f2a5d822024",
        )
        self.assertEqual(
            self.authorities["trainer_schedule"]["goldens_schema"],
            "mtg_kernel_native_trainer_schedule_goldens/v1",
        )

    def test_hash_field_names_are_unambiguous(self) -> None:
        for block in self.authorities.values():
            for key in block:
                if key.endswith("_sha256"):
                    self.assertTrue(
                        "raw_file_sha256" in key or "semantic_stream_sha256" in key,
                        f"ambiguous hash field {key!r}",
                    )

    def test_excludes_new_v2_hashes(self) -> None:
        blob = json.dumps(self.authorities, sort_keys=True)
        self.assertNotIn(ARTIFACT_SHA256, blob)
        self.assertNotIn(STREAM_SHA256, blob)

    def test_every_live_dependency_file_matches_its_pin(self) -> None:
        for path, (block, field) in DEPENDENCY_PINS:
            self.assertTrue(path.exists(), str(path))
            observed = hashlib.sha256(path.read_bytes()).hexdigest()
            self.assertEqual(observed, self.authorities[block][field], str(path))

    def test_trainer_schedule_artifact_schema_matches_the_pin(self) -> None:
        schedule = json.loads(
            (DATA / "native_trainer_schedule_v1_goldens.json").read_text()
        )
        self.assertEqual(
            schedule["schema"], self.authorities["trainer_schedule"]["goldens_schema"]
        )

    def test_authority_drift_rejected(self) -> None:
        drifted = copy.deepcopy(self.artifact["positive_cases"][0]["input"])
        drifted["source_authorities"]["runtime_deck_catalog"]["protocol"] = "drift"
        with self.assertRaises(ContractRejection) as caught:
            GEN.evaluate_trajectory(drifted)
        self.assertEqual(caught.exception.code, "authority-mismatch")


class OwnershipTest(unittest.TestCase):
    def setUp(self) -> None:
        self.base = load_artifact()["positive_cases"][0]["input"]

    def test_no_inner_digest_field(self) -> None:
        self.assertEqual(set(self.base), GEN.TRAJECTORY_INPUT_FIELDS)
        self.assertNotIn("inner_trajectory_sha256", self.base)

    def test_all_inner_override_names_rejected(self) -> None:
        overrides = (
            "inner_trajectory_sha256",
            "inner_trajectory_sha256_raw32",
            "inner_environment_seed_u64_hex",
            "inner_root_u64_hex",
            "inner_pair_environment_seed_u64_hex",
            "inner_learner_seat",
            "inner_deck_p0_id",
            "inner_deck_p0_hash_u64_hex",
            "inner_deck_p1_id",
            "inner_deck_p1_hash_u64_hex",
            "inner_episode_index_u64_hex",
        )
        for field in overrides:
            record = copy.deepcopy(self.base)
            record[field] = "0" * 64
            with self.assertRaises(ShapeError, msg=field):
                GEN.evaluate_trajectory(record)

    def test_missing_required_input_field_rejected(self) -> None:
        for field in sorted(GEN.TRAJECTORY_INPUT_FIELDS):
            record = copy.deepcopy(self.base)
            del record[field]
            with self.assertRaises(ShapeError, msg=field):
                GEN.evaluate_trajectory(record)


class RejectTest(unittest.TestCase):
    def setUp(self) -> None:
        self.artifact = load_artifact()

    def test_every_stored_trajectory_reject_classifies_exactly(self) -> None:
        for case in self.artifact["trajectory_reject_cases"]:
            with self.assertRaises(ContractRejection, msg=case["name"]) as caught:
                GEN.evaluate_trajectory(case["input"])
            self.assertEqual(caught.exception.code, case["expected_code"], case["name"])

    def test_every_stored_pair_reject_classifies_exactly(self) -> None:
        for case in self.artifact["pair_reject_cases"]:
            with self.assertRaises(ContractRejection, msg=case["name"]) as caught:
                GEN.evaluate_pair(case["input"])
            self.assertEqual(caught.exception.code, case["expected_code"], case["name"])

    def test_closed_vocabulary_covered_except_counter_overflow(self) -> None:
        stored = {c["expected_code"] for c in self.artifact["trajectory_reject_cases"]}
        stored |= {c["expected_code"] for c in self.artifact["pair_reject_cases"]}
        vocabulary = set(GEN.REJECTION_CODES)
        self.assertEqual(len(vocabulary), 22)
        self.assertEqual(vocabulary - stored, {"counter-overflow"})

    def test_terminal_episode_precedes_empty_and_open_group(self) -> None:
        names = {c["name"]: c for c in self.artifact["trajectory_reject_cases"]}
        for name in (
            "compound-empty-stream-and-terminal-episode-mismatch",
            "compound-open-group-and-terminal-episode-mismatch",
        ):
            self.assertIn(name, names)
            self.assertEqual(names[name]["expected_code"], "episode-mismatch")
            with self.assertRaises(ContractRejection) as caught:
                GEN.evaluate_trajectory(names[name]["input"])
            self.assertEqual(caught.exception.code, "episode-mismatch", name)
        # The non-compound mates still report their own codes.
        self.assertEqual(names["empty-decision-stream"]["expected_code"], "empty-decision-stream")
        self.assertEqual(
            names["incomplete-physical-group"]["expected_code"], "malformed-physical-group"
        )

    def test_both_deck_id_rejects_present(self) -> None:
        names = {c["name"] for c in self.artifact["trajectory_reject_cases"]}
        self.assertIn("deck-id-case-drift-burn", names)
        self.assertIn("deck-id-unknown-not-in-catalog", names)

    def test_legal_width_boundaries_present(self) -> None:
        names = {c["name"] for c in self.artifact["trajectory_reject_cases"]}
        self.assertIn("legal-action-count-zero", names)
        self.assertIn("legal-action-count-sixty-five", names)


class PairTest(unittest.TestCase):
    def setUp(self) -> None:
        self.pair = load_artifact()["pair_positive_cases"][0]

    def test_pair_invariants(self) -> None:
        record = self.pair["input"]
        even, odd = record["even_start"], record["odd_start"]
        base_seed = int(record["base_seed_u64_hex"], 16)
        pair_index = int(record["pair_index_u64_hex"], 16)
        self.assertEqual((base_seed, pair_index), (71_501, 0))
        self.assertEqual(int(even["episode_index_u64_hex"], 16), 2 * pair_index)
        self.assertEqual(int(odd["episode_index_u64_hex"], 16), 2 * pair_index + 1)
        self.assertEqual((even["learner_seat"], odd["learner_seat"]), ("p0", "p1"))
        derived = GEN.derive_train_env_root(base_seed, pair_index)
        self.assertEqual(derived, NATIVE_ROOT)
        for side in (even, odd):
            self.assertEqual(int(side["pair_environment_seed_u64_hex"], 16), derived)
        for field in ("deck_p0_id", "deck_p0_hash_u64_hex", "deck_p1_id", "deck_p1_hash_u64_hex"):
            self.assertEqual(even[field], odd[field], field)

    def test_stored_pair_digests_match_recomputation(self) -> None:
        even_sha, odd_sha = GEN.evaluate_pair(self.pair["input"])
        self.assertEqual(even_sha, self.pair["even_trajectory_sha256"])
        self.assertEqual(odd_sha, self.pair["odd_trajectory_sha256"])
        self.assertNotEqual(even_sha, odd_sha)

    def test_pair_arithmetic_boundaries(self) -> None:
        record = copy.deepcopy(self.pair["input"])
        record["base_seed_u64_hex"] = f"{1 << 63:016x}"
        with self.assertRaises(ContractRejection) as caught:
            GEN.evaluate_pair(record)
        self.assertEqual(caught.exception.code, "schedule-integer-outside-u63")
        record = copy.deepcopy(self.pair["input"])
        record["pair_index_u64_hex"] = f"{1 << 62:016x}"
        with self.assertRaises(ContractRejection) as caught:
            GEN.evaluate_pair(record)
        self.assertEqual(caught.exception.code, "pair-index-outside-episode-domain")
        GEN.derive_train_env_root((1 << 63) - 1, (1 << 62) - 1)




# ------------------------- local independent envelope and trainer-root builders


def local_envelope(case_input: dict, inner_stream: bytes) -> bytes:
    """Fully local 34-atom envelope rebuild. Every string and hash comes from the
    artifact's own authority values, every hash pin is decoded to raw 32 bytes,
    every integer is encoded at its exact big-endian width, and the final atom is
    a local hashlib digest of the stored inner stream."""
    authorities = case_input["source_authorities"]
    inner = authorities["inner_trajectory"]
    env = authorities["environment_randomization"]
    reset = authorities["reset_trajectory"]
    schedule = authorities["trainer_schedule"]
    catalog = authorities["runtime_deck_catalog"]
    episode = int(case_input["episode_index_u64_hex"], 16)

    def utf8(tag: str, value: str) -> bytes:
        return local_atom(tag, value.encode("utf-8"))

    def raw32(tag: str, pin: str) -> bytes:
        decoded = bytes.fromhex(pin)
        assert len(decoded) == 32
        return local_atom(tag, decoded)

    def u64(tag: str, value: int) -> bytes:
        return local_atom(tag, value.to_bytes(8, "big"))

    return b"".join(
        (
            local_atom(
                "domain", b"mtg-kernel-native-full-episode-trajectory-sha256-v2"
            ),
            utf8("inner_trajectory_identity_utf8", inner["identity"]),
            utf8("inner_trajectory_goldens_schema_utf8", inner["goldens_schema"]),
            utf8(
                "inner_trajectory_goldens_generator_identity_utf8",
                inner["goldens_generator_identity"],
            ),
            utf8(
                "inner_trajectory_golden_stream_identity_utf8",
                inner["golden_semantic_stream_identity"],
            ),
            raw32("inner_trajectory_goldens_file_sha256_raw32", inner["goldens_raw_file_sha256"]),
            raw32(
                "inner_trajectory_golden_stream_sha256_raw32",
                inner["golden_semantic_stream_sha256"],
            ),
            utf8("environment_randomization_identity_utf8", env["identity"]),
            utf8("environment_randomization_namespace_utf8", env["namespace"]),
            utf8(
                "environment_randomization_kdf_goldens_schema_utf8",
                env["kdf_goldens_schema"],
            ),
            raw32(
                "environment_randomization_kdf_goldens_file_sha256_raw32",
                env["kdf_goldens_raw_file_sha256"],
            ),
            utf8("reset_trajectory_goldens_schema_utf8", reset["goldens_schema"]),
            utf8("reset_trajectory_generator_identity_utf8", reset["generator_identity"]),
            utf8(
                "reset_trajectory_physical_projection_identity_utf8",
                reset["physical_projection_identity"],
            ),
            utf8(
                "reset_trajectory_vector_stream_identity_utf8",
                reset["portable_semantic_stream_identity"],
            ),
            raw32("reset_trajectory_goldens_file_sha256_raw32", reset["goldens_raw_file_sha256"]),
            raw32(
                "reset_trajectory_vector_stream_sha256_raw32",
                reset["portable_semantic_stream_sha256"],
            ),
            utf8("trainer_schedule_identity_utf8", schedule["identity"]),
            utf8("trainer_seed_version_utf8", schedule["seed_version"]),
            raw32(
                "trainer_schedule_goldens_file_sha256_raw32",
                schedule["goldens_raw_file_sha256"],
            ),
            utf8("runtime_deck_catalog_schema_utf8", catalog["schema"]),
            utf8("runtime_deck_protocol_utf8", catalog["protocol"]),
            utf8(
                "runtime_deck_materialization_protocol_utf8",
                catalog["materialization_protocol"],
            ),
            utf8("runtime_deck_hash_algorithm_utf8", catalog["deck_hash_algorithm"]),
            raw32("runtime_deck_catalog_file_sha256_raw32", catalog["catalog_raw_file_sha256"]),
            u64("episode_index_u64be", episode),
            u64("pair_index_u64be", episode // 2),
            u64(
                "pair_environment_seed_u64be",
                int(case_input["pair_environment_seed_u64_hex"], 16),
            ),
            utf8("deck_p0_id_utf8", case_input["deck_p0_id"]),
            u64("deck_p0_hash_u64be", int(case_input["deck_p0_hash_u64_hex"], 16)),
            utf8("deck_p1_id_utf8", case_input["deck_p1_id"]),
            u64("deck_p1_hash_u64be", int(case_input["deck_p1_hash_u64_hex"], 16)),
            local_atom(
                "learner_seat_u8",
                bytes((0 if case_input["learner_seat"] == "p0" else 1,)),
            ),
            local_atom("inner_trajectory_sha256_raw32", hashlib.sha256(inner_stream).digest()),
        )
    )


def local_train_env_root(base_seed: int, pair_index: int, seed_version: str) -> int:
    """Local trainer-root KDF over the frozen authority seed version and the
    frozen train-env namespace."""
    hasher = hashlib.sha256()
    hasher.update(local_atom("version", seed_version.encode("utf-8")))
    hasher.update(local_atom("namespace", b"train-env"))
    hasher.update(local_atom("field-name", b"base_seed"))
    hasher.update(local_atom("u63", base_seed.to_bytes(8, "big")))
    hasher.update(local_atom("field-name", b"pair_index"))
    hasher.update(local_atom("u63", pair_index.to_bytes(8, "big")))
    return int.from_bytes(hasher.digest()[:8], "big") & ((1 << 63) - 1)


class LocalEnvelopeTest(unittest.TestCase):
    def setUp(self) -> None:
        self.artifact = load_artifact()

    def test_local_envelope_bytes_and_digest_match_every_stored_case(self) -> None:
        for case in self.artifact["positive_cases"]:
            inner = bytes.fromhex(case["inner_stream_hex"])
            self.assertEqual(
                hashlib.sha256(inner).hexdigest(), case["inner_sha256"], case["name"]
            )
            built = local_envelope(case["input"], inner)
            self.assertEqual(built.hex(), case["v2_stream_hex"], case["name"])
            self.assertEqual(
                hashlib.sha256(built).hexdigest(), case["v2_sha256"], case["name"]
            )
            self.assertEqual(len(parse_atoms(built)), 34, case["name"])

    def test_generator_envelope_matches_the_local_build_secondarily(self) -> None:
        for case in self.artifact["positive_cases"]:
            _, envelope = GEN.evaluate_trajectory(case["input"])
            inner = bytes.fromhex(case["inner_stream_hex"])
            self.assertEqual(envelope, local_envelope(case["input"], inner), case["name"])


class LocalPairTest(unittest.TestCase):
    def setUp(self) -> None:
        self.artifact = load_artifact()
        self.pair = self.artifact["pair_positive_cases"][0]
        self.positives = {c["name"]: c for c in self.artifact["positive_cases"]}

    def test_pair_starts_tie_to_matching_positive_cases(self) -> None:
        record = self.pair["input"]
        even_match = [
            c for c in self.positives.values() if c["input"] == record["even_start"]
        ]
        odd_match = [
            c for c in self.positives.values() if c["input"] == record["odd_start"]
        ]
        self.assertEqual(len(even_match), 1, "even start must tie to one positive case")
        self.assertEqual(len(odd_match), 1, "odd start must tie to one positive case")
        self.assertEqual(even_match[0]["name"], "episode-0-native-root-learner-p0-p0-win")
        self.assertEqual(odd_match[0]["name"], "episode-1-native-root-learner-p1-p1-win")

    def test_local_root_and_locally_built_envelope_digests_match_the_pair_record(self) -> None:
        record = self.pair["input"]
        seed_version = self.artifact["source_authorities"]["trainer_schedule"]["seed_version"]
        derived = local_train_env_root(
            int(record["base_seed_u64_hex"], 16),
            int(record["pair_index_u64_hex"], 16),
            seed_version,
        )
        self.assertEqual(derived, NATIVE_ROOT)
        for side in ("even_start", "odd_start"):
            self.assertEqual(
                int(record[side]["pair_environment_seed_u64_hex"], 16), derived, side
            )
        digests = {}
        for side, field in (
            ("even_start", "even_trajectory_sha256"),
            ("odd_start", "odd_trajectory_sha256"),
        ):
            match = [c for c in self.positives.values() if c["input"] == record[side]][0]
            inner = bytes.fromhex(match["inner_stream_hex"])
            built = local_envelope(record[side], inner)
            digests[side] = hashlib.sha256(built).hexdigest()
            self.assertEqual(digests[side], self.pair[field], side)
        self.assertNotEqual(digests["even_start"], digests["odd_start"])

    def test_generator_pair_digests_match_secondarily(self) -> None:
        even_sha, odd_sha = GEN.evaluate_pair(self.pair["input"])
        self.assertEqual(even_sha, self.pair["even_trajectory_sha256"])
        self.assertEqual(odd_sha, self.pair["odd_trajectory_sha256"])


class HiddenFieldTest(unittest.TestCase):
    """Structural traversal must run before classification, so an early
    rejection cannot hide an unrelated unknown or missing field."""

    def setUp(self) -> None:
        self.artifact = load_artifact()

    def _reject(self, family: str, code: str) -> dict:
        for case in self.artifact[family]:
            if case["expected_code"] == code:
                return case
        raise AssertionError(f"no {family} case with code {code}")

    def test_unknown_field_hidden_behind_authority_mismatch_is_rejected(self) -> None:
        case = self._reject("trajectory_reject_cases", "authority-mismatch")
        drifted = copy.deepcopy(self.artifact)
        target = [c for c in drifted["trajectory_reject_cases"] if c["name"] == case["name"]][0]
        target["input"]["smuggled_unknown_field"] = 1
        # The classifier alone would still say authority-mismatch.
        with self.assertRaises(ContractRejection) as caught:
            GEN.evaluate_trajectory(case["input"])
        self.assertEqual(caught.exception.code, "authority-mismatch")
        with self.assertRaises(ShapeError):
            GEN.audit_artifact(drifted)

    def test_missing_field_hidden_behind_authority_mismatch_is_rejected(self) -> None:
        case = self._reject("trajectory_reject_cases", "authority-mismatch")
        drifted = copy.deepcopy(self.artifact)
        target = [c for c in drifted["trajectory_reject_cases"] if c["name"] == case["name"]][0]
        del target["input"]["deck_p1_id"]
        with self.assertRaises(ShapeError):
            GEN.audit_artifact(drifted)

    def test_hidden_fields_behind_pair_seed_overflow_are_rejected(self) -> None:
        case = self._reject("pair_reject_cases", "schedule-integer-outside-u63")
        for mutate in ("unknown", "missing"):
            drifted = copy.deepcopy(self.artifact)
            target = [c for c in drifted["pair_reject_cases"] if c["name"] == case["name"]][0]
            if mutate == "unknown":
                target["input"]["even_start"]["smuggled_unknown_field"] = 1
            else:
                del target["input"]["odd_start"]["terminal"]
            with self.assertRaises(ShapeError, msg=mutate):
                GEN.audit_artifact(drifted)

    def test_traversal_still_admits_intentionally_invalid_values(self) -> None:
        # A malformed commitment is a value defect, not a shape defect.
        case = self._reject("trajectory_reject_cases", "malformed-commitment")
        GEN.require_trajectory_shape(case["input"], "probe")
        # And an out-of-domain episode index likewise passes the shape gate.
        case = self._reject("trajectory_reject_cases", "episode-index-outside-u63")
        GEN.require_trajectory_shape(case["input"], "probe")

    def test_nested_unknown_and_missing_mutations_are_rejected(self) -> None:
        base = self.artifact["positive_cases"][0]["input"]
        # Decision row.
        row_unknown = copy.deepcopy(base)
        row_unknown["decisions"][0]["smuggled"] = 1
        with self.assertRaises(ShapeError):
            GEN.require_trajectory_shape(row_unknown, "probe")
        row_missing = copy.deepcopy(base)
        del row_missing["decisions"][0]["actor_seat"]
        with self.assertRaises(ShapeError):
            GEN.require_trajectory_shape(row_missing, "probe")
        # Terminal.
        term_unknown = copy.deepcopy(base)
        term_unknown["terminal"]["smuggled"] = 1
        with self.assertRaises(ShapeError):
            GEN.require_trajectory_shape(term_unknown, "probe")
        term_missing = copy.deepcopy(base)
        del term_missing["terminal"]["winner"]
        with self.assertRaises(ShapeError):
            GEN.require_trajectory_shape(term_missing, "probe")
        # Authority block.
        auth_missing = copy.deepcopy(base)
        del auth_missing["source_authorities"]["trainer_schedule"]["seed_version"]
        with self.assertRaises(ShapeError):
            GEN.require_trajectory_shape(auth_missing, "probe")

    def test_pair_outer_and_start_mutations_are_rejected(self) -> None:
        pair = self.artifact["pair_positive_cases"][0]["input"]
        outer_unknown = copy.deepcopy(pair)
        outer_unknown["smuggled"] = 1
        with self.assertRaises(ShapeError):
            GEN.require_pair_shape(outer_unknown, "probe")
        for key in ("base_seed_u64_hex", "pair_index_u64_hex", "even_start", "odd_start"):
            outer_missing = copy.deepcopy(pair)
            del outer_missing[key]
            with self.assertRaises(ShapeError, msg=key):
                GEN.require_pair_shape(outer_missing, "probe")
        for side in ("even_start", "odd_start"):
            start_unknown = copy.deepcopy(pair)
            start_unknown[side]["smuggled"] = 1
            with self.assertRaises(ShapeError, msg=side):
                GEN.require_pair_shape(start_unknown, "probe")
            start_missing = copy.deepcopy(pair)
            del start_missing[side]["learner_seat"]
            with self.assertRaises(ShapeError, msg=side):
                GEN.require_pair_shape(start_missing, "probe")


class CapTest(unittest.TestCase):
    def setUp(self) -> None:
        self.artifact = load_artifact()

    def test_case_cap_plus_one_fails(self) -> None:
        oversized = copy.deepcopy(self.artifact)
        template = oversized["trajectory_reject_cases"][0]
        oversized["trajectory_reject_cases"] = [
            {**copy.deepcopy(template), "name": f"cap-probe-{index:04d}"}
            for index in range(GEN.MAX_CASES + 1)
        ]
        self.assertEqual(len(oversized["trajectory_reject_cases"]), GEN.MAX_CASES + 1)
        with self.assertRaises(ShapeError):
            GEN.audit_artifact(oversized)

    def test_decision_cap_plus_one_fails(self) -> None:
        oversized = copy.deepcopy(self.artifact["positive_cases"][0]["input"])
        row = copy.deepcopy(oversized["decisions"][0])
        oversized["decisions"] = [copy.deepcopy(row) for _ in range(GEN.MAX_DECISIONS + 1)]
        self.assertEqual(len(oversized["decisions"]), GEN.MAX_DECISIONS + 1)
        with self.assertRaises(ShapeError):
            GEN.require_trajectory_shape(oversized, "probe")

    def test_exact_four_mebibyte_ceiling_and_current_size(self) -> None:
        self.assertEqual(GEN.MAX_ARTIFACT_BYTES, 4 * 1024 * 1024)
        self.assertEqual(len(ARTIFACT_PATH.read_bytes()), ARTIFACT_BYTES)
        self.assertLess(ARTIFACT_BYTES, GEN.MAX_ARTIFACT_BYTES)


class CliByteStabilityTest(unittest.TestCase):
    """A --check implementation that overwrote the target and then failed must
    not be able to pass, so target bytes are snapshotted around both calls."""

    def _run_check(self, target: Path):
        return subprocess.run(
            [sys.executable, "-B", str(GENERATOR_PATH), "--out", str(target), "--check"],
            capture_output=True,
            text=True,
            check=False,
        )

    def test_successful_check_does_not_rewrite_the_target(self) -> None:
        before = ARTIFACT_PATH.read_bytes()
        before_sha = hashlib.sha256(before).hexdigest()
        result = self._run_check(ARTIFACT_PATH)
        after = ARTIFACT_PATH.read_bytes()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("CHECK OK", result.stdout)
        self.assertEqual(after, before)
        self.assertEqual(hashlib.sha256(after).hexdigest(), before_sha)
        self.assertEqual(before_sha, ARTIFACT_SHA256)

    def test_failing_check_does_not_rewrite_the_target(self) -> None:
        import tempfile

        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "mutated.json"
            mutated = ARTIFACT_PATH.read_bytes().replace(b"draw", b"drav", 1)
            self.assertNotEqual(mutated, ARTIFACT_PATH.read_bytes())
            target.write_bytes(mutated)
            before_sha = hashlib.sha256(target.read_bytes()).hexdigest()
            result = self._run_check(target)
            after = target.read_bytes()
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(hashlib.sha256(after).hexdigest(), before_sha)
            self.assertEqual(after, mutated)


class IndependentCanonicalEncodingTest(unittest.TestCase):
    """The expected canonical bytes are produced by the local serializer only.
    The generator's own serializer is never consulted for the expectation."""

    def test_local_canonical_encoding_reproduces_the_stored_artifact(self) -> None:
        self.assertEqual(local_cj_with_lf(load_artifact()), ARTIFACT_PATH.read_bytes())

    def test_local_encoding_is_sorted_compact_ascii_with_one_final_lf(self) -> None:
        encoded = local_cj_with_lf(load_artifact())
        self.assertTrue(encoded.isascii())
        self.assertTrue(encoded.endswith(b"\n"))
        self.assertEqual(encoded.count(b"\n"), 1)
        self.assertNotIn(b", ", encoded)
        self.assertNotIn(b": ", encoded)
        decoded = json.loads(encoded.decode("ascii"))
        self.assertEqual(list(decoded), sorted(decoded))

    def test_generator_serializer_agrees_with_the_local_one_secondarily(self) -> None:
        artifact = load_artifact()
        self.assertEqual(GEN.canonical_file_bytes(artifact), local_cj_with_lf(artifact))


class ArtifactCeilingGuardTest(unittest.TestCase):
    """Exercise the ceiling guard itself rather than only asserting the constant."""

    def test_lowered_ceiling_makes_audit_artifact_reject(self) -> None:
        from unittest import mock

        artifact = load_artifact()
        current_size = len(local_cj_with_lf(artifact))
        self.assertEqual(current_size, ARTIFACT_BYTES)
        lowered = current_size - 1
        with mock.patch.object(GEN, "MAX_ARTIFACT_BYTES", lowered):
            self.assertEqual(GEN.MAX_ARTIFACT_BYTES, lowered)
            with self.assertRaises(ShapeError):
                GEN.audit_artifact(copy.deepcopy(artifact))
        # The constant is restored by the context manager.
        self.assertEqual(GEN.MAX_ARTIFACT_BYTES, 4 * 1024 * 1024)

    def test_guard_passes_again_after_restoration(self) -> None:
        self.assertEqual(GEN.MAX_ARTIFACT_BYTES, 4 * 1024 * 1024)
        GEN.audit_artifact(copy.deepcopy(load_artifact()))


if __name__ == "__main__":
    unittest.main()
