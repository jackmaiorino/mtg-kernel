//! Genuine native paid Curator fixtures. Initial libraries are declared before
//! the first action; control/zone primitives are labelled where used.
use super::*;
use crate::{
    engine::{self, Action, Decision},
    event::CommittedEvent,
    policy_observation_v6::tests::put,
    state::Step,
};
const NAME: &str = "Curator of Destinies";
fn id(name: &str) -> u16 {
    card_def::card_id_by_name(name).unwrap()
}
fn ready(actor: PlayerId, library: &[&str]) -> GameState {
    let cards = library.iter().map(|n| id(n)).collect::<Vec<_>>();
    let other = [id("Forest"); 32];
    let (a, b) = if actor == PlayerId::P0 {
        (cards.as_slice(), other.as_slice())
    } else {
        (other.as_slice(), cards.as_slice())
    };
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        a,
        b,
        crate::rl::card_name,
        110_472,
        actor,
    );
    s.step = Step::Main1;
    for player in [actor, actor.opponent()] {
        for name in ["Island", "Mountain"] {
            for _ in 0..10 {
                put(&mut s, player, name, Zone::Battlefield);
            }
        }
    }
    s
}
fn restore(s: &GameState) -> GameState {
    let copy: GameState = serde_json::from_slice(&serde_json::to_vec(s).unwrap()).unwrap();
    assert_eq!(s, &copy);
    assert_eq!(s.state_hash(), copy.state_hash());
    copy
}
fn ready_native_handoff(s: &GameState) {
    let item = s
        .engine
        .pending_effect
        .as_ref()
        .unwrap()
        .resolving_item
        .clone();
    let before = s.clone();
    assert!(acquire_tutelage_resume_v1(s, &item).unwrap().is_some());
    assert_eq!(*s, before);
    for erase in [false, true] {
        let mut bad = restore(s);
        let owner = acquire_tutelage_resume_v1(&bad, &item).unwrap().unwrap();
        let position =
            crate::engine::resolution_cast_v1::resolving_index(&bad, item.v4.stack_item_id)
                .unwrap();
        assert_eq!(bad.stack.remove(position), item);
        if erase {
            bad.engine.pending_effect = None;
        } else {
            bad.engine.pending_effect.as_mut().unwrap().ctx.controller = item.controller.opponent();
        }
        let before = bad.clone();
        assert!(resume_owned_tutelage_v1(&mut bad, owner).is_err());
        assert_eq!(bad, before);
    }
}
fn next(s: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(s);
    assert!(s.engine.halted.is_none(), "{:?}", s.engine.halted);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}
fn pass(s: &mut GameState, d: Decision) {
    let a = match d {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        d => panic!("native Curator decision {d:?}"),
    };
    engine::step(s, a).unwrap()
}
fn priority(s: &mut GameState, p: PlayerId) {
    for _ in 0..20 {
        let d = next(s);
        if matches!(d,Decision::CastSpellOrPass{player,..}if player==p) {
            return;
        }
        pass(s, d)
    }
    panic!("priority absent")
}
fn tapped(s: &GameState, p: PlayerId) -> usize {
    s.players[p.index()]
        .battlefield
        .iter()
        .filter(|id| s.objects.get(**id).tapped)
        .count()
}
fn cast(s: &mut GameState, p: PlayerId, name: &str, targets: &[Target], cost: usize) -> ObjectId {
    let card = put(s, p, name, Zone::Hand);
    priority(s, p);
    let before = tapped(s, p);
    assert!(
        matches!(next(s),Decision::CastSpellOrPass{castable_spells,..}if castable_spells.contains(&card))
    );
    engine::step(s, Action::CastSpell(card)).unwrap();
    let mut chosen = 0;
    for _ in 0..16 {
        match next(s) {
            Decision::ChooseTargets {
                player,
                legal_targets,
                ..
            } => {
                assert_eq!(player, p);
                assert!(legal_targets.contains(&targets[chosen]));
                engine::step(s, Action::ChooseTarget(targets[chosen])).unwrap();
                chosen += 1
            }
            Decision::CastSpellOrPass { player, .. } => {
                assert_eq!(player, p);
                assert_eq!(chosen, targets.len());
                assert!(s.engine.pending_cast.is_none());
                assert_eq!(tapped(s, p) - before, cost);
                assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                assert!(s
                    .stack
                    .iter()
                    .any(|r| r.source == card && r.kind == StackItemKind::Spell));
                return card;
            }
            d => panic!("paid cast {d:?}"),
        }
    }
    panic!("paid cast did not publish")
}
fn resolve_response(s: &mut GameState, source: ObjectId) {
    for _ in 0..24 {
        let d = next(s);
        if s.objects.get(source).zone == Zone::Graveyard && s.engine.pending_effect.is_none() {
            return;
        }
        pass(s, d)
    }
    panic!("response absent")
}
fn paid_to_trigger(s: &mut GameState, p: PlayerId) -> (ObjectId, StackItem) {
    let card = cast(s, p, NAME, &[], 6);
    for _ in 0..24 {
        let d = next(s);
        if s.objects.get(card).zone == Zone::Battlefield {
            let row = s
                .stack
                .iter()
                .find(|r| {
                    r.source == card
                        && matches!(r.inline_effect, Some(EffectOp::CuratorDestiniesV1 { .. }))
                })
                .unwrap()
                .clone();
            assert_eq!(row.controller, p);
            assert_eq!(
                row.v4.ability_source_contract.unwrap().zone_change_count,
                s.objects.get(card).zone_change_count
            );
            assert!(matches!(d, Decision::CastSpellOrPass { .. }));
            return (card, row);
        }
        pass(s, d)
    }
    panic!("Curator did not enter")
}
fn partition_prompt(s: &mut GameState, p: PlayerId) -> Vec<ObjectId> {
    for _ in 0..24 {
        let d = next(s);
        if let Decision::ChooseEffectTargets {
            player,
            min_targets,
            max_targets,
            legal_targets,
            can_finish,
            ..
        } = d
        {
            assert_eq!(player, p);
            assert_eq!(min_targets, 0);
            assert_eq!(max_targets as usize, legal_targets.len());
            assert!(can_finish);
            validate_pending_effect_choice(s).unwrap();
            return legal_targets
                .iter()
                .map(|t| match t {
                    Target::Object(id) => *id,
                    _ => panic!(),
                })
                .collect();
        }
        pass(s, d)
    }
    panic!("private partition absent")
}
fn partition(s: &mut GameState, down: &[ObjectId]) {
    for object in down {
        let d = next(s);
        assert!(
            matches!(d,Decision::ChooseEffectTargets{legal_targets,..}if legal_targets.contains(&Target::Object(*object)))
        );
        engine::step(s, Action::ChooseEffectTarget(Target::Object(*object))).unwrap();
    }
    if s.engine.pending_effect.as_ref().unwrap().choice.is_some() {
        engine::step(s, Action::FinishEffectSelection).unwrap();
    }
    assert!(s.engine.pending_effect.as_ref().unwrap().choice.is_none());
    assert!(s
        .engine
        .pending_effect
        .as_ref()
        .unwrap()
        .answered_choice_guard
        .is_some());
    validate_pending_effect_choice(s).unwrap();
}
fn pile_prompt(s: &mut GameState, p: PlayerId, down: usize, up: usize) -> Decision {
    let d = next(s);
    assert!(
        matches!(d,Decision::ChooseEffectBoolean{player,default:Some(false),purpose:EffectBooleanChoicePurpose::CuratorPileV1{face_down_count,face_up_count,..},..}if player==p.opponent()&&usize::from(face_down_count)==down&&usize::from(face_up_count)==up)
    );
    validate_pending_effect_choice(s).unwrap();
    d
}
fn finish(s: &mut GameState) -> Vec<ObjectId> {
    let mut ordered = Vec::new();
    for _ in 0..64 {
        let d = next(s);
        match d {
            Decision::ChooseEffectTargets { legal_targets, .. } => {
                let Target::Object(object) = *legal_targets.last().unwrap() else {
                    panic!()
                };
                ordered.push(object);
                engine::step(s, Action::ChooseEffectTarget(Target::Object(object))).unwrap();
            }
            Decision::CastSpellOrPass { .. }
                if s.stack.is_empty() && s.engine.pending_effect.is_none() =>
            {
                return ordered
            }
            d => pass(s, d),
        }
    }
    panic!("Curator final moves absent")
}
fn resolve_piles(s: &mut GameState, p: PlayerId, down: &[ObjectId], choose_down: bool) {
    partition(s, down);
    let n = s.engine.pending_effect.as_ref().unwrap();
    assert!(n.choice.is_none());
    let mut copy = restore(s);
    let total = bind_library_top(s, p, 5).len();
    let d = pile_prompt(s, p, down.len(), total - down.len());
    assert_eq!(pile_prompt(&mut copy, p, down.len(), total - down.len()), d);
    assert_eq!(s, &copy);
    for b in [&mut *s, &mut copy] {
        engine::step(b, Action::ChooseEffectBoolean(choose_down)).unwrap();
        validate_pending_effect_choice(b).unwrap();
    }
    assert_eq!(s, &copy);
    let ordered = finish(s);
    assert_eq!(finish(&mut copy), ordered);
    assert_eq!(s, &copy);
}
#[test]
fn genuine_paid_complete_printing_uncounterable_and_zero_to_five_piles_restore_both_seats() {
    assert_eq!(id(NAME), 472);
    assert!(definition(472));
    assert!(card_def::preflight_fully_supported_deck(&[472]).is_err());
    let names = [
        "Forest",
        "Island",
        "Llanowar Elves",
        "Mountain",
        "Healer's Hawk",
    ];
    for actor in [PlayerId::P0, PlayerId::P1] {
        for count in 0..=5 {
            for choose_down in [false, true] {
                let mut s = ready(actor, &names[..count]);
                let (card, _) = paid_to_trigger(&mut s, actor);
                assert_eq!(s.objects.get(card).zone, Zone::Battlefield);
                assert!(engine::has_effective_keyword(&s, card, Keywords::FLYING));
                let prefix = partition_prompt(&mut s, actor);
                assert_eq!(prefix.len(), count);
                let down = prefix.iter().step_by(2).copied().collect::<Vec<_>>();
                resolve_piles(&mut s, actor, &down, choose_down);
                for object in &prefix {
                    let expected = if down.contains(object) == choose_down {
                        Zone::Hand
                    } else {
                        Zone::Graveyard
                    };
                    assert_eq!(s.objects.get(*object).zone, expected);
                    assert_eq!(s.objects.get(*object).owner, actor);
                }
                assert!(s.players[actor.index()].library.is_empty());
                assert!(crate::rl::observe_v1(&s, actor, 0).is_err());
            }
        }
    }
}
#[test]
fn genuine_paid_counterspell_cannot_counter_curator_but_native_trigger_counter_prevents_look_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor, &["Forest", "Island", "Mountain"]);
        let original = s.players[actor.index()].library.clone();
        let card = cast(&mut s, actor, NAME, &[], 6);
        let counter = cast(
            &mut s,
            actor.opponent(),
            "Counterspell",
            &[Target::Object(card)],
            2,
        );
        resolve_response(&mut s, counter);
        assert_eq!(s.objects.get(card).zone, Zone::Stack);
        for _ in 0..20 {
            let d = next(&mut s);
            if s.objects.get(card).zone == Zone::Battlefield {
                break;
            }
            pass(&mut s, d)
        }
        let row = s
            .stack
            .iter()
            .find(|r| matches!(r.inline_effect, Some(EffectOp::CuratorDestiniesV1 { .. })))
            .unwrap()
            .clone();
        let mut copy = restore(&s);
        for b in [&mut s, &mut copy] {
            // Native ability-counter primitive; no paid ability-counter producer is claimed.
            engine::counter_stack_item_by_id(b, row.v4.stack_item_id).unwrap();
            finish(b);
            assert_eq!(b.players[actor.index()].library, original);
            assert!(b.known_library_cards(actor, actor).is_empty());
            assert!(engine::validated_stack_item_target_spec(&row, b).is_err());
        }
        assert_eq!(s, copy);
    }
}
#[test]
fn genuine_paid_face_up_only_opponent_knowledge_and_private_pile_boolean_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for down_count in [0usize, 2, 5] {
            let mut s = ready(
                actor,
                &[
                    "Forest",
                    "Island",
                    "Llanowar Elves",
                    "Mountain",
                    "Healer's Hawk",
                    "Forest",
                ],
            );
            paid_to_trigger(&mut s, actor);
            let cards = partition_prompt(&mut s, actor);
            assert_eq!(s.known_library_cards(actor, actor).len(), 5);
            assert!(s.known_library_cards(actor.opponent(), actor).is_empty());
            let down = &cards[..down_count];
            partition(&mut s, down);
            let answered = restore(&s);
            let d = pile_prompt(&mut s, actor, down_count, 5 - down_count);
            let mut restored = restore(&answered);
            assert_eq!(
                pile_prompt(&mut restored, actor, down_count, 5 - down_count),
                d
            );
            assert_eq!(s, restored);
            let known = s
                .known_library_cards(actor.opponent(), actor)
                .iter()
                .map(|k| k.object)
                .collect::<Vec<_>>();
            assert_eq!(known, cards[down_count..]);
            if let Decision::ChooseEffectBoolean { purpose, .. } = &d {
                let wire = serde_json::to_value(purpose).unwrap();
                let object = wire.get("CuratorPileV1").unwrap().as_object().unwrap();
                assert_eq!(
                    object
                        .keys()
                        .cloned()
                        .collect::<std::collections::BTreeSet<_>>(),
                    ["face_down_count", "face_up_count"]
                        .iter()
                        .map(|s| s.to_string())
                        .collect()
                );
            } else {
                panic!()
            }
            // A second genuine initial world changes only the private pile's
            // printings, then executes the same paid native prefix.
            let mut alternative = [
                "Forest",
                "Island",
                "Llanowar Elves",
                "Mountain",
                "Healer's Hawk",
                "Forest",
            ];
            alternative[..down_count].fill("Swamp");
            let mut hidden_world = ready(actor, &alternative);
            paid_to_trigger(&mut hidden_world, actor);
            let other_cards = partition_prompt(&mut hidden_world, actor);
            assert_eq!(other_cards, cards);
            partition(&mut hidden_world, &other_cards[..down_count]);
            assert_eq!(
                pile_prompt(&mut hidden_world, actor, down_count, 5 - down_count),
                d
            );
            assert_eq!(
                hidden_world.known_library_cards(actor.opponent(), actor),
                s.known_library_cards(actor.opponent(), actor)
            );
            for object in &cards[down_count..] {
                assert_eq!(hidden_world.objects.get(*object), s.objects.get(*object));
            }
            assert!(crate::optional_trigger_public_v1::validate_state_checked(&s).is_err());
            engine::step(&mut s, Action::ChooseEffectBoolean(true)).unwrap();
            let mut r = restore(&s);
            finish(&mut s);
            finish(&mut r);
            assert_eq!(s, r);
            for card in down {
                assert!(s.players[actor.index()].hand.contains(card));
                assert!(s
                    .known_hand_cards(actor.opponent(), actor)
                    .iter()
                    .all(|k| k.object != *card));
            }
        }
    }
}
#[test]
fn genuine_paid_control_and_bounce_responses_keep_original_etb_controller_library_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for bounce in [false, true] {
            let mut s = ready(
                actor,
                &[
                    "Forest",
                    "Island",
                    "Llanowar Elves",
                    "Mountain",
                    "Healer's Hawk",
                ],
            );
            let (card, row) = paid_to_trigger(&mut s, actor);
            if bounce {
                let other = put(
                    &mut s,
                    actor.opponent(),
                    "Llanowar Elves",
                    Zone::Battlefield,
                );
                let response = cast(
                    &mut s,
                    actor.opponent(),
                    "Run Away Together",
                    &[Target::Object(card), Target::Object(other)],
                    2,
                );
                resolve_response(&mut s, response);
                assert_eq!(s.objects.get(card).zone, Zone::Hand);
                assert_ne!(
                    s.objects.get(card).zone_change_count,
                    row.v4.ability_source_contract.unwrap().zone_change_count
                );
            } else {
                put(
                    &mut s,
                    actor.opponent(),
                    "High Fae Trickster",
                    Zone::Battlefield,
                );
                let response = cast(
                    &mut s,
                    actor.opponent(),
                    "Involuntary Employment",
                    &[Target::Object(card)],
                    4,
                );
                resolve_response(&mut s, response);
                assert_eq!(s.objects.get(card).controller, actor.opponent());
            }
            assert_eq!(s.stack.last(), Some(&row));
            let prefix = partition_prompt(&mut s, actor);
            resolve_piles(&mut s, actor, &prefix, true);
            for object in prefix {
                assert!(s.players[actor.index()].hand.contains(&object));
            }
            assert_eq!(row.controller, actor);
        }
    }
}
#[test]
fn genuine_paid_original_graveyard_permutation_and_all_empty_pile_choices_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for down_all in [false, true] {
            let mut s = ready(
                actor,
                &[
                    "Forest",
                    "Island",
                    "Llanowar Elves",
                    "Mountain",
                    "Healer's Hawk",
                ],
            );
            paid_to_trigger(&mut s, actor);
            let prefix = partition_prompt(&mut s, actor);
            let down = if down_all { prefix.clone() } else { vec![] };
            partition(&mut s, &down);
            pile_prompt(&mut s, actor, down.len(), 5 - down.len());
            engine::step(&mut s, Action::ChooseEffectBoolean(!down_all)).unwrap();
            let mut copy = restore(&s);
            let ordered = finish(&mut s);
            assert_eq!(finish(&mut copy), ordered);
            assert_eq!(s, copy);
            assert_eq!(ordered, prefix.iter().rev().copied().collect::<Vec<_>>());
            assert_eq!(s.players[actor.index()].graveyard, ordered);
            assert!(s.players[actor.index()].hand.is_empty());
        }
    }
}
#[test]
fn genuine_paid_curator_copy_has_separate_entry_and_persistent_electroduplicate_exception_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(
            actor,
            &[
                "Forest",
                "Island",
                "Llanowar Elves",
                "Mountain",
                "Healer's Hawk",
                "Forest",
                "Mountain",
            ],
        );
        let (card, first) = paid_to_trigger(&mut s, actor);
        let prefix = partition_prompt(&mut s, actor);
        resolve_piles(&mut s, actor, &prefix, true);
        let spell = cast(
            &mut s,
            actor,
            "Electroduplicate",
            &[Target::Object(card)],
            3,
        );
        for _ in 0..24 {
            let d = next(&mut s);
            if s.objects.get(spell).zone == Zone::Graveyard {
                break;
            }
            pass(&mut s, d)
        }
        let token = s.players[actor.index()]
            .battlefield
            .iter()
            .copied()
            .find(|id| s.objects.get(*id).card_def == 472 && s.objects.get(*id).v4.is_token)
            .unwrap();
        assert_eq!(s.objects.get(token).v4.copied_end_step_sacrifices_v1, 1);
        assert!(engine::has_effective_keyword(&s, token, Keywords::HASTE));
        let row = s.stack.iter().find(|r| r.source == token).unwrap().clone();
        assert_ne!(row.inline_effect, first.inline_effect);
        let prefix = partition_prompt(&mut s, actor);
        assert_eq!(prefix.len(), 2);
        resolve_piles(&mut s, actor, &[], true);
        assert_eq!(s.objects.get(token).zone, Zone::Battlefield);
        assert!(
            s.engine
                .event_history
                .iter()
                .filter(|e| matches!(e, CommittedEvent::CuratorFinishedV1 { .. }))
                .count()
                == 2
        );
    }
}
fn readonly_reject(mut s: GameState) {
    let before = s.clone();
    assert!(validate_pending_effect_choice(&s).is_err());
    assert!(resume_resumable_resolution(&mut s).is_err());
    assert_eq!(s, before);
}
#[test]
fn genuine_paid_private_native_stage_and_public_api_forgeries_refuse_readonly_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(
            actor,
            &[
                "Forest",
                "Island",
                "Llanowar Elves",
                "Mountain",
                "Healer's Hawk",
            ],
        );
        let (_, row) = paid_to_trigger(&mut s, actor);
        let ctx = crystal_prevention_v1::context(&row);
        let before = s.clone();
        assert!(begin_resumable_resolution(
            row.inline_effect.as_ref().unwrap(),
            &ctx,
            row.clone(),
            &mut s
        )
        .is_err());
        assert_eq!(s, before);
        for unknown in [false, true] {
            let mut forged_ctx = ctx.clone();
            forged_ctx.stack_item_id = if unknown {
                Some(StackItemId(u64::MAX))
            } else {
                None
            };
            forged_ctx.ability_source_contract = None;
            let mut forged_row = row.clone();
            forged_row.kind = StackItemKind::Spell;
            forged_row.inline_effect = Some(EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            });
            forged_row.v4.ability_source_contract = None;
            forged_row.v4.stack_item_id = StackItemId(u64::MAX - 1);
            let mut bad = restore(&s);
            let before = bad.clone();
            assert!(begin_resumable_resolution(
                &EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: 1
                },
                &forged_ctx,
                forged_row,
                &mut bad
            )
            .is_err());
            assert_eq!(bad, before);
            execute(
                &EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: 1,
                },
                &forged_ctx,
                &mut bad,
            );
            let mut halted = before;
            halted.engine.halted = Some((
                engine::UnsupportedMechanic::InvalidEffectContinuation,
                ctx.source,
            ));
            assert_eq!(bad, halted);
        }
        let prefix = partition_prompt(&mut s, actor);
        let mut invalid_pick = restore(&s);
        let outsider = invalid_pick.players[actor.opponent().index()].battlefield[0];
        let before = invalid_pick.clone();
        assert!(engine::step(
            &mut invalid_pick,
            Action::ChooseEffectTarget(Target::Object(outsider))
        )
        .is_err());
        assert_eq!(invalid_pick, before);
        for mode in 0..11 {
            let mut bad = restore(&s);
            let p = bad.engine.pending_effect.as_mut().unwrap();
            match mode {
                0 => p.ctx.controller = actor.opponent(),
                1 => p.ctx.x_value = 1,
                2 => {
                    p.choice = None;
                    p.frames.clear()
                }
                3 => {
                    p.choice = None;
                    p.frames = vec![EffectFrame::Program {
                        op: p.resolving_item.inline_effect.clone().unwrap(),
                        path: vec![],
                    }]
                }
                4 => {
                    if let Some(PendingEffectChoice::SelectTargets { legal, .. }) = &mut p.choice {
                        legal.pop();
                    }
                }
                5 => {
                    if let Some(PendingEffectChoice::SelectTargets {
                        purpose: EffectTargetSelectionPurpose::CuratorCardsV1 { choice },
                        ..
                    }) = &mut p.choice
                    {
                        choice.prefix.reverse()
                    }
                }
                6 => {
                    p.resolving_item.kind = StackItemKind::Spell;
                    p.resolving_item.inline_effect = Some(EffectOp::DrawCards {
                        player: PlayerRef::Controller,
                        count: 1,
                    });
                    p.ctx.stack_item_id = None;
                    p.choice = None;
                    p.frames = vec![EffectFrame::Program {
                        op: EffectOp::DrawCards {
                            player: PlayerRef::Controller,
                            count: 1,
                        },
                        path: vec![],
                    }]
                }
                7 => {
                    let at = bad
                        .engine
                        .event_history
                        .iter()
                        .position(|e| matches!(e, CommittedEvent::CuratorPublishedV1 { .. }))
                        .unwrap();
                    bad.engine.event_history.remove(at);
                }
                8 => {
                    let at = bad
                        .engine
                        .event_history
                        .iter()
                        .position(|e| matches!(e, CommittedEvent::CuratorEnteredV1 { .. }))
                        .unwrap();
                    let receipt = bad.engine.event_history[at].clone();
                    bad.engine.event_history.push(receipt);
                }
                10 => {
                    p.resolving_item.kind = StackItemKind::Spell;
                    p.resolving_item.v4.stack_item_id = StackItemId(u64::MAX - 1);
                    p.resolving_item.inline_effect = Some(EffectOp::DrawCards {
                        player: PlayerRef::Controller,
                        count: 1,
                    });
                    p.ctx.stack_item_id = Some(StackItemId(u64::MAX));
                    p.ctx.ability_source_contract = None;
                    p.choice = None;
                    p.frames = vec![EffectFrame::Program {
                        op: EffectOp::DrawCards {
                            player: PlayerRef::Controller,
                            count: 1,
                        },
                        path: vec![],
                    }];
                    p.answered_choice_guard = None;
                }
                _ => {
                    let p = bad.engine.pending_effect.as_mut().unwrap();
                    p.resolving_item
                        .v4
                        .ability_source_contract
                        .as_mut()
                        .unwrap()
                        .zone_change_count += 1;
                }
            }
            readonly_reject(bad);
        }
        partition(&mut s, &prefix[..2]);
        ready_native_handoff(&s);
        let mut erased = restore(&s);
        erased
            .engine
            .pending_effect
            .as_mut()
            .unwrap()
            .answered_choice_guard = None;
        readonly_reject(erased);
        pile_prompt(&mut s, actor, 2, 3);
        let mut wrong = restore(&s);
        if let Some(PendingEffectChoice::ChooseBoolean {
            player,
            purpose:
                EffectBooleanChoicePurpose::CuratorPileV1 {
                    face_down_count, ..
                },
            ..
        }) = &mut wrong.engine.pending_effect.as_mut().unwrap().choice
        {
            *player = actor;
            *face_down_count = 5;
        }
        readonly_reject(wrong);
        engine::step(&mut s, Action::ChooseEffectBoolean(false)).unwrap();
        ready_native_handoff(&s);
        let mut replay = restore(&s);
        replay
            .engine
            .pending_effect
            .as_mut()
            .unwrap()
            .answered_choice_guard = None;
        readonly_reject(replay);
        let mut copy = restore(&s);
        finish(&mut s);
        finish(&mut copy);
        assert_eq!(s, copy);
        assert!(engine::validated_stack_item_target_spec(&row, &s).is_err());
    }
}

#[test]
fn native_entry_commit_capture_precedes_control_and_departure_before_collection_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor, &["Forest", "Island", "Mountain"]);
        // Native entry/control/departure primitives isolate the collector's
        // event-time contract. Paid printing coverage is in the other groups.
        let card = put(&mut s, actor, NAME, Zone::Hand);
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::zone_change(card, Zone::Battlefield),
        );
        let generation = s.objects.get(card).zone_change_count;
        crate::standard_cards_v1::gain_control(&mut s, card, actor.opponent());
        event::propose_and_commit(&mut s, event::ProposedEvent::zone_change(card, Zone::Hand));
        let mut copy = restore(&s);
        let captures = crate::trigger::capture_triggers_without_sba(&mut s);
        assert_eq!(
            crate::trigger::capture_triggers_without_sba(&mut copy),
            captures
        );
        assert_eq!(s, copy);
        let captured = captures
            .iter()
            .filter(|p| matches!(p.effect, EffectOp::CuratorDestiniesV1 { .. }))
            .collect::<Vec<_>>();
        assert_eq!(captured.len(), 1);
        let captured = captured[0];
        assert_eq!(captured.controller, actor);
        assert_eq!(captured.source, card);
        assert_eq!(
            captured.source_contract.unwrap().zone_change_count,
            generation
        );
        assert_eq!(captured.source_contract.unwrap().controller, actor);
        assert_eq!(s.objects.get(card).zone, Zone::Hand);
        assert_ne!(s.objects.get(card).zone_change_count, generation);
        assert!(crate::trigger::capture_triggers_without_sba(&mut s).is_empty());
    }
}
