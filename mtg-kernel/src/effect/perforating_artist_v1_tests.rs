use super::*;
use crate::{
    card_def,
    engine::{self, Action, Decision},
    event::CommittedEvent,
    midnight_snack_v1::tests::{drain, mana, priority, walk},
    policy_observation_v6::tests::put,
    state::Step,
};

#[test]
fn genuine_v64_paid_cast_meaning_and_full_public_form_restore_both_seats() {
    use crate::{artist_public_v64 as public, rules_meaning_artist_v64 as meaning};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        let artist = put(&mut state, actor, "Perforating Artist", Zone::Hand);
        let hidden = public::capture(&state, actor.opponent()).unwrap();
        assert!(!hidden
            .authorized_printings
            .iter()
            .any(|r| r.object == artist.0));
        assert!(public::capture(&state, actor)
            .unwrap()
            .authorized_printings
            .iter()
            .any(|r| r.object == artist.0));
        mana(&mut state, actor, "Swamp", 1);
        mana(&mut state, actor, "Mountain", 2);
        engine::step(&mut state, Action::CastSpell(artist)).unwrap();
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
        );
        let copy = restored(&state);
        for viewer in [actor, actor.opponent()] {
            let facts = public::capture(&state, viewer).unwrap();
            assert_eq!(public::capture(&copy, viewer).unwrap(), facts);
            public::validate(&state, viewer, &facts).unwrap();
            assert_eq!(facts.casts.len(), 1);
            assert!(facts.casts[0].genuine_cast_finalized);
            assert!(!facts.casts[0].is_uncast_copy);
            assert!(!facts.casts[0].payment_pending);
            assert_eq!(facts.casts[0].original_controller, actor.into());
            assert_eq!(facts.casts[0].current_form.power, Some(3));
            assert_eq!(facts.casts[0].current_form.toughness, Some(2));
            assert!(!facts.complete_ordinary_observation);
            assert!(!facts.owning_consumer_admitted);
            let mut changed = facts.meaning.clone();
            changed.ordered_rules.reverse();
            assert!(meaning::validate(&changed).is_err());
            let mut changed = facts.clone();
            changed.casts[0].original_controller = actor.opponent().into();
            let before = serde_json::to_vec(&state).unwrap();
            assert!(public::validate(&state, viewer, &changed).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
        drain(&mut state);
        let facts = public::capture(&state, actor).unwrap();
        let permanent = facts.battlefield[actor.index()]
            .iter()
            .find(|r| r.card.object == artist.0)
            .unwrap();
        assert_eq!(
            permanent.form.keyword_bits & Keywords::DEATHTOUCH.0,
            Keywords::DEATHTOUCH.0
        );
        assert_eq!(permanent.form.power, Some(3));
        assert_eq!(permanent.counters.fellowship, 0);
        assert_eq!(CARD_DEFS[460].capability, CardCapability::Partial);
        assert!(card_def::preflight_fully_supported_deck(&[460]).is_err());
    }
}

#[test]
fn genuine_v64_raid_sacrifice_discard_privacy_and_answered_restore_both_seats() {
    use crate::artist_public_v64::{self as public, PaymentKindV64, PaymentStatusV64, StageV64};
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..3 {
            let payer = actor.opponent();
            let (mut state, artist) = paid(actor);
            let permanent = put(&mut state, payer, "Fishing Pole", Zone::Battlefield);
            let card = put(&mut state, payer, "Forest", Zone::Hand);
            raid(&mut state, actor, artist, true);
            let published = public::capture(&state, actor).unwrap();
            let row = published
                .triggers
                .iter()
                .find(|r| r.stage == StageV64::Published)
                .unwrap();
            assert_eq!(row.source.captured_controller, actor.into());
            assert_eq!(row.affected_opponent, payer.into());
            assert!(row.payment.is_none());
            assert_eq!(payment(&mut state, payer), vec![Target::Object(permanent)]);
            for viewer in [actor, payer] {
                let facts = public::capture(&state, viewer).unwrap();
                let row = facts
                    .triggers
                    .iter()
                    .find(|r| r.stage == StageV64::Resolving)
                    .unwrap();
                let choice = row.payment.as_ref().unwrap();
                assert_eq!(
                    choice.kind,
                    PaymentKindV64::SacrificeControlledNonlandPermanent
                );
                assert_eq!(choice.status, PaymentStatusV64::Waiting);
                assert_eq!(choice.candidates.as_ref().unwrap()[0].object, permanent.0);
                assert!(!choice.is_target);
                assert!(choice.may_decline);
            }
            if mode == 0 {
                engine::step(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(permanent)),
                )
                .unwrap();
            } else {
                engine::step(&mut state, Action::FinishEffectSelection).unwrap();
                assert_eq!(payment(&mut state, payer), vec![Target::Object(card)]);
                let hidden = public::capture(&state, actor).unwrap();
                let shown = public::capture(&state, payer).unwrap();
                let choice = hidden.triggers[0].payment.as_ref().unwrap();
                assert_eq!(choice.kind, PaymentKindV64::DiscardCard);
                assert!(choice.candidates.is_none());
                assert!(choice.selected.is_none());
                assert_eq!(
                    shown.triggers[0]
                        .payment
                        .as_ref()
                        .unwrap()
                        .candidates
                        .as_ref()
                        .unwrap()[0]
                        .object,
                    card.0
                );
                // An otherwise identical hidden-world fixture changes only the
                // unexposed hand printing; it is not another gameplay cast.
                let mut hidden_world = state.clone();
                hidden_world.objects.get_mut(card).card_def =
                    card_def::card_id_by_name("Island").unwrap();
                assert_eq!(public::capture(&hidden_world, actor).unwrap(), hidden);
                assert_ne!(public::capture(&hidden_world, payer).unwrap(), shown);
                if mode == 1 {
                    engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(card)))
                        .unwrap();
                } else {
                    engine::step(&mut state, Action::FinishEffectSelection).unwrap();
                }
            }
            let mut copy = restored(&state);
            for viewer in [actor, payer] {
                let facts = public::capture(&state, viewer).unwrap();
                assert_eq!(public::capture(&copy, viewer).unwrap(), facts);
                assert_eq!(
                    facts.triggers[0].payment.as_ref().unwrap().status,
                    PaymentStatusV64::Answered
                );
                public::validate(&state, viewer, &facts).unwrap();
            }
            let life = state.players[payer.index()].life;
            for game in [&mut state, &mut copy] {
                drain(game);
                assert_eq!(
                    game.players[payer.index()].life,
                    life - if mode == 2 { 3 } else { 0 }
                );
                assert!(public::capture(game, actor).unwrap().triggers.is_empty());
                assert!(game.engine.halted.is_none());
            }
            assert_same(&state, &copy);
        }
    }
}

#[test]
fn genuine_v64_native_raid_root_and_payment_forgery_refuse_readonly_both_seats() {
    use crate::artist_public_v64 as public;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let payer = actor.opponent();
        let (mut state, artist) = paid(actor);
        put(&mut state, payer, "Fishing Pole", Zone::Battlefield);
        raid(&mut state, actor, artist, true);
        payment(&mut state, payer);
        for mode in 0..4 {
            let mut changed = state.clone();
            let pending = changed.engine.pending_effect.as_mut().unwrap();
            match mode {
                0 => pending.ctx.controller = payer,
                1 => pending.resolving_item.inline_effect = Some(EffectOp::Sequence(vec![])),
                2 => {
                    pending
                        .resolving_item
                        .v4
                        .ability_source_contract
                        .as_mut()
                        .unwrap()
                        .zone_change_count += 1
                }
                3 => {
                    let PendingEffectChoice::SelectTargets { legal, .. } =
                        pending.choice.as_mut().unwrap()
                    else {
                        panic!()
                    };
                    legal.clear();
                }
                _ => unreachable!(),
            }
            let before = serde_json::to_vec(&changed).unwrap();
            assert!(public::capture(&changed, actor).is_err());
            assert_eq!(serde_json::to_vec(&changed).unwrap(), before);
        }
    }
}

#[test]
fn genuine_v64_paid_control_response_keeps_original_raid_controller_and_public_roster_both_seats() {
    use crate::artist_public_v64::{self as public, StageV64};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let payer = actor.opponent();
        let (mut state, artist) = paid(actor);
        put(&mut state, payer, "High Fae Trickster", Zone::Battlefield);
        raid(&mut state, actor, artist, true);
        let employment = put(&mut state, payer, "Involuntary Employment", Zone::Hand);
        mana(&mut state, payer, "Mountain", 4);
        priority(&mut state, payer);
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { player, castable_spells, .. }
            if player == payer && castable_spells.contains(&employment))
        );
        engine::step(&mut state, Action::CastSpell(employment)).unwrap();
        assert!(
            matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(artist)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(artist))).unwrap();
        let mut copy = restored(&state);
        for game in [&mut state, &mut copy] {
            resolve_response(game, employment);
            let facts = public::capture(game, actor).unwrap();
            let raid = facts
                .triggers
                .iter()
                .find(|r| r.stage == StageV64::Published)
                .unwrap();
            assert_eq!(raid.source.captured_controller, actor.into());
            assert_eq!(raid.affected_opponent, payer.into());
            assert_eq!(
                raid.source
                    .current_original_source
                    .as_ref()
                    .unwrap()
                    .controller,
                payer.into()
            );
            assert!(payment(game, payer).contains(&Target::Object(artist)));
            let facts = public::capture(game, actor).unwrap();
            assert!(facts.triggers[0]
                .payment
                .as_ref()
                .unwrap()
                .candidates
                .as_ref()
                .unwrap()
                .iter()
                .any(|r| r.object == artist.0 && r.controller == payer.into()));
            engine::step(game, Action::ChooseEffectTarget(Target::Object(artist))).unwrap();
            drain(game);
            assert_eq!(game.objects.get(artist).zone, Zone::Graveyard);
            assert!(game.players[actor.index()].graveyard.contains(&artist));
            assert!(public::capture(game, actor).unwrap().triggers.is_empty());
        }
        assert_same(&state, &copy);
    }
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}
fn quiet(state: &mut GameState, decision: Decision) {
    let action = match decision {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        other => panic!("unexpected Artist decision {other:?}"),
    };
    engine::step(state, action).unwrap();
}
fn restored(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let copy: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(copy.state_hash(), state.state_hash());
    assert_eq!(serde_json::to_vec(&copy).unwrap(), bytes);
    copy
}
fn assert_same(a: &GameState, b: &GameState) {
    assert_eq!(a.state_hash(), b.state_hash());
    assert_eq!(
        serde_json::to_vec(a).unwrap(),
        serde_json::to_vec(b).unwrap()
    );
}
fn paid(actor: PlayerId) -> (GameState, ObjectId) {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
    let artist = put(&mut state, actor, "Perforating Artist", Zone::Hand);
    assert_eq!(state.objects.get(artist).card_def, 460);
    let definition = &CARD_DEFS[460];
    assert!(exact_definition(definition));
    assert_eq!(definition.capability, CardCapability::Partial);
    assert!(card_def::preflight_fully_supported_deck(&[460]).is_err());
    mana(&mut state, actor, "Swamp", 1);
    mana(&mut state, actor, "Mountain", 2);
    engine::step(&mut state, Action::CastSpell(artist)).unwrap();
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
    );
    assert!(state
        .stack
        .iter()
        .any(|row| row.source == artist && row.kind == StackItemKind::Spell));
    assert_eq!(
        state.players[actor.index()]
            .mana_pool
            .iter()
            .map(|n| u32::from(*n))
            .sum::<u32>(),
        0
    );
    assert!(state.engine.event_history.iter().any(|event| matches!(event,
        CommittedEvent::SpellCast { spell, controller } if *spell == artist && *controller == actor)));
    drain(&mut state);
    assert_eq!(state.objects.get(artist).zone, Zone::Battlefield);
    assert!(engine::has_effective_keyword(
        &state,
        artist,
        Keywords::DEATHTOUCH
    ));
    (state, artist)
}
fn raid(state: &mut GameState, actor: PlayerId, artist: ObjectId, attack: bool) {
    // The support attacker is a represented genuine printing. Raid's actual
    // declaration, combat, natural End step and captured trigger are native.
    let hawk = attack.then(|| put(state, actor, "Healer's Hawk", Zone::Battlefield));
    walk(state, actor, Step::End, hawk);
    assert_eq!(state.player_attacked_this_turn_v1(actor), attack);
    if attack {
        let row = state
            .stack
            .iter()
            .find(|row| row.source == artist && row.kind == StackItemKind::TriggeredAbility)
            .unwrap();
        assert_eq!(row.inline_effect, Some(super::raid()));
        assert_eq!(row.controller, actor);
        assert_eq!(row.v4.ability_source_contract.unwrap().controller, actor);
        assert!(row.targets.is_empty());
    }
}
fn payment(state: &mut GameState, payer: PlayerId) -> Vec<Target> {
    for _ in 0..32 {
        let decision = next(state);
        if let Decision::ChooseEffectTargets {
            player,
            selected_count,
            min_targets,
            max_targets,
            can_finish,
            legal_targets,
            ..
        } = decision
        {
            assert_eq!(player, payer);
            assert_eq!(
                (selected_count, min_targets, max_targets, can_finish),
                (0, 0, 1, true)
            );
            crate::effect::validate_pending_effect_choice(state).unwrap();
            return legal_targets;
        }
        quiet(state, decision);
    }
    panic!("Artist payment did not appear");
}
fn resolve_response(state: &mut GameState, spell: ObjectId) {
    for _ in 0..32 {
        let decision = next(state);
        if state.objects.get(spell).zone == Zone::Graveyard && state.engine.pending_effect.is_none()
        {
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
            return;
        }
        quiet(state, decision);
    }
    panic!("paid response did not resolve above Artist");
}

#[test]
fn genuine_paid_sacrifice_discard_decline_and_empty_rosters_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..5 {
            let payer = actor.opponent();
            let (mut state, artist) = paid(actor);
            let permanent =
                (mode < 3).then(|| put(&mut state, payer, "Fishing Pole", Zone::Battlefield));
            let card = (mode == 1 || mode == 2 || mode == 3)
                .then(|| put(&mut state, payer, "Forest", Zone::Hand));
            raid(&mut state, actor, artist, true);
            let life = state.players[payer.index()].life;
            if mode < 3 {
                assert_eq!(
                    payment(&mut state, payer),
                    vec![Target::Object(permanent.unwrap())]
                );
            }
            let mut prompt_restore = restored(&state);
            for game in [&mut state, &mut prompt_restore] {
                if mode == 0 {
                    engine::step(
                        game,
                        Action::ChooseEffectTarget(Target::Object(permanent.unwrap())),
                    )
                    .unwrap();
                } else if mode < 3 {
                    engine::step(game, Action::FinishEffectSelection).unwrap();
                    assert_eq!(payment(game, payer), vec![Target::Object(card.unwrap())]);
                } else if mode == 3 {
                    assert_eq!(payment(game, payer), vec![Target::Object(card.unwrap())]);
                }
                if mode == 1 || mode == 3 {
                    engine::step(
                        game,
                        Action::ChooseEffectTarget(Target::Object(card.unwrap())),
                    )
                    .unwrap();
                } else if mode == 2 {
                    engine::step(game, Action::FinishEffectSelection).unwrap();
                }
            }
            assert_same(&state, &prompt_restore);
            let mut answered_restore = restored(&state);
            for game in [&mut state, &mut prompt_restore, &mut answered_restore] {
                drain(game);
                assert_eq!(
                    game.players[payer.index()].life,
                    life - if mode == 2 || mode == 4 { 3 } else { 0 }
                );
                assert!(game.engine.pending_effect.is_none());
                assert!(game.engine.halted.is_none());
                if mode == 0 {
                    let chosen = permanent.unwrap();
                    assert_eq!(game.objects.get(chosen).zone, Zone::Graveyard);
                    assert!(game.engine.event_history.iter().any(|event| matches!(event,
                        CommittedEvent::Sacrificed { object, controller_before, .. } if *object == chosen && *controller_before == payer)));
                }
                if mode == 1 || mode == 3 {
                    assert_eq!(game.objects.get(card.unwrap()).zone, Zone::Graveyard);
                    assert!(game.engine.event_history.iter().any(|event| matches!(event,
                        CommittedEvent::ZoneChange { object, from: Zone::Hand, to: Zone::Graveyard, .. } if *object == card.unwrap())));
                }
            }
            assert_same(&state, &prompt_restore);
            assert_same(&state, &answered_restore);
        }
    }
}

#[test]
fn genuine_non_target_nonland_roster_includes_hexproof_and_excludes_paid_animated_land_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        let payer = actor.opponent();
        let (mut state, artist) = paid(actor);
        // Native animation is genuinely paid. Other operands are explicitly
        // battlefield support fixtures using their represented printed types.
        let land = put(&mut state, payer, "Soulstone Sanctuary", Zone::Battlefield);
        mana(&mut state, payer, "Mountain", 4);
        engine::step(&mut state, Action::ActivateAbility(land, 0)).unwrap();
        drain(&mut state);
        assert!(engine::object_has_type(&state, land, CardType::Land));
        assert!(engine::object_has_type(&state, land, CardType::Creature));
        let hexproof = put(&mut state, payer, "Saiba Cryptomancer", Zone::Battlefield);
        let artifact = put(&mut state, payer, "Fishing Pole", Zone::Battlefield);
        let enchantment = put(&mut state, payer, "Midnight Snack", Zone::Battlefield);
        assert!(engine::has_effective_keyword(
            &state,
            hexproof,
            Keywords::HEXPROOF
        ));
        raid(&mut state, actor, artist, true);
        let legal = payment(&mut state, payer);
        assert_eq!(legal.len(), 3);
        for object in [hexproof, artifact, enchantment] {
            assert!(legal.contains(&Target::Object(object)));
        }
        assert!(!legal.contains(&Target::Object(land)));
        assert!(!legal.contains(&Target::Object(artist)));
        let before = serde_json::to_vec(&state).unwrap();
        assert!(
            engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(land))).is_err()
        );
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(hexproof)),
        )
        .unwrap();
        let mut copy = restored(&state);
        for game in [&mut state, &mut copy] {
            drain(game);
            assert_eq!(game.objects.get(hexproof).zone, Zone::Graveyard);
            assert_eq!(game.objects.get(land).zone, Zone::Battlefield);
        }
        assert_same(&state, &copy);
    }
}

#[test]
fn genuine_paid_flash_control_response_keeps_captured_payer_and_allows_source_sacrifice_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        let payer = actor.opponent();
        let (mut state, artist) = paid(actor);
        put(&mut state, payer, "High Fae Trickster", Zone::Battlefield);
        raid(&mut state, actor, artist, true);
        let original = state.stack.last().unwrap().clone();
        let employment = put(&mut state, payer, "Involuntary Employment", Zone::Hand);
        mana(&mut state, payer, "Mountain", 4);
        priority(&mut state, payer);
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { player, castable_spells, .. }
            if player == payer && castable_spells.contains(&employment))
        );
        engine::step(&mut state, Action::CastSpell(employment)).unwrap();
        assert!(
            matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(artist)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(artist))).unwrap();
        let mut response_restore = restored(&state);
        for game in [&mut state, &mut response_restore] {
            resolve_response(game, employment);
            assert_eq!(game.objects.get(artist).controller, payer);
            assert_eq!(game.objects.get(artist).owner, actor);
            assert_eq!(game.stack.last(), Some(&original));
            assert!(payment(game, payer).contains(&Target::Object(artist)));
            engine::step(game, Action::ChooseEffectTarget(Target::Object(artist))).unwrap();
        }
        assert_same(&state, &response_restore);
        let mut answer_restore = restored(&state);
        for game in [&mut state, &mut response_restore, &mut answer_restore] {
            drain(game);
            assert_eq!(game.objects.get(artist).zone, Zone::Graveyard);
            assert!(game.players[actor.index()].graveyard.contains(&artist));
            assert!(game.engine.event_history.iter().any(|event| matches!(event,
                CommittedEvent::Sacrificed { object, controller_before, .. } if *object == artist && *controller_before == payer)));
        }
        assert_same(&state, &response_restore);
        assert_same(&state, &answer_restore);
    }
}

#[test]
fn genuine_no_attack_and_paid_source_departure_reentry_preserve_original_trigger_restore_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut no_attack, source) = paid(actor);
        raid(&mut no_attack, actor, source, false);
        drain(&mut no_attack);
        assert!(no_attack.stack.is_empty());
        assert_eq!(no_attack.players[actor.opponent().index()].life, 20);

        let (mut state, artist) = paid(actor);
        put(&mut state, actor, "High Fae Trickster", Zone::Battlefield);
        let victim = put(
            &mut state,
            actor.opponent(),
            "Fishing Pole",
            Zone::Battlefield,
        );
        raid(&mut state, actor, artist, true);
        let original = state.stack.last().unwrap().clone();
        let generation = original
            .v4
            .ability_source_contract
            .unwrap()
            .zone_change_count;
        let other = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let bounce = put(&mut state, actor, "Run Away Together", Zone::Hand);
        mana(&mut state, actor, "Island", 2);
        engine::step(&mut state, Action::CastSpell(bounce)).unwrap();
        assert!(
            matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(artist)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(artist))).unwrap();
        assert!(
            matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(other)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(other))).unwrap();
        resolve_response(&mut state, bounce);
        assert_eq!(state.objects.get(artist).zone, Zone::Hand);
        priority(&mut state, actor);
        // Float the recast's BR only after the bounce has paid its generic
        // cost, so that payment cannot consume Artist's only black unit.
        mana(&mut state, actor, "Swamp", 1);
        mana(&mut state, actor, "Mountain", 2);
        engine::step(&mut state, Action::CastSpell(artist)).unwrap();
        for _ in 0..16 {
            let decision = next(&mut state);
            if state.objects.get(artist).zone == Zone::Battlefield {
                break;
            }
            quiet(&mut state, decision);
        }
        assert_eq!(state.objects.get(artist).zone, Zone::Battlefield);
        assert!(state.objects.get(artist).zone_change_count > generation);
        assert_eq!(state.stack.last(), Some(&original));
        assert_eq!(
            payment(&mut state, actor.opponent()),
            vec![Target::Object(victim)]
        );
        let mut copy = restored(&state);
        for game in [&mut state, &mut copy] {
            engine::step(game, Action::ChooseEffectTarget(Target::Object(victim))).unwrap();
            drain(game);
            assert_eq!(game.objects.get(artist).zone, Zone::Battlefield);
            assert_eq!(game.objects.get(victim).zone, Zone::Graveyard);
        }
        assert_same(&state, &copy);
    }
}

fn refuses_readonly(mut state: GameState, label: &str) {
    let before = serde_json::to_vec(&state).unwrap();
    assert!(
        crate::effect::validate_pending_effect_choice(&state).is_err(),
        "{label}"
    );
    assert!(
        crate::effect::resume_resumable_resolution(&mut state).is_err(),
        "{label}"
    );
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
}

#[test]
fn genuine_paid_restore_forgery_refusals_and_consumed_row_cleanup_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let payer = actor.opponent();
        let (mut state, artist) = paid(actor);
        let victim = put(&mut state, payer, "Fishing Pole", Zone::Battlefield);
        put(&mut state, payer, "Forest", Zone::Hand);
        raid(&mut state, actor, artist, true);
        payment(&mut state, payer);
        let pending = state.engine.pending_effect.as_ref().unwrap().clone();
        let mut raw_begin = restored(&state);
        raw_begin.engine.pending_effect = None;
        let before = serde_json::to_vec(&raw_begin).unwrap();
        assert!(crate::effect::begin_resumable_resolution(
            &EffectOp::Sequence(vec![]),
            &pending.ctx,
            pending.resolving_item,
            &mut raw_begin
        )
        .is_err());
        assert_eq!(serde_json::to_vec(&raw_begin).unwrap(), before);
        for mode in 0..10 {
            let mut forged = restored(&state);
            let pending = forged.engine.pending_effect.as_mut().unwrap();
            match mode {
                0 => pending.frames.push(EffectFrame::Program {
                    op: EffectOp::Sequence(vec![]),
                    path: vec![1],
                }),
                1 => {
                    pending.choice = None;
                    pending.frames.clear();
                }
                2 => {
                    pending.choice = None;
                    pending.frames = vec![EffectFrame::Program {
                        op: EffectOp::Sequence(vec![]),
                        path: vec![1],
                    }];
                }
                3 => pending.ctx.controller = payer,
                4 => {
                    let Some(PendingEffectChoice::SelectTargets {
                        player,
                        purpose: EffectTargetSelectionPurpose::NonlandSacrificePaymentV1 { choice },
                        ..
                    }) = pending.choice.as_mut()
                    else {
                        unreachable!()
                    };
                    *player = actor;
                    choice.player = actor;
                }
                5 => {
                    let Some(PendingEffectChoice::SelectTargets {
                        purpose: EffectTargetSelectionPurpose::NonlandSacrificePaymentV1 { choice },
                        ..
                    }) = pending.choice.as_mut()
                    else {
                        unreachable!()
                    };
                    *choice.unpaid = EffectOp::LoseLife {
                        player: PlayerRef::Opponent,
                        amount: 99,
                    };
                }
                6 => {
                    let Some(PendingEffectChoice::SelectTargets { legal, .. }) =
                        pending.choice.as_mut()
                    else {
                        unreachable!()
                    };
                    legal[0]
                        .expected_object
                        .as_mut()
                        .unwrap()
                        .expected_zone_change_count += 1;
                }
                7 => {
                    let row = forged.stack.last().unwrap().clone();
                    forged.stack.push(row);
                }
                8 => {
                    forged.stack.pop();
                }
                9 => pending.ctx.x_value = pending.ctx.x_value.saturating_add(1),
                _ => unreachable!(),
            }
            refuses_readonly(forged, &format!("waiting forgery mode {mode}"));
        }
        engine::step(&mut state, Action::FinishEffectSelection).unwrap();
        let mut answered = restored(&state);
        answered
            .engine
            .pending_effect
            .as_mut()
            .unwrap()
            .answered_choice_guard = None;
        refuses_readonly(answered, "answered sacrifice guard erased");
        payment(&mut state, payer);
        let mut forged = restored(&state);
        let pending = forged.engine.pending_effect.as_mut().unwrap();
        let Some(PendingEffectChoice::SelectTargets {
            purpose: EffectTargetSelectionPurpose::DiscardPaymentV1 { choice },
            path,
            ..
        }) = pending.choice.as_mut()
        else {
            unreachable!()
        };
        *path = vec![0];
        choice.path = vec![0];
        refuses_readonly(forged, "discard path changed");
        engine::step(&mut state, Action::FinishEffectSelection).unwrap();
        let item = state
            .engine
            .pending_effect
            .as_ref()
            .unwrap()
            .resolving_item
            .clone();
        // A genuine answered pending state supplies the private owner. The
        // deliberately invalid post-acquisition Raid input forces a public
        // validation error after the row loan; that loan must still disappear.
        let owner = acquire_resume(&state, &item).unwrap().unwrap();
        let index =
            engine::resolution_cast_v1::resolving_index(&state, item.v4.stack_item_id).unwrap();
        assert_eq!(state.stack.remove(index), item);
        state.attack_declaration_turn_v1 = None;
        let remaining = state.stack.clone();
        assert!(resume_owned(&mut state, owner).is_err());
        assert_eq!(state.stack, remaining);
        assert!(!state
            .stack
            .iter()
            .any(|row| row.v4.stack_item_id == item.v4.stack_item_id));
        assert_eq!(state.objects.get(victim).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(artist).zone, Zone::Battlefield);
    }
}

#[test]
fn genuine_paid_fiery_sacrifice_replacement_and_discard_madness_keep_cost_paid_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let payer = actor.opponent();
        let (mut state, artist) = paid(actor);
        let victim = put(&mut state, payer, "Rune-Sealed Wall", Zone::Battlefield);
        raid(&mut state, actor, artist, true);
        let fiery = put(&mut state, actor, "Fiery Annihilation", Zone::Hand);
        mana(&mut state, actor, "Mountain", 3);
        engine::step(&mut state, Action::CastSpell(fiery)).unwrap();
        assert!(
            matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(victim)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(victim))).unwrap();
        assert!(matches!(
            next(&mut state),
            Decision::ChooseTargets {
                can_finish: true,
                ..
            }
        ));
        engine::step(&mut state, Action::FinishEffectSelection).unwrap();
        resolve_response(&mut state, fiery);
        assert_eq!(state.objects.get(victim).damage, 5);
        assert_eq!(state.objects.get(victim).zone, Zone::Battlefield);
        event::validate_exile_if_dies_replacements(&state).unwrap();
        assert_eq!(payment(&mut state, payer), vec![Target::Object(victim)]);
        let life = state.players[payer.index()].life;
        engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(victim)),
        )
        .unwrap();
        let mut copy = restored(&state);
        for game in [&mut state, &mut copy] {
            drain(game);
            assert_eq!(game.objects.get(victim).zone, Zone::Exile);
            assert_eq!(game.players[payer.index()].life, life);
            assert!(game.engine.event_history.iter().any(|event| matches!(event,
                CommittedEvent::Sacrificed { object, .. } if *object == victim)));
        }
        assert_same(&state, &copy);

        let (mut state, artist) = paid(actor);
        let temper = put(&mut state, payer, "Fiery Temper", Zone::Hand);
        raid(&mut state, actor, artist, true);
        assert_eq!(payment(&mut state, payer), vec![Target::Object(temper)]);
        let life = state.players[payer.index()].life;
        engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(temper)),
        )
        .unwrap();
        let mut copy = restored(&state);
        for game in [&mut state, &mut copy] {
            let mut offered = false;
            for _ in 0..32 {
                let decision = next(game);
                if let Decision::ChooseMadnessCast { player, card } = decision {
                    assert_eq!((player, card), (payer, temper));
                    assert!(game.engine.pending_effect.is_none());
                    assert_eq!(game.objects.get(temper).zone, Zone::Exile);
                    offered = true;
                    engine::step(game, Action::ChooseMadnessCast(false)).unwrap();
                    break;
                }
                quiet(game, decision);
            }
            assert!(offered);
            drain(game);
            assert_eq!(game.objects.get(temper).zone, Zone::Graveyard);
            assert_eq!(game.players[payer.index()].life, life);
            assert!(game.engine.event_history.iter().any(|event| matches!(event,
                CommittedEvent::ZoneChange { object, from: Zone::Hand, to: Zone::Exile, .. } if *object == temper)));
        }
        assert_same(&state, &copy);
    }
}
