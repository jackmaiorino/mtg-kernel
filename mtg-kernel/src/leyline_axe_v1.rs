//! Native opening-hand action for Leyline Axe. No consumer admission.
//! Its equipment bonuses/equip payment reuse the ordinary native attachment path.
use crate::{
    card_def::{self, CardType, Keywords},
    effect::{EffectOp, ObjectRef},
    engine::{Action, Decision},
    event::{self, CommittedEvent, ProposedEvent},
    ids::{ObjectId, PlayerId},
    state::{GameState, ObjectLinkV4, Step, Zone},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpeningV1 {
    input_history_index: u64,
    next: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputV1 {
    starting_player: PlayerId,
    hands: [Vec<ObjectLinkV4>; 2],
    libraries: [Vec<ObjectLinkV4>; 2],
    candidates: Vec<ObjectLinkV4>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RecordV1 {
    Input(Box<InputV1>),
    Answer {
        input_history_index: u64,
        index: u32,
        accept: bool,
        entry_history_index: Option<u64>,
    },
    Finished {
        input_history_index: u64,
    },
}
fn links(state: &GameState, ids: &[ObjectId]) -> Result<Vec<ObjectLinkV4>, String> {
    ids.iter()
        .map(|id| {
            let object = state.objects.try_get(*id).ok_or("opening card absent")?;
            Ok(ObjectLinkV4 {
                object: *id,
                zone_change_count: object.zone_change_count,
            })
        })
        .collect()
}
fn input(state: &GameState, index: u64) -> Result<&InputV1, String> {
    match state
        .engine
        .event_history
        .get(usize::try_from(index).map_err(|_| "opening input index width")?)
    {
        Some(CommittedEvent::LeylineOpeningV1 {
            record: RecordV1::Input(input),
        }) => Ok(input),
        _ => Err("opening-hand action lost its original kept-hand input".into()),
    }
}
fn opening_scene(state: &GameState) -> bool {
    state.turn == 1
        && state.step == Step::Untap
        && state.active_player == state.starting_player
        && state.priority_player == state.starting_player
        && state.stack.is_empty()
        && state.exile.is_empty()
        && state.command.is_empty()
        && state.engine.pending_cast.is_none()
        && state.engine.pending_activation.is_none()
        && state.engine.pending_effect.is_none()
        && state.engine.pending_discard.is_none()
        && state.engine.pending_triggers.is_empty()
        && state
            .players
            .iter()
            .all(|p| p.graveyard.is_empty() && !p.has_lost)
}
fn kept_hands(state: &GameState) -> Result<Option<[Vec<ObjectLinkV4>; 2]>, String> {
    if !opening_scene(state)
        || crate::london_mulligan_v1::has_pending(state)
        || state.players.iter().any(|p| !p.battlefield.is_empty())
    {
        return Ok(None);
    }
    if let Some(hands) = crate::london_mulligan_v1::kept_hands_for_opening_v1(state)? {
        return Ok(Some(hands));
    }
    if state.players.iter().any(|p| p.hand.len() != 7) {
        return Ok(None);
    }
    // A manually installed hand is not an opening deal. All fourteen actual
    // successful native draws must be present, in the historical deal order.
    let draws = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            CommittedEvent::Draw {
                player,
                object: Some(object),
            } => Some((*player, *object)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if draws.len() != 14
        || draws.iter().enumerate().any(|(i, (player, id))| {
            *player
                != if i % 2 == 0 {
                    PlayerId::P0
                } else {
                    PlayerId::P1
                }
                || state.players[player.index()].hand.get(i / 2) != Some(id)
                || state.objects.try_get(*id).is_none_or(|o| {
                    o.zone != Zone::Hand || o.owner != *player || o.zone_change_count != 1
                })
        })
    {
        return Ok(None);
    }
    Ok(Some([
        links(state, &state.players[0].hand)?,
        links(state, &state.players[1].hand)?,
    ]))
}
fn candidates(
    state: &GameState,
    hands: &[Vec<ObjectLinkV4>; 2],
) -> Result<Vec<ObjectLinkV4>, String> {
    let mut result = Vec::new();
    for player in [state.starting_player, state.starting_player.opponent()] {
        for link in &hands[player.index()] {
            let object = state
                .objects
                .try_get(link.object)
                .ok_or("opening candidate absent")?;
            if definition(object.card_def) {
                result.push(*link);
            }
        }
    }
    Ok(result)
}
fn already_started(state: &GameState) -> bool {
    state.engine.event_history.iter().any(|e| {
        matches!(
            e,
            CommittedEvent::LeylineOpeningV1 {
                record: RecordV1::Input(_)
            }
        )
    })
}
pub(crate) fn requires_initial_advance(state: &GameState) -> Result<bool, String> {
    if already_started(state) || state.leyline_opening_v1.is_some() {
        return Ok(false);
    }
    match kept_hands(state)? {
        Some(hands) => Ok(!candidates(state, &hands)?.is_empty()),
        None => Ok(false),
    }
}
// This is only the original native opening draw/bottom trace, not game replay.
// It authenticates kept-card order and generations independently of the new input.
fn validate_kept_input(state: &GameState, at: usize, original: &InputV1) -> Result<(), String> {
    let mut hands: [Vec<ObjectLinkV4>; 2] = std::array::from_fn(|_| Vec::new());
    let mut generations = std::collections::BTreeMap::<ObjectId, u32>::new();
    for event in state.engine.event_history.iter().take(at) {
        match event {
            CommittedEvent::Draw {
                player,
                object: Some(object),
            } => {
                let live = state
                    .objects
                    .try_get(*object)
                    .ok_or("opening draw card absent")?;
                if live.owner != *player
                    || hands.iter().flatten().any(|link| link.object == *object)
                {
                    return Err("opening native draw changed physical owner/order".into());
                }
                let generation = generations.entry(*object).or_insert(0);
                *generation = generation
                    .checked_add(1)
                    .ok_or("opening generation overflow")?;
                hands[player.index()].push(ObjectLinkV4 {
                    object: *object,
                    zone_change_count: *generation,
                });
            }
            CommittedEvent::ZoneChange {
                object,
                from: Zone::Hand,
                to: Zone::Library,
                ..
            } => {
                let owner = state
                    .objects
                    .try_get(*object)
                    .ok_or("opening bottom card absent")?
                    .owner;
                let roster = &mut hands[owner.index()];
                let index = roster
                    .iter()
                    .position(|link| link.object == *object)
                    .ok_or("opening bottom has no original draw")?;
                roster.remove(index);
                let generation = generations.entry(*object).or_insert(0);
                *generation = generation
                    .checked_add(1)
                    .ok_or("opening generation overflow")?;
            }
            CommittedEvent::ZoneChange { .. }
            | CommittedEvent::SpellCast { .. }
            | CommittedEvent::CreateToken { .. } => {
                return Err("ordinary gameplay preceded the opening-hand action".into())
            }
            _ => {}
        }
    }
    if hands != original.hands {
        return Err("opening input differs from actual native draw/bottom order".into());
    }
    Ok(())
}

/// Read-only structural validation under the existing mutable native-world contract.
/// The current owner retains exact original input; this is not a public authority DTO.
pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    let inputs = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, e)| match e {
            CommittedEvent::LeylineOpeningV1 {
                record: RecordV1::Input(_),
            } => Some(at),
            _ => None,
        })
        .collect::<Vec<_>>();
    if inputs.is_empty() {
        if state.leyline_opening_v1.is_some()
            || state
                .engine
                .event_history
                .iter()
                .any(|e| matches!(e, CommittedEvent::LeylineOpeningV1 { .. }))
        {
            return Err("opening-hand action has no original input".into());
        }
        return Ok(());
    }
    if inputs.len() != 1 {
        return Err("opening-hand action input duplicated".into());
    }
    let at = inputs[0];
    let original = input(state, at as u64)?;
    validate_kept_input(state, at, original)?;
    if original.starting_player != state.starting_player
        || original.candidates.is_empty()
        || original.candidates != candidates(state, &original.hands)?
        || original.hands.iter().any(|h| h.len() > 7)
    {
        return Err("opening-hand original roster changed".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for (seat, hands) in original.hands.iter().enumerate() {
        for binding in hands {
            let live = state
                .objects
                .try_get(binding.object)
                .ok_or("opening source absent")?;
            if !seen.insert(binding.object) || live.owner.index() != seat {
                return Err("opening original physical hand changed".into());
            }
        }
    }
    let mut accepted = Vec::new();
    let mut next = 0usize;
    let mut finished = false;
    let mut last = at;
    for (index, event) in state.engine.event_history.iter().enumerate().skip(at + 1) {
        match event {
            CommittedEvent::LeylineOpeningV1 {
                record:
                    RecordV1::Answer {
                        input_history_index,
                        index: ordinal,
                        accept,
                        entry_history_index,
                    },
            } => {
                if finished
                    || *input_history_index != at as u64
                    || *ordinal as usize != next
                    || next >= original.candidates.len()
                {
                    return Err("opening answer reordered or foreign".into());
                }
                let source = original.candidates[next];
                if *accept {
                    let entry = usize::try_from(
                        entry_history_index.ok_or("accepted opening card lost entry")?,
                    )
                    .map_err(|_| "opening entry index width")?;
                    if entry <= last
                        || entry >= index
                        || !matches!(state.engine.event_history.get(entry),Some(CommittedEvent::ZoneChange{object,from:Zone::Hand,to:Zone::Battlefield,controller_before}) if *object==source.object && *controller_before==state.objects.get(source.object).owner)
                    {
                        return Err("opening answer lost its actual native entry".into());
                    }
                    accepted.push(source);
                } else if entry_history_index.is_some() {
                    return Err("declined opening card invented an entry".into());
                }
                next += 1;
                last = index;
            }
            CommittedEvent::LeylineOpeningV1 {
                record:
                    RecordV1::Finished {
                        input_history_index,
                    },
            } => {
                if finished
                    || *input_history_index != at as u64
                    || next != original.candidates.len()
                {
                    return Err("opening completion erased pending answers".into());
                }
                finished = true;
            }
            CommittedEvent::LeylineOpeningV1 {
                record: RecordV1::Input(_),
            } => return Err("opening input replayed".into()),
            _ => {}
        }
    }
    if state
        .engine
        .event_log
        .iter()
        .any(|e| matches!(e, CommittedEvent::LeylineOpeningV1 { .. }))
    {
        return Err("private opening input exposed in public events".into());
    }
    if finished {
        if state.leyline_opening_v1.is_some() {
            return Err("finished opening replayed".into());
        }
        return Ok(());
    }
    let pending = state
        .leyline_opening_v1
        .ok_or("pending opening checkpoint erased")?;
    if pending.input_history_index != at as u64
        || pending.next as usize != next
        || !opening_scene(state)
        || crate::london_mulligan_v1::has_pending(state)
    {
        return Err("opening checkpoint changed its original phase or position".into());
    }
    for p in [PlayerId::P0, PlayerId::P1] {
        let expected = original.hands[p.index()]
            .iter()
            .filter(|source| !accepted.contains(source))
            .copied()
            .collect::<Vec<_>>();
        if links(state, &state.players[p.index()].hand)? != expected
            || links(state, &state.players[p.index()].library)? != original.libraries[p.index()]
            || state.players[p.index()].battlefield.len()
                != accepted
                    .iter()
                    .filter(|source| state.objects.get(source.object).owner == p)
                    .count()
        {
            return Err("opening checkpoint lost exact kept hand/library/world roster".into());
        }
    }
    for source in accepted {
        let live = state.objects.get(source.object);
        if live.zone != Zone::Battlefield
            || live.controller != live.owner
            || source.zone_change_count.checked_add(1) != Some(live.zone_change_count)
            || !state.players[live.owner.index()]
                .battlefield
                .contains(&source.object)
        {
            return Err("opening accepted card changed incarnation/controller".into());
        }
    }
    Ok(())
}
pub(crate) fn drain_or_decide(state: &mut GameState) -> Result<Option<Decision>, String> {
    validate(state)?;
    if state.leyline_opening_v1.is_none() && !already_started(state) {
        if let Some(hands) = kept_hands(state)? {
            let candidates = candidates(state, &hands)?;
            if !candidates.is_empty() {
                let opening = OpeningV1 {
                    input_history_index: state.engine.event_history.len() as u64,
                    next: 0,
                };
                let input = InputV1 {
                    starting_player: state.starting_player,
                    hands,
                    libraries: [
                        links(state, &state.players[0].library)?,
                        links(state, &state.players[1].library)?,
                    ],
                    candidates,
                };
                state
                    .engine
                    .event_history
                    .push(CommittedEvent::LeylineOpeningV1 {
                        record: RecordV1::Input(Box::new(input)),
                    });
                state.leyline_opening_v1 = Some(opening);
            }
        }
    }
    let Some(opening) = state.leyline_opening_v1 else {
        return Ok(None);
    };
    let original = input(state, opening.input_history_index)?;
    if opening.next as usize == original.candidates.len() {
        state
            .engine
            .event_history
            .push(CommittedEvent::LeylineOpeningV1 {
                record: RecordV1::Finished {
                    input_history_index: opening.input_history_index,
                },
            });
        state.leyline_opening_v1 = None;
        validate(state)?;
        return Ok(None);
    }
    let source = original
        .candidates
        .get(opening.next as usize)
        .ok_or("opening position outside original roster")?
        .object;
    Ok(Some(Decision::ChooseEffectOption {
        player: state.objects.get(source).owner,
        source,
        option_count: 2,
    }))
}
pub(crate) fn answer(state: &mut GameState, action: Action) -> Result<(), String> {
    validate(state)?;
    let pending = state
        .leyline_opening_v1
        .ok_or("no native opening-hand checkpoint")?;
    let Action::ChooseEffectOption(option) = action else {
        return Err("only the original opening-hand answer is legal before turn one".into());
    };
    if option > 1 {
        return Err("opening option outside original decline/accept pair".into());
    }
    let source = input(state, pending.input_history_index)?
        .candidates
        .get(pending.next as usize)
        .ok_or("opening already complete")?
        .object;
    // Atomic native entry/checkpoint update. A failed entry/refusal cannot
    // consume the current choice or run an ordinary cast/priority operation.
    let mut scene = state.clone();
    let before = scene.engine.event_history.len();
    let entry = if option == 1 {
        event::propose_and_commit(
            &mut scene,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        if scene.engine.halted.is_some() {
            return Err("native opening entry halted".into());
        }
        let mut rows=scene.engine.event_history.iter().enumerate().skip(before).filter_map(|(i,e)|matches!(e,CommittedEvent::ZoneChange{object,from:Zone::Hand,to:Zone::Battlefield,..}if *object==source).then_some(i as u64));
        let row = rows.next().ok_or("actual opening move absent")?;
        if rows.next().is_some() {
            return Err("opening move duplicated".into());
        }
        Some(row)
    } else {
        None
    };
    scene
        .engine
        .event_history
        .push(CommittedEvent::LeylineOpeningV1 {
            record: RecordV1::Answer {
                input_history_index: pending.input_history_index,
                index: pending.next,
                accept: option == 1,
                entry_history_index: entry,
            },
        });
    scene.leyline_opening_v1 = Some(OpeningV1 {
        next: pending
            .next
            .checked_add(1)
            .ok_or("opening ordinal overflow")?,
        ..pending
    });
    validate(&scene)?;
    *state = scene;
    Ok(())
}
pub(crate) fn require_legacy_absence(state: &GameState) -> Result<(), String> {
    validate(state)?;
    if state.leyline_opening_v1.is_some()
        || crate::standard_cards_v1::has_battlefield_copy_origin_v1(state, definition)
        || state.objects.iter().any(|(_, o)| {
            matches!(
                o.zone,
                Zone::Battlefield | Zone::Stack | Zone::Graveyard | Zone::Exile
            ) && o.v4.face_down_v1.is_none()
                && definition(o.card_def)
        })
        || state.engine.event_history.iter().any(|e| {
            matches!(
                e,
                CommittedEvent::LeylineOpeningV1 {
                    record: RecordV1::Answer { accept: true, .. }
                }
            )
        })
    {
        return Err(
            "native Leyline opening/equipment requires a distinct complete consumer".into(),
        );
    }
    Ok(())
}

pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && card_def::CARD_DEFS.get(usize::from(id)).is_some_and(|d| {
        let card_def::CardDef {
            name: _,
            capability: _,
            cost: _,
            generic_cost_reduction: _,
            ward_cost: _,
            spell_cannot_be_countered: _,
            records_mana_spent: _,
            equipment: _,
            types: _,
            subtypes: _,
            supertypes: _,
            power: _,
            toughness: _,
            starting_loyalty: _,
            enters_with_plus_one_counters: _,
            controlled_counter_keyword: _,
            game_outcome_protection_v1: _,
            controlled_creature_grant: _,
            is_land: _,
            produces_mana: _,
            colors: _,
            target_spec: _,
            keywords: _,
            spell_effect: _,
            mana_ability: _,
            alt_cost: _,
            kicker_cost: _,
            additional_cost: _,
            flashback: _,
            activated_abilities: _,
            plot_cost: _,
            madness_cost: _,
            mode2: _,
            mode3: _,
            additional_modes: _,
            kicked_target_spec: _,
            is_token: _,
            escape: _,
            mana_ability_choices: _,
            enters_battlefield_tapped: _,
            mana_ability_def: _,
            minimum_blockers: _,
            omen: _,
            mana_value: _,
            mana_ability_includes_chosen_color: _,
            as_enters_choose_color_other_than: _,
            additional_mana_abilities: _,
            object_name: _,
            enters_battlefield_tapped_unless: _,
            attachment: _,
            transform_face: _,
            saga: _,
            optional_additional_cost: _,
            changeling: _,
            bestow: _,
            delve: _,
            adventure: _,
            cant_be_blocked_by_monarchs_creatures: _,
            conditional_tap_yield: _,
            enters_tapped_unless_controller: _,
            additional_mana_ability_conditions: _,
            restricted_mana_abilities: _,
            animation: _,
            activated_ability_generic_reductions: _,
            combat_static_rules: _,
            as_enters_color_choice_v1: _,
            chosen_color_creature_boost_v1: _,
            damage_replacement_v1: _,
            token_counter_replacement_v48: _,
            starting_life_self_bonuses_v1: _,
            source_type_hexproof_v1: _,
            other_controlled_nontoken_death_v1: _,
            creature_type_entry_v1: _,
        } = d;
        d.name == "Leyline Axe"
            && d.capability == card_def::CardCapability::Partial
            && d.cost
                == crate::mana::Cost {
                    pips: &[],
                    generic: 4,
                    x_count: 0,
                }
            && d.generic_cost_reduction.is_none()
            && d.ward_cost.is_none()
            && !d.spell_cannot_be_countered
            && !d.records_mana_spent
            && d.equipment.is_some_and(|e| {
                e.power_delta == 1
                    && e.toughness_delta == 1
                    && !e.pt_controller_turn_only
                    && e.add_subtype.is_none()
                    && e.controller_turn_keywords
                        == Keywords(Keywords::DOUBLE_STRIKE.0 | Keywords::TRAMPLE.0)
                    && e.other_turn_keywords == e.controller_turn_keywords
                    && e.noncreature_spell_damage_to_each_opponent == 0
                    && !e.job_select
                    && e.granted_activated_ability.is_none()
            })
            && d.types == [CardType::Artifact]
            && d.subtypes == [card_def::Subtype::Equipment]
            && d.supertypes.is_empty()
            && d.power.is_none()
            && d.toughness.is_none()
            && d.starting_loyalty.is_none()
            && d.enters_with_plus_one_counters.is_none()
            && d.controlled_counter_keyword.is_none()
            && d.game_outcome_protection_v1.is_none()
            && d.controlled_creature_grant.is_none()
            && !d.is_land
            && d.produces_mana.is_empty()
            && d.colors.is_empty()
            && d.target_spec == card_def::TargetSpec::None
            && d.keywords == Keywords::NONE
            && (d.spell_effect)()
                == Some(EffectOp::MoveObject {
                    object: ObjectRef::ThisSource,
                    to_zone: Zone::Battlefield,
                })
            && (d.mana_ability)().is_none()
            && d.alt_cost.is_none()
            && d.kicker_cost.is_none()
            && d.additional_cost.is_none()
            && d.flashback.is_none()
            && d.activated_abilities.len() == 1
            && d.activated_abilities[0].cost
                == [card_def::CostComponent::Mana(crate::mana::Cost {
                    pips: &[],
                    generic: 3,
                    x_count: 0,
                })]
            && d.activated_abilities[0].activation_zone == Zone::Battlefield
            && d.activated_abilities[0].sorcery_speed_only
            && d.activated_abilities[0].activation_target_filter
                == card_def::ActivationTargetFilter::TargetSpecOnly
            && d.activated_abilities[0].max_activations_per_turn.is_none()
            && d.activated_abilities[0].face.is_none()
            && d.activated_abilities[0].target_spec == card_def::TargetSpec::ControlledCreature
            && (d.activated_abilities[0].effect)()
                == EffectOp::AttachSourceToTarget {
                    object: ObjectRef::Target(0),
                }
            && d.plot_cost.is_none()
            && d.madness_cost.is_none()
            && d.mode2.is_none()
            && d.mode3.is_none()
            && d.additional_modes.is_empty()
            && d.kicked_target_spec.is_none()
            && !d.is_token
            && d.escape.is_none()
            && d.mana_ability_choices.is_empty()
            && !d.enters_battlefield_tapped
            && d.mana_ability_def.is_none()
            && d.minimum_blockers == 1
            && d.omen.is_none()
            && d.mana_value == 4
            && !d.mana_ability_includes_chosen_color
            && d.as_enters_choose_color_other_than.is_none()
            && d.additional_mana_abilities.is_empty()
            && d.object_name == d.name
            && d.enters_battlefield_tapped_unless.is_none()
            && d.attachment.is_none()
            && d.transform_face.is_none()
            && d.saga.is_none()
            && d.optional_additional_cost.is_none()
            && !d.changeling
            && d.bestow.is_none()
            && !d.delve
            && d.adventure.is_none()
            && !d.cant_be_blocked_by_monarchs_creatures
            && d.conditional_tap_yield.is_none()
            && d.enters_tapped_unless_controller.is_none()
            && d.additional_mana_ability_conditions.is_empty()
            && d.restricted_mana_abilities.is_empty()
            && d.animation.is_none()
            && d.activated_ability_generic_reductions.is_empty()
            && d.combat_static_rules == card_def::CombatStaticRulesDef::NONE
            && d.as_enters_color_choice_v1.is_none()
            && d.chosen_color_creature_boost_v1.is_none()
            && d.damage_replacement_v1.is_none()
            && d.token_counter_replacement_v48.is_none()
            && d.starting_life_self_bonuses_v1.is_empty()
            && d.source_type_hexproof_v1.is_none()
            && d.other_controlled_nontoken_death_v1.is_none()
            && d.creature_type_entry_v1.is_none()
    })
}
