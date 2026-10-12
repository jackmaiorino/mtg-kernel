//! Atomic pre-move type memory for Scavenging Ooze's printed activation.
use super::ExecCtx;
use crate::card_def::{CardType, TargetSpec};
use crate::event::{self, ProposedEvent};
use crate::state::{GameState, Target, Zone};

pub(super) fn execute(ctx: &ExecCtx, state: &mut GameState) {
    let Some(source) = ctx.ability_source_contract else {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
        return;
    };
    if source.source != ctx.source
        || source.controller != ctx.controller
        || source.zone != Zone::Battlefield
        || crate::engine::validate_historical_ability_source_contract(state, source).is_err()
    {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
        return;
    }
    let Some(Target::Object(card)) = ctx.targets.first().copied() else {
        return;
    };
    if ctx.targets.len() != 1
        || ctx.target_contracts.len() != 1
        || !ctx.target_incarnation_matches(0, state)
        || !crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            TargetSpec::CardInGraveyards,
            &ctx.targets,
            0,
        )
    {
        return;
    }
    let was_creature = crate::engine::object_has_type(state, card, CardType::Creature);
    event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Exile));
    // The printed conditional is "if it was a creature card", not "if exiled".
    // Later instructions still use the pre-move fact when a move is replaced.
    if !was_creature {
        return;
    }
    if state.objects.try_get(source.source).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.zone_change_count == source.zone_change_count
            && live.owner == source.owner
    }) && event::add_plus_one_counters(state, source.source, ctx.controller, 1).is_err()
    {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
        return;
    }
    event::propose_and_commit(state, ProposedEvent::life_gain(ctx.controller, 1));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_def::{card_id_by_name, CARD_DEFS};
    use crate::effect::EffectOp;
    use crate::ids::{ObjectId, PlayerId};
    use crate::state::{AbilitySourceContractV4, GameObject, ObjectStateV4, StackTargetContractV4};

    fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
        let card_def = card_id_by_name(name).unwrap();
        let id = state.objects.push(GameObject {
            card_def,
            name: name.into(),
            owner,
            controller: owner,
            zone,
            tapped: false,
            summoning_sick: false,
            damage: 0,
            counters: Default::default(),
            attachments: vec![],
            v4: ObjectStateV4::from_card_def(card_def),
            spell_copy_origin: None,
            plotted_turn: None,
            zone_change_count: 0,
        });
        match zone {
            Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
            Zone::Graveyard => state.players[owner.index()].graveyard.push(id),
            _ => panic!("physical fixture placement unsupported"),
        }
        id
    }
    fn physical(actor: PlayerId, owner: PlayerId, name: &str) -> (GameState, ExecCtx, ObjectId) {
        let cards = ["Forest", "Island", "Mountain", "Swamp", "Lightning Bolt"]
            .map(|name| card_id_by_name(name).unwrap());
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &cards,
            &cards,
            |id| CARD_DEFS[id as usize].name.into(),
            11010,
            actor,
        );
        let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let card = put(&mut state, owner, name, Zone::Graveyard);
        let mut ctx = ExecCtx::no_targets(source, actor);
        ctx.ability_source_contract = Some(AbilitySourceContractV4::capture(&state, source));
        ctx.targets = vec![Target::Object(card)];
        ctx.target_contracts = vec![StackTargetContractV4::capture(&state, Target::Object(card))];
        (state, ctx, card)
    }
    fn restored(state: &GameState) -> GameState {
        let bytes = serde_json::to_vec(state).unwrap();
        let restored: GameState = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(bytes, serde_json::to_vec(&restored).unwrap());
        assert_eq!(state.state_hash(), restored.state_hash());
        restored
    }
    #[test]
    fn actual_typed_primitive_remembers_creature_type_and_owner_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for owner in [actor, actor.opponent()] {
                for (name, grows) in [
                    ("Llanowar Elves", true),
                    ("Lightning Bolt", false),
                    ("Forest", false),
                ] {
                    let (mut state, ctx, card) = physical(actor, owner, name);
                    let mut replay = restored(&state);
                    for branch in [&mut state, &mut replay] {
                        let before = branch.players[actor.index()].life;
                        crate::effect::execute(
                            &EffectOp::ExileGraveyardTargetThenGrowSourceIfCreature,
                            &ctx,
                            branch,
                        );
                        assert_eq!(branch.objects.get(card).zone, Zone::Exile);
                        assert!(branch.exile.contains(&card));
                        assert_eq!(branch.objects.get(card).owner, owner);
                        assert_eq!(
                            branch.objects.get(ctx.source).counters.plus1_plus1,
                            i32::from(grows)
                        );
                        assert_eq!(
                            branch.players[actor.index()].life,
                            before + i32::from(grows)
                        );
                    }
                    assert_eq!(
                        serde_json::to_vec(&state).unwrap(),
                        serde_json::to_vec(&replay).unwrap()
                    );
                }
            }
        }
    }
    #[test]
    fn actual_typed_primitive_does_not_follow_reentered_card_or_source() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for change_target in [false, true] {
                let (mut state, ctx, card) = physical(actor, actor.opponent(), "Llanowar Elves");
                if change_target {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(card, Zone::Hand),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(card, Zone::Graveyard),
                    );
                } else {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(ctx.source, Zone::Hand),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(ctx.source, Zone::Battlefield),
                    );
                }
                let mut replay = restored(&state);
                for branch in [&mut state, &mut replay] {
                    let before = branch.players[actor.index()].life;
                    crate::effect::execute(
                        &EffectOp::ExileGraveyardTargetThenGrowSourceIfCreature,
                        &ctx,
                        branch,
                    );
                    assert_eq!(branch.objects.get(ctx.source).counters.plus1_plus1, 0);
                    assert_eq!(
                        branch.players[actor.index()].life,
                        before + i32::from(!change_target)
                    );
                    assert_eq!(
                        branch.objects.get(card).zone,
                        if change_target {
                            Zone::Graveyard
                        } else {
                            Zone::Exile
                        }
                    );
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
            }
        }
    }
    #[test]
    #[cfg(feature = "standard-magezero-fixtures")]
    fn source_becoming_noncreature_still_receives_counter_on_its_exact_permanent() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, ctx, _) = physical(actor, actor.opponent(), "Llanowar Elves");
            // Existing typed characteristic record, not a fabricated printed producer.
            state.objects.get_mut(ctx.source).v4.enduring_enchantment_v1 = true;
            assert!(!crate::engine::object_has_type(
                &state,
                ctx.source,
                CardType::Creature
            ));
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                let life = branch.players[actor.index()].life;
                crate::effect::execute(
                    &EffectOp::ExileGraveyardTargetThenGrowSourceIfCreature,
                    &ctx,
                    branch,
                );
                assert_eq!(branch.objects.get(ctx.source).counters.plus1_plus1, 1);
                assert_eq!(branch.players[actor.index()].life, life + 1);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }

    #[test]
    #[cfg(feature = "standard-magezero-fixtures")]
    fn existing_registered_talent_counter_replacement_is_used_by_ooze_primitive() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, ctx, card) = physical(actor, actor.opponent(), "Llanowar Elves");
            let talent = put(&mut state, actor, "Innkeeper's Talent", Zone::Battlefield);
            let mut level_ctx = ExecCtx::no_targets(talent, actor);
            level_ctx.ability_source_contract =
                Some(AbilitySourceContractV4::capture(&state, talent));
            // Existing typed class-level primitive, not an admitted Ooze in Standard.
            crate::effect::execute(
                &crate::standard_cards_v1::gain_level_two(),
                &level_ctx,
                &mut state,
            );
            crate::effect::execute(
                &crate::standard_cards_v1::gain_level_three(),
                &level_ctx,
                &mut state,
            );
            assert_eq!(crate::standard_cards_v1::class_level(&state, talent), 3);
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                let life = branch.players[actor.index()].life;
                crate::effect::execute(
                    &EffectOp::ExileGraveyardTargetThenGrowSourceIfCreature,
                    &ctx,
                    branch,
                );
                assert_eq!(branch.objects.get(card).zone, Zone::Exile);
                assert_eq!(branch.objects.get(ctx.source).counters.plus1_plus1, 2);
                assert_eq!(branch.players[actor.index()].life, life + 1);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }

    #[test]
    #[cfg(feature = "standard-magezero-fixtures")]
    fn registered_aegis_copy_preserves_same_incarnation_ooze_primitive_source() {
        use crate::effect::EffectObjectBinding;
        use crate::state::ObjectLinkV4;
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, ctx, card) = physical(actor, actor.opponent(), "Llanowar Elves");
            let original = ctx.ability_source_contract.unwrap();
            let aegis = put(&mut state, actor, "Assimilation Aegis", Zone::Battlefield);
            let donor = put(
                &mut state,
                actor.opponent(),
                "Tolarian Terror",
                Zone::Graveyard,
            );
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(donor, Zone::Exile));
            let host = ObjectLinkV4 {
                object: ctx.source,
                zone_change_count: original.zone_change_count,
            };
            // Existing typed registered copy executor, not an admitted Ooze
            // or a Standard printed Ooze ability producer.
            state.objects.get_mut(aegis).v4.attached_to = Some(host);
            state.objects.get_mut(aegis).v4.layer_timestamp = Some(17);
            let aegis_source = AbilitySourceContractV4::capture(&state, aegis);
            state.objects.get_mut(donor).v4.exiled_by = Some(ObjectLinkV4 {
                object: aegis,
                zone_change_count: aegis_source.zone_change_count,
            });
            let mut copy_ctx = ExecCtx::no_targets(aegis, actor);
            copy_ctx.ability_source_contract = Some(aegis_source);
            let chosen = EffectObjectBinding {
                object: donor,
                expected_zone: Zone::Exile,
                expected_zone_change_count: state.objects.get(donor).zone_change_count,
            };
            assert!(
                crate::standard_cards_v1::aegis_copy_candidates(&state, &copy_ctx, host, 17)
                    .contains(&chosen)
            );
            crate::standard_cards_v1::apply_aegis_copy(&mut state, &copy_ctx, host, 17, chosen);
            assert_ne!(state.objects.get(ctx.source).card_def, original.card_def);
            assert_eq!(
                state.objects.get(ctx.source).zone_change_count,
                original.zone_change_count
            );
            assert!(
                crate::engine::validate_historical_ability_source_contract(&state, original)
                    .is_ok()
            );
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                let life = branch.players[actor.index()].life;
                crate::effect::execute(
                    &EffectOp::ExileGraveyardTargetThenGrowSourceIfCreature,
                    &ctx,
                    branch,
                );
                assert!(branch.engine.halted.is_none());
                assert_eq!(branch.objects.get(card).zone, Zone::Exile);
                assert_eq!(branch.objects.get(ctx.source).counters.plus1_plus1, 1);
                assert_eq!(branch.players[actor.index()].life, life + 1);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }

    #[test]
    fn actual_typed_primitive_source_departure_preserves_gain_and_bad_authority_is_read_only() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (state, ctx, card) = physical(actor, actor.opponent(), "Llanowar Elves");
            for gone in [false, true] {
                let mut branch = restored(&state);
                if gone {
                    event::propose_and_commit(
                        &mut branch,
                        ProposedEvent::zone_change(ctx.source, Zone::Graveyard),
                    );
                } else {
                    branch.players[actor.index()]
                        .battlefield
                        .retain(|id| *id != ctx.source);
                    branch.players[actor.opponent().index()]
                        .battlefield
                        .push(ctx.source);
                    branch.objects.get_mut(ctx.source).controller = actor.opponent();
                }
                let before = branch.players[actor.index()].life;
                crate::effect::execute(
                    &EffectOp::ExileGraveyardTargetThenGrowSourceIfCreature,
                    &ctx,
                    &mut branch,
                );
                assert_eq!(branch.objects.get(card).zone, Zone::Exile);
                assert_eq!(branch.players[actor.index()].life, before + 1);
                assert_eq!(
                    branch.objects.get(ctx.source).counters.plus1_plus1,
                    i32::from(!gone)
                );
                assert_eq!(
                    branch.players[actor.opponent().index()].life,
                    state.players[actor.opponent().index()].life
                );
            }
            let mut forged = restored(&state);
            let mut invalid = ctx.clone();
            invalid.controller = actor.opponent();
            crate::effect::execute(
                &EffectOp::ExileGraveyardTargetThenGrowSourceIfCreature,
                &invalid,
                &mut forged,
            );
            assert!(forged.engine.halted.is_some());
            assert_eq!(forged.objects.get(card).zone, Zone::Graveyard);
            assert_eq!(forged.objects.get(ctx.source).counters.plus1_plus1, 0);
            assert_eq!(
                forged.players[actor.index()].life,
                state.players[actor.index()].life
            );
        }
    }
}
