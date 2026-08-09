//! Focused current-Mage parity for Cast into the Fire, Dust to Dust, and
//! Thraben Charm, including reusable variable-cardinality spell targeting.

use mtg_kernel::card_def::{card_id_by_name, CardCapability, TargetSpec, CARD_DEFS};
use mtg_kernel::effect::{EffectOp, ObjectRef, TargetRef};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::rl::{legal_action_candidates_v1, ActionSemanticV1};
use mtg_kernel::state::{Counters, GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::surface_v2::SurfaceDecision;

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_owned()
}

fn ready_game(seed: u64) -> GameState {
    let mut state = GameState::new_from_libraries(&[], &[], card_name, seed);
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state.step = Step::Main1;
    state.players[PlayerId::P0.index()].mana_pool[ManaColor::W.pool_index()] = 8;
    state.players[PlayerId::P0.index()].mana_pool[ManaColor::R.pool_index()] = 8;
    state
}

fn put_object(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id(name);
    let object = state.objects.push(GameObject {
        card_def,
        name: name.to_owned(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(object),
        Zone::Battlefield => state.players[player.index()].battlefield.push(object),
        Zone::Library => state.players[player.index()].library.push(object),
        Zone::Graveyard => state.players[player.index()].graveyard.push(object),
        Zone::Exile => state.exile.push(object),
        Zone::Command => state.command.push(object),
        Zone::Stack => panic!("casts own stack insertion"),
    }
    object
}

fn begin_mode(state: &mut GameState, spell: ObjectId, mode: u8, mode_count: u8) -> Decision {
    engine::step(state, Action::CastSpell(spell)).unwrap();
    let choose_mode = engine::advance_until_decision(state);
    match choose_mode {
        Decision::ChooseSpellMode {
            player: PlayerId::P0,
            spell: source,
            mode_count: actual,
            legal_modes,
        } => {
            assert_eq!(source, spell);
            assert_eq!(actual, mode_count);
            assert!(legal_modes.contains(&mode));
            engine::step(state, Action::ChooseSpellMode(mode)).unwrap();
            engine::advance_until_decision(state)
        }
        decision => {
            assert_eq!(
                state
                    .engine
                    .pending_cast
                    .as_ref()
                    .and_then(|pending| pending.mode_chosen),
                Some(mode),
                "only the requested printed mode may be silently selected"
            );
            decision
        }
    }
}

fn choose_target(state: &mut GameState, decision: Decision, target: Target) -> Decision {
    assert!(matches!(
        decision,
        Decision::ChooseTargets { ref legal_targets, .. } if legal_targets.contains(&target)
    ));
    engine::step(state, Action::ChooseTarget(target)).unwrap();
    engine::advance_until_decision(state)
}

fn resolve_spell(state: &mut GameState, spell: ObjectId) {
    for _ in 0..64 {
        let decision = engine::advance_until_decision(state);
        if state.objects.get(spell).zone != Zone::Stack {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision while resolving spell: {other:?}"),
        }
    }
    panic!("spell did not resolve within the bounded test loop");
}

#[test]
fn generated_definitions_bind_the_exact_modal_target_shapes_and_programs() {
    assert_eq!(card_id("Cast into the Fire"), 12);
    assert_eq!(card_id("Dust to Dust"), 25);
    assert_eq!(card_id("Thraben Charm"), 118);
    assert_eq!(CARD_DEFS.len(), 155);

    let cast = &CARD_DEFS[card_id("Cast into the Fire") as usize];
    assert_eq!(cast.capability, CardCapability::Full);
    assert_eq!(cast.target_spec, TargetSpec::UpToTwoCreatures);
    assert_eq!(
        (cast.spell_effect)(),
        Some(EffectOp::DamageAllTargets { amount: 1 })
    );
    let cast_mode2 = cast
        .mode2
        .as_ref()
        .expect("Cast into the Fire has two modes");
    assert_eq!(cast_mode2.target_spec, TargetSpec::ArtifactPermanent);
    assert_eq!(
        (cast_mode2.effect)(),
        EffectOp::MoveAllTargets {
            to_zone: Zone::Exile
        }
    );

    let dust = &CARD_DEFS[card_id("Dust to Dust") as usize];
    assert_eq!(dust.capability, CardCapability::Full);
    assert_eq!(dust.target_spec, TargetSpec::ExactlyTwoArtifactPermanents);
    assert_eq!(
        (dust.spell_effect)(),
        Some(EffectOp::ExileAllArtifactTargets)
    );

    let thraben = &CARD_DEFS[card_id("Thraben Charm") as usize];
    assert_eq!(thraben.capability, CardCapability::Full);
    assert_eq!(thraben.target_spec, TargetSpec::Creature);
    assert_eq!(
        (thraben.spell_effect)(),
        Some(EffectOp::DealDamageByControlledCreatureCount {
            target: TargetRef::Target(0),
            multiplier: 2,
        })
    );
    let thraben_mode2 = thraben.mode2.as_ref().expect("Thraben Charm has mode two");
    assert_eq!(thraben_mode2.target_spec, TargetSpec::EnchantmentPermanent);
    assert_eq!(
        (thraben_mode2.effect)(),
        EffectOp::DestroyObject {
            object: ObjectRef::Target(0),
        }
    );
    let thraben_mode3 = thraben
        .mode3
        .as_ref()
        .expect("Thraben Charm has mode three");
    assert_eq!(thraben_mode3.target_spec, TargetSpec::UpToTwoPlayers);
    assert_eq!(
        (thraben_mode3.effect)(),
        EffectOp::ExileTargetPlayersGraveyards
    );
}

#[test]
fn optional_spell_targets_have_an_executable_flat_finish_action_and_roundtrip() {
    let mut state = ready_game(0x4341_5354_0000_0001);
    let cast = put_object(&mut state, PlayerId::P0, "Cast into the Fire", Zone::Hand);
    let decision = begin_mode(&mut state, cast, 0, 2);
    assert!(matches!(
        decision,
        Decision::ChooseTargets {
            remaining: 2,
            ref legal_targets,
            can_finish: true,
            ..
        } if legal_targets.is_empty()
    ));
    let actions = legal_action_candidates_v1(&SurfaceDecision::Decision(decision), &state).unwrap();
    assert_eq!(actions.len(), 1);
    assert!(matches!(
        actions[0].record.semantic,
        ActionSemanticV1::FinishTargetSelection {
            selected_count: 0,
            ..
        }
    ));

    let before = state.diagnostic_state_hash();
    engine::step(&mut state, Action::FinishEffectSelection).unwrap();
    assert_ne!(state.diagnostic_state_hash(), before);
    assert!(state
        .engine
        .pending_cast
        .as_ref()
        .is_some_and(|pending| pending.target_selection_finished));
    let bytes = serde_json::to_vec(&state).unwrap();
    let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        state.diagnostic_state_hash(),
        restored.diagnostic_state_hash()
    );
    assert!(matches!(
        engine::advance_until_decision(&mut restored),
        Decision::CastSpellOrPass { .. }
    ));
    resolve_spell(&mut restored, cast);
    assert_eq!(restored.objects.get(cast).zone, Zone::Graveyard);
}

#[test]
fn sparse_three_mode_choices_preserve_printed_indices_on_the_rl_surface() {
    let mut state = ready_game(0x5448_5241_4245_0000);
    put_object(&mut state, PlayerId::P0, "Faerie Seer", Zone::Battlefield);
    let charm = put_object(&mut state, PlayerId::P0, "Thraben Charm", Zone::Hand);
    engine::step(&mut state, Action::CastSpell(charm)).unwrap();
    let decision = engine::advance_until_decision(&mut state);
    assert!(matches!(
        decision,
        Decision::ChooseSpellMode {
            spell,
            mode_count: 3,
            ref legal_modes,
            ..
        } if spell == charm && legal_modes == &vec![0, 2]
    ));
    let modes = legal_action_candidates_v1(&SurfaceDecision::Decision(decision), &state)
        .unwrap()
        .into_iter()
        .map(|candidate| match candidate.record.semantic {
            ActionSemanticV1::ChooseSpellMode {
                mode_index,
                mode_count,
                ..
            } => (mode_index, mode_count),
            other => panic!("unexpected sparse-mode action: {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(modes, vec![(0, 3), (2, 3)]);
    assert!(engine::step(&mut state, Action::ChooseSpellMode(1)).is_err());
    engine::step(&mut state, Action::ChooseSpellMode(2)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseTargets {
            remaining: 2,
            can_finish: true,
            ..
        }
    ));
}

#[test]
fn cast_into_the_fire_damages_each_still_legal_target_and_exiles_artifacts() {
    let mut state = ready_game(0x4341_5354_0000_0002);
    let stale = put_object(&mut state, PlayerId::P1, "Faerie Seer", Zone::Battlefield);
    let live = put_object(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
    let cast = put_object(&mut state, PlayerId::P0, "Cast into the Fire", Zone::Hand);
    let first = begin_mode(&mut state, cast, 0, 2);
    let second = choose_target(&mut state, first, Target::Object(stale));
    let priority = choose_target(&mut state, second, Target::Object(live));
    assert!(matches!(priority, Decision::CastSpellOrPass { .. }));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(stale, Zone::Graveyard),
    );
    resolve_spell(&mut state, cast);
    assert_eq!(state.objects.get(stale).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(stale).damage, 0);
    assert_eq!(state.objects.get(live).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(live).damage, 1);

    let mut exile_state = ready_game(0x4341_5354_0000_0003);
    let artifact = put_object(
        &mut exile_state,
        PlayerId::P1,
        "Great Furnace",
        Zone::Battlefield,
    );
    let cast = put_object(
        &mut exile_state,
        PlayerId::P0,
        "Cast into the Fire",
        Zone::Hand,
    );
    let targets = begin_mode(&mut exile_state, cast, 1, 2);
    choose_target(&mut exile_state, targets, Target::Object(artifact));
    resolve_spell(&mut exile_state, cast);
    assert_eq!(exile_state.objects.get(artifact).zone, Zone::Exile);
}

#[test]
fn dust_to_dust_requires_two_distinct_artifacts_and_ignores_a_stale_one() {
    let mut state = ready_game(0x4455_5354_0000_0001);
    let dust = put_object(&mut state, PlayerId::P0, "Dust to Dust", Zone::Hand);
    let first_artifact = put_object(&mut state, PlayerId::P0, "Great Furnace", Zone::Battlefield);
    let castable = engine::advance_until_decision(&mut state);
    assert!(matches!(
        castable,
        Decision::CastSpellOrPass { ref castable_spells, .. }
            if !castable_spells.contains(&dust)
    ));

    let second_artifact = put_object(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { ref castable_spells, .. }
            if castable_spells.contains(&dust)
    ));
    engine::step(&mut state, Action::CastSpell(dust)).unwrap();
    let first = engine::advance_until_decision(&mut state);
    let second = choose_target(&mut state, first, Target::Object(first_artifact));
    assert!(matches!(
        second,
        Decision::ChooseTargets {
            remaining: 1,
            ref legal_targets,
            can_finish: false,
            ..
        } if legal_targets == &vec![Target::Object(second_artifact)]
    ));
    choose_target(&mut state, second, Target::Object(second_artifact));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(first_artifact, Zone::Hand),
    );
    resolve_spell(&mut state, dust);
    assert_eq!(state.objects.get(first_artifact).zone, Zone::Hand);
    assert_eq!(state.objects.get(second_artifact).zone, Zone::Exile);
}

#[test]
fn thraben_charm_samples_creature_count_at_resolution_and_destroys_enchantments() {
    let mut state = ready_game(0x5448_5241_4245_0001);
    put_object(&mut state, PlayerId::P0, "Faerie Seer", Zone::Battlefield);
    put_object(&mut state, PlayerId::P0, "Guttersnipe", Zone::Battlefield);
    let target = put_object(&mut state, PlayerId::P1, "Generous Ent", Zone::Battlefield);
    let charm = put_object(&mut state, PlayerId::P0, "Thraben Charm", Zone::Hand);
    let targets = begin_mode(&mut state, charm, 0, 3);
    choose_target(&mut state, targets, Target::Object(target));
    put_object(
        &mut state,
        PlayerId::P0,
        "Clockwork Percussionist",
        Zone::Battlefield,
    );
    resolve_spell(&mut state, charm);
    assert_eq!(state.objects.get(target).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(target).damage, 6);

    let mut destroy_state = ready_game(0x5448_5241_4245_0002);
    let enchantment = put_object(
        &mut destroy_state,
        PlayerId::P1,
        "Makeshift Munitions",
        Zone::Battlefield,
    );
    let charm = put_object(
        &mut destroy_state,
        PlayerId::P0,
        "Thraben Charm",
        Zone::Hand,
    );
    let targets = begin_mode(&mut destroy_state, charm, 1, 3);
    choose_target(&mut destroy_state, targets, Target::Object(enchantment));
    resolve_spell(&mut destroy_state, charm);
    assert_eq!(destroy_state.objects.get(enchantment).zone, Zone::Graveyard);
}

#[test]
fn thraben_charm_graveyard_mode_accepts_zero_one_or_both_players() {
    for (case, selected) in [
        (0_u64, Vec::new()),
        (1, vec![PlayerId::P1]),
        (2, vec![PlayerId::P0, PlayerId::P1]),
    ] {
        let mut state = ready_game(0x5448_5241_4245_1000 + case);
        let p0_card = put_object(&mut state, PlayerId::P0, "Ponder", Zone::Graveyard);
        let p1_card = put_object(&mut state, PlayerId::P1, "Preordain", Zone::Graveyard);
        let charm = put_object(&mut state, PlayerId::P0, "Thraben Charm", Zone::Hand);
        let mut decision = begin_mode(&mut state, charm, 2, 3);
        for player in selected.iter().copied() {
            decision = choose_target(&mut state, decision, Target::Player(player));
        }
        if selected.len() < 2 {
            assert!(matches!(
                decision,
                Decision::ChooseTargets {
                    can_finish: true,
                    ..
                }
            ));
            engine::step(&mut state, Action::FinishEffectSelection).unwrap();
        } else {
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
        }
        resolve_spell(&mut state, charm);
        assert_eq!(
            state.objects.get(p0_card).zone,
            if selected.contains(&PlayerId::P0) {
                Zone::Exile
            } else {
                Zone::Graveyard
            }
        );
        assert_eq!(
            state.objects.get(p1_card).zone,
            if selected.contains(&PlayerId::P1) {
                Zone::Exile
            } else {
                Zone::Graveyard
            }
        );
    }
}
