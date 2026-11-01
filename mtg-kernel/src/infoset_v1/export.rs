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
    let projected: BTreeSet<(u32, u32)> = projection
        .hand_facts
        .iter()
        .filter_map(|f| f.native)
        .map(|id| (id.0, state.objects.get(id).zone_change_count))
        .collect();
    let native: BTreeSet<(u32, u32)> = state
        .known_hand_cards(v, opp)
        .iter()
        .map(|k| (k.object.0, k.zone_change_count))
        .collect();
    if projected != native {
        return Err(refuse(
            RefusalCodeV1::R5,
            "hand physical facts differ from native rows",
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
fn tracked_order(state: &GameState, projection: &ProjectionV1, l3: bool) -> Vec<ObjectId> {
    let v = projection.viewer;
    let opp = v.opponent();
    let mut out: Vec<ObjectId> = Vec::new();
    let mut seen = BTreeSet::new();
    let mut push = |id: ObjectId, out: &mut Vec<ObjectId>| {
        if projection.tracked(id) && seen.insert(id) {
            out.push(id);
        }
    };
    for p in [PlayerId::P0, PlayerId::P1] {
        for &id in &state.players[p.index()].battlefield {
            push(id, &mut out);
        }
    }
    for p in [PlayerId::P0, PlayerId::P1] {
        for &id in &state.players[p.index()].graveyard {
            push(id, &mut out);
        }
    }
    for &id in state.exile.iter().chain(&state.command) {
        push(id, &mut out);
    }
    for item in &state.stack {
        if state.objects.get(item.source).zone == Zone::Stack {
            push(item.source, &mut out);
        }
    }
    for &id in &state.players[v.index()].hand {
        push(id, &mut out);
    }
    for f in &projection.hand_facts {
        if let Some(id) = f.native {
            push(id, &mut out);
        }
    }
    let _ = opp;
    for p in [PlayerId::P0, PlayerId::P1] {
        for id in projection.library[p.index()].values() {
            push(*id, &mut out);
        }
    }
    let mut rest: Vec<(usize, u32, ObjectId)> = state
        .objects
        .iter()
        .filter(|(id, _)| projection.tracked(*id) && !seen.contains(id))
        .map(|(id, _)| {
            let start = projection
                .current_span(id)
                .map_or(usize::MAX, |s| s.start_item);
            (start, id.0, id)
        })
        .collect();
    rest.sort();
    out.extend(rest.into_iter().map(|(_, _, id)| id));
    if l3 {
        // Mutant L3: raw native ids order the public objects.
        out.sort();
    }
    out
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
    // R3: a pending library-search plan owned by the opponent.
    match crate::effect::library_choice_search_v2::plan_future_v3(state, opp) {
        Ok(None) => {}
        Ok(Some(_)) | Err(_) => {
            return Err(refuse(
                RefusalCodeV1::R3,
                "opponent library-search plan pending",
            ));
        }
    }

    // Step 3: unknown slots per owner, hand then library.
    let mut slots: Vec<SlotV1> = Vec::new();
    let mut opp_hand_tracked: Vec<ObjectId> = Vec::new();
    for owner in [PlayerId::P0, PlayerId::P1] {
        let p = &state.players[owner.index()];
        if owner != v {
            opp_hand_tracked = projection
                .hand_facts
                .iter()
                .filter_map(|f| f.native)
                .filter(|id| p.hand.contains(id))
                .collect();
            if is(MutantV1::L8) {
                // Count facts bound to the true remaining copies.
                for f in projection.hand_facts.iter().filter(|f| f.native.is_none()) {
                    if let Some(&id) = p.hand.iter().find(|id| {
                        state.objects.get(**id).card_def == f.card_def
                            && !opp_hand_tracked.contains(id)
                    }) {
                        opp_hand_tracked.push(id);
                    }
                }
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

    // R1b first (a name-level, public-event predicate), then R1 and R2.
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
            if let Some(end) = span.end {
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
        if pinned_untracked.contains(&r.object) {
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
                        || (is(MutantV1::L8) && opp_hand_tracked.contains(id)))
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
    let survivors = &projection.survivors;
    let allocation = if is(MutantV1::L11) {
        // Tracker-rule facts only.
        Ok(projection
            .hand_facts
            .iter()
            .filter(|f| f.native.is_none())
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
        survivors.allocate(&pool5, opp_hand_slots, &name_key)
    };
    let known_names = match allocation {
        Ok(x) => x,
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
                .filter(|f| f.native.is_none())
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
    let tracked = tracked_order(state, &projection, is(MutantV1::L3));
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
    let linkable = |r: &RefV1| -> bool {
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
        let counts = (*region == "live") || (new_id.contains_key(&r.object) && linkable(r));
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
        )
    };
    let mut timestamps: BTreeSet<u64> = BTreeSet::new();
    let pinned_set: BTreeSet<ObjectId> = pinned_untracked.iter().copied().collect();
    {
        let mut resolve = |r: RefV1| -> Result<ResolvedV1, ExportErrorV1> {
            let o = r.object;
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
                                format!("event {e} of an unprojected kind names a hidden object"),
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
    for owner in [PlayerId::P0, PlayerId::P1] {
        let p = &state.players[owner.index()];
        let mut hand: Vec<u32> = Vec::new();
        if owner == v {
            for id in &p.hand {
                hand.push(new_id[id]);
            }
        } else {
            for id in &opp_hand_tracked {
                hand.push(new_id[id]);
            }
        }
        let mut library: Vec<Option<u32>> = vec![None; p.library.len()];
        for (pos, id) in &projection.library[owner.index()] {
            library[*pos as usize] = Some(new_id[id]);
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
                object: ObjectId(new_id[id]),
                zone_change_count: phi(*id, state.objects.get(*id).zone_change_count).unwrap_or(1),
            });
        }
    }
    for id in &opp_hand_tracked {
        hand_knowledge[v.index()][opp.index()].push(HandKnowledgeEntry {
            object: ObjectId(new_id[id]),
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
        };
        for (i, item) in items.iter().enumerate() {
            let mut item = item.clone();
            walker.self_object = Some(ObjectId(i as u32));
            if let Some(m) = item.as_object_mut() {
                m.remove("zone_change_count");
                for (k, f) in m.iter_mut() {
                    walker.walk(f, ClassV1::Live, k)?;
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
                            walker.walk(pf, ClassV1::Live, pk)?;
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
                    walker.walk(ef, ClassV1::Live, ek)?;
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
        walker.walk(field, ClassV1::Live, k)?;
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
