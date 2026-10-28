use super::*;
use crate::card_def::{Keywords, ManaSpendRestrictionDef, Supertype};
use crate::mana::ManaColor;
use crate::{
    card_def::{self, CardCapability},
    engine::{self, Action, Decision},
    event::{self, ProposedEvent},
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    state::{GameState, Target, Zone},
};

fn ready(actor: PlayerId) -> GameState {
    let mut s = crate::extra_turn_v1::tests::ready(actor);
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut s).unwrap();
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
    let pooled_before: usize = s.players[actor.index()]
        .mana_pool
        .iter()
        .map(|n| usize::from(*n))
        .sum();
    let restricted_before = s.players[actor.index()].restricted_mana_pool.0.len();
    engine::step(s, Action::CastSpell(spell)).unwrap();
    if let Some(target) = target {
        assert!(
            matches!(engine::advance_until_decision(s),Decision::ChooseTargets{legal_targets,..}if legal_targets.contains(&Target::Object(target)))
        );
        engine::step(s, Action::ChooseTarget(Target::Object(target))).unwrap();
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
    let pooled_after: usize = s.players[actor.index()]
        .mana_pool
        .iter()
        .map(|n| usize::from(*n))
        .sum();
    let restricted_after = s.players[actor.index()].restricted_mana_pool.0.len();
    assert_eq!(
        after + pooled_before + restricted_before - before - pooled_after - restricted_after,
        usize::from(mv)
    );
    assert_eq!(s.objects.get(spell).zone, Zone::Stack);
    let permanent = card_def::CARD_DEFS[s.objects.get(spell).card_def as usize]
        .types
        .iter()
        .any(|t| {
            matches!(
                t,
                crate::card_def::CardType::Creature
                    | crate::card_def::CardType::Enchantment
                    | crate::card_def::CardType::Artifact
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

fn funded(actor: PlayerId) -> GameState {
    let mut state = ready(actor);
    for _ in 0..12 {
        put(&mut state, actor, "Plains", Zone::Battlefield);
    }
    for _ in 0..4 {
        put(&mut state, actor, "Forest", Zone::Battlefield);
    }
    for _ in 0..2 {
        put(&mut state, actor, "Island", Zone::Battlefield);
    }
    state
}
fn cast_named(state: &mut GameState, actor: PlayerId, name: &str) -> ObjectId {
    let card = put(state, actor, name, Zone::Hand);
    cast(state, actor, card, None);
    card
}
fn restore(state: &GameState) -> GameState {
    let copy: GameState = serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
    assert_eq!(state.state_hash(), copy.state_hash());
    copy
}
fn ready_mana_source(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let giada = put(state, actor, "Giada, Font of Hope", Zone::Battlefield);
    // Primitive start-of-turn setup: this source has survived summoning sickness.
    state.objects.get_mut(giada).summoning_sick = false;
    giada
}
fn assert_castability(state: &mut GameState, actor: PlayerId, angel: ObjectId, other: ObjectId) {
    priority(state, actor);
    let decision = engine::advance_until_decision(state);
    assert!(
        matches!(&decision, Decision::CastSpellOrPass { castable_spells, .. }
        if castable_spells.contains(&angel) && !castable_spells.contains(&other))
    );
    // The production RL action enumerator consumes the same real engine quote.
    // This is action legality, not admission of Giada's rules meaning/model.
    let candidates = crate::rl::legal_action_candidates_v1(
        &crate::surface_v2::SurfaceDecision::Decision(decision),
        state,
    )
    .unwrap();
    assert!(candidates
        .iter()
        .any(|candidate| matches!(&candidate.surface_action,
        crate::surface_v2::SurfaceAction::Action(Action::CastSpell(card)) if *card == angel)));
    assert!(!candidates
        .iter()
        .any(|candidate| matches!(&candidate.surface_action,
        crate::surface_v2::SurfaceAction::Action(Action::CastSpell(card)) if *card == other)));
}
#[test]
fn exact_partial_legendary_printing_and_paid_entries_count_only_prior_angels_both_seats() {
    let id = card_def::card_id_by_name("Giada, Font of Hope").unwrap();
    assert_eq!(id, 455);
    let definition = &card_def::CARD_DEFS[usize::from(id)];
    assert_eq!(definition.capability, CardCapability::Full);
    assert_eq!(definition.subtypes, [Subtype::Angel]);
    assert_eq!(definition.supertypes, [Supertype::Legendary]);
    assert_eq!(definition.keywords, Keywords::FLYING | Keywords::VIGILANCE);
    assert_eq!((definition.power, definition.toughness), (Some(2), Some(2)));
    assert!(definition.mana_ability_choices.is_empty());
    assert!(!definition.is_automatic_payment_mana_source());
    assert!(definition.has_mana_ability());
    assert_eq!(definition.restricted_mana_abilities.len(), 1);
    assert_eq!(
        definition.restricted_mana_abilities[0].restriction,
        ManaSpendRestrictionDef::SpellWithSubtype(Subtype::Angel)
    );
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = funded(actor);
        let giada = cast_named(&mut state, actor, "Giada, Font of Hope");
        assert_eq!(state.objects.get(giada).counters.plus1_plus1, 0);
        assert!(state.objects.get(giada).summoning_sick);
        assert!(engine::available_mana_ability_choices(actor, giada, &state).is_empty());
        let first = cast_named(&mut state, actor, "Youthful Valkyrie");
        assert_eq!(state.objects.get(first).counters.plus1_plus1, 1);
        put(&mut state, actor, "Garruk's Uprising", Zone::Battlefield);
        let second = cast_named(&mut state, actor, "Serra Angel");
        assert_eq!(state.objects.get(second).counters.plus1_plus1, 2);
        assert!(state
            .engine
            .event_history
            .iter()
            .any(|event| matches!(event,
            event::CommittedEvent::GarrukEntryObservationV1 { entry, .. }
            if entry.object == second && entry.snapshot.counters.plus1_plus1 == 2)));
        let copy = restore(&state);
        assert_eq!(copy.objects.get(second).counters.plus1_plus1, 2);
    }
}
#[test]
fn automatic_angel_only_mana_and_rl_cast_legality_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let giada = ready_mana_source(&mut state, actor);
        put(&mut state, actor, "Forest", Zone::Battlefield);
        let angel = put(&mut state, actor, "Youthful Valkyrie", Zone::Hand);
        let other = put(&mut state, actor, "Helpful Hunter", Zone::Hand);
        assert_eq!(
            engine::available_mana_ability_choices(actor, giada, &state),
            [ManaColor::W]
        );
        assert_castability(&mut state, actor, angel, other);
        cast(&mut state, actor, angel, None);
        assert!(state.objects.get(giada).tapped);
        assert_eq!(state.objects.get(angel).counters.plus1_plus1, 1);
        assert!(state.players[actor.index()].restricted_mana_pool.is_empty());
    }
}
#[test]
fn changeling_spell_can_spend_restricted_white_on_generic_and_gets_entry_counters() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let giada = ready_mana_source(&mut state, actor);
        put(&mut state, actor, "Forest", Zone::Battlefield);
        let changeling = put(&mut state, actor, "Masked Vandal", Zone::Hand);
        let other = put(&mut state, actor, "Helpful Hunter", Zone::Hand);
        assert_castability(&mut state, actor, changeling, other);
        cast(&mut state, actor, changeling, None);
        assert!(state.objects.get(giada).tapped);
        assert!(engine::has_effective_subtype(
            &state,
            changeling,
            Subtype::Angel
        ));
        assert_eq!(state.objects.get(changeling).counters.plus1_plus1, 1);
    }
}
#[test]
fn floating_mana_survives_source_departure_and_restore_without_losing_restriction() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for reenter in [false, true] {
            let mut state = ready(actor);
            let giada = ready_mana_source(&mut state, actor);
            put(&mut state, actor, "Forest", Zone::Battlefield);
            let angel = put(&mut state, actor, "Youthful Valkyrie", Zone::Hand);
            let other = put(&mut state, actor, "Helpful Hunter", Zone::Hand);
            priority(&mut state, actor);
            engine::step(&mut state, Action::ActivateManaAbility(giada)).unwrap();
            assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
            let unit = state.players[actor.index()].restricted_mana_pool.0[0].clone();
            assert_eq!(
                unit.restriction,
                ManaSpendRestrictionDef::SpellWithSubtype(Subtype::Angel)
            );
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(giada, Zone::Exile));
            assert!(state.objects.get(giada).zone_change_count > unit.source.zone_change_count);
            if reenter {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(giada, Zone::Battlefield),
                );
                assert!(state.objects.get(giada).zone_change_count > unit.source.zone_change_count);
                assert!(engine::available_mana_ability_choices(actor, giada, &state).is_empty());
                assert_eq!(state.players[actor.index()].restricted_mana_pool.0[0], unit);
            }
            let mut copy = restore(&state);
            for branch in [&mut state, &mut copy] {
                assert_castability(branch, actor, angel, other);
                cast(branch, actor, angel, None);
                assert!(branch.players[actor.index()]
                    .restricted_mana_pool
                    .is_empty());
                assert_eq!(
                    branch.objects.get(angel).counters.plus1_plus1,
                    if reenter { 1 } else { 0 }
                );
            }
            assert_eq!(state.state_hash(), copy.state_hash());
        }
    }
}
#[test]
fn angel_only_mana_cannot_pay_noncreature_spells_or_activated_abilities() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let giada = ready_mana_source(&mut state, actor);
        put(&mut state, actor, "Forest", Zone::Battlefield);
        let map = put(&mut state, actor, "Expedition Map", Zone::Battlefield);
        let helpful = put(&mut state, actor, "Helpful Hunter", Zone::Hand);
        let banishing = put(&mut state, actor, "Banishing Light", Zone::Hand);
        priority(&mut state, actor);
        engine::step(&mut state, Action::ActivateManaAbility(giada)).unwrap();
        let decision = engine::advance_until_decision(&mut state);
        assert!(
            matches!(decision, Decision::CastSpellOrPass { castable_spells, activatable_abilities, .. }
            if !castable_spells.contains(&helpful) && !castable_spells.contains(&banishing)
                && !activatable_abilities.contains(&(map, 0)))
        );
        assert!(crate::mana::plan_activation_mana_v1(
            &crate::mana::Cost {
                generic: 2,
                pips: &[],
                x_count: 0
            },
            0,
            actor,
            &state,
            map,
            &[]
        )
        .is_none());
        // A second generic source would suffice for ordinary generic payment,
        // while the sole white unit remains unavailable to Banishing Light.
        put(&mut state, actor, "Forest", Zone::Battlefield);
        let decision = engine::advance_until_decision(&mut state);
        assert!(
            matches!(decision, Decision::CastSpellOrPass { castable_spells, activatable_abilities, .. }
            if !castable_spells.contains(&banishing) && activatable_abilities.contains(&(map, 0)))
        );
        assert_eq!(state.players[actor.index()].restricted_mana_pool.0.len(), 1);
    }
}
#[test]
fn simultaneous_entry_primitive_uses_sources_and_angel_counts_before_entire_batch() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for existing in [false, true] {
            let mut state = ready(actor);
            if existing {
                put(&mut state, actor, "Giada, Font of Hope", Zone::Battlefield);
            }
            let entering_giada = put(&mut state, actor, "Giada, Font of Hope", Zone::Graveyard);
            let first = put(&mut state, actor, "Serra Angel", Zone::Graveyard);
            let second = put(&mut state, actor, "Youthful Valkyrie", Zone::Graveyard);
            // Real shared simultaneous zone-entry primitive, observed before
            // the postresolution legend SBA. No fictional spell is registered.
            event::propose_and_commit_batch(
                &mut state,
                vec![
                    ProposedEvent::zone_change(entering_giada, Zone::Battlefield),
                    ProposedEvent::zone_change(first, Zone::Battlefield),
                    ProposedEvent::zone_change(second, Zone::Battlefield),
                ],
            );
            assert!(state.engine.halted.is_none());
            for object in [entering_giada, first, second] {
                assert_eq!(
                    state.objects.get(object).counters.plus1_plus1,
                    if existing { 1 } else { 0 }
                );
            }
            restore(&state);
        }
    }
}
#[test]
fn multiplicity_effective_changeling_counts_and_season_share_original_entry_input() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for season in [false, true] {
            let mut state = ready(actor);
            // Multiple sources can coexist within a resolving instruction
            // before legend SBAs. This tests the real replacement primitive.
            put(&mut state, actor, "Giada, Font of Hope", Zone::Battlefield);
            put(&mut state, actor, "Giada, Font of Hope", Zone::Battlefield);
            put(&mut state, actor, "Masked Vandal", Zone::Battlefield);
            put(
                &mut state,
                actor.opponent(),
                "Serra Angel",
                Zone::Battlefield,
            );
            if season {
                put(&mut state, actor, "Doubling Season", Zone::Battlefield);
            }
            let angel = put(&mut state, actor, "Serra Angel", Zone::Graveyard);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(angel, Zone::Battlefield),
            );
            assert!(state.engine.halted.is_none());
            assert_eq!(
                state.objects.get(angel).counters.plus1_plus1,
                if season { 12 } else { 6 }
            );
            restore(&state);
        }
    }
}
#[test]
fn paid_ability_removal_and_current_effective_subtypes_control_entry_replacement() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = funded(actor);
        let giada = cast_named(&mut state, actor, "Giada, Font of Hope");
        let old_angel = cast_named(&mut state, actor, "Youthful Valkyrie");
        let aura = put(&mut state, actor, "Witness Protection", Zone::Hand);
        cast(&mut state, actor, aura, Some(old_angel));
        assert!(!engine::has_effective_subtype(
            &state,
            old_angel,
            Subtype::Angel
        ));
        let angel = cast_named(&mut state, actor, "Serra Angel");
        assert_eq!(state.objects.get(angel).counters.plus1_plus1, 1);
        let aura = put(&mut state, actor, "Witness Protection", Zone::Hand);
        cast(&mut state, actor, aura, Some(giada));
        assert!(engine::available_mana_ability_choices(actor, giada, &state).is_empty());
        // A final entry uses the current board after Giada lost its abilities.
        let final_angel = put(&mut state, actor, "Serra Angel", Zone::Graveyard);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(final_angel, Zone::Battlefield),
        );
        assert_eq!(state.objects.get(final_angel).counters.plus1_plus1, 0);
        restore(&state);
    }
}
#[test]
fn controlled_entry_uses_recipient_controller_and_adds_to_other_actual_entry_counters() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        put(&mut state, actor, "Giada, Font of Hope", Zone::Battlefield);
        put(&mut state, actor, "Doubling Season", Zone::Battlefield);
        let foreign = put(&mut state, actor.opponent(), "Serra Angel", Zone::Graveyard);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change_to_battlefield_under_controller(foreign, actor),
        );
        assert_eq!(state.objects.get(foreign).owner, actor.opponent());
        assert_eq!(state.objects.get(foreign).controller, actor);
        assert_eq!(state.objects.get(foreign).counters.plus1_plus1, 2);
        let hydra = put(&mut state, actor, "Wildwood Scourge", Zone::Graveyard);
        let mut proposal = ProposedEvent::zone_change(hydra, Zone::Battlefield);
        if let ProposedEvent::ZoneChange(change) = &mut proposal {
            // Labeled shared entry primitive: an added Angel characteristic
            // and three original entry counters, not a printed Hydra ability.
            change.battlefield_added_subtype_v1 = Some(Subtype::Angel);
            change.battlefield_x_v48 = Some(crate::doubling_season_v48::EffectXEntryV48 {
                original: 3,
                host: None,
            });
        }
        event::propose_and_commit(&mut state, proposal);
        // Three base counters plus two prior Angels, all doubled once.
        assert_eq!(state.objects.get(hydra).counters.plus1_plus1, 10);
        restore(&state);
    }
}

#[test]
fn convoke_payment_primitive_distinguishes_tapping_a_creature_from_spending_restricted_mana() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let giada = ready_mana_source(&mut state, actor);
        let forest = put(&mut state, actor, "Forest", Zone::Battlefield);
        // Ordinary white-creature Convoke is permitted even for a non-Angel
        // spell. It is not Giada's mana ability and creates no mana.
        let (plan, convoked) = crate::mana::plan_spell_mana_total_with_convoke_v1(
            &[crate::mana::Pip::Colored(ManaColor::W)],
            1,
            actor,
            &state,
            true,
            &[],
            0,
        )
        .unwrap();
        assert_eq!(convoked, [giada]);
        assert_eq!(plan.taps, [(forest, ManaColor::G)]);
        assert!(crate::mana::plan_spell_mana_total_v1(
            &[crate::mana::Pip::Colored(ManaColor::W)],
            1,
            actor,
            &state,
            true,
            &[],
            0
        )
        .is_none());
        priority(&mut state, actor);
        engine::step(&mut state, Action::ActivateManaAbility(giada)).unwrap();
        // A separate summoning-sick creature may Convoke but cannot produce
        // mana. The only white mana is the real saved Angel-only unit.
        let elf = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let angel = &card_def::CARD_DEFS
            [usize::from(card_def::card_id_by_name("Youthful Valkyrie").unwrap())];
        let subtypes: Vec<_> = angel
            .subtypes
            .iter()
            .map(|subtype| subtype.stable_id())
            .collect();
        let (plan, convoked) = crate::mana::plan_spell_mana_total_with_convoke_subtypes_v1(
            angel.cost.pips,
            1,
            actor,
            &state,
            true,
            &[forest],
            0,
            false,
            &subtypes,
        )
        .unwrap();
        assert_eq!(convoked, [elf]);
        assert_eq!(plan.restricted_pool_used, [0]);
        assert!(crate::mana::plan_spell_mana_total_with_convoke_subtypes_v1(
            angel.cost.pips,
            1,
            actor,
            &state,
            true,
            &[forest],
            0,
            false,
            &[]
        )
        .is_none());
    }
}
