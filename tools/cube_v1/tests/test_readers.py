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
        self.assertEqual(c["decks"][2]["dealt"]["key"], ["test", 1, 0, 0])
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
                 "build_seed": 7, "cards": cards0, "build_rows": synth.uniform_build_rows(cards0, pools[0]),
                 "dealt": None},
                {"deck": 1, "source": "build", "chair": 1, "build": 0, "builder": dict(synth.UNIFORM),
                 "build_seed": 7, "cards": cards1, "build_rows": synth.uniform_build_rows(cards1, pools[1]),
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

    def test_unmirrored_halt_or_mismatched_halt_fails(self):
        report = self._read(self.symmetric(halt_in=(1,)), "unmirrored")
        self.assertEqual(report["verdict"], "failed")
        self.assertIn("unmirrored halt", report["symmetry"]["problems"][0])
        report = self._read(self.symmetric(halt_in=(0, 1), source=9), "unmirrored")
        self.assertEqual(report["verdict"], "failed")
        self.assertIn("stop kind, source or counts", report["symmetry"]["problems"][0])
        self.assertEqual(estimate(report, "candidate_edge"), F(0))


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


REPORT_GOLDEN_SHA256 = "d620a65158abe209a1373ac9f6fc957041d3e5b8cb86166a8fb41017323c2599"


class UniformPodRecordTests(unittest.TestCase):
    @unittest.skipUnless(D5_UNIFORM_FIXTURE.exists(),
                         "D5 has not yet produced a uniform pod record; its fixture is added when D5 lands")
    def test_parses_a_uniform_pod_record_from_d5(self):
        records, _ = pod_record.load_records(D5_UNIFORM_FIXTURE)
        for rec in records:
            self.assertEqual(rec["pilot"]["kind"], "uniform")


if __name__ == "__main__":
    unittest.main()
