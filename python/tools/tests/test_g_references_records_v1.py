import gzip
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import g_references_records_v1 as records
import nine_deck_cloud_v1 as cloud

GZIP = (lambda path: gzip.open(path, "wb")), (lambda path: gzip.open(path, "rb"))  # CI's Python has no zstd


class LocalPod:
    """The follower's Pod, run here: its programs execute against a local 'volume' folder."""
    ssh: list = []

    def run(self, program, args, timeout=30, read_only=False):
        code = (cloud.PRELUDE % json.dumps(args) + program).encode()
        done = subprocess.run([sys.executable, "-B", "-"], input=code, capture_output=True, timeout=timeout)
        if done.returncode:
            raise cloud.PodUnavailable(done.stderr.decode()[-400:])
        return json.loads(done.stdout.decode().strip().splitlines()[-1])


def stream(argv, **options):  # the Pod's python3 is this interpreter
    return subprocess.Popen([sys.executable if item == "python3" else item for item in argv], **options)


class RecordsTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        base = Path(self.temp.name)
        self.volume, self.dest, self.control = base / "volume" / "campaign", base / "E" / "g-i-records", base / "control"
        self.control.mkdir(parents=True)
        self.state = self.volume / "state"
        files = {"retained/G3/b03/run.json": b"{}" * 10, "retained/G3/b03/iterations/000004/episode-0.json": b"e" * 999,
                 "hot/G3/b003-a1/dispatch/report.json": b"r", "exposure/G3-b003-episodes.jsonl": b"row\n" * 50,
                 "state/configs/G3-b003-a1.json": b"c"}
        listed = []
        for name, data in files.items():
            path = self.volume / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
            listed.append({"path": name, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})
        self.seal = {"schema": records.SEAL_SCHEMA, "run": "G3", "block": 3, "root": str(self.volume),
                     "files": listed, "bytes": sum(item["bytes"] for item in listed)}
        (self.state / "seals").mkdir(parents=True)
        (self.state / "seals" / "G3-b003.json").write_text(json.dumps(self.seal))

    def cycle(self, budget=10**9):
        return records.cycle(LocalPod(), str(self.state), self.dest, budget, self.control, stream=stream, codec=GZIP)

    def test_a_verified_archive_replaces_the_volume_copy(self):
        self.assertEqual(self.cycle(), 1)
        archive = self.dest / "G3" / "G3-b003.tar.zst"
        records.verify(archive, self.seal, GZIP[1])
        row = json.loads((self.dest / "MOVED.jsonl").read_text().splitlines()[0])
        self.assertEqual((row["files"], row["raw_bytes"], row["volume_root"]), (5, self.seal["bytes"], str(self.volume)))
        self.assertEqual(row["archive_sha256"], records.sha256_file(archive))
        marker = json.loads((self.state / "moved" / "G3-b003.json").read_text())
        self.assertEqual(marker["archive_sha256"], row["archive_sha256"])
        for item in self.seal["files"]:
            self.assertFalse((self.volume / item["path"]).exists())
        self.assertFalse((self.volume / "retained" / "G3" / "b03").exists())  # emptied folders go
        self.assertTrue((self.volume / "retained" / "G3").exists())  # the run's top folder stays
        self.assertEqual(self.cycle(), 0)  # nothing pending
        self.assertEqual(len((self.dest / "MOVED.jsonl").read_text().splitlines()), 1)

    def test_a_copy_that_differs_from_the_seal_deletes_nothing(self):
        (self.volume / "hot/G3/b003-a1/dispatch/report.json").write_bytes(b"changed")
        self.assertEqual(self.cycle(), 0)
        alert = json.loads((self.control / "ALERT-records.json").read_text())
        self.assertIn("archive members differ from the seal", alert["error"])
        self.assertTrue(all((self.volume / item["path"]).exists() for item in self.seal["files"]))
        self.assertFalse((self.state / "moved").exists())
        self.assertFalse((self.dest / "MOVED.jsonl").exists())

    def test_the_study_budget_on_e_refuses_an_archive(self):
        self.assertEqual(self.cycle(budget=10), 0)
        self.assertIn("record budget on E:", json.loads((self.control / "ALERT-records.json").read_text())["error"])
        self.assertFalse((self.dest / "G3" / "G3-b003.tar.zst").exists())
        self.assertFalse((self.dest / "G3" / "G3-b003.tar.zst.partial").exists())
        self.assertTrue(all((self.volume / item["path"]).exists() for item in self.seal["files"]))

    def test_a_seal_changed_after_its_copy_is_not_finalized(self):
        pod = LocalPod()
        original = pod.run

        def run(program, args, **options):
            if program is records.FINALIZE:  # the seal changed between the copy and the deletion
                (self.state / "seals" / "G3-b003.json").write_text(json.dumps({**self.seal, "bytes": 0}))
            return original(program, args, **options)
        pod.run = run
        with self.assertRaises(cloud.PodUnavailable):
            records.move(pod, str(self.state), "G3-b003.json", self.dest, 10**9, stream=stream, codec=GZIP)
        self.assertTrue(all((self.volume / item["path"]).exists() for item in self.seal["files"]))
        self.assertFalse((self.state / "moved" / "G3-b003.json").exists())

    @unittest.skipUnless(sys.version_info >= (3, 14), "compression.zstd needs Python 3.14 (Jack's PC)")
    def test_the_zstd_codec_round_trips(self):
        self.assertEqual(records.cycle(LocalPod(), str(self.state), self.dest, 10**9, self.control, stream=stream), 1)
        records.verify(self.dest / "G3" / "G3-b003.tar.zst", self.seal, records.zstd_codec()[1])


if __name__ == "__main__":
    unittest.main()
