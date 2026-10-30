import json
import sys
import tempfile
import threading
import unittest
import urllib.error
import urllib.request
from http.server import ThreadingHTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
import viewer_rs0_review_v1 as rv  # noqa: E402

SPY = "CastSpell { actor: P0, source: CardStableRefV1 { arena_id: 0, card_db_id: 150, owner: P0, controller: P0, zone: Hand, zone_change_count: 1 } }"
SELF = "ChooseTarget { actor: P0, source: CardStableRefV1 { arena_id: 0, card_db_id: 150, owner: P0, controller: P0, zone: Battlefield, zone_change_count: 3 }, remaining: 1, target: Player { player: P0 } }"
PASS = "Pass { actor: P0 }"


def card(i, db, name):
    return {"id": i, "db": db, "name": name}


def board():
    hero = {"life": 20, "hand": [card(0, 150, "Balustrade Spy")], "library": [card(5, 151, "Dread Return")],
            "battlefield": [], "graveyard": [], "exile": [], "mana_pool": [0] * 6}
    opp = {**hero, "hand": [card(9, 6, "Lightning Bolt")], "library": []}
    return {"turn": 5, "active": 0, "priority": 0, "step": "PrecombatMain", "players": [hero, opp], "stack": []}


def root(root_id, stratum, role):
    return {"root_id": root_id, "index": 3, "roles": [{"stratum": stratum, "role": role, "certified_index": 7}]}


class Rs0ReviewTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def write(self, name, rows):
        path = self.dir / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("".join(json.dumps(r) + "\n" for r in rows), encoding="utf-8")
        return path

    def bundle(self, line=(SPY, SELF)):
        roots = self.write("roots.jsonl", [root("g#3", "spy", "k2")])
        export = self.write("export.jsonl", [{
            "kind": "review", "root_id": "g#3", "game_id": "g", "role": "k2", "stratum": "spy", "opportunity_turn": 5,
            "index": 3, "certified_index": 7, "focal": 0, "deck_ids": ["Spy/x", "Burn/y"], "winner": 1,
            "root": {"board": board(), "legal": [PASS], "actor": 0},
            "certified": {"board": board(), "legal": [PASS, SPY], "actor": 0, "selected": 0},
            "path": [{"index": 3, "actor": 0, "turn": 4, "step": "Draw", "legal": 1, "selected": 0, "action": PASS}]}])
        self.write("run/main-0.jsonl", [{"kind": "root", "root_id": "g#3"}] + [
            {"kind": "search", "root_id": "g#3", "budget": 64000, "seed": s, "o1_spy": s == 1, "failed": False,
             "selection": {"discovery": {"completions_natural": int(s == 1), "trajectories": 280}}} for s in range(3)])
        self.write("checker/rows-0.jsonl", [{"kind": "decision", "id": "g", "index": 7,
                                             "oracle": {"verdict": "proven", "line": list(line), "nodes": 99},
                                             "robust": {"verdict": "proven", "line": list(line)}}])
        out = self.dir / "positions.json"
        return rv.build(roots, export, self.dir / "run", str(self.dir / "checker" / "rows-*.jsonl"), out), out

    def test_selects_roots_by_role_in_role_order(self):
        rows = [root("b#1", "spy", "k1"), root("a#1", "spy", "k2"), root("c#1", "burn", "k2")]
        self.assertEqual([r["root_id"] for r in rv.select_roots(rows, ["spy:k2", "spy:k1"])], ["a#1", "b#1"])

    def test_describe_names_cards_and_players(self):
        names = {150: "Balustrade Spy"}
        self.assertEqual(rv.describe(SPY, names, 0), "cast spell: source: Balustrade Spy (hand)")
        self.assertIn("target: you", rv.describe(SELF, names, 0))
        self.assertIn("target: opponent", rv.describe(SELF, names, 1))
        self.assertEqual(rv.describe(PASS, names, 0), "pass")

    def test_build_joins_rs0_and_certified_line(self):
        bundle, _ = self.bundle()
        self.assertEqual(bundle["problems"], [])
        (p,) = bundle["positions"]
        self.assertEqual(p["line"][0], "cast spell: source: Balustrade Spy (hand)")
        self.assertEqual(p["certified"]["chosen"], "pass")
        self.assertEqual([v["found"] for v in p["rs0"]["64000"].values()], [False, True, False])
        self.assertTrue(p["robust"]["same_line"])

    def test_build_refuses_a_line_that_is_not_legal_at_the_certified_decision(self):
        bundle, _ = self.bundle(line=(SELF,))
        self.assertEqual(bundle["positions"], [])
        self.assertIn("does not start with a legal action", bundle["problems"][0])

    def test_server_round_trip_and_validation(self):
        _, positions = self.bundle()
        verdicts = self.dir / "lane" / "RS0-HUMAN-REVIEW.jsonl"
        server = ThreadingHTTPServer(("127.0.0.1", 0), rv.handler(rv.Review(positions, verdicts)))
        threading.Thread(target=server.serve_forever, daemon=True).start()
        base = f"http://127.0.0.1:{server.server_address[1]}/"
        try:
            self.assertIn(b"RS0 position review", urllib.request.urlopen(base).read())
            self.assertEqual(len(json.load(urllib.request.urlopen(base + "api/positions"))["positions"]), 1)

            def post(body):
                req = urllib.request.Request(base + "api/verdicts", json.dumps(body).encode(), {"Content-Type": "application/json"})
                return json.load(urllib.request.urlopen(req))

            good = {"root_id": "g#3", "real_win": "hidden", "human_find": "maybe", "how": "backward", "note": "needs the top card"}
            saved = post(good)
            self.assertEqual(saved["certified_index"], 7)
            self.assertEqual(saved["use"], "evaluation and design input only")
            post({**good, "real_win": "yes"})
            self.assertEqual(json.load(urllib.request.urlopen(base + "api/verdicts"))["g#3"]["real_win"], "yes")
            self.assertEqual(len(verdicts.read_text(encoding="utf-8").splitlines()), 2)
            for bad in ({**good, "root_id": "other"}, {**good, "how": "guess"}):
                with self.assertRaises(urllib.error.HTTPError) as err:
                    post(bad)
                self.assertEqual(err.exception.code, 400)
        finally:
            server.shutdown()
            server.server_close()


if __name__ == "__main__":
    unittest.main()
