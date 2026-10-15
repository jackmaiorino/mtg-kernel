//! Genuine Partial printing prerequisites, pending native qualification.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use crate::card_def::{
    card_id_by_name, CardCapability, CardType, CombatStaticRulesDef, Keywords, Subtype, CARD_DEFS,
};
use crate::effect::{
    EffectAnsweredChoiceGuard, EffectBooleanChoicePurpose, EffectFrame, EffectOp,
    PendingEffectChoice,
};
use crate::engine::{self, Action, Decision};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::{ManaColor, Pip};
use crate::state::{GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone};

fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let def = card_id_by_name(name).expect("requires the actual registered printing");
    let id = state.objects.push(GameObject {
        card_def: def,
        name: CARD_DEFS[usize::from(def)].object_name.into(),
        owner,
        controller: owner,
        zone: Zone::Library,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    state.players[owner.index()].library.push(id);
    event::propose_and_commit(state, ProposedEvent::zone_change(id, zone));
    id
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn restore(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let restored = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
    restored
}

fn same(a: &GameState, b: &GameState) {
    assert_eq!(
        serde_json::to_value(a).unwrap(),
        serde_json::to_value(b).unwrap()
    );
}

fn priority(state: &mut GameState, actor: PlayerId) {
    for _ in 0..8 {
        let d = next(state);
        if matches!(d, Decision::CastSpellOrPass { player, .. } if player == actor) {
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("priority not reached");
}

fn settle(state: &mut GameState) {
    for _ in 0..32 {
        let d = next(state);
        if state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
            && matches!(d, Decision::CastSpellOrPass { .. })
        {
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("stack not settled");
}

fn ready(actor: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        110_144,
        actor,
    );
    state.step = Step::Main1;
    // Real castable response cards retain both priority windows. These two
    // Mountains are also the possible later printed {R} return payment.
    for player in [PlayerId::P0, PlayerId::P1] {
        for _ in 0..2 {
            put(&mut state, player, "Mountain", Zone::Battlefield);
            put(&mut state, player, "Lightning Bolt", Zone::Hand);
        }
    }
    state
}

fn paid(actor: PlayerId) -> (GameState, ObjectId) {
    let mut state = ready(actor);
    let phoenix = put(&mut state, actor, "Flamewake Phoenix", Zone::Hand);
    let definition = &CARD_DEFS[usize::from(state.objects.get(phoenix).card_def)];
    assert_eq!(definition.capability, CardCapability::Partial);
    assert_eq!(definition.cost.generic, 1);
    assert_eq!(definition.cost.pips, &[Pip::Colored(ManaColor::R); 2]);
    assert_eq!(definition.types, &[CardType::Creature]);
    assert_eq!(definition.subtypes, &[Subtype::Phoenix]);
    assert_eq!((definition.power, definition.toughness), (Some(2), Some(2)));
    assert!(definition.keywords.has(Keywords::FLYING));
    assert!(definition.keywords.has(Keywords::HASTE));
    assert_eq!(
        definition.combat_static_rules,
        CombatStaticRulesDef {
            must_attack_each_combat: true,
            cant_be_blocked_by_subtype: None,
        }
    );
    assert!(crate::card_def::preflight_fully_supported_deck(&[state
        .objects
        .get(phoenix)
        .card_def])
    .is_err());
    for _ in 0..3 {
        let mountain = put(&mut state, actor, "Mountain", Zone::Battlefield);
        priority(&mut state, actor);
        engine::step(&mut state, Action::ActivateManaAbility(mountain)).unwrap();
        assert!(state.objects.get(mountain).tapped);
    }
    assert_eq!(
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()],
        3
    );
    engine::step(&mut state, Action::CastSpell(phoenix)).unwrap();
    settle(&mut state);
    assert_eq!(state.objects.get(phoenix).zone, Zone::Battlefield);
    assert_eq!(
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()],
        0
    );
    assert!(state.objects.get(phoenix).summoning_sick);
    assert!(state.engine.event_history.iter().any(|e| matches!(e,
        CommittedEvent::SpellCast { spell, controller } if *spell == phoenix && *controller == actor)));
    (state, phoenix)
}

fn paid_and_dead(actor: PlayerId) -> (GameState, ObjectId) {
    let (mut state, phoenix) = paid(actor);
    let opponent = actor.opponent();
    let bolt = state.players[opponent.index()]
        .hand
        .iter()
        .copied()
        .find(|id| CARD_DEFS[usize::from(state.objects.get(*id).card_def)].name == "Lightning Bolt")
        .unwrap();
    priority(&mut state, opponent);
    engine::step(&mut state, Action::CastSpell(bolt)).unwrap();
    assert!(
        matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. }
        if legal_targets.contains(&Target::Object(phoenix)))
    );
    engine::step(&mut state, Action::ChooseTarget(Target::Object(phoenix))).unwrap();
    settle(&mut state);
    assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(phoenix).zone, Zone::Graveyard);
    assert!(state.engine.event_history.iter().any(|e| matches!(e,
        CommittedEvent::ZoneChange { object, from: Zone::Battlefield, to: Zone::Graveyard, .. }
        if *object == phoenix)));
    (state, phoenix)
}

fn combat_trigger(state: &mut GameState, phoenix: ObjectId) {
    for _ in 0..32 {
        let d = next(state);
        if state.step == Step::BeginCombat
            && state.stack.last().is_some_and(|item| {
                item.source == phoenix && item.kind == StackItemKind::TriggeredAbility
            })
        {
            assert!(matches!(d, Decision::CastSpellOrPass { .. }));
            let source = state
                .stack
                .last()
                .unwrap()
                .v4
                .ability_source_contract
                .unwrap();
            assert_eq!(source.zone, Zone::Graveyard);
            assert_eq!(
                source.zone_change_count,
                state.objects.get(phoenix).zone_change_count
            );
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("genuine combat trigger not reached");
}

fn payment(state: &mut GameState, actor: PlayerId, phoenix: ObjectId) {
    for _ in 0..32 {
        let d = next(state);
        if let Decision::ChooseEffectBoolean {
            player,
            source,
            purpose,
            ..
        } = &d
        {
            assert_eq!((*player, *source), (actor, phoenix));
            assert!(matches!(purpose, EffectBooleanChoicePurpose::PayManaThen {
                player, colored, generic: 0, ..
            } if *player == actor && colored == &[ManaColor::R]));
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("payment not reached");
}

fn attack_menu(state: &mut GameState) -> Vec<ObjectId> {
    for _ in 0..32 {
        let d = next(state);
        if let Decision::DeclareAttackers { eligible, .. } = d {
            return eligible;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("declare attackers not reached");
}

fn untapped_mountains(state: &GameState, actor: PlayerId) -> usize {
    state.players[actor.index()]
        .battlefield
        .iter()
        .filter(|id| {
            let o = state.objects.get(**id);
            !o.tapped && CARD_DEFS[usize::from(o.card_def)].name == "Mountain"
        })
        .count()
}

#[test]
fn paid_death_combat_payment_restore_and_hasty_required_flying_attack_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, phoenix) = paid_and_dead(actor);
        // Existing permanent-entry operands provide the ferocious board and
        // legal defenders; they do not claim paid casts of those printings.
        put(&mut state, actor, "Serra Angel", Zone::Battlefield);
        let ground = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let flyer = put(
            &mut state,
            actor.opponent(),
            "Healer's Hawk",
            Zone::Battlefield,
        );
        let grave_generation = state.objects.get(phoenix).zone_change_count;
        let mut replay = restore(&state); // before the real beginning of combat
        for branch in [&mut state, &mut replay] {
            combat_trigger(branch, phoenix);
        }
        same(&state, &replay);
        replay = restore(&state); // published trigger, before resolution
        for branch in [&mut state, &mut replay] {
            payment(branch, actor, phoenix);
        }
        same(&state, &replay);
        let before_payment = untapped_mountains(&state, actor);
        replay = restore(&state); // actual optional payment prompt
        for branch in [&mut state, &mut replay] {
            engine::step(branch, Action::ChooseEffectBoolean(true)).unwrap();
            assert_eq!(branch.objects.get(phoenix).zone, Zone::Graveyard);
            assert_eq!(untapped_mountains(branch, actor), before_payment);
        }
        same(&state, &replay);
        replay = restore(&state); // accepted answer, before actual payment
        for branch in [&mut state, &mut replay] {
            settle(branch);
            assert_eq!(branch.objects.get(phoenix).zone, Zone::Battlefield);
            assert_eq!(
                branch.objects.get(phoenix).zone_change_count,
                grave_generation + 1
            );
            assert!(!branch.objects.get(phoenix).tapped);
            assert!(branch.objects.get(phoenix).summoning_sick);
            assert_eq!(untapped_mountains(branch, actor), before_payment - 1);
        }
        same(&state, &replay);
        replay = restore(&state); // completed paid return
        for branch in [&mut state, &mut replay] {
            assert!(attack_menu(branch).contains(&phoenix));
            let before = serde_json::to_vec(branch).unwrap();
            assert!(engine::step(branch, Action::DeclareAttackers(vec![])).is_err());
            assert_eq!(serde_json::to_vec(branch).unwrap(), before);
            engine::step(branch, Action::DeclareAttackers(vec![phoenix])).unwrap();
            assert!(branch.objects.get(phoenix).tapped);
            let mut reached_blockers = false;
            for _ in 0..16 {
                let d = next(branch);
                if let Decision::DeclareBlockers { legal_blockers, .. } = d {
                    let blockers = &legal_blockers
                        .iter()
                        .find(|(id, _)| *id == phoenix)
                        .unwrap()
                        .1;
                    assert!(!blockers.contains(&ground));
                    assert!(blockers.contains(&flyer));
                    reached_blockers = true;
                    break;
                }
                assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
                engine::step(branch, Action::Pass).unwrap();
            }
            assert!(reached_blockers, "real blocker menu not reached");
        }
        same(&state, &replay);
    }
}

#[test]
fn ferocious_needs_own_board_at_capture_and_again_before_any_payment_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for scenario in 0..4 {
            let (mut state, phoenix) = paid_and_dead(actor);
            let four = match scenario {
                0 => None,
                1 => Some(put(
                    &mut state,
                    actor.opponent(),
                    "Serra Angel",
                    Zone::Battlefield,
                )),
                2 => Some(put(&mut state, actor, "Cathar Commando", Zone::Battlefield)),
                _ => Some(put(&mut state, actor, "Serra Angel", Zone::Battlefield)),
            };
            if scenario == 3 {
                combat_trigger(&mut state, phoenix);
                // A response departure operand before resolution. No producer
                // of that response is claimed by this Phoenix test.
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(four.unwrap(), Zone::Hand),
                );
            }
            let before = untapped_mountains(&state, actor);
            let mut replay = restore(&state);
            for branch in [&mut state, &mut replay] {
                assert!(!attack_menu(branch).contains(&phoenix));
                assert_eq!(branch.objects.get(phoenix).zone, Zone::Graveyard);
                assert_eq!(untapped_mountains(branch, actor), before);
                assert!(!branch.stack.iter().any(|item| item.source == phoenix));
            }
            same(&state, &replay);
        }
    }
}

#[test]
fn graveyard_return_triggers_only_on_its_owners_actual_combat_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, phoenix) = paid_and_dead(actor);
        put(&mut state, actor, "Serra Angel", Zone::Battlefield);
        combat_trigger(&mut state, phoenix);
        payment(&mut state, actor, phoenix);
        engine::step(&mut state, Action::ChooseEffectBoolean(false)).unwrap();
        assert!(!attack_menu(&mut state).contains(&phoenix));
        engine::step(&mut state, Action::DeclareAttackers(vec![])).unwrap();
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            let mut reached = false;
            for _ in 0..96 {
                let d = next(branch);
                if branch.active_player == actor.opponent() && branch.step == Step::BeginCombat {
                    assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
                    assert!(!branch.stack.iter().any(|item| item.source == phoenix));
                    assert!(branch
                        .engine
                        .pending_triggers
                        .iter()
                        .all(|trigger| trigger.source != phoenix));
                    assert_eq!(branch.objects.get(phoenix).zone, Zone::Graveyard);
                    reached = true;
                    break;
                }
                assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
                engine::step(branch, Action::Pass).unwrap();
            }
            assert!(reached, "actual opposing combat not reached");
        }
        same(&state, &replay);
    }
}

#[test]
fn decline_or_unpayable_return_preserves_graveyard_and_never_pays_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for payable in [false, true] {
            let (mut state, phoenix) = paid_and_dead(actor);
            put(&mut state, actor, "Serra Angel", Zone::Battlefield);
            if !payable {
                let lands = state.players[actor.index()].battlefield.clone();
                for id in lands {
                    if CARD_DEFS[usize::from(state.objects.get(id).card_def)].name == "Mountain" {
                        event::propose_and_commit(
                            &mut state,
                            ProposedEvent::zone_change(id, Zone::Hand),
                        );
                    }
                }
            }
            combat_trigger(&mut state, phoenix);
            let before = untapped_mountains(&state, actor);
            let mut replay = restore(&state);
            for branch in [&mut state, &mut replay] {
                if payable {
                    payment(branch, actor, phoenix);
                    engine::step(branch, Action::ChooseEffectBoolean(false)).unwrap();
                }
                assert!(!attack_menu(branch).contains(&phoenix));
                assert_eq!(branch.objects.get(phoenix).zone, Zone::Graveyard);
                assert_eq!(untapped_mountains(branch, actor), before);
            }
            same(&state, &replay);
        }
    }
}

#[test]
fn stale_graveyard_incarnation_cannot_return_and_countered_trigger_keeps_cast_costs_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for countered in [false, true] {
            let (mut state, phoenix) = paid_and_dead(actor);
            put(&mut state, actor, "Serra Angel", Zone::Battlefield);
            combat_trigger(&mut state, phoenix);
            let before = untapped_mountains(&state, actor);
            let captured = state
                .stack
                .last()
                .unwrap()
                .v4
                .ability_source_contract
                .unwrap();
            if countered {
                let id = state.stack.last().unwrap().v4.stack_item_id;
                engine::counter_stack_item_by_id(&mut state, id).unwrap();
            } else {
                // A response round trip creates another graveyard object.
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(phoenix, Zone::Exile),
                );
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(phoenix, Zone::Graveyard),
                );
                assert!(state.objects.get(phoenix).zone_change_count > captured.zone_change_count);
                payment(&mut state, actor, phoenix);
                engine::step(&mut state, Action::ChooseEffectBoolean(true)).unwrap();
            }
            let mut replay = restore(&state);
            for branch in [&mut state, &mut replay] {
                assert!(!attack_menu(branch).contains(&phoenix));
                assert_eq!(branch.objects.get(phoenix).zone, Zone::Graveyard);
                // The optional cost is legal even when its return can no
                // longer find this incarnation. A countered trigger pays none.
                assert_eq!(
                    untapped_mountains(branch, actor),
                    before - usize::from(!countered)
                );
                assert_eq!(
                    branch.players[actor.index()].mana_pool[ManaColor::R.pool_index()],
                    0
                );
                assert_eq!(
                    branch.players[actor.index()]
                        .battlefield
                        .iter()
                        .filter(
                            |id| CARD_DEFS[usize::from(branch.objects.get(**id).card_def)].name
                                == "Mountain"
                                && branch.objects.get(**id).tapped
                        )
                        .count(),
                    3 + usize::from(!countered)
                );
            }
            same(&state, &replay);
        }
    }
}

#[test]
fn guarded_payment_rejects_changed_cost_consequence_or_branch_without_mutation_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, phoenix) = paid_and_dead(actor);
        put(&mut state, actor, "Serra Angel", Zone::Battlefield);
        combat_trigger(&mut state, phoenix);
        payment(&mut state, actor, phoenix);
        for change in 0..4 {
            let mut forged = restore(&state);
            let Some(PendingEffectChoice::ChooseBoolean { path, purpose, .. }) = forged
                .engine
                .pending_effect
                .as_mut()
                .unwrap()
                .choice
                .as_mut()
            else {
                panic!("payment")
            };
            let EffectBooleanChoicePurpose::PayManaThen {
                player,
                generic,
                then,
                ..
            } = purpose
            else {
                panic!("purpose")
            };
            match change {
                0 => path.clear(),
                1 => *generic = 1,
                2 => *then = Box::new(EffectOp::Sequence(vec![])),
                _ => *player = actor.opponent(),
            }
            let before = serde_json::to_vec(&forged).unwrap();
            assert!(engine::step(&mut forged, Action::ChooseEffectBoolean(true)).is_err());
            assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
        }
        engine::step(&mut state, Action::ChooseEffectBoolean(true)).unwrap();
        for change in 0..3 {
            let mut forged = restore(&state);
            let pending = forged.engine.pending_effect.as_mut().unwrap();
            let change_frame = |frame: &mut EffectFrame| {
                let EffectFrame::PayManaThen {
                    path,
                    generic,
                    then,
                    ..
                } = frame
                else {
                    panic!("answered payment frame")
                };
                match change {
                    0 => path.clear(),
                    1 => *generic = 1,
                    _ => *then = Box::new(EffectOp::Sequence(vec![])),
                }
            };
            change_frame(pending.frames.last_mut().unwrap());
            let Some(EffectAnsweredChoiceGuard::PayManaThen { frame }) =
                pending.answered_choice_guard.as_mut()
            else {
                panic!("answer guard")
            };
            // Changing both serialized copies must still fail against the
            // actual printed root, rather than pass their equality check.
            change_frame(frame);
            let before = serde_json::to_vec(&forged).unwrap();
            assert!(engine::step(&mut forged, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
        }
        let mut replay = restore(&state);
        settle(&mut state);
        settle(&mut replay);
        same(&state, &replay);
    }
}

#[test]
fn printed_attack_requirement_obeys_tapped_and_defender_constraints_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for defender in [false, true] {
            let (mut state, phoenix) = paid(actor);
            if defender {
                // Existing continuous keyword operand, not a paid producer.
                let timestamp = state.engine.next_effect_timestamp;
                state.engine.next_effect_timestamp += 1;
                state
                    .objects
                    .get_mut(phoenix)
                    .v4
                    .creature_upgrade
                    .get_or_insert_with(Default::default)
                    .keyword_grants
                    .push((Keywords::DEFENDER, timestamp));
            } else {
                state.objects.get_mut(phoenix).tapped = true;
            }
            let mut replay = restore(&state);
            for branch in [&mut state, &mut replay] {
                assert!(!attack_menu(branch).contains(&phoenix));
                engine::step(branch, Action::DeclareAttackers(vec![])).unwrap();
            }
            same(&state, &replay);
        }
    }
}
