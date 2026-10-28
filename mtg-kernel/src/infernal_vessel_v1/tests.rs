use super::*;
use crate::{
    card_def::{self, CardCapability},
    effect::{self, ExecCtx},
    engine::{self, Action, Decision},
    event::{self, ProposedEvent},
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    state::{GameState, Target, Zone},
};

fn ready(actor: PlayerId) -> GameState {
    let mut s = crate::extra_turn_v1::tests::ready(actor);
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut s).unwrap();
    for p in [actor, actor.opponent()] {
        for _ in 0..3 {
            put(&mut s, p, "Swamp", Zone::Battlefield);
            put(&mut s, p, "Mountain", Zone::Battlefield);
        }
        for _ in 0..10 {
            put(&mut s, p, "Forest", Zone::Battlefield);
        }
    }
    s
}
fn quiet(s: &mut GameState, d: Decision) {
    let action = match d {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
        Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
        Decision::ChooseEntryColorV36 { colors, .. } => {
            assert_eq!(colors.len(), 5);
            Action::ChooseEffectOption(0)
        }
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        other => panic!("unexpected actual native choice:{other:?}"),
    };
    engine::step(s, action).unwrap();
    assert!(s.engine.halted.is_none());
}
fn settle(s: &mut GameState) {
    for _ in 0..80 {
        let d = engine::advance_until_decision(s);
        if s.stack.is_empty()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_cast.is_none()
            && s.engine.pending_triggers.is_empty()
            && !crate::linked_aura_operation_v1::has_pending(s)
        {
            assert!(matches!(d, Decision::CastSpellOrPass { .. }));
            return;
        }
        quiet(s, d);
    }
    panic!("real resolution failed to settle")
}
fn priority(s: &mut GameState, actor: PlayerId) {
    for _ in 0..8 {
        let d = engine::advance_until_decision(s);
        if matches!(d,Decision::CastSpellOrPass{player,..}if player==actor) {
            return;
        }
        quiet(s, d);
    }
    panic!("original caster lacks priority")
}
fn cast(s: &mut GameState, actor: PlayerId, spell: ObjectId, target: Option<ObjectId>) {
    priority(s, actor);
    let d = engine::advance_until_decision(s);
    assert!(
        matches!(d,Decision::CastSpellOrPass{castable_spells,..}if castable_spells.contains(&spell))
    );
    let before = s.players[actor.index()]
        .battlefield
        .iter()
        .filter(|id| s.objects.get(**id).tapped)
        .count();
    let mv = card_def::CARD_DEFS[s.objects.get(spell).card_def as usize].mana_value;
    engine::step(s, Action::CastSpell(spell)).unwrap();
    if let Some(target) = target {
        assert!(
            matches!(engine::advance_until_decision(s),Decision::ChooseTargets{legal_targets,..}if legal_targets.contains(&Target::Object(target)))
        );
        engine::step(s, Action::ChooseTarget(Target::Object(target))).unwrap();
        if card_def::CARD_DEFS[s.objects.get(spell).card_def as usize].target_spec
            == card_def::TargetSpec::CreatureThenUpToOneAttachedEquipment
        {
            // Fiery's creature is mandatory, but its attached Equipment target
            // is optional. Finish that actual target stage before paying.
            assert!(matches!(engine::advance_until_decision(s),
                Decision::ChooseTargets { player, spell: announced, remaining: 1, can_finish: true, .. }
                    if player == actor && announced == spell));
            assert_eq!(
                s.engine.pending_cast.as_ref().unwrap().targets_chosen,
                vec![Target::Object(target)]
            );
            engine::step(s, Action::FinishEffectSelection).unwrap();
        }
    }
    // Native Step announces the cast/target; Advance finalizes the pending
    // cast, pays its actual quote and publishes the original spell before
    // returning priority to its caster. Assert payment only at that boundary.
    assert!(matches!(engine::advance_until_decision(s),
        Decision::CastSpellOrPass { player, .. } if player == actor));
    assert!(s.engine.pending_cast.is_none());
    assert!(s.engine.halted.is_none());
    let after = s.players[actor.index()]
        .battlefield
        .iter()
        .filter(|id| s.objects.get(**id).tapped)
        .count();
    assert_eq!(after - before, usize::from(mv));
    assert_eq!(s.players[actor.index()].mana_pool, [0; 6]);
    assert_eq!(s.objects.get(spell).zone, Zone::Stack);
    let permanent = card_def::CARD_DEFS[s.objects.get(spell).card_def as usize]
        .types
        .iter()
        .any(|t| {
            matches!(
                t,
                crate::card_def::CardType::Creature | crate::card_def::CardType::Enchantment
            )
        });
    settle(s);
    assert_eq!(
        s.objects.get(spell).zone,
        if permanent {
            Zone::Battlefield
        } else {
            Zone::Graveyard
        }
    );
}

fn paid_vessel(actor: PlayerId) -> (GameState, ObjectId) {
    let mut state = ready(actor);
    let vessel = put(&mut state, actor, "Infernal Vessel", Zone::Hand);
    cast(&mut state, actor, vessel, None);
    (state, vessel)
}
fn depart(state: &mut GameState, vessel: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(vessel, Zone::Graveyard));
}
fn restored(state: &GameState) -> GameState {
    let copy: GameState = serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
    assert_eq!(state.state_hash(), copy.state_hash());
    copy
}
fn assert_return(state: &GameState, vessel: ObjectId, owner: PlayerId, counters: i32) {
    let object = state.objects.get(vessel);
    assert_eq!(object.zone, Zone::Battlefield);
    assert_eq!(object.controller, owner);
    assert_eq!(object.owner, owner);
    assert_eq!(object.counters.plus1_plus1, counters);
    assert!(!object.tapped);
    for subtype in [Subtype::Human, Subtype::Cleric, Subtype::Demon] {
        assert!(engine::has_effective_subtype(state, vessel, subtype));
    }
}
#[test]
fn exact_partial_printing_paid_cast_and_lethal_damage_return_both_seats() {
    let id = card_def::card_id_by_name("Infernal Vessel").unwrap();
    assert_eq!(id, 454);
    let definition = &card_def::CARD_DEFS[usize::from(id)];
    assert_eq!(definition.capability, CardCapability::Full);
    assert_eq!(definition.subtypes, [Subtype::Human, Subtype::Cleric]);
    assert_eq!((definition.power, definition.toughness), (Some(2), Some(1)));
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, vessel) = paid_vessel(actor);
        let before = state.objects.get(vessel).zone_change_count;
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        cast(&mut state, actor, bolt, Some(vessel));
        assert_return(&state, vessel, actor, 2);
        assert_eq!(state.objects.get(vessel).zone_change_count, before + 2);
        assert_eq!(engine::effective_power(&state, vessel), 4);
        assert_eq!(engine::effective_toughness(&state, vessel), 3);
        // The returned incarnation was a Demon. It does not return again.
        depart(&mut state, vessel);
        settle(&mut state);
        assert_eq!(state.objects.get(vessel).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(vessel).counters.plus1_plus1, 0);
        assert!(!engine::has_effective_subtype(
            &state,
            vessel,
            Subtype::Demon
        ));
    }
}
#[test]
fn departed_effective_demon_prevents_trigger_even_after_zone_reset_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, vessel) = paid_vessel(actor);
        // A genuine prior continuous addition, absent from the printed definition.
        let timestamp = engine::next_timestamp(&mut state);
        state.objects.get_mut(vessel).v4.added_subtype_v1 = Some((Subtype::Demon, timestamp));
        depart(&mut state, vessel);
        assert!(state.objects.get(vessel).v4.added_subtype_v1.is_none());
        settle(&mut state);
        assert_eq!(state.objects.get(vessel).zone, Zone::Graveyard);
        assert!(!state.engine.event_history.iter().any(|event| matches!(event,
            event::CommittedEvent::ControlledCreatureDeathCaptureV1 { source, .. } if source.source == vessel)));
    }
}
#[test]
fn pending_death_restore_returns_to_owner_despite_stolen_controller_both_seats() {
    for owner in [PlayerId::P0, PlayerId::P1] {
        let (mut state, vessel) = paid_vessel(owner);
        // The departed controller's Season must not double its owner's entry counters.
        put(
            &mut state,
            owner.opponent(),
            "Doubling Season",
            Zone::Battlefield,
        );
        // Existing registered control primitive supplies a coherent stolen controller.
        let control_source = put(&mut state, owner.opponent(), "Forest", Zone::Hand);
        let mut ctx = ExecCtx::no_targets(control_source, owner.opponent());
        ctx.ability_source_contract = Some(crate::state::AbilitySourceContractV4::capture(
            &state,
            control_source,
        ));
        ctx.targets = vec![Target::Object(vessel)];
        ctx.target_contracts = vec![crate::state::StackTargetContractV4::capture(
            &state,
            Target::Object(vessel),
        )];
        effect::execute(
            &crate::effect::EffectOp::GainControlOfTargetCreatureUntilEndOfTurn { target_index: 0 },
            &ctx,
            &mut state,
        );
        assert_eq!(state.objects.get(vessel).controller, owner.opponent());
        depart(&mut state, vessel);
        assert!(state.engine.event_history.iter().any(|event| matches!(event,
            event::CommittedEvent::ControlledCreatureDeathCaptureV1 { source, .. }
            if source.source == vessel && source.controller == owner.opponent() && source.owner == owner)));
        let mut copy = restored(&state);
        settle(&mut state);
        settle(&mut copy);
        assert_return(&state, vessel, owner, 2);
        assert_return(&copy, vessel, owner, 2);
        assert_eq!(state.state_hash(), copy.state_hash());
        let mut copy = restored(&copy);
        depart(&mut copy, vessel);
        settle(&mut copy);
        assert_eq!(copy.objects.get(vessel).zone, Zone::Graveyard);
    }
}
#[test]
fn leaving_graveyard_or_reentering_it_fizzles_original_trigger_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for return_to_graveyard in [false, true] {
            let (mut state, vessel) = paid_vessel(actor);
            depart(&mut state, vessel);
            // Publish the actual captured ability before the response changes zones.
            let _ = engine::advance_until_decision(&mut state);
            assert!(state
                .stack
                .iter()
                .any(|item| item.source == vessel && item.inline_effect == Some(recipe())));
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(vessel, Zone::Exile));
            if return_to_graveyard {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(vessel, Zone::Graveyard),
                );
            }
            let mut copy = restored(&state);
            for branch in [&mut state, &mut copy] {
                settle(branch);
                assert_eq!(
                    branch.objects.get(vessel).zone,
                    if return_to_graveyard {
                        Zone::Graveyard
                    } else {
                        Zone::Exile
                    }
                );
                assert_eq!(branch.objects.get(vessel).counters.plus1_plus1, 0);
                assert!(branch.objects.get(vessel).v4.added_subtype_v1.is_none());
            }
            assert_eq!(state.state_hash(), copy.state_hash());
        }
    }
}
#[test]
fn blink_is_new_nondemon_incarnation_and_entry_counters_use_owner_replacements() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, vessel) = paid_vessel(actor);
        put(&mut state, actor, "Doubling Season", Zone::Battlefield);
        depart(&mut state, vessel);
        settle(&mut state);
        assert_return(&state, vessel, actor, 4);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(vessel, Zone::Exile));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(vessel, Zone::Battlefield),
        );
        assert!(!engine::has_effective_subtype(
            &state,
            vessel,
            Subtype::Demon
        ));
        assert_eq!(state.objects.get(vessel).counters.plus1_plus1, 0);
        depart(&mut state, vessel);
        settle(&mut state);
        assert_return(&state, vessel, actor, 4);
    }
}
#[test]
fn paid_fiery_replaced_death_never_creates_vessel_trigger() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, vessel) = paid_vessel(actor);
        let fiery = put(&mut state, actor, "Fiery Annihilation", Zone::Hand);
        cast(&mut state, actor, fiery, Some(vessel));
        assert_eq!(state.objects.get(vessel).zone, Zone::Exile);
        assert!(!state.engine.event_history.iter().any(|event| matches!(event,
            event::CommittedEvent::ControlledCreatureDeathCaptureV1 { source, .. } if source.source == vessel)));
    }
}

#[test]
fn paid_ability_removal_prevents_death_trigger_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, vessel) = paid_vessel(actor);
        put(&mut state, actor, "Island", Zone::Battlefield);
        let aura = put(&mut state, actor, "Witness Protection", Zone::Hand);
        cast(&mut state, actor, aura, Some(vessel));
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        cast(&mut state, actor, bolt, Some(vessel));
        assert_eq!(state.objects.get(vessel).zone, Zone::Graveyard);
        assert!(!state.engine.event_history.iter().any(|event| matches!(event,
            event::CommittedEvent::ControlledCreatureDeathCaptureV1 { source, .. } if source.source == vessel)));
    }
}

#[test]
fn returned_incarnation_is_demon_in_production_entry_snapshot_with_counters_and_statics() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for season in [false, true] {
            let (mut state, vessel) = paid_vessel(actor);
            put(&mut state, actor, "Garruk's Uprising", Zone::Battlefield);
            put(
                &mut state,
                actor.opponent(),
                "Authority of the Consuls",
                Zone::Battlefield,
            );
            if season {
                put(&mut state, actor, "Doubling Season", Zone::Battlefield);
            }
            let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
            let hand_before = state.players[actor.index()].hand.len();
            let opponent_life = state.players[actor.opponent().index()].life;
            let departed_generation = state.objects.get(vessel).zone_change_count;
            cast(&mut state, actor, bolt, Some(vessel));
            let count = if season { 4 } else { 2 };
            let entries: Vec<_> = state
                .engine
                .event_history
                .iter()
                .filter_map(|event| match event {
                    event::CommittedEvent::GarrukEntryObservationV1 {
                        entry,
                        effective_creature,
                        effective_power,
                        ..
                    } if entry.object == vessel
                        && entry.snapshot.zone_change_count == departed_generation + 2 =>
                    {
                        Some((entry, *effective_creature, *effective_power))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(entries.len(), 1);
            let (entry, creature, power) = entries[0];
            // This is the existing production entry-observer snapshot. It is
            // captured inside the return commit, before the effect returns.
            // Garruk is power-based; this does not invent a Demon entry trigger.
            assert_eq!(
                entry.snapshot.v4.added_subtype_v1.map(|value| value.0),
                Some(Subtype::Demon)
            );
            for subtype in [Subtype::Human, Subtype::Cleric] {
                assert!(subtype.is_in_subtype_ids(&entry.snapshot.v4.effective_subtype_ids));
            }
            assert_eq!(entry.snapshot.counters.plus1_plus1, count);
            assert!(entry.snapshot.tapped);
            assert_eq!(entry.snapshot.controller, actor);
            assert!(creature);
            assert_eq!(power, Some(2 + count));
            assert!(engine::has_effective_subtype(
                &state,
                vessel,
                Subtype::Demon
            ));
            // Paid Bolt consumes one card; the genuine qualifying entry draws
            // one. Authority still captures its opposing creature entry.
            assert_eq!(state.players[actor.index()].hand.len(), hand_before);
            assert_eq!(
                state.players[actor.opponent().index()].life,
                opponent_life + 1
            );
            let copy = restored(&state);
            assert!(engine::has_effective_subtype(&copy, vessel, Subtype::Demon));
        }
    }
}

#[test]
fn nonbattlefield_destination_does_not_install_entry_subtype_input() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, vessel) = paid_vessel(actor);
        depart(&mut state, vessel);
        let _ = engine::advance_until_decision(&mut state);
        // The shared production commit primitive receives a final destination
        // other than Battlefield. This is an explicitly labeled primitive,
        // not a claim that the current FDN pool has an exile-on-return rule.
        let mut move_away = ProposedEvent::zone_change(vessel, Zone::Exile);
        if let ProposedEvent::ZoneChange(change) = &mut move_away {
            change.battlefield_added_subtype_v1 = Some(Subtype::Demon);
        }
        event::propose_and_commit(&mut state, move_away);
        assert_eq!(state.objects.get(vessel).zone, Zone::Exile);
        assert!(state.objects.get(vessel).v4.added_subtype_v1.is_none());
        let mut copy = restored(&state);
        for branch in [&mut state, &mut copy] {
            settle(branch);
            assert_eq!(branch.objects.get(vessel).zone, Zone::Exile);
            assert!(branch.objects.get(vessel).v4.added_subtype_v1.is_none());
            assert_eq!(branch.objects.get(vessel).counters.plus1_plus1, 0);
        }
    }
}
