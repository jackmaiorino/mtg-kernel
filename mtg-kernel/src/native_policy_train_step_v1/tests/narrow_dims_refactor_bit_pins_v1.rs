//! Narrow byte-identity pins for the generic-dims CPU train step (collab
//! LANES/throughput-architecture-20261020/WIDE-BACKWARD-DESIGN.md v2.1,
//! section 4.1). Every CPU route runs three consecutive updates on the
//! runner-fixed net-8 model. Each update yields one digest over every
//! numerical field the step returns (each float by its exact bits), its
//! secondary results (JSON as canonical bytes) and the post-update state
//! digest. The effective GAE forward and exact-backward worker counts are
//! recorded beside each digest, so the preparation-worker pins also prove
//! that 1, 2 and 4 workers give one digest.
//!
//! The expected digests were captured by running this test on main's code
//! before the refactor, per target. An empty table is capture mode: the test
//! prints every pin line and fails.

use super::*;
use crate::line_b_teacher_target_v1::{line_b_teacher_target_v1, LineBDivergenceV1};
use crate::native_flat_tensorizer_v3::{encoded_decision_view_v3, NativeFlatDecisionTensorV3};
use crate::native_flat_tensorizer_v4::{
    encoded_decision_view_v4, NativeFlatDecisionTensorV4, NativeFlatTensorizerV4,
};
use crate::native_policy_train_step_v1::retention_v1::{
    RetainedImitationResultV1, RetentionGroupV1, RetentionRowV1,
};
use crate::native_policy_value_net_v1::ForwardActivationModeV1;
use crate::sideboard_play_policy_v1::FreshLineageGenerationV1;

const UPDATES_V1: usize = 3;
const VC: f32 = 0.75;
const LR: f32 = 0.00003;
/// The no-state-digest columns of the state encoder input.
const DIGEST_COLUMNS_V1: std::ops::Range<usize> = 123..219;

// Exact pins are scoped to the pinned toolchain and target tuple, as the
// fixed-partition state witness is: platform libm may differ in last bits.
// Each line is `route update forward_workers backward_workers digest`.
#[cfg(all(target_arch = "x86_64", target_os = "windows", target_env = "msvc"))]
const EXPECTED_TARGET_V1: &str = "x86_64-pc-windows-msvc";
#[cfg(all(target_arch = "x86_64", target_os = "windows", target_env = "msvc"))]
const EXPECTED_PINS_V1: &str = "";

#[cfg(all(target_arch = "x86_64", target_os = "linux", target_env = "gnu"))]
const EXPECTED_TARGET_V1: &str = "x86_64-unknown-linux-gnu";
#[cfg(all(target_arch = "x86_64", target_os = "linux", target_env = "gnu"))]
const EXPECTED_PINS_V1: &str = "";

// Unknown targets fail closed in capture mode.
#[cfg(not(any(
    all(target_arch = "x86_64", target_os = "windows", target_env = "msvc"),
    all(target_arch = "x86_64", target_os = "linux", target_env = "gnu")
)))]
const EXPECTED_TARGET_V1: &str = "an unreviewed target";
#[cfg(not(any(
    all(target_arch = "x86_64", target_os = "windows", target_env = "msvc"),
    all(target_arch = "x86_64", target_os = "linux", target_env = "gnu")
)))]
const EXPECTED_PINS_V1: &str = "";

/// SHA-256 over a length-prefixed canonical encoding: every count as a
/// little-endian u64 and every float by its exact bits. It binds the update
/// index but not the route name, so routes that must agree (worker counts)
/// compare digests directly.
struct PinDigestV1(Sha256);

impl PinDigestV1 {
    fn new(update: usize) -> Self {
        let mut digest = Self(Sha256::new());
        digest.bytes(b"mtg-kernel/narrow-dims-refactor-bit-pin/v1");
        digest.count(update);
        digest
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.0.update((bytes.len() as u64).to_le_bytes());
        self.0.update(bytes);
    }

    fn count(&mut self, value: usize) {
        self.0.update((value as u64).to_le_bytes());
    }

    fn f32(&mut self, value: f32) {
        self.0.update(value.to_bits().to_le_bytes());
    }

    fn f64(&mut self, value: f64) {
        self.0.update(value.to_bits().to_le_bytes());
    }

    fn json(&mut self, value: &serde_json::Value) {
        self.bytes(&serde_json::to_vec(value).expect("pinned JSON serializes"));
    }

    fn optional_json(&mut self, value: Option<&serde_json::Value>) {
        match value {
            None => self.count(0),
            Some(value) => {
                self.count(1);
                self.json(value);
            }
        }
    }

    fn tensors(&mut self, tensors: &[NativeNamedParameterV1]) {
        self.count(tensors.len());
        for tensor in tensors {
            self.bytes(tensor.name.as_bytes());
            self.count(tensor.shape.len());
            for &dimension in &tensor.shape {
                self.count(dimension);
            }
            let bits = tensor
                .values
                .iter()
                .flat_map(|value| value.to_bits().to_le_bytes())
                .collect::<Vec<_>>();
            self.bytes(&bits);
        }
    }

    fn gauge(&mut self, gauge: &NativeScorerBiasGaugeRecordV1) {
        self.bytes(gauge.parameter_name.as_bytes());
        self.count(gauge.substep_count);
        self.count(gauge.total_action_count);
        self.count(gauge.max_action_count);
        self.f64(gauge.sum_abs_policy_coefficients);
        self.count(gauge.substep_bounds.len());
        for bound in &gauge.substep_bounds {
            self.count(bound.action_count);
            self.f64(bound.abs_policy_coefficient);
            self.count(bound.gamma_operation_count);
            self.f64(bound.gamma);
            self.f64(bound.bound_component);
        }
        self.f64(gauge.per_substep_bound_sum);
        self.f64(gauge.cross_substep_bound);
        self.f32(gauge.raw_gradient_residual);
        self.f64(gauge.derived_absolute_bound);
        self.f64(gauge.high_precision_residual);
        self.f32(gauge.canonical_gradient);
        self.0.update(gauge.parameter_before_bits.to_le_bytes());
        self.0.update(gauge.parameter_after_bits.to_le_bytes());
    }

    fn result(&mut self, result: &NativePolicyTrainStepResultV1) {
        self.f32(result.policy_sum);
        self.f32(result.value_sum);
        self.f32(result.loss);
        self.0.update(result.adam_step.to_le_bytes());
        self.count(result.selected_outputs.len());
        for output in &result.selected_outputs {
            self.count(output.group_index);
            self.count(output.substep_index);
            self.count(output.selected_action_index);
            self.f32(output.selected_logit);
            self.f32(output.value);
            self.f32(output.selected_log_probability);
        }
        self.count(result.physical_terms.len());
        for term in &result.physical_terms {
            self.f32(term.joint_log_probability);
            self.f32(term.value);
            self.0.update(term.terminal_return.to_le_bytes());
            self.0.update(term.substep_count.to_le_bytes());
        }
        self.tensors(&result.gradients);
        self.gauge(&result.scorer_bias_gauge);
    }

    fn auxiliary(&mut self, auxiliary: &LineBAuxiliaryResultV1) {
        self.json(&auxiliary.gradient_components);
        self.count(auxiliary.divergences_before.len());
        for divergence in &auxiliary.divergences_before {
            match divergence {
                None => self.count(0),
                Some(value) => {
                    self.count(1);
                    self.f64(*value);
                }
            }
        }
        self.f64(auxiliary.auxiliary_loss);
        self.f64(auxiliary.auxiliary_head_l2);
        self.f64(auxiliary.ordinary_head_l2);
        self.f32(auxiliary.auxiliary_bias_residual);
        self.optional_json(auxiliary.cap.as_ref());
        self.optional_json(auxiliary.adam_steps.as_ref());
    }

    fn retained(&mut self, result: &RetainedImitationResultV1) {
        self.f32(result.teacher_loss);
        self.f32(result.retention_kl);
        self.f32(result.weighted_retention_loss);
        self.f32(result.loss);
        self.0.update(result.adam_step.to_le_bytes());
        self.tensors(&result.gradients);
        self.gauge(&result.teacher_gauge);
        match &result.retention_gauge {
            None => self.count(0),
            Some(gauge) => {
                self.count(1);
                self.gauge(gauge);
            }
        }
    }

    fn finish(self) -> String {
        hex_sha256(self.0.finalize().into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PinV1 {
    route: &'static str,
    update: usize,
    forward_workers: usize,
    backward_workers: usize,
    digest: String,
}

impl PinV1 {
    fn line_v1(&self) -> String {
        format!(
            "{} {} {} {} {}",
            self.route, self.update, self.forward_workers, self.backward_workers, self.digest
        )
    }
}

/// Three consecutive updates of one route from `state`; each digest also
/// covers the post-update state digest.
fn updates_v1(
    route: &'static str,
    mut state: NativePolicyValueTrainStateV1,
    mut update: impl FnMut(&mut NativePolicyValueTrainStateV1, &mut PinDigestV1),
) -> Vec<PinV1> {
    (0..UPDATES_V1)
        .map(|index| {
            take_effective_update_workers_for_test_v1();
            let mut digest = PinDigestV1::new(index);
            update(&mut state, &mut digest);
            let (forward_workers, backward_workers) = take_effective_update_workers_for_test_v1();
            digest.bytes(&state.state_sha256_v1().expect("updated state is valid"));
            PinV1 {
                route,
                update: index,
                forward_workers,
                backward_workers,
                digest: digest.finish(),
            }
        })
        .collect()
}

fn model_v1() -> NativePolicyValueNetV1 {
    NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1()).unwrap()
}

fn state_v1() -> NativePolicyValueTrainStateV1 {
    NativePolicyValueTrainStateV1::new_v1(model_v1()).unwrap()
}

/// Two V3 tensors from the real Map choice and the outputs a model records
/// for them (the `gae_v1` unit fixture).
struct V3FixtureV1 {
    tensors: [NativeFlatDecisionTensorV3; 2],
    logits: [Vec<u32>; 2],
    values: [u32; 2],
}

impl V3FixtureV1 {
    fn tensors_v1() -> [NativeFlatDecisionTensorV3; 2] {
        let first = real_map_training_tensor_v3();
        let mut second = first.clone();
        second.common.state[0] += 0.125;
        [first, second]
    }

    fn capture_v1(
        model: &NativePolicyValueNetV1,
        tensors: [NativeFlatDecisionTensorV3; 2],
        mode: ForwardActivationModeV1,
    ) -> Self {
        let outputs = tensors.each_ref().map(|tensor| {
            model
                .forward_feature_transfer_v3_mode(encoded_decision_view_v3(tensor), mode)
                .unwrap()
        });
        assert!(outputs.iter().all(|output| output.logits.len() >= 2));
        Self {
            logits: outputs
                .each_ref()
                .map(|output| output.logits.iter().map(|v| v.to_bits()).collect()),
            values: outputs.each_ref().map(|output| output.value.to_bits()),
            tensors,
        }
    }

    fn of_v1(model: &NativePolicyValueNetV1) -> Self {
        Self::capture_v1(model, Self::tensors_v1(), ForwardActivationModeV1::LibmTanh)
    }

    fn substep_v1(&self, tensor: usize, selected: usize) -> NativePolicySubstepV1<'_> {
        NativePolicySubstepV1 {
            forward: NativePolicyForwardInputV1::Encoded(Box::new(encoded_decision_view_v3(
                &self.tensors[tensor],
            ))),
            selected_action_index: selected,
            expected_raw_action_logit_bits: self.logits[tensor].as_slice(),
            expected_value_bits: self.values[tensor],
        }
    }

    /// Two groups, of two substeps and one.
    fn small_v1(&self) -> Vec<Vec<NativePolicySubstepV1<'_>>> {
        vec![
            vec![self.substep_v1(0, 0), self.substep_v1(1, 1)],
            vec![self.substep_v1(1, self.logits[1].len() - 1)],
        ]
    }

    /// 200 groups of one to three substeps (399 substeps), so four forward
    /// workers and four exact-backward workers engage.
    fn many_v1(&self) -> Vec<Vec<NativePolicySubstepV1<'_>>> {
        (0..200)
            .map(|group| {
                (0..1 + group % 3)
                    .map(|substep| {
                        let tensor = (group + substep) % 2;
                        let selected = (group * 7 + substep) % self.logits[tensor].len();
                        self.substep_v1(tensor, selected)
                    })
                    .collect()
            })
            .collect()
    }
}

fn physical_groups_v1<'a>(
    steps: &'a [Vec<NativePolicySubstepV1<'a>>],
) -> Vec<NativePolicyPhysicalDecisionV1<'a>> {
    steps
        .iter()
        .enumerate()
        .map(|(group, substeps)| NativePolicyPhysicalDecisionV1 {
            substeps,
            terminal_return: [1, -1, 0][group % 3],
            baseline_bits: 0,
        })
        .collect()
}

/// Per-group value targets and advantages, not reducible to the terminal
/// objective.
fn gae_targets_v1(group_count: usize) -> (Vec<f32>, Vec<f32>) {
    let targets = (0..group_count)
        .map(|group| 0.25 - 0.01 * group as f32)
        .collect();
    let advantages = (0..group_count)
        .map(|group| {
            let sign = if group % 2 == 0 { 0.5 } else { -0.375 };
            sign / (1.0 + group as f32)
        })
        .collect();
    (targets, advantages)
}

fn line_b_target_v1(fixture: &V3FixtureV1) -> crate::line_b_teacher_target_v1::LineBSoftmaxV1 {
    let logits: Vec<f64> = fixture.logits[1]
        .iter()
        .map(|bits| f64::from(f32::from_bits(*bits)))
        .collect();
    let returns: Vec<f64> = (0..logits.len()).map(|i| [1.0, 0.0, -1.0][i % 3]).collect();
    line_b_teacher_target_v1(&logits, &returns, 0.25).unwrap()
}

/// The terminal objective on the golden fixture's contract views.
fn terminal_contract_v1(
    route: &'static str,
    run: impl Fn(
        &mut NativePolicyValueTrainStateV1,
        &[NativePolicyPhysicalDecisionV1<'_>],
        f32,
        f32,
    ) -> NativePolicyTrainStepResultV1,
) -> Vec<PinV1> {
    let (forward, golden) = fixtures();
    let step = &golden.steps[2];
    updates_v1(route, state_v1(), |state, digest| {
        let expected = expected_outputs_for_step(state.model_v1(), &forward, step);
        let substeps = substeps_for_step(&forward, step, &expected);
        let groups = groups_for_step(step, &substeps);
        let result = run(
            state,
            &groups[..8],
            golden.value_coefficient,
            golden.optimizer.learning_rate,
        );
        digest.result(&result);
    })
}

/// The terminal objective with packed tapes and four recompute workers.
fn terminal_packed_v1() -> Vec<PinV1> {
    let (forward, golden) = fixtures();
    let step = &golden.steps[2];
    updates_v1(
        "terminal_contract_packed_recompute_w4",
        state_v1(),
        |state, digest| {
            let expected = expected_outputs_for_step(state.model_v1(), &forward, step);
            let builder =
                NativePolicyPackedForwardBuilderV1::from_model_v1(state.model_v1()).unwrap();
            let tapes = packed_tapes_for_step(&builder, &forward, step);
            let substeps = packed_substeps_for_step(&forward, step, &expected, &tapes);
            let base = groups_for_step(step, &substeps);
            let base = &base[..8];
            let substep_count = base.iter().map(|group| group.substeps.len()).sum::<usize>();
            let groups = (0..PACKED_RECOMPUTE_MIN_SUBSTEPS_V1.div_ceil(substep_count))
                .flat_map(|_| base.iter().copied())
                .collect::<Vec<_>>();
            let control = PackedRecomputeTestControlV1::new_v1(None, Vec::new());
            let result = {
                let _guard = install_packed_recompute_test_control_v1(Arc::clone(&control));
                state
                    .train_step_with_recompute_workers_v1(
                        &groups,
                        golden.value_coefficient,
                        golden.optimizer.learning_rate,
                        PACKED_RECOMPUTE_MAX_WORKERS_V1,
                    )
                    .unwrap()
            };
            assert_eq!(control.spawn_counts_v1(), (4, 4, 0));
            digest.result(&result);
        },
    )
}

/// The RT1 CPU GAE route (`train_step_gae_feature_transfer_profiled_v1`) on
/// the 399-substep fixture.
fn gae_profiled_v1(
    route: &'static str,
    mut state: NativePolicyValueTrainStateV1,
    mode: ForwardActivationModeV1,
    tensors: fn() -> [NativeFlatDecisionTensorV3; 2],
    backward_worker_limit: Option<usize>,
    forward_workers: usize,
) -> Vec<PinV1> {
    if !matches!(mode, ForwardActivationModeV1::LibmTanh) {
        state.set_training_math_v1(TrainingMathV1::PortableV1);
    }
    updates_v1(route, state, |state, digest| {
        let fixture = V3FixtureV1::capture_v1(state.model_v1(), tensors(), mode);
        let steps = fixture.many_v1();
        let groups = physical_groups_v1(&steps);
        let (targets, advantages) = gae_targets_v1(groups.len());
        let result = state
            .train_step_gae_feature_transfer_profiled_v1(
                FreshLineageGenerationV1::V3,
                &groups,
                &targets,
                &advantages,
                VC,
                LR,
                backward_worker_limit,
                forward_workers,
                &mut NativeTrainingPhaseRecorderV1::disabled_v1(),
            )
            .unwrap();
        digest.result(&result);
    })
}

/// An ordinary update on the two-group V3 fixture.
fn small_v3_v1(
    route: &'static str,
    run: impl Fn(
        &mut NativePolicyValueTrainStateV1,
        &V3FixtureV1,
        &[NativePolicyPhysicalDecisionV1<'_>],
        &mut PinDigestV1,
    ),
) -> Vec<PinV1> {
    updates_v1(route, state_v1(), |state, digest| {
        let fixture = V3FixtureV1::of_v1(state.model_v1());
        let steps = fixture.small_v1();
        let groups = physical_groups_v1(&steps);
        run(state, &fixture, &groups, digest);
    })
}

/// The real Map choice as a V4 tensor (the `retention_v1` unit fixture).
fn v4_tensor_v1() -> NativeFlatDecisionTensorV4 {
    use crate::flat_policy_v2::{FlatScoringDecisionViewV2, FlatScoringOwnedBuffersV2};
    use crate::flat_policy_v4::{FlatDecisionEncoderV4, FlatScoringDecisionViewV4};
    use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
    let session = FastActorSessionV1::from_v3_fixture_state(
        crate::policy_observation_v6::tests::map_choice_state().0,
    );
    let FastActorResponseV1::Decision(decision) = session.current_response() else {
        panic!("missing Map choice")
    };
    let (
        mut objects,
        mut relations,
        mut subtypes,
        mut uses,
        mut goads,
        mut dungeons,
        mut effects,
        mut paths,
        mut actions,
        mut refs,
    ) = (
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    let encoded = session
        .encode_current_flat_scoring_decision_owned_v4(
            decision,
            &mut FlatDecisionEncoderV4::default(),
            &mut FlatScoringOwnedBuffersV2 {
                objects: &mut objects,
                relations: &mut relations,
                object_subtypes: &mut subtypes,
                ability_uses: &mut uses,
                goads: &mut goads,
                completed_dungeons: &mut dungeons,
                effect_subtype_changes: &mut effects,
                context_path_elements: &mut paths,
                actions: &mut actions,
                action_refs: &mut refs,
            },
        )
        .unwrap();
    let common = FlatScoringDecisionViewV2::new(
        &encoded.globals,
        &objects,
        &relations,
        &subtypes,
        &uses,
        &goads,
        &dungeons,
        &effects,
        &paths,
        &actions,
        &refs,
    );
    let mut tensor = NativeFlatDecisionTensorV4::default();
    NativeFlatTensorizerV4::default()
        .fill(
            FlatScoringDecisionViewV4::new(common, &encoded.extensions),
            &mut tensor,
        )
        .unwrap();
    tensor
}

/// The V4 tensor, the outputs a model records for it and frozen parent
/// logits for retention rows.
struct V4FixtureV1 {
    tensor: NativeFlatDecisionTensorV4,
    logits: Vec<u32>,
    value: u32,
    parent: Vec<f32>,
}

impl V4FixtureV1 {
    fn of_v1(model: &NativePolicyValueNetV1) -> Self {
        let tensor = v4_tensor_v1();
        let output = model
            .forward_feature_transfer_v4(encoded_decision_view_v4(&tensor))
            .unwrap();
        assert!(output.logits.len() >= 2);
        Self {
            parent: output
                .logits
                .iter()
                .enumerate()
                .map(|(index, logit)| logit + 0.7 * index as f32)
                .collect(),
            logits: output.logits.iter().map(|v| v.to_bits()).collect(),
            value: output.value.to_bits(),
            tensor,
        }
    }

    fn substep_v1(&self, selected: usize) -> NativePolicySubstepV1<'_> {
        NativePolicySubstepV1 {
            forward: NativePolicyForwardInputV1::Encoded(Box::new(encoded_decision_view_v4(
                &self.tensor,
            ))),
            selected_action_index: selected,
            expected_raw_action_logit_bits: &self.logits,
            expected_value_bits: self.value,
        }
    }

    /// Two teaching groups, of one substep each.
    fn teaching_v1(&self) -> [Vec<NativePolicySubstepV1<'_>>; 2] {
        [
            vec![self.substep_v1(0)],
            vec![self.substep_v1(self.logits.len() - 1)],
        ]
    }

    fn retention_v1(&self) -> RetentionRowV1<'_> {
        RetentionRowV1 {
            encoded: encoded_decision_view_v4(&self.tensor),
            expected_current_logits: &self.logits,
            expected_current_value: self.value,
            parent_logits: &self.parent,
        }
    }
}

fn teaching_groups_v1<'a>(
    steps: &'a [Vec<NativePolicySubstepV1<'a>>; 2],
) -> [NativePolicyPhysicalDecisionV1<'a>; 2] {
    steps
        .each_ref()
        .map(|substeps| NativePolicyPhysicalDecisionV1 {
            substeps,
            terminal_return: 1,
            baseline_bits: 0,
        })
}

/// A runner-fixed state under the no-state-digest contract: the digest
/// columns of the state encoder weight are +0.0 and its bias holds no -0.0.
fn no_state_digest_state_v1() -> NativePolicyValueTrainStateV1 {
    let mut snapshot = state_v1().snapshot_v1().unwrap();
    for row in snapshot.parameters[STATE_FIRST_WEIGHT]
        .values
        .chunks_exact_mut(STATE_ENCODER_INPUT)
    {
        row[DIGEST_COLUMNS_V1].fill(0.0);
    }
    for value in &mut snapshot.parameters[STATE_FIRST_BIAS].values {
        if value.to_bits() == 0x8000_0000 {
            *value = 0.0;
        }
    }
    let mut state = NativePolicyValueTrainStateV1::from_snapshot_v1(model_v1(), &snapshot).unwrap();
    state.install_no_state_digest_v1(DIGEST_COLUMNS_V1).unwrap();
    state
}

fn no_state_digest_tensors_v1() -> [NativeFlatDecisionTensorV3; 2] {
    let mut tensors = V3FixtureV1::tensors_v1();
    for tensor in &mut tensors {
        tensor.common.state[DIGEST_COLUMNS_V1].fill(0.0);
    }
    tensors
}

/// The wide train state's digests before the refactor (design section 5.5):
/// a new runner-fixed wide state, and an owned snapshot of it with a later
/// Adam step and nonzero moments. Platform-free: no libm is involved.
fn wide_state_hash_pins_v1() -> Vec<PinV1> {
    let model = NativePolicyValueNetWideV1::runner_fixed_wide_v1(
        NativePolicyValueModelConfigV1::contract_wide_v1(),
    )
    .unwrap();
    let state = NativePolicyValueTrainStateWideV1::new_wide_v1(model).unwrap();
    let mut snapshot = state.snapshot_v1().unwrap();
    snapshot.adam_step = 7;
    for (index, (first, second)) in snapshot
        .first_moments
        .iter_mut()
        .zip(&mut snapshot.second_moments)
        .enumerate()
    {
        if index == CARD_EMBEDDING || index == SCORER_SECOND_BIAS {
            continue;
        }
        first.values[0] = 0.25 / (1.0 + index as f32);
        second.values[0] = 0.125 / (1.0 + index as f32);
    }
    [
        state.state_sha256_v1().unwrap(),
        wide_owned_train_snapshot_state_sha256_v1(&snapshot).unwrap(),
    ]
    .into_iter()
    .enumerate()
    .map(|(update, digest)| PinV1 {
        route: "wide_state_sha256",
        update,
        forward_workers: 0,
        backward_workers: 0,
        digest: hex_sha256(digest),
    })
    .collect()
}

fn all_pins_v1() -> Vec<PinV1> {
    let mut pins = wide_state_hash_pins_v1();
    pins.extend(terminal_contract_v1(
        "terminal_contract_sequential",
        |state, groups, vc, lr| state.train_step_v1(groups, vc, lr).unwrap(),
    ));
    pins.extend(terminal_packed_v1());
    for (route, workers) in [
        ("terminal_contract_fixed_partition_w1", 1),
        ("terminal_contract_fixed_partition_w4", 4),
    ] {
        pins.extend(terminal_contract_v1(route, |state, groups, vc, lr| {
            state
                .train_step_with_fixed_partition_parallel_backward_v1(groups, vc, lr, 1, workers)
                .unwrap()
        }));
    }
    pins.extend(small_v3_v1(
        "terminal_v3_sequential",
        |state, _, groups, digest| {
            digest.result(
                &state
                    .train_step_feature_transfer_v3(groups, VC, LR)
                    .unwrap(),
            );
        },
    ));
    for (route, workers) in [
        ("gae_v3_preparation_w1", 1),
        ("gae_v3_preparation_w2", 2),
        ("gae_v3_preparation_w4", 4),
    ] {
        pins.extend(gae_profiled_v1(
            route,
            state_v1(),
            ForwardActivationModeV1::LibmTanh,
            V3FixtureV1::tensors_v1,
            None,
            workers,
        ));
    }
    for (route, workers) in [
        ("gae_v3_fixed_partition_w1", 1),
        ("gae_v3_fixed_partition_w4", 4),
    ] {
        pins.extend(gae_profiled_v1(
            route,
            state_v1(),
            ForwardActivationModeV1::LibmTanh,
            V3FixtureV1::tensors_v1,
            Some(workers),
            2,
        ));
    }
    pins.extend(small_v3_v1("weighted_v3", |state, _, groups, digest| {
        digest.result(
            &state
                .train_step_weighted_feature_transfer_v3(groups, &[0.375, 0.625], VC, LR)
                .unwrap(),
        );
    }));
    pins.extend(small_v3_v1(
        "gae_v3_line_b",
        |state, fixture, groups, digest| {
            let input = LineBAuxiliaryInputV1 {
                scheduled_count: None,
                direction: LineBDivergenceV1::Forward,
                coefficient: 0.7,
                roots: vec![
                    LineBAuxiliaryRootV1 {
                        group_index: 0,
                        substep_index: 1,
                        target: Some(line_b_target_v1(fixture)),
                    },
                    LineBAuxiliaryRootV1 {
                        group_index: 1,
                        substep_index: 0,
                        target: None,
                    },
                ],
            };
            let (targets, advantages) = gae_targets_v1(groups.len());
            let config = state.model_v1().feature_transfer_config_v3();
            let (result, auxiliary) = state
                .train_step_gae_line_b_v1(
                    groups,
                    &targets,
                    &advantages,
                    VC,
                    LR,
                    None,
                    config,
                    &input,
                )
                .unwrap();
            digest.result(&result);
            digest.auxiliary(&auxiliary);
        },
    ));
    pins.extend(updates_v1("imitation_v4", state_v1(), |state, digest| {
        let fixture = V4FixtureV1::of_v1(state.model_v1());
        let steps = fixture.teaching_v1();
        let groups = teaching_groups_v1(&steps);
        digest.result(
            &state
                .train_step_terminal_winner_imitation_v4(&groups, LR, 4)
                .unwrap(),
        );
    }));
    pins.extend(small_v3_v1(
        "gae_v3_external_auxiliary",
        |state, fixture, groups, digest| {
            let target = line_b_target_v1(fixture);
            let external = ExternalAuxiliaryInputV1 {
                direction: LineBDivergenceV1::Forward,
                coefficient: 0.7,
                denominator: 2,
                cap: Some(1.0e-3),
                step_telemetry: true,
                rows: (0..2)
                    .map(|_| ExternalAuxiliaryRowV1 {
                        encoded: encoded_decision_view_v3(&fixture.tensors[1]),
                        target: &target,
                    })
                    .collect(),
            };
            let (targets, advantages) = gae_targets_v1(groups.len());
            let (result, auxiliary) = state
                .train_step_gae_external_auxiliary_v1(
                    FreshLineageGenerationV1::V3,
                    groups,
                    &targets,
                    &advantages,
                    VC,
                    LR,
                    None,
                    &external,
                )
                .unwrap();
            digest.result(&result);
            digest.auxiliary(&auxiliary);
        },
    ));
    pins.extend(small_v3_v1(
        "gae_v3_recovery",
        |state, _, groups, digest| {
            let (targets, advantages) = gae_targets_v1(groups.len());
            // The first update also records the diagnostics.
            let update = state.adam_step_v1() + 1;
            let (result, report) = state
                .train_step_gae_recovery_v1(
                    FreshLineageGenerationV1::V3,
                    groups,
                    &targets,
                    &advantages,
                    VC,
                    LR,
                    0.01,
                    update,
                    true,
                )
                .unwrap();
            digest.result(&result);
            digest.json(&report);
        },
    ));
    pins.extend(updates_v1("retention_v4", state_v1(), |state, digest| {
        let fixture = V4FixtureV1::of_v1(state.model_v1());
        let steps = fixture.teaching_v1();
        let groups = teaching_groups_v1(&steps);
        let two = [fixture.retention_v1(), fixture.retention_v1()];
        let one = [fixture.retention_v1()];
        let retained = [
            RetentionGroupV1 { rows: &two },
            RetentionGroupV1 { rows: &one },
        ];
        digest.retained(
            &state
                .train_step_retained_imitation_v4(&groups, &retained, 0.75, LR, 4)
                .unwrap(),
        );
    }));
    pins.extend(small_v3_v1(
        "gae_v3_e_supported",
        |state, fixture, groups, digest| {
            let input = ESupportedPolicyInputV1 {
                scheduled: 2,
                treatment: true,
                rows: vec![ESupportedPolicyRowV1 {
                    group_index: 0,
                    substep_index: 1,
                    legal_width: fixture.logits[1].len(),
                    target_index: 0,
                }],
            };
            let (targets, advantages) = gae_targets_v1(groups.len());
            let (result, report) = state
                .train_step_gae_e_supported_v1(
                    FreshLineageGenerationV1::V3,
                    groups,
                    &targets,
                    &advantages,
                    VC,
                    LR,
                    None,
                    &input,
                )
                .unwrap();
            digest.result(&result);
            digest.optional_json(report.as_ref());
        },
    ));
    pins.extend(small_v3_v1(
        "gae_v3_head_only_mask",
        |state, _, groups, digest| {
            let (targets, advantages) = gae_targets_v1(groups.len());
            let before = state.snapshot_v1().unwrap();
            let mask = HeadOnlyMaskV1::for_snapshot_v1(&before).unwrap();
            let result = state
                .train_step_gae_feature_transfer_v3(groups, &targets, &advantages, VC, LR)
                .unwrap();
            let mut after = state.snapshot_v1().unwrap();
            mask.restore_frozen_v1(&before, &mut after).unwrap();
            state.replace_snapshot_v1(&after).unwrap();
            digest.result(&result);
            digest.bytes(&mask.frozen_sha256_v1(&after));
        },
    ));
    pins.extend(gae_profiled_v1(
        "gae_v3_portable_math_w2",
        state_v1(),
        ForwardActivationModeV1::KernelDeterministicTanh,
        V3FixtureV1::tensors_v1,
        None,
        2,
    ));
    pins.extend(gae_profiled_v1(
        "gae_v3_no_state_digest_w2",
        no_state_digest_state_v1(),
        ForwardActivationModeV1::LibmTanh,
        no_state_digest_tensors_v1,
        None,
        2,
    ));
    pins
}

#[test]
fn narrow_dims_refactor_bit_pins_v1() {
    let pins = all_pins_v1();
    let lines = pins.iter().map(PinV1::line_v1).collect::<Vec<_>>();
    for line in &lines {
        println!("{line}");
    }
    let find = |route: &str, update: usize| {
        pins.iter()
            .find(|pin| pin.route == route && pin.update == update)
            .unwrap_or_else(|| panic!("missing pin {route} {update}"))
    };
    for update in 0..UPDATES_V1 {
        // Preparation workers 1, 2 and 4 engage that many forward and
        // exact-backward workers and give one digest.
        for (route, workers) in [
            ("gae_v3_preparation_w1", 1),
            ("gae_v3_preparation_w2", 2),
            ("gae_v3_preparation_w4", 4),
        ] {
            let pin = find(route, update);
            assert_eq!(
                (pin.forward_workers, pin.backward_workers),
                (workers, workers),
                "{route} {update}"
            );
            assert_eq!(
                pin.digest,
                find("gae_v3_preparation_w1", update).digest,
                "{route} {update}"
            );
        }
        // Fixed partitions consult no exact backward, and their worker
        // limit changes no bit.
        for route in ["gae_v3_fixed_partition_w1", "gae_v3_fixed_partition_w4"] {
            let pin = find(route, update);
            assert_eq!((pin.forward_workers, pin.backward_workers), (2, 0));
        }
        assert_eq!(
            find("gae_v3_fixed_partition_w4", update).digest,
            find("gae_v3_fixed_partition_w1", update).digest
        );
        assert_eq!(
            find("terminal_contract_fixed_partition_w4", update).digest,
            find("terminal_contract_fixed_partition_w1", update).digest
        );
        assert_eq!(find("gae_v3_portable_math_w2", update).forward_workers, 2);
        assert_eq!(find("gae_v3_no_state_digest_w2", update).forward_workers, 2);
    }
    let target = EXPECTED_TARGET_V1;
    let expected = EXPECTED_PINS_V1
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    assert!(
        !expected.is_empty(),
        "capture mode: no narrow pins recorded for {target}; the {} lines above are this \
         target's pins",
        lines.len()
    );
    assert_eq!(expected.len(), lines.len(), "pin count on {target}");
    for (expected, actual) in expected.iter().zip(&lines) {
        assert_eq!(expected, actual, "narrow pin drifted on {target}");
    }
}
