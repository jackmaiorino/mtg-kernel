"""Recovery archive byte binding and complete readback, with tiny local files."""
import hashlib
from pathlib import Path
import tempfile
import unittest
import zipfile

from public_training_storage_v1 import archive_native
from public_training_dispatch_v2 import pin


class ArchiveTests(unittest.TestCase):
    def test_source_pins_and_complete_readback_agree(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            native, cold = root / "native", root / "cold"
            native.mkdir(); cold.mkdir()
            payloads = {"a.json": b'{"value":1}', "b.bin": bytes(range(256)) * 16}
            for name, payload in payloads.items():
                (native / name).write_bytes(payload)
            verified = {str(path.resolve()): pin(path)["sha256"] for path in native.iterdir()}
            result = archive_native(native, cold, verified)
            self.assertEqual(result["source_bytes"], sum(map(len, payloads.values())))
            self.assertEqual(result["mismatches"], 0)
            restored = {}
            for shard in result["shards"]:
                with zipfile.ZipFile(shard["archive"]["path"]) as archive:
                    for name, expected in shard["files"].items():
                        restored[name] = archive.read(name)
                        self.assertEqual(hashlib.sha256(restored[name]).hexdigest(), expected)
            self.assertEqual(restored, payloads)
            # Streamed write preserves the established ZIP representation too.
            files = sorted(native.iterdir())
            for index, shard in enumerate(result["shards"]):
                legacy = root / f"legacy-{index}.zip"
                with zipfile.ZipFile(legacy, "x", compression=zipfile.ZIP_DEFLATED,
                                     compresslevel=1, allowZip64=True) as archive:
                    for source in files[index::2]:
                        archive.write(source, source.name)
                self.assertEqual(Path(shard["archive"]["path"]).read_bytes(), legacy.read_bytes())

    def test_changed_prevalidated_source_refuses_archive_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            native, cold = root / "native", root / "cold"
            native.mkdir(); cold.mkdir()
            source = native / "checkpoint.json"
            source.write_bytes(b"old bytes")
            verified = {str(source.resolve()): pin(source)["sha256"]}
            source.write_bytes(b"new bytes")
            with self.assertRaisesRegex(AssertionError, "source changed"):
                archive_native(native, cold, verified)
            self.assertFalse((cold / "archive.json").exists())


if __name__ == "__main__":
    unittest.main()
