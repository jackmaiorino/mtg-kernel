//! Genuine FDN267 native Partial fixtures. Non-cast setup and zone/control
//! transitions are labelled native primitives; no owner/model admission.
use super::*;
use crate::{
    engine::{self, Action, Decision},
    event::{self, ProposedEvent},
    mana::ManaColor,
    policy_observation_v6::tests::put,
    state::{StackItemKind, Step, Target},
};
fn id(name: &str) -> u16 {
    card_def::card_id_by_name(name).unwrap()
}
fn ready(actor: PlayerId) -> GameState {
    let deck = [id("Swamp"); 40];
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &deck,
        &deck,
        crate::rl::card_name,
        110_470,
        actor,
    );
    s.step = Step::Main1;
    s
}
fn restore(s: &GameState) -> GameState {
    let result = serde_json::from_slice(&serde_json::to_vec(s).unwrap()).unwrap();
    assert_eq!(s, &result);
    result
}
fn priority(s: &mut GameState, actor: PlayerId) {
    for _ in 0..12 {
        match engine::advance_until_decision(s) {
            Decision::CastSpellOrPass { player, .. } if player == actor => return,
            Decision::CastSpellOrPass { .. } => engine::step(s, Action::Pass).unwrap(),
            d => panic!("native priority: {d:?}"),
        }
    }
    panic!("native actor priority absent")
}
fn finish(s: &mut GameState) {
    for _ in 0..80 {
        let d = engine::advance_until_decision(s);
        assert!(s.engine.halted.is_none(), "{:?}", s.engine.halted);
        if s.stack.is_empty()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_triggers.is_empty()
        {
            assert!(matches!(d, Decision::CastSpellOrPass { .. }));
            return;
        }
        let action = match d {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            Decision::ChooseEffectTargets {
                legal_targets,
                can_finish,
                ..
            } => legal_targets.first().copied().map_or_else(
                || {
                    assert!(can_finish);
                    Action::FinishEffectSelection
                },
                Action::ChooseEffectTarget,
            ),
            d => panic!("genuine native resolution: {d:?}"),
        };
        engine::step(s, action).unwrap();
    }
    panic!("native resolution did not finish")
}
fn choose(s: &mut GameState, actor: PlayerId, chosen: CreatureTypeV60) {
    assert!(
        matches!(engine::advance_until_decision(s), Decision::ChooseEntryCreatureTypeV1 { player, ref types, .. }
        if player == actor && types.as_slice() == CreatureTypeV60::ALL)
    );
    engine::step(s, Action::ChooseEffectOption(chosen.choice_index())).unwrap();
    priority(s, actor);
}
fn announce_land(s: &mut GameState, actor: PlayerId) -> ObjectId {
    priority(s, actor);
    let source = put(s, actor, "Secluded Courtyard", Zone::Hand);
    engine::step(s, Action::PlayLand(source)).unwrap();
    assert!(
        matches!(engine::advance_until_decision(s), Decision::ChooseEntryCreatureTypeV1 { player, ref types, .. }
        if player == actor && types.len() == 324 && types.as_slice() == CreatureTypeV60::ALL)
    );
    assert_eq!(s.objects.get(source).zone, Zone::Hand);
    assert_eq!(s.objects.get(source).v4.kinship_type_v1, None);
    source
}
fn land(s: &mut GameState, actor: PlayerId, chosen: CreatureTypeV60) -> ObjectId {
    let source = announce_land(s, actor);
    choose(s, actor, chosen);
    source
}
fn cast(s: &mut GameState, actor: PlayerId, name: &str, target: Option<ObjectId>) -> ObjectId {
    priority(s, actor);
    let source = put(s, actor, name, Zone::Hand);
    assert!(
        matches!(engine::advance_until_decision(s), Decision::CastSpellOrPass { ref castable_spells, .. } if castable_spells.contains(&source))
    );
    engine::step(s, Action::CastSpell(source)).unwrap();
    for _ in 0..12 {
        match engine::advance_until_decision(s) {
            Decision::ChooseTargets { legal_targets, .. } => {
                let object = target.unwrap();
                let candidate = legal_targets
                    .iter()
                    .find(|t| match t {
                        Target::Object(id) => *id == object,
                        Target::StackItem(id) => s
                            .stack
                            .iter()
                            .any(|r| r.source == object && r.v4.stack_item_id == *id),
                        _ => false,
                    })
                    .copied()
                    .expect("original native target");
                engine::step(s, Action::ChooseTarget(candidate)).unwrap();
            }
            Decision::CastSpellOrPass { player, .. } => {
                assert_eq!(player, actor);
                assert!(s.engine.pending_cast.is_none());
                assert_eq!(s.objects.get(source).zone, Zone::Stack);
                let row = s.stack.iter().find(|r| r.source == source).unwrap();
                assert_eq!((row.kind, row.controller), (StackItemKind::Spell, actor));
                return source;
            }
            d => panic!("genuine paid cast: {d:?}"),
        }
    }
    panic!("paid cast did not publish")
}
#[test]
fn genuine_land_play_all_324_types_bounds_and_original_checkpoint_restore_both_seats() {
    assert!(owns_courtyard(470));
    assert!(!owns_definition(470));
    assert!(card_def::preflight_fully_supported_deck(&[470]).is_err());
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut pending = ready(actor);
        let source = announce_land(&mut pending, actor);
        for action in [
            Action::ChooseEffectOption(324),
            Action::ChooseEffectOption(u16::MAX),
            Action::Pass,
            Action::ActivateManaAbilityChoice(source, ManaColor::W),
        ] {
            let mut bad = restore(&pending);
            let before = bad.clone();
            assert!(engine::step(&mut bad, action).is_err());
            assert_eq!(bad, before);
        }
        for &chosen in CreatureTypeV60::ALL {
            let mut actual = pending.clone();
            let mut replay = restore(&pending);
            for branch in [&mut actual, &mut replay] {
                choose(branch, actor, chosen);
                assert_eq!(branch.objects.get(source).zone, Zone::Battlefield);
                assert!(!branch.objects.get(source).tapped);
                assert_eq!(branch.players[actor.index()].lands_played_this_turn, 1);
                assert_eq!(branch.objects.get(source).counters.fellowship, 0);
                assert_eq!(authenticated_type(branch, source).unwrap(), chosen);
                validate_state(branch).unwrap();
                assert!(crate::rl::observe_v1(branch, actor, 0).is_err());
                assert!(crate::drake_public_v53::capture(branch).is_err());
            }
            assert_eq!(actual, replay);
        }
    }
}
#[test]
fn genuine_colorless_and_automatic_chosen_creature_payment_counter_response_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let source = land(&mut s, actor, CreatureTypeV60::Elf);
        engine::step(
            &mut s,
            Action::ActivateManaAbilityChoice(source, ManaColor::C),
        )
        .unwrap();
        assert_eq!(
            s.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
            1
        );
        assert!(s.players[actor.index()].restricted_mana_pool.is_empty());
        let artifact = cast(&mut s, actor, "Viridian Longbow", None);
        finish(&mut s);
        assert_eq!(s.objects.get(artifact).zone, Zone::Battlefield);
        assert_eq!(s.players[actor.index()].mana_pool, [0; 6]);

        let mut s = ready(actor);
        let source = land(&mut s, actor, CreatureTypeV60::Elf);
        let bolt = put(&mut s, actor, "Lightning Bolt", Zone::Hand);
        let goblin = put(&mut s, actor, "Goblin Bushwhacker", Zone::Hand);
        let Decision::CastSpellOrPass {
            castable_spells, ..
        } = engine::advance_until_decision(&mut s)
        else {
            panic!("priority")
        };
        assert!(!castable_spells.contains(&bolt));
        assert!(!castable_spells.contains(&goblin));
        let elf = cast(&mut s, actor, "Llanowar Elves", None);
        assert!(s.objects.get(source).tapped);
        assert_eq!(s.players[actor.index()].mana_pool, [0; 6]);
        assert!(s.players[actor.index()].restricted_mana_pool.is_empty());
        for _ in 0..2 {
            put(&mut s, actor.opponent(), "Island", Zone::Battlefield);
        }
        let counter = cast(&mut s, actor.opponent(), "Counterspell", Some(elf));
        assert_eq!(
            s.players[actor.opponent().index()]
                .battlefield
                .iter()
                .filter(|id| s.objects.get(**id).tapped)
                .count(),
            2
        );
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(s.objects.get(elf).zone, Zone::Graveyard);
        assert_eq!(s.objects.get(counter).zone, Zone::Graveyard);
        assert!(s.objects.get(source).tapped);
    }
}
#[test]
fn genuine_changeling_spell_matches_unrepresented_chosen_type_with_paid_colored_mana_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let source = land(&mut s, actor, CreatureTypeV60::TimeLord);
        // Webweaver costs 3GG. Courtyard supplies one matching green unit;
        // the other green and three generic units use ordinary native lands.
        put(&mut s, actor, "Forest", Zone::Battlefield);
        for _ in 0..3 {
            put(&mut s, actor, "Soulstone Sanctuary", Zone::Battlefield);
        }
        assert!(card_def::CARD_DEFS[usize::from(id("Webweaver Changeling"))].changeling);
        let creature = cast(&mut s, actor, "Webweaver Changeling", None);
        assert!(s.objects.get(source).tapped);
        assert_eq!(
            s.players[actor.index()]
                .battlefield
                .iter()
                .filter(|id| s.objects.get(**id).tapped)
                .count(),
            5
        );
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(s.objects.get(creature).zone, Zone::Battlefield);
        assert_eq!(s.players[actor.index()].mana_pool, [0; 6]);
    }
}
#[test]
fn genuine_graveyard_creature_activation_uses_original_float_after_land_reentry_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let source = land(&mut s, actor, CreatureTypeV60::Skeleton);
        let generation = s.objects.get(source).zone_change_count;
        engine::step(
            &mut s,
            Action::ActivateManaAbilityChoice(source, ManaColor::B),
        )
        .unwrap();
        let unit = s.players[actor.index()].restricted_mana_pool.0[0].clone();
        assert_eq!(
            unit.restriction,
            card_def::ManaSpendRestrictionDef::ChosenCreatureTypeV60(Some(
                CreatureTypeV60::Skeleton
            ))
        );
        assert_eq!(s.players[actor.index()].mana_pool, [0; 6]);
        // Refusal cannot depend on finding the source in a visible zone.
        // The restricted mana's public rule survives a library departure.
        let mut hidden_source = restore(&s);
        event::propose_and_commit(
            &mut hidden_source,
            ProposedEvent::zone_change(source, Zone::Library),
        );
        assert_eq!(hidden_source.objects.get(source).zone, Zone::Library);
        for viewer in [actor, actor.opponent()] {
            assert!(crate::rl::observe_v1(&hidden_source, viewer, 0).is_err());
        }
        // Real native zone transitions, not a claim of a paid land blink spell.
        event::propose_and_commit(&mut s, ProposedEvent::zone_change(source, Zone::Graveyard));
        event::propose_and_commit(
            &mut s,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        choose(&mut s, actor, CreatureTypeV60::Elf);
        assert_eq!(s.objects.get(source).zone_change_count, generation + 2);
        assert_eq!(
            authenticated_type(&s, source).unwrap(),
            CreatureTypeV60::Elf
        );
        assert_eq!(
            s.players[actor.index()].restricted_mana_pool.0,
            vec![unit.clone()]
        );
        // The matching creature-card starts in its real activation zone.
        let skeleton = put(&mut s, actor, "Reassembling Skeleton", Zone::Graveyard);
        let generic = put(&mut s, actor, "Soulstone Sanctuary", Zone::Battlefield);
        // Keep the reentered land unavailable. Its new Elf choice must not
        // authenticate or replace the old Skeleton-restricted floating unit.
        engine::step(
            &mut s,
            Action::ActivateManaAbilityChoice(source, ManaColor::C),
        )
        .unwrap();
        // Spend C on the genuine Longbow cast, leaving only old B + generic land.
        cast(&mut s, actor, "Viridian Longbow", None);
        finish(&mut s);
        for mutate in 0..3 {
            let mut bad = restore(&s);
            let forged = &mut bad.players[actor.index()].restricted_mana_pool.0[0];
            match mutate {
                0 => {
                    forged.restriction = card_def::ManaSpendRestrictionDef::ChosenCreatureTypeV60(
                        Some(CreatureTypeV60::Elf),
                    )
                }
                1 => forged.source.zone_change_count += 1,
                _ => forged.source_card_def = id("Forest"),
            }
            let before = bad.clone();
            assert!(engine::step(&mut bad, Action::ActivateAbility(skeleton, 0)).is_err());
            assert_eq!(bad, before);
        }
        let mut replay = restore(&s);
        for branch in [&mut s, &mut replay] {
            engine::step(branch, Action::ActivateAbility(skeleton, 0)).unwrap();
            assert!(
                matches!(engine::advance_until_decision(branch), Decision::CastSpellOrPass { player, .. } if player == actor)
            );
            assert!(branch.objects.get(generic).tapped);
            assert!(branch.players[actor.index()]
                .restricted_mana_pool
                .is_empty());
        }
        assert_eq!(s, replay);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(s.objects.get(skeleton).zone, Zone::Battlefield);
        assert_eq!(s.objects.get(skeleton).owner, actor);
        assert!(s.objects.get(skeleton).tapped);
    }
}

#[test]
fn genuine_hand_creature_card_activation_uses_float_after_native_control_change_restore_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let source = land(&mut s, actor, CreatureTypeV60::Troll);
        engine::step(
            &mut s,
            Action::ActivateManaAbilityChoice(source, ManaColor::U),
        )
        .unwrap();
        let unit = s.players[actor.index()].restricted_mana_pool.0[0].clone();
        // Native control primitive, not a claim of a paid land-control spell.
        // Control does not choose a new type, or change already-produced mana.
        crate::standard_cards_v1::gain_control(&mut s, source, actor.opponent());
        assert_eq!(
            authenticated_type(&s, source).unwrap(),
            CreatureTypeV60::Troll
        );
        assert_eq!(s.objects.get(source).owner, actor);
        assert_eq!(s.objects.get(source).controller, actor.opponent());
        assert_eq!(s.players[actor.index()].restricted_mana_pool.0, vec![unit]);
        let troll = put(&mut s, actor, "Troll of Khazad-dum", Zone::Hand);
        let before_library = s.players[actor.index()].library.len();
        let mut replay = restore(&s);
        for branch in [&mut s, &mut replay] {
            // Genuine Swampcycling {1}: creature CARD activation from hand.
            engine::step(branch, Action::ActivateAbility(troll, 0)).unwrap();
            priority(branch, actor);
            assert_eq!(branch.objects.get(troll).zone, Zone::Graveyard);
            assert!(branch.players[actor.index()]
                .restricted_mana_pool
                .is_empty());
            assert!(branch.stack.iter().any(|r| r.source == troll
                && r.kind == StackItemKind::ActivatedAbility
                && r.controller == actor));
        }
        assert_eq!(s, replay);
        let mut restored_published = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        finish(&mut restored_published);
        assert_eq!(s, replay);
        assert_eq!(s, restored_published);
        assert_eq!(s.players[actor.index()].library.len(), before_library - 1);
        assert!(s.players[actor.index()]
            .hand
            .iter()
            .any(|object| s.objects.get(*object).card_def == id("Swamp")));
    }
}

#[test]
fn native_token_copy_chooses_again_without_copying_original_choice_or_counter_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let source = land(&mut s, actor, CreatureTypeV60::Elf);
        // Existing native copy primitive, not a paid copy-spell claim.
        assert!(crate::standard_cards_v1::create_token_copy(&mut s, source, actor).is_none());
        assert!(
            matches!(engine::advance_until_decision(&mut s), Decision::ChooseEntryCreatureTypeV1 { player, .. } if player == actor)
        );
        let mut replay = restore(&s);
        for branch in [&mut s, &mut replay] {
            choose(branch, actor, CreatureTypeV60::TimeLord);
            let copies = branch.players[actor.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|id| *id != source && branch.objects.get(*id).card_def == 470)
                .collect::<Vec<_>>();
            assert_eq!(copies.len(), 1);
            let copy = copies[0];
            assert!(branch.objects.get(copy).v4.is_token);
            assert_eq!(
                authenticated_type(branch, copy).unwrap(),
                CreatureTypeV60::TimeLord
            );
            assert_eq!(
                authenticated_type(branch, source).unwrap(),
                CreatureTypeV60::Elf
            );
            assert_eq!(branch.objects.get(copy).counters.fellowship, 0);
            assert_eq!(branch.players[actor.index()].lands_played_this_turn, 1);
            validate_state(branch).unwrap();
        }
        assert_eq!(s, replay);
    }
}

#[test]
fn genuine_automatic_creature_card_activation_pays_both_pips_and_preserves_native_restore_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let source = land(&mut s, actor, CreatureTypeV60::Skeleton);
        let generic = put(&mut s, actor, "Soulstone Sanctuary", Zone::Battlefield);
        let skeleton = put(&mut s, actor, "Reassembling Skeleton", Zone::Graveyard);
        let mut replay = restore(&s);
        for branch in [&mut s, &mut replay] {
            engine::step(branch, Action::ActivateAbility(skeleton, 0)).unwrap();
            priority(branch, actor);
            assert!(branch.objects.get(source).tapped);
            assert!(branch.objects.get(generic).tapped);
            assert_eq!(branch.players[actor.index()].mana_pool, [0; 6]);
            assert!(branch.players[actor.index()]
                .restricted_mana_pool
                .is_empty());
            assert!(branch.stack.iter().any(|r| r.source == skeleton
                && r.kind == StackItemKind::ActivatedAbility
                && r.controller == actor));
        }
        assert_eq!(s, replay);
        let mut published = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        finish(&mut published);
        assert_eq!(s, replay);
        assert_eq!(s, published);
        assert_eq!(s.objects.get(skeleton).zone, Zone::Battlefield);
        assert!(s.objects.get(skeleton).tapped);
    }
}
