//! A native, opt-in owner for actions taken from kept opening hands.
use crate::{
    card_def::{CardDef, CARD_DEFS},
    engine::{Action, Decision},
    event::{self, CommittedEvent, ProposedEvent},
    event_history_v1::EventKindV1,
    ids::{ObjectId, PlayerId},
    state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Step, Target, Zone},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpeningV1 {
    enabled_at: usize,
    phase: Phase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum Phase {
    London,
    Choosing { input_at: usize },
    Complete { input_at: usize, finished_at: usize },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputV1 {
    hands: [Vec<ObjectLinkV4>; 2],
    candidates: Vec<AbilitySourceContractV4>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RecordV1 {
    Enabled(PlayerId),
    Kept(Box<InputV1>),
    Entered {
        source: AbilitySourceContractV4,
        move_at: usize,
    },
    Revealed(AbilitySourceContractV4),
    Finished(PlayerId),
    FirstSpellProcessed {
        cast_at: usize,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OpeningAction {
    Enter,
    Reveal,
}

/// These printings must use the kept-hand owner in new declared games.
pub(crate) fn requires_native_opening(def: &CardDef) -> bool {
    def.is_executable() && matches!(def.name, "Leyline of the Void" | "Leyline of Sanctity")
}

fn opening_action(printing: u16) -> Option<OpeningAction> {
    let def = CARD_DEFS.get(usize::from(printing))?;
    if !def.is_executable() {
        return None;
    }
    match def.name {
        "Chancellor of the Annex" => Some(OpeningAction::Reveal),
        _ if requires_native_opening(def) || crate::leyline_axe_v1::definition(printing) => {
            Some(OpeningAction::Enter)
        }
        _ => None,
    }
}

fn initial_scene(state: &GameState) -> bool {
    state.turn == 1
        && state.step == Step::Untap
        && state.active_player == state.starting_player
        && state.priority_player == state.starting_player
        && state.command.is_empty()
        && state.stack.is_empty()
        && state.engine.pending_cast.is_none()
        && state.engine.pending_activation.is_none()
        && state.engine.pending_effect.is_none()
        && state.engine.pending_discard.is_none()
        && state.engine.pending_triggers.is_empty()
        && state.engine.halted.is_none()
        && state
            .players
            .iter()
            .all(|player| player.graveyard.is_empty() && !player.has_lost)
}

fn record_at(state: &GameState, at: usize) -> Option<&RecordV1> {
    match state.engine.event_history.get(at) {
        Some(CommittedEvent::EternalOpeningV1 { record }) => Some(record),
        _ => None,
    }
}

fn first_cast(state: &GameState, after: usize, player: PlayerId) -> Option<(usize, ObjectId)> {
    state
        .engine
        .event_history
        .scan_for_v1(&[EventKindV1::SpellCast])
        .iter()
        .enumerate()
        .find_map(|(at, event)| match event {
            CommittedEvent::SpellCast { spell, controller }
                if at >= after && *controller == player =>
            {
                Some((at, *spell))
            }
            _ => None,
        })
}

fn cast_player(state: &GameState, cast_at: usize) -> Option<PlayerId> {
    let history = &state.engine.event_history;
    let CommittedEvent::SpellCast { spell, controller } = history.get(cast_at)? else {
        return None;
    };
    let CommittedEvent::PublishedCastIdentityV1 { receipt } =
        history.get(cast_at.checked_sub(1)?)?
    else {
        return None;
    };
    (matches!(*controller, PlayerId::P0 | PlayerId::P1)
        && receipt.spell == *spell
        && receipt.controller == *controller
        && receipt.spell_cast_history_index == cast_at as u64
        && receipt.stack_item_id.0 != 0
        && receipt.origin_zone_change_count.checked_add(1) == Some(receipt.stack_zone_change_count))
    .then_some(*controller)
}

/// Only the actual native trigger collection consumes an armed opening
/// reveal. The ordinary SpellCast trigger marker is retained unchanged.
pub(crate) fn record_first_spell_processed(
    state: &mut GameState,
    player: PlayerId,
    spell: ObjectId,
) -> Result<(), String> {
    let Some(opening) = state.eternal_opening_v1 else {
        return Ok(());
    };
    let Phase::Complete { finished_at, .. } = opening.phase else {
        return Err("spell processing preceded the Eternal opening finish".into());
    };
    let (cast_at, actual) =
        first_cast(state, finished_at + 1, player).ok_or("first spell lost its native cast")?;
    if actual != spell || cast_player(state, cast_at) != Some(player) {
        return Err("first spell processing lost its exact published cast".into());
    }
    publish(state, RecordV1::FirstSpellProcessed { cast_at });
    Ok(())
}

fn processed_first_spells(
    state: &GameState,
    end: usize,
    has_reveal: bool,
) -> Result<[bool; 2], String> {
    let history = &state.engine.event_history;
    // The trigger queue is a suffix of the native cast occurrence sequence.
    // Use occurrence order, since one physical card can be cast twice.
    let mut casts = history
        .iter()
        .enumerate()
        .skip(end)
        .rev()
        .filter(|(_, event)| matches!(event, CommittedEvent::SpellCast { .. }));
    let mut first_queued = history.len();
    for pending in state.engine.event_log.iter().rev() {
        if matches!(pending, CommittedEvent::SpellCast { .. }) {
            let (at, _) = casts
                .next()
                .filter(|(_, event)| *event == pending)
                .ok_or("Eternal opening cast queue differs from native history")?;
            first_queued = at;
        }
    }
    let expected = [PlayerId::P0, PlayerId::P1].map(|player| {
        if !has_reveal {
            return None;
        }
        first_cast(state, end, player)
            .map(|(at, _)| at)
            .filter(|at| *at < first_queued)
    });
    let mut processed = [None; 2];
    let mut previous_cast = None;
    for (at, event) in history.iter().enumerate().skip(end) {
        let CommittedEvent::EternalOpeningV1 {
            record: RecordV1::FirstSpellProcessed { cast_at },
        } = event
        else {
            continue;
        };
        let caster =
            cast_player(state, *cast_at).ok_or("invalid Eternal first spell occurrence")?;
        if *cast_at >= at
            || previous_cast.is_some_and(|previous| *cast_at <= previous)
            || expected[caster.index()] != Some(*cast_at)
            || processed[caster.index()].replace(*cast_at).is_some()
        {
            return Err("invalid Eternal first spell processing receipt".into());
        }
        previous_cast = Some(*cast_at);
    }
    if processed != expected {
        return Err("missing actual Eternal first spell processing receipt".into());
    }
    Ok(processed.map(|at| at.is_some()))
}

fn initial_deal_precedes(state: &GameState, at: usize) -> bool {
    let mut draws = state
        .engine
        .event_history
        .iter()
        .take(at)
        .filter_map(|event| match event {
            CommittedEvent::Draw { player, object } => Some((*player, *object)),
            _ => None,
        });
    let expected = (0..7).flat_map(|_| [PlayerId::P0, PlayerId::P1]);
    for player in expected {
        if !matches!(draws.next(), Some((actual, Some(_))) if actual == player) {
            return false;
        }
    }
    draws.next().is_none()
}

fn publish(state: &mut GameState, record: RecordV1) -> usize {
    let at = state.engine.event_history.len();
    state
        .engine
        .event_history
        .push(CommittedEvent::EternalOpeningV1 { record });
    at
}

pub(crate) fn enabled(state: &GameState) -> bool {
    state.eternal_opening_v1.is_some()
}

pub(crate) fn requires_successor(state: &GameState) -> bool {
    enabled(state)
        || state
            .engine
            .event_history
            .scan_for_v1(&[EventKindV1::EternalOpeningV1])
            .iter()
            .any(|event| matches!(event, CommittedEvent::EternalOpeningV1 { .. }))
}

/// Enable the shared Eternal opening window immediately after the native
/// alternating deal. Owns the preceding London mulligans as part of setup.
pub fn enable_eternal_opening_v1(state: &mut GameState) -> Result<(), String> {
    if !cfg!(feature = "eternal-fixtures")
        || !initial_scene(state)
        || requires_successor(state)
        || state.london_mulligans_v1.is_some()
        || state.leyline_opening_v1.is_some()
        || !state.exile.is_empty()
        || state
            .players
            .iter()
            .any(|player| !player.battlefield.is_empty())
        || crate::leyline_axe_v1::verified_initial_deal(state)?.is_none()
        || !initial_deal_precedes(state, state.engine.event_history.len())
    {
        return Err("Eternal opening requires a fresh native Eternal deal".into());
    }
    crate::london_mulligan_v1::enable_london_mulligans_v1(state)?;
    let starting_player = state.starting_player;
    let enabled_at = publish(state, RecordV1::Enabled(starting_player));
    state.eternal_opening_v1 = Some(OpeningV1 {
        enabled_at,
        phase: Phase::London,
    });
    Ok(())
}

fn capture_candidates(
    state: &GameState,
    hands: &[Vec<ObjectLinkV4>; 2],
) -> Vec<AbilitySourceContractV4> {
    [state.starting_player, state.starting_player.opponent()]
        .into_iter()
        .flat_map(|player| &hands[player.index()])
        .filter_map(|link| {
            opening_action(state.objects.get(link.object).card_def)?;
            Some(AbilitySourceContractV4::capture(state, link.object))
        })
        .collect()
}

struct Window<'a> {
    input: &'a InputV1,
    current_player: Option<PlayerId>,
    used: Vec<AbilitySourceContractV4>,
}

impl Window<'_> {
    fn legal(&self, player: PlayerId) -> impl Iterator<Item = AbilitySourceContractV4> + '_ {
        self.input
            .candidates
            .iter()
            .copied()
            .filter(move |candidate| candidate.owner == player && !self.used.contains(candidate))
    }
}

fn validate_window(state: &GameState, opening: OpeningV1) -> Result<Window<'_>, String> {
    let (input_at, finished_at) = match opening.phase {
        Phase::Choosing { input_at } => (input_at, None),
        Phase::Complete {
            input_at,
            finished_at,
        } => (input_at, Some(finished_at)),
        Phase::London => return Err("Eternal opening still awaits London keeps".into()),
    };
    let Some(RecordV1::Kept(input)) = record_at(state, input_at) else {
        return Err("missing original Eternal kept hands".into());
    };
    if input_at <= opening.enabled_at
        || finished_at.is_some_and(|end| end <= input_at || end >= state.engine.event_history.len())
        || crate::leyline_axe_v1::history_kept_hands(state, input_at, true)? != input.hands
        || crate::london_mulligan_v1::retained_eternal_keeps_v1(state)? != &input.hands
    {
        return Err("invalid Eternal kept-hand checkpoint".into());
    }
    for (index, source) in input.candidates.iter().enumerate() {
        if opening_action(source.card_def).is_none()
            || source.zone != Zone::Hand
            || source.controller != source.owner
            || source.attached_to.is_some()
            || !matches!(source.owner, PlayerId::P0 | PlayerId::P1)
            || !input.hands[source.owner.index()].contains(&ObjectLinkV4 {
                object: source.source,
                zone_change_count: source.zone_change_count,
            })
            || input.candidates[..index]
                .iter()
                .any(|earlier| earlier.source == source.source)
        {
            return Err("invalid Eternal opening candidate binding".into());
        }
    }
    let mut current_player = Some(state.starting_player);
    let mut used = Vec::new();
    let mut entered = Vec::new();
    let mut revealed = Vec::new();
    let mut unclaimed_move = None;
    let end = finished_at.map_or(state.engine.event_history.len(), |at| at + 1);
    for (at, event) in state
        .engine
        .event_history
        .iter()
        .enumerate()
        .take(end)
        .skip(input_at + 1)
    {
        let (source, is_entry) = match event {
            CommittedEvent::EternalOpeningV1 {
                record: RecordV1::Finished(player),
            } => {
                if current_player != Some(*player)
                    || unclaimed_move.is_some()
                    || (*player != state.starting_player && finished_at != Some(at))
                {
                    return Err("invalid Eternal opening finish order".into());
                }
                current_player = (*player == state.starting_player).then(|| player.opponent());
                continue;
            }
            CommittedEvent::EternalOpeningV1 {
                record: RecordV1::Entered { source, move_at },
            } if unclaimed_move.take() == Some((*move_at, source.source, source.owner)) => {
                (*source, true)
            }
            CommittedEvent::EternalOpeningV1 {
                record: RecordV1::Revealed(source),
            } if unclaimed_move.is_none() => (*source, false),
            CommittedEvent::ZoneChange {
                object,
                from: Zone::Hand,
                to: Zone::Battlefield,
                controller_before,
            } if current_player.is_some() && unclaimed_move.is_none() => {
                unclaimed_move = Some((at, *object, *controller_before));
                continue;
            }
            CommittedEvent::EternalOpeningV1 { .. }
            | CommittedEvent::ZoneChange { .. }
            | CommittedEvent::SpellCast { .. }
            | CommittedEvent::Draw { .. }
            | CommittedEvent::UpkeepBegan { .. }
            | CommittedEvent::CreateToken { .. } => {
                return Err("invalid native occurrence in Eternal opening window".into());
            }
            _ => continue,
        };
        let expected = if is_entry {
            OpeningAction::Enter
        } else {
            OpeningAction::Reveal
        };
        if current_player != Some(source.owner)
            || opening_action(source.card_def) != Some(expected)
            || !input.candidates.contains(&source)
            || used.contains(&source)
        {
            return Err("invalid Eternal opening answer provenance".into());
        }
        used.push(source);
        if is_entry {
            entered.push(source);
        } else {
            revealed.push(source);
        }
    }
    if unclaimed_move.is_some() || current_player.is_none() != finished_at.is_some() {
        return Err("incomplete Eternal opening native window".into());
    }
    if current_player.is_some() {
        let mut original_cards = input.hands.clone();
        for player in [PlayerId::P0, PlayerId::P1] {
            let expected_hand = &mut original_cards[player.index()];
            expected_hand.retain(|link| !entered.iter().any(|source| source.source == link.object));
            let expected_battlefield = entered
                .iter()
                .filter(|source| source.owner == player)
                .map(|source| source.source);
            let live = &state.players[player.index()];
            if !initial_scene(state)
                || live.battlefield.iter().copied().ne(expected_battlefield)
                || live
                    .hand
                    .iter()
                    .copied()
                    .ne(expected_hand.iter().map(|link| link.object))
                || expected_hand.iter().any(|link| {
                    !state.objects.try_get(link.object).is_some_and(|card| {
                        card.owner == player
                            && card.controller == player
                            && card.zone == Zone::Hand
                            && card.zone_change_count == link.zone_change_count
                    })
                })
            {
                return Err("Eternal opening no longer owns its original kept scene".into());
            }
        }
        if entered.iter().any(|source| {
            !state.objects.try_get(source.source).is_some_and(|card| {
                card.zone == Zone::Battlefield
                    && card.owner == source.owner
                    && card.controller == source.controller
                    && card.card_def == source.card_def
                    && source.zone_change_count.checked_add(1) == Some(card.zone_change_count)
            })
        }) {
            return Err("Eternal opening entry lost its exact incarnation".into());
        }
        let live_candidates = capture_candidates(state, &original_cards);
        if input
            .candidates
            .iter()
            .copied()
            .filter(|source| !entered.contains(source))
            .ne(live_candidates)
        {
            return Err("Eternal opening candidate roster changed".into());
        }
    }
    let processed = if finished_at.is_some() {
        processed_first_spells(state, end, !revealed.is_empty())?
    } else {
        [false; 2]
    };
    let vintage = state.eternal_v1.as_ref().map(|eternal| &eternal.vintage);
    if vintage.is_some_and(|vintage| {
        vintage.opening_hand_offered == revealed.is_empty() || vintage.cast_a_spell != processed
    }) {
        return Err("invalid Eternal opening first-spell state".into());
    }
    let still_armed = revealed
        .iter()
        .copied()
        .filter(|source| !processed[source.owner.opponent().index()]);
    if vintage
        .into_iter()
        .flat_map(|vintage| vintage.chancellor_reveals.iter().copied())
        .ne(still_armed)
    {
        return Err("Eternal opening reveal differs from its native delayed ability".into());
    }
    Ok(Window {
        input,
        current_player,
        used,
    })
}

pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    let Some(opening) = state.eternal_opening_v1 else {
        return if requires_successor(state) {
            Err("missing Eternal opening owner".into())
        } else {
            Ok(())
        };
    };
    if !cfg!(feature = "eternal-fixtures")
        || state.leyline_opening_v1.is_some()
        || !matches!(record_at(state, opening.enabled_at), Some(RecordV1::Enabled(p)) if *p == state.starting_player)
        || !initial_deal_precedes(state, opening.enabled_at)
        || !matches!(state.starting_player, PlayerId::P0 | PlayerId::P1)
        || state.london_mulligans_v1.is_none()
    {
        return Err("invalid Eternal opening owner".into());
    }
    if opening.phase == Phase::London {
        if !initial_scene(state)
            || state
                .engine
                .event_history
                .iter()
                .skip(opening.enabled_at + 1)
                .any(|event| matches!(event, CommittedEvent::EternalOpeningV1 { .. }))
        {
            return Err("invalid Eternal opening London boundary".into());
        }
    } else {
        if crate::london_mulligan_v1::has_pending(state) {
            return Err("Eternal opening actions precede the London keeps".into());
        }
        validate_window(state, opening)?;
    }
    Ok(())
}

pub(crate) fn drain_or_decide(state: &mut GameState) -> Result<Option<Decision>, String> {
    let Some(opening) = state.eternal_opening_v1 else {
        return Ok(None);
    };
    if matches!(opening.phase, Phase::Complete { .. }) {
        return Ok(None);
    }
    if opening.phase == Phase::London {
        let hands = crate::london_mulligan_v1::kept_hands_for_opening_v1(state)?
            .ok_or("Eternal opening requires the native London keeps")?;
        crate::london_mulligan_v1::retained_eternal_keeps_v1(state)?;
        let candidates = capture_candidates(state, &hands);
        let input_at = publish(
            state,
            RecordV1::Kept(Box::new(InputV1 { hands, candidates })),
        );
        state.eternal_opening_v1 = Some(OpeningV1 {
            phase: Phase::Choosing { input_at },
            ..opening
        });
    }
    let window = validate_window(state, state.eternal_opening_v1.unwrap())?;
    let player = window
        .current_player
        .ok_or("missing Eternal opening actor")?;
    Ok(Some(Decision::ChooseEternalOpeningV1 {
        player,
        legal_cards: window.legal(player).map(|source| source.source).collect(),
    }))
}

pub(crate) fn owns_action(state: &GameState) -> bool {
    state
        .eternal_opening_v1
        .is_some_and(|opening| match opening.phase {
            Phase::London => !crate::london_mulligan_v1::has_pending(state),
            Phase::Choosing { .. } => true,
            Phase::Complete { .. } => false,
        })
}

pub(crate) fn answer(state: &mut GameState, action: Action) -> Result<(), String> {
    let opening = state
        .eternal_opening_v1
        .ok_or("missing Eternal opening owner")?;
    let Phase::Choosing { input_at } = opening.phase else {
        return Err("advance to the original Eternal opening menu first".into());
    };
    let window = validate_window(state, opening)?;
    let player = window
        .current_player
        .ok_or("missing Eternal opening actor")?;
    let mut scene = state.clone();
    let record = match action {
        Action::FinishEffectSelection => RecordV1::Finished(player),
        Action::ChooseEffectTarget(Target::Object(object)) => {
            let source = window
                .legal(player)
                .find(|source| source.source == object)
                .ok_or("choice is outside the acting player's original opening roster")?;
            match opening_action(source.card_def) {
                Some(OpeningAction::Enter) => {
                    let before = scene.engine.event_history.len();
                    event::propose_and_commit(
                        &mut scene,
                        ProposedEvent::zone_change(object, Zone::Battlefield),
                    );
                    if scene.engine.halted.is_some() {
                        return Err("native Eternal opening entry halted".into());
                    }
                    let move_at = scene
                        .engine
                        .event_history
                        .iter()
                        .enumerate()
                        .skip(before)
                        .find_map(|(at, event)| {
                            matches!(event, CommittedEvent::ZoneChange {
                                object: moved, from: Zone::Hand, to: Zone::Battlefield, ..
                            } if *moved == object)
                            .then_some(at)
                        })
                        .ok_or("actual opening entry absent")?;
                    RecordV1::Entered { source, move_at }
                }
                Some(OpeningAction::Reveal) => {
                    crate::eternal_vintage_v1::reveal_chancellor_from_opening(&mut scene, source)?;
                    RecordV1::Revealed(source)
                }
                None => return Err("invalid Eternal opening definition".into()),
            }
        }
        _ => return Err("only the original Eternal opening actions are legal".into()),
    };
    let finish = matches!(record, RecordV1::Finished(p) if p != state.starting_player);
    let at = publish(&mut scene, record);
    if finish {
        scene.eternal_opening_v1 = Some(OpeningV1 {
            phase: Phase::Complete {
                input_at,
                finished_at: at,
            },
            ..opening
        });
    }
    validate(&scene)?;
    *state = scene;
    Ok(())
}
