"""Byte-contract tests; synthetic manifests are not RNG or native qualification.

Real A/A/B generation and native import/update/recovery require separately
recorded execution. These fixtures deliberately contain no actual producer claim.
"""
from __future__ import annotations

import copy
import hashlib
import json
import struct
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from mtg_kernel_rl import phase1_fresh_initialization_v1 as fresh
from mtg_kernel_rl.model import ModelConfig


class FreshInitializationV1Tests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        # A structural payload, not output from trainer-seeded-v1.
        data = bytearray(fresh.PAYLOAD_BYTES)
        for ordinal, (name, _, offset, _) in enumerate(fresh._layout()):
            index = offset + (16 if name == "card_embedding.weight" else 0)
            struct.pack_into("<f", data, index * 4, (ordinal + 1) / 256.0)
        cls.payload = bytes(data)
        rows, payload_contract, anchor = fresh._parameter_records(cls.payload)
        sources = [{"path": name, "sha256": f"{i + 1:064x}", "bytes": 123}
                   for i, name in enumerate(fresh.SOURCE_PATHS)]
        pin = {"path": "/synthetic/runtime.bin", "sha256": "a" * 64, "bytes": 123}
        config = ModelConfig().to_dict()
        cls.manifest = {
            "schema": fresh.SCHEMA, "lineage_id": "synthetic-structure-only",
            "initializer": fresh._initializer(2026091301),
            "producer": {"source_git_commit": "b" * 40, "source_git_clean": False,
                "source_files": sources,
                "runtime": {"python_version": "synthetic", "python_implementation": "synthetic",
                    "platform_system": "Linux", "platform_machine": "synthetic", "byte_order": "little",
                    "torch_version": "synthetic", "device": "cpu", "dtype": "torch.float32",
                    "deterministic_algorithms": True, "num_threads": 1, "num_interop_threads": 1,
                    "python_executable": copy.deepcopy(pin), "torch_C": copy.deepcopy(pin),
                    "torch_cpu_library": copy.deepcopy(pin)}},
            "target": {"registry": {"path": "/synthetic/cards.json", "sha256": sources[7]["sha256"]},
                "card_db_hash": "c" * 16, "feature_contract_digest": "d" * 64,
                "feature_encoding_digest": "e" * 64, "features_source_sha256": sources[5]["sha256"],
                "feature_descriptor_sha256": sources[6]["sha256"], "registry_card_count": 184,
                "card_token_rule": fresh.TOKEN_RULE},
            "model": {"architecture": "kernel-policy-value-net-8", "generator_model_config": config,
                "generator_model_config_sha256": hashlib.sha256(fresh.canonical_json_v1(config)).hexdigest()},
            "payload": payload_contract, "parameters": rows, "optimizer_bootstrap": fresh._bootstrap(anchor),
        }

    def encoded(self, manifest: dict | None = None) -> bytes:
        return fresh.canonical_json_v1(self.manifest if manifest is None else manifest) + b"\n"

    def request(self) -> dict:
        return {"schema": fresh.REQUEST_SCHEMA, "lineage_id": "lineage-a", "base_seed": 2026091301,
                "target": {key: copy.deepcopy(self.manifest["target"][key]) for key in fresh.TARGET_KEYS}}

    def manifest_for_source_paths(self, source_paths: tuple[str, ...]) -> dict:
        sources = [{"path": name, "sha256": f"{i + 1:064x}", "bytes": 123}
                   for i, name in enumerate(source_paths)]
        manifest = copy.deepcopy(self.manifest)
        manifest["producer"]["source_files"] = sources
        manifest["target"]["registry"]["sha256"] = sources[7]["sha256"]
        manifest["target"]["features_source_sha256"] = sources[5]["sha256"]
        manifest["target"]["feature_descriptor_sha256"] = sources[6]["sha256"]
        return manifest

    def test_v4_source_paths_are_admitted_alongside_v3(self) -> None:
        # Dual admission (Jack's ruling): V4 is a new arm, never a cutover.
        manifest = self.manifest_for_source_paths(fresh.SOURCE_PATHS_V4)
        parsed = fresh.validate_initialization_bytes_v1(self.encoded(manifest), self.payload)
        self.assertEqual(parsed["producer"]["source_files"][5]["path"],
                          "python/mtg_kernel_rl/features_v7.py")
        self.assertEqual(parsed["producer"]["source_files"][6]["path"],
                          "data/flat_policy_v4/feature_contract_v4.json")
        # The sealed V3 fixture stays independently re-verifiable by the same
        # live tool: dual admission, not a hard cutover.
        self.assertEqual(fresh.validate_initialization_bytes_v1(self.encoded(), self.payload), self.manifest)

    def test_select_generation_matches_v3_or_v4_fingerprint_never_a_mixed_pair(self) -> None:
        from mtg_kernel_rl import features_v6, features_v7
        root = Path(fresh.__file__).resolve().parents[2]

        def live_hashes(source_paths: tuple[str, ...]) -> tuple[str, str]:
            return (
                hashlib.sha256((root / source_paths[5]).read_bytes()).hexdigest(),
                hashlib.sha256((root / source_paths[6]).read_bytes()).hexdigest(),
            )

        v3_source_sha256, v3_descriptor_sha256 = live_hashes(fresh.SOURCE_PATHS_V3)
        v4_source_sha256, v4_descriptor_sha256 = live_hashes(fresh.SOURCE_PATHS_V4)
        v3_target = {"feature_contract_digest": features_v6.feature_contract_fingerprint(),
                     "feature_encoding_digest": features_v6.encoding_contract_fingerprint(),
                     "features_source_sha256": v3_source_sha256,
                     "feature_descriptor_sha256": v3_descriptor_sha256}
        source_paths, module = fresh._select_generation_v1(v3_target, root)
        self.assertIs(module, features_v6)
        self.assertEqual(source_paths, fresh.SOURCE_PATHS_V3)
        v4_target = {"feature_contract_digest": features_v7.feature_contract_fingerprint(),
                     "feature_encoding_digest": features_v7.encoding_contract_fingerprint(),
                     "features_source_sha256": v4_source_sha256,
                     "feature_descriptor_sha256": v4_descriptor_sha256}
        source_paths, module = fresh._select_generation_v1(v4_target, root)
        self.assertIs(module, features_v7)
        self.assertEqual(source_paths, fresh.SOURCE_PATHS_V4)
        # A mixed pair (V3 contract/encoding fingerprint, V4 source/descriptor
        # hash) must match neither generation, never a per-field OR across
        # any of the four fields.
        mixed_target = {"feature_contract_digest": features_v6.feature_contract_fingerprint(),
                         "feature_encoding_digest": features_v6.encoding_contract_fingerprint(),
                         "features_source_sha256": v4_source_sha256,
                         "feature_descriptor_sha256": v4_descriptor_sha256}
        with self.assertRaises(fresh.FreshInitializationErrorV1):
            fresh._select_generation_v1(mixed_target, root)
        # Same idea the other way: V3 contract fingerprint alone, with every
        # other field genuinely V4, is still a mixed tuple.
        mixed_target_2 = {"feature_contract_digest": features_v6.feature_contract_fingerprint(),
                           "feature_encoding_digest": features_v7.encoding_contract_fingerprint(),
                           "features_source_sha256": v4_source_sha256,
                           "feature_descriptor_sha256": v4_descriptor_sha256}
        with self.assertRaises(fresh.FreshInitializationErrorV1):
            fresh._select_generation_v1(mixed_target_2, root)

    def test_mixed_v3_v4_source_paths_rejected(self) -> None:
        # A source_files list naming features_v7.py (V4) at index 5 but the
        # V3 feature_contract_v3.json at index 6 is a mixed tuple: rejected
        # by the exact structural check against SOURCE_PATHS_V4's own index
        # 6, never silently accepted by checking each index independently.
        mixed = list(fresh.SOURCE_PATHS_V4)
        mixed[6] = fresh.SOURCE_PATHS_V3[6]
        manifest = self.manifest_for_source_paths(mixed)
        with self.assertRaises(fresh.FreshInitializationErrorV1):
            fresh.validate_initialization_bytes_v1(self.encoded(manifest), self.payload)

    def test_exact_roundtrip_layout_and_bootstrap_requirement(self) -> None:
        parsed = fresh.validate_initialization_bytes_v1(self.encoded(), self.payload)
        self.assertEqual(parsed, self.manifest)
        self.assertEqual(parsed["payload"]["bytes"], 4923976)
        self.assertEqual(len(parsed["parameters"]), 33)
        self.assertEqual(parsed["parameters"][0]["shape"], [65537, 16])
        projection = [{key: row[key] for key in fresh.LAYOUT_KEYS} for row in parsed["parameters"]]
        expected = json.dumps(projection, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode("ascii")
        self.assertEqual(parsed["payload"]["parameter_layout_sha256"], hashlib.sha256(expected).hexdigest())
        self.assertNotEqual(hashlib.sha256(expected).hexdigest(), hashlib.sha256(expected + b"\n").hexdigest())
        anchor = parsed["parameters"][28]["byte_offset"]
        self.assertEqual(parsed["optimizer_bootstrap"]["scorer_bias_anchor_bits"],
                         int.from_bytes(self.payload[anchor:anchor + 4], "little"))
        self.assertEqual(parsed["optimizer_bootstrap"]["adam_step"], 0)
        self.assertNotIn("learning_rate", parsed["optimizer_bootstrap"])
        self.assertNotIn("state_sha256", parsed["optimizer_bootstrap"])

    def test_declared_seeds_are_repeatable_distinct_and_label_independent(self) -> None:
        a = fresh._initializer(2026091301)
        b = fresh._initializer(2026091302)
        self.assertEqual(a, fresh._initializer(2026091301))
        self.assertNotEqual(a["model_init_seed"], b["model_init_seed"])
        req = self.request()
        original = copy.deepcopy(req)
        self.assertEqual(fresh.validate_request_v1(req), req)
        self.assertEqual(req, original)
        req["lineage_id"] = "lineage-other"
        self.assertEqual(fresh._initializer(req["base_seed"]), a)
        for invalid in (-1, True, 1 << 63, 1.0):
            req["base_seed"] = invalid
            with self.subTest(seed=invalid), self.assertRaises(fresh.FreshInitializationErrorV1):
                fresh.validate_request_v1(req)

    def test_original_common_snapshot_remains_separate(self) -> None:
        from mtg_kernel_rl import common_model_snapshot_v1 as old
        self.assertEqual(old.BASE_SEED_V1, 0)
        self.assertEqual(old.MODEL_INIT_SEED_V1, 6443515232517447393)
        changed = copy.deepcopy(self.manifest)
        changed["schema"] = old.SNAPSHOT_SCHEMA_V1
        with self.assertRaises(fresh.FreshInitializationErrorV1):
            fresh.validate_initialization_bytes_v1(self.encoded(changed), self.payload)

    def test_payload_corruption_padding_and_nonfinite_are_rejected(self) -> None:
        corrupt = bytearray(self.payload)
        corrupt[70] ^= 1
        for label, payload in (("changed finite tensor", bytes(corrupt)),
                               ("trailing byte", self.payload + b"\0"),
                               ("truncated", self.payload[:-4])):
            with self.subTest(label=label), self.assertRaises(fresh.FreshInitializationErrorV1):
                fresh.validate_initialization_bytes_v1(self.encoded(), payload)
        for index, bits in ((0, 0x80000000), (64, 0x7f800000), (128, 0x7fc00000)):
            corrupt = bytearray(self.payload)
            struct.pack_into("<I", corrupt, index, bits)
            with self.subTest(index=index), self.assertRaises(fresh.FreshInitializationErrorV1):
                fresh._parameter_records(bytes(corrupt))

    def test_manifest_pin_shape_seed_layout_and_gauge_tampering_rejected(self) -> None:
        def mutations(value: dict) -> list[dict]:
            cases = []
            for keys, replacement in (
                (("initializer", "model_init_seed"), 1),
                (("producer", "source_git_clean"), 1),
                (("producer", "runtime", "num_threads"), True),
                (("target", "registry_card_count"), True),
                (("target", "registry", "sha256"), "f" * 64),
                (("payload", "parameter_layout_sha256"), "0" * 64),
                (("optimizer_bootstrap", "scorer_bias_anchor_bits"), 0),
                (("optimizer_bootstrap", "adam_step"), False),
            ):
                item = copy.deepcopy(value)
                dest = item
                for key in keys[:-1]:
                    dest = dest[key]
                dest[keys[-1]] = replacement
                cases.append(item)
            item = copy.deepcopy(value)
            item["parameters"][1]["byte_offset"] += 4
            cases.append(item)
            item = copy.deepcopy(value)
            item["producer"]["source_files"].reverse()
            cases.append(item)
            item = copy.deepcopy(value)
            item["producer"]["runtime"]["gpu_ordinal"] = 1
            cases.append(item)
            return cases
        for index, changed in enumerate(mutations(self.manifest)):
            with self.subTest(index=index), self.assertRaises(fresh.FreshInitializationErrorV1):
                fresh.validate_initialization_bytes_v1(self.encoded(changed), self.payload)

    def test_strict_json_unknown_keys_bounds_and_canonical_bytes(self) -> None:
        for raw in (b'{"x":1,"x":2}', b'{"x":NaN}', b'{"x":1e9999}', b"[" * 2000,
                    b" " * (fresh.JSON_CAP + 1)):
            with self.subTest(raw=raw[:20]), self.assertRaises(fresh.FreshInitializationErrorV1):
                fresh.parse_json_v1(raw)
        with self.assertRaises(fresh.FreshInitializationErrorV1):
            fresh.validate_initialization_bytes_v1(self.encoded() + b"\n", self.payload)
        request = self.request()
        request["checkpoint"] = None
        with self.assertRaises(fresh.FreshInitializationErrorV1):
            fresh.validate_request_v1(request)
        request = self.request()
        request["target"]["registry"]["path"] = "relative.json"
        with self.assertRaises(fresh.FreshInitializationErrorV1):
            fresh.validate_request_v1(request)

    def test_recorded_provenance_does_not_require_old_files_to_remain_live(self) -> None:
        # Portable validation must not silently become a current-checkout admission test.
        with mock.patch.object(fresh, "_pin", side_effect=AssertionError("unexpected file read")), \
                mock.patch.object(fresh, "_git", side_effect=AssertionError("unexpected git query")), \
                mock.patch.object(fresh, "_runtime", side_effect=AssertionError("unexpected runtime query")):
            parsed = fresh.validate_initialization_bytes_v1(self.encoded(), self.payload)
        self.assertFalse(parsed["producer"]["source_git_clean"])

    def test_fresh_publication_preserves_existing_files_and_rejects_bad_input(self) -> None:
        with tempfile.TemporaryDirectory(prefix="fresh-init-structural-") as temp:
            root = Path(temp).resolve()
            target = root / "new"
            result = fresh.write_initialization_v1(target, self.encoded(), self.payload)
            self.assertEqual(result["parameters"]["sha256"], hashlib.sha256(self.payload).hexdigest())
            before = {path.name: path.read_bytes() for path in target.iterdir()}
            with self.assertRaises(FileExistsError):
                fresh.write_initialization_v1(target, self.encoded(), self.payload)
            self.assertEqual(before, {path.name: path.read_bytes() for path in target.iterdir()})
            invalid = root / "invalid"
            with self.assertRaises(fresh.FreshInitializationErrorV1):
                fresh.write_initialization_v1(invalid, self.encoded(), b"short")
            self.assertFalse(invalid.exists())


if __name__ == "__main__":
    unittest.main()
