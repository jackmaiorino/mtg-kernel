import hashlib
import json
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / "python/tools/validate_search_campaign_manifest_v1.py"
MANIFEST = ROOT / "sideboard_search_campaign_manifest_v1.json"


class ValidateSearchCampaignManifest(unittest.TestCase):
    def test_write_hash_then_check_passes(self):
        result = subprocess.run(
            [sys.executable, str(TOOL), "--write-hash", str(MANIFEST)],
            cwd=ROOT, capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        hash_path = MANIFEST.with_suffix(".json.sha256")
        self.assertTrue(hash_path.exists())
        raw = MANIFEST.read_bytes()
        expected = hashlib.sha256(raw).hexdigest()
        self.assertEqual(hash_path.read_text(encoding="utf-8").strip(), expected)

        check = subprocess.run(
            [sys.executable, str(TOOL), "--check", str(MANIFEST)],
            cwd=ROOT, capture_output=True, text=True,
        )
        self.assertEqual(check.returncode, 0, check.stdout + check.stderr)

    def test_check_fails_on_a_tampered_manifest(self):
        subprocess.run([sys.executable, str(TOOL), "--write-hash", str(MANIFEST)], cwd=ROOT, check=True)
        original = MANIFEST.read_text(encoding="utf-8")
        document = json.loads(original)
        document["k_max_candidates_per_cell"] += 1
        MANIFEST.write_text(json.dumps(document, indent=2), encoding="utf-8")
        try:
            check = subprocess.run(
                [sys.executable, str(TOOL), "--check", str(MANIFEST)],
                cwd=ROOT, capture_output=True, text=True,
            )
            self.assertNotEqual(check.returncode, 0)
        finally:
            MANIFEST.write_text(original, encoding="utf-8")
            subprocess.run([sys.executable, str(TOOL), "--write-hash", str(MANIFEST)], cwd=ROOT, check=True)


if __name__ == "__main__":
    unittest.main()
