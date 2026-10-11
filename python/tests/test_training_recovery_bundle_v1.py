import hashlib
import gc
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile


SOURCE = Path(__file__).resolve().parents[1] / "tools/training_recovery_bundle_v1.py"
SPEC = importlib.util.spec_from_file_location("recovery_bundle", SOURCE)
BUNDLE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUNDLE)


class RecoveryBundleTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name).resolve()
        self.source = self.root / "source"
        self.source.mkdir()
        self.files = []
        for name, content in (("controllers/case.json", b'{"case":1}\n'),
                              ("maintenance/nested/ledger.bin", bytes(range(256)) * 9),
                              ("maintenance/empty", b""),
                              ("maintenance/same-bytes", b'{"case":1}\n')):
            path = self.source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)
            self.files.append(path)
        self.expected = sorted((dict(relative_path=path.relative_to(self.source).as_posix(),
                                     bytes=path.stat().st_size,
                                     sha256=hashlib.sha256(path.read_bytes()).hexdigest())
                                for path in self.files), key=lambda item: item["relative_path"])
        self.destination = self.root / "cold/late.zip"

    def create(self, **kwargs):
        return BUNDLE.create_bundle(self.source, self.files, self.destination, **kwargs)

    def rewrite(self, transform):
        with zipfile.ZipFile(self.destination) as original:
            members = [(info, original.read(info)) for info in original.infolist()]
        rewritten = self.root / "rewritten.zip"
        with zipfile.ZipFile(rewritten, "w") as archive:
            for info, content in members:
                replacement = transform(info, content)
                if replacement is not None:
                    archive.writestr(info, replacement)
        return rewritten

    def test_full_byte_inventory_and_extraction_without_sources(self):
        receipt = self.create(expected_inventory=self.expected)
        self.assertTrue(receipt["complete"])
        self.assertEqual(receipt["inventory"], self.expected)
        self.assertEqual(receipt["file_count"], 4)
        self.assertEqual(receipt["bundle"]["bytes"], self.destination.stat().st_size)
        self.assertEqual(receipt["bundle"]["sha256"], hashlib.sha256(self.destination.read_bytes()).hexdigest())
        self.assertEqual(set(receipt["timing"]), {"inventory_seconds", "stream_seconds", "source_verify_seconds", "fsync_seconds",
                                                 "publish_seconds", "verify_seconds"})
        with zipfile.ZipFile(self.destination) as archive:
            self.assertEqual(len(archive.infolist()), len(self.files) + 1)
            self.assertTrue(all(info.compress_type == zipfile.ZIP_STORED for info in archive.infolist()))
            for item in self.expected:
                self.assertEqual(archive.read("payload/" + item["relative_path"]),
                                 (self.source / item["relative_path"]).read_bytes())
        for path in self.files:
            path.unlink()
        proof = BUNDLE.extract_bundle(self.destination, self.root / "restored", self.expected,
                                      expected_bundle_sha256=receipt["bundle"]["sha256"])
        self.assertTrue(proof["complete"] and proof["independent_extracted_readback"])
        self.assertEqual(proof["inventory"], self.expected)

    def test_archive_is_identical_for_reversed_inventory_order(self):
        first = self.create()
        other = self.root / "cold/other.zip"
        second = BUNDLE.create_bundle(self.source, reversed(self.files), other)
        self.assertEqual(first["bundle"]["sha256"], second["bundle"]["sha256"])

    def test_existing_destination_and_failed_attempt_are_preserved(self):
        self.destination.parent.mkdir()
        self.destination.write_bytes(b"previous evidence")
        with self.assertRaisesRegex(ValueError, "preserve prior"):
            self.create()
        self.assertEqual(self.destination.read_bytes(), b"previous evidence")
        other = self.root / "cold/failed.zip"
        other.with_name(other.name + ".partial").write_bytes(b"failure evidence")
        with self.assertRaisesRegex(ValueError, "preserve prior"):
            BUNDLE.create_bundle(self.source, self.files, other)

    def test_wrong_expected_source_sha_retains_partial_without_publication(self):
        expected = [dict(item) for item in self.expected]
        expected[0]["sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "source bytes differ"):
            self.create(expected_inventory=expected)
        self.assertFalse(self.destination.exists())
        self.assertTrue(self.destination.with_name(self.destination.name + ".partial").exists())

    def test_missing_extra_and_duplicate_source_inventory_refuse(self):
        with self.assertRaisesRegex(ValueError, "expected inventory differs"):
            BUNDLE.create_bundle(self.source, self.files[:-1], self.destination,
                                 expected_inventory=self.expected)
        with self.assertRaisesRegex(ValueError, "duplicate recovery source"):
            BUNDLE.create_bundle(self.source, self.files + [self.files[0]], self.destination)
        missing = self.source / "missing"
        with self.assertRaises(FileNotFoundError):
            BUNDLE.create_bundle(self.source, [missing], self.destination)
        self.assertFalse(self.destination.exists())

    def test_outside_root_relative_and_traversal_sources_refuse(self):
        outside = self.root / "outside"
        outside.write_bytes(b"private")
        for path in (outside, Path("relative"), self.source / ".." / "outside"):
            with self.subTest(path=path), self.assertRaises(ValueError):
                BUNDLE.create_bundle(self.source, [path], self.destination)

    def test_alternate_stream_and_ambiguous_destination_names_refuse(self):
        for name in ("late.zip:stream", "late.zip.", "CON.zip"):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "unsafe relative path"):
                BUNDLE.create_bundle(self.source, self.files, self.root / name)

    def test_source_changed_after_streaming_refuses_publication(self):
        original_info = BUNDLE._zip_info
        def mutate(name):
            if name.endswith("maintenance/empty"):
                self.files[0].write_bytes(b"changed after streaming")
            return original_info(name)
        with patch.object(BUNDLE, "_zip_info", side_effect=mutate):
            with self.assertRaisesRegex(ValueError, "source mutated before publication"):
                self.create()
        self.assertFalse(self.destination.exists())

    def test_mutation_during_source_read_refuses_publication(self):
        original_info = BUNDLE._zip_info
        def mutate(name):
            if name.endswith("controllers/case.json"):
                self.files[0].write_bytes(b"changed during open handle")
            return original_info(name)
        with patch.object(BUNDLE, "_zip_info", side_effect=mutate):
            with self.assertRaisesRegex(ValueError, "source mutated during read"):
                self.create()
        self.assertFalse(self.destination.exists())

    def test_after_stream_same_size_rewrite_with_restored_mtime_refuses_publication(self):
        source = self.files[0]
        original_metadata = source.stat()
        original_info = BUNDLE._zip_info
        def mutate(name):
            if name == BUNDLE.MANIFEST:
                source.write_bytes(b'{"case":2}\n')
                os.utime(source, ns=(original_metadata.st_atime_ns, original_metadata.st_mtime_ns))
            return original_info(name)
        with patch.object(BUNDLE, "_zip_info", side_effect=mutate):
            with self.assertRaisesRegex(ValueError, "source (SHA changed|mutated) before publication"):
                self.create(expected_inventory=self.expected)
        self.assertFalse(self.destination.exists())
        self.assertTrue(self.destination.with_name(self.destination.name + ".partial").exists())

    def test_failure_cleanup_has_no_unraisable_closed_output_flush(self):
        errors = []
        with patch.object(sys, "unraisablehook", side_effect=errors.append):
            with patch.object(BUNDLE.os, "fsync", side_effect=OSError("fsync failed")):
                with self.assertRaises(OSError):
                    self.create()
            gc.collect()
        self.assertEqual(errors, [])

    def test_hard_link_sources_refuse(self):
        link = self.source / "hard-link"
        os.link(self.files[0], link)
        with self.assertRaisesRegex(ValueError, "regular unlinked file"):
            self.create()

    def test_symlink_parent_and_destination_refuse(self):
        alias = self.root / "alias"
        try:
            alias.symlink_to(self.source, target_is_directory=True)
        except OSError as error:
            self.skipTest("host cannot create symbolic links: " + str(error))
        with self.assertRaisesRegex(ValueError, "link/reparse"):
            BUNDLE.create_bundle(alias, [alias / "controllers/case.json"], self.destination)
        with self.assertRaisesRegex(ValueError, "link/reparse"):
            BUNDLE.create_bundle(self.source, self.files, alias / "late.zip")

    @unittest.skipUnless(os.name == "nt", "Windows directory junction")
    def test_windows_junction_parent_and_destination_refuse(self):
        alias = self.root / "junction"
        subprocess.run(["cmd", "/d", "/c", "mklink", "/J", str(alias), str(self.source)],
                       check=True, capture_output=True, text=True)
        try:
            with self.assertRaisesRegex(ValueError, "link/reparse"):
                BUNDLE.create_bundle(alias, [alias / "controllers/case.json"], self.destination)
            with self.assertRaisesRegex(ValueError, "link/reparse"):
                BUNDLE.create_bundle(self.source, self.files, alias / "late.zip")
        finally:
            alias.rmdir()  # Remove only the junction; its target stays intact.
        self.assertTrue(self.files[0].is_file())

    def test_member_sha_mismatch_refuses_even_when_zip_crc_is_valid(self):
        self.create()
        changed = self.rewrite(lambda info, content: b"x" * len(content)
                              if info.filename == "payload/controllers/case.json" else content)
        with self.assertRaisesRegex(ValueError, "member SHA/size differs"):
            BUNDLE.verify_bundle(changed, self.expected)

    def test_manifest_and_archive_pins_are_external(self):
        receipt = self.create()
        changed = self.rewrite(lambda info, content: content + b" "
                              if info.filename == BUNDLE.MANIFEST else content)
        with self.assertRaisesRegex(ValueError, "bundle SHA differs"):
            BUNDLE.verify_bundle(changed, self.expected,
                                 expected_bundle_sha256=receipt["bundle"]["sha256"])
        altered = [dict(item) for item in self.expected]
        altered[0]["sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "manifest differs"):
            BUNDLE.verify_bundle(self.destination, altered)

    def test_missing_extra_duplicate_and_link_archive_members_refuse(self):
        self.create()
        missing = self.rewrite(lambda info, content: None if info.filename.startswith("payload/controllers") else content)
        with self.assertRaisesRegex(ValueError, "ZIP inventory differs"):
            BUNDLE.verify_bundle(missing, self.expected)
        with zipfile.ZipFile(missing, "a") as archive:
            archive.writestr("payload/../escape", b"extra")
        with self.assertRaisesRegex(ValueError, "ZIP inventory differs"):
            BUNDLE.verify_bundle(missing, self.expected)
        changed = self.rewrite(lambda info, content: content)
        with zipfile.ZipFile(changed, "a") as archive:
            archive.writestr(BUNDLE.MANIFEST, json.dumps({"schema": BUNDLE.SCHEMA, "files": self.expected}))
        with self.assertRaisesRegex(ValueError, "ZIP inventory differs"):
            BUNDLE.verify_bundle(changed, self.expected)
        def make_link(info, content):
            if info.filename.startswith("payload/controllers"):
                info.external_attr = 0o120777 << 16
            return content
        linked = self.rewrite(make_link)
        with self.assertRaisesRegex(ValueError, "link recovery member"):
            BUNDLE.verify_bundle(linked, self.expected)

    def test_unsafe_expected_names_and_collisions_refuse(self):
        for name in ("../escape", "/absolute", "C:/absolute", "a\\b", "a//b", ".", "a.", "NUL", "a:stream"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                BUNDLE.verify_bundle(self.destination, [dict(relative_path=name, bytes=0, sha256="0" * 64)])
        for names in (("a", "A"), ("a", "a/b")):
            inventory = [dict(relative_path=name, bytes=0, sha256="0" * 64) for name in names]
            with self.assertRaises(ValueError):
                BUNDLE.verify_bundle(self.destination, inventory)

    def test_fsync_and_publication_failure_do_not_return_success(self):
        with patch.object(BUNDLE.os, "fsync", side_effect=OSError("disk fsync failed")):
            with self.assertRaisesRegex(OSError, "fsync failed"):
                self.create()
        self.assertFalse(self.destination.exists())
        other = self.root / "other.zip"
        with patch.object(BUNDLE, "_publish", side_effect=OSError("publication failed")):
            with self.assertRaisesRegex(OSError, "publication failed"):
                BUNDLE.create_bundle(self.source, self.files, other)
        self.assertFalse(other.exists())
        self.assertTrue(other.with_name(other.name + ".partial").exists())

    def test_destination_verification_failure_preserves_archive(self):
        with patch.object(BUNDLE, "verify_bundle", side_effect=ValueError("readback failed")):
            with self.assertRaisesRegex(ValueError, "readback failed"):
                self.create()
        self.assertTrue(self.destination.exists())

    def test_publication_race_preserves_other_writer_and_partial_bundle(self):
        publish = BUNDLE._publish
        def race(temporary, destination):
            destination.write_bytes(b"other writer's evidence")
            publish(temporary, destination)
        with patch.object(BUNDLE, "_publish", side_effect=race):
            with self.assertRaises(OSError):
                self.create()
        self.assertEqual(self.destination.read_bytes(), b"other writer's evidence")
        self.assertTrue(self.destination.with_name(self.destination.name + ".partial").exists())

    def test_extract_refuses_existing_or_unsafe_destination_before_writes(self):
        self.create()
        with self.assertRaisesRegex(ValueError, "fresh recovery extraction"):
            BUNDLE.extract_bundle(self.destination, self.source, self.expected)
        altered = [dict(item) for item in self.expected]
        altered[0]["sha256"] = "0" * 64
        target = self.root / "failed-restore"
        with self.assertRaises(ValueError):
            BUNDLE.extract_bundle(self.destination, target, altered)
        self.assertFalse(target.exists())

    def test_storage_reserve_refuses_before_payload(self):
        usage = type("Usage", (), {"free": 1})()
        with patch.object(BUNDLE.shutil, "disk_usage", return_value=usage):
            with self.assertRaisesRegex(ValueError, "storage reserve"):
                self.create(reserve_bytes=1024)
        self.assertFalse(self.destination.exists())
        self.assertFalse(self.destination.with_name(self.destination.name + ".partial").exists())


if __name__ == "__main__":
    unittest.main()
