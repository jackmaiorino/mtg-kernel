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

    def test_an_interrupted_deletion_stays_pending_and_is_retried(self):
        pod = LocalPod()
        original = pod.run
        failing = {"on": True}

        def run(program, args, **options):
            if program is records.FINALIZE and failing["on"]:  # a file the deletion cannot remove
                program = program.replace("os.unlink(target)", "None if target.endswith('report.json') else os.unlink(target)")
            return original(program, args, **options)
        pod.run = run
        self.assertEqual(records.cycle(pod, str(self.state), self.dest, 10**9, self.control, stream=stream,
                                       codec=GZIP), 0)
        marker = json.loads((self.state / "moved" / "G3-b003.json").read_text())
        self.assertIs(marker["deleted"], False)  # copied and verified, not yet gone
        self.assertIn("volume copy not gone", json.loads((self.control / "ALERT-records.json").read_text())["error"])
        self.assertEqual(LocalPod().run(records.LIST, {"state": str(self.state)})["pending"], ["G3-b003.json"])
        failing["on"] = False
        self.assertEqual(records.cycle(pod, str(self.state), self.dest, 10**9, self.control, stream=stream,
                                       codec=GZIP), 1)
        self.assertIs(json.loads((self.state / "moved" / "G3-b003.json").read_text())["deleted"], True)
        self.assertTrue((self.dest / "G3" / "G3-b003.seal.json").exists())
        self.assertEqual(len((self.dest / "MOVED.jsonl").read_text().splitlines()), 1)

    def test_a_present_archive_still_gets_its_seal_before_deletion(self):
        self.cycle()
        (self.dest / "G3" / "G3-b003.seal.json").unlink()
        (self.state / "moved" / "G3-b003.json").write_text(json.dumps({"deleted": False}))  # stopped mid-way
        self.assertEqual(LocalPod().run(records.LIST, {"state": str(self.state)})["pending"], ["G3-b003.json"])
        self.assertEqual(self.cycle(), 1)  # the archive is reused, its seal written again, the deletion finished
        self.assertEqual(json.loads((self.dest / "G3" / "G3-b003.seal.json").read_text()), self.seal)
        self.assertIs(json.loads((self.state / "moved" / "G3-b003.json").read_text())["deleted"], True)

    def test_a_quiesce_request_stops_moving_and_is_acknowledged(self):
        (self.control / "records-quiesce.json").write_text("{}")
        self.assertEqual(self.cycle(), 0)
        self.assertTrue((self.control / "records-quiesced.json").exists())
        self.assertTrue(all((self.volume / item["path"]).exists() for item in self.seal["files"]))
        self.assertEqual(json.loads((self.control / "records-backlog.json").read_text())["pending"], ["G3-b003.json"])

    @unittest.skipUnless(sys.version_info >= (3, 14), "compression.zstd needs Python 3.14 (Jack's PC)")
    def test_the_zstd_codec_round_trips(self):
        self.assertEqual(records.cycle(LocalPod(), str(self.state), self.dest, 10**9, self.control, stream=stream), 1)
        records.verify(self.dest / "G3" / "G3-b003.tar.zst", self.seal, records.zstd_codec()[1])


if __name__ == "__main__":
    unittest.main()
