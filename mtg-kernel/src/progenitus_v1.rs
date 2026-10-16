//! Progenitus native protection and mandatory reveal/shuffle replacement.
//! No registration, consumer admission, or serialized execution permission.
use crate::{
    card_def::{self, CardType, Keywords},
    effect::{EffectOp, ObjectRef},
    event::{self, CommittedEvent, ProposedEvent},
    ids::ObjectId,
    state::{AbilitySourceContractV4, GameState, LibraryShuffleToken, Zone},
};
use serde::{Deserialize, Serialize};

pub(crate) fn attachment_forbidden(state: &GameState, source: ObjectId, target: ObjectId) -> bool {
    let (Some(s), Some(t)) = (state.objects.try_get(source), state.objects.try_get(target)) else {
        return false;
    };
    let d = &card_def::CARD_DEFS[s.card_def as usize];
    t.zone == Zone::Battlefield
        && (d.attachment.is_some()
            || d.equipment.is_some()
            || d.bestow.is_some()
            || crate::standard_cards_v1::is_aura(d))
        && crate::engine::has_effective_keyword(state, target, Keywords::PROTECTION_FROM_EVERYTHING)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputV1 {
    pub source: AbilitySourceContractV4,
    pub original: event::ZoneChangeProposed,
}

pub(crate) struct PlanV1 {
    input: InputV1,
    shuffle: LibraryShuffleToken,
}
fn active(state: &GameState, object: ObjectId, removed: Option<bool>) -> bool {
    let Some(o) = state.objects.try_get(object) else {
        return false;
    };
    definition(o.card_def)
        && o.v4.face_down_v1.is_none()
        && o.v4.face_index == 0
        && (o.zone != Zone::Battlefield
            || !removed.unwrap_or_else(|| {
                !crate::continuous_characteristics_v1::printed_abilities_active(state, object)
            }))
}
/// Captures only a genuine would-graveyard proposal and the original source.
/// It never recognizes a caller-supplied replacement flag or library move.
pub(crate) fn plan(
    state: &GameState,
    event: &ProposedEvent,
    removed: Option<bool>,
) -> Result<Option<PlanV1>, ObjectId> {
    let ProposedEvent::ZoneChange(change) = event else {
        return Ok(None);
    };
    if change.to_zone != Zone::Graveyard || !active(state, change.object, removed) {
        return Ok(None);
    }
    let o = state.objects.get(change.object);
    // Existing Finality/Fiery rewriting has no general ordering-choice owner.
    // These primitive combinations explicitly refuse instead of picking an order.
    if o.zone == Zone::Battlefield
        && (o
            .v4
            .creature_upgrade
            .as_ref()
            .is_some_and(|u| u.finality > 0)
            || state.engine.active_replacements.iter().any(|r| {
                matches!(
                    r.kind,
                    event::ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 { .. }
                ) && event::replacement_applies(r, event, state)
            }))
    {
        return Err(change.object);
    }
    if o.zone != Zone::Library {
        o.zone_change_count.checked_add(1).ok_or(change.object)?;
    }
    let shuffle = state
        .preflight_library_shuffle(o.owner)
        .map_err(|_| change.object)?;
    Ok(Some(PlanV1 {
        input: InputV1 {
            source: AbilitySourceContractV4::capture(state, change.object),
            original: change.clone(),
        },
        shuffle,
    }))
}
impl PlanV1 {
    pub(crate) fn same_library(&self) -> bool {
        self.input.source.zone == Zone::Library
    }
    pub(crate) fn rewritten(&self) -> ProposedEvent {
        let mut change = self.input.original.clone();
        change.to_zone = Zone::Library;
        change.library_insert_visibility = event::LibraryInsertVisibility::Public;
        ProposedEvent::ZoneChange(change)
    }
    pub(crate) fn record_input(&self, state: &mut GameState) -> u64 {
        let at = state.engine.event_history.len() as u64;
        state
            .engine
            .event_history
            .push(CommittedEvent::ProgenitusInputV1 {
                input: Box::new(self.input.clone()),
            });
        at
    }
    pub(crate) fn finish(
        self,
        state: &mut GameState,
        input_index: u64,
        move_index: Option<usize>,
    ) -> Result<(), ObjectId> {
        let source = self.input.source;
        let live = state.objects.get(source.source);
        let moved = if let Some(index) = move_index {
            live.zone_change_count
                == source
                    .zone_change_count
                    .checked_add(1)
                    .ok_or(source.source)?
                && matches!(state.engine.event_history.get(index),Some(CommittedEvent::ZoneChange{object,from,to:Zone::Library,controller_before}) if *object==source.source && *from==source.zone && *controller_before==source.controller)
        } else {
            source.zone == Zone::Library
                && AbilitySourceContractV4::capture(state, source.source) == source
                && state.players[source.owner.index()]
                    .library
                    .contains(&source.source)
        };
        if live.zone != Zone::Library
            || !moved
            || !matches!(state.engine.event_history.get(input_index as usize),Some(CommittedEvent::ProgenitusInputV1{input})if input.as_ref()==&self.input)
        {
            return Err(source.source);
        }
        let receipt = CommittedEvent::ProgenitusShuffledV1 {
            input_history_index: input_index,
            move_history_index: move_index.map(|index| index as u64),
            source,
        };
        // Revealing the selected source is public, while shuffling clears all
        // library-position knowledge through the existing RNG transaction.
        state.engine.event_log.push(receipt.clone());
        state.engine.event_history.push(receipt);
        state
            .commit_library_shuffle(source.owner, self.shuffle)
            .map_err(|_| source.source)
    }
}
/// Private lexical completion held only by the original batch dispatcher.
pub(crate) struct CompletionV1 {
    plan: PlanV1,
    input: u64,
    moved: Option<usize>,
}
impl CompletionV1 {
    pub(crate) fn new(plan: PlanV1, input: u64, moved: Option<usize>) -> Self {
        Self { plan, input, moved }
    }
}
/// Keep every selected move fixed, then perform one checked shuffle per actual
/// replacement in original batch order. No trigger collection or priority occurs
/// between these phases. Repeated owner shuffles are not collapsed.
pub(crate) fn finish_batch(
    state: &mut GameState,
    completions: Vec<CompletionV1>,
) -> Result<(), ObjectId> {
    for mut completion in completions {
        let source = completion.plan.input.source;
        completion.plan.shuffle = state
            .preflight_library_shuffle(source.owner)
            .map_err(|_| source.source)?;
        completion
            .plan
            .finish(state, completion.input, completion.moved)?;
    }
    Ok(())
}
/// Final-PT previews preserve original batch ordering and original ability state.
/// Previewed RNG successors only check that every actual shuffle can commit.
pub(crate) fn preview(
    state: &GameState,
    events: &[ProposedEvent],
) -> Result<Option<Vec<ProposedEvent>>, ObjectId> {
    let mut rng_scene = None;
    let mut out = None;
    for (index, event) in events.iter().enumerate() {
        let Some(plan) = plan(state, event, None)? else {
            continue;
        };
        if events[..index].iter().any(|earlier| matches!(earlier,ProposedEvent::ZoneChange(z)if z.object==plan.input.source.source)) { return Err(plan.input.source.source); }
        let scene = rng_scene.get_or_insert_with(|| state.clone());
        let token = scene
            .preflight_library_shuffle(plan.input.source.owner)
            .map_err(|_| plan.input.source.source)?;
        scene
            .commit_library_shuffle(plan.input.source.owner, token)
            .map_err(|_| plan.input.source.source)?;
        out.get_or_insert_with(|| events.to_vec())[index] = plan.rewritten();
    }
    Ok(out)
}

/// Only the bounded new family changes an existing order domain. Ordinary
/// batches keep their original path and bytes. Other replacement semantics
/// remain delegated to the actual native replacement interpreter on a clone.
/// The replacement's reveal precedes a suspended actual-entrant order
/// choice. Reveal only each replaced source, never unrelated hidden identities.
pub(crate) fn reveal_order_sources(
    state: &mut GameState,
    original: &[crate::effect::EffectObjectBinding],
) -> Result<(), String> {
    for binding in original {
        if plan(
            state,
            &ProposedEvent::zone_change(binding.object, Zone::Graveyard),
            None,
        )
        .map_err(|_| "Progenitus reveal quote")?
        .is_none()
        {
            continue;
        }
        let object = state.objects.get(binding.object);
        let owner = object.owner;
        match object.zone {
            Zone::Library => {
                let position = state.players[owner.index()]
                    .library
                    .iter()
                    .position(|id| *id == binding.object)
                    .ok_or("Progenitus revealed library source missing")?;
                for viewer in [crate::ids::PlayerId::P0, crate::ids::PlayerId::P1] {
                    state.reveal_library_position(viewer, owner, position);
                }
            }
            Zone::Hand => {
                for viewer in [crate::ids::PlayerId::P0, crate::ids::PlayerId::P1] {
                    state.reveal_hand_card(viewer, owner, binding.object)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}
pub(crate) fn graveyard_members(
    state: &GameState,
    original: &[crate::effect::EffectObjectBinding],
) -> Result<Option<Vec<crate::effect::EffectObjectBinding>>, String> {
    let mut selected = false;
    for binding in original {
        if plan(
            state,
            &ProposedEvent::zone_change(binding.object, Zone::Graveyard),
            None,
        )
        .map_err(|_| "Progenitus replacement quote")?
        .is_some()
        {
            selected = true;
        }
    }
    if !selected {
        return Ok(None);
    }
    let mut scene = state.clone();
    let mut members = Vec::new();
    for binding in original {
        let event = ProposedEvent::zone_change(binding.object, Zone::Graveyard);
        if plan(state, &event, None)
            .map_err(|_| "Progenitus replacement quote")?
            .is_some()
        {
            continue;
        }
        if matches!(event::apply_replacements(&mut scene,event),Some(ProposedEvent::ZoneChange(z))if z.to_zone==Zone::Graveyard)
        {
            members.push(*binding);
        }
        if scene.engine.halted.is_some() {
            return Err("native graveyard replacement quote halted".into());
        }
    }
    Ok(Some(members))
}
pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    for (at, event) in state.engine.event_history.iter().enumerate() {
        if matches!(event,CommittedEvent::ProgenitusInputV1{..}) && state.engine.event_history.iter().filter(|entry|matches!(entry,CommittedEvent::ProgenitusShuffledV1{input_history_index,..}if *input_history_index==at as u64)).count()!=1 { return Err("Progenitus input has no unique actual shuffle".into()); }
    }
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::ProgenitusShuffledV1 {
            input_history_index,
            move_history_index,
            source,
        } = event
        else {
            continue;
        };
        let input_at =
            usize::try_from(*input_history_index).map_err(|_| "Progenitus input width")?;
        let valid_move=match move_history_index {
            Some(index)=>usize::try_from(*index).ok().is_some_and(|move_at| input_at<move_at && move_at<at
                && matches!(state.engine.event_history.get(move_at),Some(CommittedEvent::ZoneChange{object,from,to:Zone::Library,controller_before})if *object==source.source && *from==source.zone && *controller_before==source.controller)),
            None=>source.zone==Zone::Library && input_at<at,
        };
        if !valid_move || !definition(source.card_def)
            || !matches!(state.engine.event_history.get(input_at),Some(CommittedEvent::ProgenitusInputV1{input})if input.source==*source && input.original.object==source.source && input.original.to_zone==Zone::Graveyard)
            || state.engine.event_history.iter().filter(|e|matches!(e,CommittedEvent::ProgenitusShuffledV1{input_history_index:i,..}if i==input_history_index)).count()!=1 { return Err("Progenitus replacement lost original input/move".into()); }
        crate::engine::validate_historical_ability_source_contract(state, *source)?;
    }
    Ok(())
}
pub(crate) fn refuse_legacy(state: &GameState) -> Result<(), String> {
    if crate::standard_cards_v1::has_battlefield_copy_origin_v1(state, definition)
        || state.objects.iter().any(|(_, o)| {
            matches!(o.zone, Zone::Battlefield | Zone::Stack | Zone::Graveyard)
                && o.v4.face_down_v1.is_none()
                && definition(o.card_def)
        })
        || state.engine.event_history.iter().any(|e| {
            matches!(
                e,
                CommittedEvent::ProgenitusInputV1 { .. }
                    | CommittedEvent::ProgenitusShuffledV1 { .. }
            )
        })
    {
        Err("Progenitus needs a complete successor".into())
    } else {
        Ok(())
    }
}

impl std::hash::Hash for InputV1 {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        use std::hash::Hash;
        self.source.hash(h);
        serde_json::to_vec(&self.original)
            .expect("native proposal serializes")
            .hash(h);
    }
}

pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == 480
        && card_def::CARD_DEFS.get(usize::from(id)).is_some_and(|d| {
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
            d.name == "Progenitus"
                && d.capability == card_def::CardCapability::Partial
                && d.cost
                    == crate::mana::Cost {
                        pips: &[
                            crate::mana::Pip::Colored(crate::mana::ManaColor::W),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::W),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::U),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::U),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::B),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::B),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::R),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::R),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::G),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::G),
                        ],
                        generic: 0,
                        x_count: 0,
                    }
                && d.generic_cost_reduction.is_none()
                && d.ward_cost.is_none()
                && !d.spell_cannot_be_countered
                && !d.records_mana_spent
                && d.equipment.is_none()
                && d.types == [CardType::Creature]
                && d.subtypes == [card_def::Subtype::Hydra, card_def::Subtype::Avatar]
                && d.supertypes == [card_def::Supertype::Legendary]
                && d.power == Some(10)
                && d.toughness == Some(10)
                && d.starting_loyalty.is_none()
                && d.enters_with_plus_one_counters.is_none()
                && d.controlled_counter_keyword.is_none()
                && d.game_outcome_protection_v1.is_none()
                && d.controlled_creature_grant.is_none()
                && !d.is_land
                && d.produces_mana.is_empty()
                && d.colors
                    == [
                        crate::mana::ManaColor::W,
                        crate::mana::ManaColor::U,
                        crate::mana::ManaColor::B,
                        crate::mana::ManaColor::R,
                        crate::mana::ManaColor::G,
                    ]
                && d.target_spec == card_def::TargetSpec::None
                && d.keywords == Keywords::PROTECTION_FROM_EVERYTHING
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
                && d.activated_abilities.is_empty()
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
                && d.mana_value == 10
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
