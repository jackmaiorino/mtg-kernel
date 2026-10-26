from __future__ import annotations

import json
from pathlib import Path
import sys
import tempfile
import unittest


REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "python/tools"))
import limited_match_v1 as match


# A stand-in for kernel_limited_match_v1: same subcommands, canned outputs.
STUB = r'''
import json, sys
args = sys.argv[1:]
flags = dict(zip(args[1::2], args[2::2]))
if args[0] == "version":
    print(json.dumps({"schema": "mtg-kernel-limited-match/v1", "engine": "mtg-kernel"}))
elif args[0] == "coverage":
    print("unsupported: Koma, World-Eater in 3 decks missing [not full]")
    print("41 distinct cards in 5 decks; 1 unsupported; 0 decks failed to resolve; 2 of 5 decks play only supported cards")
    open(flags["-out"], "w").write("d1\nd2\n")
elif args[0] == "play":
    if flags["-a"] == "bad":
        sys.stderr.write("unknown policy")
        sys.exit(2)
    with open(flags["-out"], "w") as out:
        for game in range(2 * int(flags["-pairs"])):
            side = ["A", "B"] if game % 2 == 0 else ["B", "A"]
            row = {"game": game, "pair": game // 2, "leg": game % 2, "seed": int(flags["-seed"]),
                   "decks": ["d1", "d2"], "side": side, "pols": [flags["-a"], flags["-b"]], "winner": 0,
                   "result": side[0], "turns": 9, "ms": 5}
            out.write(json.dumps(row) + "\n")
    json.dump({"games": 2 * int(flags["-pairs"]), "errors": 0, "games_per_hour": 1.0},
              open(flags["-out"] + ".summary.json", "w"))
'''


class LimitedMatchV1Test(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        (root / "stub.py").write_text(STUB)
        self.kernel = match.MtgKernel([sys.executable, root / "stub.py"], root / "decks", root / "pool.tsv")
        self.root = root

    def tearDown(self):
        self.tmp.cleanup()

    def test_version_and_coverage_match_the_gorge_shape(self):
        self.assertEqual(self.kernel.version()["schema"], match.SCHEMA)
        cov = self.kernel.coverage()
        self.assertEqual((cov["cards"], cov["decks"], cov["unsupported_cards"], cov["supported_decks"]),
                         (41, 5, 1, 2))
        self.assertEqual(cov["unsupported"], {"Koma, World-Eater": {"decks": 3, "missing": "not full"}})
        self.assertEqual(cov["supported"], ["d1", "d2"])

    def test_play_argv_carries_every_setting(self):
        argv = self.kernel.play_argv("random", "pimc:sims=8:depth=4:closedlist", pairs=3, seed=7,
                                     out=Path("g.jsonl"), split="eval", first_pair=2, workers=4,
                                     belief=match.Belief(Path("bd"), Path("bp.tsv")), single=True)
        flags = dict(zip(argv[1::2], argv[2::2]))
        self.assertEqual(argv[0], "play")
        self.assertEqual((flags["-split"], flags["-seed"], flags["-first-pair"], flags["-pairs"], flags["-workers"]),
                         ("eval", "7", "2", "3", "4"))
        self.assertEqual((flags["-belief-decks"], flags["-belief-pool"], flags["-belief-split"]),
                         ("bd", "bp.tsv", "train"))
        self.assertEqual(argv[-1], "-single")

    def test_play_reads_rows_and_summary(self):
        result = self.kernel.play("random", "random:x", pairs=2, seed=5, out_dir=self.root / "run")
        match.check_rows(result.games, 2, 2)
        self.assertEqual([g["result"] for g in result.games], ["A", "B", "A", "B"])
        self.assertEqual(result.summary["games"], 4)
        self.assertEqual(result.runs[0]["games"], 4)

    def test_refusals_raise(self):
        with self.assertRaises(match.MtgKernelError):
            self.kernel.play("bad", "random", pairs=1, seed=1, out_dir=self.root / "bad")
        with self.assertRaises(ValueError):
            self.kernel.play("random", "random", pairs=0, seed=1, out_dir=self.root / "none")
        with self.assertRaises(match.MtgKernelError):
            match.check_rows([{"game": 0}], 1, 1)
        with self.assertRaises(match.MtgKernelError):
            match.parse_coverage("no summary here", [])

    def test_fixture_pool_holds_both_forty_card_decks(self):
        decks, pool = match.write_fixture_pool(self.root / "fixture")
        rows = [line.split("\t") for line in pool.read_text().splitlines()]
        self.assertEqual(rows[0][:2], ["deck", "split"])
        self.assertEqual([r[0] for r in rows[1:]], list(match.FIXTURE_DECKS))
        for stem in match.FIXTURE_DECKS:
            deck = json.loads((decks / f"{stem}.json").read_text())
            self.assertEqual(sum(c["count"] for c in deck["cards"]), 40)


if __name__ == "__main__":
    unittest.main()
