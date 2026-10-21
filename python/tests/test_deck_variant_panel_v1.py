import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "python" / "tools"))

import deck_variant_panel_v1 as dvp  # noqa: E402

FAKE = r'''
import hashlib, json, os, sys
from pathlib import Path
request = json.loads(Path(sys.argv[1]).read_text())
out = Path(request["output_directory"]); out.mkdir()
(out / "start.json").write_text(json.dumps({"schema": "public-input-evaluation-start/v1", "command": request}))
fail = {int(s) for s in os.environ.get("FAKE_FAIL_SEEDS", "").split(",") if s}
interrupt_after = int(os.environ.get("FAKE_INTERRUPT_AFTER", "-1"))
marker = os.environ.get("FAKE_INTERRUPT_MARKER")
log = os.environ.get("FAKE_LOG")
hashes = []
for i, match in enumerate(request["matches"]):
    seed = match["config"]["seed"]
    if log:
        with open(log, "a") as stream:
            stream.write(f"{seed}|{match['registered'][0]['label']}|{match['registered'][1]['label']}\n")
    if seed in fail:
        (out / "failure.json").write_text(json.dumps({"error": f"evaluation failed: match seed {seed}, game 1: decision cap"}))
        sys.exit(1)
    if i == interrupt_after and marker and not os.path.exists(marker):
        open(marker, "w").close()
        sys.exit(3)
    labels = [d["label"] for d in match["registered"]]
    focal = 0 if labels[0].startswith("B0") or labels[0] == "Inc" else 1
    value = int(hashlib.sha256(f"{seed}".encode()).hexdigest(), 16)
    winner = None if value % 7 == 0 else (focal if value % 2 else 1 - focal)
    if "B0/0001" in labels and value % 3 == 0:
        winner = focal
    body = {"schema": "public-input-evaluation-game-one/v1", "game_one_only": True, "match": match,
            "games": [{"winner": winner, "start": {"starting_player": match["config"]["game_one_chooser"]}}],
            "sideboard_decisions": [], "models": {}}
    path = out / f"match-{i:06}.json"
    path.write_text(json.dumps(body))
    hashes.append(hashlib.sha256(path.read_bytes()).hexdigest())
(out / "completion.json").write_text(json.dumps({"schema": "public-input-evaluation-completion/v1",
    "matches": len(hashes), "natural_games": len(hashes), "match_sha256": hashes}))
'''


def deck(label, role, card=0):
    return {"label": label, "role": role, "mainboard": [card] * 60, "sideboard": [1] * 15}


class Fixture(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        fake = self.tmp / "fake.py"
        fake.write_text(FAKE)
        if os.name == "nt":
            self.exe = self.tmp / "fake.cmd"
            self.exe.write_text(f'@"{sys.executable}" "{fake}" %1\r\n')
        else:
            self.exe = self.tmp / "fake.sh"
            self.exe.write_text(f'#!/bin/sh\nexec "{sys.executable}" "{fake}" "$1"\n')
            self.exe.chmod(0o755)
        self.log = self.tmp / "seeds.log"
        os.environ["FAKE_LOG"] = str(self.log)
        for key in ("FAKE_FAIL_SEEDS", "FAKE_INTERRUPT_AFTER", "FAKE_INTERRUPT_MARKER"):
            os.environ.pop(key, None)
        self.field = [deck(f"F{i}", "field", i) for i in range(9)]
        self.decks = [deck("Inc", "incumbent"), deck("B0/0000", "candidate", 2), deck("B0/0001", "candidate", 3)]
        model = self.tmp / "model.bin"
        model.write_bytes(b"weights")
        source = {"kind": "legacy", "source": {"checkpoint": dvp.pin(model)}, "v3_forced_actions": False}
        self.sources = {"pilots": {"g115": source, "a48": source}, "opponent": source}

    def plan(self, blocks=2, mode="alternate", decks=None, size=10, name="plan"):
        cases = dvp.stage_cases(7, "M1a", "Burn", blocks, ["g115", "a48"], mode)
        return dvp.build_plan("M1a", decks or self.decks, self.field, cases, self.sources, self.tmp / name,
                              str(self.tmp / f"{name}-native").replace("\\", "/"), size)

    def ledger(self, cap=1e9, per=0.01):
        return dvp.Ledger(self.tmp / "ledger.jsonl", cap, per)


class Cases(Fixture):
    def test_mana_colors_count_hybrid_halves_but_not_phyrexian(self):
        self.assertEqual(dvp.mana_colors("{R/P}"), {"R"})
        self.assertEqual(dvp.mana_colors("{1}{R/G}"), {"R", "G"})
        self.assertEqual(dvp.mana_colors("{2}{U}{U}"), {"U"})
        self.assertEqual(dvp.mana_colors(""), set())

    def test_seeds_are_stable_candidate_independent_and_stage_specific(self):
        a = dvp.case_seed(1, "M3", "Burn", 2, 5, 1, 0)
        self.assertEqual(a, dvp.case_seed(1, "M3", "Burn", 2, 5, 1, 0))
        self.assertNotEqual(a, dvp.case_seed(1, "M4", "Burn", 2, 5, 1, 0))
        self.assertNotEqual(a, dvp.case_seed(1, "qual", "Burn", 2, 5, 1, 0))
        cases = dvp.stage_cases(1, "M1a", "Burn", 8, ["g115", "a48"], "alternate")
        self.assertEqual(len(cases), 9 * 8 * 4)
        self.assertTrue(all(c["pilots"] == [["g115", "a48"][c["block"] % 2]] for c in cases))
        shared = dvp.stage_cases(1, "M3", "Burn", 2, ["g115", "a48", "H"], "all")
        self.assertTrue(all(c["pilots"] == ["g115", "a48", "H"] for c in shared))

    def test_eligibility_on_the_engine_pool_gives_48_cards(self):
        cards = json.loads((ROOT / "data/cards_v1.json").read_text(encoding="utf-8"))["cards"]
        pool = json.loads((ROOT / "data/pauper_pool_v1.json").read_text(encoding="utf-8"))
        names = {c["name"]: i for i, c in enumerate(cards)}
        unupdated = {names[n] for n in ("Pyroblast", "Gut Shot", "Searing Blaze")}
        rows = [{"card_id": i, "no_retained_embedding_update": i in unupdated} for i in range(len(cards))]
        audit = {"models": {p: {"source": {"checkpoint": {"sha256": p * 2}}, "rows": rows} for p in ("g115", "a48")}}
        result = dvp.eligible_in_cards(cards, pool, "Burn", audit, {"g115": "g115g115", "a48": "a48a48"})
        self.assertEqual(len(result["in_cards"]), 48)
        self.assertEqual(result["exposure_limited"], ["Gut Shot", "Pyroblast", "Searing Blaze"])
        self.assertIn(str(names["Sneaky Snacker"]), result["names"])
        with self.assertRaises(ValueError):
            dvp.eligible_in_cards(cards, pool, "Burn", audit, {"g115": "other", "a48": "a48a48"})

    def test_plan_puts_the_focal_deck_and_pilot_in_the_case_seat(self):
        plan = self.plan()
        self.assertEqual(plan["matches"], 9 * 2 * 4 * 3)
        for item in plan["requests"]:
            request = json.loads(Path(item["request"]["path"]).read_text())
            self.assertEqual(request["sources"][item["seat"]], self.sources["pilots"][item["pilot"]])
            for match, entry in zip(request["matches"], item["items"]):
                self.assertEqual(match["registered"][item["seat"]]["label"], self.decks[entry["deck"]]["label"])
                self.assertEqual(match["registered"][1 - item["seat"]]["label"], f"F{entry['case'][0]}")
                self.assertEqual(match["config"]["game_one_chooser"], entry["case"][3])


class Execution(Fixture):
    def test_complete_run_rows_and_win_scoring(self):
        plan = self.plan()
        outcome = dvp.dispatch(self.tmp / "plan", str(self.exe), 2, self.ledger(), None,
                               self.tmp / "STOP", [], 0, check_seconds=0)
        self.assertEqual(outcome, "complete")
        rows = dvp.rows(self.tmp / "plan", [])
        self.assertEqual(len(rows), plan["matches"])
        self.assertTrue(all(r["complete"] and r["win"] in (0, 1) for r in rows))
        self.assertTrue(any(r["draw"] for r in rows) and all(r["win"] == 0 for r in rows if r["draw"]))

    def test_scientific_failure_is_data_and_play_continues(self):
        plan = self.plan()
        item = plan["requests"][0]
        failing = item["items"][3]["seed"]
        os.environ["FAKE_FAIL_SEEDS"] = str(failing)
        dvp.dispatch(self.tmp / "plan", str(self.exe), 1, self.ledger(), None, self.tmp / "STOP", [], 0, 0)
        rows = dvp.rows(self.tmp / "plan", [])
        failed = [r for r in rows if not r["complete"]]
        self.assertEqual({r["seed"] for r in failed}, {failing})
        self.assertIn("decision cap", failed[0]["technical"])
        self.assertEqual(len(rows), plan["matches"])

    def test_interrupted_prefix_is_kept_and_resumed_at_the_first_unfinished_case(self):
        plan = self.plan(size=20)
        item = plan["requests"][0]
        os.environ["FAKE_INTERRUPT_AFTER"] = "7"
        os.environ["FAKE_INTERRUPT_MARKER"] = str(self.tmp / "interrupted")
        state = dvp.execute_request(str(self.exe), self.tmp / "plan", item, lambda *a: None)
        output = Path(json.loads(Path(item["request"]["path"]).read_text())["output_directory"])
        self.assertEqual(state["interruptions"], [{"segment": 0, "kept": 7}])
        self.assertEqual([s["end"] for s in state["segments"]], ["interrupted", "complete"])
        self.assertEqual(len(dvp.match_files(output)), 7)
        seeds = [line.split("|")[0] for line in self.log.read_text().splitlines()]
        expected = [str(entry["seed"]) for entry in item["items"]]
        self.assertEqual(seeds, expected[:8] + expected[7:], "each case after the prefix ran exactly once more")
        again = dvp.execute_request(str(self.exe), self.tmp / "plan", item, lambda *a: None)
        self.assertEqual(again, state)

    def test_dispatch_admits_nothing_once_stopped_or_over_the_cpu_cap(self):
        self.plan()
        (self.tmp / "STOP").write_text("")
        outcome = dvp.dispatch(self.tmp / "plan", str(self.exe), 2, self.ledger(), None, self.tmp / "STOP", [], 0, 0)
        self.assertTrue(outcome.startswith("yielded: stop file"))
        self.assertFalse(self.log.exists())
        (self.tmp / "STOP").unlink()
        outcome = dvp.dispatch(self.tmp / "plan", str(self.exe), 2, self.ledger(cap=0.05, per=0.01),
                               None, self.tmp / "STOP", [], 0, 0)
        self.assertTrue(outcome.startswith("yielded: CPU cap"))

    def test_qualification_parity_and_label_invariance(self):
        decks = self.decks + [dict(self.decks[0], label="B0/alias", role="alias")]
        self.plan(blocks=1, decks=decks, size=40, name="qual")
        receipt = dvp.qualify(self.tmp / "qual", self.tmp / "qroot", str(self.exe), [1, 2], self.ledger(), 24)
        self.assertTrue(all(t["parity"] for t in receipt["trials"]))
        self.assertGreater(receipt["label_invariance_cases"], 0)

    def test_chunks_verify_and_rows_read_from_the_archive_after_raw_is_freed(self):
        plan = self.plan()
        dvp.dispatch(self.tmp / "plan", str(self.exe), 2, self.ledger(), None, self.tmp / "STOP", [], 0, 0)
        before = dvp.rows(self.tmp / "plan", [])
        archive = self.tmp / "archive"
        made = dvp.seal_chunks(self.tmp / "plan", archive, max_files=60, max_bytes=2**30, final=True)
        self.assertGreater(len(made), 1)
        self.assertEqual(dvp.free_acknowledged(archive, self.tmp / "copy"), [], "nothing freed before an ack")
        for manifest in archive.glob("chunk-*.json"):
            sha = json.loads(manifest.read_text())["zip"]["sha256"]
            manifest.with_suffix(".ack").write_text(json.dumps({"zip_sha256": sha}))
        freed = dvp.free_acknowledged(archive, self.tmp / "copy")
        self.assertEqual(len(freed), len(plan["requests"]))
        self.assertEqual(dvp.rows(self.tmp / "plan", [archive]), before)


if __name__ == "__main__":
    unittest.main()
