//! `GameSummaryV1` extractor (design section 3, W3): folds
//! `state.engine.event_history` plus the legal-cast hook into one
//! self-describing, checkpoint-identified row per game per side. Runs
//! in-loop with the same episode driver W2 and W5 use.

use crate::event::CommittedEvent;
use crate::ids::PlayerId;
use crate::rl_session::{RlEpisodeSessionV1, RlSessionDecisionV1, RlSessionResponseV1};
use crate::rl::ActionSemanticV1;
use crate::state::{Target, Zone};
use serde::Serialize;
use std::collections::BTreeMap;

pub const GAME_SUMMARY_SCHEMA_V1: u32 = 1;

#[derive(Debug, Clone, Serialize)]
pub struct OpponentEvidenceRowV1 {
    pub card_id: u16,
    pub first_seen_turn: u32,
    pub end_of_game_zone: Zone,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct OwnCardOutcomeV1 {
    pub times_drawn: u8,
    pub cast: bool,
    pub stuck_in_hand: bool,
    pub died_without_dealing_damage: bool,
    pub removal_no_target: bool,
    pub counterspell_held: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ResourceCurveV1 {
    pub lands_by_turn: Vec<u32>,
    pub hand_size_by_turn: [Vec<u32>; 2],
    /// `i32`, matching `PlayerState.life: i32` exactly (`state.rs:353`) so
    /// the driver loop can push `state.players[seat].life` with no cast and
    /// no truncation risk; design section 3 writes this as `[Vec<i16>; 2]`,
    /// but the live field it is sourced from is `i32`, so this plan widens
    /// the type to match the real source rather than truncating on push.
    pub life_by_turn: [Vec<i32>; 2],
    /// Fix round 1, item 2: the fix brief indexes this as `first_attack_turn[seat]`
    /// in the same sentence as `damage_dealt_total`/`damage_taken_total`, both
    /// genuinely `[T; 2]`; the pre-fix declaration here was a bare
    /// `Option<u32>`, which cannot hold two independent seats. Widened to
    /// match its siblings and the design's per-seat "indexed by the turn
    /// watermark" framing (section 3): each seat's own first-attack turn is
    /// independently meaningful (an aggro deck's curve looks nothing like a
    /// control deck's), and collapsing them to one shared value would silently
    /// discard which seat attacked first.
    pub first_attack_turn: [Option<u32>; 2],
    pub damage_dealt_total: [i64; 2],
    pub damage_taken_total: [i64; 2],
}

#[derive(Debug, Clone, Serialize)]
pub struct GameSummaryV1 {
    pub schema_version: u32,
    pub checkpoint_weights_hash: String,
    pub checkpoint_git_head: String,
    pub winner: Option<PlayerId>,
    pub opponent_evidence: [Vec<OpponentEvidenceRowV1>; 2],
    pub own_card_outcomes: [BTreeMap<u16, OwnCardOutcomeV1>; 2],
    pub resource_curve: ResourceCurveV1,
}

/// The requires-target/is-counterspell ground truth (Task C, W4). A small
/// slice type so `game_summary_v1.rs` does not need to know how the tag
/// file is loaded from disk; the caller (Task E's driver) loads it once
/// per campaign, not once per game.
pub struct RemovalCounterspellTagsV1 {
    pub requires_target: std::collections::BTreeSet<u16>,
    pub is_counterspell: std::collections::BTreeSet<u16>,
}

fn was_card_offered_as_hand_cast_v1(semantic: &ActionSemanticV1, card_id: u16) -> bool {
    matches!(
        semantic,
        ActionSemanticV1::CastSpell { source, .. }
            if source.card_db_id == card_id && source.zone == Zone::Hand
    )
}

/// `ActionSemanticV1::CastSpell.actor` (`rl.rs:882`) and `CardStableRefV1`'s
/// own `owner`/`controller` (`rl.rs:207-208`) are `PlayerSeatV1`, the
/// RL-facing seat type; every `CommittedEvent` field that names a player
/// (`Draw.player`, `SpellCast.controller`, `ZoneChange.controller_before`)
/// is `PlayerId`, the engine-internal type (`ids.rs:35`, `event.rs`). The
/// two are related only by `impl From<PlayerId> for PlayerSeatV1`
/// (`rl.rs:184`) and have no reverse impl, so this task keeps two small,
/// separately named conversions rather than silently coercing one into the
/// other at a call site.
fn seat_index_v1(seat: crate::rl::PlayerSeatV1) -> usize {
    match seat {
        crate::rl::PlayerSeatV1::P0 => 0,
        crate::rl::PlayerSeatV1::P1 => 1,
    }
}

fn seat_index_from_player_id_v1(player: PlayerId) -> usize {
    player.0 as usize
}

fn count_lands_v1(state: &crate::state::GameState) -> u32 {
    state
        .objects
        .iter()
        .filter(|(_, object)| {
            object.zone == Zone::Battlefield
                && crate::card_def::CARD_DEFS[object.card_def as usize].is_land
        })
        .count() as u32
}

/// `Arena::push` (`ids.rs:72-76`) always assigns a new, strictly increasing
/// id and nothing ever removes an entry, so every `ObjectId` allocated this
/// game -- drawn, cast, milled, or created as a token mid-game -- is still
/// present in the terminal state's `objects` arena, only possibly in a
/// different zone, and `card_def` never changes for a given `ObjectId`
/// after creation. One pass over the terminal arena is therefore exactly
/// equivalent to incrementally rebuilding the map on every
/// `CreateToken`/`ZoneChange` event, and simpler.
fn build_object_card_def_map_v1(
    state: &crate::state::GameState,
) -> BTreeMap<crate::ids::ObjectId, u16> {
    state.objects.iter().map(|(id, object)| (id, object.card_def)).collect()
}

/// Same one-pass-over-the-terminal-arena rationale as
/// `build_object_card_def_map_v1` immediately above, but for `owner`
/// (`GameObject.owner: PlayerId`, `state.rs:234`) instead of `card_def`:
/// `owner` is fixed at object creation and never reassigned by a
/// control-change effect (only `controller` moves), so the terminal value is
/// exactly the value at any earlier point in the game too. Item 2 (fix round
/// 1) needs this to attribute `Damage`/`CombatDamageToPlayer` events to the
/// seat that owns the dealing or receiving object.
fn build_object_owner_map_v1(
    state: &crate::state::GameState,
) -> BTreeMap<crate::ids::ObjectId, PlayerId> {
    state.objects.iter().map(|(id, object)| (id, object.owner)).collect()
}

/// Every non-token object this game whose `owner` is `seat`: exactly the
/// distinct card ids of that seat's own registered 60 (a copy count > 1
/// collapses to one entry, which is what `own_card_outcomes`, keyed by
/// `card_id`, needs). Tokens are excluded because they are not registered
/// deck cards.
fn own_registered_card_ids_v1(
    state: &crate::state::GameState,
    seat: usize,
) -> std::collections::BTreeSet<u16> {
    let seat_player = PlayerId(seat as u8);
    state
        .objects
        .iter()
        .filter(|(_, object)| {
            object.owner == seat_player && !crate::card_def::CARD_DEFS[object.card_def as usize].is_token
        })
        .map(|(_, object)| object.card_def)
        .collect()
}

/// No `CommittedEvent` variant carries a turn stamp (design section 3), so
/// `run_episode_with_summary_v1` builds `turn_watermarks`, one
/// `(event_history.len(), turn)` pair recorded the first time each turn
/// reaches a decision. `event_index` is attributed to the smallest recorded
/// turn whose watermark index exceeds it (the earliest turn boundary
/// reached after the event committed); an event at or after the last
/// watermark is attributed to `final_turn`, the turn the game ended on.
fn turn_for_event_index_v1(turn_watermarks: &[(usize, u32)], final_turn: u32, event_index: usize) -> u32 {
    turn_watermarks
        .iter()
        .find(|&&(watermark_index, _)| watermark_index > event_index)
        .map(|&(_, turn)| turn)
        .unwrap_or(final_turn)
}

/// This task's operational proxy for "this specific cast resolved without
/// ever choosing a target": no `CommittedEvent` variant links a `Targeted`
/// event back to the spell that caused it (`Targeted::targeting_stack_item`
/// is a `StackItemId`, and `CommittedEvent::SpellCast` carries no
/// `StackItemId` at all), so exact stack-item identity is not recoverable
/// from `event_history` alone (this function never sees `GameState.stack`
/// either, which is empty by the terminal state this extractor actually
/// runs against). This proxy is windowed instead.
///
/// Fix round 1, item 1 (Critical): the pre-fix window was `[cast_index,
/// leave_index)`, forward only. But `finalize_owned_cast` (`engine.rs`, the
/// sole call site of `event::log_spell_cast`) calls
/// `log_final_targeting_events` *immediately* before `event::log_spell_cast`,
/// with nothing else logged in between -- so in real play, a cast's own
/// `Targeted` events (one per battlefield-zone target contract;
/// `log_final_targeting_events` only emits `Targeted` for
/// `StackTargetContractV4::Object { zone: Zone::Battlefield, .. }`, so a
/// targeted player or a targeted spell/ability on the stack never produces a
/// `Targeted` event at all) are committed directly *before* that cast's own
/// `SpellCast`, not after it. The forward-only window therefore missed
/// nearly every real targeted cast, inverting the field.
///
/// The window now also searches backward from `cast_index`, down to the
/// index right after the nearest preceding `SpellCast` (a natural
/// stack-relevant boundary: the last time a different spell was cast), or
/// down to the start of history if this is the first cast of the game. It
/// still searches forward to `leave_index` too, unioned with the backward
/// span, in case a later retargeting or trigger-driven `Targeted` event is
/// logged while this spell is still on the stack. Known imprecision, accepted
/// because stack-item identity is unrecoverable: an unrelated ability's own
/// final-targeting burst, if activated in the gap between the previous cast
/// and this one with nothing else logged in between, would be misattributed
/// to this cast as "had a target". This is a narrower failure mode than the
/// pre-fix behavior, which was wrong for nearly every real targeted cast.
fn spell_resolved_with_no_traced_target_v1(
    event_history: &[CommittedEvent],
    cast_index: usize,
    spell: crate::ids::ObjectId,
) -> bool {
    let leave_index = event_history
        .iter()
        .enumerate()
        .skip(cast_index + 1)
        .find(|(_, event)| {
            matches!(
                event,
                CommittedEvent::ZoneChange { object, from, .. }
                    if *object == spell && *from == Zone::Stack
            )
        })
        .map(|(index, _)| index)
        .unwrap_or(event_history.len());
    let boundary_index = event_history[..cast_index]
        .iter()
        .enumerate()
        .rev()
        .find(|(_, event)| matches!(event, CommittedEvent::SpellCast { .. }))
        .map(|(index, _)| index + 1)
        .unwrap_or(0);
    !event_history[boundary_index..leave_index]
        .iter()
        .any(|event| matches!(event, CommittedEvent::Targeted { .. }))
}

/// Every card_def in `seat`'s hand at the state this is called against
/// (the terminal state, in `run_episode_with_summary_v1`'s real call site;
/// a hand-built set in the exact-value unit tests below).
fn end_of_game_hand_card_ids_v1(
    state: &crate::state::GameState,
    object_card_def: &BTreeMap<crate::ids::ObjectId, u16>,
    seat: usize,
) -> std::collections::BTreeSet<u16> {
    state.players[seat]
        .hand
        .iter()
        .filter_map(|object_id| object_card_def.get(object_id).copied())
        .collect()
}

/// Folds the full `event_history` plus the live legal-cast hook
/// (`offered_as_cast`, built by `run_episode_with_summary_v1`'s driver loop)
/// into `opponent_evidence` and `own_card_outcomes` (design section 3).
/// Runs once, at episode end. Deliberately takes plain data
/// (`event_history`, the `ObjectId -> card_def` map, each seat's own
/// registered ids, each seat's end-of-game hand) rather than `&GameState`
/// directly: `run_episode_with_summary_v1`'s real call site builds these
/// from the terminal state (via `build_object_card_def_map_v1`,
/// `own_registered_card_ids_v1`, `end_of_game_hand_card_ids_v1` above), and
/// this module's tests build them by hand from a synthetic event list, with
/// no need to construct a full `GameState`.
fn fold_event_history_v1(
    event_history: &[CommittedEvent],
    final_turn: u32,
    turn_watermarks: &[(usize, u32)],
    object_card_def: &BTreeMap<crate::ids::ObjectId, u16>,
    object_owner: &BTreeMap<crate::ids::ObjectId, PlayerId>,
    own_registered_ids: &[std::collections::BTreeSet<u16>; 2],
    end_of_game_hand_card_ids: &[std::collections::BTreeSet<u16>; 2],
    offered_as_cast: &[std::collections::BTreeSet<u16>; 2],
    tags: &RemovalCounterspellTagsV1,
    resource_curve: &mut ResourceCurveV1,
) -> ([Vec<OpponentEvidenceRowV1>; 2], [BTreeMap<u16, OwnCardOutcomeV1>; 2]) {
    let mut opponent_first_seen: [BTreeMap<u16, u32>; 2] = Default::default();
    let mut opponent_end_of_game_zone: [BTreeMap<u16, Zone>; 2] = Default::default();
    let mut times_drawn: [BTreeMap<u16, u8>; 2] = Default::default();
    let mut cast_ids: [std::collections::BTreeSet<u16>; 2] = Default::default();
    let mut cast_no_target: [std::collections::BTreeSet<u16>; 2] = Default::default();
    // Item 3 (Important): keyed by `(ObjectId, incarnation)`, not `ObjectId`
    // alone -- the engine reuses an `ObjectId` across incarnations
    // (`CardStableRefV1`/`Targeted`/`SagaChapter`/`CombatDamageToPlayer` all
    // carry a `zone_change_count` for exactly this reason). `object_incarnation`
    // below is this fold's own running per-object incarnation counter, since
    // `Damage` and `ZoneChange` carry no such count directly.
    let mut ever_dealt_damage: std::collections::BTreeSet<(crate::ids::ObjectId, u32)> = Default::default();
    let mut died_without_damage: [std::collections::BTreeSet<u16>; 2] = Default::default();
    // Tracks each object's incarnation number as this fold walks
    // `event_history` forward, incremented once per `ZoneChange` event that
    // names it (mirroring `GameObject.zone_change_count`'s own per-move
    // increment, `state.rs:1447` and siblings). This never sees the two zone
    // moves the engine special-cases as uncommitted (a private
    // library-to-hand draw, and an announced cast's Hand->Stack move -- see
    // this module's own privacy-proof test doc), so its absolute values can
    // run behind the engine's true `zone_change_count` by a constant offset
    // picked up before an object's first appearance on a public zone. That
    // offset does not affect correctness here: `CombatDamageToPlayer`
    // resyncs this tracker to the engine's own authoritative
    // `source_zone_change_count` the moment it is observed (see below), and
    // once an object is battlefield-resident every zone move away from a
    // public zone is itself a committed, visible `ZoneChange`, so the
    // tracker and the engine agree for the remainder of that object's life.
    // This is exactly what item 3's own required test exercises: a creature
    // that deals damage, dies, returns, and dies again without damage must
    // key its two deaths differently.
    let mut object_incarnation: BTreeMap<crate::ids::ObjectId, u32> = Default::default();

    for (index, event) in event_history.iter().enumerate() {
        match event {
            CommittedEvent::Draw { player, object: Some(object_id) } => {
                let seat = seat_index_from_player_id_v1(*player);
                if let Some(&card_id) = object_card_def.get(object_id) {
                    if own_registered_ids[seat].contains(&card_id) {
                        *times_drawn[seat].entry(card_id).or_insert(0) += 1;
                    }
                }
            }
            CommittedEvent::SpellCast { spell, controller } => {
                let seat = seat_index_from_player_id_v1(*controller);
                if let Some(&card_id) = object_card_def.get(spell) {
                    if own_registered_ids[seat].contains(&card_id) {
                        cast_ids[seat].insert(card_id);
                        if spell_resolved_with_no_traced_target_v1(event_history, index, *spell) {
                            cast_no_target[seat].insert(card_id);
                        }
                    }
                }
            }
            CommittedEvent::Damage { source, target, amount } => {
                // Item 4 (Important): only a positive amount counts as
                // "dealt damage" toward `ever_dealt_damage` / the
                // `died_without_dealing_damage` signal it feeds.
                if *amount > 0 {
                    let incarnation = *object_incarnation.get(source).unwrap_or(&0);
                    ever_dealt_damage.insert((*source, incarnation));
                }
                // Item 2 (Important): `damage_dealt_total`/`damage_taken_total`
                // fold from `Damage` only, not also `CombatDamageToPlayer`.
                // `CombatDamageToPlayer` is a nonreplaceable marker built
                // directly from the just-committed `Damage` events of the
                // same combat-damage batch (`engine.rs`, the combat-damage
                // step collects `event_log` for `Damage { target:
                // Target::Player, .. }` immediately after committing them,
                // then logs one `CombatDamageToPlayer` per such event with
                // the same `source`/`player`/`amount`) -- so a player-facing
                // combat-damage instance produces *both* events for the same
                // damage. Summing both here would double-count every
                // player-facing combat hit relative to damage to a permanent
                // or non-combat damage to a player, which only ever produce
                // a `Damage` event. `Damage` alone already covers every
                // damage instance in the game (combat and non-combat, to a
                // player or a permanent); `CombatDamageToPlayer` is used
                // below only for its own extra, `Damage`-lacking information
                // (an authoritative `source_zone_change_count`, and "this
                // was combat damage to a player" for `first_attack_turn`).
                if let Some(&owner) = object_owner.get(source) {
                    resource_curve.damage_dealt_total[seat_index_from_player_id_v1(owner)] +=
                        i64::from(*amount);
                }
                match target {
                    Target::Player(player) => {
                        resource_curve.damage_taken_total[seat_index_from_player_id_v1(*player)] +=
                            i64::from(*amount);
                    }
                    Target::Object(target_object) => {
                        if let Some(&owner) = object_owner.get(target_object) {
                            resource_curve.damage_taken_total[seat_index_from_player_id_v1(owner)] +=
                                i64::from(*amount);
                        }
                    }
                }
            }
            CommittedEvent::CombatDamageToPlayer { source, source_zone_change_count, player: _, amount } => {
                if *amount > 0 {
                    ever_dealt_damage.insert((*source, *source_zone_change_count));
                }
                // Resync this fold's own incarnation tracker to the engine's
                // authoritative value now that it is known (see the
                // `object_incarnation` doc above).
                object_incarnation.insert(*source, *source_zone_change_count);
                // Item 2: `first_attack_turn[seat]` is the watermark turn of
                // the first `CombatDamageToPlayer` whose source is owned by
                // that seat. No `CommittedEvent` variant marks "attackers
                // declared" (all fourteen-then-fifteen variants checked
                // against `event.rs`, confirmed against the design doc's own
                // enumeration), so `CombatDamageToPlayer` -- combat damage
                // that actually landed on a player -- is the earliest
                // available signal and the one the brief names first.
                if *amount > 0 {
                    if let Some(&owner) = object_owner.get(source) {
                        let seat = seat_index_from_player_id_v1(owner);
                        if resource_curve.first_attack_turn[seat].is_none() {
                            resource_curve.first_attack_turn[seat] =
                                Some(turn_for_event_index_v1(turn_watermarks, final_turn, index));
                        }
                    }
                }
            }
            CommittedEvent::ZoneChange { object, from, to, controller_before } => {
                let Some(&card_id) = object_card_def.get(object) else { continue };
                let is_public = matches!(to, Zone::Battlefield | Zone::Graveyard | Zone::Stack | Zone::Exile);
                let is_died = matches!(to, Zone::Graveyard | Zone::Exile)
                    && !matches!(from, Zone::Graveyard | Zone::Exile);

                // Opponent evidence, from each seat's own point of view:
                // `object`'s owner (not `controller_before`, so a stolen
                // permanent still reveals its true owner's card) belonging
                // to the *other* seat, made public. Gated on the object's
                // actual `owner` (final review item 1), not on card-id
                // membership in the opponent's registered list: registered
                // decks can share card ids across archetypes (e.g. Burn and
                // Rally both register 66/76/127), so a membership-only gate
                // records a phantom opponent_evidence row for a seat's own
                // copy of a shared card the moment it becomes public. The
                // registered-id check is kept alongside the owner check as a
                // sanity filter (it should always agree for a non-token
                // object once ownership is right), but `object_owner` is the
                // authority.
                for seat in 0..2 {
                    let opponent_seat = 1 - seat;
                    let opponent_player = PlayerId(opponent_seat as u8);
                    if object_owner.get(object) == Some(&opponent_player)
                        && own_registered_ids[opponent_seat].contains(&card_id)
                        && is_public
                    {
                        opponent_first_seen[seat]
                            .entry(card_id)
                            .or_insert_with(|| turn_for_event_index_v1(turn_watermarks, final_turn, index));
                        opponent_end_of_game_zone[seat].insert(card_id, *to);
                    }
                }

                if is_died {
                    let owner_seat = seat_index_from_player_id_v1(*controller_before);
                    let incarnation = *object_incarnation.get(object).unwrap_or(&0);
                    if own_registered_ids[owner_seat].contains(&card_id)
                        && !ever_dealt_damage.contains(&(*object, incarnation))
                    {
                        died_without_damage[owner_seat].insert(card_id);
                    }
                }
                // Advance this object's tracked incarnation for every zone
                // move (not just deaths), so a later re-entry onto the
                // battlefield under the same `ObjectId` starts a fresh
                // incarnation number for the next death check.
                *object_incarnation.entry(*object).or_insert(0) += 1;
            }
            _ => {}
        }
    }

    let mut opponent_evidence: [Vec<OpponentEvidenceRowV1>; 2] = Default::default();
    for seat in 0..2 {
        for (&card_id, &first_seen_turn) in &opponent_first_seen[seat] {
            opponent_evidence[seat].push(OpponentEvidenceRowV1 {
                card_id,
                first_seen_turn,
                end_of_game_zone: opponent_end_of_game_zone[seat][&card_id],
            });
        }
    }

    let mut own_card_outcomes: [BTreeMap<u16, OwnCardOutcomeV1>; 2] = Default::default();
    for seat in 0..2 {
        for &card_id in &own_registered_ids[seat] {
            let stuck_in_hand = end_of_game_hand_card_ids[seat].contains(&card_id);
            let cast = cast_ids[seat].contains(&card_id);
            let removal_no_target = tags.requires_target.contains(&card_id)
                && cast
                && cast_no_target[seat].contains(&card_id);
            let counterspell_held = tags.is_counterspell.contains(&card_id)
                && !offered_as_cast[seat].contains(&card_id)
                && stuck_in_hand;
            own_card_outcomes[seat].insert(
                card_id,
                OwnCardOutcomeV1 {
                    times_drawn: *times_drawn[seat].get(&card_id).unwrap_or(&0),
                    cast,
                    stuck_in_hand,
                    died_without_dealing_damage: died_without_damage[seat].contains(&card_id),
                    removal_no_target,
                    counterspell_held,
                },
            );
        }
    }

    (opponent_evidence, own_card_outcomes)
}

/// Item 7 (Minor): the watermark-recording step run once per decision
/// boundary, factored out of `run_episode_with_summary_v1`'s driver loop so
/// a test can drive a real session and inspect the resulting
/// `turn_watermarks` directly (this exact function, not a re-implementation
/// of it), instead of only exercising it indirectly through the full
/// `GameSummaryV1` it eventually feeds. Appends one `(event_history.len(),
/// turn)` pair the first time a new turn is reached, and pushes this turn's
/// `lands_by_turn`/`hand_size_by_turn`/`life_by_turn` entries onto
/// `resource_curve` in the same step (unchanged from the pre-fix inline
/// logic).
fn record_turn_watermark_if_new_v1(
    session: &RlEpisodeSessionV1,
    turn_watermarks: &mut Vec<(usize, u32)>,
    last_turn: &mut u32,
    resource_curve: &mut ResourceCurveV1,
) {
    let turn = session.game_state().turn;
    if turn != *last_turn || turn_watermarks.is_empty() {
        let state = session.game_state();
        turn_watermarks.push((state.engine.event_history.len(), turn));
        resource_curve.lands_by_turn.push(count_lands_v1(state));
        for seat in 0..2 {
            resource_curve.hand_size_by_turn[seat]
                .push(state.players[seat].hand.len() as u32);
            // `PlayerState.life: i32` (`state.rs:353`) matches
            // `ResourceCurveV1.life_by_turn: [Vec<i32>; 2]`
            // exactly; no cast, no truncation.
            resource_curve.life_by_turn[seat].push(state.players[seat].life);
        }
        *last_turn = turn;
    }
}

pub fn run_episode_with_summary_v1(
    session: &mut RlEpisodeSessionV1,
    checkpoint_weights_hash: &str,
    tags: &RemovalCounterspellTagsV1,
    policy_fn: &mut dyn FnMut(&RlSessionDecisionV1) -> (u32, String),
) -> GameSummaryV1 {
    let mut offered_as_cast: [std::collections::BTreeSet<u16>; 2] = Default::default();
    let mut resource_curve = ResourceCurveV1::default();
    let mut turn_watermarks: Vec<(usize, u32)> = Vec::new();
    let mut last_turn = session.game_state().turn;

    loop {
        match session.current_response() {
            RlSessionResponseV1::Terminal(terminal) => {
                let final_state = session.game_state();
                let object_card_def = build_object_card_def_map_v1(final_state);
                let object_owner = build_object_owner_map_v1(final_state);
                let own_registered_ids = [
                    own_registered_card_ids_v1(final_state, 0),
                    own_registered_card_ids_v1(final_state, 1),
                ];
                let end_of_game_hand_card_ids = [
                    end_of_game_hand_card_ids_v1(final_state, &object_card_def, 0),
                    end_of_game_hand_card_ids_v1(final_state, &object_card_def, 1),
                ];
                let (opponent_evidence, own_card_outcomes) = fold_event_history_v1(
                    &final_state.engine.event_history,
                    final_state.turn,
                    &turn_watermarks,
                    &object_card_def,
                    &object_owner,
                    &own_registered_ids,
                    &end_of_game_hand_card_ids,
                    &offered_as_cast,
                    tags,
                    &mut resource_curve,
                );
                let winner = match terminal.winner {
                    None => None,
                    Some(crate::rl::PlayerSeatV1::P0) => Some(PlayerId::P0),
                    Some(crate::rl::PlayerSeatV1::P1) => Some(PlayerId::P1),
                };
                return GameSummaryV1 {
                    schema_version: GAME_SUMMARY_SCHEMA_V1,
                    checkpoint_weights_hash: checkpoint_weights_hash.to_owned(),
                    checkpoint_git_head: env!("MTG_KERNEL_BUILD_GIT_HEAD").to_owned(),
                    winner,
                    opponent_evidence,
                    own_card_outcomes,
                    resource_curve,
                };
            }
            RlSessionResponseV1::Decision(decision) => {
                record_turn_watermark_if_new_v1(session, &mut turn_watermarks, &mut last_turn, &mut resource_curve);
                for legal_action in &decision.legal_actions {
                    if let ActionSemanticV1::CastSpell { actor, source } = &legal_action.semantic {
                        if was_card_offered_as_hand_cast_v1(&legal_action.semantic, source.card_db_id) {
                            let seat_index = seat_index_v1(*actor);
                            offered_as_cast[seat_index].insert(source.card_db_id);
                        }
                    }
                }
                let (selected_index, selected_action_id) = policy_fn(&decision);
                session
                    .step(decision.episode_id, decision.step, selected_index, &selected_action_id)
                    .expect("policy-selected action is legal by construction");
            }
        }
    }
}

pub fn append_game_summary_jsonl_v1(
    path: &std::path::Path,
    summary: &GameSummaryV1,
) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(summary).expect("GameSummaryV1 always serializes");
    writeln!(file, "{line}")
}

#[cfg(test)]
#[derive(serde::Deserialize)]
struct GameSummaryRoundTripCheckV1 {
    schema_version: u32,
    checkpoint_weights_hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_decks::runtime_deck_by_id;
    use crate::state::SplitMix64;

    fn burn_and_rally() -> [Vec<u16>; 2] {
        [
            runtime_deck_by_id("Burn").unwrap().card_ids.to_vec(),
            runtime_deck_by_id("Rally").unwrap().card_ids.to_vec(),
        ]
    }

    #[test]
    fn run_episode_with_summary_produces_a_well_formed_row_for_both_sides() {
        let mainboards = burn_and_rally();
        let deck_ids = ["Burn".to_owned(), "Rally".to_owned()];
        let mut session = RlEpisodeSessionV1::reset_with_explicit_decks_and_limits(
            1, 0x7777_7777_7777_7777, 2000, 200_000, deck_ids, mainboards,
        )
        .unwrap();
        let tags = RemovalCounterspellTagsV1 {
            requires_target: Default::default(),
            is_counterspell: Default::default(),
        };
        let mut rng = SplitMix64::seed(0x8888_8888_8888_8888);
        let mut policy = |decision: &RlSessionDecisionV1| {
            let index = (rng.next_u64() as usize) % decision.legal_actions.len();
            (index as u32, decision.legal_actions[index].stable_id.clone())
        };
        let summary = run_episode_with_summary_v1(&mut session, "deadbeefdeadbeef", &tags, &mut policy);
        assert_eq!(summary.schema_version, GAME_SUMMARY_SCHEMA_V1);
        assert_eq!(summary.checkpoint_weights_hash, "deadbeefdeadbeef");
        assert!(!summary.checkpoint_git_head.is_empty());
        assert!(summary.resource_curve.lands_by_turn.iter().sum::<u32>() > 0, "some land entered play over a full game");
        // own_card_outcomes must only ever key cards from that seat's own
        // registered mainboard, never the opponent's.
        let burn_ids: std::collections::BTreeSet<u16> = runtime_deck_by_id("Burn").unwrap().card_ids.iter().copied().collect();
        for &card_id in summary.own_card_outcomes[0].keys() {
            assert!(burn_ids.contains(&card_id), "P0 own_card_outcomes leaked a non-Burn card_id {card_id}");
        }
    }

    #[test]
    fn legal_cast_hook_marks_a_card_offered_as_a_cast_even_if_never_cast() {
        // A minimal, direct check of the hook logic in isolation: build a
        // synthetic legal_actions list with one CastSpell semantic for a
        // known card_id in Zone::Hand and confirm the folding function
        // records it, independent of a full episode run.
        let semantic = ActionSemanticV1::CastSpell {
            actor: PlayerId::P0.into(),
            source: crate::rl::CardStableRefV1 {
                arena_id: 1,
                card_db_id: 42,
                owner: PlayerId::P0.into(),
                controller: PlayerId::P0.into(),
                zone: Zone::Hand,
                zone_change_count: 0,
            },
        };
        let offered = was_card_offered_as_hand_cast_v1(&semantic, 42);
        assert!(offered);
        let not_offered = was_card_offered_as_hand_cast_v1(&semantic, 43);
        assert!(!not_offered);
    }

    #[test]
    fn fold_event_history_records_opponent_evidence_both_directions_and_a_no_target_removal_cast() {
        use crate::ids::ObjectId;

        // A four-event synthetic history: P0 draws its own removal spell
        // (card_id 200, requires_target), P1's creature (card_id 100)
        // enters the battlefield turn 2 (revealing it to P0), P0 casts the
        // removal spell, and it resolves straight to the graveyard with no
        // `Targeted` event ever logged in its casting window (no legal
        // target was ever chosen).
        let event_history = vec![
            CommittedEvent::Draw { player: PlayerId::P0, object: Some(ObjectId(20)) }, // index 0
            CommittedEvent::ZoneChange {
                object: ObjectId(10),
                from: Zone::Library,
                to: Zone::Battlefield,
                controller_before: PlayerId::P1,
            }, // index 1: P1's card_id 100 becomes public
            CommittedEvent::SpellCast { spell: ObjectId(20), controller: PlayerId::P0 }, // index 2
            CommittedEvent::ZoneChange {
                object: ObjectId(20),
                from: Zone::Stack,
                to: Zone::Graveyard,
                controller_before: PlayerId::P0,
            }, // index 3: resolves with no Targeted event anywhere in [2, 3)
        ];
        // Turn 1 starts at history length 0; turn 2 starts at history
        // length 2 (after the Draw and the opponent's ZoneChange, before
        // the SpellCast); the game ends on turn 3.
        let turn_watermarks = vec![(0usize, 1u32), (2usize, 2u32)];
        let final_turn = 3u32;
        let object_card_def: BTreeMap<ObjectId, u16> =
            [(ObjectId(10), 100u16), (ObjectId(20), 200u16)].into_iter().collect();
        let own_registered_ids: [std::collections::BTreeSet<u16>; 2] = [
            [200u16].into_iter().collect(),
            [100u16].into_iter().collect(),
        ];
        let end_of_game_hand_card_ids: [std::collections::BTreeSet<u16>; 2] = Default::default();
        let offered_as_cast: [std::collections::BTreeSet<u16>; 2] =
            [[200u16].into_iter().collect(), Default::default()];
        let tags = RemovalCounterspellTagsV1 {
            requires_target: [200u16].into_iter().collect(),
            is_counterspell: Default::default(),
        };
        let object_owner: BTreeMap<ObjectId, PlayerId> =
            [(ObjectId(10), PlayerId::P1), (ObjectId(20), PlayerId::P0)].into_iter().collect();
        let mut resource_curve = ResourceCurveV1::default();

        let (opponent_evidence, own_card_outcomes) = fold_event_history_v1(
            &event_history,
            final_turn,
            &turn_watermarks,
            &object_card_def,
            &object_owner,
            &own_registered_ids,
            &end_of_game_hand_card_ids,
            &offered_as_cast,
            &tags,
            &mut resource_curve,
        );

        // P0's evidence about the opponent (P1's card_id 100): first seen
        // turn 2, the earliest turn boundary reached after index 1.
        assert_eq!(opponent_evidence[0].len(), 1);
        assert_eq!(opponent_evidence[0][0].card_id, 100);
        assert_eq!(opponent_evidence[0][0].first_seen_turn, 2);
        assert_eq!(opponent_evidence[0][0].end_of_game_zone, Zone::Battlefield);

        // P1's evidence about the opponent (P0's card_id 200): first seen
        // turn 3 (no watermark index exceeds 3, so it falls back to
        // final_turn), ending in the graveyard.
        assert_eq!(opponent_evidence[1].len(), 1);
        assert_eq!(opponent_evidence[1][0].card_id, 200);
        assert_eq!(opponent_evidence[1][0].first_seen_turn, 3);
        assert_eq!(opponent_evidence[1][0].end_of_game_zone, Zone::Graveyard);

        // P0's own_card_outcomes for its removal spell: drawn once, cast,
        // not stuck in hand, and removal_no_target true (it was cast, it
        // is tagged requires_target, and no Targeted event ever fired in
        // its casting window).
        let outcome = &own_card_outcomes[0][&200];
        assert_eq!(outcome.times_drawn, 1);
        assert!(outcome.cast);
        assert!(!outcome.stuck_in_hand);
        assert!(outcome.removal_no_target, "cast with no traced target must be recorded");
        assert!(!outcome.counterspell_held, "not tagged is_counterspell, so this field is always false here");
    }

    #[test]
    fn fold_event_history_records_removal_no_target_false_when_targeted_precedes_spell_cast_in_real_engine_order() {
        use crate::ids::{ObjectId, StackItemId};

        // Item 1 (Critical): the real engine order. `finalize_owned_cast`
        // (`engine.rs`) calls `log_final_targeting_events` *immediately*
        // before `event::log_spell_cast`, so a cast's own `Targeted` event
        // is committed *before* its `SpellCast`, not after -- the opposite
        // of the previous test's synthetic order. P0's removal spell
        // (card_id 200, requires_target) targets P1's creature (card_id
        // 100, already on the battlefield) and resolves to the graveyard.
        let event_history = vec![
            CommittedEvent::ZoneChange {
                object: ObjectId(10),
                from: Zone::Library,
                to: Zone::Battlefield,
                controller_before: PlayerId::P1,
            }, // index 0: P1's card_id 100 enters play
            CommittedEvent::Targeted {
                target: ObjectId(10),
                target_zone_change_count: 0,
                targeting_stack_item: StackItemId(1),
                targeting_controller: PlayerId::P0,
            }, // index 1: declared before this cast's own SpellCast
            CommittedEvent::SpellCast { spell: ObjectId(20), controller: PlayerId::P0 }, // index 2
            CommittedEvent::ZoneChange {
                object: ObjectId(20),
                from: Zone::Stack,
                to: Zone::Graveyard,
                controller_before: PlayerId::P0,
            }, // index 3: resolves, having traced a target
        ];
        let turn_watermarks = vec![(0usize, 1u32)];
        let final_turn = 1u32;
        let object_card_def: BTreeMap<ObjectId, u16> =
            [(ObjectId(10), 100u16), (ObjectId(20), 200u16)].into_iter().collect();
        let object_owner: BTreeMap<ObjectId, PlayerId> =
            [(ObjectId(10), PlayerId::P1), (ObjectId(20), PlayerId::P0)].into_iter().collect();
        let own_registered_ids: [std::collections::BTreeSet<u16>; 2] = [
            [200u16].into_iter().collect(),
            [100u16].into_iter().collect(),
        ];
        let end_of_game_hand_card_ids: [std::collections::BTreeSet<u16>; 2] = Default::default();
        let offered_as_cast: [std::collections::BTreeSet<u16>; 2] =
            [[200u16].into_iter().collect(), Default::default()];
        let tags = RemovalCounterspellTagsV1 {
            requires_target: [200u16].into_iter().collect(),
            is_counterspell: Default::default(),
        };
        let mut resource_curve = ResourceCurveV1::default();

        let (_opponent_evidence, own_card_outcomes) = fold_event_history_v1(
            &event_history,
            final_turn,
            &turn_watermarks,
            &object_card_def,
            &object_owner,
            &own_registered_ids,
            &end_of_game_hand_card_ids,
            &offered_as_cast,
            &tags,
            &mut resource_curve,
        );

        let outcome = &own_card_outcomes[0][&200];
        assert!(outcome.cast);
        assert!(
            !outcome.removal_no_target,
            "a Targeted event logged before this cast's own SpellCast (the real engine order) must still count as a traced target"
        );
    }

    #[test]
    fn fold_event_history_populates_resource_curve_damage_and_first_attack_turn_fields() {
        use crate::ids::ObjectId;

        // Item 2 (Important): exact-value coverage for `first_attack_turn`,
        // `damage_dealt_total`, `damage_taken_total`. P0's creature
        // (ObjectId 40) deals 2 non-combat damage to P1's creature (ObjectId
        // 41, a permanent target), then attacks P1 for 3 (turn 2) and again
        // for 1 (turn 3, via `turn_for_event_index_v1`'s fallback-to-final_turn
        // branch, exercising that the second attack does not overwrite
        // `first_attack_turn`); P1's creature retaliates for 5 (turn 3).
        let event_history = vec![
            CommittedEvent::Damage { source: ObjectId(40), target: Target::Object(ObjectId(41)), amount: 2 }, // index 0
            CommittedEvent::Damage { source: ObjectId(40), target: Target::Player(PlayerId::P1), amount: 3 }, // index 1
            CommittedEvent::CombatDamageToPlayer {
                source: ObjectId(40),
                source_zone_change_count: 0,
                player: PlayerId::P1,
                amount: 3,
            }, // index 2
            CommittedEvent::Damage { source: ObjectId(41), target: Target::Player(PlayerId::P0), amount: 5 }, // index 3
            CommittedEvent::CombatDamageToPlayer {
                source: ObjectId(41),
                source_zone_change_count: 0,
                player: PlayerId::P0,
                amount: 5,
            }, // index 4
            CommittedEvent::Damage { source: ObjectId(40), target: Target::Player(PlayerId::P1), amount: 1 }, // index 5
            CommittedEvent::CombatDamageToPlayer {
                source: ObjectId(40),
                source_zone_change_count: 1,
                player: PlayerId::P1,
                amount: 1,
            }, // index 6
        ];
        // Turn 1 at index 0; turn 2 at index 3; turn 3 at index 6; the game
        // ends turn 3. Per `turn_for_event_index_v1`'s own documented
        // semantics (the earliest turn boundary reached *after* the event),
        // indices 0-2 resolve to turn 2, indices 3-5 resolve to turn 3, and
        // index 6 falls back to `final_turn` (also turn 3).
        let turn_watermarks = vec![(0usize, 1u32), (3usize, 2u32), (6usize, 3u32)];
        let final_turn = 3u32;
        let object_card_def: BTreeMap<ObjectId, u16> = Default::default();
        let object_owner: BTreeMap<ObjectId, PlayerId> =
            [(ObjectId(40), PlayerId::P0), (ObjectId(41), PlayerId::P1)].into_iter().collect();
        let own_registered_ids: [std::collections::BTreeSet<u16>; 2] = Default::default();
        let end_of_game_hand_card_ids: [std::collections::BTreeSet<u16>; 2] = Default::default();
        let offered_as_cast: [std::collections::BTreeSet<u16>; 2] = Default::default();
        let tags = RemovalCounterspellTagsV1 { requires_target: Default::default(), is_counterspell: Default::default() };
        let mut resource_curve = ResourceCurveV1::default();

        fold_event_history_v1(
            &event_history,
            final_turn,
            &turn_watermarks,
            &object_card_def,
            &object_owner,
            &own_registered_ids,
            &end_of_game_hand_card_ids,
            &offered_as_cast,
            &tags,
            &mut resource_curve,
        );

        assert_eq!(resource_curve.damage_dealt_total[0], 6, "P0's creature dealt 2 + 3 + 1 across three Damage events, CombatDamageToPlayer not double-counted");
        assert_eq!(resource_curve.damage_dealt_total[1], 5, "P1's creature dealt 5");
        assert_eq!(resource_curve.damage_taken_total[0], 5, "P0 took 5 combat damage to the player");
        assert_eq!(resource_curve.damage_taken_total[1], 6, "P1 took 3 + 1 to the player and 2 to its own permanent");
        assert_eq!(resource_curve.first_attack_turn[0], Some(2), "P0's first CombatDamageToPlayer landed turn 2");
        assert_eq!(resource_curve.first_attack_turn[1], Some(3), "P1's first (and only) CombatDamageToPlayer landed turn 3");
    }

    #[test]
    fn fold_event_history_scopes_died_without_dealing_damage_by_incarnation_and_ignores_zero_amount_damage() {
        use crate::ids::ObjectId;

        // Items 3, 4, 5 (Important): card_id 501 (ObjectId 50) lives once,
        // deals damage, and dies -- `died_without_dealing_damage` must read
        // `false`. card_id 502 (ObjectId 51, the engine reusing one
        // `ObjectId` across incarnations) lives twice: its first life deals
        // damage and dies; its second life deals only a *zero*-amount
        // `Damage` event (item 4: must not count as "dealt damage") and
        // dies -- `died_without_dealing_damage` must read `true` for the
        // second death, not fall back to the first life's damage credit
        // (the pre-fix bug this item exists to close).
        let event_history = vec![
            // Card A (501): single life, deals damage, dies -> false.
            CommittedEvent::ZoneChange {
                object: ObjectId(50),
                from: Zone::Library,
                to: Zone::Battlefield,
                controller_before: PlayerId::P0,
            },
            CommittedEvent::Damage { source: ObjectId(50), target: Target::Player(PlayerId::P1), amount: 4 },
            CommittedEvent::ZoneChange {
                object: ObjectId(50),
                from: Zone::Battlefield,
                to: Zone::Graveyard,
                controller_before: PlayerId::P0,
            },
            // Card B (502): first life deals damage and dies.
            CommittedEvent::ZoneChange {
                object: ObjectId(51),
                from: Zone::Library,
                to: Zone::Battlefield,
                controller_before: PlayerId::P0,
            },
            CommittedEvent::Damage { source: ObjectId(51), target: Target::Player(PlayerId::P1), amount: 2 },
            CommittedEvent::ZoneChange {
                object: ObjectId(51),
                from: Zone::Battlefield,
                to: Zone::Graveyard,
                controller_before: PlayerId::P0,
            },
            // Card B (502): reanimated (same ObjectId, new incarnation),
            // deals zero damage, dies without ever dealing damage this life.
            CommittedEvent::ZoneChange {
                object: ObjectId(51),
                from: Zone::Graveyard,
                to: Zone::Battlefield,
                controller_before: PlayerId::P0,
            },
            CommittedEvent::Damage { source: ObjectId(51), target: Target::Player(PlayerId::P1), amount: 0 },
            CommittedEvent::ZoneChange {
                object: ObjectId(51),
                from: Zone::Battlefield,
                to: Zone::Graveyard,
                controller_before: PlayerId::P0,
            },
        ];
        let turn_watermarks = vec![(0usize, 1u32)];
        let final_turn = 1u32;
        let object_card_def: BTreeMap<ObjectId, u16> =
            [(ObjectId(50), 501u16), (ObjectId(51), 502u16)].into_iter().collect();
        let object_owner: BTreeMap<ObjectId, PlayerId> =
            [(ObjectId(50), PlayerId::P0), (ObjectId(51), PlayerId::P0)].into_iter().collect();
        let own_registered_ids: [std::collections::BTreeSet<u16>; 2] =
            [[501u16, 502u16].into_iter().collect(), Default::default()];
        let end_of_game_hand_card_ids: [std::collections::BTreeSet<u16>; 2] = Default::default();
        let offered_as_cast: [std::collections::BTreeSet<u16>; 2] = Default::default();
        let tags = RemovalCounterspellTagsV1 { requires_target: Default::default(), is_counterspell: Default::default() };
        let mut resource_curve = ResourceCurveV1::default();

        let (_opponent_evidence, own_card_outcomes) = fold_event_history_v1(
            &event_history,
            final_turn,
            &turn_watermarks,
            &object_card_def,
            &object_owner,
            &own_registered_ids,
            &end_of_game_hand_card_ids,
            &offered_as_cast,
            &tags,
            &mut resource_curve,
        );

        assert!(
            !own_card_outcomes[0][&501].died_without_dealing_damage,
            "card 501 dealt damage before it died (false case)"
        );
        assert!(
            own_card_outcomes[0][&502].died_without_dealing_damage,
            "card 502's second incarnation died without dealing damage; its first life's damage credit must not carry over (true case)"
        );
    }

    #[test]
    fn run_episode_records_turn_watermarks_monotonically_and_the_final_turn_matches_the_session() {
        // Item 7 (Minor): exercises the real `record_turn_watermark_if_new_v1`
        // function -- the same one `run_episode_with_summary_v1` calls, not
        // a re-implementation of it -- against a real, full episode, rather
        // than only the hand-authored `turn_watermarks` the exact-value
        // tests above use.
        let mainboards = burn_and_rally();
        let deck_ids = ["Burn".to_owned(), "Rally".to_owned()];
        let mut session = RlEpisodeSessionV1::reset_with_explicit_decks_and_limits(
            4, 0x9999_9999_9999_9999, 2000, 200_000, deck_ids, mainboards,
        )
        .unwrap();
        let mut rng = SplitMix64::seed(0xaaaa_aaaa_aaaa_aaaa);
        let mut resource_curve = ResourceCurveV1::default();
        let mut turn_watermarks: Vec<(usize, u32)> = Vec::new();
        let mut last_turn = session.game_state().turn;

        let final_turn = loop {
            match session.current_response() {
                RlSessionResponseV1::Terminal(_) => break session.game_state().turn,
                RlSessionResponseV1::Decision(decision) => {
                    record_turn_watermark_if_new_v1(&session, &mut turn_watermarks, &mut last_turn, &mut resource_curve);
                    let index = (rng.next_u64() as usize) % decision.legal_actions.len();
                    let selected_action_id = decision.legal_actions[index].stable_id.clone();
                    session
                        .step(decision.episode_id, decision.step, index as u32, &selected_action_id)
                        .expect("policy-selected action is legal by construction");
                }
            }
        };

        assert!(!turn_watermarks.is_empty(), "a real episode reaches at least one decision");
        for pair in turn_watermarks.windows(2) {
            let (prev_index, prev_turn) = pair[0];
            let (next_index, next_turn) = pair[1];
            assert!(next_index >= prev_index, "event_history length must be monotone non-decreasing between watermarks");
            assert!(next_turn >= prev_turn, "turn must be monotone non-decreasing between watermarks");
        }
        assert_eq!(
            turn_watermarks.last().unwrap().1,
            final_turn,
            "the last recorded watermark's turn must match the session's own turn counter at termination"
        );
    }

    #[test]
    fn append_game_summary_jsonl_writes_one_line_per_call_and_round_trips() {
        let dir = std::env::temp_dir().join(format!("game_summary_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("summaries.jsonl");
        let _ = std::fs::remove_file(&path);

        let mainboards = burn_and_rally();
        let deck_ids = ["Burn".to_owned(), "Rally".to_owned()];
        let mut session = RlEpisodeSessionV1::reset_with_explicit_decks_and_limits(
            2, 0x2222_2222_2222_2222, 2000, 200_000, deck_ids, mainboards,
        )
        .unwrap();
        let tags = RemovalCounterspellTagsV1 { requires_target: Default::default(), is_counterspell: Default::default() };
        let mut rng = SplitMix64::seed(0x3333_3333_3333_3333);
        let mut policy = |decision: &RlSessionDecisionV1| {
            let index = (rng.next_u64() as usize) % decision.legal_actions.len();
            (index as u32, decision.legal_actions[index].stable_id.clone())
        };
        let summary = run_episode_with_summary_v1(&mut session, "cafef00dcafef00d", &tags, &mut policy);
        append_game_summary_jsonl_v1(&path, &summary).unwrap();
        append_game_summary_jsonl_v1(&path, &summary).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);
        let round_tripped: GameSummaryRoundTripCheckV1 = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(round_tripped.schema_version, GAME_SUMMARY_SCHEMA_V1);
        assert_eq!(round_tripped.checkpoint_weights_hash, "cafef00dcafef00d");
    }

    /// Privacy proof (design section 3: "only player-visible information for
    /// the opponent side"). `CommittedEvent::Draw` (`event.rs:738-741`) is
    /// the *only* event a private library-to-hand move ever produces --
    /// `ProposedEvent::Draw`'s handler in `event.rs` calls `state.draw_card`
    /// directly and never builds a `ZoneChange`/`commit_zone_change` for it
    /// (unlike every other zone move in the engine), so a card that is
    /// drawn and stays in hand leaves no public trace in `event_history` at
    /// all: the same discipline covers a private top-of-library look
    /// (Delver of Secrets), which updates only `state.library_knowledge`
    /// (`state.rs:1250`), a field `fold_event_history_v1` never reads (its
    /// signature does not even accept a `&GameState`). This test exercises
    /// that guarantee end to end: P1 draws two of its own cards privately;
    /// one (301) is later discarded to the graveyard (a public
    /// `ZoneChange`) and the other (300) stays in hand for the rest of the
    /// game. P0's opponent_evidence must contain 301 (first seen at the
    /// *public* discard's turn, not the earlier private draw's turn) and
    /// must never contain 300 at all.
    #[test]
    fn fold_event_history_never_leaks_a_private_hand_card_into_opponent_evidence() {
        use crate::ids::ObjectId;

        let event_history = vec![
            CommittedEvent::Draw { player: PlayerId::P1, object: Some(ObjectId(30)) }, // index 0: private, card_id 300, stays in hand
            CommittedEvent::Draw { player: PlayerId::P1, object: Some(ObjectId(31)) }, // index 1: private, card_id 301
            CommittedEvent::ZoneChange {
                object: ObjectId(31),
                from: Zone::Hand,
                to: Zone::Graveyard,
                controller_before: PlayerId::P1,
            }, // index 2: card_id 301 discarded turn 5, made public
        ];
        // Everything happens on turn 1 except the discard, which the game
        // attributes to turn 5 via the final_turn fallback (no watermark
        // index exceeds 2).
        let turn_watermarks = vec![(0usize, 1u32)];
        let final_turn = 5u32;
        let object_card_def: BTreeMap<ObjectId, u16> =
            [(ObjectId(30), 300u16), (ObjectId(31), 301u16)].into_iter().collect();
        let own_registered_ids: [std::collections::BTreeSet<u16>; 2] =
            [Default::default(), [300u16, 301u16].into_iter().collect()];
        let end_of_game_hand_card_ids: [std::collections::BTreeSet<u16>; 2] =
            [Default::default(), [300u16].into_iter().collect()];
        let offered_as_cast: [std::collections::BTreeSet<u16>; 2] = Default::default();
        let tags = RemovalCounterspellTagsV1 { requires_target: Default::default(), is_counterspell: Default::default() };
        let object_owner: BTreeMap<ObjectId, PlayerId> =
            [(ObjectId(30), PlayerId::P1), (ObjectId(31), PlayerId::P1)].into_iter().collect();
        let mut resource_curve = ResourceCurveV1::default();

        let (opponent_evidence, own_card_outcomes) = fold_event_history_v1(
            &event_history,
            final_turn,
            &turn_watermarks,
            &object_card_def,
            &object_owner,
            &own_registered_ids,
            &end_of_game_hand_card_ids,
            &offered_as_cast,
            &tags,
            &mut resource_curve,
        );

        // P0's evidence about P1: exactly the discarded card, never the
        // one that stayed private in hand.
        assert_eq!(opponent_evidence[0].len(), 1, "only the publicly-discarded card may appear");
        assert_eq!(opponent_evidence[0][0].card_id, 301);
        assert_eq!(opponent_evidence[0][0].first_seen_turn, 5, "first_seen_turn must be the public event's turn, not the private draw's");
        assert_eq!(opponent_evidence[0][0].end_of_game_zone, Zone::Graveyard);
        assert!(
            !opponent_evidence[0].iter().any(|row| row.card_id == 300),
            "a card that never left a private zone must never appear as opponent evidence"
        );

        // P1's own bookkeeping still records both draws correctly; this is
        // private-side data, not opponent-facing.
        assert_eq!(own_card_outcomes[1][&300].times_drawn, 1);
        assert!(own_card_outcomes[1][&300].stuck_in_hand);
        assert_eq!(own_card_outcomes[1][&301].times_drawn, 1);
        assert!(!own_card_outcomes[1][&301].stuck_in_hand);
    }

    #[derive(serde::Deserialize)]
    struct RealTagFileRowV1 {
        card_id: u16,
        requires_target: bool,
        is_counterspell: bool,
    }

    #[derive(serde::Deserialize)]
    struct RealTagFileV1 {
        cards: Vec<RealTagFileRowV1>,
    }

    /// Item 6 (Important): loads the real, committed tag file the same way
    /// `mtg-kernel/tests/removal_counterspell_tags_v1.rs` (Task C's own
    /// ratified cross-check against the live engine) does: `include_str!`,
    /// so the committed JSON is embedded at compile time with no
    /// working-directory ambiguity between `cargo test` invocations. This
    /// module does not import that integration test's types directly (it is
    /// a separate `tests/` binary, not linked into the lib), so this mirrors
    /// its `TagFileV1`/`TagRowV1` shape locally instead.
    fn load_real_removal_counterspell_tags_v1() -> RemovalCounterspellTagsV1 {
        const TAG_FILE_JSON: &str = include_str!("../../data/pauper_removal_counterspell_tags_v1.json");
        let document: RealTagFileV1 = serde_json::from_str(TAG_FILE_JSON).expect("tag file parses as RealTagFileV1");
        let mut requires_target = std::collections::BTreeSet::new();
        let mut is_counterspell = std::collections::BTreeSet::new();
        for row in document.cards {
            if row.requires_target {
                requires_target.insert(row.card_id);
            }
            if row.is_counterspell {
                is_counterspell.insert(row.card_id);
            }
        }
        RemovalCounterspellTagsV1 { requires_target, is_counterspell }
    }

    #[test]
    fn run_episode_with_real_tags_records_removal_no_target_false_for_a_legally_targeted_cast_down() {
        // Item 6 (Important): `removal_no_target` and `counterspell_held`
        // were never exercised against the real, committed tag file with
        // real engine-emitted events -- the Critical bug (item 1) shipped
        // past both the full-episode test (empty tags) and the exact-value
        // test (a hand-built history with no preceding `Targeted` event, the
        // one case that does not expose the bug). `Wildfire`
        // (`data/runtime_decks_v1.json`) runs four copies of Cast Down
        // (card_id 11 in the tag file: `requires_target = true,
        // is_counterspell = false`) and plenty of creatures to target in a
        // mirror match, so a full random-policy game is very likely to cast
        // it with a legal target somewhere in the game. Deterministic:
        // searches a fixed, ordered seed range and asserts on the first game
        // that actually casts it, rather than a hand-picked magic seed.
        const CAST_DOWN_CARD_ID: u16 = 11;
        let tags = load_real_removal_counterspell_tags_v1();
        let deck_ids = ["Wildfire".to_owned(), "Wildfire".to_owned()];
        let mainboards = [
            runtime_deck_by_id("Wildfire").unwrap().card_ids.to_vec(),
            runtime_deck_by_id("Wildfire").unwrap().card_ids.to_vec(),
        ];

        for seed in 1u64..=150 {
            let mut session = RlEpisodeSessionV1::reset_with_explicit_decks_and_limits(
                seed,
                seed,
                2000,
                200_000,
                deck_ids.clone(),
                mainboards.clone(),
            )
            .unwrap();
            let mut rng = SplitMix64::seed(seed ^ 0xABCD_EF01_2345_6789);
            let mut policy = |decision: &RlSessionDecisionV1| {
                let index = (rng.next_u64() as usize) % decision.legal_actions.len();
                (index as u32, decision.legal_actions[index].stable_id.clone())
            };
            let summary = run_episode_with_summary_v1(&mut session, "fixround1castdown", &tags, &mut policy);
            for seat in 0..2 {
                if let Some(outcome) = summary.own_card_outcomes[seat].get(&CAST_DOWN_CARD_ID) {
                    if outcome.cast {
                        assert!(
                            !outcome.removal_no_target,
                            "seed {seed} seat {seat}: Cast Down was cast but recorded no traced target"
                        );
                        return;
                    }
                }
            }
        }
        panic!("no seed in 1..=150 cast Cast Down (card_id 11) in a Wildfire mirror; widen the search range or pick a different deck/card");
    }

    #[test]
    fn run_episode_with_real_tags_can_record_a_counterspell_held_to_game_end() {
        // Item 6 (Important, "if feasible within the existing helpers"): a
        // counterspell that is drawn, never offered as a legal cast (no
        // spell it could legally and affordably counter ever went on the
        // stack while it sat in hand), and stays in hand to the end of the
        // game. `Terror` (`data/runtime_decks_v1.json`) runs Counterspell
        // (card_id 17, `is_counterspell = true` in the tag file); seed 1 in
        // a Terror mirror (the first hit of a bounded, ordered seed search)
        // is a real game where this happens for P0's copy.
        const COUNTERSPELL_CARD_ID: u16 = 17;
        let tags = load_real_removal_counterspell_tags_v1();
        let deck_ids = ["Terror".to_owned(), "Terror".to_owned()];
        let mainboards = [
            runtime_deck_by_id("Terror").unwrap().card_ids.to_vec(),
            runtime_deck_by_id("Terror").unwrap().card_ids.to_vec(),
        ];
        let mut session = RlEpisodeSessionV1::reset_with_explicit_decks_and_limits(
            1, 1, 2000, 200_000, deck_ids, mainboards,
        )
        .unwrap();
        let mut rng = SplitMix64::seed(1 ^ 0x1234_5678_9abc_def0);
        let mut policy = |decision: &RlSessionDecisionV1| {
            let index = (rng.next_u64() as usize) % decision.legal_actions.len();
            (index as u32, decision.legal_actions[index].stable_id.clone())
        };
        let summary = run_episode_with_summary_v1(&mut session, "fixround1counterspell", &tags, &mut policy);
        let outcome = &summary.own_card_outcomes[0][&COUNTERSPELL_CARD_ID];
        assert!(outcome.counterspell_held, "seed 1 seat 0's Counterspell must read held to game end");
    }

    #[test]
    fn fold_event_history_gates_opponent_evidence_on_actual_owner_not_shared_card_id_membership() {
        use crate::ids::ObjectId;

        // Final review item 1 (Important): registered decks can share card
        // ids across archetypes (Burn and Rally both register card_id 66,
        // confirmed against `data/runtime_decks_v1.json`), so gating
        // opponent_evidence on card-id membership in the opponent's
        // registered list (rather than the object's actual `owner`) records
        // a phantom row for a seat's OWN copy of a shared card the moment
        // it becomes public. Two independent single-event histories: P0's
        // own copy of card_id 66 going public must not appear in P0's own
        // evidence (opponent_evidence[0]) at all, only in P1's (the true
        // opponent); symmetrically for P1's own copy.
        let object_card_def: BTreeMap<ObjectId, u16> = [(ObjectId(80), 66u16)].into_iter().collect();
        let own_registered_ids: [std::collections::BTreeSet<u16>; 2] =
            [[66u16].into_iter().collect(), [66u16].into_iter().collect()];
        let end_of_game_hand_card_ids: [std::collections::BTreeSet<u16>; 2] = Default::default();
        let offered_as_cast: [std::collections::BTreeSet<u16>; 2] = Default::default();
        let tags = RemovalCounterspellTagsV1 { requires_target: Default::default(), is_counterspell: Default::default() };
        let turn_watermarks = vec![(0usize, 1u32)];
        let final_turn = 1u32;

        // P0's own copy of the shared card goes public.
        let p0_owns_it = vec![CommittedEvent::ZoneChange {
            object: ObjectId(80),
            from: Zone::Library,
            to: Zone::Battlefield,
            controller_before: PlayerId::P0,
        }];
        let object_owner_p0: BTreeMap<ObjectId, PlayerId> = [(ObjectId(80), PlayerId::P0)].into_iter().collect();
        let mut resource_curve = ResourceCurveV1::default();
        let (opponent_evidence, _) = fold_event_history_v1(
            &p0_owns_it,
            final_turn,
            &turn_watermarks,
            &object_card_def,
            &object_owner_p0,
            &own_registered_ids,
            &end_of_game_hand_card_ids,
            &offered_as_cast,
            &tags,
            &mut resource_curve,
        );
        assert!(
            opponent_evidence[0].is_empty(),
            "P0's own copy of a shared card_id must never appear in P0's own opponent_evidence"
        );
        assert_eq!(opponent_evidence[1].len(), 1, "P1 (the true opponent) must learn about P0's public card");
        assert_eq!(opponent_evidence[1][0].card_id, 66);

        // Symmetric case: P1's own copy of the same shared card_id goes
        // public (a fresh, independent history and object_owner map).
        let p1_owns_it = vec![CommittedEvent::ZoneChange {
            object: ObjectId(80),
            from: Zone::Library,
            to: Zone::Battlefield,
            controller_before: PlayerId::P1,
        }];
        let object_owner_p1: BTreeMap<ObjectId, PlayerId> = [(ObjectId(80), PlayerId::P1)].into_iter().collect();
        let mut resource_curve = ResourceCurveV1::default();
        let (opponent_evidence, _) = fold_event_history_v1(
            &p1_owns_it,
            final_turn,
            &turn_watermarks,
            &object_card_def,
            &object_owner_p1,
            &own_registered_ids,
            &end_of_game_hand_card_ids,
            &offered_as_cast,
            &tags,
            &mut resource_curve,
        );
        assert!(
            opponent_evidence[1].is_empty(),
            "P1's own copy of a shared card_id must never appear in P1's own opponent_evidence"
        );
        assert_eq!(opponent_evidence[0].len(), 1, "P0 (the true opponent) must learn about P1's public card");
        assert_eq!(opponent_evidence[0][0].card_id, 66);
    }

    #[test]
    fn run_episode_with_real_engine_opponent_evidence_rows_are_always_backed_by_an_opponent_owned_public_reveal() {
        // Final review item 1 (Important), real-episode half: for every
        // opponent_evidence row a seat recorded, walk the real trace
        // (`event_history`, not the summary alone) and confirm at least one
        // `ZoneChange` shows an object with that card_id, owned by the
        // opponent seat, entering a public zone. Under the pre-fix
        // card-id-membership gate this could be satisfied by the
        // *observer's own* object instead, for any card_id both decks
        // register; the owner gate closes that.
        let mainboards = burn_and_rally();
        let deck_ids = ["Burn".to_owned(), "Rally".to_owned()];
        let mut session = RlEpisodeSessionV1::reset_with_explicit_decks_and_limits(
            5, 0xBBBB_BBBB_BBBB_BBBB, 2000, 200_000, deck_ids, mainboards,
        )
        .unwrap();
        let tags = RemovalCounterspellTagsV1 { requires_target: Default::default(), is_counterspell: Default::default() };
        let mut rng = SplitMix64::seed(0xCCCC_CCCC_CCCC_CCCC);
        let mut policy = |decision: &RlSessionDecisionV1| {
            let index = (rng.next_u64() as usize) % decision.legal_actions.len();
            (index as u32, decision.legal_actions[index].stable_id.clone())
        };
        let summary = run_episode_with_summary_v1(&mut session, "finalreviewitem1", &tags, &mut policy);

        let final_state = session.game_state();
        let object_card_def = build_object_card_def_map_v1(final_state);
        let object_owner = build_object_owner_map_v1(final_state);

        let mut public_reveals_by_owner_seat: [std::collections::BTreeSet<u16>; 2] = Default::default();
        for event in &final_state.engine.event_history {
            if let CommittedEvent::ZoneChange { object, to, .. } = event {
                let is_public = matches!(to, Zone::Battlefield | Zone::Graveyard | Zone::Stack | Zone::Exile);
                if !is_public {
                    continue;
                }
                let (Some(&card_id), Some(&owner)) = (object_card_def.get(object), object_owner.get(object)) else {
                    continue;
                };
                public_reveals_by_owner_seat[seat_index_from_player_id_v1(owner)].insert(card_id);
            }
        }

        let mut checked = 0usize;
        for seat in 0..2 {
            let opponent_seat = 1 - seat;
            for row in &summary.opponent_evidence[seat] {
                assert!(
                    public_reveals_by_owner_seat[opponent_seat].contains(&row.card_id),
                    "seat {seat}: opponent_evidence row for card_id {} has no matching opponent-owned (seat {opponent_seat}) public ZoneChange in the real trace",
                    row.card_id
                );
                checked += 1;
            }
        }
        assert!(checked > 0, "a full Burn vs Rally game must reveal at least one opponent card publicly");
    }
}
