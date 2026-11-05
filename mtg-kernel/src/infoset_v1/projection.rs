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
    /// The generation at which the span began: the epoch the object entered
    /// (or was first seen in). Every new span starts this way, however V
    /// came to follow the object (Sol review of amendment 43.3): the default
    /// epoch baseline must not reveal whether a copy was tracked before.
    pub start_gen: u32,
    /// The earliest generation V observed for this span: one below
    /// `start_gen` when the span began with a move V observed (V saw the
    /// epoch the card left). Read only to validate live references (R2);
    /// it never makes that epoch history-linkable or sets the baseline.
    pub cover_gen: u32,
    pub start_item: usize,
    /// The object's position within the record where the span starts, as
    /// presented to V (its index among the objects V identified in that
    /// record); `u32::MAX` when V did not identify it there (ruling 3: such
    /// a span has no V-presented position).
    pub start_pos: u32,
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
    /// Amendment 43.3: the span ended at a zone change the bot-facing
    /// transcript does not link (the adapter mints a new id on every zone
    /// change), while the object stayed in V's sight. V still sees the object
    /// in its new zone, under a new span; it cannot link the two.
    pub split: bool,
    /// The zone the object was in before the stream item that ended the
    /// span (where a move it made there started). Lead ruling 7 (a): V tells
    /// two departures apart by their origin zone.
    pub from: Zone,
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
    /// Lead ruling 11 (R2): the fact remained after a copy with its name
    /// left the hand identified by name while V held both a bound and an
    /// unbound fact of that name. V cannot tell whether the copy that left
    /// was the bound one, so whether this fact is bound (a `C:` survivor) or
    /// unbound (a `U:` occupant) is not V's information. Set from V's fact
    /// structure only; the export refuses while any such fact remains.
    pub ambiguous: bool,
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

/// Amendment 43.6 (lead ruling 9 (b)): a copy left the opponent's hand while
/// another copy with the same name and the same V-visible fact (equal 43.4
/// keys) was there with it, so V cannot tell which copy left. Every
/// candidate is listed with its epoch in the hand. Recorded from the fact
/// structure V sees (names, how each fact arose), never from which copy
/// actually left: the candidate set is the same whichever copy it was.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HandAmbiguityV1 {
    pub item: usize,
    pub card_def: u16,
    pub candidates: Vec<(ObjectId, u32)>,
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
    /// Amendment 43.6: departures from the opponent's hand that V cannot
    /// attribute between equal-key copies.
    pub hand_ambiguities: Vec<HandAmbiguityV1>,
    /// Stream items at which an owner's library order changed unseen by
    /// V's tracking (a shuffle, a reorder, a scry), with the owner.
    pub reorders: Vec<(usize, PlayerId)>,
    /// Unidentified draws from the top of an owner's library (not searches).
    pub top_draws: Vec<(usize, PlayerId)>,
    /// R8 (ruling 40.4's fallback for a gap of 6.4): V may know more about
    /// the opponent's hidden cards than the survivor tree keeps, because a
    /// card put back from a hand V knew something about was drawn back, or
    /// such put-back cards were scried or reordered. Set from V-observable
    /// events only; released once the opponent's hand is fully tracked (the
    /// tree is cleared) and no such put-back card is left in its library
    /// (a shuffle).
    pub r8_latch: bool,
    /// Stream item at which each event-history index was absorbed.
    pub event_items: Vec<usize>,
    /// Objects V identified in each event (move events), by event index.
    pub event_observed: Vec<Vec<ObjectId>>,
    /// Total stream items.
    pub items: usize,
    /// Per stream item: (events, notes) committed when it was absorbed, as
    /// running maxima (lead ruling 7 (a)): a decision presented at (h, j)
    /// follows exactly the items stamped at most (h, j).
    pub stamps: Vec<(usize, usize)>,
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
            s.cover_gen <= g
                && match s.end {
                    Some(end) => g <= end.last_gen,
                    None => g <= o.moves,
                }
        })
    }
    pub(crate) fn last_span(&self, id: ObjectId) -> Option<&SpanV1> {
        self.objects.get(id.0 as usize)?.spans.last()
    }
    /// Lead ruling 7 (a): the stream items at which V's bot-facing records
    /// split the stream into batches, from V's decision marks (event-history
    /// length, journal length), ascending. A mark with no stamped item after
    /// it separates nothing (the current decision). With no marks known the
    /// whole stream is one batch (fail closed).
    pub(crate) fn batch_bounds(&self, marks: Option<&[(usize, usize)]>) -> Vec<usize> {
        let mut out: Vec<usize> = marks
            .unwrap_or(&[])
            .iter()
            .map(|&(h, j)| {
                self.stamps
                    .iter()
                    .take_while(|&&(e, n)| e <= h && n <= j)
                    .count()
            })
            .filter(|&b| b < self.stamps.len())
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }
    /// The batch of stream item `item`: how many bounds lie at or before it.
    pub(crate) fn batch_of(bounds: &[usize], item: usize) -> usize {
        bounds.partition_point(|&b| b <= item)
    }
    /// Where V's tracking of `id` lapsed at or after `span` (the first end
    /// from `span` on that is not an amendment 43.3 split): the object left
    /// V's sight there. `None` when every later boundary is a split, so the
    /// object stayed in V's sight throughout.
    pub(crate) fn lapse_after(&self, id: ObjectId, span: &SpanV1) -> Option<SpanEndV1> {
        let o = self.objects.get(id.0 as usize)?;
        o.spans
            .iter()
            .skip_while(|s| !std::ptr::eq(*s, span))
            .filter_map(|s| s.end)
            .find(|e| !e.split)
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

/// An unidentified put-back still on top of its owner's library.
#[derive(Clone, Debug)]
struct PutBackV1 {
    /// V knew something about the hand it left (a fact, a node, the latch).
    informative: bool,
    /// The departure's removal record (the opponent's put-backs only).
    record: Vec<u64>,
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
    hand_ambiguities: Vec<HandAmbiguityV1>,
    reorders: Vec<(usize, PlayerId)>,
    /// Unidentified put-backs per owner still on top of the library (no
    /// reorder since): the next unidentified draws take them back.
    /// Per owner, the unidentified put-backs still on top of the library,
    /// newest last, each flagged when V knew something about the hand it
    /// left (a hand fact or a survivor node): drawing such a card back can
    /// tell V more than the tree keeps.
    put_backs_on_top: [Vec<PutBackV1>; 2],
    /// The removal record of the latest unidentified departure (a put-back
    /// takes it onto its stack entry).
    last_departure: Vec<u64>,
    /// Per owner: a scry or reorder touched the put-back cards since the
    /// last shuffle, so draws no longer retire their markers in order.
    put_backs_scrambled: [bool; 2],
    top_draws: Vec<(usize, PlayerId)>,
    r8_latch: bool,
    item: usize,
    next_event: usize,
    inconsistencies: Vec<String>,
    /// Zones before the current item of the objects it moved.
    prev_zones: Vec<(ObjectId, Option<Zone>)>,
    last_observed: Vec<ObjectId>,
    /// (events, notes) committed so far, and each item's stamp.
    committed: (usize, usize),
    stamps: Vec<(usize, usize)>,
    /// The owner whose library-to-hand move was the latest event, if it was
    /// one: a hand reveal right after such moves is a search-and-reveal
    /// outcome (R8b), recognized from the public event order alone.
    last_event_library_to_hand: Option<PlayerId>,
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
        hand_ambiguities: Vec::new(),
        reorders: Vec::new(),
        put_backs_on_top: [Vec::new(), Vec::new()],
        last_departure: Vec::new(),
        put_backs_scrambled: [false, false],
        top_draws: Vec::new(),
        r8_latch: false,
        item: 0,
        next_event: 0,
        inconsistencies: Vec::new(),
        prev_zones: Vec::new(),
        last_observed: Vec::new(),
        committed: (0, 0),
        stamps: Vec::new(),
        last_event_library_to_hand: None,
    };
    let events = &state.engine.event_history;
    let mut event_items = Vec::with_capacity(events.len());
    let mut event_observed = Vec::with_capacity(events.len());
    let mut cursor = 0;
    for (index, event) in events.iter().enumerate() {
        // Notes anchored before event `index` happen after event index-1:
        // a span they open may link references from event `index` on, never
        // from an earlier event (Sol r2).
        w.next_event = index;
        let mut positions = Vec::new();
        let mut after = Vec::new();
        while cursor < journal.len() && journal[cursor].history_len <= index {
            let note = &journal[cursor];
            cursor += 1;
            w.committed.1 = w.committed.1.max(cursor);
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
        w.committed.0 = index + 1;
        w.event(event, index, &positions);
        event_observed.push(std::mem::take(&mut w.last_observed));
        w.removed.clear();
        // Deferred notes follow event `index`.
        w.next_event = index + 1;
        for note in after {
            w.note(note);
        }
    }
    w.next_event = events.len();
    for (k, note) in journal[cursor..].iter().enumerate() {
        w.committed.1 = w.committed.1.max(cursor + k + 1);
        w.note(note);
    }
    // A spell being cast sits on the stack before its SpellCast commits:
    // the announcement is public, so the walk moves it as observed.
    if let Some(pending) = &state.engine.pending_cast {
        let id = pending.spell;
        let true_zone = state.objects.get(id).zone;
        if w.objs[id.0 as usize].zone != Some(true_zone) && is_public(true_zone) {
            let owner = w.owner(id);
            let from = w.objs[id.0 as usize].zone.unwrap_or(Zone::Hand);
            w.moved(id, true_zone);
            w.hand_move(id, owner, from, true_zone, true);
            w.settle(&[id]);
        }
    }
    // Every object's native generation must equal the moves V's walk counted
    // (hidden moves included): otherwise the walk missed a zone change and
    // its epochs cannot be trusted.
    // Moves without committed events (an announced cast that was aborted,
    // a madness cast rolled back to the graveyard) happen in V's sight when
    // the card is public or in V's own hand: V-tracked objects found there
    // take their native zone and epoch, under a new span (amendment 43.3).
    let final_item = w.item;
    for (id, object) in state.objects.iter() {
        let viewer_hand = object.owner == viewer && object.zone == Zone::Hand;
        let o = &mut w.objs[id.0 as usize];
        if o.tracked
            && (is_public(object.zone) || viewer_hand)
            && (o.zone != Some(object.zone) || o.moves != object.zone_change_count)
            && o.moves <= object.zone_change_count
        {
            // Amendment 43.3: an event-less zone change still re-mints the
            // object's bot-facing id, so its span ends and a new one starts
            // after the last record, with no V-presented position (two such
            // objects cannot be ordered and the export refuses R2).
            if o.moves < object.zone_change_count {
                let last_gen = o.last_obs_gen;
                let zone = o.last_obs_zone.unwrap_or(Zone::Library);
                let item = final_item;
                let from = o.zone.unwrap_or(Zone::Library);
                if let Some(span) = o.spans.last_mut().filter(|s| s.end.is_none()) {
                    span.end = Some(SpanEndV1 {
                        last_gen,
                        item,
                        zone,
                        split: true,
                        from,
                    });
                }
                o.spans.push(SpanV1 {
                    start_gen: object.zone_change_count,
                    cover_gen: object.zone_change_count,
                    start_item: item,
                    start_pos: u32::MAX,
                    start_event: state.engine.event_history.len(),
                    end: None,
                });
            }
            o.zone = Some(object.zone);
            o.moves = object.zone_change_count;
            o.last_obs_gen = o.moves;
            o.last_obs_zone = o.zone;
        }
    }
    // Only tracked objects' epochs are exported from the walk; a hidden
    // object's count may miss event-less moves (an aborted cast) without
    // affecting any exported value.
    for (id, object) in state.objects.iter() {
        let o = &w.objs[id.0 as usize];
        if !o.tracked {
            continue;
        }
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
        hand_ambiguities: w.hand_ambiguities,
        reorders: w.reorders,
        top_draws: w.top_draws,
        r8_latch: w.r8_latch,
        event_items,
        event_observed,
        items,
        stamps: w.stamps,
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
                // A span that begins with a move V observed covers the epoch
                // the card left (its origin epoch is one below the epoch it
                // entered, and V saw the move), for live-reference checks
                // only; the span itself starts at the entered epoch.
                let entered_by_observed_move =
                    observed.contains(&id) && prev_zones.iter().any(|(x, _)| *x == id);
                let o = &mut self.objs[i];
                let start_gen = o.moves;
                let cover_gen = if entered_by_observed_move {
                    o.moves.saturating_sub(1)
                } else {
                    o.moves
                };
                let start_pos = observed
                    .iter()
                    .position(|x| *x == id)
                    .map_or(u32::MAX, |p| p as u32);
                let o = &mut self.objs[i];
                o.spans.push(SpanV1 {
                    start_gen,
                    cover_gen,
                    start_item: item,
                    start_pos,
                    start_event: next_event,
                    end: None,
                });
                o.last_obs_gen = o.moves;
                o.last_obs_zone = o.zone;
            } else if !now && o.tracked {
                let from = prev_zones
                    .iter()
                    .find(|(x, _)| *x == id)
                    .map_or(o.zone, |(_, z)| *z)
                    .unwrap_or(Zone::Library);
                let o = &mut self.objs[i];
                let last_gen = o.last_obs_gen;
                let zone = o.last_obs_zone.unwrap_or(Zone::Library);
                if let Some(span) = o.spans.last_mut() {
                    span.end = Some(SpanEndV1 {
                        last_gen,
                        item,
                        zone,
                        split: false,
                        from,
                    });
                }
            } else if now {
                // Amendment 43.3: a tracked object that changed zone in this
                // item ends its span and starts a new one, at this record and
                // its position in it as presented to V. The adapter mints a
                // new id on every zone change (kernel_observation_v2.py keys
                // ids by arena, generation and zone) and links history only
                // to the current id, so V cannot join the two spans. The new
                // span starts at the new epoch: nothing from before the move
                // links to it.
                let moves_here = prev_zones.iter().filter(|(x, _)| *x == id).count() as u32;
                if moves_here > 0 {
                    let first_zone = prev_zones
                        .iter()
                        .find(|(x, _)| *x == id)
                        .and_then(|(_, z)| *z);
                    let start_pos = observed
                        .iter()
                        .position(|x| *x == id)
                        .map_or(u32::MAX, |p| p as u32);
                    let o = &mut self.objs[i];
                    let last_gen = o.moves.saturating_sub(moves_here);
                    let zone = first_zone.unwrap_or(Zone::Library);
                    if let Some(span) = o.spans.last_mut().filter(|s| s.end.is_none()) {
                        span.end = Some(SpanEndV1 {
                            last_gen,
                            item,
                            zone,
                            split: true,
                            from: zone,
                        });
                    }
                    o.spans.push(SpanV1 {
                        start_gen: o.moves,
                        cover_gen: o.moves,
                        start_item: item,
                        start_pos,
                        start_event: next_event,
                        end: None,
                    });
                }
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
            if !self.put_backs_on_top[opp.index()]
                .iter()
                .any(|x| x.informative)
            {
                self.r8_latch = false;
            }
        }
        self.survivors.assert_laminar();
        self.stamps.push(self.committed);
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
        let _ = index;
        self.last_event_library_to_hand = match event {
            CommittedEvent::Draw {
                player,
                object: Some(_),
            } => Some(*player),
            CommittedEvent::ZoneChange {
                object,
                from: Zone::Library,
                to: Zone::Hand,
                ..
            } => Some(self.owner(*object)),
            _ => None,
        };
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
                    self.transfers.push(TransferV1 {
                        item: self.item,
                        owner: *player,
                        dir: TransferDirV1::Draw,
                    });
                    self.top_draws.push((self.item, *player));
                }
                if from_library {
                    self.draw_from_top(*player, !named && !was);
                }
                // Prior tracking identifies the moved card only if V still
                // tracks it after the move (a known library card drawn); an
                // unidentified departure never names it.
                let observed: Vec<ObjectId> = if named || (was && self.is_tracked_now(id)) {
                    vec![id]
                } else {
                    vec![]
                };
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
                // Whether V knows anything about the hand a card leaves.
                let informative = owner == self.opp()
                    && (!self.hand.is_empty() || !self.survivors.is_empty() || self.r8_latch);
                self.moved(id, *to);
                let public = is_public(*from) || is_public(*to);
                let named = public || owner == self.viewer;
                if !token_or_copy {
                    self.hand_move(id, owner, *from, *to, named);
                }
                if *from == Zone::Library && *to == Zone::Hand && !named && !was {
                    self.transfers.push(TransferV1 {
                        item: self.item,
                        owner,
                        dir: TransferDirV1::Draw,
                    });
                }
                let put_back = *from == Zone::Hand && *to == Zone::Library && !named;
                if put_back {
                    let record = if owner == self.opp() {
                        std::mem::take(&mut self.last_departure)
                    } else {
                        Vec::new()
                    };
                    self.put_backs_on_top[owner.index()].push(PutBackV1 {
                        informative,
                        record,
                    });
                    // On top only when the insertion note says position 0;
                    // anywhere else (or unknown) the order is uncertain.
                    let at_top = positions.iter().any(|n| {
                        matches!(n.kind, ObservationNoteKindV1::LibraryInsertion { owner: o, position: 0 } if o == owner)
                    });
                    if !at_top {
                        self.put_backs_scrambled[owner.index()] = true;
                    }
                } else if (*from == Zone::Library || *to == Zone::Library)
                    && !self.put_backs_on_top[owner.index()].is_empty()
                {
                    // Any other move into or out of that library (a search,
                    // a mill, an identified card put on top) disturbs the
                    // put-back cards' positions.
                    self.put_backs_scrambled[owner.index()] = true;
                }
                if put_back {
                    self.transfers.push(TransferV1 {
                        item: self.item,
                        owner,
                        dir: TransferDirV1::PutBack,
                    });
                }
                // Prior tracking identifies the moved card only if V still
                // tracks it after the move (a known library card drawn); an
                // unidentified departure never names it.
                let observed: Vec<ObjectId> = if named || (was && self.is_tracked_now(id)) {
                    vec![id]
                } else {
                    vec![]
                };
                self.settle(&observed);
            }
            CommittedEvent::SpellCast { spell, .. } => {
                let id = *spell;
                let copy = self.state.objects.get(id).spell_copy_origin.is_some();
                let o = &mut self.objs[id.0 as usize];
                if copy {
                    // A spell copy is created on the stack at generation 0.
                    o.exists = true;
                    o.zone = Some(Zone::Stack);
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
                // A token is created on the battlefield at generation 0.
                let id = *object;
                let o = &mut self.objs[id.0 as usize];
                o.exists = true;
                o.zone = Some(Zone::Battlefield);
                self.settle(&[id]);
            }
            _ => self.settle(&[]),
        }
    }

    fn note(&mut self, note: &ObservationNoteV1) {
        let viewer = self.viewer;
        match &note.kind {
            ObservationNoteKindV1::LibraryRandomized { owner } => {
                self.reorders.push((self.item, *owner));
                let records: Vec<u64> = self.put_backs_on_top[owner.index()]
                    .drain(..)
                    .flat_map(|x| x.record)
                    .collect();
                if *owner == self.opp() {
                    self.survivors.unpin(&records, &[]);
                }
                self.put_backs_scrambled[owner.index()] = false;
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
                self.reorders.push((self.item, *owner));
                // Put-back cards stay among the top cards: no reset; their
                // positions are no longer certain (R8 latch).
                if !self.put_backs_on_top[owner.index()].is_empty() {
                    self.put_backs_scrambled[owner.index()] = true;
                }
                if *owner == self.opp()
                    && self.put_backs_on_top[owner.index()]
                        .iter()
                        .any(|x| x.informative)
                {
                    self.r8_latch = true;
                }
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
                self.reorders.push((self.item, *owner));
                // Put-back cards stay among the top cards: no reset; their
                // positions are no longer certain (R8 latch).
                if !self.put_backs_on_top[owner.index()].is_empty() {
                    self.put_backs_scrambled[owner.index()] = true;
                }
                if *owner == self.opp()
                    && self.put_backs_on_top[owner.index()]
                        .iter()
                        .any(|x| x.informative)
                {
                    self.r8_latch = true;
                }
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
                // A search-and-reveal outcome: the reveal follows the owner's
                // library-to-hand moves (a single search or a batch). It
                // locates a card outside the hand: R8b, applied before the
                // in-place reveal update below (ruling 44). Recognized from
                // the public event order, never from the card's identity.
                if !was && self.last_event_library_to_hand == Some(*owner) {
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
                // Tracking settles with the move this note belongs to, so an
                // identified move keeps its span.
            }
            ObservationNoteKindV1::LibraryInsertion { owner, position } => {
                let lib = std::mem::take(&mut self.library[owner.index()]);
                self.library[owner.index()] = lib
                    .into_iter()
                    .map(|(p, o)| (if p >= *position { p + 1 } else { p }, o))
                    .collect();
            }
        }
    }

    /// A draw from the top of `player`'s library. With the order certain it
    /// takes the newest put-back's card: an unidentified draw of a card from
    /// a hand V knew about undoes that put-back's removal (the draw-back
    /// amendment to 6.4, Sol-reviewed); an identified one just retires the
    /// record. If the undo cannot be done soundly, or the order is uncertain
    /// while an informative put-back remains, R8 latches (ruling 40.4).
    fn draw_from_top(&mut self, player: PlayerId, unidentified: bool) {
        let opp = self.opp();
        let p = player.index();
        if self.put_backs_scrambled[p] {
            let informative = self.put_backs_on_top[p].iter().any(|x| x.informative);
            if unidentified && informative && player == opp {
                self.r8_latch = true;
            }
            return;
        }
        let Some(top) = self.put_backs_on_top[p].pop() else {
            return;
        };
        if player != opp {
            return;
        }
        let undone =
            !unidentified || !top.informative || self.survivors.undo_departure(&top.record);
        let keep: Vec<u64> = self.put_backs_on_top[p]
            .iter()
            .flat_map(|x| x.record.iter().copied())
            .collect();
        self.survivors.unpin(&top.record, &keep);
        if !undone {
            self.r8_latch = true;
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
                ambiguous: false,
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
            ambiguous: false,
        });
    }

    /// Amendment 43.6: before a copy leaves the opponent's hand, record
    /// every group of two or more bound facts with one name and one fact
    /// kind that V cannot tell apart at this departure: the departing
    /// copy's own group when it leaves identified by name, else every such
    /// group (an unidentified departure could be any card).
    fn note_hand_ambiguity(&mut self, id: ObjectId, card_def: u16, identified: bool) {
        let item = self.item;
        let departing_how = self
            .hand
            .iter()
            .find(|f| f.native == Some(id))
            .map(|f| f.how);
        let mut groups: Vec<(u16, FactHowV1)> = Vec::new();
        for f in &self.hand {
            if f.native.is_none() || groups.contains(&(f.card_def, f.how)) {
                continue;
            }
            let wanted = if identified {
                f.card_def == card_def && departing_how.is_none_or(|h| h == f.how)
            } else {
                true
            };
            if wanted {
                groups.push((f.card_def, f.how));
            }
        }
        for (def, how) in groups {
            let candidates: Vec<(ObjectId, u32)> = self
                .hand
                .iter()
                .filter(|f| f.card_def == def && f.how == how)
                .filter_map(|f| f.native)
                .map(|o| {
                    // The departing copy was already moved (its epoch is
                    // the destination's): record its epoch in the hand.
                    let moves = self.objs[o.0 as usize].moves;
                    (
                        o,
                        if o == id {
                            moves.saturating_sub(1)
                        } else {
                            moves
                        },
                    )
                })
                .collect();
            if candidates.len() >= 2 {
                self.hand_ambiguities.push(HandAmbiguityV1 {
                    item,
                    card_def: def,
                    candidates,
                });
            }
        }
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
                    ambiguous: false,
                });
            } else if from == Zone::Library {
                self.library_to_hand(id, owner);
            }
            return;
        }
        if from != Zone::Hand || to == Zone::Hand {
            return;
        }
        self.note_hand_ambiguity(id, card_def, is_public(to) || named);
        let tracked = self.objs[id.0 as usize].tracked;
        if is_public(to) || named {
            // Identified as it leaves: drop its own fact, else one count fact
            // with its name. A fact bound to another copy stays: V tells the
            // copies apart by their public object ids, and the departure of
            // a different object does not end that copy's span (6.1). The
            // engine's own knowledge rows keep it too (R5 compares them).
            let named_facts = || self.hand.iter().filter(|f| f.card_def == card_def);
            // A marked fact keeps the ambiguity: which fact this departure
            // ends may again differ between the worlds (Sol round 1).
            let mixed = (named_facts().any(|f| f.native.is_some())
                && named_facts().any(|f| f.native.is_none()))
                || named_facts().any(|f| f.ambiguous);
            let pos = self
                .hand
                .iter()
                .position(|f| f.native == Some(id))
                .or_else(|| {
                    self.hand
                        .iter()
                        .position(|f| f.native.is_none() && f.card_def == card_def)
                });
            if let Some(pos) = pos {
                self.hand.remove(pos);
            }
            if mixed {
                // Lead ruling 11 (R2): V held a bound and an unbound fact of
                // this name, so which one this departure ended is not V's
                // information. Every remaining fact of the name is marked.
                for f in self.hand.iter_mut().filter(|f| f.card_def == card_def) {
                    f.ambiguous = true;
                }
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
        self.last_departure = self
            .survivors
            .unidentified_departure(&tracked_names, to == Zone::Library);
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
