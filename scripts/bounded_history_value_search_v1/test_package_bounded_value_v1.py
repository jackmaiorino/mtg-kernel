from __future__ import annotations

import argparse
import hashlib
import json
import struct
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import torch

import package_bounded_value_v1 as subject


class PackageBoundedValueTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.root = Path(self.tempdir.name)
        self.parent_root = self.root / "parent-source"
        self.parent_root.mkdir()
        (self.parent_root / "checkpoint.json").write_text(
            '{"schema":"synthetic-parent"}\n', encoding="utf-8"
        )
        (self.parent_root / "checkpoint.state.f32le").write_bytes(bytes(range(64)))
        self.parent_identity = {
            "manifest_sha256": subject._sha256(self.parent_root / "checkpoint.json"),
            "payload_sha256": subject._sha256(
                self.parent_root / "checkpoint.state.f32le"
            ),
            "native_state_sha256": "1" * 64,
            "model_parameter_sha256": "2" * 64,
            "adam_step": 1,
        }
        self.development_sha256 = "3" * 64
        self.initializer_path = self.root / "initializer.state.pt"
        self.model_state_path = self.root / "fitted.state.pt"
        self.fit_report_path = self.root / "fit.json"
        self.confirmation_path = self.root / "confirmation.json"
        self._write_sources()

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def _write_sources(self) -> None:
        initializer = subject.distill._model()
        initializer_payload = {"model_state_dict": initializer.state_dict()}
        torch.save(initializer_payload, self.initializer_path)
        self.initializer_sha256 = subject._sha256(self.initializer_path)

        fitted = subject.distill._model()
        fitted.load_state_dict(initializer.state_dict(), strict=True)
        with torch.no_grad():
            fitted.value_head.bias.fill_(0.25)
            fitted.state[0].bias.fill_(0.125)
        fitted_payload = {
            "schema": subject.bounded_fit.FIT_SCHEMA + ".state",
            "model_state_dict": fitted.state_dict(),
            "initializer_state_sha256": self.initializer_sha256,
            "development_cache_sha256": self.development_sha256,
        }
        torch.save(fitted_payload, self.model_state_path)
        self.model_state_sha256 = subject._sha256(self.model_state_path)
        report = {
            "schema": subject.bounded_fit.FIT_SCHEMA,
            "status": "complete",
            "source": {"cache_sha256": self.development_sha256},
            "initializer": {"sha256": self.initializer_sha256},
            "parameterization": "tanh-addition-projected-parent-bounded-value/v1",
            "config": {"parent_projection_epsilon": 0.001},
            "initial_alignment": {"pass": True},
            "model_state": {"sha256": self.model_state_sha256},
        }
        self.fit_report_path.write_text(
            json.dumps(report, sort_keys=True, indent=2) + "\n", encoding="utf-8"
        )
        confirmation = {
            "schema": subject.bounded_fit.CONFIRM_SCHEMA,
            "status": "pass",
            "gates": {"bounded_value_confirmation_pass": True},
            "fit": {"sha256": subject._sha256(self.fit_report_path)},
            "model_state": {"sha256": self.model_state_sha256},
        }
        self.confirmation_path.write_text(
            json.dumps(confirmation, sort_keys=True, indent=2) + "\n", encoding="utf-8"
        )

    def _args(self, output_root: Path) -> argparse.Namespace:
        return argparse.Namespace(
            fit_report=self.fit_report_path,
            model_state=self.model_state_path,
            initializer_state=self.initializer_path,
            confirmation=self.confirmation_path,
            parent_root=self.parent_root,
            output_root=output_root,
        )

    def _package(self, output_root: Path) -> dict[str, object]:
        with mock.patch.object(
            subject.bounded_fit,
            "DEVELOPMENT_CACHE_SHA256",
            self.development_sha256,
        ), mock.patch.object(
            subject.bounded_fit,
            "INITIALIZER_STATE_SHA256",
            self.initializer_sha256,
        ), mock.patch.object(subject, "PARENT_IDENTITY", self.parent_identity):
            return subject.package(self._args(output_root))

    def test_packages_exact_bounded_value_candidate(self) -> None:
        output_root = self.root / "candidate"
        summary = self._package(output_root)

        self.assertEqual(
            {path.name for path in output_root.iterdir()},
            {"parent", "weights.f32le", "report.json", subject.CANDIDATE_FILENAME},
        )
        self.assertEqual(
            {path.name for path in (output_root / "parent").iterdir()},
            {"checkpoint.json", "checkpoint.state.f32le"},
        )
        self.assertEqual(
            (output_root / "parent/checkpoint.json").read_bytes(),
            (self.parent_root / "checkpoint.json").read_bytes(),
        )
        self.assertEqual(
            (output_root / "parent/checkpoint.state.f32le").read_bytes(),
            (self.parent_root / "checkpoint.state.f32le").read_bytes(),
        )
        candidate_path = output_root / subject.CANDIDATE_FILENAME
        candidate = json.loads(candidate_path.read_text(encoding="utf-8"))
        report = json.loads((output_root / "report.json").read_text(encoding="utf-8"))
        weights = (output_root / "weights.f32le").read_bytes()
        self.assertEqual(candidate["schema"], subject.CANDIDATE_SCHEMA)
        self.assertEqual(report["schema"], subject.REPORT_SCHEMA)
        self.assertEqual(candidate["architecture"]["identity"], subject.ARCHITECTURE)
        self.assertEqual(candidate["architecture"]["value_model"], subject.VALUE_MODEL)
        self.assertEqual(candidate["parent"], {"directory": "parent", **self.parent_identity})
        self.assertEqual(candidate["weights"]["parameter_count"], 107_378)
        self.assertEqual(candidate["weights"]["byte_count"], 107_378 * 4)
        self.assertEqual(len(weights), 107_378 * 4)
        self.assertEqual(candidate["weights"]["sha256"], hashlib.sha256(weights).hexdigest())
        self.assertEqual(candidate["report"]["sha256"], subject._sha256(output_root / "report.json"))
        self.assertEqual(summary["candidate_json_sha256"], subject._sha256(candidate_path))
        self.assertEqual(
            candidate["composite_model_parameter_sha256"],
            hashlib.sha256(
                subject.COMPOSITE_DOMAIN
                + bytes.fromhex(self.parent_identity["model_parameter_sha256"])
                + weights
            ).hexdigest(),
        )
        first = candidate["weights"]["parameters"][0]
        self.assertEqual(first["offset_f32"], 0)
        first_value = struct.unpack("<f", weights[:4])[0]
        fitted_payload = torch.load(
            self.model_state_path, map_location="cpu", weights_only=False
        )
        expected_first = next(iter(fitted_payload["model_state_dict"].values())).flatten()[0]
        self.assertEqual(first_value, float(expected_first))

    def test_refuses_overwrite_and_policy_head_drift(self) -> None:
        output_root = self.root / "candidate"
        self._package(output_root)
        with self.assertRaisesRegex(ValueError, "refusing to overwrite"):
            self._package(output_root)

        drifted = torch.load(self.model_state_path, map_location="cpu", weights_only=False)
        drifted["model_state_dict"]["policy_head.bias"].fill_(1.0)
        torch.save(drifted, self.model_state_path)
        report = json.loads(self.fit_report_path.read_text(encoding="utf-8"))
        report["model_state"]["sha256"] = subject._sha256(self.model_state_path)
        self.fit_report_path.write_text(
            json.dumps(report, sort_keys=True, indent=2) + "\n", encoding="utf-8"
        )
        confirmation = json.loads(self.confirmation_path.read_text(encoding="utf-8"))
        confirmation["fit"]["sha256"] = subject._sha256(self.fit_report_path)
        confirmation["model_state"]["sha256"] = subject._sha256(
            self.model_state_path
        )
        self.confirmation_path.write_text(
            json.dumps(confirmation, sort_keys=True, indent=2) + "\n",
            encoding="utf-8",
        )
        with self.assertRaisesRegex(ValueError, "policy head"):
            self._package(self.root / "policy-head-drift")


if __name__ == "__main__":
    unittest.main()
