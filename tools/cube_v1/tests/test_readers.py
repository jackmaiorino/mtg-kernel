"""Tests of the M4 readers (D7, D7a) on m4-interfaces/v2, read plan v2 and the signed A6 (part 1).

Modules: seeds_v1.py, deal_common.py, pod_record.py, read_plans.py, read_sets.py and readers.py.
Every record, plan and set is synthetic, in namespace `test` (tests/cube_synth.py).
"""

from __future__ import annotations

import copy
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import re
import sys
import tempfile
import unittest

TESTS_DIR = Path(__file__).resolve().parent
CUBE_V1_DIR = TESTS_DIR.parent
sys.path.insert(0, str(CUBE_V1_DIR))
sys.path.insert(0, str(TESTS_DIR))

import cube_synth as synth  # noqa: E402
import deal  # noqa: E402
import deal_common  # noqa: E402
import pod_record  # noqa: E402
import read_plans  # noqa: E402
import read_sets  # noqa: E402
import readers  # noqa: E402
import seeds_v1  # noqa: E402

FIXTURES = TESTS_DIR / "fixtures"
A6_PART1_FIXTURE = FIXTURES / "a6_part1_signed.md"
DECLARATION = FIXTURES / "read_declaration_test.json"
DECL, DECL_SHA = readers.load_declaration(DECLARATION)
RUST_GOLDENS = CUBE_V1_DIR.parent.parent / "mtg-kernel" / "tests" / "cube_read_seed_goldens_v1.rs"
D5_UNIFORM_FIXTURE = FIXTURES / "d5_uniform_pod_record.json"
DEAL_PY_SHA256 = "0e60d49f40eb9509210f4e2f71e8d2cd7dac65e393696427043978556e5af497"
F = Fraction


def exact(value: dict) -> Fraction:
    num, den = value["exact"].split("/")
    return Fraction(int(num), int(den))


def interval(report: dict, stat: str, scoring: str = "drop") -> list[Fraction] | None:
    got = report["statistics"][stat][scoring]["interval"]
    return None if got is None else [exact(v) for v in got]


def estimate(report: dict, stat: str, scoring: str = "drop") -> Fraction | None:
    got = report["statistics"][stat][scoring]["estimate"]
    return None if got is None else exact(got)


def decl_with(**changes) -> dict:
    """The fixture declaration with changes like {"c.n": 3}."""
    decl = copy.deepcopy(DECL)
    for dotted, value in changes.items():
        node = decl["reads"]
        *head, last = dotted.split(".")
        for key in head:
            node = node[key]
        node[last] = value
    return decl


def by_slot(results: dict[int, str], arm: int | None = None, other: str = "draw"):
    """Outcome for dealer layouts: the focal result by slot j (and arm 0 x, 1 y for paired plans)."""
    def outcome(p, g, pos):
        j = g["group"] % 4 if p["family"] not in ("b", "size_b") else g["game"] % 4
        a = g["game"] % 2
        if arm is None or a == arm:
            return results.get(j, other)
        return other
    return outcome


# ---------------------------------------------------------------------------------------------
# Sets used by several tests


def c_sets(root: Path, case: str, outcome, *, n: int = 2, look2: bool = False, family: str = "c",
           look2_outcome=None) -> list[dict]:
    """(c)-family look sets on a shared b_c prep set reserving [0, 3n)."""
    prep_family = "b_c" if family in ("b", "c") else family
    prep_name = f"fx-{prep_family}-{case}-d0-prep-0-{3 * n}"
    _, outs = synth.write_prep_set(root, prep_family, prep_family, prep_name, range(0, 3 * n), DECL_SHA,
                                   reserved=[0, 3 * n])
    prep_set = f"test/v1/{prep_name}/exec-0"
    plans1 = [synth.plan(family, u, prep=outs[u]) for u in range(n)]
    set1 = synth.write_plan_set(root, family, f"fx-{family}-{case}-d0-look-1", plans1, DECL_SHA,
                                executions=[{"records": {p["pod_index"]: synth.record(p, outcome) for p in plans1}}],
                                look=1, units=[0, n], reserved=[0, 3 * n], prep_set=prep_set)
    opened = [read_sets.open_plan_set(set1, DECL_SHA, pool=synth.POOL, constants=synth.CONST)]
    if look2:
        plans2 = [synth.plan(family, u, prep=outs[u]) for u in range(n, 3 * n)]
        out2 = look2_outcome or outcome
        set2 = synth.write_plan_set(root, family, f"fx-{family}-{case}-d0-look-2", plans2, DECL_SHA,
                                    executions=[{"records": {p["pod_index"]: synth.record(p, out2) for p in plans2}}],
                                    look=2, units=[n, 3 * n], pools_with=opened[0]["manifest"]["plan_set_sha256"],
                                    prep_set=prep_set)
        opened.append(read_sets.open_plan_set(set2, DECL_SHA, pool=synth.POOL, constants=synth.CONST))
    return opened


def a_sets(root: Path, family: str, case: str, outcome, *, units: int = 2, set_suffix: str = "look-1",
           look: int | None = 1, executions=None) -> dict:
    plans = [synth.plan(family, u, mirror=m) for u in range(units) for m in (0, 1)]
    recs = {p["pod_index"]: synth.record(p, outcome) for p in plans}
    reserved = [0, 3 * units] if look == 1 else None
    set_dir = synth.write_plan_set(root, family, f"fx-{family}-{case}-d0-{set_suffix}", plans, DECL_SHA,
                                   executions=executions or [{"records": recs}], look=look, units=[0, units],
                                   reserved=reserved)
    return read_sets.open_plan_set(set_dir, DECL_SHA)


# ---------------------------------------------------------------------------------------------
# Change 11: the Rust seed port


class SeedPortTests(unittest.TestCase):
    def test_splitmix64_published_vector(self):
        rng = seeds_v1.SplitMix64(1234567)
        self.assertEqual([rng.next_u64() for _ in range(5)],
                         [6457827717110365317, 3203168211198807973, 9817491932198370423, 4593380528125082431,
                          16408922859458223821])
        self.assertEqual(seeds_v1.SplitMix64(0).next_u64(), 0xE220A8397B1DCDAF)

    def test_pinned_python_goldens(self):
        self.assertEqual(seeds_v1.derive_seed(0, seeds_v1.GAME, 3), 8375399752433882408)
        self.assertEqual(seeds_v1.game_seed(1, 27), 9862851748266521902)
        self.assertEqual(seeds_v1.pairing_seats(0, 5), (10808778974832213287, [0, 6]))
        self.assertEqual(seeds_v1.attempt_seeds(seeds_v1.game_seed(0, 0)),
                         {"deal_seed": 17342717373898743084, "env_seed": 4016630253392987778,
                          "action_seed": 6801290846407491845})
        self.assertEqual(seeds_v1.deck_seed(0, 7), 18276781107646718199)
        self.assertEqual(seeds_v1.build_seed(0, 7, 0), 11027067207131447367)
        self.assertEqual(seeds_v1.build_seed(0, 7, 1), 4583750976280949105)
        self.assertEqual(seeds_v1.build_seed(81985529216486895, 7, 1), 13959654806316832626)

    def test_rust_golden_file_holds_the_same_constants(self):
        text = RUST_GOLDENS.read_text(encoding="utf-8")
        stream = {"Game": seeds_v1.GAME, "GameCoin": seeds_v1.GAME_COIN, "GameEnv": seeds_v1.GAME_ENV,
                  "GameActions": seeds_v1.GAME_ACTIONS}
        rows = re.findall(
            r"\(\s*(\d+),\s*SeedStream::(\w+),\s*(\d+),\s*(\d+),?\s*\)",
            text,
        )
        self.assertGreater(len(rows), 20)
        for parent, name, index, value in rows:
            self.assertEqual(seeds_v1.derive_seed(int(parent), stream[name], int(index)), int(value))
        for pod, g, a, b in re.findall(r"\((\d+), (\d+), \[(\d), (\d)\]\)", text):
            self.assertEqual(seeds_v1.pairing_seats(int(pod), int(g))[1], [int(a), int(b)])

    def test_pod_pairings_order(self):
        pairs = seeds_v1.pod_pairings()
        self.assertEqual(len(pairs), 28)
        self.assertEqual(pairs[:3], [(0, 1), (0, 2), (0, 3)])
        self.assertEqual(pairs, sorted(pairs))


# ---------------------------------------------------------------------------------------------
# Changes 2 to 10 and 12: plans


class PlanFamilyTests(unittest.TestCase):
    def test_every_family_regenerates_and_refuses_any_change(self):
        cases = [("aa", {"mirror": 0}), ("a", {"mirror": 1}), ("a_early", {"mirror": 0}),
                 ("size_a", {"mirror": 1}), ("size_a_early", {"mirror": 0}), ("a_one", {"rotation": 5}),
                 ("a_early_one", {"rotation": 0}), ("a_all", {}), ("a_early_all", {})]
        for family, kw in cases:
            with self.subTest(family=family):
                plan = synth.plan(family, 3, **kw)
                read_plans.check_plan(plan)
                for mutate in (lambda p: p["games"][0].update(episode=p["games"][0]["episode"] + 1),
                               lambda p: p["games"][1].update(game_seed=1),
                               lambda p: p["seeds"].update(pod_seed=1),
                               lambda p: p["decks"][0].update(row="stated"),
                               lambda p: p["chairs"][0].update(role="older")):
                    bad = copy.deepcopy(plan)
                    mutate(bad)
                    with self.assertRaises(read_plans.PlanError):
                        read_plans.check_plan(bad)
        for family, prep_family in (("b", "b_c"), ("c", "b_c"), ("size_b", "size_b"), ("size_c", "size_c"),
                                    ("dose", "dose"), ("d13", "d13")):
            with self.subTest(family=family):
                _, _, result, sha = synth.prep_for(prep_family, 4)
                mark = {"dose": {"kind": "dose", "label": "pods-512", "ordinal": 0},
                        "d13": {"kind": "pilot", "label": "p1", "ordinal": 0}}.get(family)
                plan = synth.plan(family, 4, mark=mark, prep=(result, sha))
                read_plans.check_plan(plan, (result, sha), **synth.DEALER)
                bad = copy.deepcopy(plan)
                bad["decks"][-1]["cards"][0] += 1
                with self.assertRaises(read_plans.PlanError):
                    read_plans.check_plan(bad, (result, sha), **synth.DEALER)
                with self.assertRaisesRegex(read_plans.PlanError, "prep_sha256"):
                    read_plans.check_plan(plan, (result, "0" * 64), **synth.DEALER)

    def test_strict_shape(self):
        plan = synth.plan("a", 0, mirror=0)
        self.assertEqual(plan["schema"], "mtg-kernel-cube-read-plan/v3")
        self.assertEqual(
            list(plan["caps"].keys()),
            ["max_decisions", "max_physical_decisions", "max_policy_steps", "max_stack_objects",
             "watchdog_seconds", "max_replays"],
        )
        self.assertEqual(plan["caps"]["max_stack_objects"], 128)
        for mutate, pattern in ((lambda p: p.pop("rotation"), "missing"),
                                (lambda p: p.update(extra=1), "extra"),
                                (lambda p: p.update(schema="mtg-kernel-cube-read-plan/v1"), "schema"),
                                (lambda p: p.update(schema="mtg-kernel-cube-read-plan/v2"), "schema"),
                                (lambda p: p.update(family="z"), "family"),
                                (lambda p: p["decks"][0].update(row="other"), "row"),
                                (lambda p: p.update(unit=True), "u64"),
                                (lambda p: p["caps"].pop("max_stack_objects"), "missing"),
                                (lambda p: p["caps"].update(max_stack_objects=127), "max_stack_objects"),
                                (lambda p: p["caps"].update(max_stack_objects=1 << 32), "max_stack_objects"),
                                (lambda p: p["caps"].update(max_stack_objects=True), "max_stack_objects"),
                                (lambda p: p.__setitem__("caps", {
                                    "max_decisions": 16384, "max_physical_decisions": 16384,
                                    "max_policy_steps": 32768, "watchdog_seconds": 600,
                                    "max_stack_objects": 128, "max_replays": 0,
                                }), "order")):
            bad = copy.deepcopy(plan)
            mutate(bad)
            with self.assertRaisesRegex((read_plans.PlanError, pod_record.RecordError), pattern):
                read_plans.check_plan(bad)

    def test_identity_seeds_and_indices_by_family(self):
        mk = lambda fam, ns, **kw: read_plans.build_plan(  # noqa: E731
            fam, ns, 5, pins=synth.PINS, pilot=synth.PILOT, picker=dict(synth.UNIFORM),
            builders={"candidate": dict(synth.UNIFORM)}, **kw)
        even, odd = mk("a", "read-gate-a", mirror=0), mk("a", "read-gate-a", mirror=1)
        self.assertEqual((even["pod_index"], odd["pod_index"], even["mirror_pods"]), (10, 11, [10, 11]))
        self.assertEqual(even["seeds"], odd["seeds"])
        self.assertEqual(even["seeds"]["draft_seed"], deal.seed(["read-gate-a", "draft", "v1", 5]))
        read_plans.check_mirrors(even, odd)
        self.assertEqual([c["role"] for c in odd["chairs"]][:2], ["fixture", "learning"])
        one = mk("a_one", "read-gate-a-one", rotation=3)
        self.assertEqual(one["pod_index"], 43)
        self.assertEqual(one["seeds"]["draft_seed"], even["seeds"]["draft_seed"])
        self.assertEqual(one["seeds"]["pod_seed"], deal.seed(["read-gate-a-one", "pod", "v1", 5, 3]))
        self.assertEqual((one["seeds"]["draft_seed_namespace"], one["seeds"]["pod_seed_namespace"]),
                         ("read-gate-a", "read-gate-a-one"))
        self.assertEqual(len(one["games"]), 7)
        allp = mk("a_all", "read-gate-a-all")
        self.assertEqual((allp["pod_index"], len(allp["games"])), (5, 28))
        self.assertEqual(allp["seeds"]["pod_seed"], deal.seed(["read-gate-a-all", "pod", "v1", 5]))
        self.assertIsNone(even["seeds"]["league_seed"])
        self.assertEqual(read_plans.seed_sources("b", "read-gate-b"), ("read-gate-pools", "read-gate-pools"))
        self.assertEqual(read_plans.seed_sources("c", "read-gate-c"), ("read-gate-pools", "read-gate-pools"))
        self.assertEqual(read_plans.seed_sources("size_c", "size-gate-c"), ("size-gate-c", "size-gate-c"))
        with self.assertRaisesRegex(read_plans.PlanError, "plays in"):
            mk("a", "read-gate-b", mirror=0)

    def test_a_layout_games_coin_and_episodes(self):
        plan = synth.plan("a", 0, mirror=0)
        pod_seed = plan["seeds"]["pod_seed"]
        cross = [g for g, (lo, hi) in enumerate(seeds_v1.pod_pairings()) if lo % 2 != hi % 2]
        self.assertEqual(len(plan["games"]), 32)
        for k, g in enumerate(cross):
            for row in (0, 1):
                game = plan["games"][2 * k + row]
                g_seed, seats = seeds_v1.pairing_seats(pod_seed, g)
                self.assertEqual((game["group"], game["pairing"], game["episode"], game["game_seed"]), (k, g, g, g_seed))
                self.assertEqual(game["chairs"], seats)
                self.assertEqual(game["decks"], [2 * seats[0] + row, 2 * seats[1] + row])
        self.assertEqual([d["build"] for d in plan["decks"][:2]], [0, 1])
        self.assertIsNone(plan["pools"])
        self.assertIsNone(plan["decks"][0]["cards"])
        self.assertEqual(plan["inputs"], {"pool_sha256": None, "constants_sha256": None})

    def test_b_layout_equals_a15_and_episode_slots(self):
        _, _, result, sha = synth.prep_for("b_c", 2)
        plan = synth.plan("b", 2, prep=(result, sha))
        for s in range(8):
            deck = result["builds"][2 * s]["cards"]
            x = 8 * 2 + s
            ref = deal.build_read_b(synth.POOL[0], synth.CONSTANTS, deck, "test", "v1", x)
            episodes = [e["episode"] for e in ref["iterations"][0]["episodes"]]
            games = plan["games"][4 * s: 4 * s + 4]
            self.assertEqual([g["game_seed"] for g in games], [e["seed"] for e in episodes])
            self.assertEqual([g["decks"].index(2 * s) for g in games], [e["learner_seat"] for e in episodes])
            self.assertEqual([g["episode"] for g in games], [0, 1, 2, 3])
            for g, e in zip(games, episodes):
                self.assertEqual([plan["decks"][d]["cards"] for d in g["decks"]], [d["mainboard"] for d in e["selected"]])
            opp = plan["decks"][2 * s + 1]
            self.assertEqual(opp["dealt"], {"function": "cube_deal_b_v1", "key": ["test", "v1", x]})
            self.assertEqual(opp["builder"], {"kind": "dealt", "pin": opp["dealt"]})

    def test_a15_frozen_golden_replay_and_live_b_gate(self):
        # Pinned A1.5 golden passes and verifies exclusions, key, seeds, episodes, libraries and opening hands.
        verified = read_plans.verify_a15_golden()
        self.assertEqual(verified["sha256"], read_plans.A15_GOLDEN_SHA256)
        self.assertEqual(verified["games"], 4)

        # Live (b) planning succeeds when the pinned A1.5 golden matches.
        live_req = read_plans.build_prep_request("b_c", "read-gate-pools", 0, synth.PINS,
                                                 dict(synth.UNIFORM), {"candidate": dict(synth.UNIFORM)})
        live_req_sha = read_plans.doc_sha256(live_req)
        live_res = synth.prep_result(live_req, live_req_sha)
        live_res_sha = read_plans.doc_sha256(live_res)
        live_b = read_plans.build_plan("b", "read-gate-b", 0, pins=synth.PINS, pilot=synth.PILOT,
                                       picker=dict(synth.UNIFORM), builders={"candidate": dict(synth.UNIFORM)},
                                       prep=(live_res, live_res_sha), **synth.DEALER)
        self.assertEqual(live_b["namespace"], "read-gate-b")

        # Tampering with opening hands, expanded library, game seed or fixture hash refuses live (b) planning.
        base_doc = json.loads(read_plans.A15_GOLDEN_PATH.read_text(encoding="utf-8"))
        with tempfile.TemporaryDirectory() as tmp:
            for label, mutate, pattern in (
                ("opening-hand", lambda d: d["expected"]["games"][0]["p0_opening_hand"].reverse(), "p0_opening_hand"),
                ("expanded-lib", lambda d: d["expected"]["games"][1]["p1_expanded_library"].reverse(),
                 "p1_expanded_library"),
                ("game-seed", lambda d: d["expected"]["games"][2].update(game_seed=1), "game_seed"),
                ("episode", lambda d: d["expected"]["games"][3].update(episode=9), "episode"),
            ):
                with self.subTest(tamper=label):
                    bad = copy.deepcopy(base_doc)
                    mutate(bad)
                    bad_path = Path(tmp) / f"{label}.json"
                    bad_path.write_bytes(read_plans.serialize(bad).encode("utf-8"))
                    with self.assertRaisesRegex(read_plans.PlanError, pattern):
                        read_plans.verify_a15_golden(bad_path)

            # Live build_plan("b", "read-gate-b", ...) refuses when pinned golden is missing or hash mismatches.
            orig_path = read_plans.A15_GOLDEN_PATH
            try:
                read_plans.A15_GOLDEN_PATH = Path(tmp) / "missing.json"
                with self.assertRaisesRegex(read_plans.PlanError, "missing"):
                    read_plans.build_plan("b", "read-gate-b", 0, pins=synth.PINS, pilot=synth.PILOT,
                                          picker=dict(synth.UNIFORM), builders={"candidate": dict(synth.UNIFORM)},
                                          prep=(live_res, live_res_sha), **synth.DEALER)
                tampered_path = Path(tmp) / "tampered.json"
                tampered_path.write_bytes(b"{}")
                read_plans.A15_GOLDEN_PATH = tampered_path
                with self.assertRaisesRegex(read_plans.PlanError, "hash"):
                    read_plans.build_plan("b", "read-gate-b", 0, pins=synth.PINS, pilot=synth.PILOT,
                                          picker=dict(synth.UNIFORM), builders={"candidate": dict(synth.UNIFORM)},
                                          prep=(live_res, live_res_sha), **synth.DEALER)
            finally:
                read_plans.A15_GOLDEN_PATH = orig_path

    def test_c_dose_and_d13_layouts(self):
        _, _, result, sha = synth.prep_for("b_c", 1)
        c = synth.plan("c", 1, prep=(result, sha))
        b = synth.plan("b", 1, prep=(result, sha))
        self.assertEqual((b["prep_sha256"], b["pools"]), (c["prep_sha256"], c["pools"]))
        self.assertEqual(b["decks"][0]["cards"], c["decks"][0]["cards"])
        self.assertEqual([c["decks"][k]["build"] for k in range(3)], [0, 1, None])
        for j in range(4):
            gx, gy = c["games"][2 * j], c["games"][2 * j + 1]
            want = deal.seed(["test", "game", "v1", 1, 0, 0, j])
            self.assertEqual((gx["game_seed"], gy["game_seed"], gx["episode"]), (want, want, j))
            self.assertEqual(gx["decks"].index(0), j % 2)
        self.assertEqual(c["decks"][2]["dealt"]["key"], ["cube_deal_common_v1", "test", 1, 0, 0])
        self.assertEqual(c["decks"][2]["builder"]["pin"], c["decks"][2]["dealt"])
        pool = c["pools"][0]
        self.assertFalse(set(c["decks"][2]["cards"]) & set(pool))
        _, _, dres, dsha = synth.prep_for("dose", 1)
        dose = synth.plan("dose", 1, mark={"kind": "dose", "label": "pods-512", "ordinal": 0}, prep=(dres, dsha))
        self.assertEqual([dose["decks"][k]["build"] for k in (0, 1)], [0, 0])
        self.assertEqual({c_["role"] for c_ in dose["chairs"]}, {"fixture"})
        self.assertEqual(dose["paired_builds"][0]["kind"], "current_initial")
        _, _, r13, s13 = synth.prep_for("d13", 1)
        d13 = synth.plan("d13", 1, mark={"kind": "pilot", "label": "p1", "ordinal": 0}, prep=(r13, s13))
        kinds = [(p["q"], p["kind"], p["deck_x"] % 6, p["deck_y"] % 6) for p in d13["paired_builds"][:3]]
        self.assertEqual(kinds, [(0, "current_initial", 0, 1), (1, "k1_last_b1_last", 2, 0),
                                 (2, "candidate_stated", 2, 3)])
        firsts = [p["games_x"][0] for p in d13["paired_builds"]]
        self.assertEqual(firsts, sorted(firsts))
        g = d13["games"][2 * (12 * 0 + 4 * 2 + 2 * 1 + 1)]
        self.assertEqual(g["game_seed"], deal.seed(["test", "game", "v1", 1, 0, 1, 1, 2]))
        self.assertEqual([d13["decks"][k]["build"] for k in range(6)], [0, 0, 0, 1, None, None])

    def test_prep_requests(self):
        req = synth.prep_request("b_c", 0)
        self.assertEqual(list(req), list(read_plans.PREP_REQUEST_KEYS))
        self.assertEqual([(b["row"], b["build"]) for b in req["builds"][:2]], [("candidate", 0), ("stated", 1)])
        self.assertEqual(synth.prep_request("dose_panel", 0)["builds"], [])
        self.assertEqual(len(synth.prep_request("d13", 0)["builds"]), 32)
        real = read_plans.build_prep_request("b_c", "read-gate-pools", 0, synth.PINS, dict(synth.UNIFORM),
                                             {"candidate": dict(synth.UNIFORM)})
        self.assertEqual(real["seeds"]["draft_seed"], deal.seed(["read-gate-pools", "draft", "v1", 0]))

    def test_common_opponent_game_seed_and_unchanged_deal_py(self):
        self.assertEqual(deal_common.common_game_seed("read-gate-c", "v1", 3, 5, 0, 2),
                         deal.seed(["read-gate-c", "game", "v1", 3, 5, 0, 2]))
        self.assertEqual(deal_common.common_game_seed("panel-k1", "v1", 3, 5, 1, 0, 2),
                         deal.seed(["panel-k1", "game", "v1", 3, 5, 1, 0, 2]))
        text = (CUBE_V1_DIR / "deal.py").read_bytes().replace(b"\r\n", b"\n")
        self.assertEqual(hashlib.sha256(text).hexdigest(), DEAL_PY_SHA256)

    def test_common_opponent_independent_key_deck_golden_and_pin_checks(self):
        pool_state = deal.validate_pool(synth.POOL[0])
        constants = deal.validate_constants(synth.CONSTANTS)
        pools = synth.draft_pools(deal.seed(["dose-d8", "draft", "v1", 0]))
        seat_pool = pools[0]
        key = deal_common.common_key("dose-d8", 0, 0, 0)
        self.assertEqual(key, ["cube_deal_common_v1", "dose-d8", 0, 0, 0])

        # Independent recomputation using only the 5-item A6.17 prefix (no extra derived seed).
        basic_ids = set(pool_state["basic_id_by_color"].values())
        excluded = {cid for cid in seat_pool if cid not in basic_ids}

        def rank(prefix, item_key):
            return hashlib.sha256(json.dumps(prefix + [item_key], separators=(",", ":")).encode("utf-8")).digest()

        colors = sorted(list(deal.COLORS), key=lambda c: (rank(key + ["colors"], c), c))[:2]
        eligible_spells = [
            c for c in pool_state["spells"]
            if c["card_id"] not in excluded and set(c["colors"]).issubset(set(colors))
        ]
        spells = sorted(eligible_spells, key=lambda c: (rank(key + ["spells"], c["card_id"]), c["card_id"]))[:23]
        eligible_lands = [c for c in pool_state["nonbasic_lands"] if c["card_id"] not in excluded]
        lands = sorted(eligible_lands, key=lambda c: (rank(key + ["nonbasic_lands"], c["card_id"]), c["card_id"]))[:3]
        basics = [pool_state["basic_id_by_color"][colors[0]]] * 7 + [pool_state["basic_id_by_color"][colors[1]]] * 7
        expected_deck = sorted([c["card_id"] for c in spells] + [c["card_id"] for c in lands] + basics)

        expected_golden = [
            1, 5, 7, 11, 14, 20, 21, 22, 31, 32, 42, 46, 48, 50, 60, 192, 195, 196, 200, 216,
            228, 229, 235, 344, 348, 357, 1000, 1000, 1000, 1000, 1000, 1000, 1000,
            1003, 1003, 1003, 1003, 1003, 1003, 1003,
        ]
        self.assertEqual(expected_deck, expected_golden)
        self.assertEqual(
            hashlib.sha256(json.dumps(expected_deck, separators=(",", ":")).encode("utf-8")).hexdigest(),
            "563418680e0b22fb8c14b4aecc4aa5da53e3b33c33f2586799d07dcb1f021dde",
        )

        all_ids = {c["card_id"] for k in ("spells", "nonbasic_lands", "basics") for c in synth.POOL[0][k]}
        index_doc = {"cards": [{"card_id": cid} for cid in sorted(all_ids)]}
        table_doc = {"cards": [{"card_id": cid, "name": f"c{cid}", "vector": [0]} for cid in sorted(all_ids)]}
        dealt = deal_common.deal_common_opponent(
            pool_state, constants, seat_pool, "dose-d8", 0, 0, 0,
            card_index=index_doc, cube_table=table_doc,
        )
        self.assertEqual(dealt["mainboard"], expected_golden)
        dealt_nonbasics = [cid for cid in dealt["mainboard"] if cid not in basic_ids]
        self.assertEqual(len(dealt_nonbasics), 26)
        self.assertFalse(set(dealt_nonbasics) & excluded)

        # Removing one dealt nonbasic from the pinned index or table must fail.
        drop_cid = dealt_nonbasics[0]
        bad_index = {"cards": [{"card_id": cid} for cid in sorted(all_ids - {drop_cid})]}
        bad_table = {"cards": [{"card_id": cid} for cid in sorted(all_ids - {drop_cid})]}
        with self.assertRaisesRegex(ValueError, "card_index"):
            deal_common.deal_common_opponent(
                pool_state, constants, seat_pool, "dose-d8", 0, 0, 0,
                card_index=bad_index, cube_table=table_doc,
            )
        with self.assertRaisesRegex(ValueError, "cube_table"):
            deal_common.deal_common_opponent(
                pool_state, constants, seat_pool, "dose-d8", 0, 0, 0,
                card_index=index_doc, cube_table=bad_table,
            )

        # The old extra-seed prefix produces a different deck.
        old_prefix = key + [deal.seed(key)]
        old_colors = sorted(list(deal.COLORS), key=lambda c: (rank(old_prefix + ["colors"], c), c))[:2]
        old_elig = [c for c in pool_state["spells"]
                    if c["card_id"] not in excluded and set(c["colors"]).issubset(set(old_colors))]
        old_spells = sorted(old_elig, key=lambda c: (rank(old_prefix + ["spells"], c["card_id"]), c["card_id"]))[:23]
        old_lands = sorted(eligible_lands,
                           key=lambda c: (rank(old_prefix + ["nonbasic_lands"], c["card_id"]), c["card_id"]))[:3]
        old_basics = ([pool_state["basic_id_by_color"][old_colors[0]]] * 7
                      + [pool_state["basic_id_by_color"][old_colors[1]]] * 7)
        old_deck = sorted([c["card_id"] for c in old_spells] + [c["card_id"] for c in old_lands] + old_basics)
        self.assertNotEqual(old_deck, expected_golden)

        # Plan check enforces pinned card_index / cube_table hashes and nonbasic membership.
        idx_sha = read_plans.doc_sha256(index_doc)
        tbl_sha = read_plans.doc_sha256(table_doc)
        pins = {**synth.PINS, "card_index": idx_sha, "cube_table": tbl_sha}
        req = read_plans.build_prep_request("b_c", "test", 0, pins, dict(synth.UNIFORM),
                                            {"candidate": dict(synth.UNIFORM)})
        req_sha = read_plans.doc_sha256(req)
        res = synth.prep_result(req, req_sha)
        res_sha = read_plans.doc_sha256(res)
        plan = read_plans.build_plan("c", "test", 0, pins=pins, pilot=synth.PILOT, picker=dict(synth.UNIFORM),
                                     builders={"candidate": dict(synth.UNIFORM)}, prep=(res, res_sha),
                                     card_index=(index_doc, idx_sha), cube_table=(table_doc, tbl_sha), **synth.DEALER)
        read_plans.check_plan(plan, (res, res_sha), card_index=(index_doc, idx_sha),
                              cube_table=(table_doc, tbl_sha), **synth.DEALER)
        with self.assertRaisesRegex(read_plans.PlanError, "card_index"):
            read_plans.check_plan(plan, (res, res_sha), card_index=(index_doc, "0" * 64),
                                  cube_table=(table_doc, tbl_sha), **synth.DEALER)

    def test_live_common_opponent_requires_card_index_and_cube_table_and_cli_wires_them(self):
        # Live common-opponent planning and panel assembly refuse missing card_index / cube_table.
        _, _, dres, dsha = synth.prep_for("dose", 0, namespace="dose-d8")
        mark0 = {"kind": "dose", "label": "pods-512", "ordinal": 0}
        with self.assertRaisesRegex(read_plans.PlanError, "card_index"):
            read_plans.build_plan("dose", "dose-d8", 0, pins=synth.LIVE_PINS, pilot=synth.PILOT,
                                  picker=None, builders={"current": dict(synth.UNIFORM),
                                                         "initial": dict(synth.UNIFORM)},
                                  mark=mark0, prep=(dres, dsha), **synth.DEALER)
        with self.assertRaisesRegex(read_plans.PlanError, "card_index"):
            read_plans.build_plan("dose", "dose-d8", 0, pins=synth.LIVE_PINS, pilot=synth.PILOT,
                                  picker=None, builders={"current": dict(synth.UNIFORM),
                                                         "initial": dict(synth.UNIFORM)},
                                  mark=mark0, prep=(dres, dsha), card_index=synth.LIVE_CARD_INDEX, **synth.DEALER)

        # CLI read_plans.main wires --card-index and --cube-table and rejects missing nonbasic or omitted flags.
        with tempfile.TemporaryDirectory() as tmp:
            tdir = Path(tmp)
            spec_path = tdir / "spec.json"
            prep_path = tdir / "prep.json"
            pool_path = tdir / "pool.json"
            const_path = tdir / "constants.json"
            idx_path = tdir / "card_index.json"
            tbl_path = tdir / "cube_table.json"
            out_path = tdir / "plan.json"
            spec_path.write_bytes(read_plans.serialize({
                "family": "dose", "namespace": "dose-d8", "unit": 0, "pins": synth.LIVE_PINS,
                "pilot": synth.PILOT, "picker": None,
                "builders": {"current": dict(synth.UNIFORM), "initial": dict(synth.UNIFORM)},
                "mark": mark0,
            }).encode("utf-8"))
            prep_path.write_bytes(read_plans.serialize(dres).encode("utf-8"))
            pool_path.write_bytes(read_plans.serialize(synth.POOL[0]).encode("utf-8"))
            const_path.write_bytes(read_plans.serialize(synth.CONST[0]).encode("utf-8"))
            idx_path.write_bytes(read_plans.serialize(synth.LIVE_CARD_INDEX[0]).encode("utf-8"))
            tbl_path.write_bytes(read_plans.serialize(synth.LIVE_CUBE_TABLE[0]).encode("utf-8"))

            with self.assertRaisesRegex(read_plans.PlanError, "card_index"):
                read_plans.main(["plan", "--spec", str(spec_path), "--prep", str(prep_path),
                                 "--pool", str(pool_path), "--constants", str(const_path), "--out", str(out_path)])
            rc = read_plans.main(["plan", "--spec", str(spec_path), "--prep", str(prep_path),
                                  "--pool", str(pool_path), "--constants", str(const_path),
                                  "--card-index", str(idx_path), "--cube-table", str(tbl_path),
                                  "--out", str(out_path)])
            self.assertEqual(rc, 0)
            built_plan = json.loads(out_path.read_text(encoding="utf-8"))
            self.assertEqual(built_plan["namespace"], "dose-d8")

            # Dropping a dealt nonbasic from card_index fails in read_plans.main even when pins match the reduced file.
            basic_ids = {c["card_id"] for c in synth.POOL[0]["basics"]}
            drop_cid = next(cid for cid in built_plan["decks"][2]["cards"] if cid not in basic_ids)
            bad_idx = {"cards": [c for c in synth.LIVE_CARD_INDEX[0]["cards"] if c["card_id"] != drop_cid]}
            bad_idx_bytes = read_plans.serialize(bad_idx).encode("utf-8")
            bad_idx_path = tdir / "bad_index.json"
            bad_idx_path.write_bytes(bad_idx_bytes)
            bad_pins = {**synth.LIVE_PINS, "card_index": hashlib.sha256(bad_idx_bytes).hexdigest()}
            bad_spec_path = tdir / "bad_spec.json"
            bad_spec_path.write_bytes(read_plans.serialize({
                "family": "dose", "namespace": "dose-d8", "unit": 0, "pins": bad_pins,
                "pilot": synth.PILOT, "picker": None,
                "builders": {"current": dict(synth.UNIFORM), "initial": dict(synth.UNIFORM)},
                "mark": mark0,
            }).encode("utf-8"))
            with self.assertRaisesRegex(ValueError, "card_index"):
                read_plans.main(["plan", "--spec", str(bad_spec_path), "--prep", str(prep_path),
                                 "--pool", str(pool_path), "--constants", str(const_path),
                                 "--card-index", str(bad_idx_path), "--cube-table", str(tbl_path),
                                 "--out", str(out_path)])

            # CLI readers.main wires --card-index and --cube-table through open_plan_set.
            _, outs = synth.write_prep_set(tdir, "b_c", "b_c", "fx-b_c-shared-d0-prep-0-6", range(0, 6), DECL_SHA,
                                           reserved=[0, 6], pins=synth.LIVE_PINS)
            c_plans = [synth.plan("c", u, prep=outs[u], pins=synth.LIVE_PINS, **synth.LIVE_INDEX_ARGS)
                       for u in range(2)]
            c_dir = synth.write_plan_set(
                tdir, "c", "fx-c-shared-d0-look-1", c_plans, DECL_SHA,
                executions=[{"records": {p["pod_index"]: synth.record(p, lambda pl, gm, ps: "draw")
                                         for p in c_plans}}],
                look=1, units=[0, 2], reserved=[0, 6], prep_set="test/v1/fx-b_c-shared-d0-prep-0-6/exec-0",
            )
            rep_out = tdir / "rep.json"
            self.assertEqual(readers.main([
                "--declaration", str(DECLARATION), "--read", "c", "--look", "1",
                "--sets", str(c_dir), "--pool", str(pool_path), "--constants", str(const_path),
                "--card-index", str(idx_path), "--cube-table", str(tbl_path), "--out", str(rep_out),
            ]), 0)
            with self.assertRaisesRegex(read_sets.SetError, "card_index"):
                readers.main([
                    "--declaration", str(DECLARATION), "--read", "c", "--look", "1",
                    "--sets", str(c_dir), "--pool", str(pool_path), "--constants", str(const_path),
                    "--card-index", str(bad_idx_path), "--cube-table", str(tbl_path), "--out", str(rep_out),
                ])

    def test_fixture_set_names(self):
        for ok in ("fx-c-shared-d0", "fx-aa-mirrored-halt-d15", "fx-b-edge-undefined-d0-look-1",
                   "fx-dose-provisional-d0-prep-panel"):
            read_plans.check_set_name("test", ok.split("-")[1], ok)
        for bad in ("fx-c-swap-d0", "fx-c-shared-d2", "look-1", "fx-c-shared-d0-look-3"):
            with self.assertRaises(read_plans.PlanError):
                read_plans.check_set_name("test", "c", bad)
        read_plans.check_set_name("read-gate-c", "c", "look-1-restart-2")
        with self.assertRaises(read_plans.PlanError):
            read_plans.check_set_name("read-gate-c", "c", "fx-c-shared-d0")


# ---------------------------------------------------------------------------------------------
# Changes 1, 2, 6, 8, 9, 14, 16: records


class RecordTests(unittest.TestCase):
    def setUp(self):
        _, _, self.result, self.sha = synth.prep_for("b_c", 0)
        self.plan = synth.plan("c", 0, prep=(self.result, self.sha))
        self.rec = synth.record(self.plan, by_slot({0: "win", 1: "loss"}))

    def _rejects(self, mutate, pattern):
        bad = copy.deepcopy(self.rec)
        mutate(bad)
        with self.assertRaisesRegex(pod_record.RecordError, pattern):
            pod_record.validate_record(bad)

    def test_v2_shape(self):
        self.assertEqual(self.rec["schema"], "mtg-kernel-cube-pod-record/v2")
        self.assertTrue(pod_record.is_read_pod(self.rec))
        self._rejects(lambda r: r.update(schema="mtg-kernel-cube-pod-record/v1"), "unknown version")
        self._rejects(lambda r: r.update(interfaces="m4-interfaces/v1"), "unknown version")
        self._rejects(lambda r: r["seeds"].pop("pod_seed_namespace"), "missing")
        self._rejects(lambda r: r["games"][0].update(seat_rule="coin"), "schedule")
        self._rejects(lambda r: r["decks"][2]["builder"].update(pin=None), "builder pin")
        self._rejects(lambda r: r["paired_builds"][0].pop("q"), "missing")
        self._rejects(lambda r: r["paired_builds"].reverse(), "ordered")
        self._rejects(lambda r: r.update(namespace="client-pods"), "reserved")
        self._rejects(lambda r: r["caps"].update(max_replays=1), "max_replays")
        self._rejects(lambda r: r["caps"].pop("max_stack_objects"), "missing")
        self._rejects(lambda r: r["caps"].update(max_stack_objects=64), "max_stack_objects")
        self._rejects(lambda r: r["caps"].update(max_stack_objects=129), "max_stack_objects")
        self._rejects(lambda r: r["caps"].update(max_stack_objects=1 << 32), "max_stack_objects")
        self._rejects(lambda r: r["caps"].update(max_stack_objects=None), "max_stack_objects")
        self._rejects(lambda r: r.__setitem__("caps", {
            "max_decisions": 16384, "max_physical_decisions": 16384,
            "max_policy_steps": 32768, "watchdog_seconds": 600,
            "max_stack_objects": 128, "max_replays": 0,
        }), "order")
        self._rejects(lambda r: r["games"][0].update(scores=None), "scores")

    def test_d5_format_v3_read_and_training_fixtures(self):
        # Positive D5-format read pod fixture (exact frozen key order with max_stack_objects 128).
        d5_read = copy.deepcopy(self.rec)
        self.assertEqual(
            list(d5_read["caps"].keys()),
            ["max_decisions", "max_physical_decisions", "max_policy_steps", "max_stack_objects",
             "watchdog_seconds", "max_replays"],
        )
        self.assertEqual(d5_read["caps"]["max_stack_objects"], 128)
        pod_record.validate_record(d5_read)
        read_plans.check_record_against_plan(d5_read, self.plan)

        # Positive D5-format training pod fixture (no read seed source fields, max_replays 3, max_stack_objects 128).
        train_seeds = pod_record.pod_seeds("shakedown-pods", "b1", 0)
        pools = synth.draft_pools(train_seeds["draft_seed"])
        cards0 = synth.build_from_pool(pools[0], "candidate")
        cards1 = synth.build_from_pool(pools[1], "candidate")
        g_seed, seats = seeds_v1.pairing_seats(train_seeds["pod_seed"], 0)
        aseeds = seeds_v1.attempt_seeds(g_seed)
        d5_train = {
            "schema": "mtg-kernel-cube-pod-record/v2",
            "interfaces": "m4-interfaces/v2",
            "use": "shakedown",
            "namespace": "shakedown-pods",
            "run": "b1",
            "pod_index": 0,
            "seeds": train_seeds,
            "pins": dict(synth.PINS),
            "pilot": dict(synth.PILOT),
            "caps": {
                "max_decisions": 16384,
                "max_physical_decisions": 16384,
                "max_policy_steps": 32768,
                "max_stack_objects": 128,
                "watchdog_seconds": 600,
                "max_replays": 3,
            },
            "draft": synth.draft_record(train_seeds["draft_seed"], pools),
            "chairs": [
                {"chair": c, "role": "older" if c in (1, 6) else "learning", "picker": dict(synth.UNIFORM),
                 "pick_rows": []}
                for c in range(8)
            ],
            "decks": [
                {"deck": 0, "source": "build", "chair": 0, "build": 0, "builder": dict(synth.UNIFORM),
                 "build_seed": seeds_v1.build_seed(train_seeds["draft_seed"], 0, 0),
                 "cards": cards0, "build_rows": synth.uniform_build_rows(cards0, pools[0]),
                 "dealt": None},
                {"deck": 1, "source": "build", "chair": 1, "build": 0, "builder": dict(synth.UNIFORM),
                 "build_seed": seeds_v1.build_seed(train_seeds["draft_seed"], 1, 0),
                 "cards": cards1, "build_rows": synth.uniform_build_rows(cards1, pools[1]),
                 "dealt": None},
            ],
            "games": [{
                "game": 0, "group": 0, "role": "reference", "pairing": 0,
                "decks": [seats[0], seats[1]], "chairs": [seats[0], seats[1]],
                "seat_rule": "coin", "game_seed": g_seed, "episode": 0,
                "attempts": [{
                    "attempt": 0, "deal_seed": aseeds["deal_seed"], "env_seed": aseeds["env_seed"],
                    "action_seed": aseeds["action_seed"], "decisions": 42, "physical_decisions": 42,
                    "policy_steps": 84,
                    "end": {"kind": "natural", "winner_seat": seats[0], "reason": "life_zero"},
                    "halted": False, "stop_kind": None, "last_action_source": None,
                }],
                "scores": [1, -1],
            }],
            "paired_builds": [],
            "halts": [],
            "status": "complete",
            "stop": None,
        }
        pod_record.validate_record(d5_train)

        # v2 manifest is rejected by read_sets.load_manifest.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            opened = c_sets(root, "shared", by_slot({0: "win"}, arm=0))
            man_path = opened[0]["set_dir"] / "manifest.json"
            doc = json.loads(man_path.read_bytes().decode("utf-8"))
            self.assertEqual(doc["schema"], "mtg-kernel-cube-read-plan/v3")
            doc["schema"] = "mtg-kernel-cube-read-plan/v2"
            man_path.write_bytes(read_plans.serialize(doc).encode("utf-8"))
            with self.assertRaisesRegex(read_sets.SetError, "schema"):
                read_sets.load_manifest(opened[0]["set_dir"])

    def test_sampler_v2(self):
        row = {"kind": "pick", "sampler": "softmax-cdf-splitmix64-v2", "options": [3, 5], "chosen": 5,
               "stream_values": [9], "logits_bits": [1, 2], "p_chosen_bits": 4, "value_bits": 7}
        pod_record._validate_row(row, "row", "pick")
        row["sampler"] = "softmax-cdf-splitmix64-v1"
        with self.assertRaises(pod_record.RecordError):
            pod_record._validate_row(row, "row", "pick")

    def test_record_against_plan(self):
        read_plans.check_record_against_plan(self.rec, self.plan)
        for mutate, pattern in ((lambda r: r["pins"].update(binary="0" * 64), "pins"),
                                (lambda r: r["games"][3]["attempts"][0].update(env_seed=1), "attempt seeds"),
                                (lambda r: r["chairs"][2].update(role="fixture"), "chair"),
                                (lambda r: r["decks"][0].update(build=1), "build"),
                                (lambda r: r["games"].pop(), "games")):
            bad = copy.deepcopy(self.rec)
            mutate(bad)
            with self.assertRaisesRegex(pod_record.RecordError, pattern):
                read_plans.check_record_against_plan(bad, self.plan)

    def test_reversed_scores_and_wrong_winner_seat_are_rejected(self):
        # Dealer game (chairs = [s, None]): seats are [0, 1]. Reversing scores alone must fail.
        bad = copy.deepcopy(self.rec)
        g0 = bad["games"][0]
        self.assertEqual(g0["scores"], [1, -1])
        g0["scores"] = [-1, 1]
        with self.assertRaisesRegex(pod_record.RecordError, "winner_seat"):
            pod_record.validate_record(bad)

        # Out-of-seat winner_seat on a dealer game must fail.
        bad_seat = copy.deepcopy(self.rec)
        bad_seat["games"][0]["attempts"][0]["end"]["winner_seat"] = 2
        with self.assertRaisesRegex(pod_record.RecordError, "winner_seat"):
            pod_record.validate_record(bad_seat)

        # Chair-vs-chair game (read (a)): D5 maps winner_seat through chairs [c0, c1].
        a_plan = synth.plan("a", 0, mirror=0)
        a_rec = synth.record(a_plan, lambda p, g, pos: "win")
        pod_record.validate_record(a_rec)
        read_plans.check_record_against_plan(a_rec, a_plan)
        # Pick a game with chairs not equal to [0, 1] and reverse its scores.
        target_idx = next(i for i, g in enumerate(a_rec["games"]) if set(g["chairs"]) != {0, 1})
        bad_a = copy.deepcopy(a_rec)
        orig = bad_a["games"][target_idx]["scores"]
        bad_a["games"][target_idx]["scores"] = [-orig[0], -orig[1]]
        with self.assertRaisesRegex(pod_record.RecordError, "winner_seat"):
            pod_record.validate_record(bad_a)

        # A winner_seat matching neither chair of the pairing must fail.
        bad_a_seat = copy.deepcopy(a_rec)
        chairs = set(bad_a_seat["games"][target_idx]["chairs"])
        other = next(c for c in range(8) if c not in chairs)
        bad_a_seat["games"][target_idx]["attempts"][0]["end"]["winner_seat"] = other
        with self.assertRaisesRegex(pod_record.RecordError, "winner_seat"):
            pod_record.validate_record(bad_a_seat)

    def test_wrong_build_seed_is_rejected_in_prep_and_dose_records(self):
        req, req_sha, res, res_sha = synth.prep_for("dose", 0)
        read_plans.check_prep_result(res, req, req_sha)
        draft_seed = req["seeds"]["draft_seed"]

        # Mutate one dose arm's build_seed to another well-typed u64 (build 1 instead of build 0).
        bad_prep = copy.deepcopy(res)
        wrong_stream = seeds_v1.build_seed(draft_seed, 0, 1)
        self.assertNotEqual(wrong_stream, bad_prep["builds"][1]["build_seed"])
        bad_prep["builds"][1]["build_seed"] = wrong_stream
        with self.assertRaisesRegex(read_plans.PlanError, "build_seed"):
            read_plans.check_prep_result(bad_prep, req, req_sha)

        # Same mutation on one dose arm inside a dose pod record.
        dose_plan = synth.plan("dose", 0, mark={"kind": "dose", "label": "pods-512", "ordinal": 0},
                               prep=(res, res_sha))
        dose_rec = synth.record(dose_plan, by_slot({0: "win"}))
        pod_record.validate_record(dose_rec)
        read_plans.check_record_against_plan(dose_rec, dose_plan)
        bad_dose = copy.deepcopy(dose_rec)
        bad_dose["decks"][1]["build_seed"] = wrong_stream
        with self.assertRaisesRegex(pod_record.RecordError, "build_seed"):
            pod_record.validate_record(bad_dose)
        with self.assertRaisesRegex(pod_record.RecordError, "build_seed"):
            read_plans.check_record_against_plan(bad_dose, dose_plan)

        # D5's non-null derived build_seed on a stated build is accepted; a wrong u64 is rejected.
        c_rec = copy.deepcopy(self.rec)
        c_rec["decks"][1]["build_seed"] = seeds_v1.build_seed(c_rec["seeds"]["draft_seed"], 0, 1)
        pod_record.validate_record(c_rec)
        read_plans.check_record_against_plan(c_rec, self.plan)
        c_rec["decks"][1]["build_seed"] = seeds_v1.build_seed(c_rec["seeds"]["draft_seed"], 0, 0)
        with self.assertRaisesRegex(pod_record.RecordError, "build_seed"):
            pod_record.validate_record(c_rec)

    def test_read_halt_is_one_attempt_null_score_and_a_ledger_row(self):
        rec = synth.record(self.plan, by_slot({2: "halt"}, arm=0))
        halted = [g for g in rec["games"] if g["scores"] is None]
        self.assertEqual(len(halted), 8)
        self.assertEqual(len(rec["halts"]), 8)
        self.assertEqual(rec["status"], "complete")

    def test_client_pods(self):
        rec = copy.deepcopy(self.rec)
        rec.update(use="client", namespace="client-pods")
        rec["seeds"] = {"draft_seed": None, "pod_seed": None, "league_seed": None}
        rec["draft"] = None
        pod_record.validate_record(rec)
        rec["namespace"] = "test"
        with self.assertRaisesRegex(pod_record.RecordError, "client-pods"):
            pod_record.validate_record(rec)


# ---------------------------------------------------------------------------------------------
# Read plan v2 change 1: prep acceptance


class PrepAcceptanceTests(unittest.TestCase):
    def _set(self, executions):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        executions[0].setdefault("accept", False)
        set_dir, _ = synth.write_prep_set(Path(tmp.name), "size_c", "size_c", "fx-size_c-edge-reject-d0-prep-0-2",
                                          range(0, 2), DECL_SHA, executions=executions)
        return set_dir

    def test_lowest_passing_execution_is_accepted(self):
        self.assertEqual(read_sets.accept_prep_set(self._set([{"ended": "complete"}]))["execution"], 0)
        two = self._set([{"ended": "complete"}, {"ended": "complete"}])
        self.assertEqual(read_sets.accept_prep_set(two)["execution"], 0)

    def test_a_died_execution_then_a_retry(self):
        # exec-0 died with unit 0 complete (marker) and no receipt; exec-1 completes identically.
        set_dir = self._set([{"ended": "interrupted", "units": [0], "receipt": False}, {"ended": "complete"}])
        self.assertEqual(read_sets.accept_prep_set(set_dir)["execution"], 1)

    def test_marker_rules(self):
        for spec, pattern in (({"no_marker": [1]}, "marker"), ({"marker_sha": {0: "0" * 64}}, "marker")):
            set_dir = self._set([{"ended": "complete", **spec}])
            with self.assertRaisesRegex(read_sets.SetError, pattern):
                read_sets.accept_prep_set(set_dir)
        set_dir = self._set([{"ended": "complete", "no_marker": [1]}, {"ended": "complete"}])
        self.assertEqual(read_sets.accept_prep_set(set_dir)["execution"], 1)

    def test_boolean_marker_unit_is_rejected_and_excluded_from_retry_identity(self):
        # Python treats False == 0 and True == 1, but a marker unit must be an exact u64 integer.
        set_dir = self._set([{"ended": "complete"}])
        for unit, bad_unit in ((0, False), (1, True)):
            marker_path = set_dir / "exec-0" / "done" / f"{unit}.json"
            original = marker_path.read_bytes()
            doc = json.loads(original.decode("utf-8"))
            doc["unit"] = bad_unit
            marker_path.write_bytes(read_plans.serialize(doc).encode("utf-8"))
            self.assertIsNone(read_sets._valid_marker(set_dir / "exec-0", unit))
            with self.assertRaisesRegex(read_sets.SetError, "marker"):
                read_sets.accept_prep_set(set_dir)
            marker_path.write_bytes(original)

        # In a died attempt, a tampered output whose marker uses boolean False/True for unit 0/1
        # is malformed and ignored (discarded as partial), so a clean retry passes instead of
        # tripping a determinism failure.
        def tamper(result):
            result["builds"][0]["build_seed"] = 12345

        retry_dir = self._set([
            {"ended": "interrupted", "units": [0, 1], "receipt": False, "tamper": {0: tamper, 1: tamper}},
            {"ended": "complete"},
        ])
        for unit, bad_unit in ((0, False), (1, True)):
            marker_path = retry_dir / "exec-0" / "done" / f"{unit}.json"
            doc = json.loads(marker_path.read_bytes().decode("utf-8"))
            doc["unit"] = bad_unit
            marker_path.write_bytes(read_plans.serialize(doc).encode("utf-8"))
        self.assertEqual(read_sets.accept_prep_set(retry_dir)["execution"], 1)

    def test_byte_identity_across_attempts(self):
        def tamper(result):
            result["builds"][0]["build_seed"] = 12345
        set_dir = self._set([{"ended": "interrupted", "units": [0], "receipt": False},
                             {"ended": "complete", "tamper": {0: tamper}}])
        with self.assertRaisesRegex(read_sets.SetError, "determinism failure"):
            read_sets.accept_prep_set(set_dir)

    def test_partial_output_without_marker_is_discarded(self):
        def tamper(result):
            result["builds"][0]["build_seed"] = 12345
        set_dir = self._set([{"ended": "interrupted", "units": [0], "receipt": False, "no_marker": [0],
                              "tamper": {0: tamper}}, {"ended": "complete"}])
        self.assertEqual(read_sets.accept_prep_set(set_dir)["execution"], 1)


# ---------------------------------------------------------------------------------------------
# Change 15: executions, allocation, restarts, panels


class ExecutionTests(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.plans = [synth.plan("a", u, mirror=m) for u in range(2) for m in (0, 1)]

    def _write(self, case, executions):
        return synth.write_plan_set(self.root, "a", f"fx-a-{case}-d0-look-1", self.plans, DECL_SHA,
                                    executions=executions, look=1, units=[0, 2], reserved=[0, 6])

    def _records(self, watchdog_game=None, upto=None):
        def outcome(p, g, pos):
            if watchdog_game is not None and p["pod_index"] == 1 and g["game"] == watchdog_game:
                return ("halt", dict(synth.WATCHDOG_END), None, 50)
            return "draw"
        out = {}
        for p in self.plans:
            if upto is not None and p["pod_index"] == 1:
                out[1] = synth.record(p, outcome, games=upto + 1, status="stopped")
            elif upto is not None and p["pod_index"] > 1:
                continue
            else:
                out[p["pod_index"]] = synth.record(p, outcome)
        return out

    def test_first_execution(self):
        opened = read_sets.open_plan_set(self._write("swap", [{"records": self._records()}]), DECL_SHA)
        self.assertEqual((opened["execution"], opened["void"]), (0, None))

    def test_watchdog_rerun(self):
        e0 = self._records(watchdog_game=3)
        good = self._write("planted", [{"records": e0}, {"records": self._records(), "entry_expired": [[1, 3]]}])
        opened = read_sets.open_plan_set(good, DECL_SHA)
        self.assertEqual((opened["execution"], opened["void"]), (1, None))
        self.assertEqual(opened["expired"], {"0": [[1, 3]], "1": []})
        hang = self._write("halt-rows", [{"records": e0}, {"records": self._records(watchdog_game=3, upto=3),
                                                           "ended": "watchdog_stop", "entry_expired": [[1, 3]]}])
        self.assertIn("hang", read_sets.open_plan_set(hang, DECL_SHA)["void"])
        other = self._write("edge-cut", [{"records": e0}, {"records": self._records(watchdog_game=5, upto=5),
                                                           "ended": "watchdog_stop", "entry_expired": [[1, 3]]}])
        self.assertIn("infrastructure", read_sets.open_plan_set(other, DECL_SHA)["void"])
        pending = self._write("edge-empty", [{"records": e0}])
        self.assertIn("rerun has not run", read_sets.open_plan_set(pending, DECL_SHA)["void"])
        report = readers.run_read(DECL, DECL_SHA, "a", 1, [read_sets.open_plan_set(hang, DECL_SHA)])
        self.assertEqual(report["verdict"], "void")

    def test_interruption_and_refusals(self):
        recs = self._records()
        recs = {0: recs[0], 1: synth.record(self.plans[1], lambda p, g, pos: "draw", games=4, status="stopped")}
        opened = read_sets.open_plan_set(self._write("swap", [{"records": recs, "ended": "interrupted"}]), DECL_SHA)
        self.assertIn("interrupted", opened["void"])
        with self.assertRaisesRegex(read_sets.SetError, "no watchdog halt"):
            read_sets.open_plan_set(self._write("planted", [{"records": self._records()},
                                                            {"records": self._records()}]), DECL_SHA)
        set_dir = self._write("halt-rows", [{"records": self._records()}])
        (set_dir / "exec-0" / "records" / "pod-0.json").write_text("{}", encoding="utf-8")
        with self.assertRaisesRegex(read_sets.SetError, "hash"):
            read_sets.open_plan_set(set_dir, DECL_SHA)
        set_dir = self._write("edge-reject", [{"records": self._records()}])
        with self.assertRaisesRegex(read_sets.SetError, "declaration"):
            read_sets.open_plan_set(set_dir, "1" * 64)

    def test_stopped_paired_read_reaches_technical_recovery(self):
        # D5 copies the full plan's paired_builds (all 8 chairs, referencing games 0..63) into a
        # stopped (c) pod that holds only a played prefix (games 0..3).
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            prep_name = "fx-b_c-shared-d0-prep-0-6"
            _, outs = synth.write_prep_set(root, "b_c", "b_c", prep_name, range(0, 6), DECL_SHA, reserved=[0, 6])
            prep_set = f"test/v1/{prep_name}/exec-0"
            plans = [synth.plan("c", u, prep=outs[u]) for u in range(2)]

            # Case 1: mid-pod process death after 4 games of pod 0 (pod 1 unplayed).
            stopped_mid = synth.record(plans[0], lambda p, g, pos: "draw", games=4, status="stopped")
            self.assertEqual(len(stopped_mid["games"]), 4)
            self.assertEqual(len(stopped_mid["paired_builds"]), 8)
            self.assertEqual(stopped_mid["stop"]["game"], 4)
            self.assertEqual(stopped_mid["halts"], [])
            read_plans.check_record_against_plan(stopped_mid, plans[0])

            # Negative checks: complete record missing games fails; wrong stop metadata or pair metadata fails.
            bad_complete = copy.deepcopy(stopped_mid)
            bad_complete["status"] = "complete"
            bad_complete["stop"] = None
            with self.assertRaisesRegex(pod_record.RecordError, "not in the record"):
                pod_record.validate_record(bad_complete)

            bad_stop = copy.deepcopy(stopped_mid)
            bad_stop["stop"]["game"] = 5
            with self.assertRaisesRegex(pod_record.RecordError, "stop.game"):
                read_plans.check_record_against_plan(bad_stop, plans[0])

            for label, mutate_att in (
                ("wrong-stop-kind", lambda s: s["attempts"][0].update(stop_kind="decision_cap")),
                ("non-null-source", lambda s: s["attempts"][0].update(last_action_source=42)),
                ("extra-attempt", lambda s: s["attempts"].append(dict(s["attempts"][0]))),
                ("wrong-deal-seed", lambda s: s["attempts"][0].update(deal_seed=1)),
            ):
                with self.subTest(bad_interrupted_stop=label):
                    bad_att = copy.deepcopy(stopped_mid)
                    mutate_att(bad_att["stop"])
                    with self.assertRaisesRegex(pod_record.RecordError, "process_death"):
                        read_plans.check_record_against_plan(bad_att, plans[0])

            bad_pair = copy.deepcopy(stopped_mid)
            bad_pair["paired_builds"] = bad_pair["paired_builds"][:1]
            with self.assertRaisesRegex(pod_record.RecordError, "paired_builds"):
                read_plans.check_record_against_plan(bad_pair, plans[0])

            set_int = synth.write_plan_set(
                root, "c", "fx-c-shared-d0-look-1", plans, DECL_SHA,
                executions=[{"records": {0: stopped_mid}, "ended": "interrupted"}],
                look=1, units=[0, 2], reserved=[0, 6], prep_set=prep_set,
            )
            opened_int = read_sets.open_plan_set(set_int, DECL_SHA, pool=synth.POOL, constants=synth.CONST)
            self.assertIn("interrupted", opened_int["void"])
            rep_int = readers.run_read(DECL, DECL_SHA, "c", 1, [opened_int])
            self.assertEqual(rep_int["verdict"], "void")
            self.assertIn("interrupted", rep_int["void"])
            self.assertEqual(rep_int["next"], "technical recovery (A6.7)")
            self.assertEqual(rep_int["games"], {"played": 4, "completed": 4, "halted": 0, "planned": 128, "unplayed": 124})
            self.assertIn("a2_3", rep_int)
            self.assertIn("a2_1_warning", rep_int)
            self.assertEqual(rep_int["a2_3"]["by_arm"], {
                "candidate": {"completed": 2, "halted": 0},
                "stated": {"completed": 2, "halted": 0},
            })
            self.assertNotIn("statistics", rep_int)

            # On the same interrupted set, (b') scopes denominators and A2.3 to stated-arm games only.
            rep_bp_int = readers.run_read(DECL, DECL_SHA, "b_prime", 1, [opened_int])
            self.assertEqual(rep_bp_int["verdict"], "void")
            self.assertEqual(rep_bp_int["games"], {"played": 2, "completed": 2, "halted": 0, "planned": 64, "unplayed": 62})
            self.assertEqual(rep_bp_int["a2_3"]["by_arm"], {"stated": {"completed": 2, "halted": 0}})
            self.assertFalse(rep_bp_int["a2_1_warning"])
            self.assertNotIn("statistics", rep_bp_int)

            # Case 2: second watchdog expiry (e = 1, watchdog_stop) on pod 0 after 4 games (game 3 expired).
            def wd_at(target_game):
                def outcome(p, g, pos):
                    if p["pod_index"] == 0 and g["game"] == target_game:
                        return ("halt", dict(synth.WATCHDOG_END), None, 50)
                    return "draw"
                return outcome

            e0_recs = {p["pod_index"]: synth.record(p, wd_at(3)) for p in plans}
            e1_same = {0: synth.record(plans[0], wd_at(3), games=4, status="stopped")}
            self.assertEqual(len(e1_same[0]["paired_builds"]), 8)
            self.assertEqual(e1_same[0]["stop"]["game"], 3)
            self.assertEqual(len(e1_same[0]["halts"]), 1)

            set_hang = synth.write_plan_set(
                root, "c", "fx-c-halt-candidate-d0-look-1", plans, DECL_SHA,
                executions=[{"records": e0_recs},
                            {"records": e1_same, "ended": "watchdog_stop", "entry_expired": [[0, 3]]}],
                look=1, units=[0, 2], reserved=[0, 6], prep_set=prep_set,
            )
            opened_hang = read_sets.open_plan_set(set_hang, DECL_SHA, pool=synth.POOL, constants=synth.CONST)
            rep_hang = readers.run_read(DECL, DECL_SHA, "c", 1, [opened_hang])
            self.assertEqual(rep_hang["verdict"], "void")
            self.assertIn("hang", rep_hang["void"])
            self.assertEqual(rep_hang["next"], "technical recovery (A6.7)")
            self.assertEqual(rep_hang["games"], {"played": 4, "completed": 3, "halted": 1, "planned": 128, "unplayed": 124})
            self.assertNotIn("statistics", rep_hang)

            # Case 3: second watchdog expiry on a different game (game 5) -> void infrastructure failure, attempt ends.
            e1_diff = {0: synth.record(plans[0], wd_at(5), games=6, status="stopped")}
            set_infra = synth.write_plan_set(
                root, "c", "fx-c-halt-stated-d0-look-1", plans, DECL_SHA,
                executions=[{"records": e0_recs},
                            {"records": e1_diff, "ended": "watchdog_stop", "entry_expired": [[0, 3]]}],
                look=1, units=[0, 2], reserved=[0, 6], prep_set=prep_set,
            )
            opened_infra = read_sets.open_plan_set(set_infra, DECL_SHA, pool=synth.POOL, constants=synth.CONST)
            rep_infra = readers.run_read(DECL, DECL_SHA, "c", 1, [opened_infra])
            self.assertEqual(rep_infra["verdict"], "void")
            self.assertIn("infrastructure failure", rep_infra["void"])
            self.assertEqual(rep_infra["next"], "attempt_ends")
            self.assertEqual(rep_infra["games"], {"played": 6, "completed": 5, "halted": 1, "planned": 128, "unplayed": 122})
            self.assertNotIn("statistics", rep_infra)


class AllocationTests(unittest.TestCase):
    def test_looks_restarts_and_shared_prefix(self):
        look1 = {"look": 1, "units": [0, 4], "reserved": [0, 12], "pools_with": None, "plan_set_sha256": "x" * 64,
                 "set": "look-1", "replaces": None, "retired": []}
        read_sets.check_look_allocation(look1, 1, 4)
        look2 = {"look": 2, "units": [4, 12], "reserved": None, "pools_with": "x" * 64}
        read_sets.check_look_allocation(look2, 2, 4, look1)
        with self.assertRaises(read_sets.SetError):
            read_sets.check_look_allocation({**look2, "units": [4, 8]}, 2, 4, look1)
        with self.assertRaises(read_sets.SetError):
            read_sets.check_look_allocation({**look1, "reserved": [0, 8]}, 1, 4)
        restart = {"set": "look-1-restart-1", "replaces": "look-1", "units": [12, 16], "retired": [[4, 12]]}
        read_sets.check_restart(look1, restart)
        with self.assertRaises(read_sets.SetError):
            read_sets.check_restart(look1, {**restart, "units": [8, 12]})
        prep = {"set": "prep-0-12", "units": [0, 12], "reserved": [0, 12], "replaces": None, "retired": []}
        new = {"set": "prep-0-12-restart-1", "units": [12, 24], "reserved": [12, 24], "replaces": "prep-0-12",
               "retired": []}
        read_sets.check_shared_prefix_restart([prep], new, 4, 3)
        with self.assertRaises(read_sets.SetError):
            read_sets.check_shared_prefix_restart([prep], {**new, "reserved": [12, 21]}, 4, 3)

    def test_look2_policy_or_pin_mismatch_rejected_by_run_read(self):
        other_builder = {"kind": "learned", "pin": "1" * 64}
        other_picker = {"kind": "learned", "pin": "2" * 64}
        other_pilot = {"kind": "checkpoint", "pin": "3" * 64, "decoding": "greedy"}
        other_pins = {**synth.PINS, "binary": "4" * 64}

        for label, look2_kw, pattern in (
            ("candidate-builder", {"builders": {"candidate": other_builder}}, "builders"),
            ("picker", {"picker": other_picker}, "picker"),
            ("pilot", {"pilot": other_pilot}, "pilot"),
            ("pins", {"pins": other_pins}, "pins"),
        ):
            with self.subTest(mismatch=label), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                n = 2
                prep1_name = "fx-b_c-shared-d0-prep-0-6"
                _, outs1 = synth.write_prep_set(root, "b_c", "b_c", prep1_name, range(0, 6), DECL_SHA,
                                                reserved=[0, 6])
                prep_set1 = f"test/v1/{prep1_name}/exec-0"
                plans1 = [synth.plan("c", u, prep=outs1[u]) for u in range(n)]
                set1 = synth.write_plan_set(
                    root, "c", "fx-c-shared-d0-look-1", plans1, DECL_SHA,
                    executions=[{"records": {p["pod_index"]: synth.record(p, lambda p, g, pos: "draw") for p in plans1}}],
                    look=1, units=[0, n], reserved=[0, 3 * n], prep_set=prep_set1,
                )
                opened1 = read_sets.open_plan_set(set1, DECL_SHA, pool=synth.POOL, constants=synth.CONST)

                # Look 2 built with mutated policy/pins (reusing prep for pilot/pins or a prep with mutated picker/builder).
                if "picker" in look2_kw or "builders" in look2_kw:
                    prep2_name = "fx-b_c-halt-candidate-d0-prep-0-6"
                    prep_pins = look2_kw.get("pins", synth.PINS)
                    prep_picker = look2_kw.get("picker", dict(synth.UNIFORM))
                    prep_builders = look2_kw.get("builders", {"candidate": dict(synth.UNIFORM)})
                    _, outs2 = synth.write_prep_set(root, "b_c", "b_c", prep2_name, range(0, 6), DECL_SHA,
                                                    reserved=[0, 6], pins=prep_pins, picker=prep_picker,
                                                    builders=prep_builders)
                    prep_set2 = f"test/v1/{prep2_name}/exec-0"
                    plans2 = [
                        read_plans.build_plan("c", "test", u, pins=prep_pins, pilot=synth.PILOT,
                                              picker=prep_picker, builders=prep_builders, prep=outs2[u],
                                              **synth.DEALER)
                        for u in range(n, 3 * n)
                    ]
                else:
                    prep_set2 = prep_set1
                    plans2 = [
                        read_plans.build_plan("c", "test", u, pins=look2_kw.get("pins", synth.PINS),
                                              pilot=look2_kw.get("pilot", synth.PILOT),
                                              picker=dict(synth.UNIFORM), builders={"candidate": dict(synth.UNIFORM)},
                                              prep=outs1[u], **synth.DEALER)
                        for u in range(n, 3 * n)
                    ]
                set2 = synth.write_plan_set(
                    root, "c", "fx-c-shared-d0-look-2", plans2, DECL_SHA,
                    executions=[{"records": {p["pod_index"]: synth.record(p, lambda p, g, pos: "draw") for p in plans2}}],
                    look=2, units=[n, 3 * n], pools_with=opened1["manifest"]["plan_set_sha256"],
                    prep_set=prep_set2,
                )
                opened2 = read_sets.open_plan_set(set2, DECL_SHA, pool=synth.POOL, constants=synth.CONST)
                with self.assertRaisesRegex(read_sets.SetError, f"{pattern}|prep_set"):
                    readers.run_read(DECL, DECL_SHA, "c", 2, [opened1, opened2])
                if prep_set2 != prep_set1:
                    # Even if prep_set matched, check_look_bindings still rejects the policy mismatch itself.
                    opened2_same_prep = copy.deepcopy(opened2)
                    opened2_same_prep["manifest"]["prep_set"] = prep_set1
                    with self.assertRaisesRegex(read_sets.SetError, pattern):
                        readers.run_read(DECL, DECL_SHA, "c", 2, [opened1, opened2_same_prep])

    def test_declared_bindings_enforced_on_prep_plans_and_run_read(self):
        other_pins = {**synth.PINS, "binary": "5" * 64}
        other_pilot = {"kind": "checkpoint", "pin": "6" * 64, "decoding": "greedy"}
        other_picker = {"kind": "learned", "pin": "7" * 64}
        other_builders = {"candidate": {"kind": "learned", "pin": "8" * 64}}
        def_builders = {"candidate": dict(synth.UNIFORM)}

        req, _, res, res_sha = synth.prep_for("b_c", 0)
        read_plans.check_prep_request(req, expected_pins=synth.PINS, expected_picker=synth.UNIFORM,
                                      expected_builders=def_builders)
        with self.assertRaisesRegex(read_plans.PlanError, "pins"):
            read_plans.check_prep_request(req, expected_pins=other_pins)
        with self.assertRaisesRegex(read_plans.PlanError, "picker"):
            read_plans.check_prep_request(req, expected_picker=other_picker)
        with self.assertRaisesRegex(read_plans.PlanError, "builders"):
            read_plans.check_prep_request(req, expected_builders=other_builders)

        plan = synth.plan("c", 0, prep=(res, res_sha))
        read_plans.check_plan(plan, prep=(res, res_sha), **synth.DEALER,
                              expected_pins=synth.PINS, expected_pilot=synth.PILOT,
                              expected_picker=synth.UNIFORM, expected_builders=def_builders)
        with self.assertRaisesRegex(read_plans.PlanError, "pilot"):
            read_plans.check_plan(plan, prep=(res, res_sha), **synth.DEALER, expected_pilot=other_pilot)

        with tempfile.TemporaryDirectory() as tmp:
            opened = c_sets(Path(tmp), "shared", lambda p, g, pos: "draw")
            with self.assertRaisesRegex(read_sets.SetError, "pins"):
                read_sets.open_plan_set(opened[0]["set_dir"], DECL_SHA, pool=synth.POOL, constants=synth.CONST,
                                        expected_pins=other_pins)
            with self.assertRaisesRegex(readers.ReadError, "pilot"):
                readers.run_read(DECL, DECL_SHA, "c", 1, opened, expected_pilot=other_pilot)
            with self.assertRaisesRegex(readers.ReadError, "picker"):
                readers.run_read(DECL, DECL_SHA, "c", 1, opened, expected_picker=other_picker)
            with self.assertRaisesRegex(readers.ReadError, "builders"):
                readers.run_read(DECL, DECL_SHA, "c", 1, opened, expected_builders=other_builders)

    def test_shared_bc_prefix_mismatches_rejected_by_run_read(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            n = 2
            prep_name = "fx-b_c-shared-d0-prep-0-6"
            _, outs = synth.write_prep_set(root, "b_c", "b_c", prep_name, range(0, 6), DECL_SHA, reserved=[0, 6])
            prep_set = f"test/v1/{prep_name}/exec-0"

            b_plans = [synth.plan("b", u, prep=outs[u]) for u in range(n)]
            b_dir = synth.write_plan_set(
                root, "b", "fx-b-score-d0-look-1", b_plans, DECL_SHA,
                executions=[{"records": {p["pod_index"]: synth.record(p, lambda p, g, pos: "draw") for p in b_plans}}],
                look=1, units=[0, n], reserved=[0, 3 * n], prep_set=prep_set,
            )
            c_plans = [synth.plan("c", u, prep=outs[u]) for u in range(n)]
            c_dir = synth.write_plan_set(
                root, "c", "fx-c-shared-d0-look-1", c_plans, DECL_SHA,
                executions=[{"records": {p["pod_index"]: synth.record(p, lambda p, g, pos: "draw") for p in c_plans}}],
                look=1, units=[0, n], reserved=[0, 3 * n], prep_set=prep_set,
            )
            b_open = read_sets.open_plan_set(b_dir, DECL_SHA, pool=synth.POOL, constants=synth.CONST)
            c_open = read_sets.open_plan_set(c_dir, DECL_SHA, pool=synth.POOL, constants=synth.CONST)

            # Matching shared prefix passes from both (b) and (c) entry points.
            readers.run_read(DECL, DECL_SHA, "c", 1, [c_open], peer_sets=[b_open])
            readers.run_read(DECL, DECL_SHA, "b", 1, [b_open], peer_sets=[c_open])

            # Separate accepted prep set for (b) -> rejected on prep_set mismatch.
            prep_alt = "fx-b_c-halt-candidate-d0-prep-0-6"
            _, outs_alt = synth.write_prep_set(root, "b_c", "b_c", prep_alt, range(0, 6), DECL_SHA, reserved=[0, 6])
            b_alt_plans = [synth.plan("b", u, prep=outs_alt[u]) for u in range(n)]
            b_alt_dir = synth.write_plan_set(
                root, "b", "fx-b-halt-pool-d0-look-1", b_alt_plans, DECL_SHA,
                executions=[{"records": {p["pod_index"]: synth.record(p, lambda p, g, pos: "draw")
                                         for p in b_alt_plans}}],
                look=1, units=[0, n], reserved=[0, 3 * n], prep_set=f"test/v1/{prep_alt}/exec-0",
            )
            b_alt_open = read_sets.open_plan_set(b_alt_dir, DECL_SHA, pool=synth.POOL, constants=synth.CONST)
            with self.assertRaisesRegex(read_sets.SetError, "prep_set"):
                readers.run_read(DECL, DECL_SHA, "c", 1, [c_open], peer_sets=[b_alt_open])

            # Mutated prep_sha256, pools, or candidate cards on shared unit -> rejected through run_read.
            for label, mutate, pattern in (
                ("prep_sha256", lambda s: s["plans"][0].update(prep_sha256="0" * 64), "prep_sha256"),
                ("pools", lambda s: s["plans"][0]["pools"][0].reverse(), "pools"),
                ("candidate_cards", lambda s: s["plans"][0]["decks"][0]["cards"].reverse(), "candidate cards"),
            ):
                with self.subTest(bc_mismatch=label):
                    bad_b = copy.deepcopy(b_open)
                    mutate(bad_b)
                    with self.assertRaisesRegex(read_sets.SetError, pattern):
                        readers.run_read(DECL, DECL_SHA, "c", 1, [c_open], peer_sets=[bad_b])

    def test_shared_prefix_restart_checked_from_run_read(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            n = 2  # DECL has n_b = 2, n_c = 2 -> 3 * max(n_b, n_c) = 6
            prep0_name = "fx-b_c-shared-d0-prep-0-6"
            _, outs0 = synth.write_prep_set(root, "b_c", "b_c", prep0_name, range(0, 6), DECL_SHA, reserved=[0, 6])
            c0_plans = [synth.plan("c", u, prep=outs0[u]) for u in range(n)]
            synth.write_plan_set(
                root, "c", "fx-c-shared-d0-look-1", c0_plans, DECL_SHA,
                executions=[{"records": {0: synth.record(c0_plans[0], lambda p, g, pos: "draw", games=4,
                                                         status="stopped")}, "ended": "interrupted"}],
                look=1, units=[0, n], reserved=[0, 6], prep_set=f"test/v1/{prep0_name}/exec-0",
            )

            # Valid shared-prefix restart at f' = 6, reserved [6, 12).
            prep1_name = f"{prep0_name}-restart-1"
            _, outs1 = synth.write_prep_set(root, "b_c", "b_c", prep1_name, range(6, 12), DECL_SHA,
                                            reserved=[6, 12], replaces=prep0_name, retired=[])
            c1_plans = [synth.plan("c", u, prep=outs1[u]) for u in range(6, 6 + n)]
            c1_dir = synth.write_plan_set(
                root, "c", "fx-c-shared-d0-look-1-restart-1", c1_plans, DECL_SHA,
                executions=[{"records": {p["pod_index"]: synth.record(p, lambda p, g, pos: "draw") for p in c1_plans}}],
                look=1, units=[6, 8], reserved=[6, 12], prep_set=f"test/v1/{prep1_name}/exec-0",
                replaces="fx-c-shared-d0-look-1", retired=[[2, 6]],
            )
            c1_open = read_sets.open_plan_set(c1_dir, DECL_SHA, pool=synth.POOL, constants=synth.CONST)
            rep = readers.run_read(DECL, DECL_SHA, "c", 1, [c1_open])
            self.assertEqual(rep["units"], {"kind": "cluster", "first": 6, "end": 8})

            # Bad f' (starting at 4 instead of 6) on b_c restart -> rejected by run_read.
            bad_prep = f"{prep0_name}-restart-2"
            _, outs_bad = synth.write_prep_set(root, "b_c", "b_c", bad_prep, range(4, 10), DECL_SHA,
                                               reserved=[4, 10], replaces=prep0_name, retired=[])
            c_bad_plans = [synth.plan("c", u, prep=outs_bad[u]) for u in range(4, 4 + n)]
            c_bad_dir = synth.write_plan_set(
                root, "c", "fx-c-shared-d0-look-1-restart-2", c_bad_plans, DECL_SHA,
                executions=[{"records": {p["pod_index"]: synth.record(p, lambda p, g, pos: "draw")
                                         for p in c_bad_plans}}],
                look=1, units=[4, 6], reserved=[4, 10], prep_set=f"test/v1/{bad_prep}/exec-0",
                replaces="fx-c-shared-d0-look-1", retired=[[2, 6]],
            )
            c_bad_open = read_sets.open_plan_set(c_bad_dir, DECL_SHA, pool=synth.POOL, constants=synth.CONST)
            with self.assertRaisesRegex(read_sets.SetError, "unreserved|f'"):
                readers.run_read(DECL, DECL_SHA, "c", 1, [c_bad_open])

            # Bad retired range on plan set restart -> rejected by run_read.
            bad_ret = copy.deepcopy(c1_open)
            bad_ret["manifest"]["retired"] = []
            with self.assertRaisesRegex(read_sets.SetError, "retires"):
                readers.run_read(DECL, DECL_SHA, "c", 1, [bad_ret])


class PanelTests(unittest.TestCase):
    def test_dose_panel_two_stages(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            base = "fx-dose-provisional-d0"
            stage1, outs = synth.write_prep_set(root, "dose_panel", "dose", f"{base}-prep-panel", range(0, 2), DECL_SHA)
            man1 = read_sets.load_manifest(stage1)
            self.assertIsNone(man1["panel_sha256"])  # prep-panel is exempt from pinned-panel checks
            panel = read_plans.assemble_panel("dose", "test", man1, 0, outs, pins=synth.PINS, builders=None,
                                              **synth.DEALER)
            panel_dir = root / "test" / "v1" / f"{base}-panel"
            panel_dir.mkdir(parents=True)
            text = read_plans.serialize(panel)
            (panel_dir / "panel.json").write_text(text, encoding="utf-8", newline="\n")
            digest = hashlib.sha256(text.encode()).hexdigest()
            prep_dir, pouts = synth.write_prep_set(root, "dose", "dose", f"{base}-prep-dose-0", range(0, 2), DECL_SHA,
                                                   panel_sha256=digest)
            read_sets.check_prep_dose_set(prep_dir, digest, pool=synth.POOL, constants=synth.CONST)
            mark = {"kind": "dose", "label": "pods-512", "ordinal": 0}
            plans = [synth.plan("dose", u, mark=mark, prep=pouts[u]) for u in range(2)]
            for p in plans:
                read_plans.check_plan_against_panel(p, panel)

            def outcome(p, g, pos):
                return "win" if g["game"] % 2 == 0 else ("halt" if g["game"] == 1 else "draw")
            set_dir = synth.write_plan_set(root, "dose", f"{base}-dose-0", plans, DECL_SHA,
                                           executions=[{"records": {p["pod_index"]: synth.record(p, outcome)
                                                                    for p in plans}}],
                                           units=[0, 2], panel_sha256=digest,
                                           prep_set=f"test/v1/{base}-prep-dose-0/exec-0")
            opened = read_sets.open_plan_set(set_dir, DECL_SHA, pool=synth.POOL, constants=synth.CONST,
                                             pinned_panel=digest)
            report = readers.run_read(decl_with(**{"dose.panel_sha256": digest}), DECL_SHA, "dose", 0, [opened])
            self.assertIsNone(report["verdict"])
            lo, mid, hi = (estimate(report, "current_minus_initial", s) for s in ("lower", "drop", "upper"))
            self.assertLess(lo, mid)
            self.assertLess(mid, hi)
            with self.assertRaisesRegex(read_sets.SetError, "pinned"):
                read_sets.open_plan_set(set_dir, DECL_SHA, pool=synth.POOL, constants=synth.CONST,
                                        pinned_panel="0" * 64)
            (panel_dir / "panel.json").write_text(text.replace('"run": "v1"', '"run": "v1" '), encoding="utf-8")
            with self.assertRaisesRegex(read_sets.SetError, "hash"):
                read_sets.open_plan_set(set_dir, DECL_SHA, pool=synth.POOL, constants=synth.CONST,
                                        pinned_panel=digest)

    def test_first_dose_timing_basics_bands_provenance_a23_and_wholly_halted_void(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            base = "fx-dose-provisional-d0"
            stage1, outs = synth.write_prep_set(root, "dose_panel", "dose", f"{base}-prep-panel", range(0, 2), DECL_SHA)
            man1 = read_sets.load_manifest(stage1)
            panel = read_plans.assemble_panel("dose", "test", man1, 0, outs, pins=synth.PINS, builders=None,
                                              **synth.DEALER)
            panel_dir = root / "test" / "v1" / f"{base}-panel"
            panel_dir.mkdir(parents=True)
            text = read_plans.serialize(panel)
            (panel_dir / "panel.json").write_text(text, encoding="utf-8", newline="\n")
            digest = hashlib.sha256(text.encode()).hexdigest()

            # Populate all 4 signed A6.17 basics bands across the 8 chairs (2 chairs = 16 games/pod per band).
            chair_basics = {
                (0, "current"): 10, (0, "initial"): 10, (1, "current"): 14, (1, "initial"): 14,  # 0-14
                (2, "current"): 15, (2, "initial"): 15, (3, "current"): 19, (3, "initial"): 19,  # 15-19
                (4, "current"): 20, (4, "initial"): 20, (5, "current"): 35, (5, "initial"): 35,  # 20-35
                (6, "current"): 36, (6, "initial"): 36, (7, "current"): 40, (7, "initial"): 40,  # 36-40
            }
            _, pouts = synth.write_prep_set(root, "dose", "dose", f"{base}-prep-dose-0", range(0, 2), DECL_SHA,
                                            panel_sha256=digest, basics_by_chair=chair_basics)
            mark = {"kind": "dose", "label": "pods-512", "ordinal": 0}
            plans = [synth.plan("dose", u, mark=mark, prep=pouts[u]) for u in range(2)]

            # Band cap hits per pod: 0-14 gets 1 cap hit (game 0), 15-19 gets 0, 20-35 gets 2 (games 32,33),
            # 36-40 gets 4 (games 48..51). Game 16 finishes with 9000 decisions (>8192 -> triggers a2_1_warning).
            recs = {}
            for p in plans:
                rec = synth.record(p, lambda pl, gm, ps: (
                    ("halt", {"kind": "decision_cap"}, 77, 16384)
                    if gm["game"] in (0, 32, 33, 48, 49, 50, 51)
                    else ("win" if gm["game"] % 2 == 0 else "draw")
                ))
                rec["games"][16]["attempts"][0]["decisions"] = 9000
                recs[p["pod_index"]] = pod_record.validate_record(rec)

            # Without timing side files, first dose (ordinal 0) must be refused.
            no_timing_dir = synth.write_plan_set(
                root, "dose", f"{base}-dose-0", plans, DECL_SHA,
                executions=[{"records": recs, "timing": False}],
                units=[0, 2], panel_sha256=digest, prep_set=f"test/v1/{base}-prep-dose-0/exec-0",
            )
            opened_no_t = read_sets.open_plan_set(no_timing_dir, DECL_SHA, pool=synth.POOL, constants=synth.CONST,
                                                  pinned_panel=digest)
            with self.assertRaisesRegex(readers.ReadError, "timing side files"):
                readers.run_read(decl_with(**{"dose.panel_sha256": digest}), DECL_SHA, "dose", 0, [opened_no_t])

            # With timing side files (band-specific ms: 100, 200, 300, 400).
            def band_ms(pod_index, game):
                chair = game["game"] // 8
                return (chair // 2 + 1) * 100

            d0_sha = "a" * 64
            with_timing_dir = synth.write_plan_set(
                root, "dose", f"{base}-dose-0-restart-1", plans, DECL_SHA,
                executions=[{"records": recs, "timing": band_ms}],
                units=[0, 2], panel_sha256=digest, prep_set=f"test/v1/{base}-prep-dose-0/exec-0",
            )
            opened = read_sets.open_plan_set(with_timing_dir, DECL_SHA, pool=synth.POOL, constants=synth.CONST,
                                             pinned_panel=digest)
            report = readers.run_read(decl_with(**{"dose.panel_sha256": digest}), DECL_SHA, "dose", 0, [opened],
                                      d0_sha256=d0_sha)
            self.assertIsNone(report["verdict"])
            self.assertEqual(report["provenance"], {
                "label": "shakedown, under random play, provisional pins",
                "d0_sha256": d0_sha,
                "panel_sha256": digest,
                "panel_run": "v1",
                "mark": mark,
                "panel_pairing_note": "marks on different panels are not paired",
            })
            bands = report["first_dose_basics_bands"]["bands"]
            self.assertEqual(list(bands.keys()), ["0-14", "15-19", "20-35", "36-40"])
            for label, want_hits, want_ms in (("0-14", 2, 100), ("15-19", 0, 200),
                                              ("20-35", 4, 300), ("36-40", 8, 400)):
                b = bands[label]
                self.assertEqual(b["games"], 32)
                self.assertEqual(b["decision_cap_hits"], want_hits)
                self.assertEqual(exact(b["decision_cap_hit_rate"]), F(want_hits, 32))
                self.assertEqual(b["timing"]["count"], 32)
                self.assertEqual(exact(b["timing"]["mean_ms"]), F(want_ms))
                self.assertEqual(exact(b["timing"]["p50_ms"]), F(want_ms))
                self.assertEqual(b["timing"]["max_ms"], want_ms)
            self.assertEqual(report["denominators"]["current_minus_initial"], {
                "units_planned": 2, "units_defined": 2, "units_omitted": 0,
                "pools_planned": 16, "pools_complete": 16, "pools_empty": 0,
                "games_planned": 128, "games_completed": 114, "games_halted": 14,
            })
            self.assertEqual(report["a2_3"]["by_stop_kind"], {"decision_cap": 14})
            self.assertEqual(report["a2_3"]["by_card_charged"], {"77": 14})
            self.assertEqual(report["a2_3"]["completed_over_8192"], 2)
            self.assertTrue(report["a2_1_warning"])

            # Corrupted timing side file (missing a game line) is refused by open_plan_set.
            tpath = with_timing_dir / "exec-0" / "timing" / "pod-0.jsonl"
            tlines = tpath.read_text(encoding="utf-8").splitlines()
            tpath.write_text("\n".join(tlines[:-1]) + "\n", encoding="utf-8")
            with self.assertRaisesRegex(read_sets.SetError, "timing/pod-0.jsonl"):
                read_sets.open_plan_set(with_timing_dir, DECL_SHA, pool=synth.POOL, constants=synth.CONST,
                                        pinned_panel=digest)

            # Wholly halted dose returns verdict 'void' (not None).
            all_halt_recs = {p["pod_index"]: synth.record(p, lambda pl, gm, ps: "halt") for p in plans}
            halt_dir = synth.write_plan_set(
                root, "dose", f"{base}-dose-0-restart-2", plans, DECL_SHA,
                executions=[{"records": all_halt_recs, "timing": band_ms}],
                units=[0, 2], panel_sha256=digest, prep_set=f"test/v1/{base}-prep-dose-0/exec-0",
            )
            opened_h = read_sets.open_plan_set(halt_dir, DECL_SHA, pool=synth.POOL, constants=synth.CONST,
                                               pinned_panel=digest)
            rep_h = readers.run_read(decl_with(**{"dose.panel_sha256": digest}), DECL_SHA, "dose", 0, [opened_h],
                                     d0_sha256=d0_sha)
            self.assertEqual((rep_h["verdict"], rep_h["void"]), ("void", "all games halted"))
            self.assertTrue(rep_h["drop_interval_undefined"])
            self.assertEqual(rep_h["games"], {"played": 128, "completed": 0, "halted": 128})

            # Technically interrupted dose preserves provenance, A2.3 breakdown and A2.1 warning without statistics.
            int_dose_rec = synth.record(
                plans[0],
                lambda pl, gm, ps: ("halt", {"kind": "decision_cap"}, 77, 16384) if gm["game"] == 0 else "draw",
                games=4, status="stopped",
            )
            int_dose_rec["games"][1]["attempts"][0]["decisions"] = 9000
            int_dose_rec = pod_record.validate_record(int_dose_rec)
            int_dose_dir = synth.write_plan_set(
                root, "dose", f"{base}-dose-0-restart-3", plans, DECL_SHA,
                executions=[{"records": {0: int_dose_rec}, "ended": "interrupted"}],
                units=[0, 2], panel_sha256=digest, prep_set=f"test/v1/{base}-prep-dose-0/exec-0",
            )
            opened_int_dose = read_sets.open_plan_set(int_dose_dir, DECL_SHA, pool=synth.POOL, constants=synth.CONST,
                                                      pinned_panel=digest)
            rep_int_dose = readers.run_read(decl_with(**{"dose.panel_sha256": digest}), DECL_SHA, "dose", 0,
                                            [opened_int_dose], d0_sha256=d0_sha)
            self.assertEqual(rep_int_dose["verdict"], "void")
            self.assertEqual(rep_int_dose["next"], "technical recovery (A6.7)")
            self.assertEqual(rep_int_dose["games"], {"played": 4, "completed": 3, "halted": 1, "planned": 128, "unplayed": 124})
            self.assertEqual(rep_int_dose["provenance"], {
                "label": "shakedown, under random play, provisional pins",
                "d0_sha256": d0_sha,
                "panel_sha256": digest,
                "panel_run": "v1",
                "mark": mark,
                "panel_pairing_note": "marks on different panels are not paired",
            })
            self.assertEqual(rep_int_dose["a2_3"]["by_stop_kind"], {"decision_cap": 1})
            self.assertEqual(rep_int_dose["a2_3"]["by_card_charged"], {"77": 1})
            self.assertEqual(rep_int_dose["a2_3"]["completed_over_8192"], 1)
            self.assertTrue(rep_int_dose["a2_1_warning"])
            self.assertNotIn("statistics", rep_int_dose)


# ---------------------------------------------------------------------------------------------
# A6 statistics: reads (b), (c), (b')


class ReadBTests(unittest.TestCase):
    def test_score_five_eighths_in_either_position(self):
        unit = [[F(1), F(1), F(1, 2), F(0)]] * 8
        self.assertEqual(readers.stat_pools(unit, "drop"), F(5, 8))
        with tempfile.TemporaryDirectory() as tmp:
            opened = c_sets(Path(tmp), "score", by_slot({0: "win", 1: "win", 2: "draw", 3: "loss"}), family="b")
            report = readers.run_read(DECL, DECL_SHA, "b", 1, opened)
            self.assertEqual(estimate(report, "candidate_score"), F(5, 8))
            positions = {g["decks"].index(0) for g in opened[0]["records"][0]["games"][:4]}
            self.assertEqual(positions, {0, 1})

    def test_halt_pool_lower_completion_voids_the_win(self):
        outcome = by_slot({0: "win", 1: "win", 2: "loss", 3: "halt"})
        with tempfile.TemporaryDirectory() as tmp:
            opened = c_sets(Path(tmp), "halt-pool", outcome, family="b", look2=True)
            report = readers.run_read(DECL, DECL_SHA, "b", 1, opened[:1])
            self.assertEqual([estimate(report, "candidate_score", s) for s in readers.SCORINGS],
                             [F(2, 3), F(1, 2), F(3, 4)])
            self.assertEqual(report["verdicts_by_scoring"], {"drop": "win", "lower": "loss", "upper": "win"})
            self.assertEqual((report["verdict"], report["void"], report["next"]),
                             ("extend", "verdicts differ", "extension"))
            self.assertEqual(report["extension"], {"fresh_units": [2, 6], "pooled_units": 6})
            second = readers.run_read(DECL, DECL_SHA, "b", 2, opened)
            self.assertEqual((second["verdict"], second["next"]), ("void", "attempt_ends"))
            self.assertEqual(second["units"], {"kind": "cluster", "first": 0, "end": 6})


class ReadCTests(unittest.TestCase):
    def test_candidate_halt_fixture(self):
        outcome = by_slot({3: "halt"}, arm=0)
        with tempfile.TemporaryDirectory() as tmp:
            opened = c_sets(Path(tmp), "halt-candidate", outcome, look2=True)
            report = readers.run_read(DECL, DECL_SHA, "c", 1, opened[:1])
            self.assertEqual([estimate(report, "candidate_minus_stated", s) for s in readers.SCORINGS],
                             [F(0), F(-1, 8), F(1, 8)])
            self.assertEqual(report["verdicts_by_scoring"]["drop"], "win")
            self.assertEqual((report["verdict"], report["void"]), ("extend", "verdicts differ"))
            self.assertEqual(readers.run_read(DECL, DECL_SHA, "c", 2, opened)["verdict"], "void")
            bprime = readers.run_read(DECL, DECL_SHA, "b_prime", 1, opened[:1])
            self.assertEqual([estimate(bprime, "stated_score", s) for s in readers.SCORINGS], [F(1, 2)] * 3)
            self.assertIsNone(bprime["verdict"])
            # (b') top-level denominator counts only stated-arm games (32 per draft = 64, not 128).
            self.assertEqual(bprime["games"], {"played": 64, "completed": 64, "halted": 0})
            self.assertEqual(bprime["denominators"]["stated_score"], {
                "units_planned": 2, "units_defined": 2, "units_omitted": 0,
                "pools_planned": 16, "pools_complete": 16, "pools_empty": 0,
                "games_planned": 64, "games_completed": 64, "games_halted": 0,
            })
            self.assertEqual(list(bprime["a2_3"]["by_arm"].keys()), ["stated"])

    def test_b_prime_wholly_halted_stated_arm_is_void(self):
        # Even when all candidate-arm games complete, all stated-arm games halting makes (b') void.
        def outcome(p, g, pos):
            return "halt" if g["game"] % 2 == 1 else "win"
        with tempfile.TemporaryDirectory() as tmp:
            opened = c_sets(Path(tmp), "edge-all-halted", outcome)
            bprime = readers.run_read(DECL, DECL_SHA, "b_prime", 1, opened)
            self.assertEqual((bprime["verdict"], bprime["void"]), ("void", "all games halted"))
            self.assertTrue(bprime["drop_interval_undefined"])
            self.assertEqual(bprime["games"], {"played": 64, "completed": 0, "halted": 64})
            self.assertEqual(bprime["denominators"]["stated_score"]["games_planned"], 64)
            self.assertEqual(bprime["denominators"]["stated_score"]["games_halted"], 64)

    def test_stated_halt_reverse_sign_disposition(self):
        """The reverse-sign fixture (A6.20, (c)): a stated-arm halt. The completions swap sides (a
        stated halt is a stated win in the lower completion), the gate is void because the three
        verdicts differ, and the signed disposition is the same as for a candidate halt: the first
        look schedules the extension and the second look ends the attempt."""
        outcome = by_slot({3: "halt"}, arm=1)
        with tempfile.TemporaryDirectory() as tmp:
            opened = c_sets(Path(tmp), "halt-stated", outcome, look2=True)
            report = readers.run_read(DECL, DECL_SHA, "c", 1, opened[:1])
            self.assertEqual([estimate(report, "candidate_minus_stated", s) for s in readers.SCORINGS],
                             [F(0), F(-1, 8), F(1, 8)])
            self.assertEqual(report["verdicts_by_scoring"], {"drop": "win", "lower": "loss", "upper": "win"})
            self.assertEqual((report["verdict"], report["next"]), ("extend", "extension"))
            second = readers.run_read(DECL, DECL_SHA, "c", 2, opened)
            self.assertEqual((second["verdict"], second["next"]), ("void", "attempt_ends"))
            bprime = readers.run_read(DECL, DECL_SHA, "b_prime", 1, opened[:1])
            self.assertEqual([estimate(bprime, "stated_score", s) for s in readers.SCORINGS],
                             [F(1, 2), F(3, 8), F(5, 8)])

    def test_undefined_first_look_interval_ends_the_attempt(self):
        """A (c) first look whose drop interval is undefined: draft 0 has every game halted, so some
        resamples hold no defined draft. The read is void and the attempt ends (signed decision 2)."""
        def outcome(p, g, pos):
            return "halt" if p["unit"] == 0 else "draw"
        with tempfile.TemporaryDirectory() as tmp:
            report = readers.run_read(DECL, DECL_SHA, "c", 1, c_sets(Path(tmp), "edge-undefined", outcome))
            self.assertIsNotNone(estimate(report, "candidate_minus_stated"))
            self.assertIsNone(interval(report, "candidate_minus_stated"))
            self.assertIsNotNone(interval(report, "candidate_minus_stated", "lower"))
            self.assertEqual((report["verdict"], report["void"], report["next"]),
                             ("void", "undefined drop interval", "attempt_ends"))
            self.assertEqual(report["pools"]["empty"], 8)

    def test_all_halted_and_equality_at_the_cut(self):
        with tempfile.TemporaryDirectory() as tmp:
            report = readers.run_read(DECL, DECL_SHA, "c", 1, c_sets(Path(tmp), "edge-all-halted",
                                                                     lambda p, g, pos: "halt"))
            self.assertEqual(report["verdict"], "void")
            self.assertIsNone(estimate(report, "candidate_minus_stated"))
        self.assertEqual(readers.verdict_of([F(-1, 50), F(1)], F(-1, 50)), "inconclusive")
        self.assertEqual(readers.verdict_of([F(-1), F(-1, 50)], F(-1, 50)), "inconclusive")
        self.assertEqual(readers.verdict_of([F(-1, 51), F(1)], F(-1, 50)), "win")

    def test_third_look_and_unrecorded_count(self):
        with self.assertRaisesRegex(readers.ReadError, "look 3"):
            readers.look_units(DECL["reads"]["c"], 3)
        with self.assertRaisesRegex(readers.ReadError, "not recorded"):
            readers.look_units(decl_with(**{"c.n": None})["reads"]["c"], 1)


# ---------------------------------------------------------------------------------------------
# A6 statistics: read (a), its A/A and companions


class ReadATests(unittest.TestCase):
    def test_planted_two_pair_candidate_row(self):
        def outcome(p, g, pos):
            if g["game"] % 2 == 1:
                return "draw"
            return "win" if p["unit"] == 1 or g["group"] < 4 else "loss"
        with tempfile.TemporaryDirectory() as tmp:
            opened = a_sets(Path(tmp), "a", "planted", outcome)
            report = readers.run_read(DECL, DECL_SHA, "a", 1, [opened])
        units = readers.units_a(opened["records"], opened["plans"], range(2))
        self.assertEqual([readers.stat_a_candidate(u, "drop") for u in units], [F(-1, 2), F(1)])
        self.assertEqual(estimate(report, "candidate_edge"), F(1, 4))
        self.assertEqual(estimate(report, "stated_edge"), F(0))
        self.assertEqual(estimate(report, "interaction"), F(1, 4))
        # The full 10,000-replicate histogram against an independent enumeration of ordered pairs.
        edges = [F(-1, 2), F(1)]
        counts = {}
        for b in range(10_000):
            pair = tuple(deal.seed(["bootstrap", "test", b, t]) % 2 for t in (0, 1))
            counts[pair] = counts.get(pair, 0) + 1
        values = sorted(v for pair, c in counts.items() for v in [(edges[pair[0]] + edges[pair[1]]) / 2] * c)
        want = [readers.percentile(values, F(1, 40)), readers.percentile(values, F(39, 40))]
        self.assertEqual(interval(report, "candidate_edge"), want)
        self.assertLess(want[0], want[1])
        self.assertEqual(len(counts), 4)

    def test_halt_rows_interaction_bounds(self):
        def outcome(p, g, pos):
            return "halt" if (p["unit"], p["mirror"], g["group"]) == (0, 0, 0) else "draw"
        with tempfile.TemporaryDirectory() as tmp:
            opened = a_sets(Path(tmp), "a", "halt-rows", outcome)
        unit = readers.units_a(opened["records"], opened["plans"], range(1))[0]
        self.assertEqual([readers.stat_a_interaction(unit, s) for s in readers.SCORINGS],
                         [F(0), F(-2, 32), F(2, 32)])
        self.assertEqual(readers.stat_a_candidate(unit, "lower"), F(-1, 32))

    def test_focal_label_swap_negates(self):
        unit = {"candidate": [F(1), F(0), F(1), F(1, 2)], "stated": [F(0), F(1, 2), F(1, 2), F(1, 2)]}
        swapped = {k: [1 - w for w in v] for k, v in unit.items()}
        for fn in (readers.stat_a_candidate, readers.stat_a_stated, readers.stat_a_interaction):
            self.assertEqual(fn(swapped, "drop"), -fn(unit, "drop"))


class AATests(unittest.TestCase):
    @staticmethod
    def symmetric(halt_in=None, source=None):
        def outcome(p, g, pos):
            r = synth.unit_draw("aa", p["unit"], g["game"])
            if halt_in is not None and (p["unit"], g["game"]) == (0, 4) and p["mirror"] in halt_in:
                src = source if (source is not None and p["mirror"] == 1) else 3
                return ("halt", {"kind": "decision_cap"}, src, 77)
            p0 = "draw" if r < 0.15 else ("p0" if r < 0.6 else "p1")
            if p0 == "draw":
                return "draw"
            return "win" if (p0 == "p0") == (pos == 0) else "loss"
        return outcome

    def _read(self, outcome, case):
        with tempfile.TemporaryDirectory() as tmp:
            opened = a_sets(Path(tmp), "aa", case, outcome, units=4, set_suffix="fixed", look=None)
            return readers.run_read(DECL, DECL_SHA, "aa", 1, [opened])

    def test_exact_symmetry_with_a_mirrored_halt(self):
        report = self._read(self.symmetric(halt_in=(0, 1)), "mirrored-halt")
        self.assertTrue(report["symmetry"]["symmetric"], report["symmetry"])
        self.assertEqual(report["verdict"], "symmetric")
        self.assertEqual(report["games"]["halted"], 2)
        for name in ("candidate_edge", "stated_edge", "interaction"):
            self.assertEqual(estimate(report, name), F(0))
            self.assertEqual(interval(report, name), [F(0), F(0)])
        # A6.12 pair-by-row completions: pair 0 has 2 halted candidate-row games (game 4 in both mirrors).
        self.assertEqual(len(report["pair_by_row"]), 4)
        p0 = report["pair_by_row"][0]
        self.assertEqual(p0["pair"], 0)
        self.assertEqual((p0["candidate"]["games_planned"], p0["candidate"]["games_completed"],
                          p0["candidate"]["games_halted"]), (32, 30, 2))
        self.assertEqual([exact(p0["candidate"][s]) for s in readers.SCORINGS], [F(0), F(-1, 16), F(1, 16)])
        self.assertEqual((p0["stated"]["games_planned"], p0["stated"]["games_completed"],
                          p0["stated"]["games_halted"]), (32, 32, 0))
        self.assertEqual([exact(p0["stated"][s]) for s in readers.SCORINGS], [F(0), F(0), F(0)])
        self.assertEqual([exact(p0["interaction"][s]) for s in readers.SCORINGS], [F(0), F(-1, 16), F(1, 16)])
        for idx in (1, 2, 3):
            row = report["pair_by_row"][idx]
            self.assertEqual(row["pair"], idx)
            self.assertEqual(row["candidate"]["games_completed"], 32)
            self.assertEqual([exact(row["candidate"][s]) for s in readers.SCORINGS], [F(0), F(0), F(0)])

    def test_unmirrored_halt_or_mismatched_halt_fails(self):
        report = self._read(self.symmetric(halt_in=(1,)), "unmirrored")
        self.assertEqual(report["verdict"], "failed")
        self.assertIn("unmirrored halt", report["symmetry"]["problems"][0])
        report = self._read(self.symmetric(halt_in=(0, 1), source=9), "unmirrored")
        self.assertEqual(report["verdict"], "failed")
        self.assertIn("stop kind, source or counts", report["symmetry"]["problems"][0])
        self.assertEqual(estimate(report, "candidate_edge"), F(0))

    def test_wholly_halted_aa_is_void_not_failed(self):
        report = self._read(lambda p, g, pos: ("halt", {"kind": "decision_cap"}, 3, 77), "edge-all-halted")
        self.assertEqual(report["verdict"], "void")
        self.assertEqual(report["void"], "all games halted")
        self.assertEqual(report["next"], "to the lead (A6.12)")
        self.assertEqual(report["games"], {"played": 256, "completed": 0, "halted": 256})
        self.assertEqual(len(report["pair_by_row"]), 4)


class CompanionTests(unittest.TestCase):
    def test_one_seat_rotation_and_all_learned_count(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            plans = [synth.plan("a_one", u, rotation=r) for u in range(2) for r in range(8)]
            recs = {p["pod_index"]: synth.record(p, lambda p, g, pos: "win" if g["game"] < 4 else "loss")
                    for p in plans}
            set_dir = synth.write_plan_set(root, "a_one", "fx-a_one-rotation-d0-fixed", plans, DECL_SHA,
                                           executions=[{"records": recs}], units=[0, 2])
            report = readers.run_read(DECL, DECL_SHA, "a_one", 1, [read_sets.open_plan_set(set_dir, DECL_SHA)])
            self.assertEqual(report["games"]["played"], 2 * 56)
            self.assertEqual(estimate(report, "single_seat_edge"), F(2 * 32, 56) - 1)
            plans = [synth.plan("a_all", u) for u in range(2)]
            recs = {p["pod_index"]: synth.record(p, lambda p, g, pos: "halt" if g["game"] == 0 else "draw")
                    for p in plans}
            set_dir = synth.write_plan_set(root, "a_all", "fx-a_all-all-count-d0-fixed", plans, DECL_SHA,
                                           executions=[{"records": recs}], units=[0, 2])
            report = readers.run_read(DECL, DECL_SHA, "a_all", 1, [read_sets.open_plan_set(set_dir, DECL_SHA)])
            self.assertEqual(report["completion"]["games"], {"played": 56, "completed": 54, "halted": 2})


class D13Tests(unittest.TestCase):
    def test_panel_contrasts_and_agreement(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            base = "fx-d13-panel-d0"
            stage1, outs = synth.write_prep_set(root, "d13", "d13", f"{base}-prep-panel", range(0, 2), DECL_SHA)
            panel = read_plans.assemble_panel("d13", "test", read_sets.load_manifest(stage1), 0, outs,
                                              pins=synth.PINS, builders={r: dict(synth.UNIFORM) for r in
                                                                         ("b1_last", "initial", "k1_last")},
                                              **synth.DEALER)
            text = read_plans.serialize(panel)
            (root / "test" / "v1" / f"{base}-panel").mkdir(parents=True)
            (root / "test" / "v1" / f"{base}-panel" / "panel.json").write_text(text, encoding="utf-8", newline="\n")
            digest = hashlib.sha256(text.encode()).hexdigest()
            opened = []
            for ordinal, x_wins in ((0, True), (1, False)):
                mark = {"kind": "pilot", "label": f"pilot-{ordinal}", "ordinal": ordinal}
                plans = [synth.plan("d13", u, mark=mark, prep=outs[u]) for u in range(2)]
                outcome = (lambda xw: lambda p, g, pos: ("win" if xw else "loss") if g["game"] % 2 == 0 else "draw")(x_wins)
                set_dir = synth.write_plan_set(
                    root, "d13", f"{base}-pilot-{ordinal}", plans, DECL_SHA,
                    executions=[{"records": {p["pod_index"]: synth.record(p, outcome) for p in plans}}],
                    units=[0, 2], panel_sha256=digest, prep_set=f"test/v1/{base}-prep-panel/exec-0")
                opened.append(read_sets.open_plan_set(set_dir, DECL_SHA, pool=synth.POOL, constants=synth.CONST,
                                                      pinned_panel=digest))
            report = readers.run_read(decl_with(**{"d13.panel_sha256": digest}), DECL_SHA, "d13", 0, opened)
            first = report["pilots"][0]["contrasts"]
            self.assertEqual(set(first), {"current_initial", "k1_last_b1_last", "candidate_stated"})
            self.assertEqual(exact(first["candidate_stated"]["drop"]["estimate"]), F(1, 2))
            self.assertEqual(exact(report["pilots"][1]["contrasts"]["current_initial"]["drop"]["estimate"]), F(-1, 2))
            self.assertEqual(report["agreement"][0], {"pilots": [0, 1], "contrast": "current_initial",
                                                      "same_sign": 0, "jointly_defined": 32})


# ---------------------------------------------------------------------------------------------
# A6.18 sizing, declaration, determinism, D5 records


class SizingTests(unittest.TestCase):
    def test_branches(self):
        sizing = DECL["reads"]["c"]["sizing"]
        out = readers.size_from_probe([F(1, 10), F(-1, 10), F(3, 10), F(-3, 10)], sizing)
        # s^2 = 2/30 * ... computed exactly: (14/5)^2 * s^2 / (1/50)^2.
        values = [F(1, 10), F(-1, 10), F(3, 10), F(-3, 10)]
        s2 = sum(v * v for v in values) / 3
        self.assertEqual(exact(out["n_raw"]), F(14, 5) ** 2 * s2 / F(1, 50) ** 2)
        self.assertEqual((out["n"], out["flag"]), (512, "cap binds"))
        small = readers.size_from_probe([F(1, 1000), F(-1, 1000), F(0)], sizing)
        self.assertEqual((small["n"], small["flag"]), (128, "floor binds"))
        mid = readers.size_from_probe([F(1, 10), F(-1, 10)], sizing)
        # (14/5)^2 * (1/50) / (1/50)^2 = 392 exactly; binary floating point gives 391.99999999999994.
        self.assertNotEqual((1.96 + 0.84) ** 2 * 0.02 / 0.02 ** 2, 392)
        self.assertEqual((exact(mid["n_raw"]), mid["n"], mid["flag"]), (F(392), 392, None))
        self.assertEqual(readers.size_from_probe([F(1, 5)] * 4, sizing)["flag"], "zero spread: the cap")
        self.assertEqual(readers.size_from_probe([F(1, 5), None, F(0)], sizing)["flag"], "some units undefined: the cap")
        self.assertIsNone(readers.size_from_probe([F(1, 5), None], sizing)["n"])

    def test_size_c_probe_report_and_no_holdout_use(self):
        def outcome(p, g, pos):
            return "win" if (g["game"] % 2 == 0 and synth.unit_draw("sz", p["unit"], g["game"]) < 0.6) else "draw"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _, outs = synth.write_prep_set(root, "size_c", "size_c", "fx-size_c-edge-sizing-d0-prep-0-3",
                                           range(0, 3), DECL_SHA)
            plans = [synth.plan("size_c", u, prep=outs[u]) for u in range(3)]
            set_dir = synth.write_plan_set(root, "size_c", "fx-size_c-edge-sizing-d0-fixed", plans, DECL_SHA,
                                           executions=[{"records": {p["pod_index"]: synth.record(p, outcome)
                                                                    for p in plans}}],
                                           units=[0, 3], prep_set="test/v1/fx-size_c-edge-sizing-d0-prep-0-3/exec-0")
            opened = read_sets.open_plan_set(set_dir, DECL_SHA, pool=synth.POOL, constants=synth.CONST)
            report = readers.run_read(DECL, DECL_SHA, "size_c", 1, [opened])
            self.assertEqual(report["count"]["units_defined"], 3)
            self.assertIn("within_draft_correlation", report["diagnostics"])
            with self.assertRaisesRegex(readers.ReadError, "family size_c"):
                readers.run_read(DECL, DECL_SHA, "c", 1, [opened])


def make_countersigned_a6(pins: dict[str, str], *, inputs: dict[str, str] | None = None,
                          policies: dict[str, object] | None = None) -> tuple[bytes, str]:
    """Build a synthetic countersigned A6 document keeping exact signed part 1 bytes verbatim."""
    raw = A6_PART1_FIXTURE.read_bytes()
    lines = [
        "",
        "",
        "## Signature, part 2: pins (research lead, 2026-11-25 12:00 ET)",
        "",
        "Research lead, 2026-11-25 12:00 ET. Countersigned part 2 pins on the signed part 1 text.",
        "",
    ]
    for k, v in pins.items():
        lines.append(f"- `{k}`: `{v}`")
    if inputs is not None:
        for k, v in inputs.items():
            lines.append(f"- `{k}`: `{v}`")
    if policies is not None:
        lines.append("")
        lines.append("```json")
        lines.append(json.dumps({"policies": policies}, indent=2))
        lines.append("```")
    lines.extend(["", "Countersigned, research lead.", ""])
    cs = raw + ("\n".join(lines)).encode("utf-8")
    return cs, hashlib.sha256(cs).hexdigest()


class DeclarationTests(unittest.TestCase):
    def _doc(self):
        from decimal import Decimal
        return json.loads(DECLARATION.read_text(encoding="utf-8"), parse_float=Decimal)

    def test_fixture_and_refusals(self):
        self.assertEqual(DECL["reads"]["c"]["cut"], F(-1, 50))
        self.assertEqual(DECL["reads"]["b"]["cut"], F(13, 25))
        self.assertEqual(DECL["reads"]["a"]["levels"], [F(1, 40)] * 2)
        for mutate in (lambda d: d["reads"]["c"].pop("n"), lambda d: d["reads"]["c"].update(extra=1),
                       lambda d: d.update(read_plan="mtg-kernel-cube-read-plan/v1"),
                       lambda d: d.update(read_plan="mtg-kernel-cube-read-plan/v2"),
                       lambda d: d["reads"]["c"].update(namespace="read-gate-b"),
                       lambda d: d["reads"]["c"]["sizing"].update(delta=0.02),
                       lambda d: d["reads"]["c"].update(extension_factor=True)):
            doc = self._doc()
            mutate(doc)
            with self.assertRaises(readers.DeclarationError):
                readers.validate_declaration(doc)

    def test_unsigned_declaration_refuses_read_data(self):
        with self.assertRaisesRegex(readers.ReadError, "countersigned"):
            readers.run_read(decl_with(**{"c.namespace": "read-gate-c"}), DECL_SHA, "c", 1, [])

    def test_a6_fixture_is_self_contained_in_repo(self):
        self.assertTrue(A6_PART1_FIXTURE.is_file())
        raw = A6_PART1_FIXTURE.read_bytes()
        norm = raw.replace(b"\r\n", b"\n")
        idx = norm.find(readers.A6_PART1_HEADING)
        self.assertGreater(idx, 0)
        part1 = norm[:idx].rstrip(b"\n") + b"\n"
        self.assertEqual(hashlib.sha256(part1).hexdigest(), readers.A6_PART1_SHA256)
        src = Path(__file__).read_text(encoding="utf-8")
        self.assertNotIn("m4-docs" + "/A6-READS.md", src)
        self.assertNotIn("m4-docs" + "\\A6-READS.md", src)

    def test_live_declaration_verifies_countersigned_a6_constants_and_pins(self):
        raw_a6 = A6_PART1_FIXTURE.read_bytes()
        raw_a6_sha = hashlib.sha256(raw_a6).hexdigest()
        # The unsigned part-2 A6 file must be refused.
        with self.assertRaisesRegex(readers.ReadError, "countersigned|part 2"):
            readers.verify_countersigned_a6(raw_a6, raw_a6_sha)

        # Reviewer's negation probe and malformed part-2 blocks must be refused.
        for label, suffix, pattern in (
            ("negation-probe", b"\n\n## Signature, part 2\nNo countersignature has been issued.\n",
             "No countersignature"),
            ("missing-closing",
             b"\n\n## Signature, part 2\nResearch lead, 2026-11-25 12:00 ET. Countersigned.\n",
             "closing"),
            ("missing-pins",
             b"\n\n## Signature, part 2\nResearch lead, 2026-11-25 12:00 ET. Countersigned.\n"
             b"Countersigned, research lead.\n",
             "pin fills"),
            ("missing-timestamp",
             b"\n\n## Signature, part 2\nCountersigned by research lead.\nCountersigned, research lead.\n",
             "timestamped"),
        ):
            with self.subTest(bad_part2=label):
                bad_a6 = raw_a6 + suffix
                bad_sha = hashlib.sha256(bad_a6).hexdigest()
                with self.assertRaisesRegex(readers.ReadError, pattern):
                    readers.verify_countersigned_a6(bad_a6, bad_sha)

        # Synthetic countersigned copy keeping the exact signed part 1 bytes verbatim and filling pins.
        live_inputs = {"pool_sha256": synth.POOL[1], "constants_sha256": synth.CONST[1]}
        countersigned_a6, cs_sha = make_countersigned_a6(synth.LIVE_PINS, inputs=live_inputs)
        a6_info = readers.verify_countersigned_a6(countersigned_a6, cs_sha)
        self.assertEqual(a6_info["pins"], synth.LIVE_PINS)
        self.assertEqual(a6_info["inputs"], live_inputs)

        # Conflicting duplicate pin fill in part 2 must fail.
        dup_a6 = countersigned_a6.replace(
            b"Countersigned, research lead.\n",
            f"- `binary`: `{'0' * 64}`\nCountersigned, research lead.\n".encode("utf-8"),
        )
        with self.assertRaisesRegex(readers.ReadError, "conflicting"):
            readers.verify_countersigned_a6(dup_a6, hashlib.sha256(dup_a6).hexdigest())

        # Tampering with part 1 bytes must fail even when a6_sha256 matches the tampered file.
        tampered_a6 = countersigned_a6.replace(b"9,999", b"9,998", 1)
        tampered_sha = hashlib.sha256(tampered_a6).hexdigest()
        with self.assertRaisesRegex(readers.ReadError, "part 1"):
            readers.verify_countersigned_a6(tampered_a6, tampered_sha)

        # Build a live declaration for read-d8-aa and verify validation + live run_read.
        from decimal import Decimal
        live_doc = self._doc()
        live_doc["a6_sha256"] = cs_sha
        live_doc["pins"] = dict(synth.LIVE_PINS)
        live_doc["inputs"] = dict(live_inputs)
        live_doc["reads"]["aa"]["namespace"] = "read-d8-aa"
        live_doc["reads"]["b"]["namespace"] = "read-gate-b"
        live_decl = readers.validate_declaration(live_doc)

        # Mutating signed constants in a live declaration must fail validate_declaration.
        for mutate, pattern in (
            (lambda d: d.update(resamples=1), "resamples"),
            (lambda d: d["reads"]["b"].update(levels=[Decimal("0.20"), Decimal("0.20")]), "levels"),
            (lambda d: d["reads"]["b"].update(cut=Decimal("0")), "cut"),
            (lambda d: d["reads"]["b"]["sizing"].update(cap=256), "sizing"),
            (lambda d: d["reads"]["aa"].update(pairs=2), "pairs"),
            (lambda d: d["reads"]["aa"].update(level=Decimal("0.05")), "level"),
            (lambda d: d.update(pins=None), "pins"),
        ):
            bad = copy.deepcopy(live_doc)
            mutate(bad)
            with self.assertRaisesRegex(readers.DeclarationError, pattern):
                readers.validate_declaration(bad)

        # Build a valid live read-d8-aa set on disk and run_read through it.
        with tempfile.TemporaryDirectory() as tmp:
            root = synth.assert_test_temp_root(Path(tmp))
            plans = [
                read_plans.build_plan("aa", "read-d8-aa", u, pins=synth.LIVE_PINS, pilot=synth.PILOT,
                                      picker=dict(synth.UNIFORM), builders={"candidate": dict(synth.UNIFORM)}, mirror=m)
                for u in range(4) for m in (0, 1)
            ]
            recs = {p["pod_index"]: synth.record(p, lambda p, g, pos: "draw") for p in plans}
            entries = [(p["pod_index"], read_plans.doc_sha256(p)) for p in plans]
            man = read_plans.build_manifest("plans", "aa", "read-d8-aa", "fixed", cs_sha, entries,
                                            units=[0, 4], reserved=[0, 4])
            man["executions"].append({"execution": 0, "reason": "first", "expired_games": [],
                                      "binary": synth.BINARY, "platform": synth.PLATFORM})
            set_dir = read_plans.write_set(root, man, {f"plans/pod-{p['pod_index']}.json": p for p in plans})
            exec_dir = set_dir / "exec-0"
            (exec_dir / "records").mkdir(parents=True, exist_ok=True)
            listed = []
            for i, rec in sorted(recs.items()):
                text = json.dumps(rec, indent=2)
                (exec_dir / "records" / f"pod-{i}.json").write_bytes(text.encode())
                listed.append({"pod_index": i, "sha256": hashlib.sha256(text.encode()).hexdigest(),
                               "status": rec["status"]})
            receipt = {"execution": 0, "plan_set_sha256": man["plan_set_sha256"], "binary": synth.BINARY,
                       "platform": synth.PLATFORM, "ended": "complete", "records": listed}
            (exec_dir / "execution.json").write_bytes(read_plans.serialize(receipt).encode())
            opened = read_sets.open_plan_set(set_dir, cs_sha)

            # Live read passes with the countersigned A6 file.
            rep = readers.run_read(live_decl, cs_sha, "aa", 1, [opened], a6_file=countersigned_a6)
            self.assertEqual(rep["verdict"], "symmetric")

            # Live read without the countersigned A6 file is refused.
            with self.assertRaisesRegex(readers.ReadError, "A6 file"):
                readers.run_read(live_decl, cs_sha, "aa", 1, [opened])

            # Live read with mutated in-memory constants is still refused even when manifest repeats cs_sha.
            bad_mem = copy.deepcopy(live_decl)
            bad_mem["resamples"] = 1
            with self.assertRaisesRegex(readers.ReadError, "resamples"):
                readers.run_read(bad_mem, cs_sha, "aa", 1, [opened], a6_file=countersigned_a6)

            bad_lvl = copy.deepcopy(live_decl)
            bad_lvl["reads"]["aa"]["level"] = F(1, 5)
            with self.assertRaisesRegex(readers.ReadError, "level"):
                readers.run_read(bad_lvl, cs_sha, "aa", 1, [opened], a6_file=countersigned_a6)

            # Live declaration whose pins differ from countersigned A6 part 2 fills is refused.
            bad_pins = copy.deepcopy(live_decl)
            bad_pins["pins"]["binary"] = "0" * 64
            with self.assertRaisesRegex(readers.ReadError, "part 2 pins"):
                readers.run_read(bad_pins, cs_sha, "aa", 1, [opened], a6_file=countersigned_a6)

            # Plan pins differing from countersigned A6 part 2 pins are refused even if declaration pins match A6.
            bad_plan_opened = copy.deepcopy(opened)
            for p in bad_plan_opened["plans"].values():
                p["pins"]["binary"] = "0" * 64
            with self.assertRaisesRegex(readers.ReadError, "pins"):
                readers.run_read(live_decl, cs_sha, "aa", 1, [bad_plan_opened], a6_file=countersigned_a6)

            # Live A/A plan with a non-uniform pilot is refused by A6 policy enforcement.
            bad_pilot_opened = copy.deepcopy(opened)
            bad_pilot = {"kind": "checkpoint", "pin": "9" * 64, "decoding": "greedy"}
            for p in bad_pilot_opened["plans"].values():
                p["pilot"] = dict(bad_pilot)
            with self.assertRaisesRegex(readers.ReadError, "pilot"):
                readers.run_read(live_decl, cs_sha, "aa", 1, [bad_pilot_opened], a6_file=countersigned_a6)

            # Dealer family without pool/constants pins in countersigned A6 part 2 is refused.
            no_inp_a6, no_inp_sha = make_countersigned_a6(synth.LIVE_PINS, inputs=None)
            no_inp_decl = copy.deepcopy(live_decl)
            no_inp_decl["a6_sha256"] = no_inp_sha
            with self.assertRaisesRegex(readers.ReadError, "pool_sha256"):
                readers.run_read(no_inp_decl, no_inp_sha, "b", 1, [], a6_file=no_inp_a6)

            # Custom policy pin in A6 part 2 is enforced against plan bindings.
            pol_a6, pol_sha = make_countersigned_a6(
                synth.LIVE_PINS, inputs=live_inputs,
                policies={"dose": {"builders": {"current": {"kind": "learned", "pin": "a" * 64},
                                                "initial": {"kind": "uniform", "pin": None}}}},
            )
            pol_info = readers.verify_countersigned_a6(pol_a6, pol_sha)
            _, _, exp_builders = readers._a6_expected_policies("dose", "dose", "dose-d8", pol_info)
            self.assertEqual(exp_builders["current"], {"kind": "learned", "pin": "a" * 64})

        # In `test`, A/A uses the declared level (not hardcoded), and a_all omits bootstrap fallback.
        with tempfile.TemporaryDirectory() as tmp:
            opened_aa = a_sets(Path(tmp), "aa", "swap", lambda p, g, pos: "draw",
                               units=4, set_suffix="fixed", look=None)
            rep_aa = readers.run_read(decl_with(**{"aa.level": F(1, 20)}), DECL_SHA, "aa", 1, [opened_aa])
            self.assertEqual(exact(rep_aa["bootstrap"]["level"]), F(1, 20))
            plans_all = [synth.plan("a_all", u) for u in range(2)]
            recs_all = {p["pod_index"]: synth.record(p, lambda p, g, pos: "draw") for p in plans_all}
            sd_all = synth.write_plan_set(Path(tmp), "a_all", "fx-a_all-all-count-d0-fixed", plans_all, DECL_SHA,
                                          executions=[{"records": recs_all}], units=[0, 2])
            rep_all = readers.run_read(DECL, DECL_SHA, "a_all", 1, [read_sets.open_plan_set(sd_all, DECL_SHA)])
            self.assertNotIn("bootstrap", rep_all)

    def test_fast_indices_equal_deal_seed(self):
        for b in (0, 1, 9999):
            self.assertEqual(readers.resample_indices("read-gate-c", b, 37),
                             [deal.seed(["bootstrap", "read-gate-c", b, t]) % 37 for t in range(37)])


class DeterminismTests(unittest.TestCase):
    def test_report_bytes_and_golden(self):
        texts = []
        for _ in range(2):
            with tempfile.TemporaryDirectory() as tmp:
                opened = c_sets(Path(tmp), "shared", by_slot({0: "win", 3: "halt"}, arm=0))
                texts.append(readers.serialize_report(readers.run_read(DECL, DECL_SHA, "c", 1, opened)))
        self.assertEqual(texts[0], texts[1])
        self.assertEqual(hashlib.sha256(texts[0].encode()).hexdigest(), REPORT_GOLDEN_SHA256)

    def test_cli(self):
        with tempfile.TemporaryDirectory() as tmp:
            opened = c_sets(Path(tmp), "shared", by_slot({0: "win"}, arm=0))
            pool, const = Path(tmp) / "pool.json", Path(tmp) / "constants.json"
            pool.write_text(read_plans.serialize(synth.POOL[0]), encoding="utf-8", newline="\n")
            const.write_text(read_plans.serialize(synth.CONST[0]), encoding="utf-8", newline="\n")
            outs = []
            for k in range(2):
                out = Path(tmp) / f"r{k}.json"
                self.assertEqual(readers.main(["--declaration", str(DECLARATION), "--read", "c", "--look", "1",
                                               "--sets", str(opened[0]["set_dir"]), "--pool", str(pool),
                                               "--constants", str(const), "--out", str(out)]), 0)
                outs.append(out.read_bytes())
            self.assertEqual(outs[0], outs[1])
            self.assertEqual(json.loads(outs[0])["verdict"], "win")


REPORT_GOLDEN_SHA256 = "fc0de35729ed279d8c255aad456b508dbd0d6143e840a6ab5179f2097c8d9026"


class D8V3RestartTests(unittest.TestCase):
    """Finding 2: READ-PLAN-v3 Changes 1 & 2 (Q2.1 to Q2.4) in dose-d8 and read-d8-aa."""

    @staticmethod
    def _countersigned_a6() -> tuple[bytes, str]:
        return make_countersigned_a6(
            synth.LIVE_PINS,
            inputs={"pool_sha256": synth.POOL[1], "constants_sha256": synth.CONST[1]},
        )

    @staticmethod
    def _live_decl(cs_sha: str, panel_v1: str | None = None) -> dict:
        from decimal import Decimal
        doc = json.loads(DECLARATION.read_text(encoding="utf-8"), parse_float=Decimal)
        doc["a6_sha256"] = cs_sha
        doc["pins"] = dict(synth.LIVE_PINS)
        doc["inputs"] = {"pool_sha256": synth.POOL[1], "constants_sha256": synth.CONST[1]}
        doc["reads"]["aa"]["namespace"] = "read-d8-aa"
        doc["reads"]["dose"]["namespace"] = "dose-d8"
        doc["reads"]["dose"]["panel_drafts"] = 32
        doc["reads"]["dose"]["panel_sha256"] = panel_v1
        return readers.validate_declaration(doc)

    def test_v2_dose_seeds_and_v3_manifest_shape_refusals(self):
        # Reviewer's exact unit-32 seed vectors: v1 vs v2 must differ.
        self.assertEqual(read_plans.plan_seeds("dose", "dose-d8", 32, run="v1")["draft_seed"], 5329981908323915159)
        self.assertEqual(read_plans.plan_seeds("dose", "dose-d8", 32, run="v2")["draft_seed"], 5005737629818919850)

        # Fresh read-d8-aa manifest with reserved=None is refused at write time.
        entries = [(p, "0" * 64) for p in range(8)]
        with self.assertRaisesRegex(read_plans.PlanError, "reserved"):
            read_plans.build_manifest("plans", "aa", "read-d8-aa", "fixed", "a" * 64, entries,
                                      units=[0, 4], reserved=None)
        # Fresh dose-d8 prep-panel with reserved=None is refused at write time.
        with self.assertRaisesRegex(read_plans.PlanError, "reserved"):
            read_plans.build_manifest("prep", "dose", "dose-d8", "prep-panel", "a" * 64,
                                      [(u, "0" * 64) for u in range(32)], units=[0, 32], reserved=None)

        # Namespace `test` keeps v2 rules (Q2.3): reserved=None allowed, run="v2" refused.
        m_test = read_plans.build_manifest("plans", "aa", "test", "fx-aa-swap-d0-fixed", DECL_SHA, entries,
                                           units=[0, 4], reserved=None)
        self.assertIsNone(m_test["reserved"])
        with self.assertRaisesRegex(read_plans.PlanError, "v2 rules"):
            read_plans.build_manifest("plans", "aa", "test", "fx-aa-swap-d0-fixed", DECL_SHA, entries,
                                      units=[0, 4], reserved=[0, 4], run="v2")

    def test_d8_v3_test_root_guard_rejects_non_temp_roots(self):
        for bad_root in (synth.REPO_ROOT, synth.WORKTREE_PARENT, Path("N:/cube-20261122/runs/live"),
                         Path(tempfile.gettempdir())):
            with self.assertRaises(AssertionError):
                synth.assert_test_temp_root(bad_root)
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(synth.assert_test_temp_root(Path(tmp)), Path(tmp).resolve())

    def test_dose_d8_v2_restart_replaced_and_fresh_ordinals_and_shakedown_manifest_pin(self):
        cs_a6, cs_sha = self._countersigned_a6()
        d0_sha = "d" * 64
        with tempfile.TemporaryDirectory() as tmp:
            root = synth.assert_test_temp_root(Path(tmp))

            # --- Run v1: units [0, 32), panel P1, prep-dose-0 complete, dose-0 interrupted after units 0, 1 ---
            stage1_v1, outs_v1 = synth.write_prep_set(
                root, "dose_panel", "dose", "prep-panel", range(0, 32), cs_sha,
                reserved=[0, 32], namespace="dose-d8", run="v1",
            )
            panel_v1 = read_plans.assemble_panel(
                "dose", "dose-d8", read_sets.load_manifest(stage1_v1), 0, outs_v1,
                pins=synth.LIVE_PINS, builders=None, **synth.DEALER, **synth.LIVE_INDEX_ARGS,
            )
            p1_dir = root / "dose-d8" / "v1" / "panel"
            p1_dir.mkdir(parents=True)
            p1_text = read_plans.serialize(panel_v1)
            (p1_dir / "panel.json").write_bytes(p1_text.encode("utf-8"))
            p1_sha = hashlib.sha256(p1_text.encode("utf-8")).hexdigest()

            _, pouts_v1_0 = synth.write_prep_set(
                root, "dose", "dose", "prep-dose-0", range(0, 32), cs_sha,
                reserved=[0, 32], panel_sha256=p1_sha, namespace="dose-d8", run="v1",
            )
            mark0 = {"kind": "dose", "label": "pods-512", "ordinal": 0}
            plans_v1_0 = [synth.plan("dose", u, mark=mark0, prep=pouts_v1_0[u], namespace="dose-d8", run="v1")
                          for u in range(0, 32)]
            # Interrupt v1/dose-0 after unit 0 completes and unit 1 stops -> started={0,1}, unused=[[2,32]].
            recs_v1_0 = {
                0: synth.record(plans_v1_0[0], lambda p, g, pos: "draw"),
                1: synth.record(plans_v1_0[1], lambda p, g, pos: "draw", games=4, status="stopped"),
            }
            synth.write_plan_set(
                root, "dose", "dose-0", plans_v1_0, cs_sha,
                executions=[{"records": recs_v1_0, "ended": "interrupted"}],
                units=[0, 32], reserved=[0, 32], panel_sha256=p1_sha,
                prep_set="dose-d8/v1/prep-dose-0/exec-0", namespace="dose-d8", run="v1",
            )

            # --- Run v2: units [32, 64), fresh panel P2, replaced ordinal 0 + fresh ordinal 1 ---
            stage1_v2, outs_v2 = synth.write_prep_set(
                root, "dose_panel", "dose", "prep-panel", range(32, 64), cs_sha,
                reserved=[32, 64], namespace="dose-d8", run="v2",
            )
            panel_v2 = read_plans.assemble_panel(
                "dose", "dose-d8", read_sets.load_manifest(stage1_v2), 0, outs_v2,
                pins=synth.LIVE_PINS, builders=None, **synth.DEALER, **synth.LIVE_INDEX_ARGS,
            )
            self.assertEqual(panel_v2["run"], "v2")
            self.assertEqual(panel_v2["units"], [32, 64])
            self.assertEqual(panel_v2["drafts"][0]["seeds"]["draft_seed"], 5005737629818919850)
            p2_dir = root / "dose-d8" / "v2" / "panel"
            p2_dir.mkdir(parents=True)
            p2_text = read_plans.serialize(panel_v2)
            (p2_dir / "panel.json").write_bytes(p2_text.encode("utf-8"))
            p2_sha = hashlib.sha256(p2_text.encode("utf-8")).hexdigest()

            # Write D0.3b shakedown manifests (JSON and Markdown) pinning v1 -> p1_sha and v2 -> p2_sha.
            sm_json_path = root / "SHAKEDOWN-MANIFEST.json"
            sm_json_bytes = read_plans.serialize({
                "schema": "mtg-kernel-cube-shakedown-manifest/v1",
                "entries": [
                    {"seq": 3, "kind": "dose_panel", "namespace": "dose-d8", "run": "v1", "panel_sha256": p1_sha},
                    {"seq": 8, "kind": "restart", "namespace": "dose-d8", "run": "v2", "panel_sha256": p2_sha},
                ],
            }).encode("utf-8")
            sm_json_path.write_bytes(sm_json_bytes)
            sm_json_sha = hashlib.sha256(sm_json_bytes).hexdigest()

            sm_md_path = root / "SHAKEDOWN-MANIFEST.md"
            sm_md_bytes = (
                f"## Entry 3: dose-d8 panel\n- namespace: dose-d8, run: v1, panel_sha256: {p1_sha}\n\n"
                f"## Entry 8: restart\n- namespace: dose-d8, run: v2, panel.json sha256 {p2_sha}\n"
            ).encode("utf-8")
            sm_md_path.write_bytes(sm_md_bytes)
            sm_md_sha = hashlib.sha256(sm_md_bytes).hexdigest()
            self.assertEqual(read_sets.load_shakedown_manifest_panels(sm_md_path, sm_md_sha),
                             {"v1": p1_sha, "v2": p2_sha})

            # Replaced ordinal 0: prep-dose-0 is fresh (v1/prep-dose-0 completed), dose-0-restart-1 replaces v1/dose-0.
            prep_v2_0, pouts_v2_0 = synth.write_prep_set(
                root, "dose", "dose", "prep-dose-0", range(32, 64), cs_sha,
                reserved=[32, 64], panel_sha256=p2_sha, namespace="dose-d8", run="v2",
            )
            read_sets.check_prep_dose_set(prep_v2_0, pool=synth.POOL, constants=synth.CONST,
                                          shakedown_manifest=(sm_json_path, sm_json_sha), **synth.LIVE_INDEX_ARGS)
            plans_v2_0 = [synth.plan("dose", u, mark=mark0, prep=pouts_v2_0[u], namespace="dose-d8", run="v2")
                          for u in range(32, 64)]
            recs_v2_0 = {p["pod_index"]: synth.record(p, lambda pl, g, pos: "win" if g["game"] % 2 == 0 else "draw")
                         for p in plans_v2_0}

            # Negative writer checks before writing the valid replacement:
            # 1) Plain set name `dose-0` refused because v1/dose-0 was interrupted.
            with self.assertRaisesRegex(read_sets.SetError, "restart"):
                synth.write_plan_set(
                    root, "dose", "dose-0", plans_v2_0, cs_sha,
                    executions=[{"records": recs_v2_0}], units=[32, 64], reserved=[32, 64],
                    panel_sha256=p2_sha, prep_set="dose-d8/v2/prep-dose-0/exec-0", namespace="dose-d8", run="v2",
                )
            # 2) Wrong retired range refused.
            with self.assertRaisesRegex(read_sets.SetError, "retired"):
                synth.write_plan_set(
                    root, "dose", "dose-0-restart-1", plans_v2_0, cs_sha,
                    executions=[{"records": recs_v2_0}], units=[32, 64], reserved=[32, 64],
                    panel_sha256=p2_sha, prep_set="dose-d8/v2/prep-dose-0/exec-0",
                    replaces="v1/dose-0", retired=[[3, 32]], namespace="dose-d8", run="v2",
                )

            # Valid replaced ordinal 0:
            dose_v2_0 = synth.write_plan_set(
                root, "dose", "dose-0-restart-1", plans_v2_0, cs_sha,
                executions=[{"records": recs_v2_0}], units=[32, 64], reserved=[32, 64],
                panel_sha256=p2_sha, prep_set="dose-d8/v2/prep-dose-0/exec-0",
                replaces="v1/dose-0", retired=[[2, 32]], namespace="dose-d8", run="v2",
            )

            # Fresh ordinal 1 (never started in v1): plain names prep-dose-1 and dose-1 in v2.
            mark1 = {"kind": "dose", "label": "pods-1024", "ordinal": 1}
            _, pouts_v2_1 = synth.write_prep_set(
                root, "dose", "dose", "prep-dose-1", range(32, 64), cs_sha,
                reserved=[32, 64], panel_sha256=p2_sha, namespace="dose-d8", run="v2",
            )
            plans_v2_1 = [synth.plan("dose", u, mark=mark1, prep=pouts_v2_1[u], namespace="dose-d8", run="v2")
                          for u in range(32, 64)]
            recs_v2_1 = {p["pod_index"]: synth.record(p, lambda pl, g, pos: "win" if g["game"] % 2 == 0 else "draw")
                         for p in plans_v2_1}
            dose_v2_1 = synth.write_plan_set(
                root, "dose", "dose-1", plans_v2_1, cs_sha,
                executions=[{"records": recs_v2_1}], units=[32, 64], reserved=[32, 64],
                panel_sha256=p2_sha, prep_set="dose-d8/v2/prep-dose-1/exec-0", namespace="dose-d8", run="v2",
            )

            # Shakedown manifest D0.3b pin refusals (Q2.4):
            # 0) Missing card_index/cube_table on live dose-d8 is refused.
            with self.assertRaisesRegex(read_sets.SetError, "card_index"):
                read_sets.open_plan_set(dose_v2_0, cs_sha, pool=synth.POOL, constants=synth.CONST,
                                        shakedown_manifest=(sm_json_path, sm_json_sha))
            # 1) Missing shakedown manifest on v2 is refused.
            with self.assertRaisesRegex(read_sets.SetError, "shakedown manifest"):
                read_sets.open_plan_set(dose_v2_0, cs_sha, pool=synth.POOL, constants=synth.CONST,
                                        pinned_panel=p2_sha, **synth.LIVE_INDEX_ARGS)
            # 2) Mismatched D0.3b pin refused BEFORE trusting any digest inside.
            with self.assertRaisesRegex(read_sets.SetError, "D0.3b pin"):
                read_sets.open_plan_set(dose_v2_0, cs_sha, pool=synth.POOL, constants=synth.CONST,
                                        shakedown_manifest=(sm_json_path, "0" * 64), **synth.LIVE_INDEX_ARGS)
            tampered_sm = root / "SHAKEDOWN-MANIFEST-tampered.json"
            tampered_sm.write_bytes(sm_json_bytes + b" ")
            with self.assertRaisesRegex(read_sets.SetError, "D0.3b pin"):
                read_sets.open_plan_set(dose_v2_0, cs_sha, pool=synth.POOL, constants=synth.CONST,
                                        shakedown_manifest=(tampered_sm, sm_json_sha), **synth.LIVE_INDEX_ARGS)

            # Valid reads of both replaced ordinal 0 and fresh ordinal 1 on v2's panel:
            live_decl = self._live_decl(cs_sha, panel_v1=p1_sha)
            opened_0 = read_sets.open_plan_set(dose_v2_0, cs_sha, pool=synth.POOL, constants=synth.CONST,
                                               shakedown_manifest=(sm_json_path, sm_json_sha), **synth.LIVE_INDEX_ARGS)
            # run_read refuses a live dose-d8 set that was not checked against card_index and cube_table.
            unchecked_0 = {**opened_0, "checked_card_index": False}
            with self.assertRaisesRegex(readers.ReadError, "card_index"):
                readers.run_read(live_decl, cs_sha, "dose", 0, [unchecked_0], a6_file=cs_a6,
                                 d0_sha256=d0_sha, shakedown_manifest=(sm_json_path, sm_json_sha))
            rep_0 = readers.run_read(live_decl, cs_sha, "dose", 0, [opened_0], a6_file=cs_a6,
                                     d0_sha256=d0_sha, shakedown_manifest=(sm_json_path, sm_json_sha))
            self.assertIsNone(rep_0["verdict"])
            self.assertEqual(rep_0["units"], {"kind": "cluster", "first": 32, "end": 64})
            self.assertEqual(rep_0["provenance"]["panel_run"], "v2")
            self.assertEqual(rep_0["provenance"]["panel_sha256"], p2_sha)
            self.assertEqual(rep_0["provenance"]["shakedown_manifest_sha256"], sm_json_sha)
            self.assertEqual(rep_0["provenance"]["panel_pairing_note"], "marks on different panels are not paired")
            self.assertIn("first_dose_basics_bands", rep_0)

            opened_1 = read_sets.open_plan_set(dose_v2_1, cs_sha, pool=synth.POOL, constants=synth.CONST,
                                               shakedown_manifest=(sm_md_path, sm_md_sha), **synth.LIVE_INDEX_ARGS)
            rep_1 = readers.run_read(live_decl, cs_sha, "dose", 1, [opened_1], a6_file=cs_a6,
                                     d0_sha256=d0_sha, shakedown_manifest=(sm_md_path, sm_md_sha))
            self.assertIsNone(rep_1["verdict"])
            self.assertEqual(rep_1["units"], {"kind": "cluster", "first": 32, "end": 64})
            self.assertEqual(rep_1["provenance"]["panel_run"], "v2")
            self.assertEqual(rep_1["provenance"]["panel_sha256"], p2_sha)
            # Refuses tampered manifest at run_read entry point too.
            with self.assertRaisesRegex(read_sets.SetError, "D0.3b pin"):
                readers.run_read(live_decl, cs_sha, "dose", 1, [opened_1], a6_file=cs_a6,
                                 d0_sha256=d0_sha, shakedown_manifest=(tampered_sm, sm_md_sha))

    def test_read_d8_aa_replacement_on_fresh_pairs_and_union_of_executions_retired(self):
        cs_a6, cs_sha = self._countersigned_a6()
        live_decl = self._live_decl(cs_sha)

        # Scenario 1: simple interruption on pair 0 -> retired [[1, 4]], replaced by v2/fixed-restart-1 at [4, 8).
        with tempfile.TemporaryDirectory() as tmp:
            root = synth.assert_test_temp_root(Path(tmp))
            p1 = [synth.plan("aa", u, mirror=m, namespace="read-d8-aa", run="v1") for u in range(0, 4) for m in (0, 1)]
            recs1 = {0: synth.record(p1[0], lambda p, g, pos: "draw"),
                     1: synth.record(p1[1], lambda p, g, pos: "draw", games=4, status="stopped")}
            synth.write_plan_set(root, "aa", "fixed", p1, cs_sha,
                                 executions=[{"records": recs1, "ended": "interrupted"}],
                                 units=[0, 4], reserved=[0, 4], namespace="read-d8-aa", run="v1")

            p2 = [synth.plan("aa", u, mirror=m, namespace="read-d8-aa", run="v2") for u in range(4, 8) for m in (0, 1)]
            recs2 = {p["pod_index"]: synth.record(p, lambda pl, g, pos: "draw") for p in p2}

            # Refuses unqualified predecessor, wrong k, and units below high-water mark 4.
            with self.assertRaisesRegex(read_plans.PlanError, "qualified"):
                synth.write_plan_set(root, "aa", "fixed-restart-1", p2, cs_sha, executions=[{"records": recs2}],
                                     units=[4, 8], reserved=[4, 8], replaces="fixed", retired=[[1, 4]],
                                     namespace="read-d8-aa", run="v2")
            with self.assertRaisesRegex(read_plans.PlanError, "fixed-restart-1"):
                synth.write_plan_set(root, "aa", "fixed-restart-2", p2, cs_sha, executions=[{"records": recs2}],
                                     units=[4, 8], reserved=[4, 8], replaces="v1/fixed", retired=[[1, 4]],
                                     namespace="read-d8-aa", run="v2")
            p2_low = [synth.plan("aa", u, mirror=m, namespace="read-d8-aa", run="v2")
                      for u in range(2, 6) for m in (0, 1)]
            with self.assertRaisesRegex(read_sets.SetError, "high-water"):
                synth.write_plan_set(root, "aa", "fixed-restart-1", p2_low, cs_sha,
                                     executions=[{"records": {p["pod_index"]: synth.record(p, lambda pl, g, pos: "draw")
                                                              for p in p2_low}}],
                                     units=[2, 6], reserved=[2, 6], replaces="v1/fixed", retired=[[1, 4]],
                                     namespace="read-d8-aa", run="v2")

            aa_v2 = synth.write_plan_set(root, "aa", "fixed-restart-1", p2, cs_sha,
                                         executions=[{"records": recs2}], units=[4, 8], reserved=[4, 8],
                                         replaces="v1/fixed", retired=[[1, 4]], namespace="read-d8-aa", run="v2")
            opened = read_sets.open_plan_set(aa_v2, cs_sha)
            rep = readers.run_read(live_decl, cs_sha, "aa", 1, [opened], a6_file=cs_a6)
            self.assertEqual(rep["verdict"], "symmetric")
            self.assertEqual(rep["units"], {"kind": "pair", "first": 4, "end": 8})

        # Scenario 2 (Q2.1 lead's exact example): exec-0 completed all 4 pairs with watchdog halt on pod 2 game 3,
        # and exec-1 hung (same-game watchdog_stop) after starting only pairs 0 and 1. Union of started units is
        # {0, 1, 2, 3}, so retired MUST be [] (NOT [[2, 4]]).
        with tempfile.TemporaryDirectory() as tmp:
            root = synth.assert_test_temp_root(Path(tmp))
            p1 = [synth.plan("aa", u, mirror=m, namespace="read-d8-aa", run="v1") for u in range(0, 4) for m in (0, 1)]
            wd_out = lambda pl, g, pos: (("halt", dict(synth.WATCHDOG_END), None, 50)
                                         if (pl["pod_index"] == 2 and g["game"] == 3) else "draw")
            e0 = {p["pod_index"]: synth.record(p, wd_out) for p in p1}
            e1 = {0: synth.record(p1[0], wd_out), 1: synth.record(p1[1], wd_out),
                  2: synth.record(p1[2], wd_out, games=4, status="stopped")}
            v1_dir = synth.write_plan_set(
                root, "aa", "fixed", p1, cs_sha,
                executions=[{"records": e0}, {"records": e1, "ended": "watchdog_stop", "entry_expired": [[2, 3]]}],
                units=[0, 4], reserved=[0, 4], namespace="read-d8-aa", run="v1",
            )
            self.assertEqual(read_sets.d8_unused_reserved_ranges(v1_dir, read_sets.load_manifest(v1_dir)), [])
            p2 = [synth.plan("aa", u, mirror=m, namespace="read-d8-aa", run="v2") for u in range(4, 8) for m in (0, 1)]
            recs2 = {p["pod_index"]: synth.record(p, lambda pl, g, pos: "draw") for p in p2}
            with self.assertRaisesRegex(read_sets.SetError, "retired"):
                synth.write_plan_set(root, "aa", "fixed-restart-1", p2, cs_sha, executions=[{"records": recs2}],
                                     units=[4, 8], reserved=[4, 8], replaces="v1/fixed", retired=[[2, 4]],
                                     namespace="read-d8-aa", run="v2")
            aa_v2 = synth.write_plan_set(root, "aa", "fixed-restart-1", p2, cs_sha, executions=[{"records": recs2}],
                                         units=[4, 8], reserved=[4, 8], replaces="v1/fixed", retired=[],
                                         namespace="read-d8-aa", run="v2")
            self.assertEqual(readers.run_read(live_decl, cs_sha, "aa", 1,
                                              [read_sets.open_plan_set(aa_v2, cs_sha)], a6_file=cs_a6)["verdict"],
                             "symmetric")

        # Scenario 3 (Q2.1 non-contiguous union): exec-0 started pair 0, exec-1 started pair 2 -> retired [[1,2],[3,4]].
        with tempfile.TemporaryDirectory() as tmp:
            root = synth.assert_test_temp_root(Path(tmp))
            p1 = [synth.plan("aa", u, mirror=m, namespace="read-d8-aa", run="v1") for u in range(0, 4) for m in (0, 1)]
            v1_dir = synth.write_plan_set(
                root, "aa", "fixed", p1, cs_sha,
                executions=[
                    {"records": {0: synth.record(p1[0], lambda pl, g, pos: "draw", games=2, status="stopped")},
                     "ended": "interrupted"},
                    {"records": {4: synth.record(p1[4], lambda pl, g, pos: "draw", games=2, status="stopped")},
                     "ended": "interrupted"},
                ],
                units=[0, 4], reserved=[0, 4], namespace="read-d8-aa", run="v1",
            )
            self.assertEqual(read_sets.d8_unused_reserved_ranges(v1_dir, read_sets.load_manifest(v1_dir)),
                             [[1, 2], [3, 4]])

    def test_d8_interrupted_set_cases_i_ii_iii_and_different_game_watchdog(self):
        cs_a6, cs_sha = self._countersigned_a6()

        # Case (i): execution left no execution.json -> accepted as interrupted predecessor.
        with tempfile.TemporaryDirectory() as tmp:
            root = synth.assert_test_temp_root(Path(tmp))
            p1 = [synth.plan("aa", u, mirror=m, namespace="read-d8-aa", run="v1") for u in range(0, 4) for m in (0, 1)]
            v1_dir = synth.write_plan_set(
                root, "aa", "fixed", p1, cs_sha,
                executions=[{"records": {0: synth.record(p1[0], lambda pl, g, pos: "draw", games=2, status="stopped")},
                             "receipt": False}],
                units=[0, 4], reserved=[0, 4], namespace="read-d8-aa", run="v1",
            )
            self.assertFalse((v1_dir / "exec-0" / "execution.json").exists())
            ok, reason = read_sets.is_d8_interrupted_set(v1_dir)
            self.assertTrue(ok, reason)
            p2 = [synth.plan("aa", u, mirror=m, namespace="read-d8-aa", run="v2") for u in range(4, 8) for m in (0, 1)]
            recs2 = {p["pod_index"]: synth.record(p, lambda pl, g, pos: "draw") for p in p2}
            aa_v2 = synth.write_plan_set(root, "aa", "fixed-restart-1", p2, cs_sha, executions=[{"records": recs2}],
                                         units=[4, 8], reserved=[4, 8], replaces="v1/fixed", retired=[[1, 4]],
                                         namespace="read-d8-aa", run="v2")
            read_sets.open_plan_set(aa_v2, cs_sha)

        # Case (ii): interrupted prep set prep-dose-0 -> accepted for v2/prep-dose-0-restart-1 with retired [[5, 32]].
        with tempfile.TemporaryDirectory() as tmp:
            root = synth.assert_test_temp_root(Path(tmp))
            s1_v1, o_v1 = synth.write_prep_set(root, "dose_panel", "dose", "prep-panel", range(0, 32), cs_sha,
                                               reserved=[0, 32], namespace="dose-d8", run="v1")
            p1 = read_plans.assemble_panel("dose", "dose-d8", read_sets.load_manifest(s1_v1), 0, o_v1,
                                           pins=synth.LIVE_PINS, builders=None, **synth.DEALER,
                                           **synth.LIVE_INDEX_ARGS)
            (root / "dose-d8" / "v1" / "panel").mkdir(parents=True)
            p1_text = read_plans.serialize(p1)
            (root / "dose-d8" / "v1" / "panel" / "panel.json").write_bytes(p1_text.encode("utf-8"))
            p1_sha = hashlib.sha256(p1_text.encode("utf-8")).hexdigest()

            prep_v1_0, _ = synth.write_prep_set(
                root, "dose", "dose", "prep-dose-0", range(0, 32), cs_sha,
                executions=[{"ended": "interrupted", "units": list(range(0, 5)), "accept": False}],
                reserved=[0, 32], panel_sha256=p1_sha, namespace="dose-d8", run="v1",
            )
            ok, reason = read_sets.is_d8_interrupted_set(prep_v1_0)
            self.assertTrue(ok, reason)
            self.assertEqual(read_sets.d8_unused_reserved_ranges(prep_v1_0, read_sets.load_manifest(prep_v1_0)),
                             [[5, 32]])

            s1_v2, o_v2 = synth.write_prep_set(root, "dose_panel", "dose", "prep-panel", range(32, 64), cs_sha,
                                               reserved=[32, 64], namespace="dose-d8", run="v2")
            p2 = read_plans.assemble_panel("dose", "dose-d8", read_sets.load_manifest(s1_v2), 0, o_v2,
                                           pins=synth.LIVE_PINS, builders=None, **synth.DEALER,
                                           **synth.LIVE_INDEX_ARGS)
            (root / "dose-d8" / "v2" / "panel").mkdir(parents=True)
            p2_text = read_plans.serialize(p2)
            (root / "dose-d8" / "v2" / "panel" / "panel.json").write_bytes(p2_text.encode("utf-8"))
            p2_sha = hashlib.sha256(p2_text.encode("utf-8")).hexdigest()
            sm_bytes = read_plans.serialize({"dose_panels": {"v1": p1_sha, "v2": p2_sha}}).encode("utf-8")
            sm_sha = hashlib.sha256(sm_bytes).hexdigest()

            prep_v2_0, _ = synth.write_prep_set(
                root, "dose", "dose", "prep-dose-0-restart-1", range(32, 64), cs_sha,
                reserved=[32, 64], panel_sha256=p2_sha, replaces="v1/prep-dose-0", retired=[[5, 32]],
                namespace="dose-d8", run="v2",
            )
            read_sets.check_prep_dose_set(prep_v2_0, pool=synth.POOL, constants=synth.CONST,
                                          shakedown_manifest=(sm_bytes, sm_sha), **synth.LIVE_INDEX_ARGS)

        # Case (iii): completed set stands on its build and MUST NOT be replaced.
        with tempfile.TemporaryDirectory() as tmp:
            root = synth.assert_test_temp_root(Path(tmp))
            p1 = [synth.plan("aa", u, mirror=m, namespace="read-d8-aa", run="v1") for u in range(0, 4) for m in (0, 1)]
            recs1 = {p["pod_index"]: synth.record(p, lambda pl, g, pos: "draw") for p in p1}
            v1_dir = synth.write_plan_set(root, "aa", "fixed", p1, cs_sha, executions=[{"records": recs1}],
                                          units=[0, 4], reserved=[0, 4], namespace="read-d8-aa", run="v1")
            ok, reason = read_sets.is_d8_interrupted_set(v1_dir)
            self.assertFalse(ok)
            self.assertIn("completed", reason)
            p2 = [synth.plan("aa", u, mirror=m, namespace="read-d8-aa", run="v2") for u in range(4, 8) for m in (0, 1)]
            recs2 = {p["pod_index"]: synth.record(p, lambda pl, g, pos: "draw") for p in p2}
            with self.assertRaisesRegex(read_sets.SetError, "not an interrupted set|completed"):
                synth.write_plan_set(root, "aa", "fixed-restart-1", p2, cs_sha, executions=[{"records": recs2}],
                                     units=[4, 8], reserved=[4, 8], replaces="v1/fixed", retired=[],
                                     namespace="read-d8-aa", run="v2")

        # Different-game watchdog expiry is a void infrastructure failure, NOT a restartable interrupted set.
        with tempfile.TemporaryDirectory() as tmp:
            root = synth.assert_test_temp_root(Path(tmp))
            p1 = [synth.plan("aa", u, mirror=m, namespace="read-d8-aa", run="v1") for u in range(0, 4) for m in (0, 1)]
            wd3 = lambda pl, g, pos: (("halt", dict(synth.WATCHDOG_END), None, 50)
                                      if (pl["pod_index"] == 0 and g["game"] == 3) else "draw")
            wd5 = lambda pl, g, pos: (("halt", dict(synth.WATCHDOG_END), None, 50)
                                      if (pl["pod_index"] == 0 and g["game"] == 5) else "draw")
            e0 = {p["pod_index"]: synth.record(p, wd3) for p in p1}
            e1 = {0: synth.record(p1[0], wd5, games=6, status="stopped")}
            v1_dir = synth.write_plan_set(
                root, "aa", "fixed", p1, cs_sha,
                executions=[{"records": e0}, {"records": e1, "ended": "watchdog_stop", "entry_expired": [[0, 3]]}],
                units=[0, 4], reserved=[0, 4], namespace="read-d8-aa", run="v1",
            )
            ok, reason = read_sets.is_d8_interrupted_set(v1_dir)
            self.assertFalse(ok)
            self.assertIn("infrastructure failure", reason)


class UniformPodRecordTests(unittest.TestCase):
    @unittest.skipUnless(D5_UNIFORM_FIXTURE.exists(),
                         "D5 has not yet produced a uniform pod record; its fixture is added when D5 lands")
    def test_parses_a_uniform_pod_record_from_d5(self):
        records, _ = pod_record.load_records(D5_UNIFORM_FIXTURE)
        for rec in records:
            self.assertEqual(rec["pilot"]["kind"], "uniform")


if __name__ == "__main__":
    unittest.main()
