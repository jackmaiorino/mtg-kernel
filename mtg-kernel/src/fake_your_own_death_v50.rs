//! Ordered death-resolution primitive for the unregistered FDN printing.
//!
//! This factory does not grant an ability, collect a death, or authorize a
//! publication. Its future caller must bind the context and token fact to
//! the original recipient's actual death. No consumer admission is implied.

use crate::card_def::{
    CardType, Keywords, ManaAbilityAmountDef, ManaAbilityCostDef, ManaAbilityDef, Subtype,
    TargetSpec, CARD_DEFS,
};
use crate::effect::{EffectOp, ExecCtx, PlayerRef};
use crate::mana::ManaColor;
use crate::state::{GameState, Zone};

pub mod grant_primitive;

fn treasure_definition() -> Result<u16, String> {
    let id = crate::card_def::card_id_by_name("Treasure Token")
        .ok_or("Fake Your Own Death needs the existing Treasure blueprint")?;
    let definition = &CARD_DEFS[usize::from(id)];
    let colors = [
        ManaColor::W,
        ManaColor::U,
        ManaColor::B,
        ManaColor::R,
        ManaColor::G,
    ];
    if !definition.is_token
        || !definition.is_executable()
        || !definition.has_full_support()
        || definition.object_name != "Treasure Token"
        || definition.types != [CardType::Artifact]
        || definition.subtypes != [Subtype::Treasure]
        || !definition.colors.is_empty()
        || definition.is_land
        || definition.mana_value != 0
        || definition.power.is_some()
        || definition.toughness.is_some()
        || definition.keywords != Keywords::NONE
        || definition.mana_ability_choices != colors
        || definition.produces_mana != colors
        || definition.mana_ability_def
            != Some(ManaAbilityDef {
                cost: ManaAbilityCostDef::TapAndSacrificeSelf,
                amount: ManaAbilityAmountDef::Fixed(1),
                controller_damage: 0,
                max_activations_per_turn: None,
            })
        || (definition.mana_ability)().is_some()
        || definition.mana_ability_includes_chosen_color
        || definition.conditional_tap_yield.is_some()
        || !definition.additional_mana_abilities.is_empty()
        || !definition.additional_mana_ability_conditions.is_empty()
        || !definition.restricted_mana_abilities.is_empty()
        || !definition.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
        || definition.target_spec != TargetSpec::None
        || (definition.spell_effect)().is_some()
        || definition.enters_battlefield_tapped
        || definition.enters_battlefield_tapped_unless.is_some()
        || definition.enters_tapped_unless_controller.is_some()
        || definition.as_enters_choose_color_other_than.is_some()
        || definition.as_enters_color_choice_v1.is_some()
        || definition.enters_with_plus_one_counters.is_some()
        || definition.attachment.is_some()
        || definition.transform_face.is_some()
    {
        return Err("Fake Your Own Death Treasure blueprint changed".into());
    }
    Ok(id)
}

/// Build the ordered program once, at the death ability's resolution input.
/// `was_token_at_death` comes from the original battlefield object, including
/// copied tokens whose printed definition is a nontoken card. The controller
/// in `context` is the death trigger's controller, not the original caster or
/// the returning card's owner. The ordinary resumable interpreter must own
/// this program when its return can suspend for entry or legend choices.
pub fn primitive_death_program_v50(
    state: &GameState,
    context: &ExecCtx,
    was_token_at_death: bool,
) -> Result<EffectOp, String> {
    let source = context
        .ability_source_contract
        .ok_or("Fake Your Own Death primitive lacks its captured graveyard source")?;
    if source.source != context.source
        || source.zone != Zone::Graveyard
        || !context.targets.is_empty()
        || !context.target_contracts.is_empty()
    {
        return Err("Fake Your Own Death primitive source or target shape changed".into());
    }
    let token_def = treasure_definition()?;
    let can_return = !was_token_at_death
        && state.objects.try_get(source.source).is_some_and(|live| {
            live.zone == Zone::Graveyard
                && live.zone_change_count == source.zone_change_count
                && live.card_def == source.card_def
                && live.owner == source.owner
                && !live.v4.is_token
                && live.spell_copy_origin.is_none()
                && CARD_DEFS
                    .get(usize::from(live.card_def))
                    .is_some_and(|definition| {
                        [
                            CardType::Artifact,
                            CardType::Creature,
                            CardType::Enchantment,
                            CardType::Land,
                            CardType::Planeswalker,
                        ]
                        .into_iter()
                        .any(|kind| definition.has_type(kind))
                    })
                && state.players[source.owner.index()]
                    .graveyard
                    .contains(&source.source)
        });
    let mut program = Vec::with_capacity(2);
    if can_return {
        program.push(EffectOp::ReturnAbilitySourceFromGraveyard { tapped: true });
    }
    // Failure to return the original card does not cancel the second effect.
    program.push(EffectOp::CreateToken {
        token_def,
        controller: PlayerRef::Controller,
    });
    Ok(EffectOp::Sequence(program))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect;
    use crate::event::{self, ProposedEvent};
    use crate::ids::{ObjectId, PlayerId};
    use crate::policy_observation_v6::tests::{put, ready_state_for};
    use crate::state::AbilitySourceContractV4;

    fn context(state: &GameState, source: ObjectId, beneficiary: PlayerId) -> ExecCtx {
        let mut context = ExecCtx::no_targets(source, beneficiary);
        context.ability_source_contract = Some(AbilitySourceContractV4::capture(state, source));
        context
    }

    fn treasures(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
        let blueprint = treasure_definition().unwrap();
        state.players[player.index()]
            .battlefield
            .iter()
            .copied()
            .filter(|id| {
                let object = state.objects.get(*id);
                object.card_def == blueprint && object.v4.is_token
            })
            .collect()
    }

    #[test]
    fn primitive_returns_tapped_to_owner_and_creates_treasure_for_trigger_controller_both_seats() {
        for owner in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(owner);
            let source = put(&mut state, owner, "Llanowar Elves", Zone::Graveyard);
            let context = context(&state, source, owner.opponent());
            let before = state.objects.get(source).zone_change_count;
            let program = primitive_death_program_v50(&state, &context, false).unwrap();
            effect::execute(&program, &context, &mut state);
            let returned = state.objects.get(source);
            assert_eq!(returned.zone, Zone::Battlefield);
            assert_eq!(returned.zone_change_count, before + 1);
            assert_eq!((returned.owner, returned.controller), (owner, owner));
            assert!(returned.tapped);
            assert!(state.players[owner.index()].battlefield.contains(&source));
            assert_eq!(treasures(&state, owner.opponent()).len(), 1);
            assert!(treasures(&state, owner).is_empty());
        }
    }

    #[test]
    fn primitive_does_not_follow_reentered_card_but_still_creates_treasure_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let source = put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
            let original = context(&state, source, actor);
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Graveyard),
            );
            let generation = state.objects.get(source).zone_change_count;
            let program = primitive_death_program_v50(&state, &original, false).unwrap();
            effect::execute(&program, &original, &mut state);
            assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
            assert_eq!(state.objects.get(source).zone_change_count, generation);
            assert!(!state.players[actor.index()].battlefield.contains(&source));
            assert_eq!(treasures(&state, actor).len(), 1);
        }
    }

    #[test]
    fn primitive_original_token_fact_prevents_return_even_with_nontoken_definition_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            // A primitive operand models the original copied-token fact. This
            // fixture does not claim Llanowar Elves or Fake is a token maker.
            let source = put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
            let original = context(&state, source, actor);
            let generation = state.objects.get(source).zone_change_count;
            let program = primitive_death_program_v50(&state, &original, true).unwrap();
            effect::execute(&program, &original, &mut state);
            assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
            assert_eq!(state.objects.get(source).zone_change_count, generation);
            assert_eq!(treasures(&state, actor).len(), 1);
        }
    }

    #[test]
    fn primitive_manifested_instant_cannot_return_but_still_creates_treasure_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            // The primitive's graveyard operand is the actual front face.
            // Authentic prior manifest/grant/death custody belongs to the
            // future printing path; this does not manufacture that history.
            let source = put(&mut state, actor, "Lightning Bolt", Zone::Graveyard);
            let original = context(&state, source, actor);
            let generation = state.objects.get(source).zone_change_count;
            let program = primitive_death_program_v50(&state, &original, false).unwrap();
            effect::execute(&program, &original, &mut state);
            assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
            assert_eq!(state.objects.get(source).zone_change_count, generation);
            assert_eq!(treasures(&state, actor).len(), 1);
        }
    }

    #[test]
    fn primitive_missing_source_contract_is_rejected_without_mutating_state() {
        let mut state = ready_state_for(PlayerId::P0);
        let source = put(&mut state, PlayerId::P0, "Llanowar Elves", Zone::Graveyard);
        let before = serde_json::to_vec(&state).unwrap();
        assert!(primitive_death_program_v50(
            &state,
            &ExecCtx::no_targets(source, PlayerId::P0),
            false
        )
        .is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }

    #[test]
    fn primitive_treasure_pays_actual_tap_and_sacrifice_for_each_color_after_restore_both_seats() {
        for beneficiary in [PlayerId::P0, PlayerId::P1] {
            for color in [
                ManaColor::W,
                ManaColor::U,
                ManaColor::B,
                ManaColor::R,
                ManaColor::G,
            ] {
                let mut state = ready_state_for(beneficiary);
                let source = put(
                    &mut state,
                    beneficiary.opponent(),
                    "Llanowar Elves",
                    Zone::Graveyard,
                );
                let context = context(&state, source, beneficiary);
                let program = primitive_death_program_v50(&state, &context, false).unwrap();
                effect::execute(&program, &context, &mut state);
                let created = treasures(&state, beneficiary);
                let [treasure] = created.as_slice() else {
                    panic!("expected one genuine created Treasure");
                };
                let treasure = *treasure;
                assert!(!state.objects.get(treasure).tapped);
                let before = serde_json::to_vec(&state).unwrap();
                // Colorless is not among Treasure's printed choices. Refusal
                // must occur before either half of the actual combined cost.
                assert!(crate::engine::step(
                    &mut state,
                    crate::engine::Action::ActivateManaAbilityChoice(treasure, ManaColor::C),
                )
                .is_err());
                assert_eq!(serde_json::to_vec(&state).unwrap(), before);
                let mut replay: GameState = serde_json::from_slice(&before).unwrap();
                let history_start = state.engine.event_history.len();
                let action = crate::engine::Action::ActivateManaAbilityChoice(treasure, color);
                crate::engine::step(&mut state, action.clone()).unwrap();
                crate::engine::step(&mut replay, action).unwrap();
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
                assert_eq!(
                    state.players[beneficiary.index()].mana_pool[color.pool_index()],
                    1
                );
                assert!(treasures(&state, beneficiary).is_empty());
                let history = &state.engine.event_history[history_start..];
                let tap = history
                    .iter()
                    .position(|event| {
                        matches!(event,
                    crate::event::CommittedEvent::Tap { object } if *object == treasure)
                    })
                    .unwrap();
                let sacrifice = history.iter().position(|event| matches!(event,
                    crate::event::CommittedEvent::ZoneChange { object, from: Zone::Battlefield, to: Zone::Graveyard, .. }
                        if *object == treasure)).unwrap();
                let mana = history
                    .iter()
                    .position(|event| {
                        matches!(event,
                    crate::event::CommittedEvent::ManaAdded { player, colors }
                        if *player == beneficiary && *colors == [color])
                    })
                    .unwrap();
                assert!(tap < sacrifice && sacrifice < mana);
            }
        }
    }
}
