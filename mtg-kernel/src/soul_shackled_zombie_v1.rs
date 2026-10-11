//! Bounded, unregistered single-graveyard ETB program.
use crate::card_def::{CardType, TargetSpec};
use crate::effect::ExecCtx;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{AbilitySourceContractV4, GameState, StackTargetContractV4, Target, Zone};

pub(crate) fn legal_targets(state: &GameState, prefix: &[Target]) -> Vec<Target> {
    if prefix.len() >= 2 {
        return Vec::new();
    }
    let owner = match prefix.first() {
        None => None,
        Some(Target::Object(id)) => state.objects.try_get(*id).map(|object| object.owner),
        Some(_) => return Vec::new(),
    };
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .filter(|player| owner.is_none_or(|owner| owner == *player))
        .flat_map(|player| {
            state.players[player.index()]
                .graveyard
                .iter()
                .copied()
                .filter(move |id| {
                    state.objects.try_get(*id).is_some_and(|object| {
                        object.zone == Zone::Graveyard
                            && object.owner == player
                            && !object.v4.is_token
                            && object.spell_copy_origin.is_none()
                    })
                })
        })
        .filter(|id| !prefix.contains(&Target::Object(*id)))
        .map(Target::Object)
        .collect()
}

/// Captured owners, rather than current controllers or current zones, keep
/// the announced shared-graveyard relationship through ordinary departures.
pub(crate) fn validate_target_contracts(contracts: &[StackTargetContractV4]) -> Result<(), String> {
    if contracts.len() > 2 {
        return Err("single-graveyard trigger has too many targets".into());
    }
    let mut owner = None;
    let mut objects = Vec::new();
    for contract in contracts {
        let StackTargetContractV4::Object {
            object,
            owner: captured_owner,
            zone: Zone::Graveyard,
            spell_copy_origin: None,
            ..
        } = contract
        else {
            return Err("single-graveyard trigger lost its card target".into());
        };
        if owner.is_some_and(|owner| owner != *captured_owner) || objects.contains(object) {
            return Err(
                "single-graveyard trigger changed its shared graveyard or distinctness".into(),
            );
        }
        owner = Some(*captured_owner);
        objects.push(*object);
    }
    Ok(())
}

fn owns_program(state: &GameState, source: ObjectId) -> bool {
    crate::trigger::triggers_for(state.objects.get(source).card_def)
        .iter()
        .any(|ability| {
            (ability.effect)() == crate::effect::EffectOp::ExileSingleGraveyardTargetsThenDrainTwoV1
        })
}

/// Commit only for an actual entry by a definition owning this program.
/// This history-only receipt fixes the ETB controller independently of the
/// mutable pending/stack contracts and emits nothing on predecessor paths.
pub(crate) fn record_entry(state: &mut GameState, source: ObjectId) {
    if !owns_program(state, source) {
        return;
    }
    let source_contract = AbilitySourceContractV4::capture(state, source);
    let entry_event_index = state.engine.event_history.len() - 1;
    state
        .engine
        .event_history
        .push(CommittedEvent::SingleGraveyardEntryV1 {
            source_contract,
            entry_event_index,
        });
}

pub(crate) fn validate_source(
    state: &GameState,
    source: AbilitySourceContractV4,
) -> Result<(), String> {
    if source.zone != Zone::Battlefield {
        return Err("single-graveyard ETB lost its battlefield source".into());
    }
    if state
        .engine
        .event_log
        .iter()
        .any(|event| matches!(event, CommittedEvent::SingleGraveyardEntryV1 { .. }))
    {
        return Err("single-graveyard entry authority is history-only".into());
    }
    let entries = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            CommittedEvent::SingleGraveyardEntryV1 {
                source_contract,
                entry_event_index,
            } if source_contract.source == source.source
                && source_contract.zone_change_count == source.zone_change_count =>
            {
                Some((index, *source_contract, *entry_event_index))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(receipt_index, captured, entry_index)] = entries.as_slice() else {
        return Err("single-graveyard ETB lost its unique committed entry".into());
    };
    let actual_entry = match state.engine.event_history.get(*entry_index) {
        Some(CommittedEvent::ZoneChange {
            object,
            to: Zone::Battlefield,
            ..
        }) => *object == source.source,
        Some(CommittedEvent::CreateToken {
            object,
            token_def,
            controller,
        }) => {
            *object == source.source
                && *token_def == source.card_def
                && *controller == source.controller
                && *controller == source.owner
        }
        _ => false,
    };
    if *captured != source || *entry_index >= *receipt_index || !actual_entry {
        return Err("single-graveyard ETB source/controller differs from actual entry".into());
    }
    Ok(())
}

pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    for item in &state.stack {
        if item.inline_effect.as_ref()
            == Some(&crate::effect::EffectOp::ExileSingleGraveyardTargetsThenDrainTwoV1)
        {
            crate::engine::validated_stack_item_target_spec(item, state)?;
        }
    }
    Ok(())
}

pub(crate) fn execute(state: &mut GameState, ctx: &ExecCtx) {
    // Fix each target's type and legality before moving any target. Only an
    // actual one-generation Graveyard->Exile move satisfies the condition.
    let targets = ctx
        .targets
        .iter()
        .enumerate()
        .filter_map(|(index, target)| {
            let Target::Object(card) = *target else {
                return None;
            };
            let source = ctx.ability_source_contract?;
            if !ctx.target_incarnation_matches(index, state)
                || !crate::engine::effect_target_is_legal_from_ability_source(
                    state,
                    source,
                    ctx.controller,
                    TargetSpec::UpToTwoCardsInSingleGraveyard,
                    &ctx.targets,
                    index,
                )
            {
                return None;
            }
            let object = state.objects.get(card);
            Some((
                card,
                object.zone_change_count,
                crate::engine::object_has_type(state, card, CardType::Creature),
            ))
        })
        .collect::<Vec<_>>();
    let mut exiled_creature = false;
    for (card, generation, was_creature) in targets {
        event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Exile));
        let object = state.objects.get(card);
        exiled_creature |= was_creature
            && object.zone == Zone::Exile
            && generation.checked_add(1) == Some(object.zone_change_count);
    }
    if exiled_creature {
        event::propose_and_commit(
            state,
            ProposedEvent::life_loss(ctx.controller.opponent(), 2),
        );
        event::propose_and_commit(state, ProposedEvent::life_gain(ctx.controller, 2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_def::{card_id_by_name, CARD_DEFS};
    use crate::state::{GameObject, ObjectStateV4, Step};

    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    fn actual_zombie_token_copy_captures_committed_entry_and_rejects_rewritten_create_token() {
        // Future printed fixture requires actual catalog admission, never a stand-in root.
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let original = put(
                &mut state,
                actor.opponent(),
                "Soul-Shackled Zombie",
                Zone::Battlefield,
            );
            let token =
                crate::standard_cards_v1::create_token_copy(&mut state, original, actor).unwrap();
            let source = AbilitySourceContractV4::capture(&state, token);
            assert!(state.objects.get(token).v4.is_token);
            validate_source(&state, source).unwrap();
            crate::standard_cards_v1::gain_control(&mut state, token, actor.opponent());
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(token, Zone::Exile));
            let bytes = serde_json::to_vec(&state).unwrap();
            let restored: GameState = serde_json::from_slice(&bytes).unwrap();
            validate_source(&restored, source).unwrap();
            for rewrite in 0..3 {
                let mut forged = restored.clone();
                let entry = forged.engine.event_history.iter_mut().find(|entry|
                    matches!(entry, CommittedEvent::CreateToken { object, .. } if *object == token)).unwrap();
                if let CommittedEvent::CreateToken {
                    object,
                    token_def,
                    controller,
                } = entry
                {
                    match rewrite {
                        0 => *object = original,
                        1 => *token_def = card_id_by_name("Forest").unwrap(),
                        _ => *controller = actor.opponent(),
                    }
                }
                assert!(validate_source(&forged, source).is_err());
            }
        }
    }

    fn ready(actor: PlayerId) -> GameState {
        let forest = card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            |id| CARD_DEFS[id as usize].object_name.into(),
            110_310,
            actor,
        );
        state.step = Step::Main1;
        state
    }
    fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
        let card_def = card_id_by_name(name).unwrap();
        let id = state.objects.push(GameObject {
            card_def,
            name: CARD_DEFS[card_def as usize].object_name.into(),
            owner,
            controller: owner,
            zone: Zone::Hand,
            tapped: false,
            summoning_sick: false,
            damage: 0,
            counters: Default::default(),
            attachments: Vec::new(),
            v4: ObjectStateV4::from_card_def(card_def),
            spell_copy_origin: None,
            plotted_turn: None,
            zone_change_count: 0,
        });
        state.players[owner.index()].hand.push(id);
        event::propose_and_commit(state, ProposedEvent::zone_change(id, zone));
        id
    }
    fn ctx(state: &GameState, source: ObjectId, actor: PlayerId, cards: &[ObjectId]) -> ExecCtx {
        let mut ctx = ExecCtx::no_targets(source, actor);
        ctx.ability_source_contract = Some(AbilitySourceContractV4::capture(state, source));
        ctx.targets = cards.iter().copied().map(Target::Object).collect();
        ctx.target_contracts = ctx
            .targets
            .iter()
            .map(|target| StackTargetContractV4::capture(state, *target))
            .collect();
        ctx
    }

    #[test]
    fn registered_standins_prove_single_physical_graveyard_distinctness_and_absent_receipt() {
        // Fume Spitter and Forest are genuine registered definitions. This
        // exercises primitives only, without claiming their printed ETBs.
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let one = put(&mut state, actor, "Fume Spitter", Zone::Graveyard);
            let two = put(&mut state, actor, "Forest", Zone::Graveyard);
            let other = put(
                &mut state,
                actor.opponent(),
                "Fume Spitter",
                Zone::Graveyard,
            );
            state.objects.get_mut(two).controller = actor.opponent();
            assert!(legal_targets(&state, &[]).contains(&Target::Object(other)));
            assert_eq!(
                legal_targets(&state, &[Target::Object(one)]),
                vec![Target::Object(two)]
            );
            let mixed = [
                StackTargetContractV4::capture(&state, Target::Object(one)),
                StackTargetContractV4::capture(&state, Target::Object(other)),
            ];
            assert!(validate_target_contracts(&mixed).is_err());
            assert!(validate_target_contracts(&[mixed[0], mixed[0]]).is_err());
            assert!(!state
                .engine
                .event_history
                .iter()
                .any(|event| matches!(event, CommittedEvent::SingleGraveyardEntryV1 { .. })));
            let saved = serde_json::to_vec(&state).unwrap();
            let restored: GameState = serde_json::from_slice(&saved).unwrap();
            assert_eq!(serde_json::to_vec(&restored).unwrap(), saved);
            assert!(!String::from_utf8(saved)
                .unwrap()
                .contains("SingleGraveyardEntryV1"));
        }
    }

    #[test]
    fn registered_standins_prove_once_only_creature_exile_and_exact_stale_recheck() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for stale_creature in [false, true] {
                let mut state = ready(actor);
                let source = put(&mut state, actor, "Fume Spitter", Zone::Battlefield);
                let creature = put(
                    &mut state,
                    actor.opponent(),
                    "Fume Spitter",
                    Zone::Graveyard,
                );
                let second = put(
                    &mut state,
                    actor.opponent(),
                    if stale_creature {
                        "Forest"
                    } else {
                        "Fume Spitter"
                    },
                    Zone::Graveyard,
                );
                let ctx = ctx(&state, source, actor, &[creature, second]);
                if stale_creature {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(creature, Zone::Hand),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(creature, Zone::Graveyard),
                    );
                }
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Hand),
                );
                let saved = serde_json::to_vec(&state).unwrap();
                state = serde_json::from_slice(&saved).unwrap();
                let initial = (
                    state.players[actor.index()].life,
                    state.players[actor.opponent().index()].life,
                );
                execute(&mut state, &ctx);
                let amount = if stale_creature { 0 } else { 2 };
                assert_eq!(state.players[actor.index()].life, initial.0 + amount);
                assert_eq!(
                    state.players[actor.opponent().index()].life,
                    initial.1 - amount
                );
                assert_eq!(
                    state.objects.get(creature).zone,
                    if stale_creature {
                        Zone::Graveyard
                    } else {
                        Zone::Exile
                    }
                );
                assert_eq!(state.objects.get(second).zone, Zone::Exile);
                // A second direct execution cannot count already-exiled cards.
                execute(&mut state, &ctx);
                assert_eq!(state.players[actor.index()].life, initial.0 + amount);
                assert_eq!(
                    state.players[actor.opponent().index()].life,
                    initial.1 - amount
                );
            }
        }
    }
}
