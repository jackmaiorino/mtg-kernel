use super::*;

fn logits(tape: &Tape<'_>, nodes: &[usize]) -> Vec<f32> {
    nodes.iter().map(|&n| tape.output(n)[0]).collect()
}
fn seed_scores(tape: &mut Tape<'_>, nodes: &[usize], grad: &[f32]) {
    for (&id, &d) in nodes.iter().zip(grad) {
        tape.seed(id, &[d]);
    }
}

pub(super) fn picks(
    model: &ModelV1,
    rows: &ModelCardRowsV1,
    chair: &Chair<'_>,
    target: f64,
    mut batch: Option<&mut Batch>,
) -> Result<()> {
    let mut tape = Tape::new(model, rows);
    for (state, row) in chair.picks {
        let resolved = resolve_pick_state(rows, state).map_err(|e| e.to_string())?;
        let pack = tape.card_mean(&resolved.pack.rows, TrunkV1::Pick);
        let pool = tape.card_mean(&resolved.pool_rows, TrunkV1::Pick);
        let history = tape.card_mean(&resolved.history_rows, TrunkV1::Pick);
        let mut turn = vec![0.; STEP_COUNT_V1 + 1];
        turn[usize::from(state.step)] = 1.;
        turn[STEP_COUNT_V1] = f32::from(state.direction);
        let turn = tape.constant(turn);
        let x = tape.join(&[pack, pool, history, turn]);
        let context = tape.mlp(
            x,
            T::StateEncoder0Weight,
            T::StateEncoder0Bias,
            T::StateEncoder2Weight,
            T::StateEncoder2Bias,
        );
        // Stop the value gradient at its state input.
        let vx = tape.constant(tape.output(x).to_vec());
        let vh = tape.mlp(
            vx,
            T::Value0Weight,
            T::Value0Bias,
            T::Value2Weight,
            T::Value2Bias,
        );
        let v = tape.linear(vh, T::ValueOutWeight, Some(T::ValueOutBias), false);
        let value = tape.output(v)[0];
        let scores: Vec<_> = resolved
            .pack
            .rows
            .iter()
            .map(|&r| {
                let h = tape.card(r, TrunkV1::Pick);
                tape.scorer(
                    &[context, h],
                    T::PickScorer0Weight,
                    T::PickScorer0Bias,
                    T::PickScorer2Weight,
                )
            })
            .collect();
        let sampled = replay(
            row,
            Head::Pick,
            &resolved.pack.ids,
            &logits(&tape, &scores),
            Some(value),
        )?;
        if let Some(b) = batch.as_deref_mut() {
            let (c, lv, dv) = pick_credit(target, value, b.seats);
            let (loss, grad) = policy_term(&sampled, c);
            b.losses.pick += loss;
            b.losses.value += lv;
            b.heads.pick = 1;
            b.heads.value = 1;
            seed_scores(&mut tape, &scores, &grad);
            tape.seed(v, &[dv]);
        }
    }
    if let Some(b) = batch {
        tape.backward(&mut b.grads);
    }
    Ok(())
}

pub(super) fn builds(
    model: &ModelV1,
    rows: &ModelCardRowsV1,
    chair: &Chair<'_>,
    advantages: [f64; 2],
    mut batch: Option<&mut Batch>,
) -> Result<()> {
    if chair.pool.len() != BUILD_POOL_CARDS_V1 {
        return Err("expected 45 pool cards".into());
    }
    let pool = resolve_set(rows, chair.pool, "build pool", ModelErrorV1::Build)
        .map_err(|e| e.to_string())?;
    for (build, &advantage) in chair.builds.iter().zip(&advantages) {
        let mut tape = Tape::new(model, rows);
        let mean = tape.card_mean(&pool.rows, TrunkV1::Build);
        let context = tape.mlp(
            mean,
            T::PoolEncoder0Weight,
            T::PoolEncoder0Bias,
            T::PoolEncoder2Weight,
            T::PoolEncoder2Bias,
        );
        let counts = tape.linear(context, T::CountHeadWeight, Some(T::CountHeadBias), false);
        let sample = replay(
            &build.count,
            Head::Count,
            &(0..=40).collect::<Vec<_>>(),
            tape.output(counts),
            None,
        )?;
        let basics = sample.selected_index;
        let c = batch
            .as_ref()
            .map_or(0., |b| build_coefficient(advantage, b.seats));
        let mut path = sample.selected_log_probability;
        if let Some(b) = batch.as_deref_mut() {
            b.heads.count = 1;
            tape.seed(
                counts,
                &policy_gradient(&sample.log_probabilities, &[(basics, c)]),
            );
        }
        if build.pointer.len() != DECK_CARDS_V1 - basics || build.type_set.is_some() != (basics > 0)
        {
            return Err("build path length differs from basics count".into());
        }
        let mut chosen = Vec::new();
        let mut remaining: Vec<_> = pool
            .ids
            .iter()
            .copied()
            .zip(pool.rows.iter().copied())
            .collect();
        for (j, row) in build.pointer.iter().enumerate() {
            let m = tape.card_mean(&chosen, TrunkV1::Build);
            let step = tape.constant(vec![ratio(basics, 40.), ratio(j, 40.)]);
            let x = tape.join(&[context, m, step]);
            let q = tape.linear(
                x,
                T::PointerContextWeight,
                Some(T::PointerContextBias),
                true,
            );
            let (ids, nodes): (Vec<_>, Vec<_>) = remaining
                .iter()
                .map(|&(id, r)| {
                    let h = tape.card(r, TrunkV1::Build);
                    (
                        id,
                        tape.scorer(
                            &[q, h],
                            T::PointerScorer0Weight,
                            T::PointerScorer0Bias,
                            T::PointerScorer2Weight,
                        ),
                    )
                })
                .unzip();
            let sample = replay(row, Head::Pointer, &ids, &logits(&tape, &nodes), None)?;
            path += sample.selected_log_probability;
            if let Some(b) = batch.as_deref_mut() {
                b.heads.pointer = 1;
                seed_scores(
                    &mut tape,
                    &nodes,
                    &policy_gradient(&sample.log_probabilities, &[(sample.selected_index, c)]),
                );
            }
            let (_, r) = remaining.remove(sample.selected_index);
            chosen.insert(chosen.partition_point(|&a| a < r), r);
        }
        if let Some(row) = &build.type_set {
            let mean = tape.card_mean(&chosen, TrunkV1::Build);
            let count = tape.constant(vec![ratio(basics, 40.)]);
            let input = tape.join(&[context, mean, count]);
            let q = tape.linear(
                input,
                T::TypesetContextWeight,
                Some(T::TypesetContextBias),
                true,
            );
            let basic_rows = rows.basic_rows();
            let (options, nodes): (Vec<_>, Vec<_>) = (1u16..32)
                .filter(|mask| mask.count_ones() as usize <= basics)
                .map(|mask| {
                    let mut members: Vec<_> = basic_rows
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| mask & (1 << i) != 0)
                        .map(|(_, r)| *r)
                        .collect();
                    members.sort_unstable();
                    let mean = tape.card_mean(&members, TrunkV1::Build);
                    let n = tape.constant(vec![ratio(members.len(), 5.)]);
                    (
                        mask,
                        tape.scorer(
                            &[q, mean, n],
                            T::TypesetScorer0Weight,
                            T::TypesetScorer0Bias,
                            T::TypesetScorer2Weight,
                        ),
                    )
                })
                .unzip();
            let sample = replay(row, Head::TypeSet, &options, &logits(&tape, &nodes), None)?;
            path += sample.selected_log_probability;
            if let Some(b) = batch.as_deref_mut() {
                b.heads.type_set = 1;
                seed_scores(
                    &mut tape,
                    &nodes,
                    &policy_gradient(&sample.log_probabilities, &[(sample.selected_index, c)]),
                );
            }
            let mask = options[sample.selected_index];
            let types = mask.count_ones() as usize;
            let draws = if types > 1 { basics - types } else { 0 };
            if build.split.len() != draws {
                return Err("split draw count differs".into());
            }
            if draws > 0 {
                let (bits, nodes): (Vec<_>, Vec<_>) = basic_rows
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| mask & (1 << i) != 0)
                    .map(|(i, &r)| {
                        let h = tape.card(r, TrunkV1::Build);
                        (
                            1u16 << i,
                            tape.scorer(
                                &[q, h],
                                T::SplitScorer0Weight,
                                T::SplitScorer0Bias,
                                T::SplitScorer2Weight,
                            ),
                        )
                    })
                    .unzip();
                let scores = logits(&tape, &nodes);
                let mut multiplicity = vec![0u32; types];
                let mut logp = Vec::new();
                for draw in &build.split {
                    let sample = replay(draw, Head::Split, &bits, &scores, None)?;
                    multiplicity[sample.selected_index] += 1;
                    logp = sample.log_probabilities;
                }
                let grouped: f64 = multiplicity
                    .iter()
                    .zip(&logp)
                    .map(|(&n, &lp)| f64::from(n) * lp)
                    .sum();
                path += grouped;
                if let Some(b) = batch.as_deref_mut() {
                    b.heads.split = 1;
                    let terms: Vec<_> = multiplicity
                        .iter()
                        .enumerate()
                        .map(|(k, &n)| (k, c * f64::from(n)))
                        .collect();
                    seed_scores(&mut tape, &nodes, &policy_gradient(&logp, &terms));
                }
            }
        } else if !build.split.is_empty() {
            return Err("split rows for zero basics".into());
        }
        if let Some(b) = batch.as_deref_mut() {
            b.losses.build += c * path;
            tape.backward(&mut b.grads);
        }
    }
    Ok(())
}
