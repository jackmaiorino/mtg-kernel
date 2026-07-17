from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock


TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import fetch_corpora  # noqa: E402


def _file_lock(path: Path) -> dict[str, object]:
    payload = path.read_bytes()
    return {"size": len(payload), "sha256": hashlib.sha256(payload).hexdigest()}


def _fixture(root: Path, *, extra_entry: str | None = None) -> tuple[Path, dict, dict]:
    corpus = root / "fixture"
    (corpus / "nested").mkdir(parents=True)
    (corpus / "manifest.json").write_text(
        json.dumps({"corpus": "fixture-v1", "status": "LOCKED"}, separators=(",", ":")),
        encoding="utf-8",
    )
    (corpus / "game_alpha.txt").write_bytes(b"alpha\n")
    (corpus / "nested" / "game_beta.txt").write_bytes(b"beta\n")
    traces = []
    aggregate = bytearray()
    for relative in ("game_alpha.txt", "nested/game_beta.txt"):
        record = {"path": relative, **_file_lock(corpus / Path(relative))}
        traces.append(record)
        aggregate.extend(
            b"trace\0"
            + relative.encode()
            + b"\0"
            + str(record["size"]).encode()
            + b"\0"
            + str(record["sha256"]).encode()
            + b"\n"
        )
    manifest = _file_lock(corpus / "manifest.json")
    aggregate.extend(
        b"manifest\0manifest.json\0"
        + str(manifest["size"]).encode()
        + b"\0"
        + str(manifest["sha256"]).encode()
        + b"\n"
    )
    lock = {
        "directory": "fixture",
        "manifest_corpus": "fixture-v1",
        "required_status": "LOCKED",
        "manifest": manifest,
        "traces": traces,
        "aggregate_sha256": hashlib.sha256(aggregate).hexdigest(),
    }
    archive = root / "fixture.zip"
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as output:
        for path in sorted(corpus.rglob("*")):
            if path.is_file():
                output.write(path, path.relative_to(root).as_posix())
        if extra_entry is not None:
            output.writestr(extra_entry, b"unexpected")
    archive_lock = _file_lock(archive)
    spec = {
        "directory": "fixture",
        "asset": archive.name,
        **archive_lock,
        "content_lock_aggregate_sha256": lock["aggregate_sha256"],
    }
    return archive, spec, lock


class CorpusFetchTest(unittest.TestCase):
    def test_verified_archive_installs_atomically_and_rechecks_existing(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, spec, lock = _fixture(root)
            destination = root / "installed"
            installed = fetch_corpora.install_archive(archive, destination, spec, lock)
            self.assertEqual((installed / "game_alpha.txt").read_bytes(), b"alpha\n")
            self.assertEqual(
                fetch_corpora.install_archive(archive, destination, spec, lock), installed
            )

    def test_existing_target_does_not_bypass_archive_catalog_validation(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, spec, lock = _fixture(root)
            destination = root / "installed"
            fetch_corpora.install_archive(archive, destination, spec, lock)
            malformed = {**spec, "size": -1}
            with self.assertRaisesRegex(fetch_corpora.CorpusFetchError, "positive integer"):
                fetch_corpora.install_archive(archive, destination, malformed, lock)
            catalog = {
                "schema_version": 1,
                "repository": "fixture/repository",
                "release_tag": "fixture-v1",
                "archives": [malformed],
            }
            with mock.patch.object(
                fetch_corpora, "_catalogs", return_value=(catalog, {"fixture": lock})
            ), self.assertRaisesRegex(fetch_corpora.CorpusFetchError, "positive integer"):
                fetch_corpora.main(["--destination", str(destination)])
            for dangerous in (
                ".",
                "..",
                "...",
                "foo.",
                "foo ",
                " ",
                ".. ",
                "CON",
                "nul.txt",
                "bad?name",
            ):
                with self.subTest(directory=dangerous), self.assertRaisesRegex(
                    fetch_corpora.CorpusFetchError, "safe path component"
                ):
                    fetch_corpora._validate_archive_spec(
                        {**spec, "directory": dangerous},
                        {**lock, "directory": dangerous},
                    )

    def test_duplicate_catalog_identities_fail_before_destination_mutation(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _archive, spec, lock = _fixture(root)
            destination = root / "not-created"
            catalog = {
                "schema_version": 1,
                "repository": "fixture/repository",
                "release_tag": "fixture-v1",
                "archives": [spec, {**spec, "asset": "second.zip"}],
            }
            with mock.patch.object(
                fetch_corpora, "_catalogs", return_value=(catalog, {"fixture": lock})
            ), self.assertRaisesRegex(fetch_corpora.CorpusFetchError, "duplicate archive"):
                fetch_corpora.main(["--destination", str(destination)])
            self.assertFalse(destination.exists())

            content_locks = {"schema_version": 1, "corpora": [lock, dict(lock)]}
            with mock.patch.object(
                fetch_corpora,
                "_load_json",
                side_effect=[
                    {**catalog, "archives": [spec]},
                    content_locks,
                ],
            ), self.assertRaisesRegex(fetch_corpora.CorpusFetchError, "duplicate content-lock"):
                fetch_corpora._catalogs()

    def test_archive_byte_mutation_fails_before_extraction(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, spec, lock = _fixture(root)
            archive.write_bytes(archive.read_bytes() + b"mutation")
            with self.assertRaisesRegex(fetch_corpora.CorpusFetchError, "archive byte lock"):
                fetch_corpora.install_archive(archive, root / "installed", spec, lock)

    def test_unexpected_or_traversing_entries_fail_closed(self) -> None:
        for entry in ("fixture/unexpected.txt", "fixture/../escape.txt"):
            with self.subTest(entry=entry), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                archive, spec, lock = _fixture(root, extra_entry=entry)
                with self.assertRaises(fetch_corpora.CorpusFetchError):
                    fetch_corpora.install_archive(archive, root / "installed", spec, lock)

    def test_linked_corpus_root_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _archive, spec, lock = _fixture(root)
            linked = root / "linked"
            try:
                linked.symlink_to(root / "fixture", target_is_directory=True)
            except OSError as exc:
                self.skipTest(f"directory symlinks are unavailable: {exc}")
            with self.assertRaisesRegex(fetch_corpora.CorpusFetchError, "symlink"):
                fetch_corpora.verify_corpus(linked, lock)
            destination_link = root / "destination-link"
            destination_link.symlink_to(root, target_is_directory=True)
            catalog = {
                "schema_version": 1,
                "repository": "fixture/repository",
                "release_tag": "fixture-v1",
                "archives": [spec],
            }
            with mock.patch.object(
                fetch_corpora, "_catalogs", return_value=(catalog, {"fixture": lock})
            ), self.assertRaisesRegex(fetch_corpora.CorpusFetchError, "destination"):
                fetch_corpora.main(["--destination", str(destination_link)])

    @unittest.skipUnless(os.name == "nt", "Windows junction regression")
    def test_windows_junction_corpus_root_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _archive, spec, lock = _fixture(root)
            junction = root / "junction"
            subprocess.check_call(
                ["cmd", "/c", "mklink", "/J", str(junction), str(root / "fixture")],
                stdout=subprocess.DEVNULL,
            )
            with self.assertRaisesRegex(fetch_corpora.CorpusFetchError, "symlink"):
                fetch_corpora.verify_corpus(junction, lock)
            destination_junction = root / "destination-junction"
            subprocess.check_call(
                ["cmd", "/c", "mklink", "/J", str(destination_junction), str(root)],
                stdout=subprocess.DEVNULL,
            )
            catalog = {
                "schema_version": 1,
                "repository": "fixture/repository",
                "release_tag": "fixture-v1",
                "archives": [spec],
            }
            with mock.patch.object(
                fetch_corpora, "_catalogs", return_value=(catalog, {"fixture": lock})
            ), self.assertRaisesRegex(fetch_corpora.CorpusFetchError, "destination"):
                fetch_corpora.main(["--destination", str(destination_junction)])


if __name__ == "__main__":
    unittest.main()
