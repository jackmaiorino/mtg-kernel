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
    paid_in(ready(actor), actor)
}

fn paid_in(mut state: GameState, actor: PlayerId) -> (GameState, ObjectId) {
    let phoenix = put(&mut state, actor, "Flamewake Phoenix", Zone::Hand);
    let definition = &CARD_DEFS[usize::from(state.objects.get(phoenix).card_def)];
    assert_eq!(definition.capability, CardCapability::Full);
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
    let public = phoenix_v56(&state);
    assert_eq!(public.spells.len(), 1);
    assert_eq!(public.spells[0].source.object, phoenix.0);
    assert_eq!(public.spells[0].source.zone, Zone::Stack);
    assert!(!public.spells[0].is_copy);
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
    dead_in(paid(actor), actor)
}

fn dead_in((mut state, phoenix): (GameState, ObjectId), actor: PlayerId) -> (GameState, ObjectId) {
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
                let facts = phoenix_v56(&state);
                assert!(!facts.ferocious_now[actor.index()]);
                assert_eq!(
                    facts.triggers[0].stage,
                    crate::phoenix_public_v56::StageV56::Published
                );
            }
            let before = untapped_mountains(&state, actor);
            let mut replay = restore(&state);
            for branch in [&mut state, &mut replay] {
                assert!(!attack_menu(branch).contains(&phoenix));
                assert_eq!(branch.objects.get(phoenix).zone, Zone::Graveyard);
                assert_eq!(untapped_mountains(branch, actor), before);
                assert!(!branch.stack.iter().any(|item| item.source == phoenix));
                assert!(phoenix_v56(branch).triggers.is_empty());
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

#[test]
fn genuine_payment_stages_refuse_erased_choice_and_free_consequence_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, phoenix) = paid_and_dead(actor);
        put(&mut state, actor, "Serra Angel", Zone::Battlefield);
        combat_trigger(&mut state, phoenix);
        payment(&mut state, actor, phoenix);
        let Some(PendingEffectChoice::ChooseBoolean {
            purpose: EffectBooleanChoicePurpose::PayManaThen { then, .. },
            ..
        }) = state
            .engine
            .pending_effect
            .as_ref()
            .unwrap()
            .choice
            .as_ref()
        else {
            panic!("genuine optional payment")
        };
        let consequence = then.as_ref().clone();
        for answer in [None, Some(false), Some(true)] {
            let mut checkpoint = restore(&state);
            if let Some(answer) = answer {
                engine::step(&mut checkpoint, Action::ChooseEffectBoolean(answer)).unwrap();
            }
            crate::effect::validate_pending_effect_choice(&checkpoint).unwrap();
            for erase_only in [false, true] {
                let mut forged = restore(&checkpoint);
                let pending = forged.engine.pending_effect.as_mut().unwrap();
                pending.choice = None;
                pending.answered_choice_guard = None;
                pending.frames = if erase_only {
                    vec![]
                } else {
                    vec![EffectFrame::Program {
                        op: consequence.clone(),
                        path: vec![0, 1],
                    }]
                };
                let before = serde_json::to_vec(&forged).unwrap();
                assert!(crate::effect::validate_pending_effect_choice(&forged).is_err());
                assert!(crate::effect::resume_resumable_resolution(&mut forged).is_err());
                assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
                assert_eq!(forged.objects.get(phoenix).zone, Zone::Graveyard);
            }
            let before = untapped_mountains(&checkpoint, actor);
            let mut replay = restore(&checkpoint);
            if answer.is_none() {
                engine::step(&mut checkpoint, Action::ChooseEffectBoolean(false)).unwrap();
                engine::step(&mut replay, Action::ChooseEffectBoolean(false)).unwrap();
            }
            settle(&mut checkpoint);
            settle(&mut replay);
            same(&checkpoint, &replay);
            assert_eq!(
                checkpoint.objects.get(phoenix).zone,
                if answer == Some(true) {
                    Zone::Battlefield
                } else {
                    Zone::Graveyard
                }
            );
            assert_eq!(
                untapped_mountains(&checkpoint, actor),
                before - usize::from(answer == Some(true))
            );
        }
    }
}

fn cast_owned(state: &mut GameState, actor: PlayerId, spell: ObjectId, target: Option<Target>) {
    priority(state, actor);
    engine::step(state, Action::CastSpell(spell)).unwrap();
    if let Some(target) = target {
        assert!(
            matches!(next(state), Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&target))
        );
        engine::step(state, Action::ChooseTarget(target)).unwrap();
    }
    settle(state);
    let permanent = CARD_DEFS[usize::from(state.objects.get(spell).card_def)]
        .types
        .iter()
        .any(|kind| {
            matches!(
                kind,
                CardType::Creature
                    | CardType::Artifact
                    | CardType::Enchantment
                    | CardType::Planeswalker
            )
        });
    assert_eq!(
        state.objects.get(spell).zone,
        if permanent {
            Zone::Battlefield
        } else {
            Zone::Graveyard
        }
    );
}

/// A real paid Fake grant/death/return creates the Treasure. A second paid
/// opposing Bolt kills the returned Phoenix; two paid own Bolts exhaust the
/// remaining Mountains, so the later resolution truly needs that Treasure.
fn paid_dead_phoenix_and_treasure(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
    let (mut state, phoenix) = paid(actor);
    let fake = put(&mut state, actor, "Fake Your Own Death", Zone::Hand);
    for _ in 0..2 {
        let swamp = put(&mut state, actor, "Swamp", Zone::Battlefield);
        priority(&mut state, actor);
        engine::step(&mut state, Action::ActivateManaAbility(swamp)).unwrap();
    }
    cast_owned(&mut state, actor, fake, Some(Target::Object(phoenix)));
    let opposing = state.players[actor.opponent().index()]
        .hand
        .iter()
        .copied()
        .filter(|id| {
            CARD_DEFS[usize::from(state.objects.get(*id).card_def)].name == "Lightning Bolt"
        })
        .collect::<Vec<_>>();
    cast_owned(
        &mut state,
        actor.opponent(),
        opposing[0],
        Some(Target::Object(phoenix)),
    );
    assert_eq!(state.objects.get(phoenix).zone, Zone::Battlefield);
    let treasure = state.players[actor.index()]
        .battlefield
        .iter()
        .copied()
        .find(|id| CARD_DEFS[usize::from(state.objects.get(*id).card_def)].name == "Treasure Token")
        .unwrap();
    assert!(state.objects.get(treasure).v4.is_token);
    cast_owned(
        &mut state,
        actor.opponent(),
        opposing[1],
        Some(Target::Object(phoenix)),
    );
    let own_bolts = state.players[actor.index()]
        .hand
        .iter()
        .copied()
        .filter(|id| {
            CARD_DEFS[usize::from(state.objects.get(*id).card_def)].name == "Lightning Bolt"
        })
        .collect::<Vec<_>>();
    for bolt in own_bolts {
        cast_owned(
            &mut state,
            actor,
            bolt,
            Some(Target::Player(actor.opponent())),
        );
    }
    assert_eq!(state.objects.get(phoenix).zone, Zone::Graveyard);
    assert_eq!(untapped_mountains(&state, actor), 0);
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    assert_eq!(
        state.players[actor.index()]
            .battlefield
            .iter()
            .filter(|id| !engine::available_mana_ability_choices(actor, **id, &state).is_empty())
            .copied()
            .collect::<Vec<_>>(),
        vec![treasure]
    );
    (state, phoenix, treasure)
}

#[test]
fn real_paid_treasure_mana_returns_phoenix_or_retains_declined_cost_with_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for accept in [false, true] {
            let (mut state, phoenix, treasure) = paid_dead_phoenix_and_treasure(actor);
            let unchanged = serde_json::to_vec(&state).unwrap();
            assert!(
                crate::mana::can_pay_owned(&[Pip::Colored(ManaColor::R)], 0, actor, &state)
                    .is_none()
            );
            assert!(engine::can_pay_effect_mana(
                actor,
                &[ManaColor::R],
                0,
                &state
            ));
            assert!(!engine::can_pay_effect_mana(
                actor.opponent(),
                &[ManaColor::R],
                0,
                &state
            ));
            assert_eq!(serde_json::to_vec(&state).unwrap(), unchanged);
            // Existing permanent-entry fixture operand supplies ferocious.
            put(&mut state, actor, "Serra Angel", Zone::Battlefield);
            combat_trigger(&mut state, phoenix);
            payment(&mut state, actor, phoenix);
            let generation = state.objects.get(phoenix).zone_change_count;
            let activation_count = state.engine.mana_ability_activations;
            let prior_activator = state.engine.last_mana_ability_activator;
            let event_start = state.engine.event_history.len();
            let mut replay = restore(&state);
            for branch in [&mut state, &mut replay] {
                engine::step(branch, Action::ChooseEffectBoolean(accept)).unwrap();
                assert_eq!(branch.objects.get(treasure).zone, Zone::Battlefield);
                assert_eq!(branch.engine.mana_ability_activations, activation_count);
            }
            same(&state, &replay);
            replay = restore(&state); // actual accepted/declined typed answer
            for branch in [&mut state, &mut replay] {
                settle(branch);
                assert_eq!(
                    branch.objects.get(phoenix).zone,
                    if accept {
                        Zone::Battlefield
                    } else {
                        Zone::Graveyard
                    }
                );
                assert_eq!(
                    branch.objects.get(phoenix).zone_change_count,
                    generation + u32::from(accept)
                );
                assert_eq!(
                    branch.engine.mana_ability_activations,
                    activation_count + u64::from(accept)
                );
                assert_eq!(
                    branch.engine.last_mana_ability_activator,
                    if accept { Some(actor) } else { prior_activator }
                );
                assert_eq!(branch.players[actor.index()].mana_pool, [0; 6]);
                let events = &branch.engine.event_history[event_start..];
                let sacrificed = events
                    .iter()
                    .filter(|e| {
                        matches!(e,
                    CommittedEvent::Sacrificed { object, controller_before, .. }
                    if *object == treasure && *controller_before == actor)
                    })
                    .count();
                assert_eq!(sacrificed, usize::from(accept));
                if accept {
                    let sacrifice = events.iter().position(|e| matches!(e, CommittedEvent::Sacrificed { object, .. } if *object == treasure)).unwrap();
                    let mana = events.iter().position(|e| matches!(e, CommittedEvent::ManaAdded { player, colors } if *player == actor && *colors == [ManaColor::R])).unwrap();
                    let returned = events.iter().position(|e| matches!(e, CommittedEvent::ZoneChange { object, to: Zone::Battlefield, .. } if *object == phoenix)).unwrap();
                    assert!(sacrifice < mana && mana < returned);
                    assert_ne!(branch.objects.get(treasure).zone, Zone::Battlefield);
                } else {
                    assert_eq!(branch.objects.get(treasure).zone, Zone::Battlefield);
                    assert!(!branch.objects.get(treasure).tapped);
                }
            }
            same(&state, &replay);
        }
    }
}

#[test]
fn two_real_paid_phoenixes_order_in_the_active_apnap_group_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, first) = paid_and_dead(actor);
        let second = put(&mut state, actor, "Flamewake Phoenix", Zone::Hand);
        for _ in 0..3 {
            let land = put(&mut state, actor, "Mountain", Zone::Battlefield);
            priority(&mut state, actor);
            engine::step(&mut state, Action::ActivateManaAbility(land)).unwrap();
        }
        cast_owned(&mut state, actor, second, None);
        let bolt = state.players[actor.opponent().index()]
            .hand
            .iter()
            .copied()
            .find(|id| {
                CARD_DEFS[usize::from(state.objects.get(*id).card_def)].name == "Lightning Bolt"
            })
            .unwrap();
        cast_owned(
            &mut state,
            actor.opponent(),
            bolt,
            Some(Target::Object(second)),
        );
        put(&mut state, actor, "Serra Angel", Zone::Battlefield);
        let mut found = false;
        for _ in 0..32 {
            match next(&mut state) {
                Decision::OrderTriggers { player, pending } => {
                    assert_eq!(player, actor);
                    assert_eq!(pending.len(), 2);
                    let facts = phoenix_v56(&state);
                    assert_eq!(facts.triggers.len(), 2);
                    assert!(facts.triggers.iter().all(|row| matches!(
                        row.stage,
                        crate::phoenix_public_v56::StageV56::Held {
                            placement_ordered: false
                        }
                    )));
                    assert!(state
                        .engine
                        .pending_triggers
                        .iter()
                        .all(|t| t.controller == actor
                            && [first, second].contains(&t.source)
                            && t.source_contract.is_some_and(|s| s.zone == Zone::Graveyard
                                && s.controller == actor
                                && s.owner == actor
                                && s.zone_change_count
                                    == state.objects.get(t.source).zone_change_count)));
                    found = true;
                    break;
                }
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                other => panic!("actual two-Phoenix ordering:{other:?}"),
            }
        }
        assert!(found);
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            engine::step(branch, Action::OrderTriggers(vec![1, 0])).unwrap();
            let before = untapped_mountains(branch, actor);
            let mut answered = Vec::new();
            for _ in 0..48 {
                match next(branch) {
                    Decision::ChooseEffectBoolean { player, source, .. } => {
                        assert_eq!(player, actor);
                        assert!([first, second].contains(&source));
                        assert!(!answered.contains(&source));
                        answered.push(source);
                        engine::step(branch, Action::ChooseEffectBoolean(true)).unwrap();
                    }
                    Decision::CastSpellOrPass { .. } if branch.stack.is_empty() => break,
                    Decision::CastSpellOrPass { .. } => engine::step(branch, Action::Pass).unwrap(),
                    other => panic!("actual two-Phoenix resolution:{other:?}"),
                }
            }
            assert_eq!(answered.len(), 2);
            assert_eq!(branch.objects.get(first).zone, Zone::Battlefield);
            assert_eq!(branch.objects.get(second).zone, Zone::Battlefield);
            assert_eq!(untapped_mountains(branch, actor), before - 2);
        }
        same(&state, &replay);
    }
}

#[test]
fn real_paid_control_suppression_and_token_copy_do_not_change_graveyard_owner_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..3 {
            let (mut state, phoenix) = paid(actor);
            let other = actor.opponent();
            // Existing permanent-entry support, not an invented spell producer.
            put(&mut state, other, "High Fae Trickster", Zone::Battlefield);
            let spell_name = match mode {
                0 => "Involuntary Employment",
                1 => "Witness Protection",
                _ => "Self-Reflection",
            };
            let caster = if mode == 2 { actor } else { other };
            let spell = put(&mut state, caster, spell_name, Zone::Hand);
            for _ in 0..6 {
                put(
                    &mut state,
                    caster,
                    if mode == 0 { "Mountain" } else { "Island" },
                    Zone::Battlefield,
                );
            }
            cast_owned(&mut state, caster, spell, Some(Target::Object(phoenix)));
            let public = crate::phoenix_public_v56::capture(&state).unwrap();
            crate::phoenix_public_v56::validate(&state, &public).unwrap();
            let current = public
                .current
                .iter()
                .find(|row| row.source.object == phoenix.0)
                .unwrap();
            if mode == 0 {
                assert_eq!(state.objects.get(phoenix).controller, other);
                assert_eq!(current.source.controller, other.into());
                assert_eq!(current.source.owner, actor.into());
                assert!(current.flying && current.haste && current.attacks_each_combat_if_able);
            } else if mode == 1 {
                assert!(
                    !current.printed_abilities_active
                        && !current.flying
                        && !current.haste
                        && !current.attacks_each_combat_if_able
                );
                assert_eq!((current.power, current.toughness), (Some(1), Some(1)));
                assert!(
                    !crate::continuous_characteristics_v1::printed_abilities_active(
                        &state, phoenix
                    )
                );
                assert!(!engine::has_effective_keyword(
                    &state,
                    phoenix,
                    Keywords::FLYING
                ));
                assert!(!engine::has_effective_keyword(
                    &state,
                    phoenix,
                    Keywords::HASTE
                ));
            } else {
                let copy = state.players[actor.index()]
                    .battlefield
                    .iter()
                    .copied()
                    .find(|id| {
                        let live = state.objects.get(*id);
                        live.card_def == state.objects.get(phoenix).card_def && live.v4.is_token
                    })
                    .unwrap();
                let copy_row = public
                    .current
                    .iter()
                    .find(|row| row.source.object == copy.0)
                    .unwrap();
                assert!(
                    copy_row.token
                        && copy_row.flying
                        && copy_row.haste
                        && copy_row.attacks_each_combat_if_able
                );
                assert!(engine::has_effective_keyword(
                    &state,
                    copy,
                    Keywords::FLYING
                ));
                let bolt = state.players[other.index()]
                    .hand
                    .iter()
                    .copied()
                    .find(|id| {
                        CARD_DEFS[usize::from(state.objects.get(*id).card_def)].name
                            == "Lightning Bolt"
                    })
                    .unwrap();
                cast_owned(&mut state, other, bolt, Some(Target::Object(copy)));
                assert!(!state.players[actor.index()].graveyard.contains(&copy));
                assert!(!state.players[actor.index()].battlefield.contains(&copy));
            }
            // Pay the real opposing kill even when Employment spent its lands.
            put(&mut state, other, "Mountain", Zone::Battlefield);
            let bolt = state.players[other.index()]
                .hand
                .iter()
                .copied()
                .find(|id| {
                    CARD_DEFS[usize::from(state.objects.get(*id).card_def)].name == "Lightning Bolt"
                })
                .unwrap();
            cast_owned(&mut state, other, bolt, Some(Target::Object(phoenix)));
            assert_eq!(state.objects.get(phoenix).zone, Zone::Graveyard);
            assert_eq!(state.objects.get(phoenix).controller, actor);
            assert!(
                crate::continuous_characteristics_v1::printed_abilities_active(&state, phoenix)
            );
            put(&mut state, actor, "Serra Angel", Zone::Battlefield);
            combat_trigger(&mut state, phoenix);
            assert_eq!(
                state
                    .stack
                    .iter()
                    .filter(|item| item.kind == StackItemKind::TriggeredAbility
                        && item
                            .v4
                            .ability_source_contract
                            .is_some_and(
                                |s| CARD_DEFS[usize::from(s.card_def)].name == "Flamewake Phoenix"
                            ))
                    .count(),
                1
            );
            // A fresh Mountain is a primitive support resource; actual {R}
            // payment is made by the same generic engine path as ordinary play.
            put(&mut state, actor, "Mountain", Zone::Battlefield);
            payment(&mut state, actor, phoenix);
            let mut replay = restore(&state);
            for branch in [&mut state, &mut replay] {
                engine::step(branch, Action::ChooseEffectBoolean(true)).unwrap();
                settle(branch);
                assert_eq!(branch.objects.get(phoenix).zone, Zone::Battlefield);
                assert_eq!(branch.objects.get(phoenix).controller, actor);
                assert!(engine::has_effective_keyword(
                    branch,
                    phoenix,
                    Keywords::FLYING
                ));
                assert!(engine::has_effective_keyword(
                    branch,
                    phoenix,
                    Keywords::HASTE
                ));
            }
            same(&state, &replay);
        }
    }
}

fn phoenix_v56(state: &GameState) -> crate::phoenix_public_v56::FactsV56 {
    let before = serde_json::to_vec(state).unwrap();
    let facts = crate::phoenix_public_v56::capture(state).unwrap();
    crate::phoenix_public_v56::validate(state, &facts).unwrap();
    assert_eq!(
        crate::phoenix_public_v56::capture(&restore(state)).unwrap(),
        facts
    );
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
    let text = serde_json::to_string(&facts).unwrap();
    for private in [
        "EffectOp",
        "private_snapshot",
        "event_history",
        "operation_world",
        "canonical_path",
        "stack_item_id",
        "source_contract",
        "answered_choice_guard",
    ] {
        assert!(!text.contains(private), "private field {private}");
    }
    facts
}

#[test]
fn v56_paid_rods_grant_phoenix_damage_without_a_return_root_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, phoenix) = paid(actor);
        let mut rods = Vec::new();
        // Lands are primitive support inputs. Both Rod casts, their native Job
        // Select attachments, Equip {3}, and the later spell are actually paid.
        for _ in 0..2 {
            let rod = put(&mut state, actor, "Black Mage's Rod", Zone::Hand);
            for _ in 0..2 {
                let swamp = put(&mut state, actor, "Swamp", Zone::Battlefield);
                priority(&mut state, actor);
                engine::step(&mut state, Action::ActivateManaAbility(swamp)).unwrap();
            }
            cast_owned(&mut state, actor, rod, None);
            let hero = state.objects.get(rod).v4.attached_to.unwrap().object;
            assert_ne!(hero, phoenix);
            assert!(state.objects.get(hero).v4.is_token);
            for _ in 0..3 {
                let mountain = put(&mut state, actor, "Mountain", Zone::Battlefield);
                priority(&mut state, actor);
                engine::step(&mut state, Action::ActivateManaAbility(mountain)).unwrap();
            }
            engine::step(&mut state, Action::ActivateAbility(rod, 0)).unwrap();
            assert!(
                matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(phoenix)))
            );
            engine::step(&mut state, Action::ChooseTarget(Target::Object(phoenix))).unwrap();
            settle(&mut state);
            assert_eq!(
                state.objects.get(rod).v4.attached_to.unwrap().object,
                phoenix
            );
            rods.push(rod);
        }
        assert_eq!(state.objects.get(phoenix).attachments, rods);
        let spell = put(&mut state, actor, "Think Twice", Zone::Hand);
        for _ in 0..2 {
            let island = put(&mut state, actor, "Island", Zone::Battlefield);
            priority(&mut state, actor);
            engine::step(&mut state, Action::ActivateManaAbility(island)).unwrap();
        }
        let life = state.players[actor.opponent().index()].life;
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        assert!(
            matches!(next(&mut state), Decision::OrderTriggers { player, pending }
            if player == actor && pending.len() == 2)
        );
        assert_eq!(state.engine.pending_triggers.len(), 2);
        assert!(state.engine.pending_triggers.iter().all(|pending| {
            pending.source == phoenix
                && pending
                    .source_contract
                    .is_some_and(|source| source.zone == Zone::Battlefield)
                && pending
                    .granted_by
                    .is_some_and(|grant| rods.contains(&grant.source))
        }));
        let facts = phoenix_v56(&state);
        assert!(facts.triggers.is_empty());
        let current = facts
            .current
            .iter()
            .find(|row| row.source.object == phoenix.0)
            .unwrap();
        assert_eq!(current.power, Some(4));
        assert!(Subtype::Wizard.is_in_subtype_ids(&current.effective_subtype_ids));
        assert!(facts.ferocious_now[actor.index()]);
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            engine::step(branch, Action::OrderTriggers(vec![1, 0])).unwrap();
            assert!(matches!(next(branch), Decision::CastSpellOrPass { .. }));
            let grants = branch
                .stack
                .iter()
                .filter(|item| {
                    item.kind == StackItemKind::TriggeredAbility && item.source == phoenix
                })
                .collect::<Vec<_>>();
            assert_eq!(grants.len(), 2);
            assert!(grants.iter().all(|item| item
                .v4
                .granted_by
                .is_some_and(|grant| rods.contains(&grant.source))));
            assert!(phoenix_v56(branch).triggers.is_empty());
            let published_restore = restore(branch);
            assert_eq!(phoenix_v56(&published_restore), phoenix_v56(branch));
            settle(branch);
            assert_eq!(branch.players[actor.opponent().index()].life, life - 2);
            assert_eq!(branch.objects.get(spell).zone, Zone::Graveyard);
            assert_eq!(branch.objects.get(phoenix).zone, Zone::Battlefield);
            assert!(phoenix_v56(branch).triggers.is_empty());
        }
        same(&state, &replay);
    }
}

#[test]
fn v56_paid_current_published_payment_answer_and_completed_absence_restore_both_seats() {
    use crate::phoenix_public_v56::StageV56;
    for actor in [PlayerId::P0, PlayerId::P1] {
        for accept in [false, true] {
            let (mut state, phoenix) = paid_and_dead(actor);
            put(&mut state, actor, "Serra Angel", Zone::Battlefield);
            let initial = phoenix_v56(&state);
            assert!(initial.ferocious_now[actor.index()]);
            assert!(initial.triggers.is_empty());
            assert!(initial
                .current
                .iter()
                .any(|r| r.source.object == phoenix.0 && r.graveyard_ability_functions_here));
            combat_trigger(&mut state, phoenix);
            let published = phoenix_v56(&state);
            assert_eq!(published.triggers.len(), 1);
            assert_eq!(published.triggers[0].stage, StageV56::Published);
            assert_eq!(published.triggers[0].original_actor, actor.into());
            payment(&mut state, actor, phoenix);
            let offered = phoenix_v56(&state);
            assert_eq!(
                offered.triggers[0].stage,
                StageV56::PaymentOffered { can_pay: true }
            );
            let before = untapped_mountains(&state, actor);
            let mut replay = restore(&state);
            for branch in [&mut state, &mut replay] {
                engine::step(branch, Action::ChooseEffectBoolean(accept)).unwrap();
                let answered = phoenix_v56(branch);
                assert_eq!(
                    answered.triggers[0].stage,
                    if accept {
                        StageV56::AcceptedAwaitingPayment
                    } else {
                        StageV56::DeclinedAwaitingCompletion
                    }
                );
                assert_eq!(untapped_mountains(branch, actor), before);
                assert_eq!(branch.objects.get(phoenix).zone, Zone::Graveyard);
            }
            same(&state, &replay);
            replay = restore(&state);
            for branch in [&mut state, &mut replay] {
                settle(branch);
                let public = phoenix_v56(branch);
                // The current API does not invent a completed return occurrence.
                assert!(public.triggers.is_empty());
                let row = public
                    .current
                    .iter()
                    .find(|r| r.source.object == phoenix.0)
                    .unwrap();
                assert_eq!(
                    row.source.zone,
                    if accept {
                        Zone::Battlefield
                    } else {
                        Zone::Graveyard
                    }
                );
                assert_eq!(row.source.controller, actor.into());
                assert_eq!(
                    untapped_mountains(branch, actor),
                    before - usize::from(accept)
                );
                assert_eq!(row.attacks_each_combat_if_able, accept);
            }
            same(&state, &replay);
        }
    }
}
#[test]
fn v56_offered_payment_keeps_original_incarnation_and_does_not_recheck_ferocious_both_seats() {
    use crate::phoenix_public_v56::StageV56;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, phoenix) = paid_and_dead(actor);
        let four = put(&mut state, actor, "Serra Angel", Zone::Battlefield);
        combat_trigger(&mut state, phoenix);
        payment(&mut state, actor, phoenix);
        let original = phoenix_v56(&state).triggers[0].original_source.clone();
        // Primitive zone operands at a held payment boundary. This does not
        // claim that a spell response can occur in the middle of resolution.
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(four, Zone::Hand));
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(phoenix, Zone::Exile));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(phoenix, Zone::Graveyard),
        );
        let offered = phoenix_v56(&state);
        assert!(!offered.ferocious_now[actor.index()]);
        assert_eq!(offered.triggers[0].original_source, original);
        assert!(offered.triggers[0].current_original_source.is_none());
        assert_eq!(
            offered.triggers[0].stage,
            StageV56::PaymentOffered { can_pay: true }
        );
        let before = untapped_mountains(&state, actor);
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            engine::step(branch, Action::ChooseEffectBoolean(true)).unwrap();
            assert_eq!(
                phoenix_v56(branch).triggers[0].stage,
                StageV56::AcceptedAwaitingPayment
            );
            settle(branch);
            assert_eq!(branch.objects.get(phoenix).zone, Zone::Graveyard);
            assert_eq!(untapped_mountains(branch, actor), before - 1);
            assert!(phoenix_v56(branch).triggers.is_empty());
        }
        same(&state, &replay);
    }
}
#[test]
fn v56_genuine_paid_prompt_and_answer_forgeries_refuse_before_publication_both_seats() {
    use crate::phoenix_public_v56::{self as public, StageV56};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, phoenix) = paid_and_dead(actor);
        put(&mut state, actor, "Serra Angel", Zone::Battlefield);
        combat_trigger(&mut state, phoenix);
        payment(&mut state, actor, phoenix);
        let real = phoenix_v56(&state);
        let original = serde_json::to_vec(&state).unwrap();
        for change in 0..5 {
            let mut bad = restore(&state);
            let pending = bad.engine.pending_effect.as_mut().unwrap();
            match change {
                0 => pending.choice = None,
                1 => pending.ctx.controller = actor.opponent(),
                2 => pending.resolving_item.inline_effect = Some(EffectOp::Sequence(vec![])),
                3 => {
                    let Some(PendingEffectChoice::ChooseBoolean { path, .. }) = &mut pending.choice
                    else {
                        unreachable!()
                    };
                    path.clear();
                }
                _ => {
                    let Some(PendingEffectChoice::ChooseBoolean {
                        purpose: EffectBooleanChoicePurpose::PayManaThen { generic, .. },
                        ..
                    }) = &mut pending.choice
                    else {
                        unreachable!()
                    };
                    *generic = 1;
                }
            }
            let bytes = serde_json::to_vec(&bad).unwrap();
            assert!(public::capture(&bad).is_err());
            assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
            assert_eq!(serde_json::to_vec(&state).unwrap(), original);
        }
        let mut changed = real;
        changed.triggers[0].stage = StageV56::Published;
        assert!(public::validate(&state, &changed).is_err());
        engine::step(&mut state, Action::ChooseEffectBoolean(true)).unwrap();
        let mut erased = restore(&state);
        erased
            .engine
            .pending_effect
            .as_mut()
            .unwrap()
            .answered_choice_guard = None;
        let bytes = serde_json::to_vec(&erased).unwrap();
        assert!(public::capture(&erased).is_err());
        assert_eq!(serde_json::to_vec(&erased).unwrap(), bytes);
        let mut changed = phoenix_v56(&state);
        changed.triggers[0].original_source.generation += 1;
        assert!(public::validate(&state, &changed).is_err());
        let value = serde_json::to_value(phoenix_v56(&state)).unwrap();
        let mut injected = value.clone();
        injected["private_snapshot"] = value;
        assert!(serde_json::from_value::<public::FactsV56>(injected).is_err());
    }
}
#[test]
fn v56_independent_hidden_printings_and_face_down_forms_preserve_paid_payment_facts_both_seats() {
    use crate::state::FaceDownV1;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let build = |printing: &str| {
            let mut initial = ready(actor);
            put(&mut initial, actor.opponent(), printing, Zone::Hand);
            put(&mut initial, actor.opponent(), printing, Zone::Library);
            // A concealed 2/2 initial permanent is a primitive fixture input.
            let face = put(&mut initial, actor.opponent(), printing, Zone::Battlefield);
            initial.objects.get_mut(face).v4.face_down_v1 = Some(FaceDownV1 {
                disguised: true,
                lookers: 0,
                hidden_by: None,
                hidden_by_source: None,
            });
            let (mut state, phoenix) = dead_in(paid_in(initial, actor), actor);
            put(&mut state, actor, "Serra Angel", Zone::Battlefield);
            combat_trigger(&mut state, phoenix);
            payment(&mut state, actor, phoenix);
            let facts = phoenix_v56(&state);
            assert!(!facts.current.iter().any(|r| r.source.object == face.0));
            (facts, serde_json::to_vec(&state).unwrap())
        };
        let a = build("Flamewake Phoenix");
        let b = build("Llanowar Elves");
        assert_eq!(a.0, b.0);
        assert_ne!(a.1, b.1);
    }
}

#[test]
fn v56_native_width_public_power_and_overflow_refusal_preserve_primitive_input_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, phoenix) = paid(actor);
        // Exact counter values are primitive inputs, not printed payment outcomes.
        state.objects.get_mut(phoenix).counters.plus1_plus1 = 65537;
        let facts = phoenix_v56(&state);
        let row = facts
            .current
            .iter()
            .find(|r| r.source.object == phoenix.0)
            .unwrap();
        assert_eq!((row.power, row.toughness), (Some(65539), Some(65539)));
        state.objects.get_mut(phoenix).counters.plus1_plus1 = i32::MAX;
        let before = serde_json::to_vec(&state).unwrap();
        assert!(crate::phoenix_public_v56::capture(&state).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }
}
