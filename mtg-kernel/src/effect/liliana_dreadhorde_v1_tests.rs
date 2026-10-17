//! Future registration fixtures. All casts and loyalty costs use the real
//! decision engine. Battlefield operands and explicitly labelled event/type
//! boundaries are setup. These native Partial source fixtures remain unrun.
use super::*;
#[path = "liliana_public_v73_tests.rs"]
mod public_v73_tests;
use crate::{
    engine::time_stop_v1::tests::{next, pass, priority, restore, tapped},
    engine::{self, Action, Decision},
    policy_observation_v6::tests::put,
};
const NAME: &str = "Liliana, Dreadhorde General";
fn ready(p: PlayerId, hidden: &str) -> GameState {
    crate::effect::vivien_reid_v1::tests::ready(p, &["Forest"; 128], &[], hidden)
}
fn pay(s: &mut GameState, p: PlayerId, name: &str, targets: &[Target], cost: usize) -> ObjectId {
    let card = put(s, p, name, Zone::Hand);
    priority(s, p);
    assert!(
        matches!(next(s),Decision::CastSpellOrPass{castable_spells,..}if castable_spells.contains(&card))
    );
    let before = tapped(s, p);
    engine::step(s, Action::CastSpell(card)).unwrap();
    let mut chosen = 0;
    for _ in 0..32 {
        match next(s) {
            Decision::ChooseTargets {
                player,
                legal_targets,
                ..
            } => {
                assert_eq!(player, p);
                assert!(legal_targets.contains(&targets[chosen]));
                engine::step(s, Action::ChooseTarget(targets[chosen])).unwrap();
                chosen += 1;
            }
            Decision::ChooseKicker { player, .. } => {
                assert_eq!(player, p);
                engine::step(s, Action::ChooseKicker(false)).unwrap();
            }
            Decision::CastSpellOrPass { player, .. } => {
                assert_eq!(player, p);
                assert_eq!(chosen, targets.len());
                assert_eq!(tapped(s, p) - before, cost);
                assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                assert!(s.engine.pending_cast.is_none());
                let row = s.stack.iter().find(|r| r.source == card).unwrap();
                assert_eq!(row.kind, StackItemKind::Spell);
                assert!(row.v4.source_contract.is_some());
                assert!(s
                    .engine
                    .event_history
                    .iter()
                    .any(|e| matches!(e,E::SpellCast{spell,..}if *spell==card)));
                return card;
            }
            d => panic!("paid {name}: {d:?}"),
        }
    }
    panic!("cast did not publish")
}
fn finish(s: &mut GameState) {
    for _ in 0..512 {
        let d = next(s);
        if s.stack.is_empty()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_triggers.is_empty()
        {
            validate_state(s).unwrap();
            return;
        }
        match d {
            Decision::ChooseEffectTargets {
                legal_targets,
                can_finish,
                ..
            } => {
                let a = if can_finish {
                    Action::FinishEffectSelection
                } else {
                    Action::ChooseEffectTarget(legal_targets[0])
                };
                engine::step(s, a).unwrap();
            }
            Decision::ChooseTargets { legal_targets, .. } => {
                engine::step(s, Action::ChooseTarget(legal_targets[0])).unwrap();
            }
            d => pass(s, d),
        }
    }
    panic!("native stack did not finish")
}
fn walker(s: &mut GameState, p: PlayerId) -> ObjectId {
    let v = pay(s, p, NAME, &[], 6);
    finish(s);
    assert!(definition(s.objects.get(v).card_def));
    assert_eq!(s.objects.get(v).zone, Zone::Battlefield);
    v
}
fn activate(s: &mut GameState, p: PlayerId, v: ObjectId, index: u8) -> StackItem {
    priority(s, p);
    assert!(
        matches!(next(s),Decision::CastSpellOrPass{activatable_abilities,..}if activatable_abilities.contains(&(v,index)))
    );
    let before = crate::planeswalker_v1::loyalty(s, v).unwrap();
    let after = crate::planeswalker_v1::quote_loyalty_change_v48(
        s,
        v,
        [1, -4, -9][index as usize],
        crate::doubling_season_v48::CounterOriginV48::Cost,
    )
    .unwrap();
    engine::step(s, Action::ActivateAbility(v, index)).unwrap();
    let d = next(s);
    assert!(matches!(d,Decision::CastSpellOrPass{player,..}if player==p));
    assert!(s.engine.pending_activation.is_none());
    let row = s
        .stack
        .iter()
        .find(|r| r.source == v && r.v4.activated_ability_index == Some(index))
        .unwrap()
        .clone();
    assert_eq!(row.inline_effect, Some(program(index as usize)));
    assert!(row.targets.is_empty());
    let input = rows(s)
        .find_map(|(_, j)| match j {
            JournalV1::Published { input, item } if **item == row => Some(input),
            _ => None,
        })
        .unwrap();
    assert_eq!((input.before, input.after), (before, after));
    if after == 0 {
        assert_eq!(s.objects.get(v).zone, Zone::Graveyard);
    } else {
        assert_eq!(crate::planeswalker_v1::loyalty(s, v), Some(after));
    }
    validate_state(s).unwrap();
    row
}
fn prompt(s: &mut GameState) -> (PlayerId, ChoiceV1, Vec<Target>, bool) {
    for _ in 0..128 {
        let d = next(s);
        if let Decision::ChooseEffectTargets {
            player,
            legal_targets,
            can_finish,
            ..
        } = d
        {
            validate_pending_effect_choice(s).unwrap();
            let Some(PendingEffectChoice::SelectTargets {
                purpose: EffectTargetSelectionPurpose::LilianaV1 { choice },
                ..
            }) = s
                .engine
                .pending_effect
                .as_ref()
                .and_then(|p| p.choice.as_ref())
            else {
                panic!()
            };
            return (player, choice.clone(), legal_targets, can_finish);
        }
        pass(s, d);
    }
    panic!("Liliana prompt absent")
}
fn answer(s: &mut GameState, objects: &[ObjectId]) {
    let (_, original, _, _) = prompt(s);
    for &o in objects {
        if s.engine
            .pending_effect
            .as_ref()
            .is_some_and(|p| p.choice.is_none())
        {
            break;
        }
        let (_, _, legal, can_finish) = prompt(s);
        assert!(!can_finish);
        assert!(legal.contains(&Target::Object(o)));
        engine::step(s, Action::ChooseEffectTarget(Target::Object(o))).unwrap();
    }
    let p = s.engine.pending_effect.as_ref().unwrap();
    assert!(p.choice.is_none());
    let [EffectFrame::LilianaV1 { choice, selected }] = p.frames.as_slice() else {
        panic!()
    };
    assert_eq!(choice, &original);
    assert_eq!(
        selected.iter().map(|b| b.object).collect::<Vec<_>>(),
        objects
    );
    validate_pending_effect_choice(s).unwrap();
}
fn refuses(s: &GameState) {
    let mut bad = restore(s);
    let before = bad.clone();
    assert!(validate_state(&bad).is_err());
    assert!(engine::step(&mut bad, Action::Pass).is_err());
    assert_eq!(bad, before);
}

#[test]
fn paid_zombie_exact_token_season_cost_and_original_controller_after_departure_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for season in [false, true] {
            let mut s = ready(p, "Island");
            if season {
                pay(&mut s, p, "Doubling Season", &[], 5);
                finish(&mut s);
            }
            let v = walker(&mut s, p);
            assert_eq!(
                crate::planeswalker_v1::loyalty(&s, v),
                Some(if season { 12 } else { 6 })
            );
            activate(&mut s, p, v, 0);
            assert_eq!(
                crate::planeswalker_v1::loyalty(&s, v),
                Some(if season { 13 } else { 7 })
            );
            // Real paid ability keeps its source controller after a native
            // departure/reentry under another controller and a new incarnation.
            let generation = s.objects.get(v).zone_change_count;
            event::propose_and_commit(&mut s, event::ProposedEvent::zone_change(v, Zone::Hand));
            event::propose_and_commit(
                &mut s,
                event::ProposedEvent::zone_change_to_battlefield_under_controller(v, p.opponent()),
            );
            assert!(s.objects.get(v).zone_change_count > generation);
            let mut restored = restore(&s);
            for branch in [&mut s, &mut restored] {
                finish(branch);
            }
            assert_eq!(s, restored);
            let tokens = s.players[p.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|o| token_definition(s.objects.get(*o).card_def))
                .collect::<Vec<_>>();
            assert_eq!(tokens.len(), if season { 2 } else { 1 });
            for token in tokens {
                let d = &card_def::CARD_DEFS[s.objects.get(token).card_def as usize];
                assert_eq!(
                    (d.name, d.power, d.toughness),
                    ("Zombie Token", Some(2), Some(2))
                );
                assert_eq!(d.types, [CardType::Creature]);
                assert_eq!(d.subtypes, [card_def::Subtype::Zombie]);
                assert_eq!(d.colors, [crate::mana::ManaColor::B]);
                assert_eq!(s.objects.get(token).owner, p);
            }
            assert!(s.players[p.opponent().index()]
                .battlefield
                .iter()
                .all(|o| !token_definition(s.objects.get(*o).card_def)));
            assert!(crate::rl::observe_v1(&s, p, 0).is_err());
        }
    }
}

#[test]
fn paid_minus_four_apnap_all_choices_before_simultaneous_deaths_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Island");
        let own = ["Llanowar Elves", "Healer's Hawk", "Savannah Lions"]
            .map(|n| put(&mut s, p, n, Zone::Battlefield));
        let other = ["Llanowar Elves", "Healer's Hawk", "Savannah Lions"]
            .map(|n| put(&mut s, p.opponent(), n, Zone::Battlefield));
        let v = walker(&mut s, p);
        activate(&mut s, p, v, 1);
        let (who, c, _, _) = prompt(&mut s);
        assert_eq!(who, p);
        assert_eq!(c.phase, 0);
        answer(&mut s, &own[..2]);
        let (who, c, _, _) = prompt(&mut s);
        assert_eq!(who, p.opponent());
        assert_eq!(c.phase, 1);
        assert!(own
            .into_iter()
            .chain(other)
            .all(|o| s.objects.get(o).zone == Zone::Battlefield));
        let mut restored = restore(&s);
        for branch in [&mut s, &mut restored] {
            answer(branch, &other[..2]);
            // The next stage orders the actual physical owner's graveyard.
            let (who, c, _, _) = prompt(branch);
            assert_eq!((who, c.phase), (p, 2));
            assert!(own
                .into_iter()
                .chain(other)
                .all(|o| branch.objects.get(o).zone == Zone::Battlefield));
            answer(branch, &[own[1], own[0]]);
            answer(branch, &[other[0], other[1]]);
            let hand = branch.players[p.index()].hand.len();
            finish(branch);
            assert_eq!(branch.players[p.index()].hand.len(), hand + 2);
            for o in own[..2].iter().chain(&other[..2]) {
                assert_eq!(branch.objects.get(*o).zone, Zone::Graveyard);
            }
            assert_eq!(branch.objects.get(own[2]).zone, Zone::Battlefield);
            assert_eq!(branch.objects.get(other[2]).zone, Zone::Battlefield);
            assert_eq!(crate::planeswalker_v1::loyalty(branch, v), Some(2));
        }
        assert_eq!(s, restored);
    }
}

#[test]
fn paid_minus_four_at_four_loyalty_has_no_passive_for_later_sacrifices_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Island");
        let elf = put(&mut s, p, "Llanowar Elves", Zone::Battlefield);
        let v = walker(&mut s, p);
        pay(
            &mut s,
            p.opponent(),
            "Burst Lightning",
            &[Target::Object(v)],
            1,
        );
        finish(&mut s);
        assert_eq!(crate::planeswalker_v1::loyalty(&s, v), Some(4));
        let hand = s.players[p.index()].hand.len();
        activate(&mut s, p, v, 1);
        assert_eq!(s.objects.get(v).zone, Zone::Graveyard);
        finish(&mut s);
        assert_eq!(s.objects.get(elf).zone, Zone::Graveyard);
        assert_eq!(s.players[p.index()].hand.len(), hand);
        validate_state(&restore(&s)).unwrap();
    }
}

#[test]
fn paid_ultimate_reuses_multitype_keeper_and_sacrifices_protected_nonkeepers_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Island");
        pay(&mut s, p, "Doubling Season", &[], 5);
        finish(&mut s);
        let v = walker(&mut s, p);
        let hart = put(&mut s, p.opponent(), "Burnished Hart", Zone::Battlefield);
        let druid = put(&mut s, p.opponent(), "Paradise Druid", Zone::Battlefield);
        let pick = put(&mut s, p.opponent(), "Goldvein Pick", Zone::Battlefield);
        let anthem = put(
            &mut s,
            p.opponent(),
            "Anthem of Champions",
            Zone::Battlefield,
        );
        let extra_anthem = put(
            &mut s,
            p.opponent(),
            "Anthem of Champions",
            Zone::Battlefield,
        );
        // Native entry setup initializes genuine printed loyalty. These are
        // public scene operands, not claims of paid opponent sorcery casts.
        let ajani = put(
            &mut s,
            p.opponent(),
            "Ajani, Caller of the Pride",
            Zone::Hand,
        );
        let vivien = put(&mut s, p.opponent(), "Vivien Reid", Zone::Hand);
        for object in [ajani, vivien] {
            event::propose_and_commit(
                &mut s,
                event::ProposedEvent::zone_change(object, Zone::Battlefield),
            );
        }
        // Exact existing native indestructibility boundary. Sacrifice must
        // ignore this protection without turning the choice into targeting.
        s.engine.until_end_of_turn.push(
            engine::UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
                object_id: pick,
                object_zone_change_count: s.objects.get(pick).zone_change_count,
                layer: engine::Layers::ABILITY_ADDING,
                timestamp: 1,
                duration: engine::EffectDuration::EndOfTurn,
                keywords: Keywords::INDESTRUCTIBLE,
            },
        );
        assert!(engine::has_effective_keyword(
            &s,
            pick,
            Keywords::INDESTRUCTIBLE
        ));
        assert!(engine::has_effective_keyword(&s, druid, Keywords::HEXPROOF));
        let land = s.players[p.opponent().index()]
            .battlefield
            .iter()
            .copied()
            .find(|o| engine::object_has_type(&s, *o, CardType::Land))
            .unwrap();
        activate(&mut s, p, v, 2);
        let hand = s.players[p.index()].hand.len();
        answer(&mut s, &[land]);
        answer(&mut s, &[hart]);
        // The same artifact creature is legal for the next distinct type.
        let (_, c, legal, _) = prompt(&mut s);
        assert_eq!(c.phase, 2);
        assert!(legal.contains(&Target::Object(hart)));
        let mut restored = restore(&s);
        for branch in [&mut s, &mut restored] {
            answer(branch, &[hart]);
            answer(branch, &[anthem]);
            answer(branch, &[ajani]);
            // Opponent chooses its entire physical graveyard order next.
            finish(branch);
            for kept in [land, hart, anthem, ajani] {
                assert_eq!(branch.objects.get(kept).zone, Zone::Battlefield);
            }
            for dead in [druid, pick, extra_anthem, vivien] {
                assert_eq!(branch.objects.get(dead).zone, Zone::Graveyard);
            }
            assert_eq!(crate::planeswalker_v1::loyalty(branch, v), Some(3));
            assert_eq!(branch.players[p.index()].hand.len(), hand);
            assert_eq!(branch.players[p.opponent().index()].battlefield.len(), 4);
        }
        assert_eq!(s, restored);
    }
}

#[test]
fn paid_passive_tokens_animated_self_lki_and_source_ability_suppression_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Island");
        let v = walker(&mut s, p);
        activate(&mut s, p, v, 0);
        finish(&mut s);
        let token = *s.players[p.index()]
            .battlefield
            .iter()
            .find(|o| token_definition(s.objects.get(**o).card_def))
            .unwrap();
        let elf = put(&mut s, p, "Llanowar Elves", Zone::Battlefield);
        let hand = s.players[p.index()].hand.len();
        // Native simultaneous departure primitive. The existing incarnation
        // type boundary supplies the explicit "Liliana is a creature" case.
        let generation = s.objects.get(v).zone_change_count;
        crate::standard_cards_v1::restore_source_type_crew_input(&mut s, vec![(v, generation)]);
        assert!(engine::object_has_type(&s, v, CardType::Creature));
        event::propose_and_commit_batch(
            &mut s,
            [v, token, elf]
                .into_iter()
                .map(|o| event::ProposedEvent::zone_change(o, Zone::Graveyard))
                .collect(),
        );
        let mut restored = restore(&s);
        finish(&mut s);
        finish(&mut restored);
        assert_eq!(s, restored);
        assert_eq!(s.players[p.index()].hand.len(), hand + 3);
        assert!(s
            .objects
            .try_get(token)
            .is_none_or(|o| o.zone != Zone::Battlefield));

        let mut controlled = ready(p, "Island");
        let v = walker(&mut controlled, p);
        let elf = put(&mut controlled, p, "Llanowar Elves", Zone::Battlefield);
        let hand = controlled.players[p.index()].hand.len();
        let other_hand = controlled.players[p.opponent().index()].hand.len();
        event::propose_and_commit(
            &mut controlled,
            event::ProposedEvent::zone_change(elf, Zone::Graveyard),
        );
        // Native control boundary after the actual death was captured. It
        // cannot transfer the already captured draw's source controller.
        crate::standard_cards_v1::gain_control(&mut controlled, v, p.opponent());
        finish(&mut controlled);
        assert_eq!(controlled.players[p.index()].hand.len(), hand + 1);
        assert_eq!(
            controlled.players[p.opponent().index()].hand.len(),
            other_hand
        );

        let mut suppressed = ready(p, "Island");
        let v = walker(&mut suppressed, p);
        let elf = put(&mut suppressed, p, "Llanowar Elves", Zone::Battlefield);
        pay(
            &mut suppressed,
            p,
            "Imprisoned in the Moon",
            &[Target::Object(v)],
            3,
        );
        finish(&mut suppressed);
        assert!(!crate::continuous_characteristics_v1::printed_abilities_active(&suppressed, v));
        let hand = suppressed.players[p.index()].hand.len();
        event::propose_and_commit(
            &mut suppressed,
            event::ProposedEvent::zone_change(elf, Zone::Graveyard),
        );
        finish(&mut suppressed);
        assert_eq!(suppressed.players[p.index()].hand.len(), hand);
        validate_state(&restore(&suppressed)).unwrap();
    }
}

#[test]
fn paid_original_journal_choice_and_private_authority_forgery_refuse_readonly_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut token_state = ready(p, "Island");
        let v = walker(&mut token_state, p);
        let row = activate(&mut token_state, p, v, 0);
        let ctx = super::super::crystal_prevention_v1::context(&row);
        let mut direct = restore(&token_state);
        execute(&zombie(), &ctx, &mut direct);
        assert!(direct.engine.halted.is_some());
        assert!(direct
            .objects
            .iter()
            .all(|(_, o)| !token_definition(o.card_def)));
        let mut direct = restore(&token_state);
        event::propose_and_commit(&mut direct, event::ProposedEvent::create_token(492, p));
        assert!(direct.engine.halted.is_some());
        assert!(direct
            .objects
            .iter()
            .all(|(_, o)| !token_definition(o.card_def)));
        finish(&mut token_state);
        let mut bad = restore(&token_state);
        bad.stack.push(row);
        refuses(&bad);
        let mut s = ready(p, "Island");
        for n in ["Llanowar Elves", "Healer's Hawk", "Savannah Lions"] {
            put(&mut s, p, n, Zone::Battlefield);
        }
        let v = walker(&mut s, p);
        let row = activate(&mut s, p, v, 1);
        let before = s.clone();
        let ctx = super::super::crystal_prevention_v1::context(&row);
        assert!(begin_resumable_resolution(&sacrifice_two(), &ctx, row.clone(), &mut s).is_err());
        assert_eq!(s, before);
        prompt(&mut s);
        let mut bad = restore(&s);
        let pending = bad.engine.pending_effect.as_mut().unwrap();
        if let Some(PendingEffectChoice::SelectTargets {
            purpose: EffectTargetSelectionPurpose::LilianaV1 { choice },
            ..
        }) = &mut pending.choice
        {
            choice.rosters[p.index()].clear();
        } else {
            panic!()
        }
        refuses(&bad);
        let mut bad = restore(&s);
        let at = rows(&bad)
            .find_map(|(at, j)| matches!(j, JournalV1::Issued { .. }).then_some(at))
            .unwrap();
        bad.engine.event_history.remove(at);
        refuses(&bad);
        let mut bad = restore(&s);
        if let E::LilianaDreadhordeV1{journal}=bad.engine.event_history.iter_mut().find(|e|matches!(e,E::LilianaDreadhordeV1{journal}if matches!(journal.as_ref(),JournalV1::Published{..}))).unwrap() {
            let JournalV1::Published{input,..}=journal.as_mut() else {panic!()};
            input.source.zone_change_count+=1;
        }
        refuses(&bad);
        let mut resumed = restore(&s);
        finish(&mut s);
        finish(&mut resumed);
        assert_eq!(s, resumed);
        let mut bad = restore(&s);
        bad.stack.push(row);
        refuses(&bad);
    }
}

#[test]
fn paid_public_choices_independent_of_opponent_hidden_identity_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut worlds = Vec::new();
        for hidden in ["Forest", "Island"] {
            let mut s = ready(p, hidden);
            for n in ["Llanowar Elves", "Healer's Hawk", "Savannah Lions"] {
                put(&mut s, p, n, Zone::Battlefield);
            }
            let v = walker(&mut s, p);
            activate(&mut s, p, v, 1);
            worlds.push(prompt(&mut s));
        }
        assert_eq!(worlds[0], worlds[1]);
    }
}

#[test]
fn paid_short_creature_rosters_and_missing_permanent_types_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for own_count in 0..=2 {
            for other_count in 0..=2 {
                let mut s = ready(p, "Island");
                for _ in 0..own_count {
                    put(&mut s, p, "Llanowar Elves", Zone::Battlefield);
                }
                for _ in 0..other_count {
                    put(&mut s, p.opponent(), "Healer's Hawk", Zone::Battlefield);
                }
                let v = walker(&mut s, p);
                let hand = s.players[p.index()].hand.len();
                activate(&mut s, p, v, 1);
                finish(&mut s);
                assert_eq!(s.players[p.index()].hand.len(), hand + own_count);
                for owner in [p, p.opponent()] {
                    assert!(s.players[owner.index()]
                        .battlefield
                        .iter()
                        .all(|o| !engine::object_has_type(&s, *o, CardType::Creature)));
                }
                validate_state(&restore(&s)).unwrap();
            }
        }
        let mut s = ready(p, "Island");
        pay(&mut s, p, "Doubling Season", &[], 5);
        finish(&mut s);
        let v = walker(&mut s, p);
        activate(&mut s, p, v, 2);
        finish(&mut s);
        assert_eq!(s.players[p.opponent().index()].battlefield.len(), 1);
        assert!(engine::object_has_type(
            &s,
            s.players[p.opponent().index()].battlefield[0],
            CardType::Land
        ));
        validate_state(&restore(&s)).unwrap();
    }
}
