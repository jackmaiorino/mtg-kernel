"""Recovery archive byte binding and complete readback, with tiny local files."""
import hashlib
from contextlib import contextmanager
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace
import zipfile
import zlib

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

    def test_mutation_during_stream_is_refused_with_and_without_prior_pins(self):
        for use_prior_pins in (False, True):
            with self.subTest(use_prior_pins=use_prior_pins), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                native, cold = root / "native", root / "cold"
                native.mkdir(); cold.mkdir()
                source = native / "checkpoint.json"
                source.write_bytes(b"a" * (2 * 1024 * 1024))
                verified = {str(source.resolve()): pin(source)["sha256"]} if use_prior_pins else None
                original_open = zipfile.ZipFile.open
                mutated = False

                @contextmanager
                def changing_destination(stream):
                    nonlocal mutated
                    def write(chunk):
                        nonlocal mutated
                        result = stream.write(chunk)
                        if not mutated:
                            mutated = True
                            with source.open("r+b") as changed:
                                changed.seek(len(chunk))
                                changed.write(b"b" * 1024)
                        return result
                    with stream:
                        yield SimpleNamespace(write=write)

                def intercept_open(archive, name, mode="r", *args, **kwargs):
                    stream = original_open(archive, name, mode, *args, **kwargs)
                    return changing_destination(stream) if mode == "w" else stream

                with patch.object(zipfile.ZipFile, "open", intercept_open):
                    with self.assertRaisesRegex(AssertionError, "source changed"):
                        archive_native(native, cold, verified)
                self.assertTrue(mutated)
                self.assertFalse((cold / "archive.json").exists())

    def test_full_readback_rejects_corrupted_archive_with_and_without_prior_pins(self):
        for use_prior_pins in (False, True):
            with self.subTest(use_prior_pins=use_prior_pins), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                native, cold = root / "native", root / "cold"
                native.mkdir(); cold.mkdir()
                source = native / "checkpoint.json"
                source.write_bytes(bytes(range(256)) * 16)
                verified = {str(source.resolve()): pin(source)["sha256"]} if use_prior_pins else None
                original_fsync = os.fsync
                def corrupt_after_fsync(descriptor):
                    original_fsync(descriptor)
                    archive_path = cold / "native-0.zip"
                    if archive_path.exists() and os.fstat(descriptor).st_ino == archive_path.stat().st_ino:
                        with archive_path.open("r+b") as stream:
                            stream.seek(30 + len(source.name))
                            first = stream.read(1)
                            stream.seek(-1, 1)
                            stream.write(bytes([first[0] ^ 0xFF]))
                with patch("public_training_storage_v1.os.fsync", corrupt_after_fsync):
                    with self.assertRaises((AssertionError, zipfile.BadZipFile, zlib.error)):
                        archive_native(native, cold, verified)
                self.assertFalse((cold / "archive.json").exists())


if __name__ == "__main__":
    unittest.main()
