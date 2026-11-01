//! Engine ceiling probe for the GPU design note (measurement only): games and
//! decisions per second on a run config's real episodes, per thread and
//! across threads, for three per-decision workloads:
//!
//! - `uniform`: the engine and its decision surface only (a seeded uniform
//!   choice among the legal actions);
//! - `policy`: the net-8 fixture policy's V3 tensorization (with the 96
//!   state-digest features), CPU forward and sampling, as collection does
//!   minus records and publication;
//! - `policy-no-digest`: the same under the no-state-digest input contract.
//!
//! Run (one claim of the cores measured):
//!   MTG_KERNEL_CEILING_CONFIG=<run config.json> MTG_KERNEL_CEILING_THREADS=1,6
//!   cargo test --release --lib engine_ceiling_probe -- --ignored --nocapture

use super::*;
use crate::state::SplitMix64;
use std::time::Instant;

fn episodes_from_config() -> Vec<ExpandedEpisodeV1> {
    let path = std::env::var("MTG_KERNEL_CEILING_CONFIG").expect("set MTG_KERNEL_CEILING_CONFIG");
    let config: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    config["iterations"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|iteration| iteration["episodes"].as_array().unwrap().iter())
        .map(|scheduled| serde_json::from_value(scheduled["episode"].clone()).unwrap())
        .collect()
}

#[derive(Clone, Copy, PartialEq)]
enum Workload {
    Uniform,
    Policy,
    PolicyNoDigest,
}

/// Plays `episode` to its terminal; returns the decision count, or None when
/// the fixture policy cannot encode a decision of this deck pool (the game is
/// then left out of the rates and counted).
fn play(
    episode: &ExpandedEpisodeV1,
    workload: Workload,
    policy: &mut FrozenPlayPolicyV1,
) -> Option<u64> {
    let configs = episode.configurations().unwrap();
    let mut session = episode_session_v1(episode, &configs).unwrap();
    restore_reset_cut_v1(&mut session, episode).unwrap();
    policy.reset_sampling_v1(paired_policy_seeds_v1(episode.seed));
    let mut rng = SplitMix64::seed(episode.seed);
    let mut decisions = 0u64;
    while let FastActorResponseV1::Decision(d) = session.current_response() {
        let selected = match workload {
            Workload::Uniform => (rng.next_u64() % u64::from(d.legal_action_count)) as u32,
            Workload::Policy | Workload::PolicyNoDigest => {
                policy.select_with_training_tensor_v3(&session).ok()?.0
            }
        };
        session.step(d.episode_id, d.step, selected).unwrap();
        decisions += 1;
        assert!(decisions < 1_000_000, "decision cap");
    }
    Some(decisions)
}

#[test]
#[ignore = "timing only: set MTG_KERNEL_CEILING_CONFIG"]
fn engine_ceiling_probe_v1() {
    let episodes = episodes_from_config();
    let threads: Vec<usize> = std::env::var("MTG_KERNEL_CEILING_THREADS")
        .unwrap_or_else(|_| "1".into())
        .split(',')
        .map(|t| t.parse().unwrap())
        .collect();
    let games: usize =
        std::env::var("MTG_KERNEL_CEILING_GAMES_PER_THREAD").map_or(20, |g| g.parse().unwrap());
    let mut template = FrozenPlayPolicyV1::training_fixture_v3();
    let mut no_digest = FrozenPlayPolicyV1::training_fixture_v3();
    no_digest
        .apply_no_state_digest_v1("converted".into(), 123..219, 1_499, false)
        .unwrap();
    for (name, workload) in [
        ("uniform", Workload::Uniform),
        ("policy", Workload::Policy),
        ("policy-no-digest", Workload::PolicyNoDigest),
    ] {
        for &count in &threads {
            let base = if workload == Workload::PolicyNoDigest {
                &mut no_digest
            } else {
                &mut template
            };
            let forks: Vec<FrozenPlayPolicyV1> = (0..count)
                .map(|_| base.fork_for_collection_v3().unwrap())
                .collect();
            let started = Instant::now();
            let (decisions, completed): (u64, u64) = std::thread::scope(|s| {
                let handles: Vec<_> = forks
                    .into_iter()
                    .enumerate()
                    .map(|(t, mut policy)| {
                        let episodes = &episodes;
                        s.spawn(move || {
                            (0..games)
                                .filter_map(|g| {
                                    let episode = &episodes[(t * games + g) % episodes.len()];
                                    play(episode, workload, &mut policy)
                                })
                                .fold((0u64, 0u64), |(d, n), x| (d + x, n + 1))
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|h| h.join().unwrap())
                    .fold((0, 0), |(d, n), (x, m)| (d + x, n + m))
            });
            let seconds = started.elapsed().as_secs_f64();
            // Skipped games ran partway, so the rates are slight overestimates.
            let skipped = (count * games) as u64 - completed;
            let total_games = completed as f64;
            println!(
                "{{\"probe\":\"engine_ceiling\",\"workload\":\"{name}\",\"threads\":{count},\"games\":{total_games},\"decisions\":{decisions},\"seconds\":{seconds:.3},\"games_per_s\":{:.2},\"decisions_per_s\":{:.0},\"games_per_s_per_thread\":{:.2},\"decisions_per_game\":{:.1},\"skipped_games\":{skipped}}}",
                total_games / seconds,
                decisions as f64 / seconds,
                total_games / seconds / count as f64,
                decisions as f64 / total_games
            );
        }
    }
    let _ = &mut template;
}

/// The fields of a `Debug` struct line (`Name { a: 1, b: X }`), top level only.
fn debug_fields(text: &str) -> Vec<(String, String)> {
    let inner = text
        .split_once('{')
        .map_or("", |(_, rest)| rest.trim_end().trim_end_matches('}'));
    let mut fields = Vec::new();
    let (mut depth, mut start) = (0i32, 0usize);
    let bytes = inner.as_bytes();
    for i in 0..=bytes.len() {
        let end = i == bytes.len();
        if !end {
            match bytes[i] {
                b'{' | b'(' | b'[' => depth += 1,
                b'}' | b')' | b']' => depth -= 1,
                _ => {}
            }
        }
        if end || (bytes[i] == b',' && depth == 0) {
            if let Some((name, value)) = inner[start..i].split_once(':') {
                fields.push((name.trim().to_owned(), value.trim().to_owned()));
            }
            start = i + 1;
        }
    }
    fields
}

/// Which action records differ while every explicit input the network sees
/// is equal (`digest_only_actions_probe_v1`): for actions `a` and `b`, the
/// core fields and reference fields that differ.
fn record_differences(
    actions: &[crate::flat_policy_v2::FlatScorerActionCoreV2],
    refs: &[crate::flat_policy_v2::FlatScorerActionRefV2],
    a: usize,
    b: usize,
) -> Vec<String> {
    let mut out = Vec::new();
    let (fa, fb) = (
        debug_fields(&format!("{:?}", actions[a])),
        debug_fields(&format!("{:?}", actions[b])),
    );
    for ((name, va), (_, vb)) in fa.iter().zip(&fb) {
        if va != vb && name != "ref_start" {
            out.push(format!("action.{name}"));
        }
    }
    let refs_of = |i: usize| {
        let start = actions[i].ref_start as usize;
        refs[start..start + actions[i].ref_len as usize].to_vec()
    };
    let (ra, rb) = (refs_of(a), refs_of(b));
    if ra.len() != rb.len() {
        out.push("refs.count".into());
    } else {
        for (x, y) in ra.iter().zip(&rb) {
            let (fx, fy) = (
                debug_fields(&format!("{x:?}")),
                debug_fields(&format!("{y:?}")),
            );
            for ((name, vx), (_, vy)) in fx.iter().zip(&fy) {
                if vx != vy && name != "action_index" {
                    out.push(format!("ref.{name}"));
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The 18% (lead, 2026-10-31): decisions whose legal actions include two or
/// more that are equal in every explicit input (action features without the
/// 96 digest columns, each reference's features and card, and each
/// referenced object's features and card) yet differ in the action digest.
/// Plays D1's episodes with the V4 fixture policy (the FlatV4/V7 inputs the
/// bignet store holds) and tallies, per decision kind and action kind, which
/// action-record fields the digest alone carries. With `s4a-profile-v1` it
/// also prints the tensorizer's time split and SHA-512 block counts.
#[test]
#[ignore = "measurement only: set MTG_KERNEL_CEILING_CONFIG"]
fn digest_only_actions_probe_v1() {
    use crate::native_flat_tensorizer_v2::NATIVE_FLAT_ACTION_HASH_FEATURE_DIM_V2 as HASH;
    use std::collections::BTreeMap;
    let episodes = probe_episodes_v1();
    let games: usize =
        std::env::var("MTG_KERNEL_CEILING_GAMES_PER_THREAD").map_or(40, |g| g.parse().unwrap());
    let mut policy = FrozenPlayPolicyV1::training_fixture_v4();
    let (mut decisions, mut flagged, mut skipped) = (0u64, 0u64, 0u64);
    let mut tally: BTreeMap<String, u64> = BTreeMap::new();
    let mut examples: BTreeMap<String, String> = BTreeMap::new();
    let mut stops: BTreeMap<String, (u64, Vec<String>)> = BTreeMap::new();
    let started = Instant::now();
    'games: for episode in episodes.iter().cycle().take(games) {
        let Ok(configs) = episode.configurations() else {
            skipped += 1;
            continue;
        };
        let Ok(mut session) = episode_session_v1(episode, &configs) else {
            skipped += 1;
            continue;
        };
        restore_reset_cut_v1(&mut session, episode).unwrap();
        policy.reset_sampling_v1(paired_policy_seeds_v1(episode.seed));
        while let FastActorResponseV1::Decision(d) = session.current_response() {
            let (selected, tensor) = match policy.select_with_training_tensor_v4(&session) {
                Ok((selected, _, tensor)) => (selected, tensor),
                Err(error) => {
                    // Where the encoder stopped: the error with its digits
                    // folded, the decision kind and the two decks.
                    let kind: String = error
                        .chars()
                        .map(|c| if c.is_ascii_digit() { '#' } else { c })
                        .take(240)
                        .collect();
                    let entry = stops
                        .entry(format!("{:?} | {kind}", d.decision_kind))
                        .or_insert((0u64, Vec::new()));
                    entry.0 += 1;
                    let decks = format!(
                        "{} vs {}",
                        episode.registered[0].label, episode.registered[1].label
                    );
                    if !entry.1.contains(&decks) {
                        entry.1.push(decks);
                    }
                    skipped += 1;
                    continue 'games;
                }
            };
            decisions += 1;
            let t = &tensor.common;
            let width = t.action_features.len() / d.legal_action_count as usize;
            let object_width = t.object_features.len() / t.object_card_ids.len().max(1);
            let ref_width = t.action_ref_features.len() / t.action_ref_action_indices.len().max(1);
            // Explicit signature per action: its features without the digest
            // tail, then each reference's features, card and object.
            let mut signatures: Vec<Vec<u32>> = (0..d.legal_action_count as usize)
                .map(|a| {
                    t.action_features[a * width..(a + 1) * width - HASH]
                        .iter()
                        .map(|v| v.to_bits())
                        .collect()
                })
                .collect();
            for (r, &action) in t.action_ref_action_indices.iter().enumerate() {
                let sig = &mut signatures[action as usize];
                sig.extend(
                    t.action_ref_features[r * ref_width..(r + 1) * ref_width]
                        .iter()
                        .map(|v| v.to_bits()),
                );
                sig.push(t.action_ref_card_ids[r] as u32);
                let node = t.action_ref_node_indices[r] as usize;
                sig.extend(
                    t.object_features[node * object_width..(node + 1) * object_width]
                        .iter()
                        .map(|v| v.to_bits()),
                );
                sig.push(t.object_card_ids[node] as u32);
            }
            let digest = |a: usize| {
                t.action_features[(a + 1) * width - HASH..(a + 1) * width]
                    .iter()
                    .map(|v| v.to_bits())
                    .collect::<Vec<u32>>()
            };
            let (actions, refs) = policy.last_scored_actions_for_test_v1();
            let mut any = false;
            for a in 0..signatures.len() {
                for b in a + 1..signatures.len() {
                    if signatures[a] == signatures[b] && digest(a) != digest(b) {
                        any = true;
                        let key = format!(
                            "{:?} | {:?} | {}",
                            d.decision_kind,
                            actions[a].kind,
                            record_differences(actions, refs, a, b).join(",")
                        );
                        *tally.entry(key.clone()).or_default() += 1;
                        examples
                            .entry(key)
                            .or_insert_with(|| format!("{:?} vs {:?}", actions[a], actions[b]));
                    }
                }
            }
            flagged += u64::from(any);
            if session.step(d.episode_id, d.step, selected).is_err() {
                skipped += 1;
                continue 'games;
            }
        }
    }
    println!(
        "{{\"probe\":\"digest_only_actions\",\"games\":{games},\"skipped_games\":{skipped},\"decisions\":{decisions},\"decisions_with_digest_only_pairs\":{flagged},\"fraction\":{:.4},\"seconds\":{:.2}}}",
        flagged as f64 / decisions.max(1) as f64,
        started.elapsed().as_secs_f64()
    );
    for (key, count) in &tally {
        println!(
            "{}",
            serde_json::json!({"probe": "digest_only_pair", "key": key, "pairs": count, "example": examples[key]})
        );
    }
    for (key, (count, decks)) in &stops {
        println!(
            "{}",
            serde_json::json!({"probe": "encode_stop", "key": key, "games": count, "decks": decks})
        );
    }
    #[cfg(feature = "s4a-profile-v1")]
    println!(
        "{}",
        serde_json::json!({"probe": "tensorizer_profile", "spans": crate::search_profile_v1::take_json(),
            "digest": crate::search_profile_v1::digest::take_json(), "decisions": decisions})
    );
}

/// The probe's episodes: the run config's, or, with `MTG_KERNEL_PROBE_DECKS`
/// (a JSON list of `{label, mainboard, sideboard}` with `[count, name]`
/// rows), D1's first episode as a template with those decks paired in turn
/// and fresh seeds. Decks this build cannot execute are left out and counted.
fn probe_episodes_v1() -> Vec<ExpandedEpisodeV1> {
    let Ok(path) = std::env::var("MTG_KERNEL_PROBE_DECKS") else {
        return episodes_from_config();
    };
    let template = episodes_from_config().remove(0);
    let decks: Vec<Value> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let ids = |rows: &Value| -> Option<Vec<u16>> {
        let mut out = Vec::new();
        for row in rows.as_array()? {
            let id = crate::card_def::card_id_by_name(row[1].as_str()?)?;
            out.extend(std::iter::repeat_n(id, row[0].as_u64()? as usize));
        }
        Some(out)
    };
    let mut lists = Vec::new();
    let mut refused = 0usize;
    for deck in &decks {
        let label = deck["label"].as_str().unwrap().to_owned();
        let built = ids(&deck["mainboard"])
            .zip(ids(&deck["sideboard"]))
            .and_then(|(mainboard, sideboard)| {
                crate::sideboard::RegisteredDeckV1::new_executable_v1(
                    &label,
                    mainboard.clone(),
                    sideboard.clone(),
                )
                .ok()
                .map(|_| ExpandedDeckListV1 {
                    label: label.clone(),
                    mainboard,
                    sideboard,
                })
            });
        match built {
            Some(list) => lists.push(list),
            None => refused += 1,
        }
    }
    println!(
        "{{\"probe\":\"deck_pool\",\"decks\":{},\"executable\":{},\"refused\":{refused}}}",
        decks.len(),
        lists.len()
    );
    assert!(lists.len() >= 2, "too few executable decks");
    (0..lists.len() * 2)
        .map(|i| {
            let mut episode = template.clone();
            let (a, b) = (&lists[i % lists.len()], &lists[(i * 7 + 3) % lists.len()]);
            episode.id = format!("digest-probe-{i}");
            episode.seed = 0x5eed_0000 + i as u64;
            episode.starting_player = (i % 2) as u8;
            episode.learner_seat = 0;
            episode.opponent = None;
            episode.opponent_kind = None;
            episode.opponent_search = None;
            episode.registered = [a.clone(), b.clone()];
            episode.selected = [a.clone(), b.clone()];
            episode.postboard = false;
            episode.reset = None;
            episode.opening = None;
            episode.mulligan_modes = None;
            episode
        })
        .collect()
}
