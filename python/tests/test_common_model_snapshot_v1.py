from __future__ import annotations

import copy
import json
import math
import os
import struct
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import torch

import mtg_kernel_rl.common_model_snapshot_v1 as snapshot
from mtg_kernel_rl.checkpoint import create_adam
from mtg_kernel_rl.model import INITIALIZER_RUNNER_FIXED_V1, KernelPolicyValueNet, ModelConfig


class CommonModelSnapshotV1Tests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.repo_root = Path(__file__).resolve().parents[2]
        cls.manifest_path, cls.payload_path = snapshot.common_snapshot_default_paths_v1(
            cls.repo_root
        )
        cls.manifest_bytes = cls.manifest_path.read_bytes()
        cls.payload_bytes = cls.payload_path.read_bytes()
        cls.manifest = json.loads(cls.manifest_bytes)

    def _canonical_file(self, value: dict[str, object]) -> bytes:
        return snapshot.canonical_json_bytes(value) + b"\n"

    def _resign(self, value: dict[str, object], payload: bytes) -> bytes:
        result = copy.deepcopy(value)
        result["payload"]["sha256"] = snapshot._sha256(payload)
        for parameter in result["parameters"]:
            begin = parameter["byte_offset"]
            end = begin + parameter["byte_count"]
            parameter["tensor_sha256"] = snapshot._sha256(payload[begin:end])
        result["integrity"]["parameter_layout_sha256"] = snapshot._sha256(
            snapshot.canonical_json_bytes(snapshot._layout_projection(result["parameters"]))
        )
        entries = []
        for parameter in result["parameters"]:
            begin = parameter["byte_offset"]
            end = begin + parameter["byte_count"]
            entries.append(
                (parameter["name"], tuple(parameter["shape"]), payload[begin:end])
            )
        result["integrity"]["named_parameter_stream_sha256"] = (
            snapshot._parameter_stream_digest(entries)
        )
        core = snapshot._manifest_core_sha256(result)
        result["integrity"]["manifest_core_sha256"] = core
        result["integrity"]["snapshot_sha256"] = snapshot._snapshot_sha256(
            core, snapshot._sha256(payload)
        )
        return self._canonical_file(result)

    def _runner_state(self) -> snapshot.PythonCommonSnapshotTrainStateV1:
        model = KernelPolicyValueNet(
            ModelConfig(), initializer=INITIALIZER_RUNNER_FIXED_V1
        )
        optimizer = create_adam(model, 0.001)
        return snapshot.PythonCommonSnapshotTrainStateV1(
            model=model,
            optimizer=optimizer,
            adam_step=17,
            scorer_bias_anchor_f32_bits=123,
            model_snapshot=None,
            counters={"sentinel": 7},
            publications=["publication"],
            records=["record"],
        )

    def test_portable_check_does_not_invoke_seeded_initializer(self) -> None:
        with mock.patch.object(
            KernelPolicyValueNet,
            "reset_seeded_parameters",
            side_effect=AssertionError("portable validation invoked seeded generation"),
        ):
            validated = snapshot.portable_check_v1(self.repo_root)
        self.assertEqual(
            validated.manifest["integrity"]["snapshot_sha256"],
            self.manifest["integrity"]["snapshot_sha256"],
        )
        self.assertEqual(len(validated.payload_bytes), snapshot.PAYLOAD_BYTE_COUNT_V1)

    def test_python_candidate_reexports_exact_payload_and_bootstraps_zero_adam(self) -> None:
        candidate, optimizer, record = snapshot.build_python_snapshot_candidate_v1(
            self.manifest_path,
            self.payload_path,
            learning_rate=0.001,
            repo_root=self.repo_root,
        )
        reexported, digest = snapshot._reexport_model_payload(candidate)
        self.assertEqual(reexported, self.payload_bytes)
        self.assertEqual(
            digest,
            self.manifest["integrity"]["named_parameter_stream_sha256"],
        )
        self.assertEqual(
            record["named_parameter_stream_sha256"],
            record["loaded_named_parameter_stream_sha256"],
        )
        self.assertFalse(record["rust_seeded_initializer_reproduced"])
        self.assertTrue(record["snapshot_load_completed_before_trial_start"])
        self.assertFalse(record["snapshot_load_timed"])
        self.assertEqual(
            set(record),
            {
                "schema",
                "identity",
                "snapshot_sha256",
                "manifest_file_sha256",
                "manifest_core_sha256",
                "payload_sha256",
                "payload_byte_count",
                "parameter_layout_sha256",
                "named_parameter_stream_sha256",
                "loaded_named_parameter_stream_sha256",
                "parameter_tensor_count",
                "parameter_element_count",
                "model_config_fingerprint",
                "model_architecture_version",
                "feature_contract_digest",
                "feature_encoding_digest",
                "initializer_identity",
                "base_seed",
                "model_init_seed",
                "trainer_schedule_version",
                "python_reference_seed_version",
                "schedule_goldens_sha256",
                "authority_source_bundle_sha256",
                "authority_runtime_identity",
                "loader_identity",
                "optimizer_identity",
                "adam_step_initial",
                "moment_initialization",
                "canonical_gauge_parameters",
                "scorer_bias_anchor_f32_bits",
                "snapshot_load_completed_before_trial_start",
                "snapshot_load_timed",
                "rust_seeded_initializer_reproduced",
                "nonclaim",
            },
        )
        named = dict(candidate.named_parameters())
        scorer_bytes = named["scorer.2.bias"].detach().contiguous().numpy().tobytes()
        self.assertEqual(
            int.from_bytes(scorer_bytes, "little"),
            record["scorer_bias_anchor_f32_bits"],
        )
        for name, parameter in candidate.named_parameters():
            state = optimizer.state[parameter]
            self.assertEqual(set(state), {"step", "exp_avg", "exp_avg_sq"})
            for key in state:
                self.assertEqual(state[key].dtype, torch.float32, (name, key))
                self.assertTrue(
                    all(
                        bits == 0
                        for (bits,) in struct.iter_unpack(
                            "<I", state[key].detach().contiguous().numpy().tobytes()
                        )
                    ),
                    (name, key),
                )
        value_payload = b"".join(
            self.payload_bytes[
                entry["byte_offset"] : entry["byte_offset"] + entry["byte_count"]
            ]
            for entry in self.manifest["parameters"][29:]
        )
        value_loaded = b"".join(
            parameter.detach().contiguous().numpy().tobytes()
            for name, parameter in candidate.named_parameters()
            if name.startswith("value_head.")
        )
        self.assertEqual(value_loaded, value_payload)

    def test_failed_install_preserves_all_live_state(self) -> None:
        state = self._runner_state()
        model_identity = id(state.model)
        optimizer_identity = id(state.optimizer)
        parameter_bits = [
            parameter.detach().contiguous().numpy().tobytes()
            for parameter in state.model.parameters()
        ]
        counters = copy.deepcopy(state.counters)
        publications = copy.deepcopy(state.publications)
        records = copy.deepcopy(state.records)
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            manifest_path = root / "manifest.json"
            payload_path = root / "parameters.f32le"
            manifest_path.write_bytes(self.manifest_bytes)
            corrupted = bytearray(self.payload_bytes)
            corrupted[400] ^= 1
            payload_path.write_bytes(corrupted)
            with self.assertRaises(snapshot.CommonModelSnapshotErrorV1):
                snapshot.load_python_snapshot_into_state_v1(
                    state,
                    manifest_path,
                    payload_path,
                    learning_rate=0.001,
                    repo_root=self.repo_root,
                )
        self.assertEqual(id(state.model), model_identity)
        self.assertEqual(id(state.optimizer), optimizer_identity)
        self.assertEqual(state.adam_step, 17)
        self.assertEqual(state.scorer_bias_anchor_f32_bits, 123)
        self.assertIsNone(state.model_snapshot)
        self.assertEqual(state.counters, counters)
        self.assertEqual(state.publications, publications)
        self.assertEqual(state.records, records)
        self.assertEqual(
            [
                parameter.detach().contiguous().numpy().tobytes()
                for parameter in state.model.parameters()
            ],
            parameter_bits,
        )

    def test_successful_install_swaps_only_snapshot_bootstrap_fields(self) -> None:
        state = self._runner_state()
        counters = copy.deepcopy(state.counters)
        publications = copy.deepcopy(state.publications)
        records = copy.deepcopy(state.records)
        receipt = snapshot.load_python_snapshot_into_state_v1(
            state,
            self.manifest_path,
            self.payload_path,
            learning_rate=0.001,
            repo_root=self.repo_root,
        )
        self.assertEqual(state.adam_step, 0)
        self.assertEqual(
            state.scorer_bias_anchor_f32_bits,
            receipt["scorer_bias_anchor_f32_bits"],
        )
        self.assertEqual(state.model_snapshot, receipt)
        self.assertEqual(state.counters, counters)
        self.assertEqual(state.publications, publications)
        self.assertEqual(state.records, records)

    def test_strict_json_binding_and_layout_mutations_are_rejected(self) -> None:
        self.assertRaises(
            snapshot.CommonModelSnapshotErrorV1,
            snapshot.validate_snapshot_bytes_v1,
            b"",
            self.payload_bytes,
            repo_root=self.repo_root,
        )
        for manifest in (
            self.manifest_bytes[:-1],
            self.manifest_bytes + b"\n",
            b" " + self.manifest_bytes,
        ):
            with self.assertRaises(snapshot.CommonModelSnapshotErrorV1):
                snapshot.validate_snapshot_bytes_v1(
                    manifest, self.payload_bytes, repo_root=self.repo_root
                )
        duplicate = self.manifest_bytes.replace(
            b'"schema":',
            b'"schema":"mtg-kernel-common-model-snapshot/v1","schema":',
            1,
        )
        with self.assertRaises(snapshot.CommonModelSnapshotErrorV1):
            snapshot.validate_snapshot_bytes_v1(
                duplicate, self.payload_bytes, repo_root=self.repo_root
            )

        mutations: list[dict[str, object]] = []
        value = copy.deepcopy(self.manifest)
        value["unknown"] = 1
        mutations.append(value)
        value = copy.deepcopy(self.manifest)
        del value["purpose"]
        mutations.append(value)
        value = copy.deepcopy(self.manifest)
        value["schema"] = 1
        mutations.append(value)
        value = copy.deepcopy(self.manifest)
        value["identity"] = "wrong"
        mutations.append(value)
        value = copy.deepcopy(self.manifest)
        value["model"]["model_config"]["hidden_dim"] = 65
        mutations.append(value)
        value = copy.deepcopy(self.manifest)
        value["authority"]["sources"][0]["sha256"] = "00"
        mutations.append(value)
        value = copy.deepcopy(self.manifest)
        value["initializer"]["trainer_schedule_version"] = "wrong"
        mutations.append(value)
        value = copy.deepcopy(self.manifest)
        value["initializer"]["model_init_seed"] = 1
        mutations.append(value)
        value = copy.deepcopy(self.manifest)
        value["parameters"][0], value["parameters"][1] = (
            value["parameters"][1],
            value["parameters"][0],
        )
        mutations.append(value)
        value = copy.deepcopy(self.manifest)
        value["parameters"][1]["name"] = value["parameters"][0]["name"]
        mutations.append(value)
        value = copy.deepcopy(self.manifest)
        value["parameters"][1]["name"] = "wrong"
        mutations.append(value)
        value = copy.deepcopy(self.manifest)
        value["parameters"][1]["shape"] = [114, 64]
        mutations.append(value)
        value = copy.deepcopy(self.manifest)
        value["parameters"][1]["shape"] = [7296]
        mutations.append(value)
        value = copy.deepcopy(self.manifest)
        value["parameters"][1]["element_offset"] += 1
        mutations.append(value)
        value = copy.deepcopy(self.manifest)
        value["parameters"][1]["element_offset"] -= 1
        mutations.append(value)
        value = copy.deepcopy(self.manifest)
        value["parameters"][1]["byte_offset"] = (1 << 64) - 1
        mutations.append(value)
        value = copy.deepcopy(self.manifest)
        value["parameters"][-1]["byte_count"] += 4
        mutations.append(value)
        value = copy.deepcopy(self.manifest)
        value["integrity"]["parameter_layout_sha256"] = "00"
        mutations.append(value)
        value = copy.deepcopy(self.manifest)
        value["integrity"]["named_parameter_stream_sha256"] = "00"
        mutations.append(value)
        value = copy.deepcopy(self.manifest)
        value["optimizer_bootstrap"]["scorer_bias_anchor_f32_bits"] ^= 1
        mutations.append(value)
        for mutation in mutations:
            with self.assertRaises(snapshot.CommonModelSnapshotErrorV1):
                snapshot.validate_snapshot_bytes_v1(
                    self._canonical_file(mutation),
                    self.payload_bytes,
                    repo_root=self.repo_root,
                )

    def test_each_tensor_payload_corruption_and_endian_swap_are_rejected(self) -> None:
        for entry in self.manifest["parameters"]:
            corrupted = bytearray(self.payload_bytes)
            corrupted[entry["byte_offset"]] ^= 1
            with self.assertRaises(snapshot.CommonModelSnapshotErrorV1):
                snapshot.validate_snapshot_bytes_v1(
                    self.manifest_bytes, bytes(corrupted), repo_root=self.repo_root
                )
        endian = bytearray(self.payload_bytes)
        for offset in range(0, len(endian), 4):
            endian[offset : offset + 4] = reversed(endian[offset : offset + 4])
        with self.assertRaises(snapshot.CommonModelSnapshotErrorV1):
            snapshot.validate_snapshot_bytes_v1(
                self.manifest_bytes, bytes(endian), repo_root=self.repo_root
            )

    def test_resigned_nonfinite_padding_and_anchor_mutations_are_rejected(self) -> None:
        for value in (math.nan, math.inf, -math.inf):
            payload = bytearray(self.payload_bytes)
            payload[64:68] = struct.pack("<f", value)
            manifest = self._resign(self.manifest, bytes(payload))
            with self.assertRaises(snapshot.CommonModelSnapshotErrorV1):
                snapshot.validate_snapshot_bytes_v1(
                    manifest, bytes(payload), repo_root=self.repo_root
                )
        for bits in (1, 0x8000_0000):
            payload = bytearray(self.payload_bytes)
            payload[:4] = struct.pack("<I", bits)
            manifest = self._resign(self.manifest, bytes(payload))
            with self.assertRaises(snapshot.CommonModelSnapshotErrorV1):
                snapshot.validate_snapshot_bytes_v1(
                    manifest, bytes(payload), repo_root=self.repo_root
                )
        changed = copy.deepcopy(self.manifest)
        changed["optimizer_bootstrap"]["scorer_bias_anchor_f32_bits"] ^= 1
        manifest = self._resign(changed, self.payload_bytes)
        with self.assertRaises(snapshot.CommonModelSnapshotErrorV1):
            snapshot.validate_snapshot_bytes_v1(
                manifest, self.payload_bytes, repo_root=self.repo_root
            )

    def test_file_capture_rejects_oversized_nonregular_links_and_races(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            regular = root / "regular.bin"
            regular.write_bytes(b"abc")
            self.assertEqual(snapshot._capture_regular_file(regular, 3), b"abc")
            with self.assertRaises(snapshot.CommonModelSnapshotErrorV1):
                snapshot._capture_regular_file(regular, 2)
            with self.assertRaises(snapshot.CommonModelSnapshotErrorV1):
                snapshot._capture_regular_file(root, 100)

            oversized = root / "oversized.bin"
            with oversized.open("wb") as handle:
                handle.truncate(snapshot.PAYLOAD_MAX_BYTES_V1 + 1)
            with self.assertRaises(snapshot.CommonModelSnapshotErrorV1):
                snapshot._capture_regular_file(
                    oversized, snapshot.PAYLOAD_MAX_BYTES_V1
                )

            race = root / "race.bin"
            race.write_bytes(b"abc")

            def mutate() -> None:
                with race.open("ab") as handle:
                    handle.write(b"d")
                    handle.flush()
                    os.fsync(handle.fileno())

            with self.assertRaises(snapshot.CommonModelSnapshotErrorV1):
                snapshot._capture_regular_file(
                    race, 100, after_read_hook=mutate
                )

            link = root / "link.bin"
            try:
                link.symlink_to(regular)
            except OSError:
                pass
            else:
                with self.assertRaises(snapshot.CommonModelSnapshotErrorV1):
                    snapshot._capture_regular_file(link, 100)


if __name__ == "__main__":
    unittest.main()
