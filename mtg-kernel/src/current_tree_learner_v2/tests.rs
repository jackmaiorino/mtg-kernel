use super::*;
use serde_json::json;

struct OwnedDecision {
    state: NumericTree,
    actions: Vec<NumericTree>,
    flags: Vec<[bool; FLAG_COUNT]>,
    selected: usize,
}
impl OwnedDecision {
    fn view(&self) -> DecisionView<'_> {
        DecisionView {
            state: &self.state,
            actions: &self.actions,
            flags: &self.flags,
        }
    }
}

struct OwnedTrajectory {
    pair_index: u64,
    leg: u8,
    seat: u8,
    terminal: TerminalV2,
    decisions: Vec<OwnedDecision>,
}

fn tree(value: serde_json::Value) -> NumericTree {
    NumericTree::probe_fixture_v1(&value).unwrap()
}

/// A decision with two to four legal actions and varied payloads, flags and
/// tree shapes.
fn decision(seed: u64) -> OwnedDecision {
    let width = 2 + (seed % 3) as usize;
    let phase = if seed % 2 == 0 { "main" } else { "combat" };
    let state = tree(json!({
        "life": [20 + seed % 7, 17 - seed % 5],
        "hand": (seed * 977) % 60_000,
        "zones": {"library": 33 - seed % 9, "battlefield": [(seed * 31) % 65_535, 40_000, 123]},
        "phase": phase,
    }));
    let actions = (0..width as u64)
        .map(|i| {
            tree(json!({
                "kind": "cast",
                "card": (seed * 7_919 + i * 104_729) % 65_535,
                "targets": [i, (seed + i) * 5_003 % 65_535],
            }))
        })
        .collect();
    let flags = (0..width as u64)
        .map(|i| std::array::from_fn(|f| (seed + 3 * i + f as u64) % 3 == 0))
        .collect();
    OwnedDecision {
        state,
        actions,
        flags,
        selected: (seed % width as u64) as usize,
    }
}

fn decisions(base: u64, count: u64) -> Vec<OwnedDecision> {
    (0..count).map(|i| decision(base + i)).collect()
}

fn trajectory(
    pair_index: u64,
    leg: u8,
    seat: u8,
    terminal: TerminalV2,
    decisions: Vec<OwnedDecision>,
) -> OwnedTrajectory {
    OwnedTrajectory {
        pair_index,
        leg,
        seat,
        terminal,
        decisions,
    }
}

fn natural(learner_return: i8) -> TerminalV2 {
    TerminalV2::Natural { learner_return }
}

fn batch() -> Vec<OwnedTrajectory> {
    vec![
        trajectory(0, 0, 0, natural(1), decisions(1, 4)),
        trajectory(0, 0, 1, natural(-1), decisions(11, 3)),
        trajectory(0, 1, 0, natural(0), decisions(21, 5)),
        trajectory(0, 1, 1, natural(0), decisions(31, 2)),
        trajectory(1, 0, 0, natural(-1), decisions(41, 3)),
        trajectory(1, 1, 1, natural(1), decisions(51, 4)),
    ]
}

fn views(owned: &[OwnedTrajectory]) -> Vec<TrajectoryView<'_>> {
    owned
        .iter()
        .map(|t| TrajectoryView {
            pair_index: t.pair_index,
            leg: t.leg,
            seat: t.seat,
            terminal: t.terminal,
            decisions: t
                .decisions
                .iter()
                .map(|d| SelectedView {
                    view: d.view(),
                    selected: d.selected,
                })
                .collect(),
        })
        .collect()
}

fn random_model<const H: usize>(seed: u64) -> TreeModelV2<H> {
    let mut rng = SplitMix64::seed(seed);
    TreeModelV2::from_parameters(
        (0..TreeModelV2::<H>::PARAMETER_COUNT)
            .map(|_| ((rng.next_u64() >> 11) as f64 * TWO_POW_MINUS_53 - 0.5) * 0.8)
            .collect(),
    )
    .unwrap()
}

fn blocks(hidden: usize) -> Vec<(&'static str, usize, usize)> {
    let layout = Layout { hidden };
    vec![
        ("input", 0, layout.recurrent()),
        ("recurrent", layout.recurrent(), layout.bias()),
        ("encoder bias", layout.bias(), layout.head()),
        ("head state", layout.head(), layout.head() + hidden),
        ("head action", layout.head() + hidden, layout.head() + 2 * hidden),
        ("head bilinear", layout.head() + 2 * hidden, layout.flags()),
        ("flags", layout.flags(), layout.policy_bias()),
        ("policy bias", layout.policy_bias(), layout.value_weights()),
        ("value weights", layout.value_weights(), layout.value_bias()),
        ("value bias", layout.value_bias(), parameter_count_v2(hidden)),
    ]
}

#[test]
fn contracts_parameter_counts_and_identities() {
    assert_eq!(PARAMETER_COUNT_V2, 1_832);
    assert_eq!(PARAMETER_COUNT_V2_H16, 664);
    assert_eq!(ModelV2::PARAMETER_COUNT, 1_832);
    assert_eq!(ModelV2H16::PARAMETER_COUNT, 664);
    assert_eq!(Layout { hidden: 32 }.value_bias() + 1, 1_832);
    assert_eq!(Layout { hidden: 16 }.value_bias() + 1, 664);
    assert!(CONTRACT_V2.starts_with(
        "current-tree-learner/v2/f64/input20/hidden32/flags6/value1/gae-adam"
    ));
    assert!(CONTRACT_V2.ends_with("/libm-free-v1"));
    assert_eq!(
        CONTRACT_V2_H16,
        CONTRACT_V2.replace("hidden32", "hidden16")
    );
    assert_eq!(ModelV2::CONTRACT, CONTRACT_V2);
    assert_eq!(ModelV2H16::CONTRACT, CONTRACT_V2_H16);
    let wide = feature_identity_v2();
    let narrow = feature_identity_v2_h16();
    assert_ne!(wide, narrow);
    for identity in [&wide, &narrow] {
        assert_eq!(identity.len(), 64);
        assert!(identity.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(*identity, feature_identity_v1());
    }
    assert_eq!(wide, feature_identity_v2());
}

#[test]
fn deterministic_initialization_is_fixed_finite_with_a_zero_value_head() {
    fn check<const H: usize>() {
        let model = TreeModelV2::<H>::deterministic_initialization();
        assert_eq!(model, TreeModelV2::<H>::deterministic_initialization());
        let layout = Layout { hidden: H };
        let parameters = model.parameters();
        assert_eq!(parameters.len(), parameter_count_v2(H));
        let input_scale = 1. / (INPUT as f64).sqrt();
        let hidden_scale = 1. / (H as f64).sqrt();
        for (index, value) in parameters.iter().enumerate() {
            let bound = if index < layout.recurrent() {
                input_scale
            } else if index < layout.bias() || (layout.head()..layout.flags()).contains(&index) {
                hidden_scale
            } else {
                0.
            };
            assert!(value.is_finite() && value.abs() <= bound * (1. + 1e-12), "{index}");
            if bound == 0. {
                assert_eq!(value.to_bits(), 0, "{index}");
            }
        }
        assert!(parameters[..layout.bias()].iter().all(|v| *v != 0.));
        let learner = TreeLearnerV2::<H>::new(OptimizerConfigV2::campaign_v1(200)).unwrap();
        assert_eq!(learner.step(), 0);
        let snapshot = learner.snapshot();
        assert!(snapshot
            .first_moment_bits
            .iter()
            .chain(&snapshot.second_moment_bits)
            .all(|bits| *bits == 0));
        let fixture = decision(7);
        let tape = learner.model().forward_view(fixture.view()).unwrap();
        assert_eq!(tape.value.to_bits(), 0);
        assert!(tape.logits.iter().all(|v| v.is_finite()));
    }
    check::<32>();
    check::<16>();
    assert_ne!(
        ModelV2::deterministic_initialization().parameters()[..16],
        ModelV2H16::deterministic_initialization().parameters()[..16]
    );
}

/// Fixed advantages and value targets for five decisions.
fn examples(owned: &[OwnedDecision]) -> Vec<ExampleV2<'_>> {
    let advantages = [0.7, -1.3, 0.4, 2.1, -0.2];
    let targets = [0.3, -0.6, 0.9, 0., -0.95];
    owned
        .iter()
        .zip(advantages.into_iter().zip(targets))
        .map(|(d, (advantage, value_target))| ExampleV2 {
            view: d.view(),
            selected: d.selected,
            advantage,
            value_target,
        })
        .collect()
}

fn finite_difference_check<const H: usize>(value_coefficient: f64, entropy_coefficient: f64) {
    let owned = decisions(3, 5);
    let model = random_model::<H>(0x5eed + H as u64);
    let batch = examples(&owned);
    let (_, gradient) = model
        .loss_and_gradient(&batch, value_coefficient, entropy_coefficient)
        .unwrap();
    let loss_at = |parameters: Vec<f64>| {
        TreeModelV2::<H>::from_parameters(parameters)
            .unwrap()
            .loss_and_gradient(&batch, value_coefficient, entropy_coefficient)
            .unwrap()
            .0
            .total
    };
    let step = 1e-6;
    for (name, start, end) in blocks(H) {
        let width = end - start;
        let mut indices = vec![start, end - 1, start + width / 2, start + width / 3];
        indices.extend((start..end).step_by(width.div_ceil(7)));
        indices.sort_unstable();
        indices.dedup();
        for index in indices {
            let mut plus = model.parameters().to_vec();
            plus[index] += step;
            let mut minus = model.parameters().to_vec();
            minus[index] -= step;
            let numeric = (loss_at(plus) - loss_at(minus)) / (2. * step);
            let analytic = gradient[index];
            let tolerance = 1e-7 + 1e-6 * analytic.abs().max(numeric.abs());
            assert!(
                (numeric - analytic).abs() <= tolerance,
                "H {H} c_v {value_coefficient} c_ent {entropy_coefficient} {name} {index}: \
                 analytic {analytic} numeric {numeric}"
            );
        }
    }
}

#[test]
fn finite_differences_match_every_parameter_block() {
    for (value_coefficient, entropy_coefficient) in [(0., 0.), (1., 0.), (0., 0.5), (0.5, 0.01)] {
        finite_difference_check::<16>(value_coefficient, entropy_coefficient);
        finite_difference_check::<32>(value_coefficient, entropy_coefficient);
    }
}

#[test]
fn every_block_but_the_policy_bias_receives_gradient() {
    let owned = decisions(3, 5);
    let model = random_model::<32>(17);
    let (_, gradient) = model
        .loss_and_gradient(&examples(&owned), 0.5, 0.01)
        .unwrap();
    for (name, start, end) in blocks(32) {
        let largest = gradient[start..end]
            .iter()
            .fold(0_f64, |m, v| m.max(v.abs()));
        if name == "policy bias" {
            // A shift of every logit leaves the softmax unchanged.
            assert!(largest < 1e-12, "{largest}");
        } else {
            assert!(largest > 1e-6, "{name} {largest}");
        }
    }
}

#[test]
fn entropy_term_alone_matches_its_closed_form() {
    let owned = decisions(5, 1);
    let model = random_model::<16>(23);
    let example = [ExampleV2 {
        view: owned[0].view(),
        selected: owned[0].selected,
        advantage: 0.,
        value_target: 0.,
    }];
    let (loss, _) = model.loss_and_gradient(&example, 0., 1.).unwrap();
    let logits = model.forward_view(owned[0].view()).unwrap().logits;
    let max = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let sum = logits.iter().map(|z| (z - max).exp()).sum::<f64>();
    let entropy = -logits
        .iter()
        .map(|z| {
            let log_p = z - max - sum.ln();
            log_p.exp() * log_p
        })
        .sum::<f64>();
    assert!((loss.entropy - entropy).abs() < 1e-14);
    assert!((loss.total + entropy).abs() < 1e-14);
}

#[test]
fn generalized_advantage_estimation_matches_hand_computed_fixtures() {
    // Dyadic fixture, exact: gamma = lambda = 1/2, values 1/2, -1/4, 1/8, R = 1.
    // delta = [-5/8, 5/16, 7/8]; A_2 = 7/8, A_1 = 5/16 + 7/32 = 17/32,
    // A_0 = -5/8 + 17/128 = -63/128.
    assert_eq!(
        gae_v2(&[0.5, -0.25, 0.125], 1., 0.5, 0.5),
        vec![-63. / 128., 17. / 32., 7. / 8.]
    );
    assert_eq!(gae_v2(&[0.25], -1., 0.997, 0.95), vec![-1.25]);
    assert!(gae_v2(&[], 1., 0.997, 0.95).is_empty());
    // Design parameters against the sum formula.
    let values = [0.1, -0.3, 0.25, 0.6, -0.45];
    let (gamma, lambda, terminal) = (0.997, 0.95, -1.);
    let deltas = (0..values.len())
        .map(|t| match values.get(t + 1) {
            Some(next) => gamma * next - values[t],
            None => terminal - values[t],
        })
        .collect::<Vec<f64>>();
    let advantages = gae_v2(&values, terminal, gamma, lambda);
    for t in 0..values.len() {
        let expected = (t..values.len())
            .map(|k| (gamma * lambda).powi((k - t) as i32) * deltas[k])
            .sum::<f64>();
        assert!((advantages[t] - expected).abs() < 1e-14, "{t}");
    }
}

#[test]
fn scale_only_standardization_does_not_center() {
    let (standardized, sigma) = scale_only_standardize_v2(&[2., 4.]).unwrap();
    assert_eq!(sigma, 1.);
    assert_eq!(standardized, vec![2. / (1. + 1e-8), 4. / (1. + 1e-8)]);
    let (standardized, sigma) = scale_only_standardize_v2(&[1., -1., 3., -3.]).unwrap();
    assert!((sigma - 5_f64.sqrt()).abs() < 1e-15);
    for (value, raw) in standardized.iter().zip([1., -1., 3., -3.]) {
        assert!((value - raw / (5_f64.sqrt() + 1e-8)).abs() < 1e-15);
    }
    let (standardized, sigma) = scale_only_standardize_v2(&[0.5]).unwrap();
    assert_eq!(sigma, 0.);
    assert_eq!(standardized, vec![0.5 / 1e-8]);
    assert_eq!(scale_only_standardize_v2(&[]), Err(Error::NoDecision));
    assert_eq!(
        scale_only_standardize_v2(&[1., f64::NAN]),
        Err(Error::NonFiniteLogit)
    );
}

#[test]
fn global_norm_clip_scales_only_above_the_limit() {
    let mut gradient = vec![3., -4.];
    assert!((clip_global_norm_v2(&mut gradient, 1.) - 5.).abs() < 1e-15);
    assert!((gradient[0] - 0.6).abs() < 1e-15 && (gradient[1] + 0.8).abs() < 1e-15);
    let mut gradient = vec![0.3, 0.4];
    assert!((clip_global_norm_v2(&mut gradient, 1.) - 0.5).abs() < 1e-15);
    assert_eq!(gradient, vec![0.3, 0.4]);
    let mut gradient = vec![30., 40., 0.];
    assert!((clip_global_norm_v2(&mut gradient, 2.) - 50.).abs() < 1e-13);
    let norm = clip_global_norm_v2(&mut gradient, 2.);
    assert!((norm - 2.).abs() < 1e-15 && norm <= 2. * (1. + 1e-15));
}

#[test]
fn cosine_schedule_end_points_and_shape() {
    let config = OptimizerConfigV2::campaign_v1(200);
    assert_eq!(config.learning_rate(1), Ok(3e-4));
    assert_eq!(config.learning_rate(200), Ok(1e-4));
    assert_eq!(config.learning_rate(0), Err(Error::Range));
    assert_eq!(config.learning_rate(201), Err(Error::Range));
    let mut previous = f64::INFINITY;
    for update in 1..=200 {
        let rate = config.learning_rate(update).unwrap();
        assert!(rate <= previous && (1e-4..=3e-4).contains(&rate));
        let fraction = (update - 1) as f64 / 199.;
        let expected = 1e-4 + 2e-4 * 0.5 * (1. + (std::f64::consts::PI * fraction).cos());
        assert!((rate - expected).abs() < 1e-18, "{update}");
        previous = rate;
    }
    let short = OptimizerConfigV2::campaign_v1(3);
    assert!((short.learning_rate(2).unwrap() - 2e-4).abs() < 1e-18);
    assert_eq!(OptimizerConfigV2::campaign_v1(1).learning_rate(1), Ok(3e-4));
    assert_eq!(
        OptimizerConfigV2::campaign_v1(0).learning_rate(1),
        Err(Error::ModelContract)
    );
}

#[test]
fn libm_free_functions_track_the_platform_functions() {
    let close = |ours: f64, reference: f64, what: &str| {
        let tolerance = 4e-15 * reference.abs().max(1e-300);
        assert!(
            (ours - reference).abs() <= tolerance,
            "{what}: {ours:e} vs {reference:e}"
        );
    };
    let mut x = -25.;
    while x <= 25. {
        close(tanh_v2(x), x.tanh(), "tanh");
        x += 0.0371;
    }
    for x in [1e-300, 1e-12, 1e-6, 0.01, 0.3, 0.5493, 1., 9., 17.99, 18., 30.] {
        close(tanh_v2(x), x.tanh(), "tanh");
        close(tanh_v2(-x), -x.tanh(), "tanh");
    }
    assert_eq!(tanh_v2(0.).to_bits(), 0);
    assert_eq!(tanh_v2(-0.).to_bits(), (-0_f64).to_bits());
    let mut x = -700.;
    while x <= 0. {
        close(exp_nonpositive_v2(x), x.exp(), "exp");
        x += 0.913;
    }
    assert_eq!(exp_nonpositive_v2(0.), 1.);
    let mut x = 1.;
    while x < 1e6 {
        close(ln_v2(x), x.ln(), "ln");
        close(ln_v2(1. / x), (1. / x).ln(), "ln");
        x *= 1.37;
    }
    assert_eq!(ln_v2(1.), 0.);
    for x in [1e-300, 2.5e-310, 5e-324, 1e-10, 0.5, 2., 3., 10., 1e10, 1e300] {
        close(sqrt_v2(x), x.sqrt(), "sqrt");
    }
    assert_eq!(sqrt_v2(4.), 2.);
    assert_eq!(sqrt_v2(0.), 0.);
    assert!(sqrt_v2(-1.).is_nan() && sqrt_v2(f64::INFINITY).is_nan());
    for i in 0..=64 {
        let fraction = f64::from(i) / 64.;
        let reference = (std::f64::consts::PI * fraction).cos();
        assert!((cos_pi_v2(fraction) - reference).abs() < 1e-15, "{i}");
    }
    assert_eq!(powi_v2(0.999, 0), 1.);
    assert_eq!(powi_v2(0.5, 10), 1. / 1024.);
    assert!((powi_v2(0.999, 200) - 0.999_f64.powi(200)).abs() < 1e-15);
    let probabilities = probabilities_v2(&[1., 2., -800., 0.5]).unwrap();
    let total = probabilities.iter().sum::<f64>();
    assert!((total - 1.).abs() < 1e-15);
    assert!(probabilities[2] < 1e-300);
    assert_eq!(probabilities_v2(&[]), Err(Error::ActionRange));
    assert_eq!(probabilities_v2(&[f64::NAN]), Err(Error::NonFiniteLogit));
}

#[test]
fn snapshot_round_trip_is_bit_exact() {
    let owned = batch();
    let mut learner = LearnerV2::new(OptimizerConfigV2::campaign_v1(4)).unwrap();
    learner.update_views(&views(&owned)).unwrap();
    let snapshot = learner.snapshot();
    let text = serde_json::to_string(&snapshot).unwrap();
    let parsed: SnapshotV2 = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed, snapshot);
    let mut restored = LearnerV2::restore(parsed).unwrap();
    assert_eq!(restored.snapshot(), snapshot);
    assert_eq!(restored.snapshot().state_sha256(), snapshot.state_sha256());
    // Restored and original continue to identical bits.
    let first = learner.update_views(&views(&owned)).unwrap();
    let second = restored.update_views(&views(&owned)).unwrap();
    assert_eq!(first, second);
    assert_eq!(learner.snapshot(), restored.snapshot());
    // A fresh step-0 snapshot also restores.
    let fresh = LearnerV2H16::new(OptimizerConfigV2::campaign_v1(4)).unwrap();
    assert_eq!(
        LearnerV2H16::restore(fresh.snapshot()).unwrap().snapshot(),
        fresh.snapshot()
    );
    let mut unknown: serde_json::Value = serde_json::from_str(&text).unwrap();
    unknown["extra"] = json!(1);
    assert!(serde_json::from_value::<SnapshotV2>(unknown).is_err());
}

#[test]
fn restore_refuses_wrong_or_damaged_snapshots() {
    let owned = batch();
    let mut learner = LearnerV2::new(OptimizerConfigV2::campaign_v1(4)).unwrap();
    let fresh = learner.snapshot();
    learner.update_views(&views(&owned)).unwrap();
    let trained = learner.snapshot();
    let refuses = |snapshot: SnapshotV2, what: &str| {
        assert_eq!(
            LearnerV2::restore(snapshot).err(),
            Some(Error::ModelContract),
            "{what}"
        );
    };
    let mut s = trained.clone();
    s.contract = CONTRACT_V2_H16.into();
    refuses(s, "contract");
    let mut s = trained.clone();
    s.feature_identity = feature_identity_v1();
    refuses(s, "feature identity");
    refuses(
        LearnerV2H16::new(OptimizerConfigV2::campaign_v1(4))
            .unwrap()
            .snapshot(),
        "other profile",
    );
    let mut s = trained.clone();
    s.parameter_bits.pop();
    refuses(s, "short parameters");
    let mut s = trained.clone();
    s.first_moment_bits.push(0);
    refuses(s, "long first moment");
    let mut s = trained.clone();
    s.second_moment_bits.clear();
    refuses(s, "empty second moment");
    let mut s = trained.clone();
    s.parameter_bits[5] = f64::NAN.to_bits();
    refuses(s, "NaN parameter");
    let mut s = trained.clone();
    s.first_moment_bits[9] = f64::INFINITY.to_bits();
    refuses(s, "infinite moment");
    let mut s = trained.clone();
    s.second_moment_bits[3] = (-1e-9_f64).to_bits();
    refuses(s, "negative second moment");
    let mut s = fresh.clone();
    s.first_moment_bits[0] = 1e-12_f64.to_bits();
    refuses(s, "first moment at step 0");
    let mut s = fresh.clone();
    s.second_moment_bits[7] = 1e-12_f64.to_bits();
    refuses(s, "second moment at step 0");
    let mut s = trained.clone();
    s.step = 5;
    refuses(s, "step past the horizon");
    let mut s = trained.clone();
    s.config.beta2 = 1.;
    refuses(s, "invalid configuration");
    assert!(LearnerV2::restore(trained).is_ok());
    assert!(LearnerV2::restore(fresh).is_ok());
}

#[test]
fn failed_updates_leave_the_state_unchanged() {
    let owned = batch();
    let mut learner = LearnerV2::new(OptimizerConfigV2::campaign_v1(2)).unwrap();
    learner.update_views(&views(&owned)).unwrap();
    let before = learner.snapshot();
    let mut check = |trajectories: Vec<TrajectoryView<'_>>, error: Error| {
        assert_eq!(learner.update_views(&trajectories), Err(error));
        assert_eq!(learner.snapshot(), before);
    };
    // The selected ordinal of the last trajectory's last decision is out of range.
    let mut bad = views(&owned);
    let last = bad.last_mut().unwrap().decisions.last_mut().unwrap();
    last.selected = last.view.actions.len();
    check(bad, Error::ActionRange);
    // A repeated (pair, leg, seat) key.
    let mut repeated = views(&owned);
    repeated[5].pair_index = 0;
    repeated[5].leg = 0;
    repeated[5].seat = 0;
    check(repeated, Error::ModelContract);
    // A forced single-action step is not a retained decision.
    let forced = decision(4);
    let mut with_forced = views(&owned);
    with_forced[2].decisions.push(SelectedView {
        view: DecisionView {
            state: &forced.state,
            actions: &forced.actions[..1],
            flags: &forced.flags[..1],
        },
        selected: 0,
    });
    check(with_forced, Error::ActionRange);
    // A return outside {-1, 0, 1}, a leg outside {0, 1} and an empty batch.
    let mut out_of_range = views(&owned);
    out_of_range[3].terminal = natural(2);
    check(out_of_range, Error::ModelContract);
    let mut leg = views(&owned);
    leg[1].leg = 2;
    check(leg, Error::ModelContract);
    check(Vec::new(), Error::NoDecision);
    // Only truncated games: nothing to learn from.
    let mut truncated = views(&owned);
    for t in &mut truncated {
        t.terminal = TerminalV2::Truncated;
    }
    check(truncated, Error::NoDecision);
    // The horizon is two updates; the third is refused.
    learner.update_views(&views(&owned)).unwrap();
    let after = learner.snapshot();
    assert_eq!(learner.update_views(&views(&owned)), Err(Error::Range));
    assert_eq!(learner.snapshot(), after);
}

#[test]
fn collection_order_does_not_change_the_bits() {
    let owned = batch();
    let mut canonical = LearnerV2::new(OptimizerConfigV2::campaign_v1(3)).unwrap();
    let mut shuffled = LearnerV2::new(OptimizerConfigV2::campaign_v1(3)).unwrap();
    for round in 0..3 {
        let mut permuted = views(&owned);
        permuted.reverse();
        permuted.swap(0, 3 + round);
        let a = canonical.update_views(&views(&owned)).unwrap();
        let b = shuffled.update_views(&permuted).unwrap();
        assert_eq!(a, b);
        assert_eq!(canonical.snapshot(), shuffled.snapshot());
    }
}

#[test]
fn truncated_games_add_nothing_to_either_loss() {
    // A truncated seat in the middle of the canonical order, with decisions,
    // changes neither the advantages, nor N, nor either loss.
    let owned = batch();
    let mut extended = batch();
    extended.push(trajectory(1, 0, 1, TerminalV2::Truncated, decisions(61, 6)));
    let mut plain = LearnerV2::new(OptimizerConfigV2::campaign_v1(2)).unwrap();
    let mut with_truncated = LearnerV2::new(OptimizerConfigV2::campaign_v1(2)).unwrap();
    for _ in 0..2 {
        let a = plain.update_views(&views(&owned)).unwrap();
        let b = with_truncated.update_views(&views(&extended)).unwrap();
        assert_eq!(a.truncated_trajectories, 0);
        assert_eq!(b.truncated_trajectories, 1);
        assert_eq!(
            UpdateReportV2 {
                truncated_trajectories: 0,
                ..b
            },
            a
        );
        assert_eq!(plain.snapshot(), with_truncated.snapshot());
    }
}

#[test]
fn pair_centered_rewards_encode_half_integer_advantages() {
    assert_eq!(pair_centered_terminal_rewards_v1(1, Some(-1)), Ok([1, 1]));
    assert_eq!(pair_centered_terminal_rewards_v1(1, Some(0)), Ok([1, 0]));
    assert_eq!(pair_centered_terminal_rewards_v1(0, Some(0)), Ok([0, 0]));
    assert_eq!(pair_centered_terminal_rewards_v1(1, Some(1)), Ok([0, 0]));
    assert_eq!(pair_centered_terminal_rewards_v1(-1, Some(0)), Ok([-1, 0]));
    assert_eq!(pair_centered_terminal_rewards_v1(-1, Some(1)), Ok([-1, -1]));
    assert_eq!(pair_centered_terminal_rewards_v1(1, None), Ok([0, 0]));
    assert_eq!(
        pair_centered_terminal_rewards_v1(2, None),
        Err(Error::ModelContract)
    );
    assert_eq!(
        pair_centered_terminal_rewards_v1(0, Some(-2)),
        Err(Error::ModelContract)
    );
    for own in -1..=1_i8 {
        for other in -1..=1_i8 {
            let rewards = pair_centered_terminal_rewards_v1(own, Some(other)).unwrap();
            let mean = f64::from(own + other) / 2.;
            assert_eq!(
                f64::from(rewards[0] + rewards[1]) / 2.,
                f64::from(own) - mean
            );
        }
    }
}

/// Ruling (11): one checkpoint digest, pinned here, that hosted CI checks on
/// Linux and on Windows. Three updates of the design configuration from the
/// deterministic initialization over a fixed batch.
#[test]
fn pinned_checkpoint_state_digests() {
    fn digest<const H: usize>() -> String {
        let owned = batch();
        let mut learner = TreeLearnerV2::<H>::new(OptimizerConfigV2::campaign_v1(3)).unwrap();
        for _ in 0..3 {
            learner.update_views(&views(&owned)).unwrap();
        }
        assert_eq!(learner.step(), 3);
        learner.snapshot().state_sha256()
    }
    let wide = digest::<32>();
    let narrow = digest::<16>();
    println!("current_tree_learner_v2 pinned state digests: h32 {wide} h16 {narrow}");
    assert_eq!(wide, "pending");
    assert_eq!(narrow, "pending");
}
