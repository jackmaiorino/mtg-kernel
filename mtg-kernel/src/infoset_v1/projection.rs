//! V's projected history (W1 design 6.1, 6.2 step 2): a port of Spellbench's
//! per-viewer tracker (`integrations/mtg_kernel/kernel_history_v2.py`,
//! `PublicHistory`) for one viewer, extended with spans, observed epochs,
//! unidentified transfers and the public-event logs the refusals read.
//!
//! The walk reads the committed event history and the observation journal in
//! the order the Python tracker absorbs them. It reads true object identities
//! (it has to: the native stream is omniscient), but every output is a
//! function of what V observed: which objects V can follow and since when,
//! the generations V saw them at, name-level hand facts, and public events.

use super::ledger::SurvivorTreeV1;
use crate::event::CommittedEvent;
use crate::ids::{ObjectId, PlayerId};
use crate::state::{GameState, ObservationNoteKindV1, ObservationNoteV1, Zone};
use std::collections::BTreeMap;

pub(crate) fn is_public(zone: Zone) -> bool {
    !matches!(zone, Zone::Hand | Zone::Library)
}

/// One maximal interval in which an object stays tracked (6.1). Items are
/// positions in the merged event and note stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SpanV1 {
    /// The generation at which the span began.
    pub start_gen: u32,
    pub start_item: usize,
    /// The event-history index from which references made inside the span
    /// may link to the object (the first event at or after `start_item`).
    pub start_event: usize,
    pub end: Option<SpanEndV1>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SpanEndV1 {
    /// The last generation V observed inside the span (e_last).
    pub last_gen: u32,
    /// The stream item that ended the span.
    pub item: usize,
    /// The zone V last saw the object in.
    pub zone: Zone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FactHowV1 {
    Revealed,
    Tracked,
    FromPublicZone,
}

/// A tracker-rule hand fact (kernel_history_v2.py `hand[viewer][owner]`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HandFactV1 {
    pub card_def: u16,
    pub how: FactHowV1,
    /// The physical copy while V can tell which one; `None` once it cannot.
    pub native: Option<ObjectId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransferDirV1 {
    /// Library to hand, unidentified to V.
    Draw,
    /// Hand to library, unidentified to V.
    PutBack,
}

/// An unidentified hand-library transfer of `owner` (6.3 C1 inventory).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TransferV1 {
    pub item: usize,
    pub owner: PlayerId,
    pub dir: TransferDirV1,
}

/// A card of `owner` named `card_def` became tracked by V from a hidden zone
/// (made public, looked at, or searched and revealed). R1b reads this log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ArrivalV1 {
    pub item: usize,
    pub owner: PlayerId,
    pub card_def: u16,
}

#[derive(Clone, Debug, Default)]
struct ObjectTrackV1 {
    exists: bool,
    owner: Option<PlayerId>,
    zone: Option<Zone>,
    moves: u32,
    tracked: bool,
    last_obs_gen: u32,
    last_obs_zone: Option<Zone>,
    spans: Vec<SpanV1>,
    /// Event index of the latest unidentified library-to-hand move, for the
    /// search-and-reveal outcome (R8b).
    arrived_from_library_at: Option<usize>,
}

/// The projection's output.
#[derive(Clone, Debug)]
pub(crate) struct ProjectionV1 {
    pub viewer: PlayerId,
    pub objects: Vec<ObjectTrackSummaryV1>,
    /// V's library knowledge per owner: position from the top to object.
    pub library: [BTreeMap<u32, ObjectId>; 2],
    /// Tracker-rule facts about the opponent's hand, in tracker order.
    pub hand_facts: Vec<HandFactV1>,
    pub survivors: SurvivorTreeV1,
    pub transfers: Vec<TransferV1>,
    pub arrivals: Vec<ArrivalV1>,
    /// Stream item at which each event-history index was absorbed.
    pub event_items: Vec<usize>,
    /// Objects V identified in each event (move events), by event index.
    pub event_observed: Vec<Vec<ObjectId>>,
    /// Total stream items.
    pub items: usize,
    /// Internal consistency failures (a projection bug): R5.
    pub inconsistencies: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ObjectTrackSummaryV1 {
    pub exists: bool,
    pub zone: Option<Zone>,
    pub tracked: bool,
    pub moves: u32,
    pub last_obs_gen: u32,
    pub spans: Vec<SpanV1>,
}

impl ProjectionV1 {
    pub(crate) fn tracked(&self, id: ObjectId) -> bool {
        self.objects.get(id.0 as usize).is_some_and(|o| o.tracked)
    }
    pub(crate) fn current_span(&self, id: ObjectId) -> Option<&SpanV1> {
        let o = self.objects.get(id.0 as usize)?;
        o.spans.last().filter(|s| s.end.is_none())
    }
    /// The span containing generation `g` of `id`, if V observed it.
    pub(crate) fn span_of(&self, id: ObjectId, g: u32) -> Option<&SpanV1> {
        let o = self.objects.get(id.0 as usize)?;
        o.spans.iter().rev().find(|s| {
            s.start_gen <= g
                && match s.end {
                    Some(end) => g <= end.last_gen,
                    None => g <= o.moves,
                }
        })
    }
    pub(crate) fn last_span(&self, id: ObjectId) -> Option<&SpanV1> {
        self.objects.get(id.0 as usize)?.spans.last()
    }
    /// Whether V tracked `id` right after stream item `item`.
    pub(crate) fn tracked_after(&self, id: ObjectId, item: usize) -> bool {
        self.objects.get(id.0 as usize).is_some_and(|o| {
            o.spans
                .iter()
                .any(|s| s.start_item <= item && s.end.is_none_or(|e| item < e.item))
        })
    }
}

struct WalkV1<'a> {
    state: &'a GameState,
    viewer: PlayerId,
    objs: Vec<ObjectTrackV1>,
    library: [BTreeMap<u32, ObjectId>; 2],
    hand: Vec<HandFactV1>,
    removed: Vec<(PlayerId, Option<ObjectId>)>,
    survivors: SurvivorTreeV1,
    transfers: Vec<TransferV1>,
    arrivals: Vec<ArrivalV1>,
    item: usize,
    next_event: usize,
    inconsistencies: Vec<String>,
    /// Zones before the current item of the objects it moved.
    prev_zones: Vec<(ObjectId, Option<Zone>)>,
    last_observed: Vec<ObjectId>,
}

/// Projects V's history from the native stream.
pub(crate) fn project_v1(
    state: &GameState,
    journal: &[ObservationNoteV1],
    viewer: PlayerId,
) -> ProjectionV1 {
    let n = state.objects.len();
    let mut objs = vec![ObjectTrackV1::default(); n];
    for (id, object) in state.objects.iter() {
        let o = &mut objs[id.0 as usize];
        o.owner = Some(object.owner);
        // Cards begin in their owner's library; tokens and spell copies get
        // their zone from the event that creates them.
        if !object.v4.is_token && object.spell_copy_origin.is_none() {
            o.exists = true;
            o.zone = Some(Zone::Library);
        }
    }
    let mut w = WalkV1 {
        state,
        viewer,
        objs,
        library: [BTreeMap::new(), BTreeMap::new()],
        hand: Vec::new(),
        removed: Vec::new(),
        survivors: SurvivorTreeV1::default(),
        transfers: Vec::new(),
        arrivals: Vec::new(),
        item: 0,
        next_event: 0,
        inconsistencies: Vec::new(),
        prev_zones: Vec::new(),
        last_observed: Vec::new(),
    };
    let events = &state.engine.event_history;
    let mut event_items = Vec::with_capacity(events.len());
    let mut event_observed = Vec::with_capacity(events.len());
    let mut cursor = 0;
    for (index, event) in events.iter().enumerate() {
        let mut positions = Vec::new();
        let mut after = Vec::new();
        while cursor < journal.len() && journal[cursor].history_len <= index {
            let note = &journal[cursor];
            cursor += 1;
            if matches!(
                note.kind,
                ObservationNoteKindV1::LibraryRemoval { .. }
                    | ObservationNoteKindV1::LibraryInsertion { .. }
            ) {
                positions.push(note);
            }
            if let ObservationNoteKindV1::HandCardRevealed { object, .. } = note.kind {
                if note.history_len == index
                    && w.objs
                        .get(object.0 as usize)
                        .is_some_and(|o| o.zone != Some(Zone::Hand))
                {
                    // Anchored to this event but made once its card reached
                    // the hand: the kernel commits the event after the move.
                    after.push(note);
                    continue;
                }
            }
            w.note(note);
        }
        event_items.push(w.item);
        w.next_event = index;
        w.event(event, index, &positions);
        event_observed.push(std::mem::take(&mut w.last_observed));
        w.removed.clear();
        for note in after {
            w.note(note);
        }
    }
    w.next_event = events.len();
    for note in &journal[cursor..] {
        w.note(note);
    }
    // Every object's native generation must equal the moves V's walk counted
    // (hidden moves included): otherwise the walk missed a zone change and
    // its epochs cannot be trusted.
    for (id, object) in state.objects.iter() {
        let o = &w.objs[id.0 as usize];
        if o.exists && o.moves != object.zone_change_count {
            w.inconsistencies.push(format!(
                "object {} moved {} times in the walk, generation {}",
                id.0, o.moves, object.zone_change_count
            ));
        }
        if o.exists && o.zone != Some(object.zone) {
            w.inconsistencies
                .push(format!("object {} zone differs from the walk", id.0));
        }
    }
    let items = w.item;
    ProjectionV1 {
        viewer,
        objects: w
            .objs
            .into_iter()
            .map(|o| ObjectTrackSummaryV1 {
                exists: o.exists,
                zone: o.zone,
                tracked: o.tracked,
                moves: o.moves,
                last_obs_gen: o.last_obs_gen,
                spans: o.spans,
            })
            .collect(),
        library: w.library,
        hand_facts: w.hand,
        survivors: w.survivors,
        transfers: w.transfers,
        arrivals: w.arrivals,
        event_items,
        event_observed,
        items,
        inconsistencies: w.inconsistencies,
    }
}

impl WalkV1<'_> {
    fn opp(&self) -> PlayerId {
        self.viewer.opponent()
    }
    fn owner(&self, id: ObjectId) -> PlayerId {
        self.objs[id.0 as usize]
            .owner
            .expect("arena objects have owners")
    }
    fn card_def(&self, id: ObjectId) -> u16 {
        self.state.objects.get(id).card_def
    }

    fn is_tracked_now(&self, id: ObjectId) -> bool {
        let o = &self.objs[id.0 as usize];
        if !o.exists {
            return false;
        }
        match o.zone {
            None => false,
            Some(Zone::Hand) => {
                o.owner == Some(self.viewer) || self.hand.iter().any(|f| f.native == Some(id))
            }
            Some(Zone::Library) => {
                let owner = self.owner(id);
                self.library[owner.index()].values().any(|&x| x == id)
            }
            Some(_) => true,
        }
    }

    /// Recomputes tracking after one stream item. `observed` lists objects V
    /// identified in the item (their new generation is V-observed).
    #[allow(
        clippy::needless_range_loop,
        reason = "the index is also the object id"
    )]
    fn settle(&mut self, observed: &[ObjectId]) {
        let item = self.item;
        let next_event = self.next_event;
        let prev_zones = std::mem::take(&mut self.prev_zones);
        let opp = self.opp();
        for id in observed {
            let o = &mut self.objs[id.0 as usize];
            o.last_obs_gen = o.moves;
            o.last_obs_zone = o.zone;
        }
        self.last_observed = observed.to_vec();
        for i in 0..self.objs.len() {
            let id = ObjectId(i as u32);
            let now = self.is_tracked_now(id);
            let o = &self.objs[i];
            if now && !o.tracked {
                // Became tracked: from a hidden zone unless it was just
                // created. R1b reads these arrivals; a card of the opponent
                // located while in the library taints survivor nodes (R8b).
                let before = prev_zones
                    .iter()
                    .find(|(x, _)| *x == id)
                    .map_or(o.zone, |(_, z)| *z);
                if let (Some(before), Some(owner)) = (before, o.owner) {
                    if !is_public(before) {
                        let card_def = self.state.objects.get(id).card_def;
                        self.arrivals.push(ArrivalV1 {
                            item,
                            owner,
                            card_def,
                        });
                        if before == Zone::Library && owner == opp {
                            self.survivors.taint_name(card_def);
                        }
                    }
                }
                let o = &mut self.objs[i];
                o.spans.push(SpanV1 {
                    start_gen: o.moves,
                    start_item: item,
                    start_event: next_event,
                    end: None,
                });
                o.last_obs_gen = o.moves;
                o.last_obs_zone = o.zone;
            } else if !now && o.tracked {
                let o = &mut self.objs[i];
                let last_gen = o.last_obs_gen;
                let zone = o.last_obs_zone.unwrap_or(Zone::Library);
                if let Some(span) = o.spans.last_mut() {
                    span.end = Some(SpanEndV1 {
                        last_gen,
                        item,
                        zone,
                    });
                }
            } else if now {
                self.objs[i].last_obs_gen = self.objs[i].moves;
                self.objs[i].last_obs_zone = self.objs[i].zone;
            }
            let o = &mut self.objs[i];
            o.tracked = now;
        }
        // A hand with no untracked card left: F is cleared (6.4).
        let opp_hand = self.state_hand_untracked(opp);
        if opp_hand == 0 {
            self.survivors.clear();
        }
        self.survivors.assert_laminar();
        self.item += 1;
    }

    fn state_hand_untracked(&self, owner: PlayerId) -> usize {
        self.objs
            .iter()
            .filter(|o| {
                o.exists && o.owner == Some(owner) && o.zone == Some(Zone::Hand) && !o.tracked
            })
            .count()
    }

    fn moved(&mut self, id: ObjectId, zone: Zone) {
        let before = self.objs[id.0 as usize].zone;
        self.prev_zones.push((id, before));
        let o = &mut self.objs[id.0 as usize];
        o.zone = Some(zone);
        o.moves += 1;
    }

    fn event(&mut self, event: &CommittedEvent, index: usize, positions: &[&ObservationNoteV1]) {
        match event {
            CommittedEvent::Draw { player, object } => {
                let Some(id) = *object else {
                    self.settle(&[]);
                    return;
                };
                let was = self.objs[id.0 as usize].tracked;
                let from_library = self.objs[id.0 as usize].zone == Some(Zone::Library);
                self.moved(id, Zone::Hand);
                self.library_to_hand(id, *player);
                let named = *player == self.viewer;
                if !named && !was && from_library {
                    self.objs[id.0 as usize].arrived_from_library_at = Some(index);
                    self.transfers.push(TransferV1 {
                        item: self.item,
                        owner: *player,
                        dir: TransferDirV1::Draw,
                    });
                }
                let observed: Vec<ObjectId> = if named || was { vec![id] } else { vec![] };
                self.settle(&observed);
            }
            CommittedEvent::ZoneChange {
                object, from, to, ..
            } => {
                let id = *object;
                let was = self.objs[id.0 as usize].tracked;
                let owner = self.owner(id);
                let token_or_copy = {
                    let o = self.state.objects.get(id);
                    o.v4.is_token || o.spell_copy_origin.is_some()
                };
                if !self.objs[id.0 as usize].exists {
                    self.objs[id.0 as usize].exists = true;
                }
                self.moved(id, *to);
                let public = is_public(*from) || is_public(*to);
                let named = public || owner == self.viewer;
                if !token_or_copy {
                    self.hand_move(id, owner, *from, *to, named);
                }
                if *from == Zone::Library && *to == Zone::Hand && !named && !was {
                    self.objs[id.0 as usize].arrived_from_library_at = Some(index);
                    self.transfers.push(TransferV1 {
                        item: self.item,
                        owner,
                        dir: TransferDirV1::Draw,
                    });
                }
                if *from == Zone::Hand && *to == Zone::Library && !named {
                    self.transfers.push(TransferV1 {
                        item: self.item,
                        owner,
                        dir: TransferDirV1::PutBack,
                    });
                }
                let _ = positions;
                let observed: Vec<ObjectId> = if named || was { vec![id] } else { vec![] };
                self.settle(&observed);
            }
            CommittedEvent::SpellCast { spell, .. } => {
                let id = *spell;
                let copy = self.state.objects.get(id).spell_copy_origin.is_some();
                let o = &mut self.objs[id.0 as usize];
                if copy {
                    o.exists = true;
                    o.zone = Some(Zone::Stack);
                    o.moves += 1;
                    self.settle(&[id]);
                    return;
                }
                let source = o.zone;
                if source == Some(Zone::Stack) {
                    self.settle(&[id]);
                    return;
                }
                let was = o.tracked;
                let owner = self.owner(id);
                self.moved(id, Zone::Stack);
                if let Some(source) = source {
                    self.hand_move(id, owner, source, Zone::Stack, true);
                }
                let _ = was;
                self.settle(&[id]);
            }
            CommittedEvent::CreateToken { object, .. } => {
                let id = *object;
                let o = &mut self.objs[id.0 as usize];
                o.exists = true;
                o.zone = Some(Zone::Battlefield);
                o.moves += 1;
                self.settle(&[id]);
            }
            _ => self.settle(&[]),
        }
    }

    fn note(&mut self, note: &ObservationNoteV1) {
        let viewer = self.viewer;
        match &note.kind {
            ObservationNoteKindV1::LibraryRandomized { owner } => {
                self.library[owner.index()].clear();
                self.settle(&[]);
            }
            ObservationNoteKindV1::LibraryLooked {
                observer,
                owner,
                positions,
            } => {
                if *observer != viewer {
                    self.settle(&[]);
                    return;
                }
                let gone: Vec<u32> = positions.iter().map(|(p, _)| *p).collect();
                let natives: Vec<ObjectId> = positions.iter().map(|(_, o)| *o).collect();
                let lib = &mut self.library[owner.index()];
                lib.retain(|p, o| !gone.contains(p) && !natives.contains(o));
                let mut observed = Vec::new();
                for (p, id) in positions {
                    self.library[owner.index()].insert(*p, *id);
                    observed.push(*id);
                }
                self.settle(&observed);
            }
            ObservationNoteKindV1::LibraryReordered {
                owner,
                ordered,
                revealed_to,
            } => {
                let len = ordered.len() as u32;
                self.library[owner.index()].retain(|p, _| *p >= len);
                let mut observed = Vec::new();
                if revealed_to.contains(&viewer) {
                    for (p, id) in ordered.iter().enumerate() {
                        self.library[owner.index()].insert(p as u32, *id);
                        observed.push(*id);
                    }
                }
                self.settle(&observed);
            }
            ObservationNoteKindV1::LibraryScried {
                owner,
                retained_top,
                ordered_bottom,
            } => {
                let top = retained_top.len() as u32;
                let bottom = ordered_bottom.len() as u32;
                let prefix = top + bottom;
                let length = self.library_count(*owner);
                let old = std::mem::take(&mut self.library[owner.index()]);
                let mut updated: BTreeMap<u32, ObjectId> = old
                    .iter()
                    .filter(|(p, _)| **p >= prefix)
                    .map(|(p, o)| (*p - bottom, *o))
                    .collect();
                let mut observed = Vec::new();
                if *owner == viewer {
                    for (p, id) in retained_top.iter().enumerate() {
                        updated.insert(p as u32, *id);
                        observed.push(*id);
                    }
                    for (offset, id) in ordered_bottom.iter().enumerate() {
                        updated.insert(length - bottom + offset as u32, *id);
                        observed.push(*id);
                    }
                } else if prefix == 1 {
                    let first = retained_top.iter().chain(ordered_bottom).next().copied();
                    if let (Some(fact), Some(first)) = (old.get(&0), first) {
                        if *fact == first {
                            updated.insert(if bottom > 0 { length - 1 } else { 0 }, *fact);
                            observed.push(*fact);
                        }
                    }
                }
                self.library[owner.index()] = updated;
                self.settle(&observed);
            }
            ObservationNoteKindV1::HandCardRevealed {
                observer,
                owner,
                object,
            } => {
                if *observer != viewer || *owner == viewer {
                    self.settle(&[]);
                    return;
                }
                let id = *object;
                let card_def = self.card_def(id);
                let was = self.objs[id.0 as usize].tracked;
                // A search-and-reveal outcome: the card was moved from the
                // library into the hand by the event this reveal anchors to.
                // It locates a card outside the hand: R8b, applied before the
                // in-place reveal update below (ruling 44).
                if !was
                    && self.objs[id.0 as usize].arrived_from_library_at == Some(note.history_len)
                {
                    self.survivors.taint_name(card_def);
                }
                self.revealed_in_hand(id, card_def, was);
                self.settle(&[id]);
            }
            ObservationNoteKindV1::LibraryRemoval { owner, position } => {
                let lib = std::mem::take(&mut self.library[owner.index()]);
                let captured = lib.get(position).copied();
                self.library[owner.index()] = lib
                    .into_iter()
                    .filter(|(p, _)| p != position)
                    .map(|(p, o)| (if p > *position { p - 1 } else { p }, o))
                    .collect();
                self.removed.push((*owner, captured));
                self.settle(&[]);
            }
            ObservationNoteKindV1::LibraryInsertion { owner, position } => {
                let lib = std::mem::take(&mut self.library[owner.index()]);
                self.library[owner.index()] = lib
                    .into_iter()
                    .map(|(p, o)| (if p >= *position { p + 1 } else { p }, o))
                    .collect();
                self.settle(&[]);
            }
        }
    }

    fn library_count(&self, owner: PlayerId) -> u32 {
        self.objs
            .iter()
            .filter(|o| o.exists && o.zone == Some(Zone::Library) && o.owner == Some(owner))
            .count() as u32
    }

    /// A known library card that reaches its owner's hand stays known to V.
    fn library_to_hand(&mut self, id: ObjectId, owner: PlayerId) {
        if owner == self.viewer {
            return;
        }
        let known = self
            .removed
            .iter()
            .any(|(o, captured)| *o == owner && *captured == Some(id));
        if known {
            let card_def = self.card_def(id);
            self.hand.push(HandFactV1 {
                card_def,
                how: FactHowV1::Tracked,
                native: Some(id),
            });
        }
    }

    fn revealed_in_hand(&mut self, id: ObjectId, card_def: u16, was_tracked: bool) {
        if self.hand.iter().any(|f| f.native == Some(id)) {
            return;
        }
        if !was_tracked {
            // 6.4: a card of the hand becomes tracked in place.
            self.survivors.tracked_in_place(card_def);
        }
        for fact in &mut self.hand {
            if fact.native.is_none() && fact.card_def == card_def {
                fact.native = Some(id);
                return;
            }
        }
        self.hand.push(HandFactV1 {
            card_def,
            how: FactHowV1::Revealed,
            native: Some(id),
        });
    }

    fn hand_move(&mut self, id: ObjectId, owner: PlayerId, from: Zone, to: Zone, named: bool) {
        if owner == self.viewer {
            return;
        }
        let card_def = self.card_def(id);
        if to == Zone::Hand && from != Zone::Hand {
            if is_public(from) {
                self.hand.push(HandFactV1 {
                    card_def,
                    how: FactHowV1::FromPublicZone,
                    native: Some(id),
                });
            } else if from == Zone::Library {
                self.library_to_hand(id, owner);
            }
            return;
        }
        if from != Zone::Hand || to == Zone::Hand {
            return;
        }
        let tracked = self.objs[id.0 as usize].tracked;
        if is_public(to) || named {
            // Identified as it leaves: drop that fact, else one fact with
            // its name.
            let pos = self
                .hand
                .iter()
                .position(|f| f.native == Some(id))
                .or_else(|| {
                    self.hand
                        .iter()
                        .position(|f| f.native.is_none() && f.card_def == card_def)
                })
                .or_else(|| self.hand.iter().position(|f| f.card_def == card_def));
            if let Some(pos) = pos {
                self.hand.remove(pos);
            }
            if !tracked {
                self.survivors.identified_departure(card_def);
            }
            return;
        }
        // An unidentified card left to a hidden zone: each name loses one
        // fact, and no remaining fact can still be tied to one copy.
        let tracked_names: Vec<u16> = self
            .hand
            .iter()
            .filter_map(|f| f.native.map(|_| f.card_def))
            .collect();
        self.survivors.unidentified_departure(&tracked_names);
        let mut seen = std::collections::BTreeSet::new();
        let mut kept = Vec::new();
        for fact in self.hand.iter().rev() {
            if seen.contains(&fact.card_def) {
                kept.push(HandFactV1 {
                    native: None,
                    ..*fact
                });
            } else {
                seen.insert(fact.card_def);
            }
        }
        kept.reverse();
        self.hand = kept;
    }
}
