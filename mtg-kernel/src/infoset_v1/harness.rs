//! AC1 to AC3 harnesses on engine games (design v7, section 13).
//!
//! Worlds are coupled: W' starts from W's state with whole hidden objects
//! permuted (F-deal at the deal, F-slot at a decision), replays W's actions
//! by semantic match and shares W's random stream. A decision is compared
//! only while V's views (V4 observation and menu) are byte-equal; at the
//! first difference the pair is truncated.

use super::tests::export_at;
use super::*;
use crate::ids::{ObjectId, PlayerId};
use crate::rl::PlayerSeatV1;
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::state::{GameState, ObjectStateV4, SplitMix64, Zone};
use std::collections::BTreeMap;

pub(crate) fn seat(p: PlayerSeatV1) -> PlayerId {
    match p {
        PlayerSeatV1::P0 => PlayerId::P0,
        PlayerSeatV1::P1 => PlayerId::P1,
    }
}

/// Viewer-local ids: every `arena_id` becomes its order of first
/// appearance in the value, and hashes over native ids are dropped, so two
/// worlds that differ only in provenance give equal views (the producer's
/// projection keeps native ids inside the environment the same way).
fn localize(value: &mut serde_json::Value, ids: &mut BTreeMap<u64, u64>) {
    match value {
        serde_json::Value::Object(m) => {
            // Epoch numbers are identity bookkeeping (no tensorizer reads
            // them) and are canonicalized by design (6.5).
            // Session counters are canonical constants or V's own index
            // in the export (6.8), and no tensorizer reads them.
            m.retain(|k, _| {
                !k.contains("hash")
                    && !matches!(
                        k.as_str(),
                        "zone_change_count" | "physical_decision_id" | "step_index"
                    )
            });
            for (k, v) in m.iter_mut() {
                if k == "arena_id" {
                    if let Some(n) = v.as_u64() {
                        let next = ids.len() as u64;
                        *v = serde_json::Value::from(*ids.entry(n).or_insert(next));
                        continue;
                    }
                }
                localize(v, ids);
            }
        }
        serde_json::Value::Array(xs) => xs.iter_mut().for_each(|x| localize(x, ids)),
        _ => {}
    }
}

/// Sorts every `legal_targets` list by content without native ids.
fn sort_candidate_lists(value: &mut serde_json::Value) {
    fn strip(v: &serde_json::Value) -> String {
        let mut v = v.clone();
        localize(&mut v, &mut BTreeMap::new());
        fn drop_ids(v: &mut serde_json::Value) {
            match v {
                serde_json::Value::Object(m) => {
                    m.remove("arena_id");
                    m.values_mut().for_each(drop_ids);
                }
                serde_json::Value::Array(xs) => xs.iter_mut().for_each(drop_ids),
                _ => {}
            }
        }
        drop_ids(&mut v);
        v.to_string()
    }
    match value {
        serde_json::Value::Object(m) => {
            for (k, v) in m.iter_mut() {
                if k == "legal_targets" {
                    if let serde_json::Value::Array(xs) = v {
                        xs.sort_by_key(strip);
                    }
                }
                sort_candidate_lists(v);
            }
        }
        serde_json::Value::Array(xs) => xs.iter_mut().for_each(sort_candidate_lists),
        _ => {}
    }
}

/// V's view at the current decision: the V4 observation and menu, with
/// viewer-local ids.
pub(crate) fn view(s: &FastActorSessionV1) -> Option<Vec<u8>> {
    let FastActorResponseV1::Decision(d) = s.current_response() else {
        return None;
    };
    let (obs, menu) = s.actor_visible_decision_v4(d).ok()?;
    // The engine lists candidates in native-id order, which the wire
    // re-sorts (the adapter's row order): compare candidate sets by content.
    let mut obs = serde_json::to_value(&obs).expect("view serializes");
    sort_candidate_lists(&mut obs);
    localize(&mut obs, &mut BTreeMap::new());
    let mut rows: Vec<String> = menu
        .iter()
        .map(|m| {
            let mut v = serde_json::to_value(m).expect("semantics serialize");
            localize(&mut v, &mut BTreeMap::new());
            v.to_string()
        })
        .collect();
    rows.sort();
    Some(serde_json::to_vec(&(obs, rows)).expect("view serializes"))
}

/// AC3's view: the V4 observation and the menu with `arena_id`s renamed
/// through `ids` (native to canonical for the live session), bookkeeping
/// fields dropped, and candidate lists and menu rows sorted (the wire
/// re-sorts the engine's native-id order).
fn ac3_view(s: &FastActorSessionV1, ids: Option<&BTreeMap<u64, u64>>) -> Option<serde_json::Value> {
    use super::order::canonical_rename_v1 as rename;
    let FastActorResponseV1::Decision(d) = s.current_response() else {
        return None;
    };
    let (obs, menu) = s.actor_visible_decision_v4(d).ok()?;
    let mut obs = serde_json::to_value(&obs).ok()?;
    // Timestamps are re-ranked by design (6.5): compare their dense ranks.
    fn timestamps(v: &serde_json::Value, out: &mut std::collections::BTreeSet<u64>) {
        match v {
            serde_json::Value::Object(m) => m.iter().for_each(|(k, x)| {
                if k.contains("timestamp") {
                    if let Some(t) = x.as_u64() {
                        out.insert(t);
                    }
                }
                timestamps(x, out);
            }),
            serde_json::Value::Array(xs) => xs.iter().for_each(|x| timestamps(x, out)),
            _ => {}
        }
    }
    fn rerank(v: &mut serde_json::Value, rank: &BTreeMap<u64, u64>) {
        match v {
            serde_json::Value::Object(m) => m.iter_mut().for_each(|(k, x)| {
                if k.contains("timestamp") {
                    if let Some(r) = x.as_u64().and_then(|t| rank.get(&t)) {
                        *x = serde_json::json!(r);
                    }
                }
                rerank(x, rank);
            }),
            serde_json::Value::Array(xs) => xs.iter_mut().for_each(|x| rerank(x, rank)),
            _ => {}
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    timestamps(&obs, &mut seen);
    let rank: BTreeMap<u64, u64> = seen
        .into_iter()
        .enumerate()
        .map(|(i, t)| (t, i as u64))
        .collect();
    rerank(&mut obs, &rank);
    // Attachment lists name objects by bare arena id, outside the
    // `arena_id` keys `rename` maps: map them the same way, as sets.
    fn attachments(v: &mut serde_json::Value, ids: Option<&BTreeMap<u64, u64>>) {
        match v {
            serde_json::Value::Object(m) => m.iter_mut().for_each(|(k, x)| {
                if let (true, serde_json::Value::Array(xs)) = (k == "attachments", &mut *x) {
                    if let Some(ids) = ids {
                        for a in xs.iter_mut() {
                            if let Some(n) = a.as_u64() {
                                *a = ids
                                    .get(&n)
                                    .map_or(serde_json::json!("hidden"), |c| serde_json::json!(c));
                            }
                        }
                    }
                    xs.sort_by_key(|a| a.to_string());
                } else {
                    attachments(x, ids);
                }
            }),
            serde_json::Value::Array(xs) => xs.iter_mut().for_each(|x| attachments(x, ids)),
            _ => {}
        }
    }
    attachments(&mut obs, ids);
    rename(&mut obs, ids);
    let mut rows: Vec<String> = menu
        .iter()
        .map(|m| {
            let mut v = serde_json::to_value(m).expect("semantics serialize");
            rename(&mut v, ids);
            v.to_string()
        })
        .collect();
    rows.sort();
    Some(serde_json::json!([obs, rows]))
}

/// The semantic form of each candidate at the current decision, with
/// decision-local ids (fixtures order their default answers by it).
pub(crate) fn local_menu(s: &FastActorSessionV1) -> Vec<Vec<u8>> {
    menu(s)
}

/// The semantic form of each candidate at the current decision, with
/// decision-local ids.
fn menu(s: &FastActorSessionV1) -> Vec<Vec<u8>> {
    let mut ids = BTreeMap::new();
    s.diagnostic_current_action_semantics()
        .unwrap_or_default()
        .iter()
        .map(|x| {
            let mut v = serde_json::to_value(x).expect("semantics serialize");
            localize(&mut v, &mut ids);
            serde_json::to_vec(&v).expect("semantics serialize")
        })
        .collect()
}

/// The chosen candidate's semantics with native ids.
pub(crate) fn native_action(s: &FastActorSessionV1, index: usize) -> serde_json::Value {
    let sem = s.diagnostic_current_action_semantics().unwrap_or_default();
    serde_json::to_value(&sem[index]).expect("semantics serialize")
}

/// A recorded game: the reset session, chosen candidates by semantics.
pub(crate) struct GameV1 {
    pub start: FastActorSessionV1,
    pub actions: Vec<Vec<u8>>,
    /// The same choices with native ids (realizability replays).
    pub native: Vec<serde_json::Value>,
}

/// Whether AC1's games run with the bridge's neutral choices, as the
/// entrant's bridge always does (lead ruling 6). On by default;
/// `W1_AC1_NEUTRAL=0` gives the former configuration for comparison.
pub(crate) fn ac1_neutral() -> bool {
    std::env::var("W1_AC1_NEUTRAL").map_or(true, |v| v != "0")
}

/// An AC1 engine-family game, in AC1's configuration ([`ac1_neutral`]).
pub(crate) fn record_game(decks: [&str; 2], seed: u64, max_decisions: usize) -> GameV1 {
    record_game_with(decks, seed, max_decisions, ac1_neutral())
}

/// A uniformly random game; `neutral` enables the bridge's neutral choices
/// on the reset session before the import, as the bridge does.
pub(crate) fn record_game_with(
    decks: [&str; 2],
    seed: u64,
    max_decisions: usize,
    neutral: bool,
) -> GameV1 {
    let mut live = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
        seed,
        seed,
        2_000,
        20_000,
        [decks[0].to_string(), decks[1].to_string()],
    )
    .expect("reset");
    if neutral {
        live.enable_spellbench_neutral_choices();
    }
    let start = FastActorSessionV1::import_infoset_v1(&live, live.game_state().clone(), 0);
    let mut s = start.clone();
    let mut rng = SplitMix64::seed(seed ^ 0x51ed);
    let mut actions = Vec::new();
    let mut native = Vec::new();
    for _ in 0..max_decisions {
        let FastActorResponseV1::Decision(d) = s.current_response() else {
            break;
        };
        let a = (rng.next_u64() % u64::from(d.legal_action_count)) as u32;
        actions.push(menu(&s)[a as usize].clone());
        native.push(native_action(&s, a as usize));
        s.step(d.episode_id, d.step, a).expect("legal step");
    }
    GameV1 {
        start,
        actions,
        native,
    }
}

/// Swaps the definitions of two pristine hidden objects (whole objects:
/// definitions travel with their state, which is pristine by I1).
pub(crate) fn swap_objects(state: &mut GameState, a: ObjectId, b: ObjectId) {
    let (da, na) = {
        let o = state.objects.get(a);
        (o.card_def, o.name.clone())
    };
    let (db, nb) = {
        let o = state.objects.get(b);
        (o.card_def, o.name.clone())
    };
    let oa = state.objects.get_mut(a);
    oa.card_def = db;
    oa.name = nb;
    oa.v4 = ObjectStateV4::from_card_def(db);
    let ob = state.objects.get_mut(b);
    ob.card_def = da;
    ob.name = na;
    ob.v4 = ObjectStateV4::from_card_def(da);
}

/// Exchanges the slots (positions and zones) of two hidden objects of one
/// owner; definitions and incarnations travel with the objects.
pub(crate) fn move_objects(state: &mut GameState, a: ObjectId, b: ObjectId) {
    let owner = state.objects.get(a).owner;
    let p = &mut state.players[owner.index()];
    for v in [&mut p.hand, &mut p.library] {
        for x in v.iter_mut() {
            if *x == a {
                *x = b;
            } else if *x == b {
                *x = a;
            }
        }
    }
    let (za, zb) = (state.objects.get(a).zone, state.objects.get(b).zone);
    state.objects.get_mut(a).zone = zb;
    state.objects.get_mut(b).zone = za;
}

/// One family pair's comparison.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub(crate) struct PairReportV1 {
    pub compared: u64,
    pub truncated: bool,
    /// Correct producer: decisions whose payload bytes or presence differ.
    pub p_differs: u64,
    pub p_refusals: BTreeMap<String, u64>,
    /// Mutant witnesses: compared decisions where P agrees and M differs.
    pub witnesses: BTreeMap<String, u64>,
    /// Compared decisions where P agrees across the worlds and M's export
    /// departs from P's in at least one of them (a witness needs one).
    pub departs: BTreeMap<String, u64>,
    /// Diagnostics: the first decision where P differs, and each mutant's
    /// first departure (step and the outcomes' heads).
    pub notes: Vec<String>,
}

/// Diagnostics (ruling 2 (b)): where two worlds' delivered payloads
/// differ: the canonical export bytes, and each payload field that differs,
/// with the surface context printed in full on both sides.
fn split_difference(s: &FastActorSessionV1, t: &FastActorSessionV1, viewer: PlayerId) -> String {
    let plain = ExportOptionsV1::default();
    let (xs, xt) = (export_at(s, viewer, plain), export_at(t, viewer, plain));
    let export_equal = outcome(&xs) == outcome(&xt);
    let (Ok(xs), Ok(xt)) = (xs, xt) else {
        return format!("export_equal {export_equal}; an export refused");
    };
    let (Some(ps), Some(pt)) = (
        PayloadV1::new(&xs, s, viewer, 0),
        PayloadV1::new(&xt, t, viewer, 0),
    ) else {
        return format!("export_equal {export_equal}; a payload refused");
    };
    let (a, b) = (
        serde_json::to_value(&ps).expect("payload"),
        serde_json::to_value(&pt).expect("payload"),
    );
    let fields: Vec<String> = a
        .as_object()
        .expect("object")
        .iter()
        .filter(|(k, v)| b.get(k.as_str()) != Some(*v))
        .map(|(k, _)| k.clone())
        .collect();
    let state_diffs = if a["state"] != b["state"] {
        format!(
            "{:?}",
            super::tests::diffs(&a["state"], &b["state"], String::new(), 6)
        )
    } else {
        "none".into()
    };
    format!(
        "export_equal {export_equal}; differing payload fields {fields:?}; state diffs {state_diffs}; surface W {}; surface W' {}",
        a["surface"], b["surface"]
    )
}

/// A short, comparable head of an outcome for diagnostics.
pub(crate) fn head(o: &Result<Vec<u8>, String>) -> String {
    use sha2::Digest as _;
    match o {
        Err(e) => format!("Err({e})"),
        Ok(b) => {
            let text = String::from_utf8_lossy(b);
            let v: Option<serde_json::Value> = serde_json::from_slice(b).ok();
            match v {
                Some(v) if v.get("payload_json").is_some() => format!(
                    "payload sha {} map {} keys {}",
                    &format!(
                        "{:x}",
                        sha2::Sha256::digest(v["payload_json"].to_string().as_bytes())
                    )[..12],
                    v["order_map"],
                    v["row_keys_sorted"].as_array().map_or(0, |a| a.len())
                ),
                _ => text.chars().take(60).collect(),
            }
        }
    }
}

pub(crate) fn outcome(r: &Result<InfosetV1, ExportErrorV1>) -> Result<Vec<u8>, String> {
    match r {
        Ok(x) => Ok(x.bytes.clone()),
        Err(e) => Err(e.code_name().to_string()),
    }
}

/// P's compared outcome (fairness gate G5): the complete extension the
/// bridge delivers at this decision, compared across paired worlds: the
/// payload bytes (envelope, surface scalars and records, canonical state),
/// the order map, the row keys, a refusal's code and subclass, and absence
/// at an ineligible decision. The order map and row keys index the lockstep
/// rows, whose order the adapter replaces by the wire's semantic order, so
/// they are compared in that row-order-free form: each imported row's
/// paired lockstep key, and the row keys as a sorted list. Keys pair only
/// equal keys, so the map adds to the keys only which wire candidate holds
/// each key: a function of the base wire candidate list, which every bot
/// receives and which is not part of this extension. An export
/// refusal of subclass `export` is `Err(code)`; any other subclass is
/// `Err(code:subclass)`.
pub(crate) fn p_outcome(s: &FastActorSessionV1, viewer: PlayerId) -> Result<Vec<u8>, String> {
    use crate::agent_bridge_infoset_v1::{delivered_v1, eligible_semantics_v1, DeliveredV1};
    let semantics = s.diagnostic_current_action_semantics().unwrap_or_default();
    if !eligible_semantics_v1(&semantics) {
        return Ok(b"absent".to_vec());
    }
    match delivered_v1(s, viewer, 0) {
        Err(fault) => Err(format!("fault: {fault}")),
        Ok(DeliveredV1::Refused {
            error, subclass, ..
        }) => Err(if subclass == "export" {
            error.code_name().to_string()
        } else {
            format!("{}:{subclass}", error.code_name())
        }),
        Ok(DeliveredV1::Payload { value, .. }) => {
            let keys: Option<Vec<String>> = serde_json::from_value(value["row_keys"].clone()).ok();
            let rows: Option<Vec<usize>> = serde_json::from_value(value["live_rows"].clone()).ok();
            let paired = match (&keys, &rows) {
                (Some(k), Some(r)) => Some(r.iter().map(|&i| k[i].clone()).collect::<Vec<_>>()),
                _ => None,
            };
            let mut sorted = keys.clone();
            if let Some(k) = sorted.as_mut() {
                k.sort();
            }
            Ok(serde_json::to_vec(&serde_json::json!({
                "payload_json": value["payload_json"],
                "order_map": paired,
                "row_keys_sorted": sorted,
            }))
            .expect("delivered extension serializes"))
        }
    }
}

/// A producer's outcome as the bridge would deliver it (G5): absent at an
/// ineligible decision, a refusal by code, a payload refused by the surface
/// as `R2:surface`, else the export bytes. Mutants and P are compared
/// through it alike, so no witness rests on what the entrant never sees.
pub(crate) fn deliverable_outcome(
    s: &FastActorSessionV1,
    viewer: PlayerId,
    options: ExportOptionsV1,
) -> Result<Vec<u8>, String> {
    let semantics = s.diagnostic_current_action_semantics().unwrap_or_default();
    if !crate::agent_bridge_infoset_v1::eligible_semantics_v1(&semantics) {
        return Ok(b"absent".to_vec());
    }
    let x = export_at(s, viewer, options);
    let bytes = outcome(&x)?;
    let x = x.expect("outcome passed");
    let context = s.payload_surface_context_v1(&super::order::canonical_ids_v1(&x));
    match context {
        Some(c) if c.discard.as_ref().is_none_or(|d| d.player == viewer) => Ok(bytes),
        _ => Err("R2:surface".into()),
    }
}

/// The former P comparison (export and surface records only), kept for the
/// combat and discard record checks.
#[allow(dead_code)]
pub(crate) fn p_export_outcome(
    s: &FastActorSessionV1,
    viewer: PlayerId,
) -> Result<Vec<u8>, String> {
    let x = export_at(s, viewer, ExportOptionsV1::default());
    let mut bytes = outcome(&x)?;
    let x = x.expect("outcome passed");
    if s.surface_context_v1().is_none() {
        let ids = super::order::canonical_ids_v1(&x);
        let context = s.payload_surface_context_v1(&ids);
        if let Some(combat) = context.as_ref().and_then(|c| c.combat.as_ref()) {
            bytes.extend_from_slice(b"|combat:");
            bytes.extend(serde_json::to_vec(combat).expect("combat serializes"));
        }
        if let Some(discard) = context.as_ref().and_then(|c| c.discard.as_ref()) {
            bytes.extend_from_slice(b"|discard:");
            bytes.extend(serde_json::to_vec(discard).expect("discard serializes"));
        }
    }
    Ok(bytes)
}

/// The strict-view dump of one pair (rulings 4 and 5): both worlds' bridge
/// mirrors and the file (see `transcript.rs`).
pub(crate) struct StrictPairV1 {
    pub dump: super::transcript::DumpV1,
    pub w: super::transcript::WorldV1,
    pub w2: super::transcript::WorldV1,
}

/// P's and each mutant's delivered outcome at a V decision, as digests.
fn v_info(
    s: &FastActorSessionV1,
    t: &FastActorSessionV1,
    viewer: PlayerId,
    mutants: &[MutantV1],
    p: &Result<Vec<u8>, String>,
    p2: &Result<Vec<u8>, String>,
    old_equal: bool,
) -> serde_json::Value {
    use super::transcript::digest;
    let m: serde_json::Map<String, serde_json::Value> = mutants
        .iter()
        .map(|m| {
            let o = ExportOptionsV1 { mutant: Some(*m) };
            (
                format!("{m:?}"),
                serde_json::json!([
                    digest(&deliverable_outcome(s, viewer, o)),
                    digest(&deliverable_outcome(t, viewer, o))
                ]),
            )
        })
        .collect();
    serde_json::json!({"p": [digest(p), digest(p2)], "ph": [head(p), head(p2)], "m": m,
                       "old": old_equal})
}

/// Plays W and W' in lockstep from `w` and `w2` (equal up to the family's
/// permutation), comparing every viewer decision while views agree.
pub(crate) fn compare_worlds(
    w: FastActorSessionV1,
    w2: FastActorSessionV1,
    actions: &[Vec<u8>],
    viewer: PlayerId,
    mutants: &[MutantV1],
    strict: Option<StrictPairV1>,
) -> PairReportV1 {
    compare_worlds_with(w, w2, actions, viewer, mutants, false, usize::MAX, strict)
}

/// As [`compare_worlds`]; with `index_fallback`, W' takes W's candidate
/// index when no candidate matches semantically (F-prov: the worlds differ
/// only in native ids, which the semantic form carries).
///
/// The report is the former comparison (V's current observation with
/// decision-local ids). With `strict`, the pair continues past that view's
/// first difference until the replay ends or `limit` V decisions, writing
/// every decision to the dump; the strict checker decides truncation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn compare_worlds_with(
    w: FastActorSessionV1,
    w2: FastActorSessionV1,
    actions: &[Vec<u8>],
    viewer: PlayerId,
    mutants: &[MutantV1],
    index_fallback: bool,
    limit: usize,
    mut strict: Option<StrictPairV1>,
) -> PairReportV1 {
    let mut report = PairReportV1::default();
    let (mut s, mut t) = (w, w2);
    let mut old_live = true;
    let mut v_points = 0usize;
    let mut end = "actions";
    for (k, chosen) in actions.iter().enumerate() {
        if old_live && strict.is_none() && report.compared as usize >= limit {
            end = "limit";
            break;
        }
        if strict.is_some() && v_points >= limit {
            end = "limit";
            break;
        }
        let (FastActorResponseV1::Decision(d), FastActorResponseV1::Decision(e)) =
            (s.current_response(), t.current_response())
        else {
            end = "terminal";
            break;
        };
        let mut line = strict.as_mut().map(|st| {
            serde_json::json!({"k": k,
                "a": [format!("{:?}", d.acting_player), format!("{:?}", e.acting_player)],
                "w": [st.w.decision(&s), st.w2.decision(&t)]})
        });
        if seat(d.acting_player) == viewer && d.legal_action_count >= 2 {
            v_points += 1;
            let old_equal = old_live && view(&s) == view(&t);
            if old_live && !old_equal {
                report.truncated = true;
                old_live = false;
            }
            let p = p_outcome(&s, viewer);
            let p2 = p_outcome(&t, viewer);
            if old_live {
                report.compared += 1;
                if let Err(code) = &p {
                    *report.p_refusals.entry(code.clone()).or_default() += 1;
                }
                if p != p2 {
                    if report.p_differs == 0 {
                        report.notes.push(format!(
                            "P differs at decision {}: {} vs {}; split: {}",
                            report.compared,
                            head(&p),
                            head(&p2),
                            split_difference(&s, &t, viewer)
                        ));
                    }
                    report.p_differs += 1;
                } else {
                    let plain = ExportOptionsV1::default();
                    let (ps, pt) = (
                        deliverable_outcome(&s, viewer, plain),
                        deliverable_outcome(&t, viewer, plain),
                    );
                    for m in mutants {
                        let o = ExportOptionsV1 { mutant: Some(*m) };
                        let (ms, mt) = (
                            deliverable_outcome(&s, viewer, o),
                            deliverable_outcome(&t, viewer, o),
                        );
                        if ms != mt {
                            *report.witnesses.entry(format!("{m:?}")).or_default() += 1;
                        }
                        if ms != ps || mt != pt {
                            let n = report.departs.entry(format!("{m:?}")).or_default();
                            if *n == 0 {
                                report.notes.push(format!(
                                    "{m:?} departs at decision {}: P {} / {}; M {} / {}",
                                    report.compared,
                                    head(&ps),
                                    head(&pt),
                                    head(&ms),
                                    head(&mt)
                                ));
                            }
                            *n += 1;
                        }
                    }
                }
            }
            if let Some(line) = line.as_mut() {
                line["v"] = v_info(&s, &t, viewer, mutants, &p, &p2, old_equal);
            } else if !old_live {
                end = "old view";
                break;
            }
        }
        let a = menu(&s).iter().position(|x| x == chosen);
        let b = menu(&t).iter().position(|x| x == chosen).or(
            if index_fallback && d.legal_action_count == e.legal_action_count {
                a
            } else {
                None
            },
        );
        if let (Some(st), Some(line)) = (strict.as_mut(), line.as_mut()) {
            line["c"] = serde_json::json!([
                a.map_or(serde_json::Value::Null, |a| st.w.raw_candidate(a)),
                b.map_or(serde_json::Value::Null, |b| st.w2.raw_candidate(b))
            ]);
            st.dump.line(line);
            if let (Some(a), Some(b)) = (a, b) {
                st.w.advance(a);
                st.w2.advance(b);
            }
        }
        let (Some(a), Some(b)) = (a, b) else {
            if old_live {
                report.truncated = true;
            }
            end = "replay";
            break;
        };
        if s.step(d.episode_id, d.step, a as u32).is_err()
            || t.step(e.episode_id, e.step, b as u32).is_err()
        {
            if old_live {
                report.truncated = true;
            }
            end = "step";
            break;
        }
    }
    if let Some(st) = strict {
        let rebuilds = format!(
            "{end}; raw rebuilds {} {}",
            st.w.rebuilds(),
            st.w2.rebuilds()
        );
        st.dump.finish(&rebuilds);
    }
    report
}

/// A prefix replay's shared dump lines and V's world at its end (strict
/// view): the lines are written at the head of every pair that branches
/// there.
pub(crate) struct PrefixV1 {
    pub lines: Vec<serde_json::Value>,
    pub world: super::transcript::WorldV1,
}

impl PrefixV1 {
    pub(crate) fn new(viewer: PlayerId, tag: &str) -> Option<Self> {
        (!tag.is_empty() && super::transcript::DumpV1::enabled()).then(|| Self {
            lines: Vec::new(),
            world: super::transcript::WorldV1::new(viewer),
        })
    }

    /// Records decision `k` of the shared prefix answered by `row`.
    pub(crate) fn push(&mut self, k: usize, s: &FastActorSessionV1, row: usize) {
        let FastActorResponseV1::Decision(d) = s.current_response() else {
            return;
        };
        let w = self.world.decision(s);
        let c = self.world.raw_candidate(row);
        self.world.advance(row);
        let a = format!("{:?}", d.acting_player);
        self.lines.push(
            serde_json::json!({"k": k, "prefix": true, "a": [a, a], "w": [w, w],
                                           "c": [c, c]}),
        );
    }

    /// The dump of a pair that branches at decision `k` (the next one, at
    /// `s`): W answers row `a`, W' row `b`.
    pub(crate) fn branch(
        &self,
        name: &str,
        header: serde_json::Value,
        k: usize,
        s: &FastActorSessionV1,
        a: usize,
        b: usize,
    ) -> StrictPairV1 {
        let mut pair = self.pair(name, header);
        let FastActorResponseV1::Decision(d) = s.current_response() else {
            return pair;
        };
        let w = pair.w.decision(s);
        let c = [pair.w.raw_candidate(a), pair.w.raw_candidate(b)];
        pair.w2 = pair.w.clone();
        pair.w.advance(a);
        pair.w2.advance(b);
        let who = format!("{:?}", d.acting_player);
        pair.dump
            .line(&serde_json::json!({"k": k, "branch": true, "a": [who, who],
                                           "w": [w, w], "c": c}));
        pair
    }

    /// The dump of one pair branching here.
    pub(crate) fn pair(&self, name: &str, header: serde_json::Value) -> StrictPairV1 {
        let mut dump = super::transcript::DumpV1::open(name, header);
        for l in &self.lines {
            dump.line(l);
        }
        StrictPairV1 {
            dump,
            w: self.world.clone(),
            w2: self.world.clone(),
        }
    }
}

/// A fresh pair dump from the game start.
pub(crate) fn fresh_pair(
    tag: &str,
    name: &str,
    header: serde_json::Value,
    viewer: PlayerId,
) -> Option<StrictPairV1> {
    (!tag.is_empty() && super::transcript::DumpV1::enabled()).then(|| StrictPairV1 {
        dump: super::transcript::DumpV1::open(name, header),
        w: super::transcript::WorldV1::new(viewer),
        w2: super::transcript::WorldV1::new(viewer),
    })
}

/// Objects V never tracked during `game` (whole prefix), per owner.
pub(crate) fn never_seen(game: &GameV1, viewer: PlayerId) -> Vec<ObjectId> {
    let mut s = game.start.clone();
    for chosen in &game.actions {
        let FastActorResponseV1::Decision(d) = s.current_response() else {
            break;
        };
        let Some(a) = menu(&s).iter().position(|x| x == chosen) else {
            break;
        };
        s.step(d.episode_id, d.step, a as u32).expect("replay");
    }
    let state = s.game_state();
    let p = super::projection::project_v1(state, state.observation_journal.notes(), viewer);
    state
        .objects
        .iter()
        .filter(|(id, o)| {
            !o.v4.is_token
                && o.spell_copy_origin.is_none()
                && p.objects[id.0 as usize].spans.is_empty()
                && matches!(o.zone, Zone::Library | Zone::Hand)
        })
        .map(|(id, _)| id)
        .collect()
}

/// F-deal: swap the definitions of two opponent cards V never sees.
pub(crate) fn f_deal(
    game: &GameV1,
    viewer: PlayerId,
    mutants: &[MutantV1],
    tag: &str,
) -> Option<PairReportV1> {
    let opp = viewer.opponent();
    let hidden = never_seen(game, viewer);
    let start = game.start.game_state();
    let pick: Vec<ObjectId> = hidden
        .iter()
        .copied()
        .filter(|id| start.objects.get(*id).owner == opp)
        .collect();
    let a = *pick.first()?;
    let b = *pick
        .iter()
        .find(|id| start.objects.get(**id).card_def != start.objects.get(a).card_def)?;
    let mut st = start.clone();
    swap_objects(&mut st, a, b);
    let w2 = FastActorSessionV1::import_infoset_v1(&game.start, st, 0);
    let header =
        serde_json::json!({"family": "F-deal", "tag": tag, "viewer": format!("{viewer:?}")});
    let strict = fresh_pair(tag, &format!("{tag}-fdeal"), header, viewer);
    Some(compare_worlds(
        game.start.clone(),
        w2,
        &game.actions,
        viewer,
        mutants,
        strict,
    ))
}

/// F-prov: swap the deal slots (library positions at the deal) of two
/// same-name opponent cards V never sees before both are hidden. Only their
/// provenance (native ids) differs.
pub(crate) fn f_prov(
    game: &GameV1,
    viewer: PlayerId,
    mutants: &[MutantV1],
    tag: &str,
) -> Option<PairReportV1> {
    let opp = viewer.opponent();
    let start = game.start.game_state();
    let library = &start.players[opp.index()].library;
    let (i, j) = (0..library.len()).find_map(|i| {
        (i + 1..library.len())
            .find(|&j| {
                start.objects.get(library[i]).card_def == start.objects.get(library[j]).card_def
            })
            .map(|j| (i, j))
    })?;
    let mut st = start.clone();
    st.players[opp.index()].library.swap(i, j);
    let w2 = FastActorSessionV1::import_infoset_v1(&game.start, st, 0);
    let header =
        serde_json::json!({"family": "F-prov", "tag": tag, "viewer": format!("{viewer:?}")});
    let strict = fresh_pair(tag, &format!("{tag}-fprov"), header, viewer);
    Some(compare_worlds_with(
        game.start.clone(),
        w2,
        &game.actions,
        viewer,
        mutants,
        true,
        usize::MAX,
        strict,
    ))
}

/// F-slot: at decision `at`, swap two of V's unknown slots of one owner.
pub(crate) fn f_slot(
    game: &GameV1,
    viewer: PlayerId,
    at: usize,
    mutants: &[MutantV1],
    tag: &str,
) -> Option<PairReportV1> {
    if at >= game.actions.len() {
        // The game ended before decision `at`.
        return None;
    }
    let mut s = game.start.clone();
    let mut prefix = PrefixV1::new(viewer, tag);
    for (k, chosen) in game.actions.iter().take(at).enumerate() {
        let FastActorResponseV1::Decision(d) = s.current_response() else {
            return None;
        };
        let a = menu(&s).iter().position(|x| x == chosen)?;
        if let Some(prefix) = prefix.as_mut() {
            prefix.push(k, &s, a);
        }
        s.step(d.episode_id, d.step, a as u32).ok()?;
    }
    let state = s.game_state();
    let p = super::projection::project_v1(state, state.observation_journal.notes(), viewer);
    let opp = viewer.opponent();
    let pinned = super::export::contract_pins(state);
    let unknown: Vec<ObjectId> = state.players[opp.index()]
        .hand
        .iter()
        .chain(&state.players[opp.index()].library)
        .copied()
        .filter(|id| !p.tracked(*id) && !pinned.contains(id))
        .collect();
    // Prefer two occupants whose definitions and true incarnations both
    // differ (a drawn card and a library card), so L2 can show.
    let differ = |a: ObjectId, b: ObjectId, zcc: bool| {
        let (x, y) = (state.objects.get(a), state.objects.get(b));
        x.card_def != y.card_def && (!zcc || x.zone_change_count != y.zone_change_count)
    };
    let (a, b) = [true, false].iter().find_map(|&zcc| {
        unknown.iter().find_map(|&a| {
            unknown
                .iter()
                .find(|&&b| differ(a, b, zcc))
                .map(|&b| (a, b))
        })
    })?;
    let mut st = state.clone();
    // Whole objects change places: each keeps its definition and incarnation
    // and takes the other's slot (and zone).
    move_objects(&mut st, a, b);
    let w2 = FastActorSessionV1::import_infoset_v1(&s, st, 0);
    // Rebuild W the same way so both sessions share construction.
    let w = FastActorSessionV1::import_infoset_v1(&s, s.game_state().clone(), 0);
    let header = serde_json::json!({"family": "F-slot", "tag": tag, "at": at,
                                    "viewer": format!("{viewer:?}")});
    let strict = prefix.map(|p| p.pair(&format!("{tag}-fslot{at}"), header));
    Some(compare_worlds(
        w,
        w2,
        &game.actions[at..],
        viewer,
        mutants,
        strict,
    ))
}

/// AC3: the imported state's menu is in bijection with the live menu and a
/// random rollout from it runs to the end with zero engine faults.
pub(crate) fn ac3_check(
    live: &FastActorSessionV1,
    export: &InfosetV1,
    seed: u64,
) -> Result<(), String> {
    // At a combat scan root the context carries the scan record (amendment
    // 43.1), as the payload does; a surface the payload cannot carry refuses
    // (R2) and has nothing to check.
    let context = match live.surface_context_v1() {
        Some(c) => c,
        None => match live.payload_surface_context_v1(&super::order::canonical_ids_v1(export)) {
            Some(c) => c,
            None => return Ok(()),
        },
    };
    let imported = FastActorSessionV1::import_infoset_with_surface_v1(
        live,
        export.state.clone(),
        0,
        Some(context),
    );
    let (FastActorResponseV1::Decision(d), FastActorResponseV1::Decision(e)) =
        (live.current_response(), imported.current_response())
    else {
        super::tests::dump("ac3-halt-true", live.game_state());
        super::tests::dump("ac3-halt-export", &export.state);
        super::tests::dump("ac3-halt-handles", &export.handles);
        return Err(format!(
            "the imported root is not a decision: {:?}",
            imported.current_response()
        ));
    };
    if d.legal_action_count != e.legal_action_count || d.acting_player != e.acting_player {
        return Err(format!(
            "menu size {} vs {}",
            d.legal_action_count, e.legal_action_count
        ));
    }
    // V's observation and menu, with viewer-local ids, are identical.
    let ids: BTreeMap<u64, u64> = export
        .tracked_natives
        .iter()
        .enumerate()
        .map(|(n, id)| (u64::from(id.0), n as u64))
        .collect();
    if let (Some(a), Some(b)) = (ac3_view(live, Some(&ids)), ac3_view(&imported, None)) {
        if a != b {
            return Err(format!(
                "the imported view differs at {:?}",
                super::tests::diffs(&a, &b, String::new(), 8)
            ));
        }
    }
    if menu(live).len() != menu(&imported).len() {
        return Err("menu length".into());
    }
    let mut s = imported;
    let mut rng = SplitMix64::seed(seed);
    for _ in 0..30_000 {
        let d = match s.current_response() {
            FastActorResponseV1::Decision(d) => d,
            FastActorResponseV1::Terminal(t) => {
                // A natural end or the session's decision limit; never a
                // halt (an engine fault).
                if t.terminal_classification != crate::rl::TerminalClassificationV1::Halted {
                    return Ok(());
                }
                return Err(format!("rollout halted: {}", t.terminal_reason));
            }
        };
        let a = (rng.next_u64() % u64::from(d.legal_action_count)) as u32;
        s.step(d.episode_id, d.step, a)
            .map_err(|e| format!("rollout fault: {e:?}"))?;
    }
    Err("the rollout did not reach a terminal".into())
}

/// Opponent choices made while resolving an effect: the kinds whose
/// alternatives can stay hidden from V (put-back sets, scry and surveil
/// orders, look-and-bottom, search picks).
fn hidden_choice_kind(kind: &str) -> bool {
    matches!(
        kind,
        "choose_effect_target"
            | "choose_effect_option"
            | "finish_effect_selection"
            | "choose_cost_target"
    )
}

/// F-choice on an engine game: at up to `points` opponent effect choices,
/// W' takes up to `alternatives` other candidates of the same kind, then
/// both replay W's later actions by semantic match. Only alternatives that
/// keep V's views equal give compared decisions (the rest truncate at once);
/// each pair compares at most `limit` decisions.
pub(crate) fn f_choice(
    game: &GameV1,
    viewer: PlayerId,
    mutants: &[MutantV1],
    points: usize,
    alternatives: usize,
    limit: usize,
    tag: &str,
) -> (Vec<PairReportV1>, u64) {
    let opp = viewer.opponent();
    let mut out = Vec::new();
    let mut tried = 0u64;
    let mut used = 0;
    let mut s = game.start.clone();
    let mut prefix = PrefixV1::new(viewer, tag);
    let kind_of = |x: &[u8]| -> String {
        serde_json::from_slice::<serde_json::Value>(x)
            .ok()
            .and_then(|v| v["action_kind"].as_str().map(str::to_string))
            .unwrap_or_default()
    };
    for (i, chosen) in game.actions.iter().enumerate() {
        let FastActorResponseV1::Decision(d) = s.current_response() else {
            break;
        };
        let m = menu(&s);
        let Some(a) = m.iter().position(|x| x == chosen) else {
            break;
        };
        let k = kind_of(chosen.as_slice());
        if used < points && seat(d.acting_player) == opp && hidden_choice_kind(&k) {
            let alts: Vec<usize> = (0..m.len())
                .filter(|&b| b != a && kind_of(m[b].as_slice()) == k)
                .take(alternatives)
                .collect();
            if !alts.is_empty() {
                used += 1;
            }
            for b in alts {
                let (mut w, mut w2) = (s.clone(), s.clone());
                if w.step(d.episode_id, d.step, a as u32).is_err()
                    || w2.step(d.episode_id, d.step, b as u32).is_err()
                {
                    continue;
                }
                tried += 1;
                let header = serde_json::json!({"family": "F-choice", "tag": tag, "at": i,
                    "alternative": b, "viewer": format!("{viewer:?}")});
                let strict = prefix
                    .as_ref()
                    .map(|p| p.branch(&format!("{tag}-fchoice{i}-{b}"), header, i, &s, a, b));
                let r = compare_worlds_with(
                    w,
                    w2,
                    &game.actions[i + 1..],
                    viewer,
                    mutants,
                    false,
                    limit,
                    strict,
                );
                if r.compared > 0 {
                    out.push(r);
                }
            }
        }
        if let Some(prefix) = prefix.as_mut() {
            prefix.push(i, &s, a);
        }
        s.step(d.episode_id, d.step, a as u32).expect("replay");
    }
    (out, tried)
}

/// Candidates of the current decision that cast a copy V tracks in the
/// opponent's hand, grouped by the copy's definition: (definition, menu
/// indices in menu order).
pub(crate) fn tracked_hand_casts(s: &FastActorSessionV1, viewer: PlayerId) -> Vec<(u64, Vec<usize>)> {
    let opp = viewer.opponent();
    let state = s.game_state();
    let projection =
        super::projection::project_v1(state, state.observation_journal.notes(), viewer);
    let sem = s.diagnostic_current_action_semantics().unwrap_or_default();
    let mut groups: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
    for (i, x) in sem.iter().enumerate() {
        let v = serde_json::to_value(x).expect("semantics serialize");
        if v["action_kind"] != "cast_spell" {
            continue;
        }
        let (Some(arena), Some(def)) = (
            v["source"]["arena_id"].as_u64(),
            v["source"]["card_db_id"].as_u64(),
        ) else {
            continue;
        };
        let id = ObjectId(arena as u32);
        let o = state.objects.get(id);
        if o.zone == Zone::Hand && o.owner == opp && projection.tracked(id) {
            groups.entry(def).or_default().push(i);
        }
    }
    groups.into_iter().filter(|(_, g)| g.len() >= 2).collect()
}

/// A continuation from `w`: up to `n` decisions answered uniformly at random
/// (seeded), as semantic menu rows.
fn continuation(w: &FastActorSessionV1, seed: u64, n: usize) -> Vec<Vec<u8>> {
    let mut s = w.clone();
    let mut rng = SplitMix64::seed(seed);
    let mut out = Vec::new();
    for _ in 0..n {
        let FastActorResponseV1::Decision(d) = s.current_response() else {
            break;
        };
        let a = (rng.next_u64() % u64::from(d.legal_action_count)) as u32;
        out.push(menu(&s)[a as usize].clone());
        if s.step(d.episode_id, d.step, a).is_err() {
            break;
        }
    }
    out
}

/// F-cast (lead ruling 7 (b), amendment 43.4): at an opponent decision whose
/// menu casts two copies of one card that V tracks in the opponent's hand
/// (revealed, fetched and revealed, or returned from a public zone), W casts
/// one and W' the other. V's bot-facing transcript gives the opponent's hand
/// cards no id, so V may not be able to tell which copy left; the strict
/// checker decides. W takes the recorded choice when it is one of the copies
/// (the game's own actions continue), else the first copy, and both worlds
/// continue with a seeded random continuation from W.
pub(crate) fn f_cast(
    game: &GameV1,
    viewer: PlayerId,
    mutants: &[MutantV1],
    points: usize,
    limit: usize,
    tag: &str,
) -> (Vec<PairReportV1>, u64) {
    let opp = viewer.opponent();
    let mut out = Vec::new();
    let (mut tried, mut used) = (0u64, 0usize);
    let mut s = game.start.clone();
    let mut prefix = PrefixV1::new(viewer, tag);
    for (i, chosen) in game.actions.iter().enumerate() {
        let FastActorResponseV1::Decision(d) = s.current_response() else {
            break;
        };
        let m = menu(&s);
        let Some(a) = m.iter().position(|x| x == chosen) else {
            break;
        };
        if used < points && seat(d.acting_player) == opp {
            for (_, group) in tracked_hand_casts(&s, viewer) {
                used += 1;
                let x = if group.contains(&a) { a } else { group[0] };
                let Some(&y) = group.iter().find(|&&b| b != x) else {
                    continue;
                };
                let (mut w, mut w2) = (s.clone(), s.clone());
                if w.step(d.episode_id, d.step, x as u32).is_err()
                    || w2.step(d.episode_id, d.step, y as u32).is_err()
                {
                    continue;
                }
                tried += 1;
                let actions = if x == a {
                    game.actions[i + 1..].to_vec()
                } else {
                    continuation(&w, 0xfca5 ^ i as u64, 300)
                };
                let header = serde_json::json!({"family": "F-cast", "tag": tag, "at": i,
                    "copies": [x, y], "recorded": x == a, "viewer": format!("{viewer:?}")});
                let strict = prefix
                    .as_ref()
                    .map(|p| p.branch(&format!("{tag}-fcast{i}-{y}"), header, i, &s, x, y));
                let r = compare_worlds_with(w, w2, &actions, viewer, mutants, false, limit, strict);
                out.push(r);
            }
        }
        if let Some(prefix) = prefix.as_mut() {
            prefix.push(i, &s, a);
        }
        s.step(d.episode_id, d.step, a as u32).expect("replay");
    }
    (out, tried)
}

/// The first byte difference of two exports, with a window on each side.
pub(crate) fn byte_window(x: &[u8], y: &[u8]) -> String {
    let at = x
        .iter()
        .zip(y)
        .position(|(u, v)| u != v)
        .unwrap_or(x.len().min(y.len()));
    let window = |b: &[u8]| {
        String::from_utf8_lossy(&b[at.saturating_sub(160)..(at + 120).min(b.len())]).to_string()
    };
    format!(
        "lengths {} and {}, first difference at byte {at}:\n  W  ...{}...\n  W' ...{}...",
        x.len(),
        y.len(),
        window(x),
        window(y)
    )
}

fn tally(reports: &[PairReportV1], totals: &mut BTreeMap<String, u64>) -> (u64, u64) {
    let (mut compared, mut truncated) = (0, 0);
    for r in reports {
        assert_eq!(
            r.p_differs, 0,
            "the correct producer differs across worlds: {r:?}"
        );
        compared += r.compared;
        truncated += u64::from(r.truncated);
        for (m, n) in &r.witnesses {
            *totals.entry(m.clone()).or_default() += n;
        }
    }
    (compared, truncated)
}

#[test]
fn ac1_f_deal_and_f_slot_on_engine_games() {
    let mutants = super::fixtures::LEAKAGE_V1;
    let mut totals: BTreeMap<String, u64> = BTreeMap::new();
    let mut compared = 0;
    let mut truncated = 0;
    for (decks, seed) in [
        (["CawGates", "Faeries"], 11u64),
        (["Wildfire", "Burn"], 12),
        (["Spy", "Elves"], 13),
    ] {
        let game = record_game(decks, seed, 300);
        for viewer in [PlayerId::P0, PlayerId::P1] {
            let mut reports = Vec::new();
            reports.extend(f_deal(&game, viewer, &mutants, ""));
            reports.extend(f_prov(&game, viewer, &mutants, ""));
            for at in [20, 60, 120] {
                reports.extend(f_slot(&game, viewer, at, &mutants, ""));
            }
            let (c, t) = tally(&reports, &mut totals);
            compared += c;
            truncated += t;
        }
    }
    eprintln!(
        "AC1 engine-game families (F-deal, F-prov, F-slot): compared {compared}, truncated pairs {truncated}, witnesses {totals:?}"
    );
    assert!(compared > 0);
}

/// The AC1 engine-family games (deck pair, seed).
pub(crate) const ENGINE_FAMILY_GAMES: [([&str; 2], u64); 6] = [
    (["CawGates", "Faeries"], 11),
    (["Wildfire", "Burn"], 12),
    (["Spy", "Elves"], 13),
    (["CawGates", "Faeries"], 31),
    (["Terror", "CawGates"], 32),
    (["Faeries", "Terror"], 33),
];

/// A dump tag for one game and viewer.
fn game_tag(family: &str, decks: [&str; 2], seed: u64, viewer: PlayerId) -> String {
    format!("{family}-{}-{}-{seed}-p{}", decks[0], decks[1], viewer.0)
}

/// G9 (fairness gate): L13 over the AC1 engine families. Here F-deal, F-prov
/// and F-slot on every engine-family game (F-choice runs in
/// `ac1_f_choice_over_every_engine_family_game`), with every leakage mutant.
/// A witness is a compared decision where P agrees across the worlds and L13
/// does not. Ruling 2 (c) accepts the written vacuity argument provisionally:
/// no witness, no cross-world P difference, departures reported. With
/// `W1_STRICT_DUMP_DIR` set, every pair is dumped for the strict view
/// (rulings 4 and 5), whose checker applies the same criteria to the
/// decisions it compares, F-choice included.
#[test]
fn l13_over_the_ac1_engine_families() {
    let mutants = super::fixtures::LEAKAGE_V1;
    let (mut compared, mut witnesses, mut departs) = (0u64, 0u64, 0u64);
    let (mut p_differs, mut p_equal) = (0u64, 0u64);
    let mut notes: Vec<String> = Vec::new();
    for (decks, seed) in ENGINE_FAMILY_GAMES {
        let game = record_game(decks, seed, 300);
        for viewer in [PlayerId::P0, PlayerId::P1] {
            let tag = game_tag("fam", decks, seed, viewer);
            let mut reports = Vec::new();
            reports.extend(f_deal(&game, viewer, &mutants, &tag));
            reports.extend(f_prov(&game, viewer, &mutants, &tag));
            for at in [20, 60, 120] {
                reports.extend(f_slot(&game, viewer, at, &mutants, &tag));
            }
            for (i, r) in reports.iter().enumerate() {
                for note in &r.notes {
                    notes.push(format!("{decks:?} seed {seed} {viewer:?} pair {i}: {note}"));
                }
                compared += r.compared;
                p_differs += r.p_differs;
                p_equal += r.compared - r.p_differs;
                witnesses += r.witnesses.get("L13").copied().unwrap_or(0);
                departs += r.departs.get("L13").copied().unwrap_or(0);
            }
        }
    }
    eprintln!(
        "L13 over F-deal, F-prov and F-slot on the AC1 engine-family games: compared {compared} ({p_equal} with P equal across the worlds), witnesses {witnesses}, decisions where L13 departs from P {departs}"
    );
    assert!(compared > 0);
    for note in notes.iter().filter(|n| n.contains("L13")) {
        eprintln!("L13 diagnostics: {note}");
    }
    assert_eq!(p_differs, 0, "the correct producer differs across worlds");
    assert!(p_equal > 0, "no decision with P equal across the worlds");
    assert_eq!(
        witnesses, 0,
        "an L13 witness contradicts the vacuity argument"
    );
}

/// Ruling 3, permanent AC1 witness: on Wildfire vs Burn (seed 12), viewer
/// p0, the first two F-choice pairs once exported `engine.event_history`
/// naming a tracked object C:25 in one world and C:24 in the other (and
/// C:29 / C:30), because `C:` handles followed S's zone-list order. Handles
/// now follow span start in V's transcript; P must agree across the worlds.
///
/// Ruling 4: the former view (V's current observation with decision-local
/// ids) is weaker than V's transcript, so these pairs compared decisions that
/// are not V-equivalent. The strict checker reads their dumps (written by
/// `ac1_f_choice_over_every_engine_family_game`) and requires P equal at
/// every decision whose transcript is equal; here the pairs must exist.
#[test]
fn ac1_f_choice_wildfire_burn_handle_order_witness() {
    let game = record_game_with(["Wildfire", "Burn"], 12, 300, false);
    let (reports, _) = f_choice(
        &game,
        PlayerId::P0,
        &super::fixtures::LEAKAGE_V1,
        3,
        2,
        40,
        "",
    );
    assert!(reports.len() >= 2, "the witness pairs exist: {reports:?}");
    for r in &reports[..2] {
        assert!(r.compared > 0, "the witness pair compares decisions: {r:?}");
    }
}

/// Amendment 43.3, permanent AC1 witness (lead ruling 6): on Wildfire vs
/// Burn (seed 12), viewer p0, at decision 93 Burn pays Fireblast's cost by
/// sacrificing its two Mountains, in opposite orders in W and W' (candidate
/// 0 is W's alternative). V's bot-facing records are byte-equal after the
/// sacrifice (two Mountain moves to the graveyard, each under a fresh id),
/// but P once named the sacrificed copies by their battlefield spans
/// (`event_history[214].Sacrificed.object` C:34 vs C:19). Under 43.3 each
/// Mountain's span ends at its unlinked move, the graveyard copies take new
/// handles where V receives them, and the earlier references become
/// tombstones: P must agree across the worlds, while mutant L4 (raw history
/// links) must not.
#[test]
fn ac1_wildfire_burn_fireblast_sacrifice_order_witness() {
    // The witness was found without neutral choices; it stays there.
    let game = record_game_with(["Wildfire", "Burn"], 12, 300, false);
    let viewer = PlayerId::P0;
    let at = 93;
    let mut s = game.start.clone();
    for chosen in &game.actions[..at] {
        let FastActorResponseV1::Decision(d) = s.current_response() else {
            panic!("the game ends before decision {at}");
        };
        let a = menu(&s).iter().position(|x| x == chosen).expect("replay");
        s.step(d.episode_id, d.step, a as u32).expect("replay");
    }
    let FastActorResponseV1::Decision(d) = s.current_response() else {
        panic!("no decision {at}");
    };
    assert_eq!(
        seat(d.acting_player),
        viewer.opponent(),
        "Burn chooses at {at}"
    );
    let m = menu(&s);
    let a = m
        .iter()
        .position(|x| *x == game.actions[at])
        .expect("W's choice");
    let b = 0;
    assert_ne!(a, b, "W' takes the other sacrifice order");
    let kind =
        |x: &[u8]| serde_json::from_slice::<serde_json::Value>(x).unwrap()["action_kind"].clone();
    assert_eq!(
        kind(&m[a]),
        kind(&m[b]),
        "both are the same hidden-choice kind"
    );
    let (mut w, mut w2) = (s.clone(), s.clone());
    w.step(d.episode_id, d.step, a as u32).expect("W");
    w2.step(d.episode_id, d.step, b as u32).expect("W'");
    let r = compare_worlds_with(
        w,
        w2,
        &game.actions[at + 1..],
        viewer,
        &super::fixtures::LEAKAGE_V1,
        false,
        6,
        None,
    );
    eprintln!("Fireblast witness: {r:?}");
    assert!(r.compared > 0, "the witness pair compares decisions: {r:?}");
    assert_eq!(
        r.p_differs, 0,
        "P differs across the sacrifice orders: {r:?}"
    );
    // The raw-history mutant names the copies through their earlier spans
    // (the tapped Mountain's Tap event), so it must tell the orders apart.
    assert!(
        r.witnesses.get("L4").copied().unwrap_or(0) > 0,
        "L4 differs across the sacrifice orders: {r:?}"
    );
}

/// Amendment 43.3 invariant (and the Sol review's regression): a span never
/// contains a zone change, so every tracked object's current span starts at
/// its current generation, however V came to follow it (a split, an observed
/// arrival, a reveal in place). With no live or linked reference below it,
/// the exported generation is then 1 for every tracked object, and cannot
/// tell a copy V followed before from one it did not (the paired cleanup
/// discard of a revealed and an unknown copy).
#[test]
fn spans_start_at_the_current_generation() {
    let mut checked = 0u64;
    for (decks, seed) in ENGINE_FAMILY_GAMES {
        let game = record_game(decks, seed, 300);
        let mut s = game.start.clone();
        for chosen in &game.actions {
            let FastActorResponseV1::Decision(d) = s.current_response() else {
                break;
            };
            let state = s.game_state();
            for v in [PlayerId::P0, PlayerId::P1] {
                let p = super::projection::project_v1(state, state.observation_journal.notes(), v);
                for (id, o) in state.objects.iter() {
                    if !p.tracked(id) {
                        continue;
                    }
                    let span = p.current_span(id).expect("a tracked object has a span");
                    assert_eq!(
                        span.start_gen, o.zone_change_count,
                        "{decks:?} seed {seed} viewer {v:?}: object {} span starts at {} but its generation is {}",
                        id.0, span.start_gen, o.zone_change_count
                    );
                    assert!(
                        span.cover_gen + 1 >= span.start_gen && span.cover_gen <= span.start_gen
                    );
                    checked += 1;
                }
            }
            let a = menu(&s).iter().position(|x| x == chosen).expect("replay");
            s.step(d.episode_id, d.step, a as u32).expect("replay");
        }
    }
    eprintln!("spans start at the current generation: {checked} tracked objects checked");
    assert!(checked > 0);
}

/// Ruling 3 (3): the F-choice family over every deck pair and seed of the
/// AC1 engine families, both viewers. The counts printed are the former
/// view's; rulings 4 and 5 make V's whole transcript the test of
/// V-equivalence, which the strict checker applies to this test's dumps
/// (`W1_STRICT_DUMP_DIR`), requiring 0 cross-world P differences among the
/// decisions it compares.
#[test]
fn ac1_f_choice_over_every_engine_family_game() {
    let (mut compared, mut differs, mut truncated, mut pairs) = (0u64, 0u64, 0u64, 0usize);
    let mut notes = Vec::new();
    for (decks, seed) in ENGINE_FAMILY_GAMES {
        let game = record_game(decks, seed, 300);
        for viewer in [PlayerId::P0, PlayerId::P1] {
            let tag = game_tag("fchoice", decks, seed, viewer);
            let (reports, _) =
                f_choice(&game, viewer, &super::fixtures::LEAKAGE_V1, 3, 2, 40, &tag);
            for r in &reports {
                pairs += 1;
                compared += r.compared;
                differs += r.p_differs;
                truncated += u64::from(r.truncated);
                for n in &r.notes {
                    notes.push(format!("{decks:?} seed {seed} {viewer:?}: {n}"));
                }
            }
        }
    }
    for n in notes.iter().filter(|n| n.contains("P differs")) {
        eprintln!("F-choice diagnostics: {n}");
    }
    eprintln!(
        "F-choice over every engine-family game, former view: pairs {pairs}, compared {compared}, cross-world P differences {differs}, truncated pairs {truncated}"
    );
    assert!(compared > 0);
}

#[test]
fn ac1_f_choice_on_engine_games() {
    let mutants = super::fixtures::LEAKAGE_V1;
    let mut totals: BTreeMap<String, u64> = BTreeMap::new();
    let (mut compared, mut truncated, mut pairs, mut tried) = (0, 0, 0, 0);
    for (decks, seed) in [
        (["CawGates", "Faeries"], 31u64),
        (["Terror", "CawGates"], 32),
        (["Faeries", "Terror"], 33),
    ] {
        let game = record_game(decks, seed, 300);
        for viewer in [PlayerId::P0, PlayerId::P1] {
            let (reports, n) = f_choice(&game, viewer, &mutants, 3, 2, 40, "");
            tried += n;
            pairs += reports.len();
            let (c, t) = tally(&reports, &mut totals);
            compared += c;
            truncated += t;
        }
    }
    eprintln!(
        "AC1 engine-game F-choice: alternatives {tried}, pairs with compared decisions {pairs}, compared {compared}, truncated pairs {truncated}, witnesses {totals:?}"
    );
    assert!(tried > 0 && pairs > 0 && compared > 0, "F-choice exercised");
    // Hidden choices change which objects history names: the raw-history
    // mutant must show it.
    assert!(
        totals.get("L4").copied().unwrap_or(0) > 0,
        "L4 has F-choice witnesses"
    );
}

/// Lead ruling 7 (c): L2 is vacuous. P fixes every unpinned `U:` occupant at
/// generation 1 (6.5's static assertion), so mutant L2 (unknown-slot
/// occupants keep their true incarnation) either exports P's own bytes or
/// trips that assertion and refuses R2, the same way in both worlds of any
/// pair. Checked at every decision of the AC1 engine-family games, both
/// viewers. If unpinned `U:` generations ever vary, or L2 ever delivers a
/// payload P does not, this test fails and L2 needs measured witnesses.
#[test]
fn l2_is_vacuous_unpinned_unknown_generations_are_fixed_at_one() {
    let l2 = ExportOptionsV1 {
        mutant: Some(MutantV1::L2),
    };
    let (mut exports, mut occupants, mut l2_refusals, mut l2_equal) = (0u64, 0u64, 0u64, 0u64);
    for (decks, seed) in ENGINE_FAMILY_GAMES {
        let game = record_game(decks, seed, 300);
        let mut s = game.start.clone();
        for chosen in &game.actions {
            let FastActorResponseV1::Decision(d) = s.current_response() else {
                break;
            };
            let viewer = seat(d.acting_player);
            if d.legal_action_count >= 2 {
                if let Ok(x) = export_at(&s, viewer, ExportOptionsV1::default()) {
                    exports += 1;
                    for (id, o) in x.state.objects.iter() {
                        if let HandleV1::U { pinned: false, .. } = x.handles[id.0 as usize] {
                            occupants += 1;
                            assert_eq!(
                                o.zone_change_count, 1,
                                "{decks:?} seed {seed}: an unpinned U: occupant at generation {}",
                                o.zone_change_count
                            );
                        }
                    }
                    match export_at(&s, viewer, l2) {
                        Ok(m) => {
                            assert_eq!(
                                m.bytes, x.bytes,
                                "{decks:?} seed {seed}: L2 exports a payload P does not"
                            );
                            l2_equal += 1;
                        }
                        Err(e) => {
                            assert_eq!(e.code_name(), "R2", "{decks:?} seed {seed}: {e:?}");
                            let ExportErrorV1::Refused { detail, .. } = &e else {
                                unreachable!()
                            };
                            assert!(
                                detail.contains("generation 1"),
                                "{decks:?} seed {seed}: L2 refuses for another reason: {detail}"
                            );
                            l2_refusals += 1;
                        }
                    }
                }
            }
            let a = menu(&s).iter().position(|x| x == chosen).expect("replay");
            s.step(d.episode_id, d.step, a as u32).expect("replay");
        }
    }
    eprintln!(
        "L2 vacuity: {exports} exports, {occupants} unpinned U: occupants all at generation 1; L2 refuses R2 (static assertion) at {l2_refusals}, equals P at {l2_equal}"
    );
    assert!(exports > 0 && occupants > 0);
}

#[test]
fn ac3_menu_and_rollout_smoke_from_exported_states() {
    let mut checked = 0;
    super::tests::play(["CawGates", "Faeries"], 21, 300, |s, v| {
        if let Ok(x) = export_at(s, v, ExportOptionsV1::default()) {
            ac3_check(s, &x, 7).expect("AC3");
            checked += 1;
        }
    });
    assert!(checked > 0, "no exported decision to check");
}
