//! Input-validity checks for a frozen V4 policy (diagnostic only).
//!
//! Plays the policy against itself with `cfg.pilot_deck` in the pilot seat.
//! At sampled multi-action decisions it checks that scoring is stateless, that
//! the encoded input does not depend on hidden identities, how much the 96
//! SHA-512 state-digest inputs move the policy, which legal actions are told
//! apart only by the action hash, and whether chosen state edits (own library
//! composition, graveyard identities, the Prismatic Strands colour) reach the
//! non-hash inputs at all.

use super::{mix, named, seat_index, MAX_PHYSICAL};
use crate::card_def::{CardType, CARD_DEFS};
use crate::native_flat_tensorizer_v2::NativeFlatDecisionTensorV2;
use crate::native_flat_tensorizer_v4::NativeFlatDecisionTensorV4;
use crate::paired_bo1_harness_v1::paired_policy_seeds_v1;
use crate::rl::TerminalClassificationV1;
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::runtime_decks::RUNTIME_DECKS;
use crate::ids::PlayerId;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use crate::state::{GameState, ObjectStateV4, SplitMix64};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Write;
use std::sync::Mutex;

const STATE_HEAD: usize = 123;
const ACTION_DIM: usize = 195;
const ACTION_EXPLICIT: usize = 99;
const REF_DIM: usize = 25;

fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|x| x.to_bits()).collect()
}

/// Which tensor components differ between two encodings.
fn tensor_diff(a: &NativeFlatDecisionTensorV2, b: &NativeFlatDecisionTensorV2) -> Vec<&'static str> {
    let mut out = Vec::new();
    if a.state.len() != b.state.len() || bits(&a.state[..STATE_HEAD]) != bits(&b.state[..STATE_HEAD]) {
        out.push("state_head");
    }
    if a.state.len() == b.state.len() && bits(&a.state[STATE_HEAD..]) != bits(&b.state[STATE_HEAD..]) {
        out.push("state_digest");
    }
    if bits(&a.object_features) != bits(&b.object_features)
        || a.object_card_ids != b.object_card_ids
        || a.object_groups != b.object_groups
    {
        out.push("objects");
    }
    if a.object_node_ids != b.object_node_ids {
        out.push("node_ids");
    }
    if bits(&a.edge_features) != bits(&b.edge_features)
        || a.edge_source_indices != b.edge_source_indices
        || a.edge_target_indices != b.edge_target_indices
    {
        out.push("edges");
    }
    let rows = |t: &NativeFlatDecisionTensorV2, lo: usize, hi: usize| -> Vec<u32> {
        t.action_features
            .chunks_exact(ACTION_DIM)
            .flat_map(|r| bits(&r[lo..hi]))
            .collect()
    };
    if rows(a, 0, ACTION_EXPLICIT) != rows(b, 0, ACTION_EXPLICIT) {
        out.push("action_explicit");
    }
    if rows(a, ACTION_EXPLICIT, ACTION_DIM) != rows(b, ACTION_EXPLICIT, ACTION_DIM) {
        out.push("action_hash");
    }
    if bits(&a.action_ref_features) != bits(&b.action_ref_features)
        || a.action_ref_card_ids != b.action_ref_card_ids
        || a.action_ref_action_indices != b.action_ref_action_indices
        || a.action_ref_node_indices != b.action_ref_node_indices
    {
        out.push("action_refs");
    }
    out
}

/// Non-hash inputs differ (what the model can use beyond 96 random numbers).
fn semantic_differs(diff: &[&str]) -> bool {
    diff.iter().any(|d| *d != "state_digest" && *d != "action_hash" && *d != "node_ids")
}

/// Groups of legal actions whose explicit features and references (ref
/// features plus referenced node) are identical: told apart only by hash.
fn hash_only_groups(t: &NativeFlatDecisionTensorV2) -> Vec<Vec<usize>> {
    let n = t.action_features.len() / ACTION_DIM;
    let mut keys: BTreeMap<(Vec<u32>, Vec<(Vec<u32>, i64)>), Vec<usize>> = BTreeMap::new();
    let mut refs: Vec<Vec<(Vec<u32>, i64)>> = vec![Vec::new(); n];
    for (r, &a) in t.action_ref_action_indices.iter().enumerate() {
        refs[a as usize].push((
            bits(&t.action_ref_features[r * REF_DIM..(r + 1) * REF_DIM]),
            t.action_ref_node_indices[r],
        ));
    }
    for (i, row) in t.action_features.chunks_exact(ACTION_DIM).enumerate() {
        let mut rf = std::mem::take(&mut refs[i]);
        rf.sort();
        keys.entry((bits(&row[..ACTION_EXPLICIT]), rf)).or_default().push(i);
    }
    keys.into_values().filter(|g| g.len() > 1).collect()
}

fn softmax(logits: &[f32]) -> Vec<f64> {
    super::softmax(logits)
}

fn tv(p: &[f64], q: &[f64]) -> f64 {
    0.5 * p.iter().zip(q).map(|(a, b)| (a - b).abs()).sum::<f64>()
}

fn argmax(p: &[f64]) -> usize {
    (0..p.len()).max_by(|&a, &b| p[a].partial_cmp(&p[b]).unwrap()).unwrap()
}

fn score_tensor(
    policy: &mut FrozenPlayPolicyV1,
    session: &FastActorSessionV1,
    label: &str,
) -> Result<(Vec<f32>, NativeFlatDecisionTensorV2), String> {
    policy.reset_sampling_v1([1, 2]);
    let s = policy
        .score_fast_session_v1(session)
        .map_err(|e| format!("[{label}] {e}"))?;
    let t = policy
        .diagnostic_last_v4_tensor()
        .ok_or("policy is not a V4 fresh-lineage policy")?
        .common
        .clone();
    Ok((s.logits, t))
}

fn is_land(state: &GameState, o: crate::ids::ObjectId) -> bool {
    CARD_DEFS[state.objects.get(o).card_def as usize].types.contains(&CardType::Land)
}

fn set_def(state: &mut GameState, o: crate::ids::ObjectId, def: u16) {
    let obj = state.objects.get_mut(o);
    obj.card_def = def;
    obj.name = CARD_DEFS[def as usize].name.to_string();
    obj.v4 = ObjectStateV4::from_card_def(def);
}

fn kind_of(v: &Value) -> String {
    let src = v["source"].as_str().unwrap_or("");
    format!("{}:{}", v["action_kind"].as_str().unwrap_or("?"), src)
}

pub(super) fn run_validity_game(
    cfg: &super::CensusConfigV1,
    game: u64,
    base: &mut FrozenPlayPolicyV1,
    roll: &mut FrozenPlayPolicyV1,
    sink: &Mutex<std::fs::File>,
) -> Result<(), String> {
    let n = cfg.decks.len() as u64;
    let pilot = (game % 2) as usize;
    let opp_deck = cfg.decks[((game / 2) % n) as usize];
    let starting = ((game / (2 * n)) % 2) as u8;
    let seed = mix(cfg.base_seed ^ mix(game));
    let mut deck_ix = [opp_deck; 2];
    deck_ix[pilot] = cfg.pilot_deck;
    let decks = [&RUNTIME_DECKS[deck_ix[0]], &RUNTIME_DECKS[deck_ix[1]]];
    let mut session =
        FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
            1,
            seed,
            MAX_PHYSICAL,
            MAX_PHYSICAL * 128,
            [decks[0].id.to_owned(), decks[1].id.to_owned()],
            [decks[0].card_ids.to_vec(), decks[1].card_ids.to_vec()],
            PlayerId(starting),
        )
        .map_err(|e| format!("{e:?}"))?;
    base.reset_sampling_v1(paired_policy_seeds_v1(seed));
    let mut rng = SplitMix64::seed(mix(seed ^ 0x5641_4C49));
    let mut rows: Vec<Value> = Vec::new();
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut bump = |k: String| *counts.entry(k).or_default() += 1;
    let mut ordinal = 0u64;
    let score = loop {
        match session.current_response() {
            FastActorResponseV1::Terminal(t) => {
                let natural = t.terminal_classification == TerminalClassificationV1::Natural;
                break match t.winner {
                    Some(w) if natural => f64::from(u8::from(seat_index(w) == pilot)),
                    _ => 0.5,
                };
            }
            FastActorResponseV1::Decision(d) => {
                let actor = seat_index(d.acting_player);
                let k = d.legal_action_count as usize;
                let st = session.game_state();
                let step = format!("{:?}", st.step);
                let active = st.active_player.0 as usize == actor;
                let sem: Vec<Value> = session
                    .diagnostic_current_action_semantics()
                    .ok_or("semantics")?
                    .iter()
                    .map(|x| named(serde_json::to_value(x).unwrap_or_default()))
                    .collect();
                if actor == pilot {
                    let strands = sem.iter().any(|v| {
                        v["action_kind"] == "cast_spell" && v["source"] == "Prismatic Strands"
                    });
                    bump(format!("pilot_step|{step}|{}|strands_offered={strands}", if active { "own" } else { "opp" }));
                    let me = &st.players[pilot];
                    let has = |zone: &[crate::ids::ObjectId]| {
                        zone.iter().any(|&o| st.objects.get(o).name == "Prismatic Strands")
                    };
                    let (in_hand, in_gy) = (has(&me.hand), has(&me.graveyard));
                    let priority = sem.iter().any(|v| v["action_kind"] == "pass");
                    if priority && (in_hand || in_gy) && !strands {
                        let untapped: Vec<String> = me
                            .battlefield
                            .iter()
                            .filter(|&&o| !st.objects.get(o).tapped)
                            .map(|&o| st.objects.get(o).name.clone())
                            .collect();
                        let white_creature = me.battlefield.iter().any(|&o| {
                            let ob = st.objects.get(o);
                            !ob.tapped
                                && CARD_DEFS[ob.card_def as usize].types.contains(&CardType::Creature)
                                && CARD_DEFS[ob.card_def as usize].colors.contains(&crate::mana::ManaColor::W)
                        });
                        let lands = me.battlefield.iter().filter(|&&o| !st.objects.get(o).tapped && is_land(st, o)).count();
                        let castable_guess = (in_hand && lands >= 3) || (in_gy && white_creature);
                        bump(format!("strands_not_offered|{step}|{}|hand={in_hand}|gy={in_gy}|guess_castable={castable_guess}", if active { "own" } else { "opp" }));
                        if castable_guess && rows.len() < 400 {
                            rows.push(json!({"kind":"strands_not_offered","step":step,"active":active,"turn":st.turn,
                                "untapped":untapped,"hand":super::zone_names(st,&me.hand),"gy":super::zone_names(st,&me.graveyard),
                                "pool":me.mana_pool,"cands":sem}));
                        }
                    }
                }
                if k >= 2 {
                    ordinal += 1;
                    let draw = (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
                    if draw < cfg.root_prob {
                        let row = check_decision(cfg, &session, base, roll, seed, ordinal, &sem)?;
                        let mut row = row;
                        row["game"] = json!(game);
                        row["actor_is_pilot"] = json!(actor == pilot);
                        row["step"] = json!(step);
                        row["active"] = json!(active);
                        row["turn"] = json!(st.turn);
                        rows.push(row);
                    }
                }
                let a = base.select_fast_session_v1(&session)?;
                if actor == pilot {
                    let v = &sem[a as usize];
                    if v["source"] == "Prismatic Strands" || v["source"] == "Basilisk Gate" {
                        bump(format!("pilot_chosen|{}|{step}|{}", kind_of(v), if active { "own" } else { "opp" }));
                    }
                }
                session
                    .step(d.episode_id, d.step, a)
                    .map_err(|e| format!("{e:?}"))?;
            }
        }
    };
    let mut out = json!({"kind":"validity_game","game":game,"pilot_seat":pilot,
        "pilot_deck":decks[pilot].id,"opp_deck":RUNTIME_DECKS[opp_deck].id,
        "starting_player":starting,"score":score,"counts":counts})
    .to_string();
    out.push('\n');
    for r in rows {
        out.push_str(&r.to_string());
        out.push('\n');
    }
    let mut f = sink.lock().map_err(|_| "sink poisoned")?;
    f.write_all(out.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

fn check_decision(
    _cfg: &super::CensusConfigV1,
    session: &FastActorSessionV1,
    base: &mut FrozenPlayPolicyV1,
    roll: &mut FrozenPlayPolicyV1,
    game_seed: u64,
    ordinal: u64,
    sem: &[Value],
) -> Result<Value, String> {
    let FastActorResponseV1::Decision(d) = session.current_response() else {
        return Err("terminal".into());
    };
    let actor = seat_index(d.acting_player);
    let (logits, t0) = score_tensor(roll, session, "root")?;
    let p0 = softmax(&logits);
    // 1. Stateless scoring: a second policy instance with its own history.
    let (logits_b, tb) = score_tensor(base, session, "second_instance")?;
    let stateless = bits(&logits) == bits(&logits_b) && tensor_diff(&t0, &tb).is_empty();
    // 2. Hidden-identity invariance: redeterminized clones encode identically.
    let mut hidden_diff: Vec<&str> = Vec::new();
    let mut hidden_errors: Vec<Value> = Vec::new();
    for r in 0..2u64 {
        let det = mix(game_seed ^ mix(0x4849_4444 ^ (ordinal << 4) ^ r));
        let clone = session.census_redeterminized_clone_v1(det)?;
        let tc = match score_tensor(roll, &clone, "redeterminized") {
            Ok((_, tc)) => tc,
            Err(e) => {
                hidden_errors.push(json!({"error": e, "cands": sem}));
                continue;
            }
        };
        for x in tensor_diff(&t0, &tc) {
            if !hidden_diff.contains(&x) {
                hidden_diff.push(x);
            }
        }
    }
    // 3. State-digest sensitivity: replace the 96 hash inputs with fresh
    // uniform(-1, 1) draws (same distribution) and with zeros.
    let mut rng = SplitMix64::seed(mix(game_seed ^ mix(0x4447 ^ ordinal)));
    let mut tvs = Vec::new();
    let mut flips = 0u32;
    let mut chosen_shift = Vec::new();
    let a0 = argmax(&p0);
    for rep in 0..5 {
        let mut t = NativeFlatDecisionTensorV4 { common: t0.clone() };
        for x in t.common.state[STATE_HEAD..].iter_mut() {
            *x = if rep == 4 {
                0.0
            } else {
                let u = (rng.next_u64() >> 32) as u32;
                ((f64::from(u) / f64::from(u32::MAX)) * 2.0 - 1.0) as f32
            };
        }
        let s = roll.diagnostic_forward_v4(&t)?;
        let q = softmax(&s.logits);
        if rep < 4 {
            tvs.push(tv(&p0, &q));
            if argmax(&q) != a0 {
                flips += 1;
            }
            chosen_shift.push(q[a0] - p0[a0]);
        } else {
            chosen_shift.push(f64::NAN);
            tvs.push(tv(&p0, &q));
        }
    }
    // 4. Actions told apart only by the 96-dim action hash.
    let groups: Vec<Vec<String>> = hash_only_groups(&t0)
        .into_iter()
        .map(|g| g.into_iter().map(|i| sem[i].to_string()).collect())
        .collect();
    // 5. Edits that a player could know about but the policy may not see.
    let st = session.game_state();
    let me = &st.players[actor];
    let op = &st.players[1 - actor];
    let mut edits = serde_json::Map::new();
    // 5a. Own library composition: turn one own library land into a nonland
    // card from the opponent's (hidden) library, or a nonland into a land.
    let own_land = me.library.iter().copied().find(|&o| is_land(st, o));
    let own_nonland = me.library.iter().copied().find(|&o| !is_land(st, o));
    let opp_nonland = op.library.iter().copied().find(|&o| !is_land(st, o));
    let opp_land = op.library.iter().copied().find(|&o| is_land(st, o));
    let lib_pair = match (own_land, opp_nonland, own_nonland, opp_land) {
        (Some(a), Some(b), _, _) => Some((a, b)),
        (_, _, Some(a), Some(b)) => Some((a, b)),
        _ => None,
    };
    if let Some((mine, theirs)) = lib_pair {
        let (da, db) = (st.objects.get(mine).card_def, st.objects.get(theirs).card_def);
        let edited = session.census_edited_clone_v1(|s| {
            set_def(s, mine, db);
            set_def(s, theirs, da);
        });
        let d: Value = match score_tensor(roll, &edited, "library_edit") {
            Ok((_, te)) => json!(tensor_diff(&t0, &te)),
            Err(e) => json!(format!("encode_error: {e}")),
        };
        edits.insert("own_library_land_count".into(), json!(d));
    }
    // 5b. Graveyard identities: change the oldest and newest card of each
    // graveyard to a card definition from that player's library.
    for (who, player) in [("own", me), ("opp", op)] {
        let Some(&donor) = player.library.first() else { continue };
        let donor_def = st.objects.get(donor).card_def;
        for (pos, o) in [("oldest", player.graveyard.first()), ("newest", player.graveyard.last())] {
            let Some(&o) = o else { continue };
            if st.objects.get(o).card_def == donor_def {
                continue;
            }
            let edited = session.census_edited_clone_v1(|s| set_def(s, o, donor_def));
            let d: Value = match score_tensor(roll, &edited, "graveyard_edit") {
                Ok((_, te)) => json!(tensor_diff(&t0, &te)),
                Err(e) => json!(format!("encode_error: {e}")),
            };
            edits.insert(format!("{who}_graveyard_{pos}"), json!({"len": player.graveyard.len(), "diff": d}));
        }
    }
    // 5c. Prismatic Strands shield colour (when one is active).
    let shield = st.engine.active_replacements.iter().position(|r| {
        matches!(
            r.kind,
            crate::event::ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn { .. }
        )
    });
    if let Some(i) = shield {
        let edited = session.census_edited_clone_v1(|s| {
            if let crate::event::ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn {
                color,
                ..
            } = &mut s.engine.active_replacements[i].kind
            {
                *color = if *color == crate::mana::ManaColor::R {
                    crate::mana::ManaColor::G
                } else {
                    crate::mana::ManaColor::R
                };
            }
        });
        let d: Value = match score_tensor(roll, &edited, "shield_colour") {
            Ok((_, te)) => json!(tensor_diff(&t0, &te)),
            Err(e) => json!(format!("encode_error: {e}")),
        };
        edits.insert("strands_shield_colour".into(), json!(d));
        let removed = session.census_edited_clone_v1(|s| {
            s.engine.active_replacements.remove(i);
        });
        let d: Value = match score_tensor(roll, &removed, "shield_removed") {
            Ok((_, te)) => json!(tensor_diff(&t0, &te)),
            Err(e) => json!(format!("encode_error: {e}")),
        };
        edits.insert("strands_shield_present".into(), json!(d));
    }
    let semantic_edits: serde_json::Map<String, Value> = edits
        .iter()
        .map(|(k, v)| {
            let diff: Vec<String> = serde_json::from_value(
                v.get("diff").cloned().unwrap_or_else(|| v.clone()),
            )
            .unwrap_or_default();
            let refs: Vec<&str> = diff.iter().map(String::as_str).collect();
            (k.clone(), json!(semantic_differs(&refs)))
        })
        .collect();
    Ok(json!({"kind":"validity_decision","k":p0.len(),"chosen_kinds":sem.get(a0).map(kind_of),
        "stateless":stateless,"hidden_diff":hidden_diff,"hidden_errors":hidden_errors,
        "digest_tv":tvs,"digest_argmax_flips":flips,"digest_chosen_shift":chosen_shift,"p_max":p0[a0],
        "hash_only_groups":groups,"edits":edits,"edits_semantic":semantic_edits}))
}
