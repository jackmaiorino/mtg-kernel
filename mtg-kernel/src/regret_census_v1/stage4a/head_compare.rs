//! The paired head comparison (collab
//! LANES/lead/reviews/search-screen-20261125/DECLARATION-draft2.md section 5;
//! the screen's T5). Measurement only: nothing here is read by a search, the
//! episode or any other capture field.
//!
//! At a placed decision, for each covered root action and each world w in
//! 0..W: draw a lawful sample of the root player's information set with the
//! search's sampler (`world::sample`, `FutureChanceV3`, the searcher's deck
//! prior) from a seed keyed by (`ss1-hc`, seed, state hash, step, live action,
//! w, attempt); bind the action's canonical bytes to the sampled world's live
//! menu as the search does; apply it; then both seats play the frozen policy
//! (forced choices take their only action without a draw, as in a search
//! rollout) until a natural end or the h0 point: the first decision of the
//! root player with two or more legal actions after the root action, the
//! FV-h0 leaf (`value_leaf.rs`, H0 under the flat root). The observation there
//! is encoded once by each head's own encoder, and each head scores it; the
//! two encodings' digests are recorded and compared, a mismatch is recorded
//! and never repaired.
//!
//! Scoring (section 5): a world that ends before h0 scores the terminal
//! result for the root player (win 1, draw or nonnatural 1/2, loss 0), the
//! same for both heads; otherwise a head's raw value v becomes
//! u = clamp((v + 1) / 2, 0, 1), scored u when the root player observes at
//! h0 and 1 - u otherwise; both kinds of world contribute (an opponent
//! observer is counted in `checks`, and under this h0 rule never occurs). A
//! head's estimate for an action is the mean over its W worlds; it is null
//! when any world was not scored cleanly (no lawful sample, a fault or a
//! digest mismatch), with the reasons counted. The argmax over covered actions breaks
//! ties by the lowest covered index and is null when any estimate is.

use super::play::{acting, decision, terminal, End};
use super::tree::{canon, live_index};
use super::world::DeckPrior;
use crate::ids::PlayerId;
use crate::rl_session::FastActorSessionV1;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use serde_json::{json, Value};
use sha2::Digest;

/// The record schema of `head_compare`.
pub(crate) const HEAD_COMPARE_SCHEMA_V1: &str = "ss1-head-compare-v1";
/// The first element of every seed tuple.
pub(crate) const HEAD_COMPARE_SEED_TAG_V1: &str = "ss1-hc";
/// Lawful-sample attempts per world (the search's rejection limit).
pub(crate) const HEAD_COMPARE_ATTEMPTS_V1: u64 = super::arms::MAX_CONSECUTIVE_REJECTIONS;
/// Engine transitions allowed between the root action and h0.
pub(crate) const HEAD_COMPARE_TRANSITION_CAP_V1: u64 = 1_000_000;

/// What keys one state's worlds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HeadCompareKeyV1 {
    pub(crate) worlds: u64,
    pub(crate) seed: u64,
    /// The placement hash of the state.
    pub(crate) hash: u64,
    pub(crate) step: u64,
}

/// SHA256(json([tag, seed, hash hex, step, action, w, attempt])): word 0 is
/// the world sample's seed, word 1 the policy seed of its continuation.
pub(crate) fn head_compare_seeds_v1(
    key: &HeadCompareKeyV1,
    action: usize,
    w: u64,
    attempt: u64,
) -> [u64; 2] {
    let text = json!([
        HEAD_COMPARE_SEED_TAG_V1,
        key.seed,
        format!("{:016x}", key.hash),
        key.step,
        action,
        w,
        attempt
    ])
    .to_string();
    let digest = sha2::Sha256::digest(text.as_bytes());
    let word = |i: usize| u64::from_le_bytes(digest[8 * i..8 * i + 8].try_into().expect("8 bytes"));
    [word(0), word(1)]
}

/// The terminal score for the root player (nonnatural counts 1/2).
pub(crate) fn terminal_score_v1(end: End) -> f64 {
    match end {
        End::Natural { win: true, .. } => 1.0,
        End::Natural { draw: true, .. } | End::NonNatural => 0.5,
        End::Natural { .. } => 0.0,
    }
}

fn terminal_label(end: End) -> &'static str {
    match end {
        End::Natural { win: true, .. } => "win",
        End::Natural { draw: true, .. } => "draw",
        End::NonNatural => "nonnatural",
        End::Natural { .. } => "loss",
    }
}

/// A head's score at h0 for the root player: u = clamp((v + 1) / 2, 0, 1)
/// when the root player observes, 1 - u otherwise.
pub(crate) fn head_score_v1(raw: f32, observer_is_root: bool) -> f64 {
    let u = ((f64::from(raw) + 1.0) / 2.0).clamp(0.0, 1.0);
    if observer_is_root {
        u
    } else {
        1.0 - u
    }
}

/// How a continuation from the root action ended.
pub(crate) enum ToH0V1 {
    /// The game ended before h0 (the outcome for the root player).
    Terminal(End),
    /// The h0 point: the root player's first choice with two or more legal
    /// actions.
    H0(Box<FastActorSessionV1>),
}

/// Plays both seats with `play` from `s` (after the root action) to a
/// natural end or h0. Returns the end and the transitions taken.
pub(crate) fn play_to_h0_v1(
    mut s: FastActorSessionV1,
    root: PlayerId,
    play: &mut FrozenPlayPolicyV1,
) -> Result<(ToH0V1, u64), String> {
    let mut transitions = 0;
    loop {
        if let Some(end) = terminal(&s, root) {
            return Ok((ToH0V1::Terminal(end), transitions));
        }
        let d = decision(&s).ok_or("a non-terminal response without a decision")?;
        if acting(&d) == root && d.legal_action_count >= 2 {
            return Ok((ToH0V1::H0(Box::new(s)), transitions));
        }
        if transitions >= HEAD_COMPARE_TRANSITION_CAP_V1 {
            return Err(format!(
                "no h0 or end within {HEAD_COMPARE_TRANSITION_CAP_V1} transitions"
            ));
        }
        let a = if d.legal_action_count < 2 {
            0
        } else {
            play.select_fast_session_v1(&s)?
        };
        s.step(d.episode_id, d.step, a)
            .map_err(|e| format!("step {} action {a}: {e:?}", d.step))?;
        transitions += 1;
    }
}

/// One head's raw value and observation digest at `s`. The head's encoder
/// starts fresh for every call (its sampling streams are never read), so a
/// score does not depend on what the head scored before.
pub(crate) fn score_head_v1(
    head: &mut FrozenPlayPolicyV1,
    s: &FastActorSessionV1,
) -> Result<(f32, String), String> {
    head.reset_sampling_v1([0, 0]);
    let value = head.score_fast_session_v1(s)?.value;
    if !value.is_finite() {
        return Err(format!("non-finite value {value}"));
    }
    Ok((value, head.last_observation_digest_v1()?))
}

fn head_json(r: &Result<(f32, String), String>) -> Value {
    match r {
        Ok((v, digest)) => json!({"value":f64::from(*v),"digest":digest}),
        Err(e) => json!({"fault":e}),
    }
}

/// The heads at one observation: their records, both values when both
/// scored, and whether the digests agree.
fn both_heads(
    a: &mut FrozenPlayPolicyV1,
    b: &mut FrozenPlayPolicyV1,
    s: &FastActorSessionV1,
) -> (Value, Option<(f32, f32)>, bool) {
    let ra = score_head_v1(a, s);
    let rb = score_head_v1(b, s);
    let equal = matches!((&ra, &rb), (Ok((_, x)), Ok((_, y))) if x == y);
    let values = match (&ra, &rb) {
        (Ok((x, _)), Ok((y, _))) => Some((*x, *y)),
        _ => None,
    };
    (
        json!({"a":head_json(&ra),"b":head_json(&rb),"digests_equal":equal}),
        values,
        equal,
    )
}

/// One world: its record and, when it scored cleanly, (score a, score b);
/// otherwise the exclusion reason.
#[allow(clippy::too_many_arguments, reason = "one world's inputs")]
fn one_world(
    root: &FastActorSessionV1,
    prior: &DeckPrior,
    focal: PlayerId,
    edge: &[u8],
    key: &HeadCompareKeyV1,
    action: usize,
    w: u64,
    play: &mut FrozenPlayPolicyV1,
    a: &mut FrozenPlayPolicyV1,
    b: &mut FrozenPlayPolicyV1,
) -> (Value, Result<(f64, f64), &'static str>) {
    let mut sampled = None;
    let mut attempts = 0;
    for attempt in 0..HEAD_COMPARE_ATTEMPTS_V1 {
        attempts = attempt + 1;
        let [world_seed, policy_seed] = head_compare_seeds_v1(key, action, w, attempt);
        if let Ok(x) = super::world::sample(root, world_seed, prior) {
            sampled = Some((x.world, policy_seed));
            break;
        }
    }
    let mut record = json!({"w":w,"attempts":attempts});
    let Some((mut world, policy_seed)) = sampled else {
        record["end"] = json!("no_world");
        return (record, Err("no_world"));
    };
    let fault = |mut record: Value, e: String| {
        record["end"] = json!("fault");
        record["fault"] = json!(e);
        (record, Err("fault"))
    };
    // The root action in the sampled world: its canonical bytes bound to
    // this world's live menu, as a search executes a root edge.
    let live = match decision(&world)
        .ok_or_else(|| "the sampled world has no decision".to_string())
        .and_then(|d| {
            if acting(&d) != focal {
                return Err("the sampled world's actor is not the root player".into());
            }
            let cn = canon(&world, d).map_err(|e| format!("{e:?}"))?;
            let live = live_index(&cn, edge).map_err(|e| format!("{e:?}"))?;
            world
                .step(d.episode_id, d.step, live)
                .map_err(|e| format!("root action {live}: {e:?}"))?;
            Ok(live)
        }) {
        Ok(live) => live,
        Err(e) => return fault(record, e),
    };
    record["live"] = json!(live);
    play.reset_sampling_v1(crate::paired_bo1_harness_v1::paired_policy_seeds_v1(
        policy_seed,
    ));
    let (end, transitions) = match play_to_h0_v1(world, focal, play) {
        Ok(x) => x,
        Err(e) => return fault(record, e),
    };
    record["transitions"] = json!(transitions);
    match end {
        ToH0V1::Terminal(end) => {
            let score = terminal_score_v1(end);
            record["end"] = json!("terminal");
            record["result"] = json!(terminal_label(end));
            record["score_a"] = json!(score);
            record["score_b"] = json!(score);
            (record, Ok((score, score)))
        }
        ToH0V1::H0(s) => {
            let d = decision(&s).expect("h0 is a decision");
            let observer = acting(&d);
            let observer_is_root = observer == focal;
            let (heads, values, equal) = both_heads(a, b, &s);
            record["end"] = json!("h0");
            record["turn"] = json!(s.game_state().turn);
            record["observer"] = json!(observer.0);
            record["observer_is_root"] = json!(observer_is_root);
            record["heads"] = heads;
            let Some((va, vb)) = values else {
                record["fault"] = json!("a head did not score");
                return (record, Err("fault"));
            };
            let (sa, sb) = (
                head_score_v1(va, observer_is_root),
                head_score_v1(vb, observer_is_root),
            );
            record["score_a"] = json!(sa);
            record["score_b"] = json!(sb);
            if !equal {
                (record, Err("digest_mismatch"))
            } else {
                (record, Ok((sa, sb)))
            }
        }
    }
}

/// The highest estimate in covered order (ties to the first), or null when
/// any covered action has none.
fn argmax(covered: &[usize], estimates: &[Option<f64>]) -> Value {
    let mut best: Option<(usize, f64)> = None;
    for (&action, e) in covered.iter().zip(estimates) {
        let Some(e) = *e else {
            return Value::Null;
        };
        if best.is_none_or(|(_, b)| e > b) {
            best = Some((action, e));
        }
    }
    best.map_or(Value::Null, |(a, _)| json!(a))
}

/// The comparison at the placed decision `root` (the root player `focal`
/// acting) over the covered live actions `covered` (sorted): every world of
/// every action, both heads at the realized root, the estimates and argmaxes.
/// `play` plays both seats; `a` and `b` are the two heads.
#[allow(clippy::too_many_arguments, reason = "explicit roles")]
pub(crate) fn head_compare_v1(
    root: &FastActorSessionV1,
    prior: &DeckPrior,
    focal: PlayerId,
    covered: &[usize],
    full_coverage: bool,
    key: &HeadCompareKeyV1,
    play: &mut FrozenPlayPolicyV1,
    a: &mut FrozenPlayPolicyV1,
    b: &mut FrozenPlayPolicyV1,
) -> Value {
    let mut out = json!({"schema":HEAD_COMPARE_SCHEMA_V1,"seed_tag":HEAD_COMPARE_SEED_TAG_V1,
        "worlds":key.worlds,"seed":key.seed,"actions":covered,"full_coverage":full_coverage,
        "h0":"the root player's first choice with two or more legal actions after the root action",
        "scoring":"terminal before h0: win 1, draw or nonnatural 1/2, loss 0 (both heads); \
            at h0: u = clamp((v+1)/2, 0, 1), 1 - u if the observer is not the root player \
            (both contribute); estimate: mean over the worlds, null if any world had no \
            lawful sample, a fault or a digest mismatch"});
    let Some(d) = decision(root).filter(|d| acting(d) == focal) else {
        out["status"] = json!("fault");
        out["reason"] = json!("the root is not a decision of the root player");
        return out;
    };
    let cn = match canon(root, d) {
        Ok(cn) => cn,
        Err(e) => {
            out["status"] = json!("fault");
            out["reason"] = json!(format!("root canon: {e:?}"));
            return out;
        }
    };
    // H3: both heads at the realized root.
    let (root_heads, _, _) = both_heads(a, b, root);
    let mut checks = std::collections::BTreeMap::<&str, u64>::new();
    for k in [
        "h0",
        "terminal_before_h0",
        "no_world",
        "fault",
        "digest_mismatch",
        "observer_not_root",
    ] {
        checks.insert(k, 0);
    }
    let mut per_action = Vec::new();
    let (mut est_a, mut est_b) = (Vec::new(), Vec::new());
    for &action in covered {
        let mut worlds = Vec::new();
        let mut excluded = std::collections::BTreeMap::<&str, u64>::new();
        let (mut sum_a, mut sum_b) = (0.0_f64, 0.0_f64);
        let Some(edge) = cn.menu.get(action) else {
            excluded.insert("fault", key.worlds);
            *checks.get_mut("fault").expect("listed") += key.worlds;
            per_action.push(json!({"action":action,"fault":"outside the live menu",
                "worlds":[],"estimate_a":null,"estimate_b":null,"excluded":excluded}));
            est_a.push(None);
            est_b.push(None);
            continue;
        };
        for w in 0..key.worlds {
            let (record, scored) = one_world(root, prior, focal, edge, key, action, w, play, a, b);
            if record["observer_is_root"] == false {
                *checks.get_mut("observer_not_root").expect("listed") += 1;
            }
            match record["end"].as_str() {
                Some("h0") => *checks.get_mut("h0").expect("listed") += 1,
                Some("terminal") => *checks.get_mut("terminal_before_h0").expect("listed") += 1,
                _ => {}
            }
            match scored {
                Ok((x, y)) => {
                    sum_a += x;
                    sum_b += y;
                }
                Err(reason) => {
                    *excluded.entry(reason).or_default() += 1;
                    *checks.get_mut(reason).expect("listed") += 1;
                }
            }
            worlds.push(record);
        }
        let clean = excluded.is_empty() && key.worlds > 0;
        let n = key.worlds as f64;
        let (ea, eb) = if clean {
            (Some(sum_a / n), Some(sum_b / n))
        } else {
            (None, None)
        };
        est_a.push(ea);
        est_b.push(eb);
        per_action.push(json!({"action":action,"worlds":worlds,"estimate_a":ea,
            "estimate_b":eb,"excluded":excluded}));
    }
    out["status"] = json!("ok");
    out["root"] = root_heads;
    out["per_action"] = json!(per_action);
    out["argmax_a"] = argmax(covered, &est_a);
    out["argmax_b"] = argmax(covered, &est_b);
    out["checks"] = json!(checks);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_follow_section_5() {
        assert_eq!(head_score_v1(0.0, true), 0.5);
        assert_eq!(head_score_v1(0.5, true), 0.75);
        // The opponent observing: the root player's score is 1 - u.
        assert_eq!(head_score_v1(0.5, false), 0.25);
        // Clamped to [0, 1].
        assert_eq!(head_score_v1(3.0, true), 1.0);
        assert_eq!(head_score_v1(-3.0, true), 0.0);
        assert_eq!(head_score_v1(-3.0, false), 1.0);
        let win = End::Natural {
            win: true,
            draw: false,
        };
        let draw = End::Natural {
            win: false,
            draw: true,
        };
        let loss = End::Natural {
            win: false,
            draw: false,
        };
        assert_eq!(terminal_score_v1(win), 1.0);
        assert_eq!(terminal_score_v1(draw), 0.5);
        assert_eq!(terminal_score_v1(End::NonNatural), 0.5);
        assert_eq!(terminal_score_v1(loss), 0.0);
    }

    #[test]
    fn argmax_breaks_ties_low_and_needs_every_estimate() {
        let covered = [2, 5, 7];
        assert_eq!(
            argmax(&covered, &[Some(0.4), Some(0.6), Some(0.6)]),
            json!(5)
        );
        assert_eq!(
            argmax(&covered, &[Some(0.6), Some(0.6), Some(0.1)]),
            json!(2)
        );
        assert_eq!(argmax(&covered, &[Some(0.6), None, Some(0.1)]), Value::Null);
        assert_eq!(argmax(&[], &[]), Value::Null);
    }

    #[test]
    fn seeds_differ_by_every_element() {
        let key = HeadCompareKeyV1 {
            worlds: 16,
            seed: 20_261_125,
            hash: 7,
            step: 3,
        };
        let base = head_compare_seeds_v1(&key, 1, 0, 0);
        assert_eq!(base, head_compare_seeds_v1(&key, 1, 0, 0));
        assert_ne!(base[0], base[1]);
        for other in [
            head_compare_seeds_v1(&key, 2, 0, 0),
            head_compare_seeds_v1(&key, 1, 1, 0),
            head_compare_seeds_v1(&key, 1, 0, 1),
            head_compare_seeds_v1(&HeadCompareKeyV1 { hash: 8, ..key }, 1, 0, 0),
            head_compare_seeds_v1(&HeadCompareKeyV1 { step: 4, ..key }, 1, 0, 0),
            head_compare_seeds_v1(&HeadCompareKeyV1 { seed: 1, ..key }, 1, 0, 0),
        ] {
            assert_ne!(base, other);
        }
    }
}
