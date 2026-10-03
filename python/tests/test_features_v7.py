"""V4/V7 analog of test_features_v6.py's descriptor-and-identity regression.

Mirrors FeaturesV6Tests.test_descriptor_and_rust_identity_match_actual_source
for the Phase 1 fresh-lineage generation: data/flat_policy_v4/*, generated
from python/mtg_kernel_rl/features_v7.py. This does not re-test the whole V6
suite; it only enforces that the V4 contract files were regenerated together
with features_v7.py and never hand-edited out of step with it, the same way
test_features_v6.py:589 does for V3.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import unittest

from mtg_kernel_rl import features_v7 as v7


class FeaturesV7Tests(unittest.TestCase):
    def test_descriptor_and_rust_identity_match_actual_source(self):
        root = Path(__file__).resolve().parents[2]
        raw = (root / "data/flat_policy_v4/feature_contract_v4.json").read_bytes()
        descriptor = json.loads(raw)
        identity = (root / "data/flat_policy_v4/feature_identity.rs").read_text()
        self.assertEqual(descriptor["features_source_sha256"], hashlib.sha256(Path(v7.__file__).read_bytes()).hexdigest())
        self.assertEqual(descriptor["feature_contract_digest"], v7.feature_contract_fingerprint())
        self.assertEqual(descriptor["feature_encoding_digest"], v7.encoding_contract_fingerprint())
        self.assertIn(hashlib.sha256(raw).hexdigest(), identity)


if __name__ == "__main__":
    unittest.main()
