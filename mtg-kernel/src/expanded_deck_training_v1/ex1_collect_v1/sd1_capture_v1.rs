//! SD1's capture at the placed decision (collab
//! LANES/search-diagnostic-20261108/DESIGN.md v4 sections 3.1, 3.2 and 6 E2).
//! Opt-in through `W5CaptureOptionsV1::sd1`; nothing here runs otherwise.
//!
//! - **Arms:** every arm x search seed, each search under its own world
//!   lifecycle (`world::WorldMode`), with the root's counts, seeded edge order
//!   and both execution picks (`frozen_choice` from the engine, `pick-v2` from
//!   the counts) in live menu order; without a frozen pick, the plain
//!   fallback's draw from p (its own seed).
//! - **Coverage:** every legal action on a menu of at most `coverage_max`;
//!   otherwise the arms' supported actions (n >= 8), both picks and fallback
//!   draws, plus the
//!   top `coverage_max` actions by p.
//! - **References:** Q_a and Q_b per covered action and V_p's two halves,
//!   `half_rollouts` continuations each, in the section 3.2 chance order:
//!   clone the pre-action session (the realized state, never
//!   redeterminized), install fresh future randomness, execute the root
//!   action (V_p: p's own draw), continue with SELF to the end. Each rollout
//!   has its own policy and future-randomness seeds; the halves differ in
//!   both. Natural wins, natural draws and nonnatural endings are recorded
//!   separately; the estimand (nonnatural = 1/2) and its worst cases belong to
//!   the analyzer.

use super::*;

/// SD1's per-rollout seed words: SHA256(json([tag, run, update, seed, slot,
/// half, action, r])); word 0 seeds both policy streams, word 1 the future
/// engine randomness, word 2 V_p's root draw.
#[allow(clippy::too_many_arguments, reason = "one seed tuple")]
pub(super) fn sd1_rollout_seeds_v1(
    tag: &str,
    run: &str,
    update: u64,
    seed: u64,
    slot: u64,
    half: &str,
    action: Option<usize>,
    r: u64,
) -> [u64; 3] {
    let text = json!([tag, run, update, seed, slot, half, action, r]).to_string();
    let digest = sha2::Sha256::digest(text.as_bytes());
    let word = |i: usize| {
        let mut b = [0_u8; 8];
        b.copy_from_slice(&digest[8 * i..8 * i + 8]);
        u64::from_le_bytes(b)
    };
    [word(0), word(1), word(2)]
}

/// Outcome codes of one continuation.
pub(super) const SD1_LOSS: usize = 0;
pub(super) const SD1_WIN: usize = 1;
pub(super) const SD1_DRAW: usize = 2;
pub(super) const SD1_NONNATURAL: usize = 3;

/// One reference continuation in the section 3.2 chance order.
fn sd1_continuation_v1(
    session: &FastActorSessionV1,
    d: &FastActorDecisionV1,
    rollout_policy: &mut FrozenPlayPolicyV1,
    focal: u8,
    action: usize,
    policy_seed: u64,
    future: u64,
) -> Result<usize, String> {
    let mut s = session.kernel_clone_with_future_randomness_v1(future);
    s.step(d.episode_id, d.step, action as u32).map_err(err)?;
    rollout_policy.reset_sampling_v1(crate::paired_bo1_harness_v1::paired_policy_seeds_v1(
        policy_seed,
    ));
    loop {
        match s.current_response() {
            FastActorResponseV1::Terminal(t) => {
                return Ok(
                    if t.terminal_classification != TerminalClassificationV1::Natural {
                        SD1_NONNATURAL
                    } else {
                        match t.winner {
                            Some(winner) if seat(winner) == focal => SD1_WIN,
                            Some(_) => SD1_LOSS,
                            None => SD1_DRAW,
                        }
                    },
                );
            }
            FastActorResponseV1::Decision(next) => {
                let a = rollout_policy.select_fast_session_v1(&s)?;
                s.step(next.episode_id, next.step, a).map_err(err)?;
            }
        }
    }
}

/// `pick-v2` on one tree's root (W1's execution rule, `serve.rs`
/// `pick_v2`): most visits, then 2 wins + draws (value units in a value-leaf
/// tree), then the lowest live index.
pub(super) fn sd1_pick_v2(
    n: &[u64],
    wins: &[u64],
    draws: &[u64],
    units: Option<&[u64]>,
) -> Option<usize> {
    let best_n = *n.iter().max()?;
    let score = |i: usize| match units {
        Some(u) => u128::from(u[i]),
        None => 2 * u128::from(wins[i]) + u128::from(draws[i]),
    };
    (0..n.len())
        .filter(|&i| n[i] == best_n)
        .fold(None, |best: Option<usize>, i| match best {
            Some(b) if score(b) >= score(i) => Some(b),
            _ => Some(i),
        })
}

/// p's draw from a uniform in [0, 1): inverse CDF in live order; the last
/// action takes any rounding remainder.
pub(super) fn sd1_draw_v1(p: &[f64], u: f64) -> usize {
    let mut acc = 0.0;
    for (a, &pa) in p.iter().enumerate() {
        acc += pa;
        if u < acc {
            return a;
        }
    }
    p.len() - 1
}

/// The coverage set (section 3.1), sorted.
pub(super) fn sd1_coverage_v1(
    width: usize,
    p: &[f64],
    from_arms: &std::collections::BTreeSet<usize>,
    coverage_max: usize,
) -> (Vec<usize>, bool) {
    if width <= coverage_max {
        return ((0..width).collect(), true);
    }
    let mut order: Vec<usize> = (0..width).collect();
    order.sort_by(|&a, &b| p[b].total_cmp(&p[a]).then(a.cmp(&b)));
    let mut covered = from_arms.clone();
    covered.extend(order.into_iter().take(coverage_max));
    (covered.into_iter().collect(), false)
}

impl Ex1EpisodeHookV1 {
    pub(super) fn capture_sd1_v1(
        &mut self,
        session: &FastActorSessionV1,
        d: FastActorDecisionV1,
        logit_bits: &[u32],
        hash: u64,
    ) -> Result<Value, String> {
        use crate::regret_census_v1::stage4a::value_leaf::Horizon;
        use crate::regret_census_v1::stage4a::world::WorldMode;
        let (capture, rollout_policy) = self.capture.as_mut().expect("capture mode");
        let sd1 = capture.sd1.clone().expect("sd1 capture");
        let slot = self.w5.as_ref().map_or(0, |w5| w5.slot);
        let width = d.legal_action_count as usize;
        let z: Vec<f64> = logit_bits
            .iter()
            .map(|&b| f64::from(f32::from_bits(b)))
            .collect();
        let p = line_b_softmax_v1(&z)
            .map_err(|e| format!("sd1 capture: {e}"))?
            .probabilities;
        if p.len() != width {
            return Err("sd1 capture: logits and the live menu differ in width".into());
        }
        let focal = self.learner_seat;
        let features = {
            let state = session.game_state();
            let (me, them) = (usize::from(focal), 1 - usize::from(focal));
            let battlefield = |who: usize| {
                state
                    .objects
                    .iter()
                    .filter(|(_, o)| {
                        o.zone == crate::state::Zone::Battlefield && o.controller.index() == who
                    })
                    .count()
            };
            // Actor-visible scalars only (SD2 section 7's inputs); nothing
            // here reads them.
            json!({"turn":state.turn,"step":format!("{:?}", state.step),
                "learner_active":state.active_player.index() == me,
                "life":[state.players[me].life, state.players[them].life],
                "hand":[state.players[me].hand.len(), state.players[them].hand.len()],
                "library":[state.players[me].library.len(), state.players[them].library.len()],
                "graveyard":[state.players[me].graveyard.len(), state.players[them].graveyard.len()],
                "battlefield":[battlefield(me), battlefield(them)]})
        };
        let mut arms_out = Vec::new();
        let mut from_arms = std::collections::BTreeSet::new();
        for arm in &sd1.arms {
            let world = WorldMode::parse(&arm.world)?;
            let vl = match &arm.value_leaf {
                Some(v) => Some((Horizon::parse(&v.horizon)?, v.sims)),
                None => None,
            };
            for &search_seed in &arm.search_seeds {
                // Rollout arms at one budget and search seed share their seed
                // root across world modes (paired simulation seeds);
                // value-leaf arms have their own.
                let seed_root = match &arm.value_leaf {
                    None => format!("sd1/{hash:016x}/{}/b{}/s{search_seed}", d.step, arm.budget),
                    Some(v) => format!(
                        "sd1/{hash:016x}/{}/vl/{}/n{}/s{search_seed}",
                        d.step, v.horizon, v.sims
                    ),
                };
                let started = crate::regret_census_v1::stage4a::process_cpu::seconds()?;
                let (record, root) = self.searcher.sd1_search_v1(
                    session,
                    arm.budget,
                    vl,
                    world,
                    seed_root.clone(),
                )?;
                let cpu_s = crate::regret_census_v1::stage4a::process_cpu::seconds()? - started;
                self.searcher.clear();
                let mut row = json!({"arm":arm.label,"world":arm.world,"search_seed":search_seed,
                    "seed_root":seed_root,"status":if root.is_some() {"ok"} else {"failed"},
                    "cpu_s":cpu_s,"search":record});
                let frozen = root.as_ref().and_then(|r| r.frozen_live);
                if frozen.is_none() {
                    // The executor's plain fallback plays p's draw (no
                    // supported edge, or a failed search): drawn here from
                    // the arm's own seed and covered, so G^pick evaluates it.
                    let [u, _, _] = sd1_rollout_seeds_v1(
                        "sd1/fallback",
                        &self.options.run,
                        self.options.update,
                        self.seed,
                        slot,
                        &arm.label,
                        Some(search_seed as usize),
                        0,
                    );
                    let fallback = sd1_draw_v1(&p, (u >> 11) as f64 * (1.0 / (1_u64 << 53) as f64));
                    from_arms.insert(fallback);
                    row["fallback"] = json!(fallback);
                }
                if let Some(r) = root {
                    for (a, &count) in r.n.iter().enumerate() {
                        if count >= crate::ex1_v1::EX1_SUPPORT_MIN_V1 {
                            from_arms.insert(a);
                        }
                    }
                    let pick_v2 = sd1_pick_v2(&r.n, &r.wins, &r.draws, r.units.as_deref());
                    from_arms.extend(r.frozen_live.map(|a| a as usize));
                    from_arms.extend(pick_v2);
                    row["visits"] = json!(r.visits);
                    row["n"] = json!(r.n);
                    row["wins"] = json!(r.wins);
                    row["draws"] = json!(r.draws);
                    if let Some(u) = &r.units {
                        row["value_units"] = json!(u);
                    }
                    row["perm_live"] = json!(r.perm_live);
                    row["frozen_choice"] = json!(r.frozen_live);
                    row["pick_v2"] = json!(pick_v2);
                }
                arms_out.push(row);
            }
        }
        let (covered, full) = sd1_coverage_v1(width, &p, &from_arms, sd1.coverage_max as usize);
        let mut out = json!({"reached":true,"schema":SD1_CAPTURE_VERSION_V1,"step":d.step,
            "physical_decision_id":d.physical_decision_id,"width":width,
            "policy":{"logit_bits":logit_bits},"features":features,"arms":arms_out});
        if sd1.half_rollouts == 0 {
            return Ok(out);
        }
        let (run, update, seed) = (self.options.run.clone(), self.options.update, self.seed);
        let rollouts = sd1.half_rollouts;
        let mut q = serde_json::Map::new();
        for half in ["a", "b"] {
            let (mut wins, mut draws, mut nonnatural) = (vec![], vec![], vec![]);
            for &action in &covered {
                let mut c = [0_u64; 4];
                for r in 0..rollouts {
                    let [policy_seed, future, _] = sd1_rollout_seeds_v1(
                        "sd1/qref",
                        &run,
                        update,
                        seed,
                        slot,
                        half,
                        Some(action),
                        r,
                    );
                    let o = sd1_continuation_v1(
                        session,
                        &d,
                        rollout_policy,
                        focal,
                        action,
                        policy_seed,
                        future,
                    )?;
                    c[o] += 1;
                }
                wins.push(c[SD1_WIN]);
                draws.push(c[SD1_DRAW]);
                nonnatural.push(c[SD1_NONNATURAL]);
            }
            q.insert(
                half.to_string(),
                json!({"wins":wins,"draws":draws,"nonnatural":nonnatural}),
            );
        }
        // V_p: after the fresh randomness, the root action is p's draw at the
        // top 53 bits of the rollout's own seed word, then as above.
        let mut vp = serde_json::Map::new();
        for half in ["a", "b"] {
            let mut c = [0_u64; 4];
            let mut drawn = vec![0_u64; width];
            for r in 0..rollouts {
                let [policy_seed, future, draw] =
                    sd1_rollout_seeds_v1("sd1/vp", &run, update, seed, slot, half, None, r);
                let action = sd1_draw_v1(&p, (draw >> 11) as f64 * (1.0 / (1_u64 << 53) as f64));
                drawn[action] += 1;
                let o = sd1_continuation_v1(
                    session,
                    &d,
                    rollout_policy,
                    focal,
                    action,
                    policy_seed,
                    future,
                )?;
                c[o] += 1;
            }
            vp.insert(
                half.to_string(),
                json!({"wins":c[SD1_WIN],"draws":c[SD1_DRAW],"nonnatural":c[SD1_NONNATURAL],
                    "drawn":drawn}),
            );
        }
        out["references"] = json!({"half_rollouts":rollouts,"coverage_max":sd1.coverage_max,
            "full_coverage":full,"actions":covered,
            "seed_tags":{"q":"sd1/qref","vp":"sd1/vp"},"q":q,"vp":vp});
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_v2_matches_the_w1_rule() {
        // Most visits wins outright.
        assert_eq!(
            sd1_pick_v2(&[3, 9, 4], &[3, 1, 4], &[0, 0, 0], None),
            Some(1)
        );
        // Equal visits: higher 2w + d, then the lowest index.
        assert_eq!(
            sd1_pick_v2(&[9, 9, 4], &[3, 5, 4], &[2, 0, 0], None),
            Some(1)
        );
        assert_eq!(
            sd1_pick_v2(&[9, 9, 4], &[4, 3, 4], &[0, 2, 0], None),
            Some(0)
        );
        // Value-leaf trees compare value units.
        assert_eq!(
            sd1_pick_v2(&[9, 9], &[0, 0], &[0, 0], Some(&[5, 7])),
            Some(1)
        );
        assert_eq!(sd1_pick_v2(&[], &[], &[], None), None);
    }

    #[test]
    fn draw_is_the_inverse_cdf() {
        let p = [0.25, 0.5, 0.25];
        assert_eq!(sd1_draw_v1(&p, 0.0), 0);
        assert_eq!(sd1_draw_v1(&p, 0.2499), 0);
        assert_eq!(sd1_draw_v1(&p, 0.25), 1);
        assert_eq!(sd1_draw_v1(&p, 0.7499), 1);
        assert_eq!(sd1_draw_v1(&p, 0.9999999), 2);
    }

    #[test]
    fn coverage_is_full_on_small_menus_and_top_k_plus_arms_otherwise() {
        let p = [0.1, 0.4, 0.2, 0.3];
        let arms: std::collections::BTreeSet<usize> = [0].into_iter().collect();
        assert_eq!(sd1_coverage_v1(4, &p, &arms, 4), (vec![0, 1, 2, 3], true));
        assert_eq!(sd1_coverage_v1(4, &p, &arms, 2), (vec![0, 1, 3], false));
        // Ties in p go to the lower index.
        let flat = [0.25; 4];
        assert_eq!(
            sd1_coverage_v1(4, &flat, &Default::default(), 2),
            (vec![0, 1], false)
        );
    }

    #[test]
    fn rollout_seeds_differ_by_half_action_and_tag() {
        let a = sd1_rollout_seeds_v1("sd1/qref", "sd1/panel", 0, 7, 1, "a", Some(2), 0);
        let b = sd1_rollout_seeds_v1("sd1/qref", "sd1/panel", 0, 7, 1, "b", Some(2), 0);
        let c = sd1_rollout_seeds_v1("sd1/qref", "sd1/panel", 0, 7, 1, "a", Some(3), 0);
        let v = sd1_rollout_seeds_v1("sd1/vp", "sd1/panel", 0, 7, 1, "a", None, 0);
        assert!(a != b && a != c && a != v && a[0] != a[1] && a[1] != a[2]);
    }
}
