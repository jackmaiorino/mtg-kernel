//! Genuine FDN119 Partial acceptance source. All casts spend seeded fixture mana.
//! Rust compilation and runtime qualification are pending with the integrating owner.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, CardCapability, CardType, Keywords,
    SourceTypeHexproofDefV1, Subtype, Supertype, CARD_DEFS,
};
use mtg_kernel::combat_damage_v1::enable_foundations_combat_v1;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::CommittedEvent;
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{CastMethodV4, GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::surface_v2::{
    HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode, SurfaceAction, SurfaceDecision,
};

const ELENDA: &str = "Elenda, Saint of Dusk";

fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def =
        card_id_by_name(name).unwrap_or_else(|| panic!("missing genuine printing {name}"));
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
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
        Zone::Hand => state.players[owner.index()].hand.push(id),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
        _ => panic!("fixture zone"),
    }
    id
}
fn ready(actor: PlayerId, life: i32) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 60],
        &[forest; 60],
        |_| "Forest".into(),
        110119,
        actor,
    );
    // Initial setup only. Subsequent turn/control/life changes use actual play.
    state.step = Step::Main1;
    state.players[actor.index()].life = life;
    enable_foundations_combat_v1(&mut state).unwrap();
    state
}
fn surface() -> HarnessSurfaceV2 {
    HarnessSurfaceV2::new_with_priority_mode_v1(
        PriorityModeV1::EngineWindowsV1,
        SuppressionAuditMode::Full,
    )
}
fn next(surface: &mut HarnessSurfaceV2, state: &mut GameState) -> Decision {
    match surface.next_decision(state) {
        SurfaceDecision::Decision(d)
            if !matches!(d, Decision::Halted { .. } | Decision::GameOver { .. }) =>
        {
            d
        }
        d => panic!("unexpected decision {d:?}"),
    }
}
fn apply(surface: &mut HarnessSurfaceV2, state: &mut GameState, action: Action) {
    surface.apply(state, SurfaceAction::Action(action)).unwrap();
}
fn priority(surface: &mut HarnessSurfaceV2, state: &mut GameState, actor: PlayerId) {
    for _ in 0..8 {
        match next(surface, state) {
            Decision::CastSpellOrPass { player, .. } if player == actor => return,
            Decision::CastSpellOrPass { .. } => apply(surface, state, Action::Pass),
            d => panic!("expected priority, got {d:?}"),
        }
    }
    panic!("priority not reached");
}
fn replay(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let copy: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(bytes, serde_json::to_vec(&copy).unwrap());
    assert_eq!(state.state_hash(), copy.state_hash());
    copy
}
fn finish_to(surface: &mut HarnessSurfaceV2, state: &mut GameState, remaining: usize) {
    for _ in 0..64 {
        let d = next(surface, state);
        if state.stack.len() == remaining && state.engine.pending_triggers.is_empty() {
            return;
        }
        assert!(state.stack.len() >= remaining);
        match d {
            Decision::CastSpellOrPass { .. } => apply(surface, state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => apply(
                surface,
                state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            d => panic!("unexpected resolution {d:?}"),
        }
    }
    panic!("resolution did not settle");
}
fn announce(
    surface: &mut HarnessSurfaceV2,
    state: &mut GameState,
    actor: PlayerId,
    name: &str,
    target: Option<Target>,
) -> ObjectId {
    let spell = put(state, actor, name, Zone::Hand);
    state.players[actor.index()].mana_pool = [10; 6];
    priority(surface, state, actor);
    assert!(
        matches!(next(surface, state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell))
    );
    let before: u32 = state.players[actor.index()]
        .mana_pool
        .iter()
        .map(|&x| u32::from(x))
        .sum();
    apply(surface, state, Action::CastSpell(spell));
    if let Some(target) = target {
        assert!(
            matches!(next(surface, state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&target))
        );
        apply(surface, state, Action::ChooseTarget(target));
    }
    assert!(matches!(
        next(surface, state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(state
        .stack
        .iter()
        .any(|item| item.source == spell && item.v4.cast_method == Some(CastMethodV4::Normal)));
    let after: u32 = state.players[actor.index()]
        .mana_pool
        .iter()
        .map(|&x| u32::from(x))
        .sum();
    assert_eq!(
        before - after,
        u32::from(CARD_DEFS[state.objects.get(spell).card_def as usize].mana_value)
    );
    spell
}
fn cast_elenda(surface: &mut HarnessSurfaceV2, state: &mut GameState, actor: PlayerId) -> ObjectId {
    let id = announce(surface, state, actor, ELENDA, None);
    // Restore a real finalized, paid stack item before any player passes.
    *state = replay(state);
    finish_to(surface, state, 0);
    assert_eq!(state.objects.get(id).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(id).owner, actor);
    assert_eq!(state.objects.get(id).controller, actor);
    assert!(state.objects.get(id).summoning_sick);
    id
}
fn stats(state: &GameState, id: ObjectId, pt: i32, menace: bool) {
    assert_eq!(
        (
            engine::effective_power(state, id),
            engine::effective_toughness(state, id)
        ),
        (pt, pt)
    );
    assert_eq!(
        engine::has_effective_keyword(state, id, Keywords::MENACE),
        menace
    );
    assert!(engine::has_effective_keyword(state, id, Keywords::LIFELINK));
    let facts = mtg_kernel::starting_life_statics_v1::characteristics(state, id).unwrap();
    assert_eq!(facts.starting_life, 20);
    assert_eq!(
        facts.controller_life,
        state.players[facts.controller.index()].life
    );
    assert!(facts.printed_abilities_active);
    assert_eq!(
        mtg_kernel::source_type_hexproof_v1::characteristics(state, id)
            .unwrap()
            .rule,
        SourceTypeHexproofDefV1::Instants
    );
}

#[test]
fn genuine_definition_is_partial_and_has_exact_printed_cost_and_typed_rules() {
    let id = card_id_by_name(ELENDA).unwrap();
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.capability, CardCapability::Partial);
    assert!(preflight_fully_supported_deck(&[id]).is_err());
    assert_eq!(def.mana_value, 4);
    assert_eq!(def.cost.generic, 2);
    assert_eq!(
        def.cost.pips,
        &[Pip::Colored(ManaColor::W), Pip::Colored(ManaColor::B)]
    );
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Vampire, Subtype::Knight]);
    assert_eq!(def.supertypes, &[Supertype::Legendary]);
    assert!(def.keywords.has(Keywords::LIFELINK));
    assert!(!def.keywords.has(Keywords::HEXPROOF));
    assert_eq!(
        def.source_type_hexproof_v1,
        Some(SourceTypeHexproofDefV1::Instants)
    );
    assert_eq!(def.starting_life_self_bonuses_v1.len(), 2);
}

#[test]
fn genuine_paid_casts_restore_at_all_thresholds_for_both_seats_and_spend_exact_colors() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (life, pt, menace) in [(20, 4, false), (21, 5, true), (29, 5, true), (30, 10, true)] {
            let mut state = ready(actor, life);
            let mut surface = surface();
            let spell = put(&mut state, actor, ELENDA, Zone::Hand);
            for (pool, legal) in [
                ([0, 0, 1, 0, 0, 3], false),
                ([1, 0, 0, 0, 0, 3], false),
                ([1, 0, 1, 0, 0, 2], true),
            ] {
                state.players[actor.index()].mana_pool = pool;
                assert!(
                    matches!(next(&mut surface, &mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell) == legal)
                );
            }
            apply(&mut surface, &mut state, Action::CastSpell(spell));
            next(&mut surface, &mut state);
            assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
            assert!(state.stack.iter().any(|item| item.source == spell));
            let mut restored = replay(&state);
            finish_to(&mut surface, &mut state, 0);
            finish_to(&mut self::surface(), &mut restored, 0);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
            stats(&state, spell, pt, menace);
        }
    }
}

#[test]
fn genuine_paid_self_bolts_recompute_downward_thresholds() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, 30);
        let mut surface = surface();
        let elenda = cast_elenda(&mut surface, &mut state, actor);
        for (life, pt, menace) in [(27, 5, true), (24, 5, true), (21, 5, true), (18, 4, false)] {
            announce(
                &mut surface,
                &mut state,
                actor,
                "Lightning Bolt",
                Some(Target::Player(actor)),
            );
            state = replay(&state);
            finish_to(&mut surface, &mut state, 0);
            assert_eq!(state.players[actor.index()].life, life);
            stats(&state, elenda, pt, menace);
        }
    }
}

#[test]
fn own_instant_and_opponent_sorcery_target_but_opponent_instant_refuses_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (opponent, name, legal) in [
            (false, "Lightning Bolt", true),
            (true, "Lightning Bolt", false),
            (true, "Luminous Rebuke", true),
        ] {
            let mut state = ready(actor, 20);
            let mut surface = surface();
            let elenda = cast_elenda(&mut surface, &mut state, actor);
            let caster = if opponent { actor.opponent() } else { actor };
            if opponent {
                crate::fixture_turn_v1::reach(&mut state, caster, Step::Main1);
            }
            let spell = put(&mut state, caster, name, Zone::Hand);
            state.players[caster.index()].mana_pool = [10; 6];
            priority(&mut surface, &mut state, caster);
            apply(&mut surface, &mut state, Action::CastSpell(spell));
            assert!(
                matches!(next(&mut surface, &mut state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(elenda)) == legal)
            );
            if !legal {
                let bytes = serde_json::to_vec(&state).unwrap();
                assert!(
                    engine::step(&mut state, Action::ChooseTarget(Target::Object(elenda))).is_err()
                );
                assert_eq!(bytes, serde_json::to_vec(&state).unwrap());
                apply(
                    &mut surface,
                    &mut state,
                    Action::ChooseTarget(Target::Player(actor)),
                );
            } else {
                apply(
                    &mut surface,
                    &mut state,
                    Action::ChooseTarget(Target::Object(elenda)),
                );
            }
            next(&mut surface, &mut state);
            state = replay(&state);
            finish_to(&mut surface, &mut state, 0);
            assert_eq!(
                state.objects.get(elenda).zone,
                if opponent && legal {
                    Zone::Graveyard
                } else {
                    Zone::Battlefield
                }
            );
            if !opponent {
                assert_eq!(state.objects.get(elenda).damage, 3);
            }
        }
    }
}

#[test]
fn restored_paid_bolt_revalidates_after_real_flashed_control_and_ability_changes() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for suppress in [false, true] {
            let mut state = ready(actor, 30);
            let mut surface = surface();
            // Genuine registered static source seeded on the battlefield.
            put(
                &mut state,
                actor.opponent(),
                "High Fae Trickster",
                Zone::Battlefield,
            );
            let elenda = cast_elenda(&mut surface, &mut state, actor);
            announce(
                &mut surface,
                &mut state,
                actor,
                "Lightning Bolt",
                Some(Target::Object(elenda)),
            );
            state = replay(&state);
            announce(
                &mut surface,
                &mut state,
                actor.opponent(),
                "Involuntary Employment",
                Some(Target::Object(elenda)),
            );
            finish_to(&mut surface, &mut state, 1);
            assert_eq!(state.objects.get(elenda).controller, actor.opponent());
            assert_eq!(state.objects.get(elenda).owner, actor);
            stats(&state, elenda, 4, false);
            if suppress {
                announce(
                    &mut surface,
                    &mut state,
                    actor.opponent(),
                    "Witness Protection",
                    Some(Target::Object(elenda)),
                );
                finish_to(&mut surface, &mut state, 1);
                assert!(
                    mtg_kernel::source_type_hexproof_v1::characteristics(&state, elenda).is_none()
                );
                assert!(!engine::has_effective_keyword(
                    &state,
                    elenda,
                    Keywords::LIFELINK
                ));
                assert_eq!(engine::effective_toughness(&state, elenda), 1);
            }
            state = replay(&state);
            finish_to(&mut surface, &mut state, 0);
            assert_eq!(
                state.objects.get(elenda).zone,
                if suppress {
                    Zone::Graveyard
                } else {
                    Zone::Battlefield
                }
            );
            if !suppress {
                assert_eq!(state.objects.get(elenda).damage, 0);
            }
        }
    }
}

#[test]
fn real_paid_witness_and_broken_wings_suppress_then_restore_every_printed_rule() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, 30);
        let mut surface = surface();
        let elenda = cast_elenda(&mut surface, &mut state, actor);
        let aura = announce(
            &mut surface,
            &mut state,
            actor,
            "Witness Protection",
            Some(Target::Object(elenda)),
        );
        finish_to(&mut surface, &mut state, 0);
        assert_eq!(
            (
                engine::effective_power(&state, elenda),
                engine::effective_toughness(&state, elenda)
            ),
            (1, 1)
        );
        for keyword in [Keywords::MENACE, Keywords::LIFELINK] {
            assert!(!engine::has_effective_keyword(&state, elenda, keyword));
        }
        assert!(
            !mtg_kernel::starting_life_statics_v1::characteristics(&state, elenda)
                .unwrap()
                .printed_abilities_active
        );
        assert!(mtg_kernel::source_type_hexproof_v1::characteristics(&state, elenda).is_none());
        announce(
            &mut surface,
            &mut state,
            actor,
            "Broken Wings",
            Some(Target::Object(aura)),
        );
        state = replay(&state);
        finish_to(&mut surface, &mut state, 0);
        assert_eq!(state.objects.get(aura).zone, Zone::Graveyard);
        stats(&state, elenda, 10, true);
    }
}

fn attack_next_turn(state: &mut GameState, actor: PlayerId, elenda: ObjectId) {
    crate::fixture_turn_v1::reach(state, actor.opponent(), Step::Main1);
    crate::fixture_turn_v1::reach(state, actor, Step::DeclareAttackers);
    assert!(
        matches!(engine::advance_until_decision(state), Decision::DeclareAttackers { eligible, .. } if eligible.contains(&elenda))
    );
    engine::step(state, Action::DeclareAttackers(vec![elenda])).unwrap();
    crate::fixture_turn_v1::reach(state, actor, Step::DeclareBlockers);
}
#[test]
fn real_unblocked_combat_lifelink_crosses_both_thresholds_and_restores() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, 20);
        let mut surface = surface();
        let elenda = cast_elenda(&mut surface, &mut state, actor);
        for (own_life, opposing_life, pt) in [(24, 16, 5), (29, 11, 5), (34, 6, 10)] {
            attack_next_turn(&mut state, actor, elenda);
            state = replay(&state);
            engine::step(&mut state, Action::DeclareBlockers(vec![])).unwrap();
            crate::fixture_turn_v1::reach(&mut state, actor, Step::Main2);
            assert_eq!(state.players[actor.index()].life, own_life);
            assert_eq!(state.players[actor.opponent().index()].life, opposing_life);
            stats(&state, elenda, pt, true);
        }
    }
}

#[test]
fn genuine_menace_rejects_one_blocker_readonly_and_allows_two_only_above_start() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for life in [20, 21] {
            let mut state = ready(actor, life);
            let mut surface = surface();
            let elenda = cast_elenda(&mut surface, &mut state, actor);
            let first = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let second = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            attack_next_turn(&mut state, actor, elenda);
            state = replay(&state);
            let before = serde_json::to_vec(&state).unwrap();
            let result = engine::step(&mut state, Action::DeclareBlockers(vec![(first, elenda)]));
            if life > 20 {
                assert!(result.is_err());
                assert_eq!(before, serde_json::to_vec(&state).unwrap());
                engine::step(
                    &mut state,
                    Action::DeclareBlockers(vec![(first, elenda), (second, elenda)]),
                )
                .unwrap();
            } else {
                result.unwrap();
            }
        }
    }
}

#[test]
fn genuine_opted_in_stack_departure_rejects_type_only_tampering_with_original_journals() {
    let mut state = ready(PlayerId::P0, 20);
    let mut surface = surface();
    cast_elenda(&mut surface, &mut state, PlayerId::P0);
    let bolt = announce(
        &mut surface,
        &mut state,
        PlayerId::P0,
        "Lightning Bolt",
        Some(Target::Player(PlayerId::P1)),
    );
    finish_to(&mut surface, &mut state, 0);
    for changed_bits in [0, 8] {
        let mut forged = replay(&state);
        let mut count = 0;
        for event in &mut forged.engine.event_history {
            if let CommittedEvent::ProtectionSourceColorDepartureV1 { snapshot, .. } = event {
                if snapshot.source.source == bolt && snapshot.source.zone == Zone::Stack {
                    assert!(snapshot
                        .source_types_v1
                        .0
                        .unwrap()
                        .contains(CardType::Instant));
                    assert!(!snapshot.source_type_input_v1.is_empty());
                    snapshot.source_types_v1 =
                        serde_json::from_value(serde_json::json!(changed_bits)).unwrap();
                    count += 1;
                }
            }
        }
        assert_eq!(count, 1);
        let before = serde_json::to_vec(&forged).unwrap();
        assert!(engine::step(&mut forged, Action::Pass).is_err());
        assert_eq!(before, serde_json::to_vec(&forged).unwrap());
        assert!(matches!(
            engine::advance_until_decision(&mut forged),
            Decision::Halted { .. }
        ));
    }
}
