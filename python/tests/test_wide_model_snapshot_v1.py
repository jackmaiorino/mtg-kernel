from __future__ import annotations

import os
import platform
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

    def test_wide_authority_source_historical_literals_are_not_silently_overwritten_in_place(
        self,
    ) -> None:
        # Wide-snapshot sibling of
        # test_authority_source_historical_literals_are_not_silently_overwritten_in_place
        # (collab CLAUDE #221/#239).
        self.assertEqual(
            wide_snapshot.WIDE_FROZEN_AUTHORITY_SOURCE_HASHES_HISTORICAL_V1,
            (
                "2e3e830d4212b8c8f8085861b2508c49a6d7192b9621cef087dd396e22d12c59",
                "fce419176dbd15e2b911e5c5f688bb390e731e3817da142571f38b1a7cc778eb",
                "45bd3ad1efb8b3ecb697961655fa51ce8e23efd2b11b3ecee8f7ef9bd29c4f35",
                "9f7520edcaae80fdd6478f0cf7f2fb8035a56efb7ce2860e36b2ad3b511afb5d",
            ),
        )
        self.assertEqual(
            wide_snapshot.WIDE_FROZEN_AUTHORITY_SOURCE_BUNDLE_SHA256_HISTORICAL_V1,
            "85446eae753b1055d3dedeb56b7080a49327eeee52e492b74f42a0cfde52cb8b",
        )

    def test_wide_authority_source_binding_accepts_both_profiles_and_rejects_hybrids(
        self,
    ) -> None:
        historical_sources = wide_snapshot._wide_historical_authority_sources()
        historical_bundle = wide_snapshot.WIDE_FROZEN_AUTHORITY_SOURCE_BUNDLE_SHA256_HISTORICAL_V1
        current_sources, current_bundle = wide_snapshot._wide_source_records(self.repo_root)
        self.assertTrue(
            wide_snapshot._wide_authority_source_binding_is_known(
                historical_sources, historical_bundle, self.repo_root
            )
        )
        self.assertTrue(
            wide_snapshot._wide_authority_source_binding_is_known(
                current_sources, current_bundle, self.repo_root
            )
        )
        self.assertFalse(
            wide_snapshot._wide_authority_source_binding_is_known(
                current_sources, historical_bundle, self.repo_root
            )
        )
        self.assertFalse(
            wide_snapshot._wide_authority_source_binding_is_known(
                historical_sources, current_bundle, self.repo_root
            )
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

    @unittest.skipUnless(
        os.name == "nt"
        and platform.machine() == "AMD64"
        and platform.python_version() == "3.13.14",
        "declared authority runtime regression",
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

    @unittest.skipUnless(
        os.name == "nt"
        and platform.machine() == "AMD64"
        and platform.python_version() == "3.13.14",
        "declared authority runtime regression",
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
