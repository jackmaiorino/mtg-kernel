from __future__ import annotations

import copy
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
import fdn_booster_pool_v1 as pool


class FoundationsBoosterTargetTests(unittest.TestCase):
    def setUp(self) -> None:
        self.document = json.loads(pool.TARGET.read_bytes())

    def test_complete_target_is_valid_and_historical_reference_is_preserved(self) -> None:
        pool.validate(self.document)
        historical = json.loads((pool.TARGET.parent / "card_names.json").read_bytes())
        self.assertEqual(self.document["card_names"], historical["card_names"])
        names = set(self.document["card_names"])
        self.assertTrue({"Grim Tutor", "Temporal Manipulation", "Goblin Bushwhacker", "Akroma's Memorial"}.issubset(names))
        self.assertNotIn("Sol Ring", names)
        self.assertNotIn("Adamant Will", names)

    def test_same_size_substitution_cannot_redefine_the_target(self) -> None:
        changed = copy.deepcopy(self.document)
        changed["card_names"][0] = "Adamant Will"
        changed["card_names"].sort()
        with self.assertRaisesRegex(ValueError, "membership changed"):
            pool.validate(changed)

    def test_missing_guest_and_nonbooster_printing_are_rejected(self) -> None:
        changed = copy.deepcopy(self.document)
        guest = next(card for card in changed["cards"] if card["set"] == "SPG")
        guest["collector_number"] = "84"
        with self.assertRaisesRegex(ValueError, "Special Guests"):
            pool.validate(changed)
        changed = copy.deepcopy(self.document)
        main = next(card for card in changed["cards"] if card["set"] == "FDN")
        main["collector_number"] = "488"
        with self.assertRaisesRegex(ValueError, "outside the booster target"):
            pool.validate(changed)

    def test_token_variants_remain_separate_and_dependencies_are_in_scope(self) -> None:
        dragons = [token for token in self.document["token_dependencies"] if token["name"] == "Dragon"]
        self.assertEqual(len(dragons), 2)
        changed = copy.deepcopy(self.document)
        changed["token_dependencies"][0]["required_by"] = ["Sol Ring"]
        with self.assertRaisesRegex(ValueError, "target cards"):
            pool.validate(changed)

    def test_report_never_claims_full_rules_support(self) -> None:
        result = pool.report()
        self.assertEqual(result["required_card_count"], 286)
        self.assertEqual(sum(result["status_counts"].values()), 286)
        self.assertIn("Registry admission only", result["coverage_meaning"])

    def test_same_size_printing_and_token_edge_mutations_are_rejected(self) -> None:
        changed = copy.deepcopy(self.document)
        changed["cards"][0]["scryfall_id"] = "not-a-real-printing"
        with self.assertRaisesRegex(ValueError, "printing identities or token dependencies"):
            pool.validate(changed)
        changed = copy.deepcopy(self.document)
        changed["token_dependencies"][0]["required_by"] = ["Abrade"]
        with self.assertRaisesRegex(ValueError, "printing identities or token dependencies"):
            pool.validate(changed)


if __name__ == "__main__":
    unittest.main()
