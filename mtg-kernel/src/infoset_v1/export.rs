//! The canonical export (W1 design v7, 6.2 steps 1 to 11).

use super::ledger::{AllocationErrorV1, NameKeyV1, NamesV1};
use super::pins::{allocate_pins, allocate_pins_independent_v4, PinCapacityV1, PinV1};
use super::projection::{is_public, project_v1, ProjectionV1, SpanV1};
use super::rewrite::{has_tag, tagged_value, ClassV1, RefV1, ResolvedV1, WalkerV1};
use super::{ExportErrorV1, FaultCodeV1, HandleV1, RefusalCodeV1, INFOSET_SCHEMA_V1};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{
    Counters, GameObject, GameState, HandKnowledgeEntry, LibraryKnowledgeEntry, ObjectStateV4,
    ObservationNoteV1, Zone,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// The seed every exported state's future random stream starts from.
pub const INFOSET_RNG_SEED_V1: u64 = 0x5731_5f69_6e66_6f73;

/// The isolated mutants of AC1 (design v7, section 13). Each changes
/// exactly one rule; the harness runs them on both worlds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub enum MutantV1 {
    /// No refill: unknown slots keep their true definitions.
    L1,
    /// Unknown-slot occupants keep their true incarnation.
    L2,
    /// Public objects ordered by their raw native ids.
    L3,
    /// Raw history references: no tombstones for objects that have a handle.
    L4,
    /// Alpha-renaming only: references link across spans.
    L5,
    /// Pinned objects at their true slot, zone and generation.
    L6,
    /// Synthetic refusal that reads the opponent's true hand.
    L7,
    /// Count facts bound to the true remaining copies.
    L8,
    /// v4's independent per-pin placement.
    L9,
    /// v4's creation-time incarnation formula.
    L10,
    /// Tracker-rule facts only, no survivor tree (v5's refill).
    L11,
    /// v6's allocator: tracked cards and pins count toward nodes.
    L12,
    /// v6's true-identity join test instead of R1b.
    L13,
    /// The rule before amendment 43.4 (harness only, lead ruling 7 (b)):
    /// tracked objects in the opponent's hand take `C:` handles by span
    /// start and the hand lists them in fact order. Its witnesses are the
    /// pairs that differed before the fix.
    L14,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ExportOptionsV1 {
    pub mutant: Option<MutantV1>,
}

pub struct ExportInputV1<'a> {
    pub state: &'a GameState,
    pub journal: &'a [ObservationNoteV1],
    pub viewer: PlayerId,
    /// The public decklists (card definitions), indexed by owner.
    pub decklists: [&'a [u16]; 2],
    /// Where V's decisions fell in the committed stream (event-history
    /// length, journal length), oldest first: the bounds of the batches V's
    /// bot-facing transcript receives (lead ruling 7 (a)). `None` when the
    /// caller has no session: the whole stream is one batch (fail closed).
    pub marks: Option<&'a [(usize, usize)]>,
}

/// The export: the canonical state, its bytes and its handle table.
#[derive(Clone, Debug)]
pub struct InfosetV1 {
    pub state: GameState,
    pub bytes: Vec<u8>,
    pub sha256: String,
    /// One handle per canonical arena index.
    pub handles: Vec<HandleV1>,
    /// Harness diagnostics only, never in the payload: the native object
    /// behind each `C:` handle, in handle order.
    pub(crate) tracked_natives: Vec<ObjectId>,
}

/// R3's detail when the opponent's library-search plan is pending (design
/// 6.7): the bridge's wire subclass `opponent`.
pub(crate) const R3_OPPONENT_DETAIL: &str = "opponent library-search plan pending";
/// R3b's detail (amendment 43.2): V's own pending library search, decided
/// by the pending effect's owner and chooser; the wire subclass
/// `own_library_search`.
pub(crate) const R3B_OWN_DETAIL: &str = "own library-search plan pending";
/// R3's detail when a pending library search is neither the opponent's plan
/// nor V's own readable one (fail closed: the wire subclass `plan_error`,
/// a C5 fault on the entrant side).
pub(crate) const R3_UNREADABLE_DETAIL: &str = "library-search plan unreadable";

/// R1's detail for a live reference to one of two or more same-name objects
/// that left together in one batch of V's transcript (lead ruling 7 (a)):
/// the bridge's wire subclass `same_name_batch`, a C5 fault.
pub(crate) const R1_SAME_NAME_BATCH_DETAIL: &str =
    "a live reference names one of two same-name objects that left in one batch V cannot order";

/// R1's detail for a live reference that crosses the opponent's hand seam
/// (amendment 43.8, lead ruling 13): a card of its object's name entered or
/// left the opponent's hand in V's transcript after the reference's
/// creation bound, so V cannot tell which copy it names. The wire subclass
/// `same_name_hand`, a C5 fault.
pub(crate) const R1_SAME_NAME_HAND_DETAIL: &str =
    "a live reference names one of two same-name copies in the opponent's hand that V cannot tell apart";

fn refuse(code: RefusalCodeV1, detail: impl Into<String>) -> ExportErrorV1 {
    ExportErrorV1::refuse(code, detail)
}

fn fault(code: FaultCodeV1, detail: impl Into<String>) -> ExportErrorV1 {
    ExportErrorV1::fault(code, detail)
}

fn card_name(def: u16) -> &'static str {
    crate::card_def::CARD_DEFS
        .get(def as usize)
        .map_or("", |c| c.name)
}

fn name_key(def: u16) -> NameKeyV1 {
    (card_name(def), def)
}

fn multiset(defs: impl IntoIterator<Item = u16>) -> NamesV1 {
    let mut m = NamesV1::new();
    for d in defs {
        *m.entry(d).or_default() += 1;
    }
    m
}

fn subtract(pool: &mut NamesV1, defs: impl IntoIterator<Item = u16>) -> Result<(), u16> {
    for d in defs {
        match pool.get_mut(&d) {
            Some(c) if *c > 0 => {
                *c -= 1;
                if *c == 0 {
                    pool.remove(&d);
                }
            }
            _ => return Err(d),
        }
    }
    Ok(())
}

/// Contract-pinned objects in contract order: stack position bottom to top,
/// then the resolving item; within an item by contract kind.
pub(crate) fn contract_pins(state: &GameState) -> Vec<ObjectId> {
    let mut out = Vec::new();
    let items = state.stack.iter().chain(
        state
            .engine
            .pending_effect
            .as_ref()
            .map(|pending| &pending.resolving_item),
    );
    for item in items {
        let v4 = &item.v4;
        let mut push = |id: ObjectId| {
            if !out.contains(&id) {
                out.push(id);
            }
        };
        if let Some(c) = &v4.ability_source_contract {
            push(c.source);
        }
        if let Some(c) = &v4.source_contract {
            push(c.source);
        }
        if let Some(c) = &v4.madness_source_contract {
            push(c.source);
        }
        if let Some(c) = &v4.granted_by {
            push(c.source);
        }
        if let Some(grant) = &v4.cauldron_grant.0 {
            push(grant.host.source);
            push(grant.donor.source);
        }
    }
    out
}

/// Amendment 43.5 (lead ruling 8): whether a waiting stack item's
/// resolution reads none of its paid-cost objects. Fail closed: only an
/// activated ability whose whole effect is one generic op that never
/// consults the paid-cost record (in `effect.rs` only
/// `GainLifeEqualToPaidCostManaValue` and
/// `DealDamageToTargetEqualToChosenCostCreaturePower` read it), with no
/// captured last-known power, no payment by its own source, no grant and
/// no catalog-owned payment record (`fixed_source_damage_v3`).
fn paid_costs_unread_v1(state: &GameState, item: &crate::state::StackItem) -> bool {
    use crate::effect::EffectOp;
    let source = item.v4.ability_source_contract.map(|c| c.source);
    item.kind == crate::state::StackItemKind::ActivatedAbility
        && source == Some(item.source)
        && item.v4.granted_by.is_none()
        && item.v4.cauldron_grant.0.is_none()
        && matches!(
            item.inline_effect,
            Some(
                EffectOp::GainLife { .. }
                    | EffectOp::DamageAllCreatures { .. }
                    | EffectOp::DealDamage { .. }
            )
        )
        && item
            .v4
            .paid_cost_refs
            .iter()
            .all(|p| p.power_lki.is_none() && p.object != item.source)
        && !crate::fixed_source_damage_v3::tracked(state, item)
}

/// Amendment 43.5: the paid-cost records (object, epoch) whose live
/// references may become 6.9 tombstones for viewer `v`. Every live
/// occurrence of the record (anywhere outside the event history and the
/// history-like records) must sit in a waiting stack item whose resolution
/// reads none of its paid objects ([`paid_costs_unread_v1`]), with the
/// payment public to V. Any other occurrence (a pending resolution's
/// context, a pending trigger, a nested record) keeps R1.
pub(crate) fn unread_paid_costs_v1(
    state: &GameState,
    tagged: &Value,
    v: PlayerId,
) -> BTreeSet<(ObjectId, u32)> {
    type OccurrenceV1 = (String, Option<ObjectId>, Option<u64>);
    fn scan(value: &Value, path: &str, out: &mut Vec<OccurrenceV1>) {
        match value {
            Value::Object(m) => {
                for (k, x) in m {
                    let at = format!("{path}.{k}");
                    if k == "paid_cost_refs" {
                        for e in x.as_array().into_iter().flatten() {
                            out.push((
                                at.clone(),
                                super::rewrite::parse_tag(&e["object"]),
                                e["zone_change_count"].as_u64(),
                            ));
                        }
                    }
                    scan(x, &at, out);
                }
            }
            Value::Array(xs) => {
                for (i, x) in xs.iter().enumerate() {
                    scan(x, &format!("{path}.{i}"), out);
                }
            }
            _ => {}
        }
    }
    let mut occurrences = Vec::new();
    if let Some(map) = tagged.as_object() {
        for (k, x) in map {
            if HISTORY_RECORDS_V1.contains(&k.as_str()) {
                continue;
            }
            if k == "engine" {
                for (ek, ex) in x.as_object().into_iter().flatten() {
                    if ek != "event_history" && ek != "event_log" {
                        scan(ex, &format!("engine.{ek}"), &mut occurrences);
                    }
                }
                continue;
            }
            scan(x, k, &mut occurrences);
        }
    }
    let bit = 1u8 << v.index();
    let unread_at = |path: &str, object: ObjectId, g: u64| -> bool {
        let Some(i) = path
            .strip_prefix("stack.")
            .and_then(|rest| rest.strip_suffix(".v4.paid_cost_refs"))
            .and_then(|i| i.parse::<usize>().ok())
        else {
            return false;
        };
        state.stack.get(i).is_some_and(|item| {
            paid_costs_unread_v1(state, item)
                && item.v4.paid_cost_refs.iter().any(|p| {
                    p.object == object
                        && u64::from(p.zone_change_count) == g
                        && p.visible_to_mask & bit != 0
                })
        })
    };
    let mut keys: BTreeMap<(ObjectId, u32), bool> = BTreeMap::new();
    for (path, object, g) in occurrences {
        // A record the scan cannot read: nothing becomes a tombstone.
        let (Some(object), Some(g)) = (object, g.and_then(|g| u32::try_from(g).ok())) else {
            return BTreeSet::new();
        };
        let ok = unread_at(&path, object, u64::from(g));
        *keys.entry((object, g)).or_insert(true) &= ok;
    }
    keys.into_iter()
        .filter_map(|(k, ok)| ok.then_some(k))
        .collect()
}

/// The number of history-like records `rewrite_state` numbers (the
/// transient event log's entries, then one per history record present):
/// amendment 43.5's paid-cost tombstones are numbered after them.
fn record_count_v1(tagged: &Value) -> usize {
    let log = tagged["engine"]["event_log"].as_array().map_or(0, Vec::len);
    let records = tagged.as_object().map_or(0, |m| {
        m.keys()
            .filter(|k| HISTORY_RECORDS_V1.contains(&k.as_str()))
            .count()
    });
    log + records
}

/// An unknown hidden slot (step 3).
#[derive(Clone, Copy, Debug)]
struct SlotV1 {
    owner: PlayerId,
    zone: Zone,
    /// Index in the exported zone vector.
    index: usize,
    /// The true occupant (mutants and invariants only).
    native: ObjectId,
}

/// What fills an unknown slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FillV1 {
    Pin { object: ObjectId, h: u32 },
    KnownName(u16),
    Refill(u16),
}

/// Step 1: invariants on S. A failure is an engine fault.
fn invariants(
    state: &GameState,
    projection: &ProjectionV1,
    decklists: [&[u16]; 2],
) -> Result<(), ExportErrorV1> {
    // I3: card-pool predicates, constant false on the board's eight decks.
    if crate::bloodthirsty_conqueror_v49::has_history(state)
        || crate::soul_counters_v1::has_history(state)
        || state.extra_turn_v1.is_some()
        || crate::linked_aura_operation_v1::pending_original_operation(state).is_some()
        || crate::engine::time_stop_v1::sampler_conflicts_v68(state, &[]).unwrap_or(true)
        || crate::engine::muldrotha_v1::sampler_conflicts_v71(state, &[]).unwrap_or(true)
        || crate::engine::tinybones_v72::sampler_conflicts_v72(state, &[]).unwrap_or(true)
        || crate::effect::liliana_dreadhorde_v1::sampler_conflicts_v73(state, &[]).unwrap_or(true)
        || crate::effect::chandra_flameshaper_v1::sampler_conflicts_v74(state, &[]).unwrap_or(true)
        || crate::effect::kaito_cunning_v75::sampler_conflicts_v75(state, &[]).unwrap_or(true)
    {
        return Err(fault(FaultCodeV1::I3, "a card-pool predicate holds"));
    }
    for owner in [PlayerId::P0, PlayerId::P1] {
        let p = &state.players[owner.index()];
        let mut seen = BTreeSet::new();
        for &id in p.hand.iter().chain(&p.library) {
            // I2: no duplicate ids among hidden positions.
            if !seen.insert(id) {
                return Err(fault(
                    FaultCodeV1::I2,
                    format!("object {} appears twice", id.0),
                ));
            }
            if projection.tracked(id) {
                continue;
            }
            // I1: the pool is pristine (sampler.rs:662-680).
            let obj = state.objects.get(id);
            if obj.owner != owner
                || obj.spell_copy_origin.is_some()
                || !obj.attachments.is_empty()
                || obj.v4.attached_to.is_some()
                || obj.controller != owner
                || obj.tapped
                || obj.summoning_sick
                || obj.damage != 0
                || obj.counters != Counters::default()
                || obj.plotted_turn.is_some()
                || obj.v4.is_token
                || obj.v4 != ObjectStateV4::from_card_def(obj.card_def)
            {
                return Err(fault(
                    FaultCodeV1::I1,
                    format!("hidden object {} is not pristine", id.0),
                ));
            }
        }
        // I4: observed multiplicities within the public decklist.
        let observed = state
            .objects
            .iter()
            .filter(|(id, o)| {
                o.owner == owner
                    && !o.v4.is_token
                    && o.spell_copy_origin.is_none()
                    && projection.tracked(*id)
            })
            .map(|(_, o)| o.card_def);
        let mut deck = multiset(decklists[owner.index()].iter().copied());
        if let Err(d) = subtract(&mut deck, observed) {
            return Err(fault(
                FaultCodeV1::I4,
                format!(
                    "{} copies of {} observed exceed the decklist",
                    owner.0,
                    card_name(d)
                ),
            ));
        }
    }
    Ok(())
}

/// R5: physical facts against the native V-observer rows, both ways.
fn knowledge_check(state: &GameState, projection: &ProjectionV1) -> Result<(), ExportErrorV1> {
    let v = projection.viewer;
    let opp = v.opponent();
    if !projection.inconsistencies.is_empty() {
        return Err(refuse(
            RefusalCodeV1::R5,
            projection.inconsistencies.join("; "),
        ));
    }
    // Amendment 43.8: V's facts about the opponent's hand are names, never
    // copies; the check is soundness by name (L and C within the true hand),
    // which only a projection defect can fail. The engine's copy-level rows
    // may claim more than V's conservative readings.
    let mut hand = multiset(
        state.players[opp.index()]
            .hand
            .iter()
            .map(|id| state.objects.get(*id).card_def),
    );
    if subtract(&mut hand, projection.hand_facts.iter().map(|f| f.card_def)).is_err() {
        return Err(refuse(
            RefusalCodeV1::R5,
            "hand facts claim a card the hand does not hold",
        ));
    }
    for owner in [PlayerId::P0, PlayerId::P1] {
        let projected: BTreeSet<(u32, u32, u32)> = projection.library[owner.index()]
            .iter()
            .map(|(p, id)| (*p, id.0, state.objects.get(*id).zone_change_count))
            .collect();
        let native: BTreeSet<(u32, u32, u32)> = state
            .known_library_cards(v, owner)
            .iter()
            .map(|k| (k.position, k.object.0, k.zone_change_count))
            .collect();
        if projected != native {
            return Err(refuse(
                RefusalCodeV1::R5,
                format!("library facts of {} differ from native rows", owner.0),
            ));
        }
    }
    Ok(())
}

/// Canonical order of tracked objects (6.2 step 7).
/// The tracked objects in `C:` handle order (design 6.2 steps 2 and 7;
/// ruling 3): by the start of their current span in V's transcript, keyed
/// (record index, position within that record as presented to V). Nothing
/// here reads S's zone-list, arena or id order. Two spans that start in the
/// same record without a V-presented position cannot be ordered from V's
/// transcript: the export refuses (R2, fail closed) rather than guess.
///
/// Amendment 43.4 (lead ruling 7 (b)): the tracked objects in the
/// opponent's hand (`opp_hand`, already in their 43.4 order) are not keyed by
/// span start. The bot-facing transcript gives them no id, so which of two
/// same-name copies remains after one leaves is not V's information; they
/// follow every other tracked object, in the order given.
fn tracked_order(
    state: &GameState,
    projection: &ProjectionV1,
    l3: bool,
    opp_hand: Option<&[ObjectId]>,
) -> Result<Vec<ObjectId>, ExportErrorV1> {
    let mut keyed: Vec<((usize, u32), ObjectId)> = state
        .objects
        .iter()
        .filter(|(id, _)| projection.tracked(*id))
        .filter(|(id, _)| l3 || opp_hand.is_none_or(|h| !h.contains(id)))
        .map(|(id, _)| {
            let key = projection
                .current_span(id)
                .map_or((usize::MAX, u32::MAX), |s| (s.start_item, s.start_pos));
            (key, id)
        })
        .collect();
    if l3 {
        // Mutant L3: raw native ids order the public objects.
        let mut out: Vec<ObjectId> = keyed.into_iter().map(|(_, id)| id).collect();
        out.sort();
        return Ok(out);
    }
    keyed.sort_by_key(|(key, _)| *key);
    if keyed.windows(2).any(|w| w[0].0 == w[1].0) {
        return Err(refuse(
            RefusalCodeV1::R2,
            "two tracked objects start their spans in one record without a V-presented order",
        ));
    }
    let mut out: Vec<ObjectId> = keyed.into_iter().map(|(_, id)| id).collect();
    out.extend(opp_hand.unwrap_or(&[]).iter().copied());
    Ok(out)
}

/// Amendment 43.8 (lead ruling 13, rule 2): whether a live reference to an
/// opponent-owned object crosses the hand seam, and if so its creation
/// bound, a stream item. A reference with epochs, every one of which V
/// tracked its object at, does not cross (43.3's spans, R1b and R1 decide
/// it). For an epoch V did not track: the object's own cast receipt, or a
/// reference inside an item the object activated from the hand and paid
/// with itself, is a record its departure wrote, bounded by that departure
/// when V saw it (where V's unbroken tracking after it began); every other
/// such epoch is bounded by game start (which hidden copy it names, and
/// when that copy entered the hand, is not V's information). With no
/// epoch: the stack item it sits in bounds it, from the span at which V
/// tracked the item's source at the epoch the item records (or the source's
/// current span when the item records none; a self-paid item: its
/// departure), inside a pending resolution its item's, inside this turn's
/// combat state the turn's start; else game start. The item's own `source`
/// field, when V tracked the source there, does not cross; nor does a
/// reference whose object's current span began at or before the bound. A
/// zone's membership list never crosses. Several epochs take the earliest
/// bound.
pub(crate) fn seam_bound_v1(projection: &ProjectionV1, r: &RefV1) -> Option<usize> {
    if r.membership {
        return None;
    }
    if r.generations.is_empty() {
        let bound = match r.container {
            Some((source, g)) => {
                let span = match g {
                    Some(g) => projection.tracked_at(source, g),
                    None => projection.current_span(source),
                };
                match (span, g) {
                    // The item's own `source` field names the object at the
                    // epoch the item records, which V tracked: as a reference
                    // to that epoch, it does not cross.
                    (Some(_), _) if source == r.object => return None,
                    (Some(s), _) => s.start_item,
                    // An item its source activated from the hand and paid
                    // with itself: bounded by that departure, its receipt.
                    (None, Some(g)) if r.item_self_paid == Some((source, g)) => {
                        projection.left_seen(source, g).unwrap_or(0)
                    }
                    _ => 0,
                }
            }
            // This turn's until-end-of-turn effects: the turn's start.
            None if r.turn_scoped => projection.turn_starts.last().copied().unwrap_or(0),
            // The announcement in progress: made after the stream so far,
            // except its own cost payments.
            None if r.announcing => projection.items,
            None => 0,
        };
        // V has followed the object's current incarnation (its current
        // span, a transcript entity; no chain through splits, Sol design
        // review 2) since before the reference could exist.
        if projection
            .current_span(r.object)
            .is_some_and(|s| s.start_item <= bound)
        {
            return None;
        }
        return Some(bound);
    }
    let mut bound: Option<usize> = None;
    for &g in &r.generations {
        if projection.tracked_at(r.object, g).is_some() {
            continue;
        }
        let receipt = r.cast_receipt || r.item_self_paid == Some((r.object, g));
        let b = receipt
            .then(|| projection.left_seen(r.object, g))
            .flatten()
            .unwrap_or(0);
        bound = Some(bound.map_or(b, |x: usize| x.min(b)));
    }
    bound
}

/// The exported state of one viewer at one decision.
pub(crate) fn export_v1(
    input: &ExportInputV1<'_>,
    options: ExportOptionsV1,
) -> Result<InfosetV1, ExportErrorV1> {
    let state = input.state;
    let v = input.viewer;
    let opp = v.opponent();
    let mutant = options.mutant;
    let is = |m: MutantV1| mutant == Some(m);

    // Step 2 (the projection feeds step 1's I1 and I4).
    let projection = project_v1(state, input.journal, v);
    // Step 1.
    invariants(state, &projection, input.decklists)?;
    if is(MutantV1::L7) {
        // Harness sensitivity: a refusal that reads the opponent's true hand.
        let hand = &state.players[opp.index()].hand;
        if hand
            .first()
            .is_some_and(|id| state.objects.get(*id).card_def.is_multiple_of(2))
        {
            return Err(refuse(RefusalCodeV1::L7, "synthetic"));
        }
    }
    knowledge_check(state, &projection)?;
    // R3: a pending library-search plan owned by the opponent (design 6.7).
    // R3b (amendment 43.2): V's own pending library search, decided by the
    // pending effect's owner and chooser (the planner reads it for V), never
    // inferred from another refusal. Any other pending search the planner
    // cannot read refuses fail closed under R3's own detail (C5).
    use crate::effect::library_choice_search_v2::plan_future_v3;
    match plan_future_v3(state, opp) {
        Ok(None) => {}
        Ok(Some(_)) => return Err(refuse(RefusalCodeV1::R3, R3_OPPONENT_DETAIL)),
        Err(_) => match plan_future_v3(state, v) {
            Ok(Some(_)) => return Err(refuse(RefusalCodeV1::R3b, R3B_OWN_DETAIL)),
            Ok(None) | Err(_) => return Err(refuse(RefusalCodeV1::R3, R3_UNREADABLE_DETAIL)),
        },
    }

    // Step 3: unknown slots per owner, hand then library.
    let mut slots: Vec<SlotV1> = Vec::new();
    let mut opp_hand_tracked: Vec<ObjectId> = Vec::new();
    for owner in [PlayerId::P0, PlayerId::P1] {
        let p = &state.players[owner.index()];
        if owner != v {
            // Amendments 43.7 and 43.8: no opponent-hand object is
            // tracked, so none takes a `C:` handle; V's facts (names) become
            // known-name occupants below. Mutants L14 (the rule before 43.4:
            // known copies tracked, in fact order) and L8 (count facts too,
            // invented continuity) bind facts to the true copies of their
            // name, in hand order: mutants may read the truth.
            let bind = |facts: &mut dyn Iterator<Item = u16>, out: &mut Vec<ObjectId>| {
                for def in facts {
                    if let Some(&id) = p
                        .hand
                        .iter()
                        .find(|id| state.objects.get(**id).card_def == def && !out.contains(id))
                    {
                        out.push(id);
                    }
                }
            };
            if is(MutantV1::L14) || is(MutantV1::L8) {
                bind(
                    &mut projection
                        .hand_facts
                        .iter()
                        .filter(|f| f.known)
                        .map(|f| f.card_def),
                    &mut opp_hand_tracked,
                );
            }
            if is(MutantV1::L8) {
                bind(
                    &mut projection
                        .hand_facts
                        .iter()
                        .filter(|f| !f.known)
                        .map(|f| f.card_def),
                    &mut opp_hand_tracked,
                );
            }
            let untracked: Vec<ObjectId> = p
                .hand
                .iter()
                .copied()
                .filter(|id| !opp_hand_tracked.contains(id))
                .collect();
            for (k, native) in untracked.into_iter().enumerate() {
                slots.push(SlotV1 {
                    owner,
                    zone: Zone::Hand,
                    index: opp_hand_tracked.len() + k,
                    native,
                });
            }
        }
        for (i, &native) in p.library.iter().enumerate() {
            if !projection.library[owner.index()].contains_key(&(i as u32)) {
                slots.push(SlotV1 {
                    owner,
                    zone: Zone::Library,
                    index: i,
                    native,
                });
            }
        }
    }

    // Live references (collect pass): R(o), R1, R1b, R2 inputs.
    let mut tagged = tagged_value(state);
    let mut refs: Vec<(RefV1, &'static str)> = Vec::new();
    {
        let mut record = |r: RefV1, region: &'static str| refs.push((r, region));
        collect_refs(&tagged, &mut record)?;
    }

    let pins_all = contract_pins(state);
    let pinned_untracked: Vec<ObjectId> = pins_all
        .iter()
        .copied()
        .filter(|id| !projection.tracked(*id))
        .collect();
    let in_slot: HashMap<ObjectId, usize> = slots
        .iter()
        .enumerate()
        .map(|(i, s)| (s.native, i))
        .collect();
    // Amendment 43.5 (lead ruling 8): a paid-cost reference to an object
    // that has since left V's tracking (no handle, no pin) becomes a 6.9
    // tombstone instead of an R1 refusal, when no pending resolution reads
    // the paid object and V saw the payment (V observed the paid epoch).
    // The record keeps its own fields; only the object becomes `H:`, at
    // generation 0. Otherwise R1 stays. R1b still reads such a reference
    // (Sol round 1): whether the paid copy is untracked can depend on which
    // same-name copy V later drew, so R1b's name-level join test must
    // refuse in both worlds before tracking decides the tombstone; so does
    // R1 `same_name_batch` below (Sol round 2).
    let unread_paid = unread_paid_costs_v1(state, &tagged, v);
    let paid_tomb = |r: &RefV1| -> bool {
        r.class == ClassV1::Live
            && r.paid_cost
            && !projection.tracked(r.object)
            && !pinned_untracked.contains(&r.object)
            && matches!(r.generations.as_slice(), [g]
                if projection.span_of(r.object, *g).is_some()
                    && unread_paid.contains(&(r.object, *g)))
    };

    // Lead ruling 7 (a): a live reference keeps its link across an unlinked
    // visible move (6.5, the Lembas table) only while V can tell which copy
    // moved. If another object of the same name and owner also left the
    // same zone (changing zone in V's sight, or leaving V's sight) in the
    // same batch of V's transcript (no V decision between the two), V cannot
    // pair the reference with either copy: refuse, fail closed. Reads V's
    // spans, the names and zones V saw and V's own decision points only.
    // It reads 43.5's tombstone-eligible paid-cost references too (Sol
    // round 2): which copy stays tracked after such a batch is not V's
    // information, so the batch test runs before tracking decides. Lead
    // ruling 11 (R1): it runs before R1b and the R1 untracked test, which
    // read tracking, so both worlds refuse with the same subclass.
    // Amendment 43.8 (lead ruling 13, rule 2): a live reference to an
    // opponent-owned object that crosses the hand seam ([`seam_bound_v1`])
    // refuses `same_name_hand` when a card of its name entered or left the
    // opponent's hand in V's transcript after the reference's creation
    // bound. It reads V's transcript only and runs before every test that
    // reads spans.
    for (r, region) in &refs {
        if *region != "live" {
            continue;
        }
        let o = state.objects.get(r.object);
        if o.owner != opp {
            continue;
        }
        let name = card_name(o.card_def);
        // Moves without committed events (an aborted cast back to the hand)
        // put a copy's native epochs ahead of the walk's, so a reference to
        // an earlier incarnation of it cannot be placed by the spans. The
        // engine reveals such a card in the hand; any reveal of a card of
        // the name in the opponent's hand that is not a search outcome may
        // be one (the wire cannot tell), so by name, after one, a live
        // reference to an incarnation of an opponent object of that name
        // before its current span (or with none) refuses alike (43.7's
        // desync rule, triggered from V's transcript; Sol design review 1,
        // finding 2).
        if !r.membership
            && projection.revealed.iter().any(|d| card_name(*d) == name)
            && r.generations.iter().any(|&g| {
                projection
                    .current_span(r.object)
                    .is_none_or(|s| g < s.start_gen)
            })
        {
            return Err(refuse(RefusalCodeV1::R1, R1_SAME_NAME_HAND_DETAIL));
        }
        if let Some(b) = seam_bound_v1(&projection, r) {
            if projection
                .seams
                .iter()
                .any(|(item, d)| *item > b && card_name(*d) == name)
            {
                return Err(refuse(RefusalCodeV1::R1, R1_SAME_NAME_HAND_DETAIL));
            }
        }
    }

    let bounds = projection.batch_bounds(input.marks);
    for (r, region) in &refs {
        if *region != "live" {
            continue;
        }
        let Some(track) = projection.objects.get(r.object.0 as usize) else {
            continue;
        };
        let o = state.objects.get(r.object);
        for &g in &r.generations {
            // Every end of the referenced object counts, a lapse (it left
            // V's sight) as much as a split (lead ruling 11, R1): a twin's
            // departure counts either way. The scan stops after the first
            // lapse: which later spans belong to the referenced copy is not
            // V's information there, and R1b decides.
            // From the span the reference resolves to (Sol round 7: an
            // older span ending at the same generation is history the
            // reference does not name); without one, every end at or after g.
            let from_span = projection.span_of(r.object, g).map(|s| s.start_item);
            let ends = track
                .spans
                .iter()
                .filter(|s| from_span.is_none_or(|start| s.start_item >= start))
                .filter_map(|s| s.end)
                .filter(|e| e.last_gen >= g);
            let mut lapsed = false;
            for end in ends {
                if lapsed {
                    break;
                }
                lapsed = !end.split;
                let batch = ProjectionV1::batch_of(&bounds, end.item);
                let twin = projection.objects.iter().enumerate().any(|(i, other)| {
                    let other_obj = state.objects.get(ObjectId(i as u32));
                    i != r.object.0 as usize
                        && other_obj.owner == o.owner
                        && card_name(other_obj.card_def) == card_name(o.card_def)
                        && other.spans.iter().filter_map(|s| s.end).any(|e| {
                            e.from == end.from && ProjectionV1::batch_of(&bounds, e.item) == batch
                        })
                });
                if twin {
                    return Err(refuse(RefusalCodeV1::R1, R1_SAME_NAME_BATCH_DETAIL));
                }
            }
        }
    }

    // R1b (a name-level, public-event predicate), then R1 and R2.
    for (r, region) in &refs {
        if *region != "live" {
            continue;
        }
        for &g in &r.generations {
            let Some(span) = projection.span_of(r.object, g) else {
                if projection.tracked(r.object) || pinned_untracked.contains(&r.object) {
                    return Err(refuse(
                        RefusalCodeV1::R2,
                        "a live reference to an unobserved epoch",
                    ));
                }
                continue;
            };
            // R1b reads where V's tracking lapsed: an amendment 43.3 split
            // (a visible zone change the bot cannot link) is no lapse, and a
            // live reference across one links as 6.5 maps it.
            if let Some(end) = projection.lapse_after(r.object, span) {
                let owner = state.objects.get(r.object).owner;
                let def = state.objects.get(r.object).card_def;
                let joined = if is(MutantV1::L13) {
                    // v6: refuse only if the referenced copy itself is back.
                    projection.tracked(r.object)
                } else {
                    projection
                        .arrivals
                        .iter()
                        .any(|a| a.item > end.item && a.owner == owner && a.card_def == def)
                };
                if joined {
                    return Err(refuse(
                        RefusalCodeV1::R1b,
                        format!(
                            "a live reference may join a later {} of its owner",
                            card_name(def)
                        ),
                    ));
                }
            }
        }
    }
    for (r, region) in &refs {
        if *region != "live" || projection.tracked(r.object) {
            continue;
        }
        if pinned_untracked.contains(&r.object) || paid_tomb(r) {
            continue;
        }
        // A live physical reference to an object V does not track: the
        // refusal reads only V's tracking, never the object's true zone.
        return Err(refuse(
            RefusalCodeV1::R1,
            "a live reference names an untracked object",
        ));
    }
    // Step 4: joint pinned allocation.
    let opp_hand_slots = slots
        .iter()
        .filter(|s| s.owner == opp && s.zone == Zone::Hand)
        .count();
    let opp_lib_slots = slots
        .iter()
        .filter(|s| s.owner == opp && s.zone == Zone::Library)
        .count();
    let tracked_defs = |owner: PlayerId| -> Vec<u16> {
        state
            .objects
            .iter()
            .filter(|(id, o)| {
                o.owner == owner
                    && !o.v4.is_token
                    && o.spell_copy_origin.is_none()
                    && (projection.tracked(*id)
                        || ((is(MutantV1::L8) || is(MutantV1::L14))
                            && opp_hand_tracked.contains(id)))
            })
            .map(|(_, o)| o.card_def)
            .collect()
    };
    let pinned_defs = |owner: PlayerId| -> Vec<u16> {
        pinned_untracked
            .iter()
            .filter(|id| state.objects.get(**id).owner == owner)
            .map(|id| state.objects.get(*id).card_def)
            .collect()
    };
    // Pool5: decklist minus public zones, tracked objects, pinned definitions.
    let mut pool5 = multiset(input.decklists[opp.index()].iter().copied());
    subtract(&mut pool5, tracked_defs(opp)).map_err(|d| {
        fault(
            FaultCodeV1::I4,
            format!("{} observed beyond the decklist", card_name(d)),
        )
    })?;
    subtract(&mut pool5, pinned_defs(opp)).map_err(|d| {
        refuse(
            RefusalCodeV1::R7,
            format!("pinned {} beyond the decklist", card_name(d)),
        )
    })?;
    // Amendment 43.8: the names V knows in the opponent's hand (L) are
    // known-name occupants, ordered by name only; they leave the pool before
    // the survivor allocation. Mutants L8 and L14 hold copies for them
    // instead.
    let mut mutant_bound = multiset(
        opp_hand_tracked
            .iter()
            .map(|id| state.objects.get(*id).card_def),
    );
    let mut bound_names: Vec<u16> = Vec::new();
    for f in projection.hand_facts.iter().filter(|f| f.known) {
        match mutant_bound.get_mut(&f.card_def) {
            Some(c) if *c > 0 => *c -= 1,
            _ => bound_names.push(f.card_def),
        }
    }
    bound_names.sort_by_key(|d| name_key(*d));
    subtract(&mut pool5, bound_names.iter().copied()).map_err(|d| {
        fault(
            FaultCodeV1::I4,
            format!("{} known in the hand beyond the decklist", card_name(d)),
        )
    })?;
    let survivors = &projection.survivors;
    let allocation = if is(MutantV1::L11) {
        // Tracker-rule facts only.
        Ok(projection
            .hand_facts
            .iter()
            .filter(|f| !f.known)
            .map(|f| f.card_def)
            .collect::<Vec<_>>())
    } else if is(MutantV1::L12) {
        allocate_counting_arrivals(
            &projection,
            state,
            &opp_hand_tracked,
            &pool5,
            opp_hand_slots,
        )
    } else {
        survivors.allocate(
            &pool5,
            opp_hand_slots.saturating_sub(bound_names.len()),
            &name_key,
        )
    };
    let known_names = match allocation {
        Ok(x) => {
            // Sol round 8: one order for every known-name occupant, by name
            // only, so whether a name came from a bound fact or from F
            // cannot move the payload.
            let mut all: Vec<u16> = bound_names.iter().copied().chain(x).collect();
            all.sort_by_key(|d| name_key(*d));
            all
        }
        Err(e) => {
            return Err(refuse(
                RefusalCodeV1::R8,
                match e {
                    AllocationErrorV1::Unrepresentable => "survivor tree unrepresentable",
                    AllocationErrorV1::NotLaminar => "survivor tree not laminar",
                    AllocationErrorV1::Unfillable => "survivor deficit cannot be filled",
                },
            ));
        }
    };
    if projection.r8_latch && !is(MutantV1::L11) {
        // Ruling 40.4's fallback for the draw-back gap of 6.4: a card put
        // back from a hand V knew about was drawn back (or such cards were
        // scried or reordered), so V may know more than the tree keeps;
        // refuse until the projection releases the latch.
        return Err(refuse(
            RefusalCodeV1::R8,
            "a put-back card from a hand V knew about was drawn back or reordered",
        ));
    }
    if survivors.tainted() && !is(MutantV1::L11) {
        return Err(refuse(
            RefusalCodeV1::R8b,
            "a public event located a survivor-node card outside the hand",
        ));
    }
    // R4: tracker-rule facts implied by the allocation.
    if !is(MutantV1::L11) && !is(MutantV1::L8) {
        let facts = multiset(
            projection
                .hand_facts
                .iter()
                .filter(|f| !f.known)
                .map(|f| f.card_def),
        );
        let placed = multiset(known_names.iter().copied());
        if facts
            .iter()
            .any(|(d, c)| placed.get(d).copied().unwrap_or(0) < *c)
        {
            return Err(refuse(
                RefusalCodeV1::R4,
                "a tracker-rule fact is not implied by F",
            ));
        }
    }
    let cap = PinCapacityV1 {
        hand_slots: opp_hand_slots,
        required_hand: known_names.len(),
        library_slots: opp_lib_slots,
    };
    let mut pin_zone: HashMap<ObjectId, (Zone, u32)> = HashMap::new();
    let opp_pins: Vec<ObjectId> = pinned_untracked
        .iter()
        .copied()
        .filter(|id| state.objects.get(*id).owner == opp)
        .collect();
    let pin_inputs: Vec<PinV1> = opp_pins
        .iter()
        .map(|id| {
            let end = projection.last_span(*id).and_then(|s| s.end);
            let (zone, since) = end.map_or((Zone::Library, 0), |e| (e.zone, e.item));
            PinV1 {
                last_zone: if zone == Zone::Hand {
                    Zone::Hand
                } else {
                    Zone::Library
                },
                since,
            }
        })
        .collect();
    let transfers: Vec<_> = projection
        .transfers
        .iter()
        .copied()
        .filter(|t| t.owner == opp)
        .collect();
    let zones = if is(MutantV1::L9) {
        allocate_pins_independent_v4(&pin_inputs, &transfers)
    } else {
        allocate_pins(&pin_inputs, &transfers, cap)
            .ok_or_else(|| refuse(RefusalCodeV1::R7, "no feasible joint pin allocation"))?
    };
    for ((id, pin), zone) in opp_pins.iter().zip(&pin_inputs).zip(&zones) {
        pin_zone.insert(*id, (*zone, u32::from(*zone != pin.last_zone)));
    }
    for id in pinned_untracked
        .iter()
        .filter(|id| state.objects.get(**id).owner == v)
    {
        // A V-owned untracked pin is in V's library: top-most own slot.
        pin_zone.insert(*id, (Zone::Library, 0));
    }

    // Steps 4 to 6: fill the slots.
    let mut fills: Vec<Option<FillV1>> = vec![None; slots.len()];
    for id in &pinned_untracked {
        let (zone, h) = pin_zone[id];
        let owner = state.objects.get(*id).owner;
        let target = if is(MutantV1::L6) {
            in_slot.get(id).copied()
        } else {
            (0..slots.len())
                .find(|&i| fills[i].is_none() && slots[i].owner == owner && slots[i].zone == zone)
        };
        let Some(i) = target else {
            return Err(refuse(
                RefusalCodeV1::R7,
                "no free slot for a pinned object",
            ));
        };
        fills[i] = Some(FillV1::Pin { object: *id, h });
    }
    for def in &known_names {
        let Some(i) = (0..slots.len())
            .find(|&i| fills[i].is_none() && slots[i].owner == opp && slots[i].zone == Zone::Hand)
        else {
            return Err(refuse(RefusalCodeV1::R8, "no hand slot for a survivor"));
        };
        fills[i] = Some(FillV1::KnownName(*def));
    }
    for owner in [PlayerId::P0, PlayerId::P1] {
        let mut pool = multiset(input.decklists[owner.index()].iter().copied());
        subtract(&mut pool, tracked_defs(owner))
            .and_then(|()| subtract(&mut pool, pinned_defs(owner)))
            .map_err(|d| {
                refuse(
                    RefusalCodeV1::R5,
                    format!("{} beyond the decklist", card_name(d)),
                )
            })?;
        if owner == opp {
            subtract(&mut pool, known_names.iter().copied())
                .map_err(|_| refuse(RefusalCodeV1::R8, "survivors beyond the pool"))?;
        }
        let mut sorted: Vec<u16> = Vec::new();
        for (d, c) in &pool {
            sorted.extend(std::iter::repeat_n(*d, *c as usize));
        }
        let free: Vec<usize> = (0..slots.len())
            .filter(|&i| fills[i].is_none() && slots[i].owner == owner)
            .collect();
        if free.len() != sorted.len() {
            return Err(refuse(
                RefusalCodeV1::R5,
                format!(
                    "refill pool of {} has {} cards for {} slots",
                    owner.0,
                    sorted.len(),
                    free.len()
                ),
            ));
        }
        for (i, def) in free.into_iter().zip(sorted) {
            let def = if is(MutantV1::L1) {
                state.objects.get(slots[i].native).card_def
            } else {
                def
            };
            fills[i] = Some(FillV1::Refill(def));
        }
    }
    let fills: Vec<FillV1> = fills
        .into_iter()
        .map(|f| f.expect("every slot filled"))
        .collect();

    // Step 7: handles. C first, then U per owner in step-3 order, then H.
    let mut tracked = tracked_order(
        state,
        &projection,
        is(MutantV1::L3),
        (!is(MutantV1::L14)).then_some(opp_hand_tracked.as_slice()),
    )?;
    if is(MutantV1::L8) || is(MutantV1::L14) {
        // Mutant L8's bound copies take `C:` handles (invented continuity);
        // L14 keeps the rule before 43.7 (bound copies tracked, fact order).
        for id in &opp_hand_tracked {
            if !tracked.contains(id) {
                tracked.push(*id);
            }
        }
    }
    let mut handles: Vec<HandleV1> = Vec::new();
    let mut new_id: HashMap<ObjectId, u32> = HashMap::new();
    for (n, id) in tracked.iter().enumerate() {
        new_id.insert(*id, handles.len() as u32);
        handles.push(HandleV1::C { n: n as u32 });
    }
    let mut slot_id: Vec<u32> = vec![0; slots.len()];
    for owner in [PlayerId::P0, PlayerId::P1] {
        let mut i_owner = 0u32;
        for (i, s) in slots.iter().enumerate() {
            if s.owner != owner {
                continue;
            }
            slot_id[i] = handles.len() as u32;
            let (known_name, pinned) = match fills[i] {
                FillV1::Pin { object, .. } => {
                    new_id.insert(object, handles.len() as u32);
                    (false, true)
                }
                FillV1::KnownName(_) => (true, false),
                FillV1::Refill(_) => (false, false),
            };
            handles.push(HandleV1::U {
                owner: owner.0,
                i: i_owner,
                known_name,
                pinned,
            });
            i_owner += 1;
        }
    }

    // Step 8: the canonical epoch path. p0(o) = min R(o) over live and
    // linked references, else the start of the current span.
    // Mutant L8's bound copies keep `C:` continuity: history references made
    // inside the copy's last span link to its handle, as if the count fact
    // were still that physical copy.
    let l8_bound: BTreeSet<ObjectId> = if is(MutantV1::L8) {
        opp_hand_tracked
            .iter()
            .copied()
            .filter(|id| !projection.tracked(*id))
            .collect()
    } else {
        BTreeSet::new()
    };
    let linkable = |r: &RefV1| -> bool {
        if l8_bound.contains(&r.object) {
            let Some(span) = projection.last_span(r.object) else {
                return false;
            };
            if !r.generations.is_empty() {
                return r
                    .generations
                    .iter()
                    .all(|&g| g >= span.start_gen && span.end.is_none_or(|end| g <= end.last_gen));
            }
            return match r.class {
                ClassV1::Event(e) => {
                    e >= span.start_event
                        && span
                            .end
                            .is_none_or(|end| projection.event_items[e] < end.item)
                }
                ClassV1::Record(_) => false,
                ClassV1::Live => true,
            };
        }
        let Some(span) = projection.current_span(r.object) else {
            return false;
        };
        if is(MutantV1::L5) {
            return true;
        }
        if !r.generations.is_empty() {
            return r.generations.iter().all(|&g| g >= span.start_gen);
        }
        match r.class {
            ClassV1::Event(e) => e >= span.start_event,
            ClassV1::Record(_) => false,
            ClassV1::Live => true,
        }
    };
    let mut p0: HashMap<ObjectId, u32> = HashMap::new();
    for (r, region) in &refs {
        // Mutant L4 links raw history references to current handles; its
        // epochs map on the same path, so they count toward p0 too.
        let raw_l4 =
            is(MutantV1::L4) && r.class != ClassV1::Live && !pinned_untracked.contains(&r.object);
        let counts = (*region == "live" && !paid_tomb(r))
            || (new_id.contains_key(&r.object) && (linkable(r) || raw_l4));
        if counts {
            for &g in &r.generations {
                let e = p0.entry(r.object).or_insert(g);
                *e = (*e).min(g);
            }
        }
    }
    let base = |id: ObjectId| -> u32 {
        let span_start = projection
            .current_span(id)
            .or_else(|| projection.last_span(id))
            .map_or(0, |s: &SpanV1| s.start_gen);
        p0.get(&id).copied().unwrap_or(span_start)
    };
    let phi = |id: ObjectId, g: u32| -> Option<u32> {
        let b = base(id);
        let g = if is(MutantV1::L10) {
            // v4: a reference's epoch is the object's epoch when the
            // reference was made, one observed move after a departure.
            let e_last = projection.objects[id.0 as usize].last_obs_gen;
            if g < e_last {
                g + 1
            } else {
                g
            }
        } else {
            g
        };
        g.checked_sub(b).map(|d| d + 1)
    };

    // Step 8 (rewrite) with tombstones for unlinkable history references.
    let tomb_base = handles.len() as u32;
    let mut tombstones: Vec<GameObject> = Vec::new();
    let template = |owner: PlayerId, def: u16, zone: Zone, zcc: u32| GameObject {
        card_def: def,
        name: card_name(def).to_string(),
        owner,
        controller: owner,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: zcc,
    };
    let unnamed_def = |owner: PlayerId| -> u16 {
        input.decklists[owner.index()]
            .iter()
            .copied()
            .min_by_key(|d| name_key(*d))
            .unwrap_or(0)
    };
    let event_kind_ok = |e: usize| -> bool {
        matches!(
            state.engine.event_history[e],
            crate::event::CommittedEvent::Draw { .. }
                | crate::event::CommittedEvent::ZoneChange { .. }
                | crate::event::CommittedEvent::SpellCast { .. }
                | crate::event::CommittedEvent::CreateToken { .. }
                // The public cast announcement right before SpellCast; an
                // unlinked reference in it becomes an unnamed tombstone.
                | crate::event::CommittedEvent::PublishedCastIdentityV1 { .. }
        )
    };
    let mut timestamps: BTreeSet<u64> = BTreeSet::new();
    let pinned_set: BTreeSet<ObjectId> = pinned_untracked.iter().copied().collect();
    // Amendment 43.5's tombstones are `H:<e>:<k>`: e numbers past every
    // event and history-like record, k counts them in walk order.
    let paid_e = (state.engine.event_history.len() + record_count_v1(&tagged)) as u32;
    let mut paid_k = 0u32;
    {
        let mut resolve = |r: RefV1| -> Result<ResolvedV1, ExportErrorV1> {
            let o = r.object;
            if paid_tomb(&r) {
                let def = state.objects.get(o).card_def;
                let id = tomb_base + tombstones.len() as u32;
                tombstones.push(template(state.objects.get(o).owner, def, Zone::Command, 0));
                handles.push(HandleV1::H {
                    e: paid_e,
                    j: paid_k,
                    named: true,
                });
                paid_k += 1;
                return Ok(Some((id, vec![0; r.generations.len()])));
            }
            let raw_history = is(MutantV1::L4) && r.class != ClassV1::Live;
            if let Some(&id) = new_id.get(&o) {
                let pin = pinned_set.contains(&o);
                if r.class == ClassV1::Live || linkable(&r) || (raw_history && !pin) {
                    let g = r
                        .generations
                        .iter()
                        .map(|&g| {
                            phi(o, g).ok_or_else(|| refuse(RefusalCodeV1::R2, "epoch below p0"))
                        })
                        .collect::<Result<Vec<u32>, _>>()?;
                    return Ok(Some((id, g)));
                }
            }
            if let (true, Some(&i)) = (raw_history, in_slot.get(&o)) {
                return Ok(Some((slot_id[i], vec![1; r.generations.len()])));
            }
            match r.class {
                ClassV1::Live => Err(refuse(
                    RefusalCodeV1::R1,
                    "a live reference names an object with no handle",
                )),
                ClassV1::Event(e) | ClassV1::Record(e) => {
                    if let ClassV1::Event(e) = r.class {
                        let item = projection.event_items[e];
                        let seen = projection.tracked_after(o, item)
                            && item > 0
                            && projection.tracked_after(o, item - 1);
                        if !event_kind_ok(e) && !seen {
                            return Err(refuse(
                                RefusalCodeV1::R2,
                                format!(
                                    "a {} event names a hidden object",
                                    serde_json::to_value(&state.engine.event_history[e])
                                        .ok()
                                        .and_then(|v| v
                                            .as_object()
                                            .and_then(|m| m.keys().next().cloned()))
                                        .unwrap_or_default()
                                ),
                            ));
                        }
                    }
                    let named = match r.class {
                        ClassV1::Event(e) => {
                            if event_kind_ok(e) {
                                projection.event_observed[e].contains(&o)
                            } else {
                                let item = projection.event_items[e];
                                projection.tracked_after(o, item)
                            }
                        }
                        _ => false,
                    };
                    let owner = state.objects.get(o).owner;
                    let def = if named {
                        state.objects.get(o).card_def
                    } else {
                        unnamed_def(owner)
                    };
                    let id = tomb_base + tombstones.len() as u32;
                    tombstones.push(template(owner, def, Zone::Command, 0));
                    handles.push(HandleV1::H {
                        e: match r.class {
                            ClassV1::Event(e) => e as u32,
                            _ => (state.engine.event_history.len() + e) as u32,
                        },
                        j: r.slot,
                        named,
                    });
                    Ok(Some((id, vec![0; r.generations.len()])))
                }
            }
        };
        let mut ts = |t: u64| -> Option<u64> {
            timestamps.insert(t);
            None
        };
        rewrite_state(&mut tagged, &mut resolve, &mut ts)?;
    }

    // Arena: C objects rewritten, U occupants fresh, H tombstones.
    let mut items: Vec<Value> = Vec::with_capacity(handles.len());
    let objects_tagged = tagged_value(&state.objects);
    let arena = objects_tagged["items"]
        .as_array()
        .ok_or_else(|| refuse(RefusalCodeV1::R2, "arena shape"))?;
    for id in &tracked {
        let mut value = arena[id.0 as usize].clone();
        let current = state.objects.get(*id).zone_change_count;
        {
            let mut resolve = |r: RefV1| -> Result<ResolvedV1, ExportErrorV1> {
                let Some(&nid) = new_id.get(&r.object) else {
                    return Err(refuse(
                        RefusalCodeV1::R1,
                        "a tracked object references an object with no handle",
                    ));
                };
                let g = r
                    .generations
                    .iter()
                    .map(|&g| {
                        phi(r.object, g).ok_or_else(|| refuse(RefusalCodeV1::R2, "epoch below p0"))
                    })
                    .collect::<Result<Vec<u32>, _>>()?;
                Ok(Some((nid, g)))
            };
            let mut ts = |t: u64| -> Option<u64> {
                timestamps.insert(t);
                None
            };
            let map = value
                .as_object_mut()
                .ok_or_else(|| refuse(RefusalCodeV1::R2, "object shape"))?;
            let own = phi(*id, current)
                .ok_or_else(|| refuse(RefusalCodeV1::R2, format!("current epoch of {}", id.0)))?;
            map.insert("zone_change_count".into(), Value::from(own));
            let mut walker = WalkerV1 {
                resolve: &mut resolve,
                timestamp: &mut ts,
                slot: 0,
                check_pairs: true,
                self_object: Some(*id),
                item_self_paid: None,
                container: None,
                membership: false,
                turn_scoped: false,
                announcing: false,
            };
            for (k, field) in map.iter_mut() {
                if k == "zone_change_count" {
                    continue;
                }
                walker.walk(field, ClassV1::Live, k)?;
            }
        }
        items.push(value);
    }
    for owner in [PlayerId::P0, PlayerId::P1] {
        for (i, s) in slots.iter().enumerate() {
            if s.owner != owner {
                continue;
            }
            let object = match fills[i] {
                FillV1::Pin { object, h } => {
                    let true_obj = state.objects.get(object);
                    let e_last = projection
                        .last_span(object)
                        .and_then(|sp| sp.end)
                        .map_or(true_obj.zone_change_count, |e| e.last_gen);
                    let zcc = if is(MutantV1::L6) {
                        true_obj.zone_change_count
                    } else {
                        phi(object, e_last).unwrap_or(1) + h
                    };
                    let zone = if is(MutantV1::L6) {
                        true_obj.zone
                    } else {
                        s.zone
                    };
                    template(owner, true_obj.card_def, zone, zcc)
                }
                FillV1::KnownName(def) | FillV1::Refill(def) => {
                    let zcc = if is(MutantV1::L2) {
                        state.objects.get(s.native).zone_change_count
                    } else {
                        1
                    };
                    template(owner, def, s.zone, zcc)
                }
            };
            items.push(serde_json::to_value(&object).expect("objects serialize"));
        }
    }
    for t in &tombstones {
        items.push(serde_json::to_value(t).expect("objects serialize"));
    }

    // Zone vectors and knowledge rows (step 9).
    let map = tagged
        .as_object_mut()
        .ok_or_else(|| refuse(RefusalCodeV1::R2, "state shape"))?;
    map.insert("objects".into(), json!({ "items": items }));
    // Every object a zone vector or knowledge row names has a handle; a
    // projection that tracks a fact's object without tracking the object
    // is a ledger bug, refused (R5) rather than exported.
    let handle_of = |id: &ObjectId| -> Result<u32, ExportErrorV1> {
        new_id.get(id).copied().ok_or_else(|| {
            refuse(
                RefusalCodeV1::R5,
                format!("object {} is placed without a handle", id.0),
            )
        })
    };
    for owner in [PlayerId::P0, PlayerId::P1] {
        let p = &state.players[owner.index()];
        let mut hand: Vec<u32> = Vec::new();
        if owner == v {
            for id in &p.hand {
                hand.push(handle_of(id)?);
            }
        } else {
            for id in &opp_hand_tracked {
                hand.push(handle_of(id)?);
            }
        }
        let mut library: Vec<Option<u32>> = vec![None; p.library.len()];
        for (pos, id) in &projection.library[owner.index()] {
            library[*pos as usize] = Some(handle_of(id)?);
        }
        for (i, s) in slots.iter().enumerate() {
            if s.owner != owner {
                continue;
            }
            match s.zone {
                Zone::Hand => {
                    if hand.len() != s.index {
                        return Err(refuse(RefusalCodeV1::R2, "hand slot order"));
                    }
                    hand.push(slot_id[i]);
                }
                _ => library[s.index] = Some(slot_id[i]),
            }
        }
        let library: Vec<u32> = library
            .into_iter()
            .map(|x| x.ok_or_else(|| refuse(RefusalCodeV1::R2, "library slot unfilled")))
            .collect::<Result<_, _>>()?;
        let pm = &mut map["players"][owner.index()];
        pm["hand"] = json!(hand);
        pm["library"] = json!(library);
    }
    // Knowledge rows: V-observer physical rows and synthetic bindings; the
    // opponent-observer rows are empty (R6 is a schema constant).
    let mut library_knowledge: [[Vec<LibraryKnowledgeEntry>; 2]; 2] = Default::default();
    let mut hand_knowledge: [[Vec<HandKnowledgeEntry>; 2]; 2] = Default::default();
    for owner in [PlayerId::P0, PlayerId::P1] {
        for (pos, id) in &projection.library[owner.index()] {
            library_knowledge[v.index()][owner.index()].push(LibraryKnowledgeEntry {
                position: *pos,
                object: ObjectId(handle_of(id)?),
                zone_change_count: phi(*id, state.objects.get(*id).zone_change_count).unwrap_or(1),
            });
        }
    }
    for id in &opp_hand_tracked {
        hand_knowledge[v.index()][opp.index()].push(HandKnowledgeEntry {
            object: ObjectId(handle_of(id)?),
            zone_change_count: phi(*id, state.objects.get(*id).zone_change_count).unwrap_or(1),
        });
    }
    for (i, f) in fills.iter().enumerate() {
        if let FillV1::KnownName(_) = f {
            hand_knowledge[v.index()][opp.index()].push(HandKnowledgeEntry {
                object: ObjectId(slot_id[i]),
                zone_change_count: 1,
            });
        }
    }
    hand_knowledge[v.index()][opp.index()].sort_by_key(|k| k.object);
    map.insert(
        "library_knowledge".into(),
        serde_json::to_value(&library_knowledge).expect("rows"),
    );
    map.insert(
        "hand_knowledge".into(),
        serde_json::to_value(&hand_knowledge).expect("rows"),
    );

    // Timestamps: rank 1..N with ties kept; next timestamp N + 1.
    let rank: BTreeMap<u64, u64> = timestamps
        .iter()
        .enumerate()
        .map(|(i, t)| (*t, i as u64 + 1))
        .collect();
    let next = rank.len() as u64 + 1;
    rewrite_timestamps(&mut tagged, &rank)?;
    tagged["engine"]["next_effect_timestamp"] = json!(next);

    // Step 11: static assertions, then the canonical state.
    if has_tag(&tagged) {
        return Err(refuse(
            RefusalCodeV1::R2,
            "a native id survived the rewrite",
        ));
    }
    let mut out: GameState = serde_json::from_value(tagged)
        .map_err(|e| refuse(RefusalCodeV1::R2, format!("the export does not load: {e}")))?;
    out.resample_future_randomness_for_search_v3(INFOSET_RNG_SEED_V1);
    crate::effect::refresh_binding_fingerprints_v1(&mut out);
    static_assertions(&out, &handles, v)?;
    let bytes = canonical_bytes(&out, &handles);
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    Ok(InfosetV1 {
        state: out,
        bytes,
        sha256,
        handles,
        tracked_natives: tracked,
    })
}

/// Diagnostics only (the bridge's `W1_INFOSET_REFUSAL_LOG`, never on the
/// wire): every live reference to an object V does not track, with the
/// object's true card, owner, zone and generation, V's last span of it, and
/// the JSON paths of the state where the reference sits. Reads S freely; it
/// explains a refusal after the fact and feeds nothing back.
pub(crate) fn refusal_diagnostics_v1(input: &ExportInputV1<'_>) -> Value {
    let state = input.state;
    let projection = project_v1(state, input.journal, input.viewer);
    let tagged = tagged_value(state);
    let mut refs: Vec<(RefV1, &'static str)> = Vec::new();
    {
        let mut record = |r: RefV1, region: &'static str| refs.push((r, region));
        if collect_refs(&tagged, &mut record).is_err() {
            return json!({"error": "collect pass refused"});
        }
    }
    let pins = contract_pins(state);
    fn paths(v: &Value, tag: &str, at: String, out: &mut Vec<String>) {
        match v {
            Value::String(s) if s == tag => out.push(at),
            Value::Array(xs) => {
                for (i, x) in xs.iter().enumerate() {
                    paths(x, tag, format!("{at}[{i}]"), out);
                }
            }
            Value::Object(m) => {
                for (k, x) in m {
                    if k == tag {
                        out.push(format!("{at}.<key>"));
                    }
                    paths(x, tag, format!("{at}.{k}"), out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for (r, region) in &refs {
        if *region != "live" || projection.tracked(r.object) || !seen.insert(r.object) {
            continue;
        }
        let o = state.objects.get(r.object);
        let tag = format!("{}{}", crate::ids::OBJECT_ID_TAG_PREFIX_V1, r.object.0);
        let mut at = Vec::new();
        paths(&tagged, &tag, "state".into(), &mut at);
        at.retain(|p| {
            !p.starts_with("state.engine.event_history") && !p.starts_with("state.objects")
        });
        at.truncate(12);
        let span = projection.last_span(r.object).map(|s| {
            json!({"start_item": s.start_item, "start_gen": s.start_gen,
                   "end": s.end.map(|e| json!({"item": e.item, "zone": format!("{:?}", e.zone),
                                               "last_gen": e.last_gen, "split": e.split}))})
        });
        out.push(json!({
            "object": r.object.0,
            "card": card_name(o.card_def),
            "owner": o.owner.0,
            "zone": format!("{:?}", o.zone),
            "generation": o.zone_change_count,
            "ref_generations": r.generations,
            "pinned": pins.contains(&r.object),
            "last_span": span,
            "paths": at,
        }));
    }
    // Lead ruling 9 (a): the items that hold paid-cost records, with their
    // kind, source card, effect and 43.5's verdict (log only).
    let holder = |where_: &str, i: usize, item: &crate::state::StackItem| {
        let effect = item
            .inline_effect
            .as_ref()
            .map(|e| format!("{e:?}").chars().take(240).collect::<String>());
        json!({
            "at": format!("{where_}[{i}]"),
            "kind": format!("{:?}", item.kind),
            "source_card": card_name(state.objects.get(item.source).card_def),
            "source_zone": format!("{:?}", state.objects.get(item.source).zone),
            "controller": item.controller.0,
            "inline_effect": effect,
            "granted": item.v4.granted_by.is_some(),
            "paid_cost_cards": item
                .v4
                .paid_cost_refs
                .iter()
                .map(|p| card_name(p.card_def))
                .collect::<Vec<_>>(),
            "unread_43_5": paid_costs_unread_v1(state, item),
        })
    };
    let mut holders: Vec<Value> = state
        .stack
        .iter()
        .enumerate()
        .filter(|(_, item)| !item.v4.paid_cost_refs.is_empty())
        .map(|(i, item)| holder("stack", i, item))
        .collect();
    if let Some(pending) = state.engine.pending_effect.as_ref() {
        if !pending.resolving_item.v4.paid_cost_refs.is_empty() {
            holders.push(holder("pending_effect", 0, &pending.resolving_item));
        }
    }
    json!({"viewer": input.viewer.0, "items": projection.items, "untracked_live_refs": out,
           "paid_cost_holders": holders,
           "seam_refs": seam_refs_v1(input.state, input.viewer)})
}

/// Collect pass: every reference with its region.
fn collect_refs(
    tagged: &Value,
    record: &mut dyn FnMut(RefV1, &'static str),
) -> Result<(), ExportErrorV1> {
    let mut copy = tagged.clone();
    let mut resolve = |r: RefV1| -> Result<ResolvedV1, ExportErrorV1> {
        let region = match r.class {
            ClassV1::Live => "live",
            ClassV1::Event(_) => "event",
            ClassV1::Record(_) => "record",
        };
        record(r, region);
        Ok(None)
    };
    let mut ts = |_t: u64| -> Option<u64> { None };
    rewrite_state(&mut copy, &mut resolve, &mut ts)?;
    // The arena's own references (attachments, links) are live.
    if let Some(items) = tagged["objects"]["items"].as_array() {
        let mut walker = WalkerV1 {
            resolve: &mut resolve,
            timestamp: &mut ts,
            slot: 0,
            check_pairs: true,
            self_object: None,
            item_self_paid: None,
            container: None,
            membership: false,
            turn_scoped: false,
            announcing: false,
        };
        for (i, item) in items.iter().enumerate() {
            let mut item = item.clone();
            walker.self_object = Some(ObjectId(i as u32));
            if let Some(m) = item.as_object_mut() {
                m.remove("zone_change_count");
                for (k, f) in m.iter_mut() {
                    // Amendment 43.8: an object's attachment list names
                    // objects in their current incarnation (every zone
                    // change clears it, `clear_object_relations`): like a
                    // zone's membership, it never crosses the hand seam.
                    walker.membership = k == "attachments";
                    let walked = walker.walk(f, ClassV1::Live, k);
                    walker.membership = false;
                    walked?;
                }
            }
        }
    }
    Ok(())
}

/// History-like records (6.9): references outside the current span become
/// tombstones instead of refusals.
const HISTORY_RECORDS_V1: &[&str] = &[
    "trigger_uses_v1",
    "creature_death_turn_v1",
    "creatures_attacked_turn_v1",
    "attack_declaration_turn_v1",
    "counter_lki_v1",
    "creature_stats_lki_v1",
];

/// Fields rebuilt by the export instead of rewritten.
const REBUILT_V1: &[&str] = &["objects", "library_knowledge", "hand_knowledge"];

fn rewrite_state(
    tagged: &mut Value,
    resolve: &mut dyn FnMut(RefV1) -> Result<ResolvedV1, ExportErrorV1>,
    ts: &mut dyn FnMut(u64) -> Option<u64>,
) -> Result<(), ExportErrorV1> {
    let map = tagged
        .as_object_mut()
        .ok_or_else(|| refuse(RefusalCodeV1::R2, "state shape"))?;
    let mut walker = WalkerV1 {
        resolve,
        timestamp: ts,
        slot: 0,
        check_pairs: true,
        self_object: None,
        item_self_paid: None,
        container: None,
        membership: false,
        turn_scoped: false,
        announcing: false,
    };
    let mut record = 0usize;
    for (k, field) in map.iter_mut() {
        if REBUILT_V1.contains(&k.as_str()) {
            continue;
        }
        if k == "players" {
            // Hands and libraries are rebuilt from the slot layout.
            if let Some(players) = field.as_array_mut() {
                for p in players {
                    if let Some(pm) = p.as_object_mut() {
                        for (pk, pf) in pm.iter_mut() {
                            if pk == "hand" || pk == "library" {
                                continue;
                            }
                            walker.membership = pk == "battlefield" || pk == "graveyard";
                            let walked = walker.walk(pf, ClassV1::Live, pk);
                            walker.membership = false;
                            walked?;
                        }
                    }
                }
            }
            continue;
        }
        if k == "engine" {
            let Some(em) = field.as_object_mut() else {
                return Err(refuse(RefusalCodeV1::R2, "engine shape"));
            };
            for (ek, ef) in em.iter_mut() {
                if ek == "event_history" {
                    if let Some(events) = ef.as_array_mut() {
                        for (e, event) in events.iter_mut().enumerate() {
                            walker.slot = 0;
                            walker.walk(event, ClassV1::Event(e), "event")?;
                        }
                    }
                } else if ek == "event_log" {
                    if let Some(events) = ef.as_array_mut() {
                        for event in events.iter_mut() {
                            walker.slot = 0;
                            walker.walk(event, ClassV1::Record(record), "event_log")?;
                            record += 1;
                        }
                    }
                } else {
                    // Amendment 43.8: the combat state names creatures in
                    // their current incarnation (leaving the battlefield
                    // removes a creature from combat, 506.4): membership.
                    // This turn's until-end-of-turn effects are bounded by
                    // the turn's start.
                    walker.membership = ek == "combat";
                    walker.turn_scoped = ek == "until_end_of_turn";
                    walker.announcing = ek == "pending_cast" || ek == "pending_activation";
                    let walked = walker.walk(ef, ClassV1::Live, ek);
                    walker.membership = false;
                    walker.turn_scoped = false;
                    walker.announcing = false;
                    walked?;
                }
            }
            continue;
        }
        if HISTORY_RECORDS_V1.contains(&k.as_str()) {
            walker.slot = 0;
            walker.walk(field, ClassV1::Record(record), k)?;
            record += 1;
            continue;
        }
        walker.membership = k == "exile" || k == "command";
        let walked = walker.walk(field, ClassV1::Live, k);
        walker.membership = false;
        walked?;
    }
    Ok(())
}

fn rewrite_timestamps(value: &mut Value, rank: &BTreeMap<u64, u64>) -> Result<(), ExportErrorV1> {
    let mut resolve = |_r: RefV1| -> Result<ResolvedV1, ExportErrorV1> { Ok(None) };
    let mut missing = None;
    let mut ts = |t: u64| -> Option<u64> {
        let r = rank.get(&t).copied();
        if r.is_none() {
            missing = Some(t);
        }
        r
    };
    let mut walker = WalkerV1 {
        resolve: &mut resolve,
        timestamp: &mut ts,
        slot: 0,
        check_pairs: false,
        self_object: None,
        item_self_paid: None,
        container: None,
        membership: false,
        turn_scoped: false,
        announcing: false,
    };
    if let Some(map) = value.as_object_mut() {
        for (k, f) in map.iter_mut() {
            if k == "engine" {
                if let Some(em) = f.as_object_mut() {
                    for (ek, ef) in em.iter_mut() {
                        if ek != "next_effect_timestamp" {
                            walker.walk(ef, ClassV1::Live, ek)?;
                        }
                    }
                }
                continue;
            }
            walker.walk(f, ClassV1::Live, k)?;
        }
    }
    if let Some(t) = missing {
        return Err(refuse(
            RefusalCodeV1::R2,
            format!("timestamp {t} has no rank"),
        ));
    }
    Ok(())
}

/// 6.8's static assertions on the canonical state.
fn static_assertions(
    state: &GameState,
    handles: &[HandleV1],
    v: PlayerId,
) -> Result<(), ExportErrorV1> {
    let fail = |d: &str| Err(refuse(RefusalCodeV1::R2, format!("static assertion: {d}")));
    let opp = v.opponent();
    if state.objects.len() != handles.len() {
        return fail("one handle per object");
    }
    if !state.hand_knowledge[opp.index()].iter().all(Vec::is_empty)
        || !state.library_knowledge[opp.index()]
            .iter()
            .all(Vec::is_empty)
    {
        return fail("opponent-observer rows are empty");
    }
    for k in &state.hand_knowledge[v.index()][opp.index()] {
        match handles.get(k.object.0 as usize) {
            Some(HandleV1::C { .. })
            | Some(HandleV1::U {
                known_name: true, ..
            }) => {}
            _ => return fail("a hand row binds a known object or a known_name occupant"),
        }
    }
    for (id, o) in state.objects.iter() {
        match &handles[id.0 as usize] {
            HandleV1::H { .. } => {
                if o.zone_change_count != 0 || o.zone != Zone::Command {
                    return fail("tombstones are generation 0 in no zone");
                }
            }
            HandleV1::U { pinned: false, .. } => {
                if o.zone_change_count != 1 {
                    return fail("unreferenced and synthetic occupants are generation 1");
                }
            }
            _ => {}
        }
        if !is_public(o.zone) && matches!(handles[id.0 as usize], HandleV1::H { .. }) {
            return fail("tombstone in a hidden zone");
        }
    }
    Ok(())
}

/// SHA-256 of the canonical bytes, from handle labels (the payload's form).
pub(crate) fn canonical_digest_v1(state: &GameState, labels: &[String]) -> String {
    format!(
        "{:x}",
        Sha256::digest(canonical_bytes_from_labels(state, labels))
    )
}

/// The payload bytes: schema, handle labels and the canonical state.
fn canonical_bytes(state: &GameState, handles: &[HandleV1]) -> Vec<u8> {
    let labels: Vec<String> = handles.iter().map(HandleV1::label).collect();
    canonical_bytes_from_labels(state, &labels)
}

fn canonical_bytes_from_labels(state: &GameState, labels: &[String]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(INFOSET_SCHEMA_V1.as_bytes());
    bytes.push(b'\n');
    bytes.extend_from_slice(&serde_json::to_vec(&labels).expect("labels serialize"));
    bytes.push(b'\n');
    bytes.extend_from_slice(&serde_json::to_vec(state).expect("state serializes"));
    bytes
}

/// Mutant L12: v6's allocator, in which tracked cards and pins count.
fn allocate_counting_arrivals(
    projection: &ProjectionV1,
    state: &GameState,
    opp_hand_tracked: &[ObjectId],
    pool5: &NamesV1,
    slots: usize,
) -> Result<Vec<u16>, AllocationErrorV1> {
    let h0: Vec<u16> = opp_hand_tracked
        .iter()
        .map(|id| state.objects.get(*id).card_def)
        .collect();
    let mut placed: Vec<u16> = Vec::new();
    let mut pool = pool5.clone();
    for node in projection.survivors.nodes() {
        let mut have = multiset(h0.iter().chain(&placed).copied());
        let inter = |have: &NamesV1| -> i64 {
            node.k
                .iter()
                .map(|(n, c)| i64::from((*c).min(have.get(n).copied().unwrap_or(0))))
                .sum()
        };
        while node.m - inter(&have) > 0 {
            let pick = node
                .k
                .keys()
                .copied()
                .filter(|n| pool.get(n).copied().unwrap_or(0) > 0)
                .min_by_key(|n| name_key(*n))
                .ok_or(AllocationErrorV1::Unfillable)?;
            if placed.len() >= slots {
                return Err(AllocationErrorV1::Unfillable);
            }
            *pool.get_mut(&pick).expect("present") -= 1;
            placed.push(pick);
            *have.entry(pick).or_default() += 1;
        }
    }
    Ok(placed)
}

/// Diagnostics (amendment 43.8; tests and the refusal log, never the
/// payload): the live references to opponent
/// objects that cross the hand seam, with their creation bound, the seam
/// events of their name and the JSON paths that hold their object.
pub(crate) fn seam_refs_v1(state: &GameState, viewer: PlayerId) -> Vec<String> {
    let projection = project_v1(state, state.observation_journal.notes(), viewer);
    let tagged = tagged_value(state);
    let mut refs: Vec<(RefV1, &'static str)> = Vec::new();
    {
        let mut record = |r: RefV1, region: &'static str| refs.push((r, region));
        if collect_refs(&tagged, &mut record).is_err() {
            return vec!["collect refused".into()];
        }
    }
    fn paths(v: &Value, tag: &str, at: String, out: &mut Vec<String>) {
        match v {
            Value::String(s) if s == tag => out.push(at),
            Value::Array(xs) => {
                for (i, x) in xs.iter().enumerate() {
                    paths(x, tag, format!("{at}[{i}]"), out);
                }
            }
            Value::Object(m) => {
                for (k, x) in m {
                    paths(x, tag, format!("{at}.{k}"), out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    let opp = viewer.opponent();
    for (r, region) in &refs {
        if *region != "live" || state.objects.get(r.object).owner != opp {
            continue;
        }
        let Some(b) = seam_bound_v1(&projection, r) else {
            continue;
        };
        let name = card_name(state.objects.get(r.object).card_def);
        let seams: Vec<usize> = projection
            .seams
            .iter()
            .filter(|(_, d)| card_name(*d) == name)
            .map(|(i, _)| *i)
            .collect();
        let tag = format!("{}{}", crate::ids::OBJECT_ID_TAG_PREFIX_V1, r.object.0);
        let mut at = Vec::new();
        paths(&tagged, &tag, "state".into(), &mut at);
        at.retain(|p| !p.starts_with("state.engine.event_history"));
        at.truncate(12);
        out.push(format!(
            "object {} {name} gens {:?} bound {b} seams {seams:?} refuses {} receipt {} paid {} container {:?} paths {at:?}",
            r.object.0,
            r.generations,
            seams.iter().any(|i| *i > b),
            r.cast_receipt,
            r.paid_cost,
            r.container,
        ));
    }
    out
}
