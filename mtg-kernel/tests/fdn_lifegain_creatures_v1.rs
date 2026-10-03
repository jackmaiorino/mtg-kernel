//! Life-gain event granularity, per-turn trigger use and kicked permanent returns.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, Keywords, Subtype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::effect::{self, EffectOp, ExecCtx, ObjectRef};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::rl;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::surface_v2::{
    HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode, SurfaceAction, SurfaceDecision,
};
use mtg_kernel::trigger;

fn ready() -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state =
        GameState::new_from_libraries(&[forest; 40], &[forest; 40], |_| "Forest".into(), 123);
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: true,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        _ => panic!("helper zone"),
    }
    id
}

fn move_to(state: &mut GameState, object: ObjectId, zone: Zone) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, zone));
}

fn grave(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let object = put(state, player, name, Zone::Hand);
    move_to(state, object, Zone::Graveyard);
    object
}

fn surface() -> HarnessSurfaceV2 {
    HarnessSurfaceV2::new_with_priority_mode_v1(
        PriorityModeV1::EngineWindowsV1,
        SuppressionAuditMode::Full,
    )
}

fn next(surface: &mut HarnessSurfaceV2, state: &mut GameState) -> Decision {
    match surface.next_decision(state) {
        SurfaceDecision::Decision(decision @ Decision::Halted { .. }) => panic!(
            "{decision:?}; pending validation: {:?}",
            effect::validate_pending_effect_choice(state)
        ),
        SurfaceDecision::Decision(decision) => decision,
        other => panic!("expected decision: {other:?}"),
    }
}

fn apply(surface: &mut HarnessSurfaceV2, state: &mut GameState, action: Action) {
    surface.apply(state, SurfaceAction::Action(action)).unwrap();
}

fn answer(
    surface: &mut HarnessSurfaceV2,
    state: &mut GameState,
    decision: Decision,
    preferred: &[ObjectId],
) {
    let action = match decision {
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        Decision::ChooseTargets { legal_targets, .. } => Action::ChooseTarget(
            preferred
                .iter()
                .map(|&id| Target::Object(id))
                .find(|target| legal_targets.contains(target))
                .unwrap_or(legal_targets[0]),
        ),
        Decision::ChooseEffectTargets { legal_targets, .. } => Action::ChooseEffectTarget(
            preferred
                .iter()
                .map(|&id| Target::Object(id))
                .find(|target| legal_targets.contains(target))
                .unwrap_or(legal_targets[0]),
        ),
        Decision::CastSpellOrPass { .. } => Action::Pass,
        other => panic!("unexpected decision: {other:?}"),
    };
    apply(surface, state, action);
}

fn drain(surface: &mut HarnessSurfaceV2, state: &mut GameState, preferred: &[ObjectId]) {
    for _ in 0..100 {
        let decision = next(surface, state);
        if matches!(decision, Decision::CastSpellOrPass { .. }) && state.stack.is_empty() {
            assert!(state.engine.pending_triggers.is_empty());
            assert!(state.engine.pending_effect.is_none());
            assert!(surface.suppressions().is_empty());
            return;
        }
        answer(surface, state, decision, preferred);
    }
    panic!("did not finish resolving");
}

fn queue(state: &mut GameState) {
    let pending = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(pending);
}

fn gain(state: &mut GameState, player: PlayerId, amount: i32) {
    event::propose_and_commit(state, ProposedEvent::life_gain(player, amount));
    queue(state);
}

fn place_counter(state: &mut GameState, object: ObjectId, player: PlayerId) {
    let ctx = ExecCtx {
        stack_item_id: None,
        source: object,
        controller: player,
        targets: vec![],
        target_contracts: vec![],
        discarded: vec![],
        paid_cost_refs: vec![],
        hidden_ability_source: None,
        ability_source_contract: None,
        kicked: false,
        optional_additional_cost_paid: None,
        x_value: 0,
    };
    effect::execute(
        &EffectOp::PutPlusOnePlusOneCounter {
            object: ObjectRef::ThisSource,
        },
        &ctx,
        state,
    );
    queue(state);
}

fn announce(
    state: &mut GameState,
    surface: &mut HarnessSurfaceV2,
    name: &str,
    mana: u8,
    kicked: Option<bool>,
) -> ObjectId {
    let object = put(state, PlayerId::P0, name, Zone::Hand);
    state.players[0].mana_pool[ManaColor::W.pool_index()] = mana;
    assert!(
        matches!(next(surface, state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&object))
    );
    apply(surface, state, Action::CastSpell(object));
    if let Some(kicked) = kicked {
        assert!(matches!(
            next(surface, state),
            Decision::ChooseKicker { .. }
        ));
        apply(surface, state, Action::ChooseKicker(kicked));
    }
    assert!(matches!(
        next(surface, state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.stack.last().unwrap().kicked, kicked == Some(true));
    object
}

#[test]
fn definitions_append_full_cards_with_printed_stats_keywords_and_costs() {
    for (id, name, stats, keyword) in [
        (218, "Exemplar of Light", (3, 3), Keywords::FLYING),
        (219, "Sun-Blessed Healer", (3, 1), Keywords::LIFELINK),
    ] {
        assert_eq!(card_id_by_name(name), Some(id));
        preflight_fully_supported_deck(&[id]).unwrap();
        let def = &CARD_DEFS[id as usize];
        assert_eq!((def.power, def.toughness), (Some(stats.0), Some(stats.1)));
        assert!(def.keywords.has(keyword));
        let mut state = ready();
        let mut surface = surface();
        let object = announce(
            &mut state,
            &mut surface,
            name,
            if id == 218 { 4 } else { 2 },
            None,
        );
        drain(&mut surface, &mut state, &[]);
        assert_eq!(state.players[0].mana_pool[ManaColor::W.pool_index()], 0);
        assert_eq!(state.objects.get(object).zone, Zone::Battlefield);
    }
    assert!(CARD_DEFS[219].subtypes.contains(&Subtype::Cleric));
}

#[test]
fn one_positive_gain_gives_one_counter_and_one_draw_regardless_of_amount() {
    let mut state = ready();
    let exemplar = put(
        &mut state,
        PlayerId::P0,
        "Exemplar of Light",
        Zone::Battlefield,
    );
    let hand = state.players[0].hand.len();
    gain(&mut state, PlayerId::P0, 7);
    assert_eq!(state.engine.pending_triggers.len(), 1);
    drain(&mut surface(), &mut state, &[]);
    assert_eq!(state.objects.get(exemplar).counters.plus1_plus1, 1);
    assert_eq!(state.players[0].hand.len(), hand + 1);
}

#[test]
fn separate_gains_each_add_counters_but_draw_only_once_in_the_turn() {
    let mut state = ready();
    let exemplar = put(
        &mut state,
        PlayerId::P0,
        "Exemplar of Light",
        Zone::Battlefield,
    );
    gain(&mut state, PlayerId::P0, 1);
    gain(&mut state, PlayerId::P0, 2);
    drain(&mut surface(), &mut state, &[]);
    assert_eq!(state.objects.get(exemplar).counters.plus1_plus1, 2);
    assert_eq!(state.players[0].hand.len(), 1);
    gain(&mut state, PlayerId::P0, 3);
    drain(&mut surface(), &mut state, &[]);
    assert_eq!(state.objects.get(exemplar).counters.plus1_plus1, 3);
    assert_eq!(state.players[0].hand.len(), 1);
}

#[test]
fn opponent_zero_and_negative_gains_do_not_trigger_exemplar() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Exemplar of Light",
        Zone::Battlefield,
    );
    for (player, amount) in [(PlayerId::P1, 3), (PlayerId::P0, 0), (PlayerId::P0, -1)] {
        gain(&mut state, player, amount);
        assert!(state.engine.pending_triggers.is_empty());
    }
    assert!(state.trigger_uses_v1.is_none());
}

#[test]
fn split_lifelink_damage_is_one_gain_while_distinct_sources_are_separate() {
    for distinct in [false, true] {
        let mut state = ready();
        let exemplar = put(
            &mut state,
            PlayerId::P0,
            "Exemplar of Light",
            Zone::Battlefield,
        );
        let healer = put(
            &mut state,
            PlayerId::P0,
            "Sun-Blessed Healer",
            Zone::Battlefield,
        );
        let second = if distinct {
            put(
                &mut state,
                PlayerId::P0,
                "Sun-Blessed Healer",
                Zone::Battlefield,
            )
        } else {
            healer
        };
        let victim = put(
            &mut state,
            PlayerId::P1,
            "Dazzling Angel",
            Zone::Battlefield,
        );
        event::propose_and_commit_batch(
            &mut state,
            vec![
                ProposedEvent::damage(healer, Target::Object(victim), 1),
                ProposedEvent::damage(second, Target::Player(PlayerId::P1), 2),
            ],
        );
        let gains = state
            .engine
            .event_log
            .iter()
            .filter(|ev| matches!(ev, CommittedEvent::LifeGain { .. }))
            .count();
        assert_eq!(gains, if distinct { 2 } else { 1 });
        queue(&mut state);
        drain(&mut surface(), &mut state, &[]);
        assert_eq!(
            state.objects.get(exemplar).counters.plus1_plus1,
            if distinct { 2 } else { 1 }
        );
        assert_eq!(state.players[0].hand.len(), 1);
    }
}

#[test]
fn draw_limit_is_consumed_when_triggered_before_any_draw_resolves() {
    let mut state = ready();
    let exemplar = put(
        &mut state,
        PlayerId::P0,
        "Exemplar of Light",
        Zone::Battlefield,
    );
    place_counter(&mut state, exemplar, PlayerId::P0);
    place_counter(&mut state, exemplar, PlayerId::P0);
    assert_eq!(state.engine.pending_triggers.len(), 1);
    assert_eq!(state.players[0].hand.len(), 0);
    assert_eq!(state.trigger_uses_v1.as_ref().unwrap()[0].uses, 1);
    drain(&mut surface(), &mut state, &[]);
    assert_eq!(state.players[0].hand.len(), 1);
}

#[test]
fn felling_blow_places_a_counter_and_triggers_one_draw() {
    let mut state = ready();
    let exemplar = put(
        &mut state,
        PlayerId::P0,
        "Exemplar of Light",
        Zone::Battlefield,
    );
    let victim = put(
        &mut state,
        PlayerId::P1,
        "Treetop Snarespinner",
        Zone::Battlefield,
    );
    let spell = put(&mut state, PlayerId::P0, "Felling Blow", Zone::Hand);
    state.players[0].mana_pool[ManaColor::G.pool_index()] = 3;
    let mut surface = surface();
    assert!(
        matches!(next(&mut surface, &mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell))
    );
    apply(&mut surface, &mut state, Action::CastSpell(spell));
    drain(&mut surface, &mut state, &[exemplar, victim]);
    assert_eq!(state.objects.get(exemplar).counters.plus1_plus1, 1);
    assert_eq!(state.objects.get(victim).zone, Zone::Graveyard);
    assert!(state.engine.event_history.iter().any(|ev| matches!(ev, CommittedEvent::Damage { source, target: Target::Object(target), amount: 4, .. } if *source == exemplar && *target == victim)));
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(state.engine.event_history.iter().filter(|ev| matches!(ev, CommittedEvent::PlusOneCountersAdded { object, count: 1, .. } if *object == exemplar)).count(), 1);
}

#[test]
fn actual_next_turn_untap_clears_usage_and_allows_another_draw() {
    let mut state = ready();
    let exemplar = put(
        &mut state,
        PlayerId::P0,
        "Exemplar of Light",
        Zone::Battlefield,
    );
    place_counter(&mut state, exemplar, PlayerId::P0);
    drain(&mut surface(), &mut state, &[]);
    state.step = Step::Cleanup;
    let mut surface = surface();
    assert!(matches!(
        next(&mut surface, &mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.active_player, PlayerId::P1);
    assert!(state.trigger_uses_v1.is_none());
    place_counter(&mut state, exemplar, PlayerId::P0);
    drain(&mut surface, &mut state, &[]);
    assert_eq!(state.players[0].hand.len(), 2);
}

#[test]
fn opponent_counter_placement_does_not_use_the_draw_limit() {
    let mut state = ready();
    let exemplar = put(
        &mut state,
        PlayerId::P0,
        "Exemplar of Light",
        Zone::Battlefield,
    );
    place_counter(&mut state, exemplar, PlayerId::P1);
    assert!(state.engine.pending_triggers.is_empty());
    assert!(state.trigger_uses_v1.is_none());
    place_counter(&mut state, exemplar, PlayerId::P0);
    drain(&mut surface(), &mut state, &[]);
    assert_eq!(state.players[0].hand.len(), 1);
}

#[test]
fn control_change_preserves_limit_and_opponents_turn_gets_a_new_limit() {
    let mut state = ready();
    let exemplar = put(
        &mut state,
        PlayerId::P0,
        "Exemplar of Light",
        Zone::Battlefield,
    );
    place_counter(&mut state, exemplar, PlayerId::P0);
    drain(&mut surface(), &mut state, &[]);
    state.objects.get_mut(exemplar).controller = PlayerId::P1;
    place_counter(&mut state, exemplar, PlayerId::P1);
    assert!(state.engine.pending_triggers.is_empty());
    state.active_player = PlayerId::P1;
    place_counter(&mut state, exemplar, PlayerId::P1);
    drain(&mut surface(), &mut state, &[]);
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(state.players[1].hand.len(), 1);
}

#[test]
fn bounce_resets_incarnation_limit_but_old_gain_cannot_counter_the_new_object() {
    let mut state = ready();
    let exemplar = put(
        &mut state,
        PlayerId::P0,
        "Exemplar of Light",
        Zone::Battlefield,
    );
    gain(&mut state, PlayerId::P0, 1);
    move_to(&mut state, exemplar, Zone::Hand);
    move_to(&mut state, exemplar, Zone::Battlefield);
    drain(&mut surface(), &mut state, &[]);
    assert_eq!(state.objects.get(exemplar).counters.plus1_plus1, 0);
    assert_eq!(state.players[0].hand.len(), 0);
    place_counter(&mut state, exemplar, PlayerId::P0);
    drain(&mut surface(), &mut state, &[]);
    move_to(&mut state, exemplar, Zone::Hand);
    assert!(state.trigger_uses_v1.is_none());
    move_to(&mut state, exemplar, Zone::Battlefield);
    place_counter(&mut state, exemplar, PlayerId::P0);
    drain(&mut surface(), &mut state, &[]);
    assert_eq!(state.players[0].hand.len(), 2);
}

#[test]
fn pending_draw_survives_source_departure_and_restores_public_usage() {
    let mut state = ready();
    let exemplar = put(
        &mut state,
        PlayerId::P0,
        "Exemplar of Light",
        Zone::Battlefield,
    );
    place_counter(&mut state, exemplar, PlayerId::P0);
    let observed = rl::observe_v2(&state, &surface(), PlayerId::P0, 0).unwrap();
    let usage = observed.projection.engine_context.trigger_uses.unwrap();
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0].uses, 1);
    let mut restored: GameState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(state.state_hash(), restored.state_hash());
    for game in [&mut state, &mut restored] {
        move_to(game, exemplar, Zone::Hand);
        drain(&mut surface(), game, &[]);
        assert_eq!(game.players[0].hand.len(), 2);
    }
    assert_eq!(
        serde_json::to_vec(&state).unwrap(),
        serde_json::to_vec(&restored).unwrap()
    );
}

#[test]
fn angel_and_unicorn_entry_interactions_gain_counter_and_draw_once() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Dazzling Angel",
        Zone::Battlefield,
    );
    put(
        &mut state,
        PlayerId::P0,
        "Good-Fortune Unicorn",
        Zone::Battlefield,
    );
    let exemplar = put(&mut state, PlayerId::P0, "Exemplar of Light", Zone::Hand);
    move_to(&mut state, exemplar, Zone::Battlefield);
    queue(&mut state);
    drain(&mut surface(), &mut state, &[]);
    assert_eq!(state.players[0].life, 21);
    assert_eq!(state.objects.get(exemplar).counters.plus1_plus1, 2);
    assert_eq!(state.players[0].hand.len(), 1);
}

#[test]
fn healer_base_declined_and_kicked_casts_pay_exact_cost_and_only_kicked_returns() {
    for (mana, kick, remaining, returns) in [
        (2, None, 0, false),
        (4, Some(false), 2, false),
        (4, Some(true), 0, true),
    ] {
        let mut state = ready();
        let target = grave(&mut state, PlayerId::P0, "Llanowar Elves");
        let mut surface = surface();
        let healer = announce(&mut state, &mut surface, "Sun-Blessed Healer", mana, kick);
        drain(&mut surface, &mut state, &[target]);
        assert_eq!(
            state.players[0].mana_pool[ManaColor::W.pool_index()],
            remaining
        );
        assert_eq!(state.objects.get(healer).zone, Zone::Battlefield);
        assert_eq!(
            state.objects.get(target).zone,
            if returns {
                Zone::Battlefield
            } else {
                Zone::Graveyard
            }
        );
    }
}

#[test]
fn kicked_healer_without_eligible_graveyard_card_still_resolves() {
    let mut state = ready();
    let mut surface = surface();
    let healer = announce(
        &mut state,
        &mut surface,
        "Sun-Blessed Healer",
        4,
        Some(true),
    );
    drain(&mut surface, &mut state, &[]);
    assert_eq!(state.objects.get(healer).zone, Zone::Battlefield);
    assert!(state.stack.is_empty());
}

#[test]
fn graveyard_filter_covers_permanent_types_and_rejects_lands_spells_and_large_cards() {
    let mut state = ready();
    let mut eligible = vec![];
    for name in [
        "Llanowar Elves",
        "Gnarlid Colony",
        "Ichor Wellspring",
        "Bind the Monster",
    ] {
        eligible.push(grave(&mut state, PlayerId::P0, name));
    }
    for name in [
        "Forest",
        "Blood Fountain",
        "Fleeting Distraction",
        "Drossforge Bridge",
        "Blood Token",
        "Dazzling Angel",
        "Exemplar of Light",
    ] {
        let id = grave(&mut state, PlayerId::P0, name);
        if name == "Blood Fountain" {
            eligible.push(id);
        }
    }
    grave(&mut state, PlayerId::P1, "Llanowar Elves");
    let actual = engine::legal_targets_for(
        TargetSpec::NonlandPermanentCardInOwnGraveyardManaValueAtMost(2),
        &[],
        &state,
    );
    assert_eq!(
        actual,
        eligible.into_iter().map(Target::Object).collect::<Vec<_>>()
    );
}

#[test]
fn returned_artifact_has_its_real_entry_trigger() {
    let mut state = ready();
    let wellspring = grave(&mut state, PlayerId::P0, "Ichor Wellspring");
    let mut surface = surface();
    announce(
        &mut state,
        &mut surface,
        "Sun-Blessed Healer",
        4,
        Some(true),
    );
    drain(&mut surface, &mut state, &[wellspring]);
    assert_eq!(state.objects.get(wellspring).zone, Zone::Battlefield);
    assert_eq!(state.players[0].hand.len(), 1);
}

fn stage_return(
    state: &mut GameState,
    surface: &mut HarnessSurfaceV2,
    target: ObjectId,
) -> ObjectId {
    let healer = announce(state, surface, "Sun-Blessed Healer", 4, Some(true));
    for _ in 0..30 {
        let decision = next(surface, state);
        if state
            .stack
            .last()
            .is_some_and(|item| item.source == healer && item.targets == [Target::Object(target)])
        {
            return healer;
        }
        answer(surface, state, decision, &[target]);
    }
    panic!("return trigger was not placed");
}

#[test]
fn return_target_cannot_follow_a_new_graveyard_incarnation() {
    let mut state = ready();
    let target = grave(&mut state, PlayerId::P0, "Llanowar Elves");
    let mut surface = surface();
    stage_return(&mut state, &mut surface, target);
    move_to(&mut state, target, Zone::Hand);
    move_to(&mut state, target, Zone::Graveyard);
    drain(&mut surface, &mut state, &[]);
    assert_eq!(state.objects.get(target).zone, Zone::Graveyard);
}

#[test]
fn return_keeps_cast_kicker_provenance_after_healer_bounces() {
    let mut state = ready();
    let target = grave(&mut state, PlayerId::P0, "Llanowar Elves");
    let mut surface = surface();
    let healer = stage_return(&mut state, &mut surface, target);
    move_to(&mut state, healer, Zone::Hand);
    move_to(&mut state, healer, Zone::Battlefield);
    drain(&mut surface, &mut state, &[]);
    assert_eq!(state.objects.get(target).zone, Zone::Battlefield);
}

#[test]
fn aura_host_choice_ignores_hexproof_respects_protection_and_restores_exactly() {
    let mut state = ready();
    let aura = grave(&mut state, PlayerId::P0, "Bind the Monster");
    let protected = put(
        &mut state,
        PlayerId::P1,
        "Guardian of the Guildpact",
        Zone::Battlefield,
    );
    let hexproof = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    state.engine.until_end_of_turn.push(
        mtg_kernel::engine::UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
            object_id: hexproof,
            object_zone_change_count: state.objects.get(hexproof).zone_change_count,
            layer: mtg_kernel::engine::Layers::ABILITY_ADDING,
            duration: mtg_kernel::engine::EffectDuration::EndOfTurn,
            keywords: Keywords::HEXPROOF,
            timestamp: 1,
        },
    );
    let mut surface = surface();
    announce(
        &mut state,
        &mut surface,
        "Sun-Blessed Healer",
        4,
        Some(true),
    );
    for _ in 0..30 {
        let decision = next(&mut surface, &mut state);
        if let Decision::ChooseEffectTargets {
            ref legal_targets,
            can_finish,
            ..
        } = decision
        {
            assert!(legal_targets.contains(&Target::Object(hexproof)));
            assert!(!legal_targets.contains(&Target::Object(protected)));
            assert!(!can_finish);
            effect::validate_pending_effect_choice(&state).unwrap();
            let bytes = serde_json::to_vec(&state).unwrap();
            let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
            let mut restored_surface = surface.clone();
            assert_eq!(state.state_hash(), restored.state_hash());
            assert_eq!(
                format!("{decision:?}"),
                format!("{:?}", next(&mut restored_surface, &mut restored))
            );
            for (game, surf) in [
                (&mut state, &mut surface),
                (&mut restored, &mut restored_surface),
            ] {
                apply(
                    surf,
                    game,
                    Action::ChooseEffectTarget(Target::Object(hexproof)),
                );
                drain(surf, game, &[]);
                assert_eq!(game.objects.get(aura).zone, Zone::Battlefield);
                assert_eq!(
                    game.objects.get(aura).v4.attached_to.unwrap().object,
                    hexproof
                );
                assert!(game.objects.get(hexproof).attachments.contains(&aura));
                assert!(game.objects.get(hexproof).tapped);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
            return;
        }
        answer(&mut surface, &mut state, decision, &[aura]);
    }
    panic!("Aura attachment choice was not reached");
}

#[test]
fn aura_with_no_legal_host_stays_in_graveyard_after_healer_leaves() {
    let mut state = ready();
    let aura = grave(&mut state, PlayerId::P0, "Bind the Monster");
    let mut surface = surface();
    let healer = stage_return(&mut state, &mut surface, aura);
    move_to(&mut state, healer, Zone::Hand);
    drain(&mut surface, &mut state, &[]);
    assert_eq!(state.objects.get(aura).zone, Zone::Graveyard);
}
