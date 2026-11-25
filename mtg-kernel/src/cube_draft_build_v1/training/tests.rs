use super::*;

const HISTORY_ONLY_CARD: u16 = 55;

fn data(seed: u64) -> (ModelV1, ModelCardRowsV1) {
    data_with_widths(seed, 3, 2)
}

fn data_with_widths(seed: u64, table: usize, dense: usize) -> (ModelV1, ModelCardRowsV1) {
    let mut rng = SplitMix64::seed(seed);
    let mut draw = || ((rng.next_u64() >> 40) as f32 / (1 << 24) as f32 - 0.5) * 0.45;
    let rows = ModelCardRowsV1::new_v1(
        (0..64).collect(),
        table,
        (0..64 * table).map(|_| draw() * 3.).collect(),
        dense,
        (0..64 * dense).map(|_| draw() * 4.).collect(),
        [60, 63, 59, 62, 61],
    )
    .unwrap();
    let mut model = ModelV1::initialise_v1(64, table, dense, seed).unwrap();
    model.visit_tensors_mut_v1(|_, _, _, params| {
        for p in params {
            *p = draw();
        }
    });
    (model, rows)
}
fn row(
    head: Head,
    options: Vec<u16>,
    logits: &[f32],
    chosen: usize,
    value: Option<f32>,
) -> (Row, f64) {
    let mask = vec![true; logits.len()];
    let counts = crate::m4_sampler_v1::counts_v1(logits, &mask).unwrap();
    let word = counts[..chosen].iter().sum::<u64>() << 11;
    let sample = sample_v1(logits, &mask, word).unwrap();
    assert_eq!(sample.selected_index, chosen);
    (
        Row {
            kind: head,
            sampler: SAMPLER_ID_V1.into(),
            chosen: options[chosen],
            options,
            stream_values: vec![word],
            logits_bits: logits.iter().map(|x| x.to_bits()).collect(),
            p_chosen_bits: (counts[chosen] as f64 / GRID_SIZE as f64).to_bits(),
            value_bits: value.map(f32::to_bits),
        },
        sample.selected_log_probability,
    )
}
fn pick(f: &PickForwardV1<'_>, step: usize) -> ((PickStateV1, Row), f64, f32) {
    pick_choice(f, step, 0)
}
fn pick_choice(
    f: &PickForwardV1<'_>,
    step: usize,
    choice: usize,
) -> ((PickStateV1, Row), f64, f32) {
    let state = PickStateV1 {
        pack: (step as u16..(step + PACK_SIZE - step % PACK_SIZE) as u16).collect(),
        pool: (0..step as u16).collect(),
        history: vec![HISTORY_ONLY_CARD, 63],
        step: step as u8,
        direction: if step / PACK_SIZE == 1 { -1 } else { 1 },
    };
    let c = f.pick_logits_v1(&state).unwrap();
    let value = f.value_v1(&state).unwrap();
    let (r, lp) = row(Head::Pick, c.ids, &c.logits, choice, Some(value));
    ((state, r), lp, value)
}
fn build(f: &BuildForwardV1<'_>, pool: &[u16], sibling: usize, b: usize, mask: u8) -> (Build, f64) {
    let ctx = f.build_context_v1(pool).unwrap();
    let (count, mut path) = row(
        Head::Count,
        (0..=40).collect(),
        &ctx.count_logits_v1(),
        b,
        None,
    );
    let mut remaining = pool.to_vec();
    let mut chosen = Vec::new();
    let mut pointer = Vec::new();
    for _ in 0..40 - b {
        let c = ctx.pointer_logits_v1(b, &chosen, &remaining).unwrap();
        let (r, lp) = row(Head::Pointer, c.ids, &c.logits, sibling, None);
        chosen.push(r.chosen);
        remaining.retain(|&x| x != r.chosen);
        pointer.push(r);
        path += lp;
    }
    let mut type_set = None;
    let mut split = Vec::new();
    if b > 0 {
        let ctx = ctx.type_context_v1(b, &chosen).unwrap();
        let all = ctx.typeset_logits_v1();
        let options: Vec<_> = (1u16..32)
            .filter(|m| m.count_ones() as usize <= b)
            .collect();
        let logits: Vec<_> = options.iter().map(|&m| all[usize::from(m) - 1]).collect();
        let index = options.iter().position(|&m| m == u16::from(mask)).unwrap();
        let (r, lp) = row(Head::TypeSet, options, &logits, index, None);
        type_set = Some(r);
        path += lp;
        let types = mask.count_ones() as usize;
        if types > 1 && b > types {
            let c = ctx.split_logits_v1(mask).unwrap();
            let options: Vec<_> = c.type_bits.into_iter().map(u16::from).collect();
            let mut term = vec![0.; types];
            let mut n = vec![0; types];
            for i in 0..b - types {
                let k = if i % 5 == 0 {
                    (sibling + 1) % types
                } else {
                    sibling % types
                };
                let (r, lp) = row(Head::Split, options.clone(), &c.logits, k, None);
                split.push(r);
                term[k] = lp;
                n[k] += 1;
            }
            path += term
                .iter()
                .zip(n)
                .map(|(l, n)| l * f64::from(n))
                .sum::<f64>();
        }
    }
    (
        Build {
            count,
            pointer,
            type_set,
            split,
        },
        path,
    )
}
fn joint_picks(model: &ModelV1, rows: &ModelCardRowsV1, choice: usize) -> Vec<(PickStateV1, Row)> {
    let forward = model.pick_forward_v1(rows, 1).unwrap();
    (0..STEP_COUNT_V1)
        .filter(|step| step % PACK_SIZE != PACK_SIZE - 1)
        .map(|step| pick_choice(&forward, step, choice).0)
        .collect()
}

fn scores() -> Role<'static> {
    Role::Learning {
        scores: [[1; 7], [-1; 7]],
    }
}

#[test]
fn central_differences_cover_every_tensor() {
    let (mut model, rows) = data_with_widths(44, 58, 58);
    let pool: Vec<_> = (0..45).collect();
    let (pick_row, pick_lp, base) = pick(&model.pick_forward_v1(&rows, 1).unwrap(), 6);
    let (record, build_lp) = build(&model.build_forward_v1(&rows, 1).unwrap(), &pool, 0, 38, 7);
    let picks = [pick_row];
    let builds = [record];
    let chair = Chair {
        role: scores(),
        picks: &picks,
        pool: &pool,
        builds: &builds,
    };
    let mut batch = Batch::new(&model, 1);
    let target = 0.75;
    paths::picks(&model, &rows, &chair, target, Some(&mut batch)).unwrap();
    paths::builds(&model, &rows, &chair, [1.5, 0.], Some(&mut batch)).unwrap();
    let c = f64::from(base) - target;
    assert_eq!(batch.losses.pick, c * pick_lp);
    assert_eq!(batch.losses.value, 0.5 * c * c);
    assert_eq!(batch.losses.build, -0.75 * build_lp);

    // Use D2's independent public forward on both sides. Keep the pick
    // advantage fixed and exclude the value loss when perturbing its inputs.
    let mut rng = SplitMix64::seed(0xd4_0044);
    for (i, spec) in model.specs.clone().into_iter().enumerate() {
        let grad = &batch.grads[i];
        let (peak, _) = grad
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.abs().total_cmp(&b.abs()))
            .unwrap();
        let mut random = (rng.next_u64() % grad.len() as u64) as usize;
        if grad.len() > 1 && random == peak {
            random = (random + 1) % grad.len();
        }
        // Random coordinates can be inactive (for instance unused embedding rows).
        let mut probes = vec![(peak, true), (random, false)];
        let id = T::ALL[i];
        let width = *spec.shape.last().unwrap();
        let slices: &[(usize, usize)] = match id {
            T::BuildMlp0Weight | T::PickMlp0Weight => {
                assert_eq!(width, 74);
                &[(0, 74)]
            }
            T::StateEncoder0Weight | T::Value0Weight => {
                &[(HISTORY_COLUMNS_V1.start, HISTORY_COLUMNS_V1.end)]
            }
            T::TypesetScorer0Weight => &[(2 * H, 2 * H + 1)],
            T::PointerContextWeight => &[(2 * H, 2 * H + 1), (2 * H + 1, 2 * H + 2)],
            _ => &[],
        };
        // Check every named input column at its strongest output row.
        for &(start, end) in slices {
            for column in start..end {
                let index = (column..grad.len())
                    .step_by(width)
                    .max_by(|&a, &b| grad[a].abs().total_cmp(&grad[b].abs()))
                    .unwrap();
                probes.push((index, true));
            }
        }
        if id == T::PickEmbedding {
            let start = rows.row_of(HISTORY_ONLY_CARD).unwrap() * width;
            probes.extend((start..start + width).map(|j| (j, true)));
        }
        for (j, require_signal) in probes {
            let analytic = grad[j];
            if require_signal {
                assert!(analytic.abs() > 1e-7, "no signal in {}[{j}]", spec.name);
            }
            let center = model.values[i][j];
            // A wider perturbation resolves small slopes above f32 forward rounding.
            let delta = if j == peak { 1. / 128. } else { 1. / 32. };
            let mut eval = |v| {
                model.values[i][j] = v;
                match spec.set {
                    ParameterSetV1::Build => {
                        -0.75 * build(&model.build_forward_v1(&rows, 1).unwrap(), &pool, 0, 38, 7).1
                    }
                    set => {
                        let (_, lp, v) = pick(&model.pick_forward_v1(&rows, 1).unwrap(), 6);
                        if set == ParameterSetV1::Pick {
                            c * lp
                        } else {
                            0.5 * (f64::from(v) - target).powi(2)
                        }
                    }
                }
            };
            let hi = center + delta;
            let lo = center - delta;
            let numerical = (eval(hi) - eval(lo)) / f64::from(hi - lo);
            model.values[i][j] = center;
            let error = (numerical - f64::from(analytic)).abs();
            assert!(
                error <= 0.025 * f64::from(analytic.abs()) + 2e-5
                    && (!require_signal || error < f64::from(analytic.abs()) / 2.),
                "{}[{j}] analytic {analytic} numerical {numerical}",
                spec.name
            );
        }
    }
    assert_eq!(model.layout().len(), 44);
}
#[test]
fn detached_value_never_moves_pick_parameters() {
    let (model, rows) = data(9);
    let (p, _, v) = pick(&model.pick_forward_v1(&rows, 1).unwrap(), 7);
    let picks = [p];
    let chair = Chair {
        role: scores(),
        picks: &picks,
        pool: &[],
        builds: &[],
    };
    let mut joint = Batch::new(&model, 1);
    paths::picks(&model, &rows, &chair, f64::from(v) + 1., Some(&mut joint)).unwrap();
    // Vary only the value output weights, preserving V at this input by
    // assigning the original value to its bias.
    let mut altered = model.clone();
    altered.values[T::ValueOutWeight.index()].fill(0.);
    altered.values[T::ValueOutBias.index()][0] = v;
    let mut linear = Batch::new(&altered, 1);
    paths::picks(
        &altered,
        &rows,
        &chair,
        f64::from(v) + 1.,
        Some(&mut linear),
    )
    .unwrap();
    assert_ne!(
        joint.grads[T::Value2Bias.index()],
        linear.grads[T::Value2Bias.index()]
    );
    for (i, spec) in model.specs.iter().enumerate() {
        if spec.set == ParameterSetV1::Pick {
            assert_eq!(joint.grads[i], linear.grads[i], "{}", spec.name);
        }
    }
}

#[test]
fn replay_is_atomic_for_current_and_snapshot_chairs() {
    let (model, rows) = data(12);
    let (snap, _) = data(13);
    let pool: Vec<_> = (0..45).collect();
    let picks = joint_picks(&model, &rows, 0);
    let older = joint_picks(&snap, &rows, 0);
    let f = model.build_forward_v1(&rows, 1).unwrap();
    let builds = [build(&f, &pool, 0, 38, 7).0, build(&f, &pool, 1, 39, 3).0];
    let old_builds = [build(&snap.build_forward_v1(&rows, 1).unwrap(), &pool, 0, 38, 7).0];
    let league = BTreeMap::from([("pinned".into(), &snap)]);
    let corruptions = [
        Head::Pick,
        Head::Pick,
        Head::Pick,
        Head::Pick,
        Head::Count,
        Head::Pointer,
        Head::TypeSet,
        Head::Split,
    ];
    for (chair_index, (case, &head)) in (0..2).flat_map(|chair| {
        corruptions
            .iter()
            .enumerate()
            .map(move |case| (chair, case))
    }) {
        let mut m = model.clone();
        let mut opt = Optimizer::new(&m);
        let untouched = opt.clone();
        let mut current = picks.clone();
        let mut old = older.clone();
        let mut current_builds = builds.clone();
        let mut snapshot_builds = old_builds.clone();
        let (pick, build) = if chair_index == 0 {
            (&mut current[0].1, &mut current_builds[0])
        } else {
            (&mut old[41].1, &mut snapshot_builds[0])
        };
        let row = match head {
            Head::Pick => pick,
            Head::Count => &mut build.count,
            Head::Pointer => &mut build.pointer[0],
            Head::TypeSet => build.type_set.as_mut().unwrap(),
            Head::Split => &mut build.split[0],
        };
        match case {
            1 => row.p_chosen_bits ^= 1,
            2 => *row.value_bits.as_mut().unwrap() ^= 1,
            3 => row.stream_values[0] = u64::MAX,
            _ => row.logits_bits[0] ^= 1,
        }
        let chairs = [
            Chair {
                role: scores(),
                picks: &current,
                pool: &pool,
                builds: &current_builds,
            },
            Chair {
                role: Role::Older {
                    picker_pin: Some("pinned"),
                    builder_pin: "pinned",
                },
                picks: &old,
                pool: &pool,
                builds: &snapshot_builds,
            },
        ];
        let error = update(&mut m, &mut opt, &rows, Phase::Joint, &chairs, &league).unwrap_err();
        assert!(
            error.starts_with(&format!("chair {chair_index}: {head:?} replay")),
            "{error}"
        );
        assert_eq!(m.parameter_sha256(), model.parameter_sha256());
        assert_eq!(opt, untouched);
    }
    let mut m = model.clone();
    let mut opt = Optimizer::new(&m);
    let chairs = [
        Chair {
            role: scores(),
            picks: &picks,
            pool: &pool,
            builds: &builds,
        },
        Chair {
            role: Role::Older {
                picker_pin: Some("pinned"),
                builder_pin: "pinned",
            },
            picks: &older,
            pool: &pool,
            builds: &old_builds,
        },
    ];
    let mut solo = m.clone();
    let mut solo_opt = opt.clone();
    update(
        &mut solo,
        &mut solo_opt,
        &rows,
        Phase::Joint,
        &chairs[..1],
        &league,
    )
    .unwrap();
    update(&mut m, &mut opt, &rows, Phase::Joint, &chairs, &league).unwrap();
    assert_eq!(m.parameter_sha256(), solo.parameter_sha256());
    assert_eq!(opt, solo_opt);
    assert_eq!(opt.steps, [1, 1, 1]);
    assert_eq!(
        opt.head_updates,
        HeadUpdates {
            pick: 1,
            value: 1,
            count: 1,
            pointer: 1,
            type_set: 1,
            split: 1
        }
    );
}
#[test]
fn pick_return_rounds_once_over_fourteen_scores() {
    let scores = [[-1; 7], [-1, -1, -1, -1, -1, 0, 0]];
    let (pick_return, [a, b]) = chair_returns(&scores).unwrap();
    assert_eq!(pick_return.to_bits(), 0xbfeb_6db6_db6d_b6db);
    let mean_of_sibling_means = (a + b) / 2.;
    assert_eq!(mean_of_sibling_means.to_bits(), 0xbfeb_6db6_db6d_b6dc);
}

#[test]
fn older_only_split_rows_do_not_increment_head_updates() {
    let (mut model, rows) = data(13);
    let (snapshot, _) = data(19);
    let pool: Vec<_> = (0..45).collect();
    let forward = model.build_forward_v1(&rows, 1).unwrap();
    let builds = [
        build(&forward, &pool, 0, 38, 1).0,
        build(&forward, &pool, 1, 39, 2).0,
    ];
    let old_forward = snapshot.build_forward_v1(&rows, 1).unwrap();
    let old_build = [build(&old_forward, &pool, 0, 38, 7).0];
    assert!(builds.iter().all(|build| build.split.is_empty()));
    assert!(!old_build[0].split.is_empty());
    let chairs = [
        Chair {
            role: scores(),
            picks: &[],
            pool: &pool,
            builds: &builds,
        },
        Chair {
            role: Role::Older {
                picker_pin: None,
                builder_pin: "old",
            },
            picks: &[],
            pool: &pool,
            builds: &old_build,
        },
    ];
    let mut opt = Optimizer::new(&model);
    opt.head_updates.split = 3;
    update(
        &mut model,
        &mut opt,
        &rows,
        Phase::Builder,
        &chairs,
        &BTreeMap::from([("old".into(), &snapshot)]),
    )
    .unwrap();
    assert_eq!(
        opt.head_updates,
        HeadUpdates {
            count: 1,
            pointer: 1,
            type_set: 1,
            split: 3,
            ..HeadUpdates::default()
        }
    );
}

#[test]
fn builder_phase_freezes_sets_and_counts_present_heads_only() {
    let (mut m, rows) = data(13);
    let before = m.clone();
    let pool: Vec<_> = (0..45).collect();
    let f = m.build_forward_v1(&rows, 1).unwrap();
    let builds = [build(&f, &pool, 0, 40, 1).0, build(&f, &pool, 1, 40, 2).0];
    let chair = Chair {
        role: scores(),
        picks: &[],
        pool: &pool,
        builds: &builds,
    };
    let mut opt = Optimizer::new(&m);
    update(
        &mut m,
        &mut opt,
        &rows,
        Phase::Builder,
        &[chair],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(opt.steps, [1, 0, 0]);
    assert_eq!(
        opt.head_updates,
        HeadUpdates {
            count: 1,
            type_set: 1,
            ..HeadUpdates::default()
        }
    );
    for (i, s) in m.specs.iter().enumerate() {
        if s.set != ParameterSetV1::Build {
            assert_eq!(before.values[i], m.values[i]);
            assert!(opt.first[i]
                .iter()
                .chain(&opt.second[i])
                .all(|v| v.to_bits() == 0));
        }
    }
    assert_ne!(m.parameter_sha256(), before.parameter_sha256());
}

#[test]
fn grouped_split_gradient_rounds_once_and_masked_entries_stay_zero() {
    let lp = [-0.3125, -1.25, -150., f64::NEG_INFINITY];
    let terms = [(0, 2.5), (1, -1.75), (2, 0.)];
    let g = policy_gradient(&lp, &terms);
    for (i, &x) in lp[..3].iter().enumerate() {
        let p = m4_exp(x.clamp(-120., 0.));
        let reference = (2.5 * (f64::from(i == 0) - p) - 1.75 * (f64::from(i == 1) - p)) as f32;
        assert_eq!(g[i].to_bits(), reference.to_bits());
    }
    // Scale up to make the floor visible after the f32 cast.
    let g = policy_gradient(&lp, &[(0, 1e20)]);
    assert_eq!(
        g[2].to_bits(),
        ((-1e20 * m4_exp((-150_f64).clamp(-120., 0.))) as f32).to_bits()
    );
    assert_eq!(g[3].to_bits(), 0);
}

#[test]
fn adam_uses_independent_bias_corrections_and_preserves_failed_state() {
    let (mut m, _) = data(77);
    let mut opt = Optimizer::new(&m);
    let mut batch = Batch::new(&m, 1);
    let id = T::CountHeadBias.index();
    let pick = T::PickScorer2Weight.index();
    let mut expected = m.values[id][0];
    let mut first = 0_f32;
    let mut second = 0_f32;
    for (t, g, phase) in [(1, 0.75_f32, Phase::Builder), (2, -0.5, Phase::Joint)] {
        batch.grads[id][0] = g;
        batch.grads[pick][0] = 0.25;
        let original_pick = m.values[pick][0];
        adam(&mut m, &mut opt, phase, &batch).unwrap();
        first += (g - first) * (1. - 0.9_f32);
        second = second * 0.999 + g * g * (1. - 0.999_f32);
        let c1 = 1. - MATH.powi_f64_v1(f64::from(0.9_f32), t);
        let c2 = 1. - MATH.powi_f64_v1(f64::from(0.999_f32), t);
        expected += -((f64::from(0.001_f32) / c1) as f32) * first
            / (second.sqrt() / c2.sqrt() as f32 + 1e-8);
        assert_eq!(m.values[id][0].to_bits(), expected.to_bits());
        assert_eq!(opt.first[id][0], first);
        assert_eq!(opt.second[id][0], second);
        if t == 1 {
            assert_eq!(m.values[pick][0], original_pick);
        } else {
            assert!((m.values[pick][0] - original_pick + 0.001).abs() < 1e-7);
        }
    }
    assert_eq!(opt.steps, [2, 1, 1]);
    batch.grads[T::ValueOutBias.index()][0] = f32::NAN;
    let before = m.parameter_sha256();
    let saved = opt.clone();
    assert!(adam(&mut m, &mut opt, Phase::Joint, &batch).is_err());
    assert_eq!(m.parameter_sha256(), before);
    assert_eq!(opt, saved);
}

#[test]
fn joint_update_credits_each_learning_chair_with_two_seat_denominator() {
    let (mut model, rows) = data(17);
    let (snapshot, _) = data(19);
    let pool: Vec<_> = (0..45).collect();
    let picks0 = joint_picks(&model, &rows, 0);
    let picks1 = joint_picks(&model, &rows, 1);
    let older = joint_picks(&snapshot, &rows, 0);
    let forward = model.build_forward_v1(&rows, 1).unwrap();
    let builds0 = [
        build(&forward, &pool, 0, 38, 7).0,
        build(&forward, &pool, 1, 39, 3).0,
    ];
    let builds1 = [builds0[1].clone(), builds0[0].clone()];
    let old_build = [build(
        &snapshot.build_forward_v1(&rows, 1).unwrap(),
        &pool,
        0,
        38,
        7,
    )
    .0];
    let chairs = [
        Chair {
            role: Role::Learning {
                scores: [[1; 7], [0; 7]],
            },
            picks: &picks0,
            pool: &pool,
            builds: &builds0,
        },
        Chair {
            role: Role::Learning {
                scores: [[-1; 7], [-1; 7]],
            },
            picks: &picks1,
            pool: &pool,
            builds: &builds1,
        },
        Chair {
            role: Role::Older {
                picker_pin: Some("old"),
                builder_pin: "old",
            },
            picks: &older,
            pool: &pool,
            builds: &old_build,
        },
    ];
    // Supply the focal targets and sibling contrasts independently of update.
    // Accumulate directly into the two-seat batch to preserve the declared sum order.
    let mut expected = Batch::new(&model, 2);
    for (chair, target, advantages) in [(&chairs[0], 0.5, [1., -1.]), (&chairs[1], -1., [0., 0.])] {
        paths::picks(&model, &rows, chair, target, Some(&mut expected)).unwrap();
        paths::builds(&model, &rows, chair, advantages, Some(&mut expected)).unwrap();
    }
    let mut opt = Optimizer::new(&model);
    let losses = update(
        &mut model,
        &mut opt,
        &rows,
        Phase::Joint,
        &chairs,
        &BTreeMap::from([("old".into(), &snapshot)]),
    )
    .unwrap();
    assert_eq!(
        losses.pick.to_bits(),
        expected.losses.pick.to_bits(),
        "pick loss"
    );
    assert_eq!(
        losses.value.to_bits(),
        expected.losses.value.to_bits(),
        "value loss"
    );
    assert_eq!(
        losses.build.to_bits(),
        expected.losses.build.to_bits(),
        "build loss"
    );
    for (i, grad) in expected.grads.iter().enumerate() {
        for (j, &g) in grad.iter().enumerate() {
            let first = (g - 0.0) * (1.0 - 0.9_f32);
            assert_eq!(
                opt.first[i][j].to_bits(),
                first.to_bits(),
                "{}[{j}] first moment",
                model.specs[i].name
            );
        }
    }
}

#[test]
fn unequal_policy_gradient_matches_independent_f64_softmax() {
    let logits = [0.75_f32, 1.75];
    let sample = sample_v1(&logits, &[true; 2], 0).unwrap();
    // The logits differ by one. Their exact softmax odds are 1:e.
    let denominator = 1.0 + std::f64::consts::E;
    let oracle = [1.0 / denominator, std::f64::consts::E / denominator];
    for (j, p) in oracle.into_iter().enumerate() {
        let actual = m4_exp(sample.log_probabilities[j].clamp(-120., 0.));
        assert!((actual - p).abs() <= 1e-8 * p, "p[{j}] {actual} vs {p}");
    }
    // At coefficient 25 the two analytical magnitudes lie within 1e-8
    // relative of an f32, so the same tolerance tests the final cast too.
    for chosen in 0..2 {
        let grad = policy_gradient(&sample.log_probabilities, &[(chosen, 25.)]);
        for (j, p) in oracle.into_iter().enumerate() {
            let expected = 25. * (f64::from(j == chosen) - p);
            assert!(
                (f64::from(grad[j]) - expected).abs() <= 1e-8 * expected.abs(),
                "choice {chosen}, logit {j}: {} vs {expected}",
                grad[j]
            );
        }
    }
}
