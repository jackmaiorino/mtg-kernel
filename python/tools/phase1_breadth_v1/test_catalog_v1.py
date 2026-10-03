"""Small offline catalog contracts; synthetic reports are never runtime proof.

Run from this directory with unittest. No engine, model, network or subprocess is
used. The positive campaign case is an explicitly synthetic future input shape,
not a qualification of any actual MTGO registration or the pending snapshot004.
"""
from collections import Counter
import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

import catalog_v1 as catalog


RUNTIME = "b" * 64
SPLIT_SEED = "phase1-meta-grouped-split-2026091301"


class Fixture:
    def __init__(self, root):
        self.root = Path(root)
        self.serial = 0
        self.registry = {"cards": [
            {"name": name, "engine_capability": "full", "is_token": False,
             "supertypes": ["Basic"]}
            for name in ("Basic A", "Basic B", "Basic C", "Basic D")
        ] + [{"name": "Spell", "engine_capability": "full", "is_token": False,
              "supertypes": []}]}

    def pin_bytes(self, raw, name="input.json"):
        self.serial += 1
        directory = self.root / str(self.serial)
        directory.mkdir()
        path = directory / name
        path.write_bytes(raw)
        return {"path": str(path.resolve()), "sha256": hashlib.sha256(raw).hexdigest(),
                "bytes": len(raw)}

    def pin(self, value, name="input.json"):
        return self.pin_bytes(json.dumps(value, sort_keys=True, allow_nan=False).encode(), name)

    @staticmethod
    def row(archetype, player, index=0, split="train"):
        left, right = ("Basic A", "Basic B") if archetype == "A" else ("Basic C", "Basic D")
        main = {left: 60 - index}
        if index:
            main[right] = index
        zones = {"mainboard": main, "sideboard": {right: 15}}
        return {"event_id": "99", "player_id": str(player), "deck_id": str(1000 + player),
                "registration_id": "99:" + str(player), "archetype": archetype,
                "archetype_provenance": "synthetic unit fixture, not a source classification",
                "split": split, "list_sha256": catalog.digest(zones),
                "mainboard_size": 60, "sideboard_size": 15,
                "size_legal_envelope": True, "current_exact_size_supported": True,
                "registry_supported_candidate": True, "qualified_executable": False,
                "qualification": None, "support_blockers": [], **zones}

    @staticmethod
    def zones(row):
        return {zone: copy.deepcopy(row[zone]) for zone in ("mainboard", "sideboard")}

    def spec(self, rows, *, scope="engineering", choices=None, reserved=None):
        return {"schema": "phase1-breadth-catalog-source/v1", "scope": scope,
                "registry": self.pin(self.registry), "qualification_runtime_sha256": RUNTIME,
                "data": {"kind": "engineering", "registrations": self.pin(rows),
                         "provenance": "explicit synthetic engineering fixture, no field claim"},
                "roster": ["A", "B"],
                "postboards": self.pin({"schema": "phase1-postboard-configurations/v1",
                                         "configurations": choices or {}}),
                "prior_reserved_groups": self.pin({"schema": "phase1-reserved-list-groups/v1",
                                                    "groups": reserved or []})}

    def engineering(self):
        rows = [self.row("A", 1), self.row("A", 2), self.row("B", 3)]
        post = {"mainboard": {"Basic A": 59, "Basic B": 1},
                "sideboard": {"Basic A": 1, "Basic B": 14}}
        return rows, self.spec(rows, choices={rows[0]["list_sha256"]: [post]})

    def snapshot(self, *, entrants=20, scope="campaign", mutate=None):
        """Future schema only: twenty synthetic lists and synthetic reports."""
        rows = [self.row(a, offset + i + 1, i) for a, offset in (("A", 0), ("B", 10))
                for i in range(10)]
        spec = self.spec(rows, scope=scope)
        splits = []
        for archetype in ("A", "B"):
            ordered = sorted((row for row in rows if row["archetype"] == archetype),
                             key=lambda row: catalog.digest([SPLIT_SEED, archetype, row["list_sha256"]]))
            for position, row in enumerate(ordered):
                row["split"] = "train" if position < 8 else "development" if position == 8 else "confirmation"
            splits.append({"archetype": archetype, "unique_list_groups": 10,
                           "train_groups": 8, "development_groups": 1,
                           "confirmation_groups": 1, "sparse_all_training": False})
        for row in rows:
            identity = {"list_sha256": row["list_sha256"],
                        "registry_sha256": spec["registry"]["sha256"], "runtime_sha256": RUNTIME}
            report = {"schema": "phase1-registration-qualification/v1", "complete": True,
                      "qualification_kind": "complete-bo3-registration", **identity,
                      "test_fixture_only": True, "actual_runtime_execution": False}
            row["qualification"] = {**identity, "evidence": self.pin(report)}
            row["qualified_executable"] = True
        missing = entrants - len(rows)
        event = {"event_id": "99", "name": "Synthetic Pauper Challenge", "date": "2026-07-15",
                 "official_starttime": "2026-07-15 12:00:00", "source_timezone": None,
                 "provenance": {"source_kind": "synthetic_offline_fixture"},
                 "entrant_count": entrants, "queued_player_count": None,
                 "published_registration_count": len(rows), "missing_registration_count": missing,
                 "publication_status": "complete_lists" if missing == 0 else "partial_lists",
                 "registrations": copy.deepcopy(rows)}
        inventory = [{"event_id": "99", "index_date": "2026-07-15", "status": "normalized",
                      "provenance": {"source_kind": "synthetic_offline_fixture"}}]
        summary = {"schema": "phase1-pauper-meta-snapshot/v1",
                   "status": "ready_for_frozen_design_review" if missing <= 2 else "pending",
                   "registry": spec["registry"], "inventory_completeness_verified": True,
                   "indexed_events": 1, "ingested_events": 1, "missing_event_ids": [],
                   "unknown_denominator_event_ids": [], "official_entrants_in_known_events": entrants,
                   "complete_inventory_denominator": entrants, "published_registrations": len(rows),
                   "known_missing_registrations": missing, "registrations_with_unknown_archetype": 0,
                   "unknown_field_registration_count": missing, "unknown_field_weight": missing / entrants,
                   "registry_supported_candidate_registrations": len(rows),
                   "qualified_executable_registrations": len(rows), "selected_qualified_registrations": len(rows),
                   "required_qualified_registrations": (entrants * 9 + 9) // 10,
                   "qualified_coverage": len(rows) / entrants,
                   "meta_coverage_claim": missing <= 2, "blockers": [] if missing <= 2 else ["coverage"],
                   "selected_roster": [{"archetype": a, "known_registrations": 10,
                                        "qualified_registrations": 10, "weight": 10 / entrants}
                                       for a in ("A", "B")],
                   "window": {"as_of": "2026-09-13", "start_inclusive": "2026-07-13",
                              "end_exclusive": "2026-09-07", "weeks": 8},
                   "build_provenance": {"card_name_aliases": {}, "split_seed": SPLIT_SEED,
                                        "test_fixture_only": True}}
        files = {"inventory.json": inventory, "events.json": [event], "registrations.json": rows,
                 "splits.json": splits, "snapshot.json": summary}
        if mutate:
            mutate(files)
        spec["data"] = {"kind": "mtgo_snapshot", "bundle": self.pin(
            [self.pin(value, name) for name, value in files.items()], "output-pins.json")}
        return spec, files


class CatalogTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="breadth-catalog-fixture-")
        self.addCleanup(self.temp.cleanup)
        self.fixture = Fixture(self.temp.name)

    def test_engineering_preserves_multiplicity_and_legal_postboard(self):
        rows, spec = self.fixture.engineering()
        before = copy.deepcopy(spec)
        result = catalog.load_catalog_v1(spec)
        self.assertEqual(spec, before)
        self.assertEqual([a["field_count"] for a in result["archetypes"]], [2, 1])
        first = result["archetypes"][0]["train"][0]
        self.assertEqual(first["id"], rows[0]["list_sha256"])
        self.assertEqual(first["multiplicity"], 2)
        self.assertEqual(len(first["registered"]["mainboard"]), 60)
        self.assertEqual(len(first["registered"]["sideboard"]), 15)
        registered = first["registered"]
        selected = first["postboard"][0]
        self.assertNotEqual(registered["mainboard"], selected["mainboard"])
        self.assertEqual(Counter(registered["mainboard"] + registered["sideboard"]),
                         Counter(selected["mainboard"] + selected["sideboard"]))
        self.assertFalse(result["provenance"]["meta_coverage_claim"])
        self.assertFalse(result["provenance"]["unseen_holdout_claim"])
        self.assertFalse(result["provenance"]["model_loaded"])

    def test_pins_strict_json_and_exact_list_digest_reject_tamper(self):
        for raw in (b'{"x":1,"x":2}', b'{"x":NaN}'):
            with self.subTest(raw=raw), self.assertRaises(ValueError):
                catalog.read_pin(self.fixture.pin_bytes(raw), [])
        pinned = self.fixture.pin({"valid": True})
        Path(pinned["path"]).write_bytes(b'{}')
        with self.assertRaisesRegex(ValueError, "pin differs"):
            catalog.read_pin(pinned, [])
        rows, _ = self.fixture.engineering()
        rows[0]["mainboard"] = {"Basic A": 59, "Basic B": 1}
        with self.assertRaisesRegex(ValueError, "digest differs"):
            catalog.load_catalog_v1(self.fixture.spec(rows))

    def test_train_catalog_excludes_holdouts_for_both_deck_roles(self):
        rows, _ = self.fixture.engineering()
        reserved = self.fixture.row("A", 4, 2, "development")
        rows.append(reserved)
        result = catalog.load_catalog_v1(self.fixture.spec(rows))
        self.assertEqual(result["archetypes"][0]["field_count"], 3)
        self.assertEqual([g["id"] for g in result["archetypes"][0]["train"]],
                         [rows[0]["list_sha256"]])
        # The shared admitted train pool supplies both learner and opponent decks.
        with self.assertRaisesRegex(ValueError, "non-training|excluded"):
            catalog.load_catalog_v1(self.fixture.spec(rows, choices={reserved["list_sha256"]: []}))
        rows[1]["split"] = "confirmation"
        with self.assertRaisesRegex(ValueError, "crosses archetypes or splits"):
            catalog.load_catalog_v1(self.fixture.spec(rows))

    def test_reserved_exact_and_sideboard_reachable_combined75_reject(self):
        rows, _ = self.fixture.engineering()
        original = self.fixture.zones(rows[0])
        reachable = {"mainboard": {"Basic A": 59, "Basic B": 1},
                     "sideboard": {"Basic A": 1, "Basic B": 14}}
        self.assertNotEqual(catalog.digest(original), catalog.digest(reachable))
        self.assertEqual(catalog.combined_key(original), catalog.combined_key(reachable))
        for held in (original, reachable):
            prior = [{"list_sha256": catalog.digest(held),
                      "combined75_sha256": catalog.combined_key(held)}]
            with self.subTest(held=held), self.assertRaisesRegex(ValueError, "reserved"):
                catalog.load_catalog_v1(self.fixture.spec(rows, reserved=prior,
                    choices={rows[0]["list_sha256"]: [reachable]}))
        changed = {"mainboard": {"Basic A": 59, "Basic C": 1}, "sideboard": {"Basic B": 15}}
        with self.assertRaisesRegex(ValueError, "changes registered 75"):
            catalog.load_catalog_v1(self.fixture.spec(rows, choices={rows[0]["list_sha256"]: [changed]}))

    def test_absent_nonfull_token_and_combined_copy_limit_reject(self):
        for defect in ("absent", "partial", "token", "copies"):
            with self.subTest(defect=defect):
                rows = [self.fixture.row("A", 1), self.fixture.row("B", 2)]
                registry = copy.deepcopy(self.fixture.registry)
                if defect == "absent":
                    rows[0]["mainboard"] = {"Unknown card": 60}
                elif defect == "partial":
                    registry["cards"][0]["engine_capability"] = "partial"
                elif defect == "token":
                    registry["cards"][0]["is_token"] = True
                else:
                    rows[0]["mainboard"] = {"Basic A": 56, "Spell": 4}
                    rows[0]["sideboard"] = {"Basic B": 14, "Spell": 1}
                rows[0]["list_sha256"] = catalog.digest(self.fixture.zones(rows[0]))
                spec = self.fixture.spec(rows)
                spec["registry"] = self.fixture.pin(registry)
                with self.assertRaisesRegex(ValueError, "Unimplemented|fully executable|four copies"):
                    catalog.load_catalog_v1(spec)

    def test_synthetic_future_campaign_contract_uses_exact_train_groups(self):
        spec, files = self.fixture.snapshot()
        result = catalog.load_catalog_v1(spec)
        self.assertEqual(result["scope"], "campaign")
        self.assertEqual([a["field_count"] for a in result["archetypes"]], [10, 10])
        self.assertEqual([len(a["train"]) for a in result["archetypes"]], [8, 8])
        admitted = {g["id"] for a in result["archetypes"] for g in a["train"]}
        reserved = {r["list_sha256"] for r in files["registrations.json"] if r["split"] != "train"}
        self.assertFalse(admitted & reserved)
        self.assertEqual(result["provenance"]["selected_qualified_registrations"], 20)
        self.assertFalse(result["provenance"]["prior_training_exposure_audited"])
        self.assertFalse(result["provenance"]["unseen_holdout_claim"])

    def test_campaign_qualification_runtime_and_report_pins_must_match(self):
        spec, _ = self.fixture.snapshot()
        spec["qualification_runtime_sha256"] = "c" * 64
        with self.assertRaisesRegex(ValueError, "Qualification runtime"):
            catalog.load_catalog_v1(spec)
        spec, files = self.fixture.snapshot()
        report = files["registrations.json"][0]["qualification"]["evidence"]
        Path(report["path"]).write_bytes(b'{}')
        with self.assertRaisesRegex(ValueError, "pin differs"):
            catalog.load_catalog_v1(spec)

    def test_missing_publication_mass_is_preserved_and_never_renormalized_to_coverage(self):
        spec, _ = self.fixture.snapshot(entrants=22)
        result = catalog.load_catalog_v1(spec)
        self.assertEqual(result["provenance"]["denominator"], 22)
        self.assertEqual(result["provenance"]["missing_registrations"], 2)
        self.assertTrue(result["provenance"]["conditional_field_weights"])
        spec, _ = self.fixture.snapshot(entrants=40, scope="engineering")
        result = catalog.load_catalog_v1(spec)
        self.assertEqual(result["provenance"]["denominator"], 40)
        self.assertEqual(result["provenance"]["missing_registrations"], 20)
        self.assertFalse(result["provenance"]["meta_coverage_claim"])
        spec["scope"] = "campaign"
        with self.assertRaisesRegex(ValueError, "not ready|90 percent"):
            catalog.load_catalog_v1(spec)
        _, engineering = self.fixture.engineering()
        engineering["scope"] = "campaign"
        with self.assertRaisesRegex(ValueError, "Engineering fixtures"):
            catalog.load_catalog_v1(engineering)

    def test_snapshot_event_inventory_and_split_summaries_must_reconcile(self):
        def wrong_entrants(files):
            files["events.json"][0]["entrant_count"] += 1
        def missing_inventory(files):
            files["inventory.json"] = []
        def wrong_split_count(files):
            files["splits.json"][0]["train_groups"] = 9
        for mutate in (wrong_entrants, missing_inventory, wrong_split_count):
            with self.subTest(mutation=mutate.__name__):
                spec, _ = self.fixture.snapshot(mutate=mutate)
                with self.assertRaises(ValueError):
                    catalog.load_catalog_v1(spec)

    def test_frozen_split_membership_seed_and_eight_week_window_are_bound(self):
        def swapped_membership(files):
            rows = files["registrations.json"]
            train = next(row for row in rows if row["archetype"] == "A" and row["split"] == "train")
            confirmation = next(row for row in rows if row["archetype"] == "A" and row["split"] == "confirmation")
            train["split"], confirmation["split"] = confirmation["split"], train["split"]
            # Preserve counts and the event/registration join. Only membership
            # differs from the original pinned split algorithm.
            files["events.json"][0]["registrations"] = copy.deepcopy(rows)

        def changed_seed(files):
            files["snapshot.json"]["build_provenance"]["split_seed"] = SPLIT_SEED + "-changed"

        def outside_event(files):
            files["events.json"][0]["date"] = "2026-09-07"

        def outside_inventory(files):
            files["inventory.json"][0]["index_date"] = "2026-07-12"

        def malformed_window(files):
            # Eight weeks long, but not complete Monday-based weeks.
            files["snapshot.json"]["window"].update(
                start_inclusive="2026-07-14", end_exclusive="2026-09-08")

        for mutate, message in ((swapped_membership, "split membership"),
                                (changed_seed, "split seed"),
                                (outside_event, "outside the pinned window"),
                                (outside_inventory, "outside the pinned window"),
                                (malformed_window, "eight-week calendar window")):
            with self.subTest(mutation=mutate.__name__):
                spec, _ = self.fixture.snapshot(mutate=mutate)
                with self.assertRaisesRegex(ValueError, message):
                    catalog.load_catalog_v1(spec)

    def test_renaming_duplicate_physical_registration_cannot_inflate_field_weight(self):
        def duplicate_registration(files, *, canonical_new_player=False):
            rows = files["registrations.json"]
            duplicate = copy.deepcopy(rows[0])
            duplicate["registration_id"] = "99:999"
            if canonical_new_player:
                duplicate["player_id"] = "999"
            rows.append(duplicate)
            event = files["events.json"][0]
            event.update(entrant_count=21, published_registration_count=21,
                         registrations=copy.deepcopy(rows))
            summary = files["snapshot.json"]
            summary.update(official_entrants_in_known_events=21,
                           complete_inventory_denominator=21, published_registrations=21,
                           registry_supported_candidate_registrations=21,
                           qualified_executable_registrations=21,
                           selected_qualified_registrations=21,
                           required_qualified_registrations=19)
            for row in summary["selected_roster"]:
                count = 11 if row["archetype"] == "A" else 10
                row.update(known_registrations=count, qualified_registrations=count,
                           weight=count / 21)
            # All list groups, qualification pins, joins and denominators still
            # reconcile. The repeated event/player/deck must itself reject.

        for canonical_new_player in (False, True):
            with self.subTest(canonical_new_player=canonical_new_player):
                spec, _ = self.fixture.snapshot(mutate=lambda files: duplicate_registration(
                    files, canonical_new_player=canonical_new_player))
                with self.assertRaisesRegex(ValueError, "physical registration"):
                    catalog.load_catalog_v1(spec)


if __name__ == "__main__":
    unittest.main()
