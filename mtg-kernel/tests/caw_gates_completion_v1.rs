//! Remaining Caw-Gates cards, checked against Mage
//! `a5c90fe180021e70e2a644ade00eeab07f857a40`.

use mtg_kernel::card_def::{
    card_id_by_name, mana_colors_mask, CardCapability, CardType, CostComponent, Keywords,
    PermanentFilterDef, Subtype, TargetSpec, CARD_DEFS, KERNEL_CARDDB_HASH,
};
use mtg_kernel::effect::{EffectOp, PlayerRef};
use mtg_kernel::engine::{self, Action, CostKind, Decision};
use mtg_kernel::event::{self, ProposedEvent, ReplacementEffectKind};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::rl::{
    legal_action_candidates_v1, observe_v2, ActionSemanticV1, PendingEffectChoiceSemanticV4,
};
use mtg_kernel::state::{
    CastMethodV4, Counters, GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone,
};
use mtg_kernel::surface_v2::{HarnessSurfaceV2, SurfaceDecision};
use mtg_kernel::trigger::{self, TriggerCondition};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn ready_main(seed: u64) -> GameState {
    let mut state = GameState::new_from_libraries(&[], &[], card_name, seed);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
}

fn put_object(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id(name);
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.to_string(),
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
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Library => state.players[player.index()].library.push(id),
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        Zone::Exile => state.exile.push(id),
        Zone::Command => state.command.push(id),
        Zone::Stack => panic!("test helper does not fabricate stack-zone objects"),
    }
    id
}

fn projected_actions(
    state: &GameState,
    decision: &Decision,
) -> Vec<mtg_kernel::rl::LegalActionCandidateV1> {
    legal_action_candidates_v1(&SurfaceDecision::Decision(decision.clone()), state)
        .expect("legal action projection")
}

fn collect_triggers(state: &mut GameState) {
    let pending = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(pending);
}

fn next_nonpriority(state: &mut GameState) -> Decision {
    for _ in 0..24 {
        let decision = engine::advance_until_decision(state);
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => return other,
        }
    }
    panic!("no non-priority decision within bounded walk")
}

fn pass_until(state: &mut GameState, done: impl Fn(&GameState) -> bool) {
    for _ in 0..32 {
        let decision = engine::advance_until_decision(state);
        if done(state) {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::Halted { mechanic, source } => {
                panic!("unexpected halt {mechanic:?} from {source}")
            }
            other => panic!("unexpected decision while passing: {other:?}"),
        }
    }
    panic!("condition did not become true within bounded priority walk")
}

fn begin_journey(state: &mut GameState, journey: ObjectId) -> Decision {
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 2;
    engine::step(state, Action::CastSpell(journey)).unwrap();
    next_nonpriority(state)
}

fn finish_journey_target(state: &mut GameState, decision: &Decision, target: ObjectId) {
    assert!(matches!(
        decision,
        Decision::ChooseTargets {
            player: PlayerId::P0,
            spell,
            legal_targets,
            ..
        } if *spell == card_id_source(state, "Journey to Nowhere")
            && legal_targets.contains(&Target::Object(target))
    ));
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
}

fn card_id_source(state: &GameState, name: &str) -> ObjectId {
    let card_def = card_id(name);
    state
        .objects
        .iter()
        .find_map(|(id, object)| (object.card_def == card_def).then_some(id))
        .unwrap_or_else(|| panic!("physical {name} source"))
}

#[test]
fn definitions_ids_hash_and_generated_programs_are_exact() {
    assert_eq!(KERNEL_CARDDB_HASH, 0xf5d4_55dd_4a3a_d03b);
    assert_eq!(CARD_DEFS.len(), 160);
    for (name, expected_id) in [
        ("Guardian of the Guildpact", 49),
        ("Journey to Nowhere", 61),
        ("Prismatic Strands", 88),
        ("The Modern Age", 115),
    ] {
        let id = card_id(name);
        assert_eq!(id, expected_id, "stable card id for {name}");
        assert_eq!(CARD_DEFS[id as usize].capability, CardCapability::Full);
    }

    let guardian = &CARD_DEFS[49];
    assert_eq!(guardian.types, &[CardType::Creature]);
    assert_eq!(guardian.subtypes, &[Subtype::Spirit]);
    assert_eq!((guardian.power, guardian.toughness), (Some(2), Some(3)));
    assert_eq!(mana_colors_mask(guardian.colors).count_ones(), 1);
    assert!(guardian.keywords.has(Keywords::PROTECTION_FROM_MONOCOLORED));

    let journey_triggers = trigger::triggers_for(61);
    assert_eq!(journey_triggers.len(), 2);
    assert_eq!(journey_triggers[0].condition, TriggerCondition::Etb);
    assert_eq!(
        journey_triggers[1].condition,
        TriggerCondition::LeftBattlefield
    );
    assert_eq!(
        trigger::target_spec_for_trigger(61, &(journey_triggers[0].effect)()),
        Some(TargetSpec::CreatureOtherThanSource)
    );
    assert_eq!(
        (journey_triggers[0].effect)(),
        EffectOp::ExileTargetLinkedToSource {
            object: mtg_kernel::effect::ObjectRef::Target(0),
        }
    );
    assert_eq!(
        (journey_triggers[1].effect)(),
        EffectOp::ReturnObjectsExiledBySource
    );

    let strands = &CARD_DEFS[88];
    assert_eq!(
        (strands.spell_effect)(),
        Some(EffectOp::PreventDamageFromChosenColorUntilEndOfTurn {
            player: PlayerRef::Controller,
        })
    );
    assert_eq!(
        strands.flashback.as_ref().unwrap().cost,
        &[CostComponent::TapUntappedControlledPermanent(
            PermanentFilterDef::CreatureWithColor(ManaColor::W)
        )]
    );

    let age = &CARD_DEFS[115];
    assert_eq!(age.subtypes, &[Subtype::Saga]);
    let face = age.transform_face.as_ref().unwrap();
    assert_eq!(face.name, "Vector Glider");
    assert_eq!(face.types, &[CardType::Enchantment, CardType::Creature]);
    assert_eq!(face.subtypes, &[Subtype::Spirit]);
    assert_eq!((face.power, face.toughness), (Some(2), Some(3)));
    assert_eq!(face.colors, &[ManaColor::U]);
    assert!(face.keywords.has(Keywords::FLYING));
    let saga = age.saga.as_ref().unwrap();
    assert_eq!(saga.chapter_effects.len(), 3);
    assert_eq!((saga.chapter_effects[0])(), (saga.chapter_effects[1])());
    assert_eq!((saga.chapter_effects[2])(), EffectOp::TransformSagaSource);
}

#[test]
fn guardian_protection_filters_monocolored_targets_blockers_and_damage() {
    let mut state = ready_main(0x4755_4152_4449_414e);
    let guardian = put_object(
        &mut state,
        PlayerId::P1,
        "Guardian of the Guildpact",
        Zone::Battlefield,
    );
    let ordinary = put_object(
        &mut state,
        PlayerId::P1,
        "Voldaren Epicure",
        Zone::Battlefield,
    );
    let bolt = put_object(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;

    let observation = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 0).unwrap();
    let public_guardian = observation.projection.battlefield[PlayerId::P1.index()]
        .iter()
        .find(|card| card.stable.arena_id == guardian.0)
        .unwrap();
    assert!(
        public_guardian
            .characteristics
            .effective_keywords
            .protection_from_monocolored
    );

    engine::step(&mut state, Action::CastSpell(bolt)).unwrap();
    let targets = next_nonpriority(&mut state);
    assert!(matches!(
        targets,
        Decision::ChooseTargets { ref legal_targets, .. }
            if !legal_targets.contains(&Target::Object(guardian))
                && legal_targets.contains(&Target::Object(ordinary))
                && legal_targets.contains(&Target::Player(PlayerId::P1))
    ));
    assert!(projected_actions(&state, &targets).iter().all(|candidate| {
        !matches!(
            candidate.record.semantic,
            ActionSemanticV1::ChooseTarget {
                target: mtg_kernel::rl::TargetRefV1::Object { ref object },
                ..
            } if object.arena_id == guardian.0
        )
    }));

    let mut combat = ready_main(0x4755_4152_4449_414f);
    let attacker = put_object(
        &mut combat,
        PlayerId::P0,
        "Guardian of the Guildpact",
        Zone::Battlefield,
    );
    let red_blocker = put_object(
        &mut combat,
        PlayerId::P1,
        "Voldaren Epicure",
        Zone::Battlefield,
    );
    let multicolor_blocker = put_object(
        &mut combat,
        PlayerId::P1,
        "Burning-Tree Emissary",
        Zone::Battlefield,
    );
    combat.step = Step::DeclareBlockers;
    combat.engine.combat.attackers = vec![attacker];
    combat.engine.combat.attackers_declared = true;
    assert!(matches!(
        engine::advance_until_decision(&mut combat),
        Decision::DeclareBlockers { legal_blockers, .. }
            if legal_blockers == vec![(attacker, vec![multicolor_blocker])]
    ));
    let before = combat.clone();
    assert!(engine::step(
        &mut combat,
        Action::DeclareBlockers(vec![(red_blocker, attacker)])
    )
    .is_err());
    assert_eq!(combat, before);

    let red_source = red_blocker;
    event::propose_and_commit(
        &mut combat,
        ProposedEvent::damage(red_source, Target::Object(attacker), 4),
    );
    assert_eq!(combat.objects.get(attacker).damage, 0);
    event::propose_and_commit(
        &mut combat,
        ProposedEvent::damage(multicolor_blocker, Target::Object(attacker), 2),
    );
    assert_eq!(combat.objects.get(attacker).damage, 2);

    let mut copied = ready_main(0x4755_4152_4449_4150);
    let protected = put_object(
        &mut copied,
        PlayerId::P0,
        "Guardian of the Guildpact",
        Zone::Battlefield,
    );
    let chain = put_object(&mut copied, PlayerId::P0, "Chain Lightning", Zone::Hand);
    copied.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
    copied.players[1].mana_pool[ManaColor::R.pool_index()] = 2;
    engine::step(&mut copied, Action::CastSpell(chain)).unwrap();
    let chain_targets = next_nonpriority(&mut copied);
    assert!(matches!(chain_targets, Decision::ChooseTargets { .. }));
    engine::step(
        &mut copied,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();
    assert!(matches!(
        next_nonpriority(&mut copied),
        Decision::ChooseSpellCopyPayment {
            player: PlayerId::P1,
            ..
        }
    ));
    engine::step(&mut copied, Action::ChooseSpellCopyPayment(true)).unwrap();
    engine::step(&mut copied, Action::ChooseSpellCopyRetarget(true)).unwrap();
    let copy_targets = engine::advance_until_decision(&mut copied);
    assert!(matches!(
        copy_targets,
        Decision::ChooseTargets { ref legal_targets, .. }
            if !legal_targets.contains(&Target::Object(protected))
    ));
    let before = copied.clone();
    assert!(engine::step(&mut copied, Action::ChooseTarget(Target::Object(protected))).is_err());
    assert_eq!(copied, before);
}

#[test]
fn journey_links_exact_exile_incarnations_and_returns_only_the_linked_object() {
    let mut state = ready_main(0x4a4f_5552_4e45_5901);
    let target = put_object(
        &mut state,
        PlayerId::P1,
        "Writhing Chrysalis",
        Zone::Battlefield,
    );
    let journey = put_object(&mut state, PlayerId::P0, "Journey to Nowhere", Zone::Hand);
    let target_decision = begin_journey(&mut state, journey);
    finish_journey_target(&mut state, &target_decision, target);
    pass_until(&mut state, |state| {
        state.objects.get(target).zone == Zone::Exile
    });

    assert_eq!(state.engine.linked_exile_records.len(), 1);
    let link = state.engine.linked_exile_records[0];
    assert_eq!(link.source.source, journey);
    assert_eq!(link.exiled, target);
    assert_eq!(
        link.source.zone_change_count,
        state.objects.get(journey).zone_change_count
    );
    assert_eq!(
        link.exiled_zone_change_count,
        state.objects.get(target).zone_change_count
    );
    let restored: GameState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert_eq!(restored, state);
    let public = serde_json::to_string(
        &observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P1, 0).unwrap(),
    )
    .unwrap();
    assert!(!public.contains("linked_exile_records"));

    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(journey, Zone::Graveyard),
    );
    collect_triggers(&mut state);
    pass_until(&mut state, |state| {
        state.objects.get(target).zone == Zone::Battlefield
    });
    assert!(state.engine.linked_exile_records.is_empty());
    assert_eq!(state.objects.get(target).controller, PlayerId::P1);
}

#[test]
fn journey_leave_before_enter_and_changed_exile_generation_are_historically_safe() {
    let mut trick = ready_main(0x4a4f_5552_4e45_5902);
    let target = put_object(
        &mut trick,
        PlayerId::P1,
        "Writhing Chrysalis",
        Zone::Battlefield,
    );
    let journey = put_object(&mut trick, PlayerId::P0, "Journey to Nowhere", Zone::Hand);
    let target_decision = begin_journey(&mut trick, journey);
    finish_journey_target(&mut trick, &target_decision, target);
    assert!(matches!(
        engine::advance_until_decision(&mut trick),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(
        trick.stack.last().unwrap().kind,
        StackItemKind::TriggeredAbility
    );

    event::propose_and_commit(
        &mut trick,
        ProposedEvent::zone_change(journey, Zone::Graveyard),
    );
    collect_triggers(&mut trick);
    pass_until(&mut trick, |state| {
        state.objects.get(target).zone == Zone::Exile
    });
    assert!(trick.engine.linked_exile_records.is_empty());
    assert_eq!(trick.objects.get(journey).zone, Zone::Graveyard);

    let mut stale = ready_main(0x4a4f_5552_4e45_5903);
    let target = put_object(
        &mut stale,
        PlayerId::P1,
        "Writhing Chrysalis",
        Zone::Battlefield,
    );
    let journey = put_object(&mut stale, PlayerId::P0, "Journey to Nowhere", Zone::Hand);
    let target_decision = begin_journey(&mut stale, journey);
    finish_journey_target(&mut stale, &target_decision, target);
    pass_until(&mut stale, |state| {
        state.objects.get(target).zone == Zone::Exile
    });
    event::propose_and_commit(
        &mut stale,
        ProposedEvent::zone_change(journey, Zone::Graveyard),
    );
    collect_triggers(&mut stale);
    let old_generation = stale.objects.get(target).zone_change_count;
    event::propose_and_commit(
        &mut stale,
        ProposedEvent::zone_change(target, Zone::Graveyard),
    );
    event::propose_and_commit(&mut stale, ProposedEvent::zone_change(target, Zone::Exile));
    assert!(stale.objects.get(target).zone_change_count > old_generation);
    pass_until(&mut stale, |state| state.stack.is_empty());
    assert_eq!(stale.objects.get(target).zone, Zone::Exile);
    assert!(stale.engine.linked_exile_records.is_empty());
}

#[test]
fn prismatic_strands_choice_is_rl_stable_and_prevents_all_chosen_color_damage() {
    let mut state = ready_main(0x5052_4953_4d41_5401);
    let strands = put_object(&mut state, PlayerId::P0, "Prismatic Strands", Zone::Hand);
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 3;
    engine::step(&mut state, Action::CastSpell(strands)).unwrap();
    let color_choice = next_nonpriority(&mut state);
    assert!(matches!(
        color_choice,
        Decision::ChooseEffectOption {
            player: PlayerId::P0,
            source,
            option_count: 5,
        } if source == strands
    ));
    let colors = projected_actions(&state, &color_choice)
        .into_iter()
        .map(|candidate| match candidate.record.semantic {
            ActionSemanticV1::ChooseEffectColor { color, .. } => color,
            other => panic!("unexpected Strands action: {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        colors,
        vec![
            ManaColor::W,
            ManaColor::U,
            ManaColor::B,
            ManaColor::R,
            ManaColor::G,
        ]
    );
    let pending = state.engine.pending_effect.as_ref().unwrap();
    assert!(matches!(
        pending.choice,
        Some(mtg_kernel::effect::PendingEffectChoice::ChooseOption { .. })
    ));
    let snapshot = serde_json::to_string(&state).unwrap();
    let restored: GameState = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(restored, state);
    let observed = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 0).unwrap();
    assert!(matches!(
        observed
            .projection
            .engine_context
            .pending_effect
            .as_ref()
            .and_then(|pending| pending.choice.as_ref()),
        Some(PendingEffectChoiceSemanticV4::Color { legal_colors, .. })
            if legal_colors.len() == 5 && !legal_colors.contains(&ManaColor::C)
    ));

    let before_bad = state.clone();
    assert!(engine::step(&mut state, Action::ChooseEffectOption(5)).is_err());
    assert_eq!(state, before_bad);
    engine::step(&mut state, Action::ChooseEffectOption(3)).unwrap();
    pass_until(&mut state, |state| {
        state.objects.get(strands).zone == Zone::Graveyard
    });
    assert!(matches!(
        state.engine.active_replacements.as_slice(),
        [replacement]
            if matches!(
                replacement.kind,
                ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn {
                    color: ManaColor::R,
                    ..
                }
            )
    ));

    let red = put_object(
        &mut state,
        PlayerId::P1,
        "Voldaren Epicure",
        Zone::Battlefield,
    );
    let green = put_object(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let creature = put_object(&mut state, PlayerId::P0, "Sacred Cat", Zone::Battlefield);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(red, Target::Player(PlayerId::P0), 5),
    );
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(red, Target::Object(creature), 4),
    );
    assert_eq!(state.players[0].life, 20);
    assert_eq!(state.objects.get(creature).damage, 0);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(green, Target::Player(PlayerId::P0), 2),
    );
    assert_eq!(state.players[0].life, 18);
    state.turn += 1;
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(red, Target::Player(PlayerId::P0), 1),
    );
    assert_eq!(state.players[0].life, 17);
}

#[test]
fn strands_flashback_taps_an_untapped_white_creature_and_is_fail_closed() {
    let mut state = ready_main(0x5052_4953_4d41_5402);
    let strands = put_object(
        &mut state,
        PlayerId::P0,
        "Prismatic Strands",
        Zone::Graveyard,
    );
    let white = put_object(
        &mut state,
        PlayerId::P0,
        "Guardian of the Guildpact",
        Zone::Battlefield,
    );
    state.objects.get_mut(white).summoning_sick = true;
    let second_white = put_object(&mut state, PlayerId::P0, "Sacred Cat", Zone::Battlefield);
    let tapped_white = put_object(
        &mut state,
        PlayerId::P0,
        "Guardian of the Guildpact",
        Zone::Battlefield,
    );
    state.objects.get_mut(tapped_white).tapped = true;
    let nonwhite = put_object(&mut state, PlayerId::P0, "Faerie Seer", Zone::Battlefield);

    engine::step(&mut state, Action::CastSpell(strands)).unwrap();
    let cost = engine::advance_until_decision(&mut state);
    assert!(matches!(
        cost,
        Decision::ChooseCostTargets {
            player: PlayerId::P0,
            source,
            cost_kind: CostKind::TapPermanents,
            remaining: 1,
            ref candidates,
        } if source == strands && candidates == &vec![white, second_white]
    ));
    let before_bad = state.clone();
    assert!(engine::step(&mut state, Action::ChooseCostTarget(nonwhite)).is_err());
    assert_eq!(state, before_bad);

    let mut stale = state.clone();
    stale.objects.get_mut(white).tapped = true;
    let stale_before_action = stale.clone();
    assert!(engine::step(&mut stale, Action::ChooseCostTarget(white)).is_err());
    assert_eq!(stale, stale_before_action);

    engine::step(&mut state, Action::ChooseCostTarget(white)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(state.objects.get(white).tapped);
    assert!(state.players[0].mana_pool.iter().all(|mana| *mana == 0));
    let item = state.stack.last().unwrap();
    assert_eq!(item.kind, StackItemKind::Spell);
    assert!(item.is_flashback);
    assert_eq!(item.v4.cast_method, Some(CastMethodV4::Flashback));
    assert_eq!(item.v4.paid_cost_refs.len(), 1);
    assert_eq!(item.v4.paid_cost_refs[0].object, white);

    let color_choice = next_nonpriority(&mut state);
    assert!(matches!(color_choice, Decision::ChooseEffectOption { .. }));
    engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
    pass_until(&mut state, |state| {
        state.objects.get(strands).zone == Zone::Exile
    });
}

#[test]
fn modern_age_chapters_loot_privately_then_transform_with_exact_face_state() {
    let mut state = ready_main(0x4d4f_4445_524e_4147);
    put_object(&mut state, PlayerId::P0, "Thoughtcast", Zone::Library);
    put_object(&mut state, PlayerId::P0, "Mountain", Zone::Library);
    let fodder = put_object(&mut state, PlayerId::P0, "Ponder", Zone::Hand);
    let age = put_object(&mut state, PlayerId::P0, "The Modern Age", Zone::Hand);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 2;
    engine::step(&mut state, Action::CastSpell(age)).unwrap();
    let first_discard = next_nonpriority(&mut state);
    assert!(matches!(
        first_discard,
        Decision::Discard {
            player: PlayerId::P0,
            count: 1,
            ..
        }
    ));
    assert_eq!(state.objects.get(age).counters.lore, 1);
    assert_eq!(state.objects.get(age).v4.face_index, 0);
    let opponent = serde_json::to_string(
        &observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P1, 0).unwrap(),
    )
    .unwrap();
    assert!(!opponent.contains("Thoughtcast"));
    engine::step(&mut state, Action::Discard(vec![fodder])).unwrap();
    assert_eq!(state.objects.get(fodder).zone, Zone::Graveyard);

    state.step = Step::Draw;
    state.priority_player = PlayerId::P0;
    state.engine.priority_passes = [false, false];
    let second_discard = next_nonpriority(&mut state);
    assert!(matches!(second_discard, Decision::Discard { count: 1, .. }));
    assert_eq!(state.step, Step::Main1);
    assert_eq!(state.objects.get(age).counters.lore, 2);
    let discard = state.players[0]
        .hand
        .iter()
        .copied()
        .find(|&object| object != age)
        .unwrap();
    engine::step(&mut state, Action::Discard(vec![discard])).unwrap();

    state.step = Step::Draw;
    state.priority_player = PlayerId::P0;
    state.engine.priority_passes = [false, false];
    let mut saw_final_trigger = false;
    for _ in 0..16 {
        let decision = engine::advance_until_decision(&mut state);
        if state.stack.last().is_some_and(|item| {
            item.kind == StackItemKind::TriggeredAbility
                && item.source == age
                && item.inline_effect == Some(EffectOp::TransformSagaSource)
        }) {
            saw_final_trigger = true;
            assert_eq!(state.objects.get(age).zone, Zone::Battlefield);
            assert_eq!(state.objects.get(age).counters.lore, 3);
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            Decision::Halted { mechanic, source } => {
                panic!("unexpected final-chapter halt {mechanic:?} from {source}")
            }
            other => panic!("unexpected final-chapter decision: {other:?}"),
        }
        if state.objects.get(age).v4.face_index == 1 {
            break;
        }
    }
    assert!(saw_final_trigger);
    let transformed = state.objects.get(age);
    assert_eq!(transformed.zone, Zone::Battlefield);
    assert_eq!(transformed.controller, PlayerId::P0);
    assert_eq!(transformed.name, "Vector Glider");
    assert_eq!(transformed.v4.face_index, 1);
    assert_eq!(transformed.counters.lore, 0);
    assert!(transformed.summoning_sick);
    assert!(engine::object_has_type(&state, age, CardType::Enchantment));
    assert!(engine::object_has_type(&state, age, CardType::Creature));
    assert_eq!(engine::effective_power(&state, age), 2);
    assert_eq!(engine::effective_toughness(&state, age), 3);
    assert!(engine::has_effective_keyword(&state, age, Keywords::FLYING));

    let restored: GameState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert_eq!(restored, state);
    let observation = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P1, 0).unwrap();
    let face = observation.projection.battlefield[0]
        .iter()
        .find(|card| card.stable.arena_id == age.0)
        .unwrap();
    assert_eq!(face.face_index, 1);
    assert_eq!(face.card_name, "Vector Glider");
    assert_eq!(face.characteristics.base_power, Some(2));
    assert_eq!(face.characteristics.base_toughness, Some(3));
    assert!(face.characteristics.effective_keywords.flying);
}
