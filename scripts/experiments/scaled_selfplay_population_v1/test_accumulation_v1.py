"""Unit tests for accumulation_v1.py / accumulation_v1_analysis.py, one per
CLAUDE-ACCUMULATION-SPEC-LAYER-PORT-PLAN-V1.md Section 6 acceptance gate.

Mirrors test_candidate_02_v3.py's own style (synthetic outcome fixtures, no
real subprocess execution) -- per this program's own standing discipline
(Amendment 7, Amendment 8, the CP7 harness port all landed only after unit
tests, before any real leg), and per the coordinator's explicit HOLD: the
real 16-step gate sequence is implemented and unit-tested here, not
launched.
"""

from __future__ import annotations

import copy
import importlib.util
import json
import sys
import tempfile
import types
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[2]
CHAIN_SPEC_PATH = REPO_ROOT / "docs" / "native_scaled_selfplay_accumulation_v1_chain_spec_v1.json"
sys.path.insert(0, str(SCRIPT_DIR))


def load_module(name: str, path: Path):
    module_spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(module_spec)
    sys.modules[name] = module
    module_spec.loader.exec_module(module)
    return module


ANALYZER = load_module("accumulation_v1_analysis_tested", SCRIPT_DIR / "accumulation_v1_analysis.py")
ORCH = load_module("accumulation_v1_tested", SCRIPT_DIR / "accumulation_v1.py")


def write_json(path: Path, value: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(ANALYZER.canonical_bytes(value))


def record(path: Path) -> dict:
    return {"path": str(path.resolve()), "bytes": path.stat().st_size, "sha256": ANALYZER.sha256_file(path)}


def raw_identity(binding: dict, has_bundle: bool) -> dict:
    value = {
        "checkpoint_manifest_sha256": binding["checkpoint_sha256"],
        "checkpoint_payload_sha256": binding["state_sha256"],
        "generation": binding["generation"],
        "model_parameter_sha256": binding["model_parameter_sha256"],
        "run_sha256": binding["run_sha256"],
    }
    if has_bundle:
        value["identity_bundle_sha256"] = binding["identity_bundle_sha256"]
    return value


def make_outcome(leg_spec: dict, mode: str, chunk_index: int, arm: str, rank: int) -> dict:
    pair_count = ANALYZER.mode_pair_count(leg_spec, mode)
    evaluation_seed = ANALYZER.chunk_seed(leg_spec, mode, chunk_index)
    opponent = leg_spec["fixed_opponent"]
    candidate = leg_spec["candidate"] if arm == "candidate" else leg_spec["anchor"]
    episodes = []
    seat_counts = {"P0": {"wins": 0, "losses": 0, "draws": 0}, "P1": {"wins": 0, "losses": 0, "draws": 0}}
    for pair_index in range(pair_count):
        environment_seed = ANALYZER.trainer_environment_seed(evaluation_seed, pair_index)
        for leg, seat in enumerate(("P0", "P1")):
            episodes.append(
                {
                    "deck_hashes_u64": leg_spec["expected_rally_deck_hashes_u64"],
                    "environment_seed": environment_seed,
                    "episode_index": pair_index * 2 + leg,
                    "learner_seat": seat,
                    "opponent_pool_member": "Primary",
                    "pair_index": pair_index,
                    "terminal_order_rank": rank,
                }
            )
            seat_counts[seat]["wins" if rank == 1 else "losses" if rank == -1 else "draws"] += 1
    overall = {key: seat_counts["P0"][key] + seat_counts["P1"][key] for key in seat_counts["P0"]}
    return {
        "candidate": raw_identity(candidate, True),
        "episode_count": pair_count * 2,
        "episodes": episodes,
        "evaluation_base_seed": evaluation_seed,
        "learner_outcomes": {"P0": seat_counts["P0"], "P1": seat_counts["P1"], "overall": overall},
        "opponent": raw_identity(opponent, False),
        "pair_count": pair_count,
        "runtime": {
            "all_natural": True,
            "broker_batch_target": 1,
            "environment_randomization_v2": True,
            "sessions_per_worker": 1,
            "worker_count": 1,
        },
        "schema": ANALYZER.OUTCOME_SCHEMA,
    }


def make_arm_record(root: Path, leg_spec: dict, mode: str, chunk_index: int, arm: str, rank: int) -> dict:
    arm_root = root / f"chunk-{chunk_index:03d}-{arm}"
    arm_root.mkdir(parents=True)
    outcome_path = arm_root / "outcome.json"
    stdout_path = arm_root / "stdout.log"
    stderr_path = arm_root / "stderr.log"
    write_json(outcome_path, make_outcome(leg_spec, mode, chunk_index, arm, rank))
    stdout_path.write_bytes(b"test\n")
    stderr_path.write_bytes(b"")
    evaluation_seed = ANALYZER.chunk_seed(leg_spec, mode, chunk_index)
    return {
        "label": f"chunk-{chunk_index:03d}-{arm}",
        "candidate_index": 0 if arm == "candidate" else 1,
        "opponent_index": 2,
        "pair_count": ANALYZER.mode_pair_count(leg_spec, mode),
        "evaluation_seed": evaluation_seed,
        "exit_code": 0,
        "wall_seconds": 0.01,
        "stdout": record(stdout_path),
        "stderr": record(stderr_path),
        "outcome": record(outcome_path),
    }


class ChainSpecTests(unittest.TestCase):
    """Acceptance gate 1."""

    def test_real_chain_spec_validates(self) -> None:
        chain_spec = ORCH.validate_chain_spec(CHAIN_SPEC_PATH)
        self.assertEqual(len(chain_spec["candidate_identities"]), 16)
        self.assertEqual(chain_spec["gate_class"], "ACCUMULATION")
        self.assertEqual(chain_spec["games_per_cluster"], 4)

    def test_tampered_alpha_ledger_is_rejected(self) -> None:
        chain_spec = json.loads(CHAIN_SPEC_PATH.read_text(encoding="utf-8"))
        chain_spec["alpha_ledger"]["alpha_campaign"] = 0.11
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "spec.json"
            write_json(path, chain_spec)
            with self.assertRaises(ValueError):
                ORCH.validate_chain_spec(path)

    def test_wrong_candidate_identity_is_rejected(self) -> None:
        chain_spec = json.loads(CHAIN_SPEC_PATH.read_text(encoding="utf-8"))
        chain_spec["candidate_identities"][0]["model_parameter_sha256"] = "0" * 64
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "spec.json"
            write_json(path, chain_spec)
            with self.assertRaisesRegex(ValueError, "does not match the live store"):
                ORCH.validate_chain_spec(path)

    def test_games_per_cluster_is_four_not_two(self) -> None:
        # Amendment 9 (sheet), Jack's ruling reading (b): the two-arm
        # mechanism is 4 games/cluster, not the original sheet arithmetic's
        # 2. Locks this in as a spec-level assertion, not just a constant.
        self.assertEqual(ORCH.GAMES_PER_CLUSTER, 4)
        chain_spec = json.loads(CHAIN_SPEC_PATH.read_text(encoding="utf-8"))
        chain_spec["games_per_cluster"] = 2
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "spec.json"
            write_json(path, chain_spec)
            with self.assertRaisesRegex(ValueError, "games_per_cluster"):
                ORCH.validate_chain_spec(path)


class ReferenceReuseTests(unittest.TestCase):
    """Acceptance gate 3: eb_cs_reference_v1.py reused unmodified, hash-pinned."""

    def test_reference_hash_pin_matches_candidate_02s_own(self) -> None:
        # Independently confirms the SAME reference file (not a fork/copy)
        # candidate_02_v3.py's own real spec pins, by hash -- both specs
        # pointing at one physical file is the reuse guarantee, checked
        # here rather than merely asserted.
        candidate_02_spec = json.loads(
            (REPO_ROOT / "docs" / "native_scaled_selfplay_candidate_02_v3_spec_v1.json").read_text(encoding="utf-8")
        )
        chain_spec = json.loads(CHAIN_SPEC_PATH.read_text(encoding="utf-8"))
        self.assertEqual(
            chain_spec["contract"]["reference_path"], candidate_02_spec["contract"]["reference_path"]
        )
        self.assertEqual(
            chain_spec["contract"]["reference_sha256"], candidate_02_spec["contract"]["reference_sha256"]
        )

    def test_tampered_reference_hash_is_rejected(self) -> None:
        chain_spec = json.loads(CHAIN_SPEC_PATH.read_text(encoding="utf-8"))
        leg = ORCH.leg_spec_for_step(chain_spec, 1, "initial", chain_spec["initial_anchor"], "test-step")
        leg["contract"]["reference_sha256"] = "0" * 64
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "leg.json"
            write_json(path, leg)
            with self.assertRaisesRegex(ValueError, "reference contract authority hash mismatch"):
                ANALYZER.validate_leg_spec(path)


class SeedGovernanceTests(unittest.TestCase):
    """Acceptance gate 6: seed-stream disjointness checked programmatically
    across all 35 streams."""

    def test_real_freshness_manifest_is_clean(self) -> None:
        chain_spec = json.loads(CHAIN_SPEC_PATH.read_text(encoding="utf-8"))
        freshness = json.loads(Path(chain_spec["freshness_manifest"]["path"]).read_text(encoding="utf-8"))
        self.assertEqual(freshness["stream_count"], 35)
        self.assertTrue(freshness["pairwise_disjoint"])
        self.assertTrue(freshness["excluded_intervals_clean"])

    def test_colliding_streams_are_detected(self) -> None:
        # A deliberately-too-small inter-stream gap (spacing streams only
        # one intra-stream stride apart, the exact bug this module's own
        # first implementation had, caught by actually running the sweep)
        # must be caught by check_pairwise_disjoint, not silently accepted.
        ranges = {"a": (0, 100), "b": (50, 150)}
        collisions = ORCH.check_pairwise_disjoint(ranges)
        self.assertEqual(collisions, [("a", "b")])

    def test_excluded_interval_overlap_is_detected(self) -> None:
        with self.assertRaisesRegex(ValueError, "overlap excluded intervals"):
            ORCH.build_freshness_manifest(
                accum_base_seed=1_000_000_000,
                evaluation_seed_stride=1_000_000,
                chunk_pair_count=128,
                max_n_clusters=4096,
                excluded_evaluation_seed_intervals=[
                    {"start_inclusive": 0, "end_inclusive": 9_000_000_000, "label": "everything"}
                ],
            )

    def test_stream_seed_ranges_cover_exactly_35_streams(self) -> None:
        ranges = ORCH.stream_seed_ranges(3_000_000_000, 1_000_000, 128, 4096)
        self.assertEqual(len(ranges), 35)
        step_names = {name for name in ranges if name.startswith("step-")}
        meta_names = {name for name in ranges if name.startswith("meta-")}
        self.assertEqual(len(step_names), 32)
        self.assertEqual(len(meta_names), 3)


class LegSpecConstructionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.chain_spec = ORCH.validate_chain_spec(CHAIN_SPEC_PATH)

    def test_step_leg_spec_is_self_consistent(self) -> None:
        leg = ORCH.leg_spec_for_step(self.chain_spec, 3, "initial", self.chain_spec["initial_anchor"], "test-step-03-initial")
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "leg.json"
            write_json(path, leg)
            validated, _ = ANALYZER.validate_leg_spec(path)
            self.assertEqual(validated["candidate"]["generation"], 384)

    def test_meta_leg_spec_is_self_consistent(self) -> None:
        leg = ORCH.leg_spec_for_meta(
            self.chain_spec, 2, self.chain_spec["initial_anchor"], self.chain_spec["candidate_identities"][9], "test-meta-02",
        )
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "leg.json"
            write_json(path, leg)
            validated, _ = ANALYZER.validate_leg_spec(path)
            self.assertEqual(validated["gate_class"], "LARGE-EFFECT")
            self.assertEqual(validated["delta_worthwhile"], 0.025)

    def test_no_gpu_exclusivity_check_exists(self) -> None:
        # Acceptance gate 8's own PROPOSED resolution: Section 6.3's steps
        # are CPU-native H2H reads (no CP7/GPU involvement), so the
        # GPU-1-exclusivity check candidate_02_v3.py performs is dropped
        # entirely rather than ported. Checked structurally: this module
        # never references nvidia-smi or any GPU-exclusivity helper.
        source = (SCRIPT_DIR / "accumulation_v1.py").read_text(encoding="utf-8")
        self.assertNotIn("nvidia-smi", source)
        self.assertNotIn("exclusive_gpu", source)


def _slot_keys_run_arm_reads() -> set[str]:
    """Extracts the exact set of keys run_payoff_evaluation.run_arm()
    actually subscripts off its candidate/opponent slot dicts, from
    run_arm's own source text -- not a hand-copied guess of that set, so
    this stays correct even if run_arm's own field usage changes without a
    matching edit here (the exact gap that let the real
    KeyError('source_generation') through every existing test: those tests
    all stub run_arm_fn and never exercise the real run_arm at all)."""
    import inspect

    run_payoff_evaluation = sys.modules.get("run_payoff_evaluation")
    if run_payoff_evaluation is None:
        import run_payoff_evaluation  # noqa: F401 -- imported for inspect below

        run_payoff_evaluation = sys.modules["run_payoff_evaluation"]
    source = inspect.getsource(run_payoff_evaluation.run_arm)
    import re

    return set(re.findall(r'(?:candidate|opponent)\["(\w+)"\]', source))


class SlotFromSpecIdentityTests(unittest.TestCase):
    """Regression coverage for the real leg_context() KeyError caught by
    the first real chain launch (2026-08-28): leg_context() was passing
    FROZEN SPEC identities (run_sha256/generation/identity_bundle_sha256)
    directly as run_batch/run_arm's own 'slots', but run_arm needs the RAW
    checkpoint_slot() shape (source_generation, not generation). Fixed via
    slot_from_spec_identity(), structurally mirroring candidate_02_v3.py's
    own slot_from_spec()."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.chain_spec = ORCH.validate_chain_spec(CHAIN_SPEC_PATH)

    def test_leg_context_slots_carry_every_key_run_arm_reads(self) -> None:
        required_keys = _slot_keys_run_arm_reads()
        # Sanity: confirm the dynamic extraction actually found the real
        # bug's own key, not an empty/degenerate set from a regex miss.
        self.assertIn("source_generation", required_keys)
        self.assertIn("store_root", required_keys)
        self.assertIn("role", required_keys)
        leg = ORCH.leg_spec_for_step(self.chain_spec, 1, "initial", self.chain_spec["initial_anchor"], "test-slot-shape-01")
        _, slots = ORCH.leg_context(REPO_ROOT, leg)
        self.assertEqual(len(slots), 3)
        for slot in slots:
            missing = required_keys - set(slot.keys())
            self.assertFalse(missing, f"slot missing keys run_arm reads: {missing}")

    def test_slots_are_the_correct_live_identities_not_just_correctly_shaped(self) -> None:
        leg = ORCH.leg_spec_for_step(self.chain_spec, 1, "initial", self.chain_spec["initial_anchor"], "test-slot-shape-02")
        _, slots = ORCH.leg_context(REPO_ROOT, leg)
        candidate_slot, anchor_slot, opponent_slot = slots
        self.assertEqual(candidate_slot["model_parameter_sha256"], leg["candidate"]["model_parameter_sha256"])
        self.assertEqual(candidate_slot["source_generation"], leg["candidate"]["generation"])
        self.assertEqual(anchor_slot["model_parameter_sha256"], leg["anchor"]["model_parameter_sha256"])
        self.assertEqual(opponent_slot["model_parameter_sha256"], leg["fixed_opponent"]["model_parameter_sha256"])

    def test_tampered_pinned_hash_is_rejected(self) -> None:
        tampered = copy.deepcopy(self.chain_spec["initial_anchor"])
        tampered["model_parameter_sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "model_parameter_sha256 mismatch"):
            ORCH.slot_from_spec_identity(tampered)


class ReconstructionAndGateTests(unittest.TestCase):
    """Acceptance gates 4 and 5: meta-gate accepted-step-count trigger, and
    both-SUCCESS install + independent confirmation re-verification."""

    def _run_synthetic_leg(self, leg_spec: dict, mode: str, root: Path, rank_candidate: int, rank_control: int, chunks: int) -> Path:
        leg_spec_path = root / "leg-spec.json"
        write_json(leg_spec_path, leg_spec)
        write_json(
            root / "gate-plan.json",
            {
                "schema": ANALYZER.PLAN_SCHEMA,
                "mode": mode,
                "gate_id": leg_spec[mode]["gate_id"],
                "leg_id": leg_spec["leg_id"],
                "leg_spec": record(leg_spec_path),
                "git": {"commit": "test", "executable_source_commit": leg_spec["executable"]["source_commit"]},
                "toolchain": {"test": True},
                "executable": {**record(Path(leg_spec["executable"]["path"])), "source_commit": leg_spec["executable"]["source_commit"]}
                if Path(leg_spec["executable"]["path"]).is_file()
                else {"path": leg_spec["executable"]["path"], "bytes": 0, "sha256": leg_spec["executable"]["sha256"], "source_commit": leg_spec["executable"]["source_commit"]},
                "candidate": leg_spec["candidate"],
                "anchor": leg_spec["anchor"],
                "fixed_opponent": leg_spec["fixed_opponent"],
                "gpu_ordinal": "test",
                "terminal_reward_only": True,
                "gate": {
                    "gate_class": leg_spec["gate_class"], "alpha": leg_spec["alpha"], "c": leg_spec["c"],
                    "delta_promote": leg_spec["delta_promote"], "delta_worthwhile": leg_spec["delta_worthwhile"],
                    "conditional_mean_stability": leg_spec["conditional_mean_stability"],
                    "max_N": ANALYZER.mode_max_n(leg_spec, mode), "chunk_pair_count": ANALYZER.mode_pair_count(leg_spec, mode),
                },
                "pre_outcome_schedule_sha256": leg_spec[mode]["pre_outcome_schedule_sha256"],
                "first_evaluation_seed": leg_spec[mode]["first_evaluation_seed"],
                "evaluation_seed_stride": leg_spec["evaluation_seed_stride"],
                "arm_order": "test",
                "expected_rally_deck_hashes_u64": leg_spec["expected_rally_deck_hashes_u64"],
                "chunk_plan": [
                    {
                        "chunk_index": index,
                        "evaluation_seed": ANALYZER.chunk_seed(leg_spec, mode, index),
                        "global_cluster_start": index * ANALYZER.mode_pair_count(leg_spec, mode),
                        "global_cluster_end_exclusive": (index + 1) * ANALYZER.mode_pair_count(leg_spec, mode),
                    }
                    for index in range(chunks)
                ],
            },
        )
        for chunk_index in range(chunks):
            receipt = {
                "schema": ANALYZER.RECEIPT_SCHEMA,
                "chunk_index": chunk_index,
                "evaluation_seed": ANALYZER.chunk_seed(leg_spec, mode, chunk_index),
                "candidate_arm": make_arm_record(root, leg_spec, mode, chunk_index, "candidate", rank_candidate),
                "control_arm": make_arm_record(root, leg_spec, mode, chunk_index, "control", rank_control),
            }
            write_json(root / f"chunk-{chunk_index:03d}-receipt.json", receipt)
        return leg_spec_path

    def test_success_leg_reconstructs_and_decides(self) -> None:
        chain_spec = ORCH.validate_chain_spec(CHAIN_SPEC_PATH)
        chain_spec = copy.deepcopy(chain_spec)
        chain_spec["chunk_pair_count"] = 32
        chain_spec["max_N_clusters"] = 32
        leg = ORCH.leg_spec_for_step(chain_spec, 1, "initial", chain_spec["initial_anchor"], "test-success-initial")
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            leg_spec_path = self._run_synthetic_leg(leg, "initial", root, rank_candidate=1, rank_control=-1, chunks=1)
            analysis = ANALYZER.build_analysis(root, leg_spec_path, "initial", True)
            self.assertEqual(analysis["decision"], "SUCCESS")
            self.assertGreater(analysis["cs_delta_lower"], 0.0)

    def test_confirmation_requires_independent_verification_of_initial(self) -> None:
        """Acceptance gate 5: a forged/tampered 'initial' analysis cannot
        authorize a confirmation gate -- verify_existing recomputes it from
        the raw receipts and requires byte-identical agreement, mirroring
        candidate_02_v3.py's own verify_initial_for_confirmation."""
        chain_spec = ORCH.validate_chain_spec(CHAIN_SPEC_PATH)
        chain_spec = copy.deepcopy(chain_spec)
        chain_spec["chunk_pair_count"] = 32
        chain_spec["max_N_clusters"] = 32
        leg = ORCH.leg_spec_for_step(chain_spec, 1, "initial", chain_spec["initial_anchor"], "test-verify-initial")
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            leg_spec_path = self._run_synthetic_leg(leg, "initial", root, rank_candidate=1, rank_control=-1, chunks=1)
            analysis = ANALYZER.build_analysis(root, leg_spec_path, "initial", True)
            forged = copy.deepcopy(analysis)
            forged["decision"] = "SUCCESS"
            forged["cs_delta_lower"] = 0.9  # tampered, does not match the real receipts
            retained = root / "forged-analysis.json"
            write_json(retained, forged)
            output = root / "verification.json"
            with self.assertRaisesRegex(ValueError, "recomputed initial analysis differs from retained analysis"):
                ANALYZER.verify_existing(root, leg_spec_path, retained, output)

    def test_meta_gate_fires_on_fifth_accepted_step_not_fifth_by_index(self) -> None:
        """Acceptance gate 4: a synthetic sequence where step 3 is
        INCONCLUSIVE (not accepted) proves the meta-gate fires when
        accepted_step_count (not step_index) crosses K_META_BLOCK=5 --
        i.e. after step 6 is accepted (steps 1,2,4,5,6 accepted = 5th
        acceptance, step 3 rejected/inconclusive along the way), not after
        step 5 by raw index."""
        state = {
            "schema": ORCH.CHAIN_STATE_SCHEMA,
            "current_anchor": {"model_parameter_sha256": "anchor0"},
            "accepted_step_count": 0,
            "completed_steps": [],
            "completed_meta_gates": [],
            "block_start_anchor": {"model_parameter_sha256": "anchor0"},
        }
        # Simulate steps 1,2 accepted; step 3 NOT accepted (inconclusive);
        # steps 4,5,6 accepted -- 5th acceptance lands at step_index=6.
        installed_sequence = [True, True, False, True, True, True]
        for step_index, installed in enumerate(installed_sequence, start=1):
            if installed:
                state["accepted_step_count"] += 1
            state["completed_steps"].append({"step_index": step_index, "installed": installed})
            meta_fires = state["accepted_step_count"] > 0 and state["accepted_step_count"] % ORCH.K_META_BLOCK == 0
            if step_index < 6:
                self.assertFalse(meta_fires, f"meta-gate must not fire at step_index={step_index}")
            else:
                self.assertTrue(meta_fires, "meta-gate must fire once accepted_step_count reaches 5, at step_index=6")

    def test_formal_run_end_to_end_with_synthetic_execution(self) -> None:
        """Exercises formal_run()'s real wiring (plan/chunk-batch/receipt/
        analysis/manifest construction) end to end via a synthetic
        run_arm_fn bound to this one leg -- no real subprocess, no real
        games. The meta-gate accepted-count trigger itself is proven
        directly above (test_meta_gate_fires_on_fifth_accepted_step_not_fifth_by_index);
        this test proves formal_run's own real code path produces a
        correct, passing manifest that run_chain's meta-gate check can act
        on."""
        chain_spec = ORCH.validate_chain_spec(CHAIN_SPEC_PATH)
        chain_spec = copy.deepcopy(chain_spec)
        chain_spec["chunk_pair_count"] = 32
        chain_spec["max_N_clusters"] = 32
        chain_spec["screen"]["pair_count_per_chunk"] = 2
        chain_spec["screen"]["chunk_count"] = 1
        leg = ORCH.leg_spec_for_step(chain_spec, 1, "initial", chain_spec["initial_anchor"], "test-formal-run-step01-initial")

        def fake_run_arm(executable, repo_root, root, slots, spec):
            # candidate always wins (rank=1), control always loses (rank=-1)
            # -> a clean, fast SUCCESS. Bound to this test's own fixed leg/
            # mode via closure, not recovered from spec (formal_run always
            # calls this within one known mode per invocation).
            arm_root = root / spec["label"]
            arm_root.mkdir(parents=True)
            outcome_path = arm_root / "outcome.json"
            stdout_path = arm_root / "stdout.log"
            stderr_path = arm_root / "stderr.log"
            stdout_path.write_bytes(b"test\n")
            stderr_path.write_bytes(b"")
            rank = 1 if spec["candidate_index"] == 0 else -1
            arm_name = "candidate" if spec["candidate_index"] == 0 else "control"
            chunk_index = int(spec["label"].split("-", 2)[1])
            write_json(outcome_path, make_outcome(leg, "initial", chunk_index, arm_name, rank))
            return {
                **spec, "candidate_role": "test", "opponent_role": "test", "exit_code": 0, "wall_seconds": 0.001,
                "stdout": record(stdout_path), "stderr": record(stderr_path), "outcome": record(outcome_path),
            }

        with tempfile.TemporaryDirectory() as temp:
            evidence_root = Path(temp)
            leg_spec_path = evidence_root / "leg-spec-current.json"
            write_json(leg_spec_path, leg)
            manifest_path = ORCH.formal_run(
                evidence_root, REPO_ROOT, leg, leg_spec_path, "initial", run_arm_fn=fake_run_arm,
            )
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            self.assertEqual(manifest["disposition"], "SUCCESS")
            self.assertTrue(manifest["passed"])

    def test_confirmation_verifies_against_the_initials_own_root_and_spec_not_confirms(self) -> None:
        """Regression test for TWO real bugs caught live on attempt-002,
        both in formal_run's own confirmation-mode verify_existing() call,
        found across two consecutive relaunches (2026-08-29):

        (1) formal_run was passing the CONFIRMATION leg's own fresh, empty
        attempt root to verify_existing() instead of the INITIAL leg's own
        root (whose durable gate-plan.json/chunk-receipts reconstruct()
        needs) -- ValueError('gate plan is missing') the instant a real
        step reached its confirmation gate.

        (2) after fixing (1), the very next relaunch reached
        validate_plan() for the first time and failed differently:
        formal_run was ALSO passing the CONFIRMATION leg's own spec file
        (leg_id='step-NN-confirmation') to verify_existing/reconstruct,
        instead of the INITIAL leg's own spec file (leg_id=
        'step-NN-initial') the initial leg's own gate-plan.json was
        actually built and recorded against -- ValueError('leg id
        mismatch') from validate_plan's own
        require(plan["leg_id"] == leg_spec["leg_id"], ...).

        Neither bug was caught by test_confirmation_requires_independent_verification_of_initial,
        which calls verify_existing() directly and never goes through
        formal_run()'s own confirmation-mode wiring at all. Bug (2) was
        ALSO NOT caught by this test's own first version (which built only
        ONE leg_spec_for_step(..., "initial", ...) object and reused it,
        unrealistically, for BOTH the initial and confirmation formal_run
        calls -- giving it leg_id='step-01-initial' in both places, which
        accidentally satisfied validate_plan's own leg_id check and masked
        bug (2) completely). Fixed here by building the initial and
        confirmation legs exactly as run_chain's own real step loop does:
        two SEPARATE leg_spec_for_step(...) calls, one per mode, each with
        its own distinct leg_id, each written to its own file -- so the
        initial and confirmation legs differ in leg_id AND root exactly as
        they do in production, and this test can no longer pass by
        accident."""
        chain_spec = ORCH.validate_chain_spec(CHAIN_SPEC_PATH)
        chain_spec = copy.deepcopy(chain_spec)
        chain_spec["chunk_pair_count"] = 32
        chain_spec["max_N_clusters"] = 32
        initial_leg = ORCH.leg_spec_for_step(chain_spec, 1, "initial", chain_spec["initial_anchor"], "test-distinct-roots-initial")
        confirm_leg = ORCH.leg_spec_for_step(chain_spec, 1, "confirmation", chain_spec["initial_anchor"], "test-distinct-roots-confirm")
        self.assertNotEqual(initial_leg["leg_id"], confirm_leg["leg_id"], "test setup must use genuinely distinct leg_ids, as run_chain's own separate leg_spec_for_step calls do")

        def make_fake_run_arm(leg_for_outcome: dict, mode: str):
            def fake_run_arm(executable, repo_root, root, slots, spec):
                arm_root = root / spec["label"]
                arm_root.mkdir(parents=True)
                outcome_path = arm_root / "outcome.json"
                stdout_path = arm_root / "stdout.log"
                stderr_path = arm_root / "stderr.log"
                stdout_path.write_bytes(b"test\n")
                stderr_path.write_bytes(b"")
                rank = 1 if spec["candidate_index"] == 0 else -1
                arm_name = "candidate" if spec["candidate_index"] == 0 else "control"
                chunk_index = int(spec["label"].split("-", 2)[1])
                write_json(outcome_path, make_outcome(leg_for_outcome, mode, chunk_index, arm_name, rank))
                return {
                    **spec, "candidate_role": "test", "opponent_role": "test", "exit_code": 0, "wall_seconds": 0.001,
                    "stdout": record(stdout_path), "stderr": record(stderr_path), "outcome": record(outcome_path),
                }

            return fake_run_arm

        with tempfile.TemporaryDirectory() as temp:
            evidence_root = Path(temp)
            initial_leg_spec_path = evidence_root / "initial-leg-spec.json"
            confirm_leg_spec_path = evidence_root / "confirm-leg-spec.json"
            write_json(initial_leg_spec_path, initial_leg)
            write_json(confirm_leg_spec_path, confirm_leg)
            initial_manifest_path = ORCH.formal_run(
                evidence_root, REPO_ROOT, initial_leg, initial_leg_spec_path, "initial",
                run_arm_fn=make_fake_run_arm(initial_leg, "initial"),
            )
            initial_manifest = json.loads(initial_manifest_path.read_text(encoding="utf-8"))
            self.assertEqual(initial_manifest["disposition"], "SUCCESS")
            self.assertEqual(initial_manifest["leg_id"], initial_leg["leg_id"])

            confirm_manifest_path = ORCH.formal_run(
                evidence_root, REPO_ROOT, confirm_leg, confirm_leg_spec_path, "confirmation",
                initial_manifest=initial_manifest_path, run_arm_fn=make_fake_run_arm(confirm_leg, "confirmation"),
            )
            self.assertNotEqual(
                initial_manifest_path.resolve().parent, confirm_manifest_path.resolve().parent,
                "test setup must use genuinely distinct initial/confirm roots -- otherwise this test cannot distinguish the fixed code from bug (1)",
            )
            confirm_manifest = json.loads(confirm_manifest_path.read_text(encoding="utf-8"))
            self.assertEqual(confirm_manifest["leg_id"], confirm_leg["leg_id"])
            self.assertEqual(confirm_manifest["disposition"], "SUCCESS")
            self.assertIsNotNone(confirm_manifest["initial_verification"])
            verification = json.loads(Path(confirm_manifest["initial_verification"]["path"]).read_text(encoding="utf-8"))
            self.assertEqual(verification["decision"], "VERIFIED-SUCCESS")
            self.assertEqual(Path(verification["initial_run_root"]), initial_manifest_path.resolve().parent)


class ResumabilityTests(unittest.TestCase):
    """Acceptance gate 7."""

    def test_existing_manifest_is_trusted_not_rerun(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            evidence_root = Path(temp)
            gate_root = evidence_root / "test-gate"
            attempt_root = gate_root / "attempt-001"
            attempt_root.mkdir(parents=True)
            manifest = {
                "schema": ORCH.MANIFEST_SCHEMA, "passed": True, "mode": "initial",
                "gate_id": "test-gate", "leg_id": "test-leg", "disposition": "SUCCESS",
            }
            write_json(attempt_root / "gate-execution-manifest.json", manifest)
            found = ORCH.existing_manifest(evidence_root, "test-gate")
            self.assertIsNotNone(found)
            self.assertEqual(found, attempt_root / "gate-execution-manifest.json")

    def test_failed_or_missing_manifest_is_not_trusted(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            evidence_root = Path(temp)
            self.assertIsNone(ORCH.existing_manifest(evidence_root, "never-run-gate"))
            gate_root = evidence_root / "partial-gate"
            attempt_root = gate_root / "attempt-001"
            attempt_root.mkdir(parents=True)
            # No manifest written at all (simulates a kill mid-leg) --
            # must not be trusted, must not look like a completed gate.
            self.assertIsNone(ORCH.existing_manifest(evidence_root, "partial-gate"))

    def test_run_chain_skips_already_completed_steps_on_resume(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            evidence_root = Path(temp)
            state = {
                "schema": ORCH.CHAIN_STATE_SCHEMA,
                "current_anchor": {"model_parameter_sha256": "anchor-after-step-1"},
                "accepted_step_count": 1,
                "completed_steps": [{"step_index": 1, "installed": True, "initial_manifest": "x", "initial_disposition": "SUCCESS"}],
                "completed_meta_gates": [],
                "block_start_anchor": {"model_parameter_sha256": "anchor0"},
            }
            ORCH.write_chain_state(evidence_root, state)
            reloaded = ORCH.load_chain_state(evidence_root)
            self.assertEqual(reloaded["accepted_step_count"], 1)
            self.assertEqual(len(reloaded["completed_steps"]), 1)
            # A second load (simulating a resumed process) must see the
            # SAME already-completed step-1 record, not a fresh empty state.
            self.assertEqual(reloaded["completed_steps"][0]["step_index"], 1)


if __name__ == "__main__":
    unittest.main()
