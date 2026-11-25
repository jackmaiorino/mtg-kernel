use super::*;

fn data(seed: u64) -> (ModelV1, ModelCardRowsV1) {
    let mut rng = SplitMix64::seed(seed);
    let mut draw = || ((rng.next_u64() >> 40) as f32 / (1 << 24) as f32 - 0.5) * 0.45;
    let rows = ModelCardRowsV1::new_v1(
        (0..64).collect(),
        3,
        (0..64 * 3).map(|_| draw() * 3.).collect(),
        2,
        (0..128).map(|_| draw() * 4.).collect(),
        [60, 63, 59, 62, 61],
    )
    .unwrap();
    let mut model = ModelV1::initialise_v1(64, 3, 2, seed).unwrap();
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
    let state = PickStateV1 {
        pack: (step as u16..(step + PACK_SIZE - step % PACK_SIZE) as u16).collect(),
        pool: (0..step as u16).collect(),
        history: vec![61, 63],
        step: step as u8,
        direction: if step / PACK_SIZE == 1 { -1 } else { 1 },
    };
    let c = f.pick_logits_v1(&state).unwrap();
    let value = f.value_v1(&state).unwrap();
    let (r, lp) = row(Head::Pick, c.ids, &c.logits, 0, Some(value));
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
fn scores() -> Role<'static> {
    Role::Learning {
        scores: [[1; 7], [-1; 7]],
    }
}

#[test]
fn central_differences_cover_every_tensor() {
    let (mut model, rows) = data(44);
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
    for (i, spec) in model.specs.clone().into_iter().enumerate() {
        let (j, &analytic) = batch.grads[i]
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.abs().total_cmp(&b.abs()))
            .unwrap();
        assert!(analytic.abs() > 1e-7, "no signal in {}", spec.name);
        let center = model.values[i][j];
        let delta = 1. / 128.;
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
                && error < f64::from(analytic.abs()) / 2.,
            "{}[{j}] analytic {analytic} numerical {numerical}",
            spec.name
        );
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
    let make_picks = |model: &ModelV1| {
        let f = model.pick_forward_v1(&rows, 1).unwrap();
        (0..45)
            .filter(|n| n % 15 != 14)
            .map(|n| pick(&f, n).0)
            .collect::<Vec<_>>()
    };
    let picks = make_picks(&model);
    let older = make_picks(&snap);
    let f = model.build_forward_v1(&rows, 1).unwrap();
    let builds = [build(&f, &pool, 0, 38, 7).0, build(&f, &pool, 1, 39, 3).0];
    let old_builds = [build(&snap.build_forward_v1(&rows, 1).unwrap(), &pool, 0, 40, 1).0];
    let league = BTreeMap::from([("pinned".into(), &snap)]);
    for tamper in 0..6 {
        let mut m = model.clone();
        let mut opt = Optimizer::new(&m);
        let untouched = opt.clone();
        let mut current = picks.clone();
        let mut old = older.clone();
        let row = if tamper < 3 {
            &mut current[0].1
        } else {
            &mut old[41].1
        };
        match tamper % 3 {
            0 => row.logits_bits[0] ^= 1,
            1 => row.p_chosen_bits ^= 1,
            _ => *row.value_bits.as_mut().unwrap() ^= 1,
        }
        let chairs = [
            Chair {
                role: scores(),
                picks: &current,
                pool: &pool,
                builds: &builds,
            },
            Chair {
                role: Role::Older {
                    picker_pin: "pinned",
                    builder_pin: "pinned",
                },
                picks: &old,
                pool: &pool,
                builds: &old_builds,
            },
        ];
        assert!(update(&mut m, &mut opt, &rows, Phase::Joint, &chairs, &league).is_err());
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
                picker_pin: "pinned",
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
