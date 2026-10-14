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

pub mod death_custody;
pub mod grant_primitive;
pub mod spell_custody;

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
    primitive_death_program_from_input_v50(
        context,
        was_token_at_death,
        state.objects.try_get(source.source),
        state.players[source.owner.index()]
            .graveyard
            .contains(&source.source),
    )
}

pub(crate) fn primitive_death_program_from_input_v50(
    context: &ExecCtx,
    was_token_at_death: bool,
    original: Option<&crate::state::GameObject>,
    in_original_graveyard: bool,
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
        && original.is_some_and(|live| {
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
                && in_original_graveyard
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
    fn ownerless_install_root_refuses_without_grant_or_gameplay_mutation_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let context = ExecCtx::no_targets(source, actor);
            for (round, op) in [
                EffectOp::FakeInstallGrantV50,
                EffectOp::FakeDeathFromCaptureV50 { capture: u64::MAX },
            ]
            .into_iter()
            .cycle()
            .take(6)
            .enumerate()
            {
                state = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                let before = state.clone();
                effect::execute(&op, &context, &mut state);
                assert_eq!(
                    state.engine.halted,
                    Some((
                        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                        source
                    ))
                );
                state.engine.halted = None;
                assert_eq!(state, before, "ownerless {op:?} restore {round}");
                assert!(spell_custody::installed_grants(&state).unwrap().is_empty());
            }
        }
    }

    #[test]
    fn forged_death_root_and_original_publication_refuse_readonly_both_seats() {
        use crate::engine::{self, Action, Decision};
        use crate::event::CommittedEvent;
        use crate::state::{AbilitySourceContractV4, StackItem, StackItemKind, StackStateV4};
        use spell_custody::JournalV50;
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let item = StackItem {
                kind: StackItemKind::TriggeredAbility,
                source,
                controller: actor,
                targets: Vec::new(),
                is_copy: false,
                inline_effect: Some(EffectOp::FakeDeathFromCaptureV50 { capture: 0 }),
                discarded: Vec::new(),
                is_flashback: false,
                mode_chosen: 0,
                madness_offer: false,
                kicked: false,
                v4: StackStateV4 {
                    stack_item_id: crate::ids::StackItemId(1),
                    ability_source_contract: Some(AbilitySourceContractV4::capture(&state, source)),
                    ..Default::default()
                },
            };
            state.stack.push(item.clone());
            for _ in 0..3 {
                state = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                let before = state.clone();
                assert!(death_custody::validate_trigger_identity(&state, &item).is_err());
                assert!(death_custody::publish(&mut state, &item).is_err());
                assert_eq!(state, before);
                assert!(death_custody::acquire_counter(&state, &item).is_err());
                assert!(death_custody::acquire_resolution(&state, &item).is_err());
                assert!(
                    engine::counter_stack_item_by_id(&mut state, item.v4.stack_item_id).is_err()
                );
                assert_eq!(state, before);
                assert!(engine::step(&mut state, Action::Pass).is_err());
                assert_eq!(state, before);
                assert!(matches!(
                    engine::advance_until_decision(&mut state),
                    Decision::Halted { .. }
                ));
                state.engine.halted = None;
                assert_eq!(state, before);
            }
            for orphan in [
                JournalV50::DeathPublished {
                    capture: 0,
                    collection: 0,
                    item: Box::new(item.clone()),
                },
                JournalV50::DeathCounterInput {
                    publication: 0,
                    original_stack: vec![item.clone()],
                },
                JournalV50::DeathCounterClosed {
                    input: 0,
                    remaining_stack: Vec::new(),
                },
                JournalV50::DeathResolutionInput {
                    publication: 0,
                    original_stack: vec![item.clone()],
                    context: Box::new(context(&state, source, actor)),
                    original_source: Some(Box::new(state.objects.get(source).clone())),
                    in_original_graveyard: false,
                    program: Box::new(EffectOp::Sequence(Vec::new())),
                },
                JournalV50::DeathTreasureInput {
                    resolution: 0,
                    replacement: crate::doubling_season_v48::ReplacementInputV48::capture(
                        &state, actor,
                    ),
                },
                JournalV50::DeathResolutionClosed {
                    input: 0,
                    treasure: 0,
                    returned: None,
                    created: Vec::new(),
                },
            ] {
                let mut injected = state.clone();
                injected
                    .engine
                    .event_history
                    .push(CommittedEvent::FakeJournalV50 {
                        journal: Box::new(orphan),
                    });
                for _ in 0..3 {
                    injected =
                        serde_json::from_slice(&serde_json::to_vec(&injected).unwrap()).unwrap();
                    let before = injected.clone();
                    assert!(engine::step(&mut injected, Action::Pass).is_err());
                    assert_eq!(injected, before);
                    assert!(matches!(
                        engine::advance_until_decision(&mut injected),
                        Decision::Halted { .. }
                    ));
                    injected.engine.halted = None;
                    assert_eq!(injected, before);
                }
            }
        }
    }

    #[test]
    fn orphan_cleanup_and_empty_death_inputs_refuse_before_engine_work_both_seats() {
        use spell_custody::JournalV50;
        for actor in [PlayerId::P0, PlayerId::P1] {
            for journal in [
                JournalV50::CleanupCompleted {
                    occurrence: u64::MAX,
                },
                JournalV50::DeathInput {
                    lookbacks: Vec::new(),
                    departure: 0,
                },
                JournalV50::DeathCollected {
                    input: u64::MAX,
                    ordinal: 0,
                    capture: 0,
                },
                JournalV50::CollectionCompleted { input: u64::MAX },
                JournalV50::CollectionInput { captures: vec![0] },
            ] {
                let mut state = ready_state_for(actor);
                state
                    .engine
                    .event_history
                    .push(crate::event::CommittedEvent::FakeJournalV50 {
                        journal: Box::new(journal),
                    });
                for _ in 0..3 {
                    state = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                    let before = state.clone();
                    assert!(crate::engine::step(&mut state, crate::engine::Action::Pass).is_err());
                    assert_eq!(state, before);
                    assert!(matches!(
                        crate::engine::advance_until_decision(&mut state),
                        crate::engine::Decision::Halted {
                            mechanic: crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                            source: ObjectId(0),
                        }
                    ));
                    state.engine.halted = None;
                    assert_eq!(state, before);
                }
            }
        }
    }

    #[test]
    fn forged_mutable_death_input_cannot_supply_collection_authority_both_seats() {
        use crate::engine::{self, Action, Decision};
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let events = vec![crate::event::CommittedEvent::FakeJournalV50 {
                journal: Box::new(spell_custody::JournalV50::DeathInput {
                    lookbacks: Vec::new(),
                    departure: u64::MAX,
                }),
            }];
            for _ in 0..3 {
                state = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                let before = state.clone();
                assert!(death_custody::collect(&mut state, &events).is_err());
                assert_eq!(state, before);
            }
            state.engine.event_log = events;
            for _ in 0..3 {
                state = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                let before = state.clone();
                assert!(engine::step(&mut state, Action::Pass).is_err());
                assert_eq!(state, before);
                assert!(matches!(
                    engine::advance_until_decision(&mut state),
                    Decision::Halted { .. }
                ));
                state.engine.halted = None;
                assert_eq!(state, before);
            }
        }
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

    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    fn primitive_finishes_treasure_before_legend_sba_choice_and_restore_both_seats() {
        for owner in [PlayerId::P0, PlayerId::P1] {
            for keep_returned in [false, true] {
                let mut state = ready_state_for(owner);
                let existing = put(
                    &mut state,
                    owner,
                    "Balmor, Battlemage Captain",
                    Zone::Battlefield,
                );
                let returned = put(
                    &mut state,
                    owner,
                    "Balmor, Battlemage Captain",
                    Zone::Graveyard,
                );
                let beneficiary = owner.opponent();
                let context = context(&state, returned, beneficiary);
                let program = primitive_death_program_v50(&state, &context, false).unwrap();
                effect::execute(&program, &context, &mut state);
                assert_eq!(state.objects.get(returned).zone, Zone::Battlefield);
                assert!(state.objects.get(returned).tapped);
                assert_eq!(treasures(&state, beneficiary).len(), 1);
                assert!(state.pending_legend_rule_v1.is_none());

                // This is the next SBA pass, after both ordered instructions.
                // The primitive does not publish an actual Fake death trigger.
                assert!(crate::legend_rule_v1::stage(&mut state, &[]));
                assert!(matches!(
                    crate::legend_rule_v1::drain_or_decide(&mut state).unwrap(),
                    Some(crate::engine::Decision::ChooseLegendPermanent { player, candidates })
                        if player == owner && candidates.len() == 2
                            && candidates.contains(&existing) && candidates.contains(&returned)
                ));
                let serialized = serde_json::to_vec(&state).unwrap();
                let mut restored: GameState = serde_json::from_slice(&serialized).unwrap();
                let keep = if keep_returned { returned } else { existing };
                let discard = if keep_returned { existing } else { returned };
                for candidate in [&mut state, &mut restored] {
                    crate::legend_rule_v1::answer(candidate, keep).unwrap();
                    assert!(crate::legend_rule_v1::drain_or_decide(candidate)
                        .unwrap()
                        .is_none());
                    assert_eq!(candidate.objects.get(keep).zone, Zone::Battlefield);
                    assert_eq!(candidate.objects.get(discard).zone, Zone::Graveyard);
                    assert_eq!(treasures(candidate, beneficiary).len(), 1);
                    assert!(treasures(candidate, owner).is_empty());
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&restored).unwrap()
                );
            }
        }
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
