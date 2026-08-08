from __future__ import annotations

import unittest
from pathlib import Path

import mtg_kernel_rl.common_model_snapshot_v1 as frozen_snapshot
import mtg_kernel_rl.wide_model_snapshot_v1 as wide_snapshot


class WideModelSnapshotV1Tests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.repo_root = Path(__file__).resolve().parents[2]
        cls.wide_manifest_path, cls.wide_payload_path = (
            wide_snapshot.wide_snapshot_default_paths_v1(cls.repo_root)
        )
        cls.frozen_manifest_path, cls.frozen_payload_path = (
            frozen_snapshot.common_snapshot_default_paths_v1(cls.repo_root)
        )

    def test_wide_portable_check_passes(self) -> None:
        validated = wide_snapshot.wide_portable_check_v1(self.repo_root)
        self.assertEqual(
            validated.manifest["model"]["model_architecture_version"],
            "kernel-policy-value-net-8w128",
        )
        self.assertEqual(
            validated.manifest["payload"]["parameter_element_count"],
            wide_snapshot.WIDE_PARAMETER_ELEMENT_COUNT_V1,
        )
        self.assertEqual(
            validated.manifest["payload"]["parameter_element_count"], 2_750_754
        )
        self.assertEqual(
            validated.manifest["payload"]["payload_byte_count"], 11_003_016
        )
        self.assertEqual(
            validated.manifest["model"]["model_config_fingerprint"],
            wide_snapshot.WIDE_MODEL_CONFIG_FINGERPRINT_V1,
        )

    def test_wide_authority_regeneration_is_byte_identical(self) -> None:
        wide_snapshot.wide_authority_check_v1(self.repo_root)

    def test_frozen_snapshot_rejected_by_wide_loader(self) -> None:
        with self.assertRaises(wide_snapshot.CommonModelSnapshotErrorV1):
            wide_snapshot.validate_wide_snapshot_files_v1(
                self.frozen_manifest_path,
                self.frozen_payload_path,
                repo_root=self.repo_root,
            )

    def test_wide_snapshot_rejected_by_frozen_loader(self) -> None:
        with self.assertRaises(frozen_snapshot.CommonModelSnapshotErrorV1):
            frozen_snapshot.validate_snapshot_files_v1(
                self.wide_manifest_path,
                self.wide_payload_path,
                repo_root=self.repo_root,
            )

    def test_frozen_snapshot_still_untouched_by_wide_module_import(self) -> None:
        # Importing the wide module must not mutate or invalidate the frozen
        # authority snapshot's own regeneration check.
        frozen_snapshot.authority_check_v1(self.repo_root)

    def test_wide_config_from_dict_round_trips_and_rejects_frozen_values(self) -> None:
        config = wide_snapshot.WideModelConfigV1()
        restored = wide_snapshot.WideModelConfigV1.from_dict(config.to_dict())
        self.assertEqual(config, restored)
        bad = config.to_dict()
        bad["hidden_dim"] = 64
        with self.assertRaises(ValueError):
            wide_snapshot.WideModelConfigV1.from_dict(bad)


if __name__ == "__main__":
    unittest.main()
