import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import native_artifact_identity_v1 as identity


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    data = value if isinstance(value, bytes) else json.dumps(value).encode()
    path.write_bytes(data)
    return {"path": str(path), "sha256": hashlib.sha256(data).hexdigest()}


class PortableIdentityTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()

    def chain(self, name, attempt=1, *, change=None, content_addressed=False):
        root = self.root / name
        descriptor_value = {
            "export_directory": str(root / "inputs" / "export"),
            "source_run_path": str(root / "inputs" / "source-run.json"),
            "expected_model_parameter_sha256": "a" * 64,
            "source_registry_path": str(root / "inputs" / "cards.json"),
            "expected_source_registry_sha256": "b" * 64,
        }
        if content_addressed:
            raw = hashlib.sha256(json.dumps(descriptor_value).encode()).hexdigest()
            descriptor_path = root / "inputs" / "objects" / raw / "play-import.json"
        else:
            descriptor_path = root / "inputs" / "play-import.json"
        descriptor = save(descriptor_path, descriptor_value)
        dependencies = [descriptor]
        previous = None
        for block in (1, 2):
            folder = root / "G1" / f"b{block:03}-a{attempt}"
            episode = {
                "schema": "mtg-kernel-expanded-deck-trajectory/v1",
                "episode": {"id": str(block), "seed": 99, "opponent": {
                    "play_import": descriptor, "checkpoint": previous}},
                "seat_behaviors": [{"identity": {"checkpoint_sha256":
                    previous["sha256"] if previous else None}}],
                "decisions": [{"selected": 2, "logits_bits": [1, 0x80000000]}],
                "terminal": {"winner": 0},
            }
            if change == "action" and block == 1:
                episode["decisions"][0]["selected"] = 3
            trajectory = save(folder / "collect" / "episode-0000.json", episode)
            checkpoint = {"schema": "mtg-kernel-expanded-deck-checkpoint/v1",
                          "state_sha256": "c" * 64, "adam_step": block,
                          "parameters": [1, 2], "first_moments": [3, 4],
                          "second_moments": [5, 6], "trajectories": [trajectory]}
            if change == "moment" and block == 1:
                checkpoint["second_moments"][0] = 7
            previous = save(folder / "checkpoint.json", checkpoint)
            dependencies.extend([trajectory, previous])
        return root, previous, dependencies

    def report(self, chain):
        root, pin, dependencies = chain
        return identity.Identity({"native": str(root)}, dependencies).report(pin)

    def test_whole_chain_matches_across_root_and_attempt_without_dropping_content(self):
        left = self.chain("straight")
        right = self.chain("retry", 7)
        before = {pin["path"]: Path(pin["path"]).read_bytes() for pin in left[2] + right[2]}
        a, b = self.report(left), self.report(right)
        self.assertNotEqual(a["file_sha256"], b["file_sha256"])
        self.assertEqual(a["content_sha256"], b["content_sha256"])
        self.assertEqual(len(a["verified_artifacts"]), 5)
        self.assertNotEqual(left[2][0]["sha256"], right[2][0]["sha256"])
        for path, data in before.items():
            self.assertEqual(Path(path).read_bytes(), data)
        for change in ("action", "moment"):
            with self.subTest(change=change):
                different = self.report(self.chain(change, 4, change=change))
                # Even with the same final state_sha256/Adam step, a prior
                # action or optimizer bit stays committed by the whole chain.
                self.assertNotEqual(a["content_sha256"], different["content_sha256"])

    def test_pin_mismatch_missing_file_and_missing_hash_only_dependency_refuse(self):
        root, pin, dependencies = self.chain("broken")
        descriptor = Path(dependencies[0]["path"])
        original = descriptor.read_bytes()
        descriptor.write_bytes(original + b" ")
        with self.assertRaisesRegex(ValueError, "pin mismatch"):
            self.report((root, pin, dependencies))
        descriptor.unlink()
        with self.assertRaises(FileNotFoundError):
            self.report((root, pin, dependencies))
        unknown = save(root / "unknown.json", {"checkpoint_sha256": "d" * 64})
        with self.assertRaisesRegex(ValueError, "missing retained dependency"):
            identity.Identity({"native": str(root)}).report(unknown)

    def test_cloud_content_addressed_import_paths_normalize_the_verified_hash_component(self):
        left = self.chain("cloud-left", content_addressed=True)
        right = self.chain("cloud-right", 7, content_addressed=True)
        self.assertNotEqual(left[2][0]["path"], right[2][0]["path"])
        self.assertNotEqual(left[2][0]["sha256"], right[2][0]["sha256"])
        self.assertEqual(self.report(left)["content_sha256"], self.report(right)["content_sha256"])
        changed = self.chain("cloud-change", 8, content_addressed=True, change="action")
        self.assertNotEqual(self.report(left)["content_sha256"], self.report(changed)["content_sha256"])
        # The same 64 characters outside the verified inputs/objects address
        # retain their literal meaning.
        reader = identity.Identity({"native": str(left[0])})
        pin = left[2][0]
        data = reader.artifact(pin)["content_sha256"]
        unrelated = str(left[0] / "other" / pin["sha256"] / "play-import.json")
        normalized = reader.portable_path(unrelated, raw_sha256=pin["sha256"], content_sha256=data)
        self.assertIn(pin["sha256"], normalized["components"])

    def test_hash_only_learner_reference_resolves_retained_checkpoint(self):
        root, _, deps = self.chain("retained")
        pin = save(root / "learner.json", {"checkpoint_sha256": deps[2]["sha256"]})
        report = identity.Identity({"native": str(root)}, deps).report(pin)
        self.assertEqual(len(report["verified_artifacts"]), 4)
        changed = copy.deepcopy(deps)
        changed[2]["path"] += ".missing"
        with self.assertRaises(FileNotFoundError):
            identity.Identity({"native": str(root)}, changed).report(pin)

    def test_container_and_its_native_json_have_identical_document_identity(self):
        fixtures = Path(__file__).parent / "fixtures" / "checkpoint_container_v1"
        pins = [save(self.root / file, (fixtures / file).read_bytes())
                for file in ("checkpoint-v1.json", "checkpoint.ckpt")]
        reader = identity.Identity({"native": str(self.root)})
        self.assertEqual(reader.report(pins[0])["content_sha256"], reader.report(pins[1])["content_sha256"])

    def test_only_declared_root_boundary_and_attempt_components_are_normalized(self):
        reader = identity.Identity({"native": str(self.root)})
        one = reader.portable_path(str(self.root / "G1/b002-a1/episode-0000.json"))
        retry = reader.portable_path(str(self.root / "G1/b002-a7/episode-0000.json"))
        self.assertEqual(one, retry)
        for suffix in ("G2/b002-a1/episode-0000.json", "G1/b003-a1/episode-0000.json",
                       "G1/b002-a1/episode-0001.json"):
            self.assertNotEqual(one, reader.portable_path(str(self.root / suffix)))
        self.assertIn("absolute_or_external", reader.portable_path(str(self.root) + "-sibling/x"))
        with self.assertRaisesRegex(ValueError, "non-canonical"):
            reader.portable_path(str(self.root) + "/../elsewhere/x")

    def test_normalization_tags_and_json_numbers_cannot_collide(self):
        reader = identity.Identity({"native": str(self.root)})
        path = str(self.root / "x")
        values = [{"path": path}, {"path": reader.portable_path(path)},
                  {"path": ["path", reader.portable_path(path)]},
                  {"value": 0}, {"value": -0.0}, {"value": 0.0}, {"value": False}]
        hashes = [identity.digest(reader.normalize(value)) for value in values]
        self.assertEqual(len(set(hashes)), len(values))

    def test_duplicates_nonfinite_and_compressed_records_refuse(self):
        for index, data in enumerate((b'{"x":1,"x":2}', b'{"x":NaN}', b'{"x":Infinity}', b'{"x":1e400}',
                                      b'{"x":', b'\x28\xb5\x2f\xfd\0')):
            with self.subTest(data=data):
                pin = save(self.root / str(index), data)
                with self.assertRaises(ValueError):
                    identity.Identity({"native": str(self.root)}).report(pin)

    def test_cli_verifies_exact_request_and_leaves_artifacts_unchanged(self):
        root, pin, dependencies = self.chain("cli")
        request = {"schema": "mtg-kernel-native-artifact-identity-request/v1",
                   "roots": {"native": str(root)}, "artifact": pin, "dependencies": dependencies}
        manifest = self.root / "request.json"
        save(manifest, request)
        result = subprocess.run([sys.executable, identity.__file__, str(manifest)],
                                capture_output=True, text=True, check=True)
        self.assertEqual(json.loads(result.stdout), self.report((root, pin, dependencies)))


if __name__ == "__main__":
    unittest.main()
