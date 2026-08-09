//! Focused rules and RL coverage for the artifact and Equipment completion wave.
//! Current checked-in Mage Java sources are the behavior authority.

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, CostComponent, Keywords, Subtype, TargetSpec,
    CARD_DEFS,
};
use mtg_kernel::effect::{self, EffectOp, ExecCtx, ObjectRef};
use mtg_kernel::engine::{self, Action, Decision, UnsupportedMechanic};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId, StackItemId};
use mtg_kernel::mana::{Cost, ManaColor};
use mtg_kernel::rl::{card_name, observe_v2, EffectDurationV2, PlayerSeatV1};
use mtg_kernel::state::{
    AbilitySourceContractV4, Counters, GameObject, GameState, ObjectLinkV4, ObjectStateV4,
    StackItem, StackItemKind, StackStateV4, StackTargetContractV4, Step, Target, Zone,
};
use mtg_kernel::surface_v2::HarnessSurfaceV2;
use mtg_kernel::trigger::{self, PendingTrigger};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn ready_main(seed: u64) -> GameState {
    let library = vec![card_id("Mountain"); 12];
    let mut state = GameState::new_from_libraries(&library, &library, card_name, seed);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
}

fn put_object(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id(name);
    let object = state.objects.push(GameObject {
        card_def,
        name: name.to_string(),
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
        Zone::Stack => panic!("test helper does not construct stack items"),
    }
    object
}

fn attach_exact_for_test(state: &mut GameState, equipment: ObjectId, host: ObjectId) {
    let host_generation = state.objects.get(host).zone_change_count;
    state.objects.get_mut(equipment).v4.attached_to = Some(ObjectLinkV4 {
        object: host,
        zone_change_count: host_generation,
    });
    state.objects.get_mut(host).attachments.push(equipment);
    state.objects.get_mut(host).attachments.sort_unstable();
    state.objects.get_mut(host).attachments.dedup();
    state.validate_attachment_relations().unwrap();
}

fn add_generic_mana(state: &mut GameState, amount: u8) {
    state.players[PlayerId::P0.index()].mana_pool[ManaColor::C.pool_index()] += amount;
}

fn resolve_until_idle(state: &mut GameState) {
    for _ in 0..96 {
        let decision = engine::advance_until_decision(state);
        if let Decision::Halted { mechanic, source } = decision {
            panic!("engine halted while resolving {source:?}: {mechanic:?}");
        }
        if state.stack.is_empty()
            && state.engine.pending_cast.is_none()
            && state.engine.pending_activation.is_none()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision while resolving: {other:?}"),
        }
    }
    panic!("stack did not settle within the focused bound");
}

fn pass_until_halted(state: &mut GameState) -> (UnsupportedMechanic, ObjectId) {
    for _ in 0..8 {
        match engine::advance_until_decision(state) {
            Decision::Halted { mechanic, source } => return (mechanic, source),
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision before fail-closed halt: {other:?}"),
        }
    }
    panic!("tampered stack did not halt within the focused bound");
}

fn execute_pending_trigger(state: &mut GameState, pending: &PendingTrigger) {
    effect::execute(
        &pending.effect,
        &ExecCtx {
            source: pending.source,
            ability_source_contract: pending.source_contract,
            controller: pending.controller,
            stack_item_id: None,
            targets: pending.targets.clone(),
            target_contracts: pending.target_contracts.clone(),
            discarded: Vec::new(),
            paid_cost_refs: Vec::new(),
            hidden_ability_source: None,
            kicked: pending.kicked,
        },
        state,
    );
}

#[test]
fn generated_registry_and_recipes_match_current_mage_authority() {
    assert_eq!(card_id("Black Mage's Rod"), 5);
    assert_eq!(card_id("Cryogen Relic"), 18);
    assert_eq!(card_id("Gorilla Shaman"), 46);
    assert_eq!(card_id("Hunter's Blowgun"), 56);
    assert_eq!(card_id("Unexpected Fangs"), 124);
    assert_eq!(card_id("Hero Token"), 159);
    assert_eq!(CARD_DEFS.len(), 160);

    let rod = &CARD_DEFS[card_id("Black Mage's Rod") as usize];
    assert_eq!(rod.capability, CardCapability::Full);
    let rod_equipment = rod.equipment.expect("Rod is generic Equipment");
    assert_eq!(
        (rod_equipment.power_delta, rod_equipment.toughness_delta),
        (1, 0)
    );
    assert_eq!(rod_equipment.add_subtype, Some(Subtype::Wizard));
    assert_eq!(rod_equipment.noncreature_spell_damage_to_each_opponent, 1);
    assert!(rod_equipment.job_select);
    assert_eq!(
        rod.activated_abilities[0].target_spec,
        TargetSpec::ControlledCreature
    );

    let relic = &CARD_DEFS[card_id("Cryogen Relic") as usize];
    assert_eq!(relic.capability, CardCapability::Full);
    assert_eq!(
        relic.activated_abilities[0].target_spec,
        TargetSpec::UpToOneTappedCreature
    );

    let gorilla = &CARD_DEFS[card_id("Gorilla Shaman") as usize];
    assert_eq!(gorilla.types, &[CardType::Creature]);
    assert_eq!(
        gorilla.activated_abilities[0].target_spec,
        TargetSpec::NoncreatureArtifactPermanent
    );
    assert_eq!(
        gorilla.activated_abilities[0].cost,
        &[CostComponent::Mana(Cost {
            pips: &[],
            generic: 1,
            x_count: 2,
        })]
    );

    let blowgun = &CARD_DEFS[card_id("Hunter's Blowgun") as usize];
    let blowgun_equipment = blowgun.equipment.expect("Blowgun is generic Equipment");
    assert_eq!(
        (
            blowgun_equipment.power_delta,
            blowgun_equipment.toughness_delta
        ),
        (1, 1)
    );
    assert!(blowgun_equipment
        .controller_turn_keywords
        .has(Keywords::DEATHTOUCH));
    assert!(blowgun_equipment.other_turn_keywords.has(Keywords::REACH));

    let fangs = &CARD_DEFS[card_id("Unexpected Fangs") as usize];
    assert_eq!(fangs.target_spec, TargetSpec::Creature);
    assert_eq!(
        (fangs.spell_effect)(),
        Some(EffectOp::AddCountersToTarget {
            target_index: 0,
            optional: false,
            plus1_plus1: 1,
            lifelink: 1,
            stun: 0,
        })
    );

    let hero = &CARD_DEFS[card_id("Hero Token") as usize];
    assert!(hero.is_token);
    assert_eq!(hero.subtypes, &[Subtype::Hero]);
    assert_eq!((hero.power, hero.toughness), (Some(1), Some(1)));
}

#[test]
fn job_select_and_equipment_characteristics_are_exact_and_turn_conditional() {
    let mut state = ready_main(0x4a4f_4253_454c_4543);
    let rod = put_object(&mut state, PlayerId::P0, "Black Mage's Rod", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(rod, Zone::Battlefield),
    );
    let triggers = trigger::collect_and_process(&mut state);
    assert_eq!(triggers.len(), 1);
    assert!(matches!(
        triggers[0].effect,
        EffectOp::CreateTokenAndAttachSource { token_def } if token_def == card_id("Hero Token")
    ));
    execute_pending_trigger(&mut state, &triggers[0]);

    let hero = state.players[0]
        .battlefield
        .iter()
        .copied()
        .find(|id| state.objects.get(*id).card_def == card_id("Hero Token"))
        .expect("Job Select created a Hero");
    assert_eq!(state.objects.get(rod).v4.attached_to.unwrap().object, hero);
    assert_eq!(state.objects.get(hero).attachments, vec![rod]);
    assert_eq!(engine::effective_power(&state, hero), 2);
    assert_eq!(engine::effective_toughness(&state, hero), 1);
    assert!(engine::has_effective_subtype(&state, hero, Subtype::Wizard));

    let blowgun = put_object(
        &mut state,
        PlayerId::P0,
        "Hunter's Blowgun",
        Zone::Battlefield,
    );
    attach_exact_for_test(&mut state, blowgun, hero);
    assert_eq!(engine::effective_power(&state, hero), 3);
    assert_eq!(engine::effective_toughness(&state, hero), 2);
    assert!(engine::has_effective_keyword(
        &state,
        hero,
        Keywords::DEATHTOUCH
    ));
    assert!(!engine::has_effective_keyword(
        &state,
        hero,
        Keywords::REACH
    ));

    state.active_player = PlayerId::P1;
    assert!(!engine::has_effective_keyword(
        &state,
        hero,
        Keywords::DEATHTOUCH
    ));
    assert!(engine::has_effective_keyword(&state, hero, Keywords::REACH));

    state.active_player = PlayerId::P0;
    let victim = put_object(&mut state, PlayerId::P1, "Guttersnipe", Zone::Battlefield);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(hero, Target::Object(victim), 1),
    );
    trigger::sba_fixed_point(&mut state);
    assert_eq!(state.objects.get(victim).zone, Zone::Graveyard);
}

#[test]
fn equipment_activation_reattaches_and_stale_source_contract_fails_closed() {
    let mut state = ready_main(0x4551_5549_5000_0001);
    let rod = put_object(
        &mut state,
        PlayerId::P0,
        "Black Mage's Rod",
        Zone::Battlefield,
    );
    let first = put_object(
        &mut state,
        PlayerId::P0,
        "Voldaren Epicure",
        Zone::Battlefield,
    );
    let second = put_object(&mut state, PlayerId::P0, "Guttersnipe", Zone::Battlefield);

    add_generic_mana(&mut state, 3);
    engine::step(&mut state, Action::ActivateAbility(rod, 0)).unwrap();
    let choose = engine::advance_until_decision(&mut state);
    assert!(matches!(
        choose,
        Decision::ChooseTargets { ref legal_targets, .. }
            if legal_targets.contains(&Target::Object(first))
                && legal_targets.contains(&Target::Object(second))
    ));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(first))).unwrap();
    resolve_until_idle(&mut state);
    assert_eq!(state.objects.get(rod).v4.attached_to.unwrap().object, first);
    assert_eq!(state.objects.get(first).attachments, vec![rod]);

    add_generic_mana(&mut state, 3);
    engine::step(&mut state, Action::ActivateAbility(rod, 0)).unwrap();
    engine::advance_until_decision(&mut state);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(second))).unwrap();
    engine::advance_until_decision(&mut state);
    let item = state
        .stack
        .last_mut()
        .expect("equip activation reached stack");
    item.v4
        .ability_source_contract
        .as_mut()
        .expect("activation has exact source")
        .zone_change_count += 1;
    assert_eq!(
        pass_until_halted(&mut state),
        (UnsupportedMechanic::InvalidEffectContinuation, rod)
    );
    assert_eq!(state.objects.get(rod).v4.attached_to.unwrap().object, first);
}

#[test]
fn rods_trigger_is_owned_by_the_equipped_creature_and_only_matches_noncreatures() {
    let mut state = ready_main(0x524f_4454_5249_4747);
    let rod = put_object(
        &mut state,
        PlayerId::P0,
        "Black Mage's Rod",
        Zone::Battlefield,
    );
    let host = put_object(
        &mut state,
        PlayerId::P1,
        "Voldaren Epicure",
        Zone::Battlefield,
    );
    state.objects.get_mut(host).controller = PlayerId::P0;
    attach_exact_for_test(&mut state, rod, host);

    let noncreature = put_object(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    state.players[0].hand.retain(|id| *id != noncreature);
    state.objects.get_mut(noncreature).zone = Zone::Stack;
    event::log_spell_cast(&mut state, noncreature, PlayerId::P0);
    let triggers = trigger::collect_and_process(&mut state);
    assert_eq!(triggers.len(), 1);
    assert_eq!(triggers[0].source, host);
    assert_eq!(triggers[0].source_contract.unwrap().source, host);
    assert_eq!(triggers[0].granted_by.unwrap().source, rod);
    execute_pending_trigger(&mut state, &triggers[0]);
    assert_eq!(state.players[1].life, 19);

    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(noncreature, Zone::Graveyard),
    );
    state.engine.next_stack_item_id = 1;
    state.stack.push(StackItem {
        kind: StackItemKind::TriggeredAbility,
        source: host,
        controller: PlayerId::P0,
        targets: Vec::new(),
        is_copy: false,
        inline_effect: Some(triggers[0].effect.clone()),
        discarded: Vec::new(),
        is_flashback: false,
        mode_chosen: 0,
        madness_offer: false,
        kicked: false,
        v4: StackStateV4 {
            stack_item_id: StackItemId(1),
            ability_source_contract: triggers[0].source_contract,
            granted_by: triggers[0].granted_by,
            ..StackStateV4::default()
        },
    });
    observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 0).unwrap();
    state.stack[0]
        .v4
        .granted_by
        .as_mut()
        .expect("trigger has exact Equipment producer")
        .attached_to = None;
    assert!(
        observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 1)
            .unwrap_err()
            .to_string()
            .contains("attachment-granted trigger source contract")
    );
    state.stack.clear();

    let creature = put_object(&mut state, PlayerId::P0, "Myr Enforcer", Zone::Hand);
    state.players[0].hand.retain(|id| *id != creature);
    state.objects.get_mut(creature).zone = Zone::Stack;
    event::log_spell_cast(&mut state, creature, PlayerId::P0);
    assert!(trigger::collect_and_process(&mut state).is_empty());
}

#[test]
fn cryogen_relic_draws_twice_and_stuns_only_an_optional_tapped_creature() {
    let mut state = ready_main(0x4352_594f_4745_4e01);
    let relic = put_object(&mut state, PlayerId::P0, "Cryogen Relic", Zone::Hand);
    let tapped = put_object(&mut state, PlayerId::P1, "Guttersnipe", Zone::Battlefield);
    let untapped = put_object(
        &mut state,
        PlayerId::P1,
        "Voldaren Epicure",
        Zone::Battlefield,
    );
    state.objects.get_mut(tapped).tapped = true;

    let hand_before = state.players[0].hand.len();
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(relic, Zone::Battlefield),
    );
    let entry = trigger::collect_and_process(&mut state);
    assert_eq!(entry.len(), 1);
    execute_pending_trigger(&mut state, &entry[0]);
    assert_eq!(state.players[0].hand.len(), hand_before);

    state.players[0].mana_pool[ManaColor::U.pool_index()] = 1;
    add_generic_mana(&mut state, 1);
    engine::step(&mut state, Action::ActivateAbility(relic, 0)).unwrap();
    let choice = engine::advance_until_decision(&mut state);
    assert!(
        matches!(
            choice,
            Decision::ChooseEffectTargets {
                ref legal_targets,
                can_finish: true,
                ..
            } if legal_targets == &vec![Target::Object(tapped)]
                && !legal_targets.contains(&Target::Object(untapped))
        ),
        "unexpected Cryogen target decision: {choice:?}"
    );
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(tapped)),
    )
    .unwrap();
    resolve_until_idle(&mut state);
    assert_eq!(state.objects.get(relic).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(tapped).counters.stun, 1);
    assert_eq!(
        state.players[0].hand.len(),
        hand_before + 1,
        "the leave trigger drew the second card"
    );

    let mut optional = ready_main(0x4352_594f_4745_4e02);
    let relic = put_object(
        &mut optional,
        PlayerId::P0,
        "Cryogen Relic",
        Zone::Battlefield,
    );
    optional.players[0].mana_pool[ManaColor::U.pool_index()] = 1;
    add_generic_mana(&mut optional, 1);
    engine::step(&mut optional, Action::ActivateAbility(relic, 0)).unwrap();
    match engine::advance_until_decision(&mut optional) {
        Decision::ChooseEffectTargets {
            can_finish: true,
            legal_targets,
            ..
        } => {
            assert!(legal_targets.is_empty());
            engine::step(&mut optional, Action::FinishEffectSelection).unwrap();
        }
        Decision::CastSpellOrPass { .. } => {}
        other => panic!("unexpected optional Cryogen decision: {other:?}"),
    }
    resolve_until_idle(&mut optional);
    assert_eq!(optional.objects.get(relic).zone, Zone::Graveyard);
}

#[test]
fn gorilla_shaman_binds_double_x_to_noncreature_artifact_mana_value() {
    let mut zero = ready_main(0x474f_5249_4c4c_4100);
    let gorilla = put_object(&mut zero, PlayerId::P0, "Gorilla Shaman", Zone::Battlefield);
    let furnace = put_object(&mut zero, PlayerId::P1, "Great Furnace", Zone::Battlefield);
    let artifact_creature = put_object(&mut zero, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
    add_generic_mana(&mut zero, 1);
    engine::step(&mut zero, Action::ActivateAbility(gorilla, 0)).unwrap();
    let choice = engine::advance_until_decision(&mut zero);
    assert!(matches!(
        choice,
        Decision::ChooseTargets { ref legal_targets, .. }
            if legal_targets.contains(&Target::Object(furnace))
                && !legal_targets.contains(&Target::Object(artifact_creature))
    ));
    engine::step(&mut zero, Action::ChooseTarget(Target::Object(furnace))).unwrap();
    engine::advance_until_decision(&mut zero);
    assert_eq!(zero.stack.last().unwrap().v4.x_value, 0);
    resolve_until_idle(&mut zero);
    assert_eq!(zero.objects.get(furnace).zone, Zone::Graveyard);

    let mut two = ready_main(0x474f_5249_4c4c_4102);
    let gorilla = put_object(&mut two, PlayerId::P0, "Gorilla Shaman", Zone::Battlefield);
    let wellspring = put_object(
        &mut two,
        PlayerId::P1,
        "Ichor Wellspring",
        Zone::Battlefield,
    );
    add_generic_mana(&mut two, 4);
    assert!(engine::step(&mut two, Action::ActivateAbility(gorilla, 0)).is_err());
    add_generic_mana(&mut two, 1);
    engine::step(&mut two, Action::ActivateAbility(gorilla, 0)).unwrap();
    engine::advance_until_decision(&mut two);
    engine::step(&mut two, Action::ChooseTarget(Target::Object(wellspring))).unwrap();
    engine::advance_until_decision(&mut two);
    assert_eq!(two.stack.last().unwrap().v4.x_value, 2);
    assert_eq!(two.players[0].mana_pool[ManaColor::C.pool_index()], 0);

    two.stack.last_mut().unwrap().v4.x_value = 1;
    assert_eq!(
        pass_until_halted(&mut two),
        (UnsupportedMechanic::InvalidEffectContinuation, gorilla)
    );
    assert_eq!(two.objects.get(wellspring).zone, Zone::Battlefield);
}

#[test]
fn unexpected_fangs_counters_are_permanent_lifelink_and_incarnation_bound() {
    let mut state = ready_main(0x4641_4e47_5300_0001);
    let fangs = put_object(&mut state, PlayerId::P0, "Unexpected Fangs", Zone::Hand);
    let creature = put_object(
        &mut state,
        PlayerId::P0,
        "Voldaren Epicure",
        Zone::Battlefield,
    );
    state.players[0].mana_pool[ManaColor::B.pool_index()] = 1;
    add_generic_mana(&mut state, 1);
    engine::step(&mut state, Action::CastSpell(fangs)).unwrap();
    engine::advance_until_decision(&mut state);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(creature))).unwrap();
    resolve_until_idle(&mut state);
    assert_eq!(state.objects.get(creature).counters.plus1_plus1, 1);
    assert_eq!(state.objects.get(creature).v4.lifelink_keyword_counters, 1);
    assert_eq!(engine::effective_power(&state, creature), 2);
    assert_eq!(engine::effective_toughness(&state, creature), 2);
    assert!(engine::has_effective_keyword(
        &state,
        creature,
        Keywords::LIFELINK
    ));
    let life_before = state.players[0].life;
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(creature, Target::Player(PlayerId::P1), 2),
    );
    assert_eq!(state.players[0].life, life_before + 2);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(creature, Zone::Graveyard),
    );
    assert_eq!(state.objects.get(creature).counters.plus1_plus1, 0);
    assert_eq!(state.objects.get(creature).v4.lifelink_keyword_counters, 0);

    let mut stale = ready_main(0x4641_4e47_5300_0002);
    let source = put_object(&mut stale, PlayerId::P0, "Unexpected Fangs", Zone::Hand);
    let target = put_object(
        &mut stale,
        PlayerId::P0,
        "Voldaren Epicure",
        Zone::Battlefield,
    );
    let old_contract = StackTargetContractV4::capture(&stale, Target::Object(target));
    event::propose_and_commit(
        &mut stale,
        ProposedEvent::zone_change(target, Zone::Graveyard),
    );
    event::propose_and_commit(
        &mut stale,
        ProposedEvent::zone_change(target, Zone::Battlefield),
    );
    effect::execute(
        &(CARD_DEFS[card_id("Unexpected Fangs") as usize].spell_effect)().unwrap(),
        &ExecCtx {
            source,
            ability_source_contract: None,
            controller: PlayerId::P0,
            stack_item_id: None,
            targets: vec![Target::Object(target)],
            target_contracts: vec![old_contract],
            discarded: Vec::new(),
            paid_cost_refs: Vec::new(),
            hidden_ability_source: None,
            kicked: false,
        },
        &mut stale,
    );
    assert_eq!(stale.objects.get(target).counters.plus1_plus1, 0);
    assert_eq!(stale.objects.get(target).v4.lifelink_keyword_counters, 0);
}

#[test]
fn rl_projection_exposes_exact_equipment_effects_and_rejects_tampering() {
    let mut state = ready_main(0x524c_4551_5549_5001);
    let host = put_object(
        &mut state,
        PlayerId::P0,
        "Voldaren Epicure",
        Zone::Battlefield,
    );
    let blowgun = put_object(
        &mut state,
        PlayerId::P0,
        "Hunter's Blowgun",
        Zone::Battlefield,
    );
    attach_exact_for_test(&mut state, blowgun, host);
    state.objects.get_mut(host).counters.plus1_plus1 = 1;
    state.objects.get_mut(host).v4.lifelink_keyword_counters = 1;

    for observer in [PlayerId::P0, PlayerId::P1] {
        let observation = observe_v2(
            &state,
            &HarnessSurfaceV2::new(),
            observer,
            u64::from(observer.index() as u32),
        )
        .unwrap();
        let public_host = observation.projection.battlefield[PlayerSeatV1::P0 as usize]
            .iter()
            .find(|card| card.stable.arena_id == host.0)
            .unwrap();
        assert_eq!(public_host.characteristics.effective_power, Some(3));
        assert_eq!(public_host.characteristics.effective_toughness, Some(3));
        assert!(public_host.characteristics.effective_keywords.deathtouch);
        assert!(public_host.characteristics.effective_keywords.lifelink);
        assert_eq!(observation.projection.object_relations.len(), 1);
        let equipment_effect = observation
            .projection
            .continuous_effects
            .iter()
            .find(|effect| {
                effect
                    .source
                    .as_ref()
                    .is_some_and(|source| source.arena_id == blowgun.0)
            })
            .expect("Equipment effect is public");
        assert_eq!(equipment_effect.duration, EffectDurationV2::WhileAttached);
        assert_eq!(
            (
                equipment_effect.power_delta,
                equipment_effect.toughness_delta
            ),
            (1, 1)
        );
        assert_eq!(equipment_effect.add_keyword_mask, Keywords::DEATHTOUCH.0);
    }

    state.objects.get_mut(host).attachments.clear();
    assert!(
        observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 2)
            .unwrap_err()
            .to_string()
            .contains("invalid attachment relation")
    );
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source,
        } if source == blowgun
    ));
}

#[test]
fn attach_effect_rejects_a_stale_target_incarnation_without_mutation() {
    let mut state = ready_main(0x4154_5441_4348_0001);
    let equipment = put_object(
        &mut state,
        PlayerId::P0,
        "Hunter's Blowgun",
        Zone::Battlefield,
    );
    let target = put_object(
        &mut state,
        PlayerId::P0,
        "Voldaren Epicure",
        Zone::Battlefield,
    );
    let source_contract = AbilitySourceContractV4::capture(&state, equipment);
    let target_contract = StackTargetContractV4::capture(&state, Target::Object(target));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(target, Zone::Graveyard),
    );
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(target, Zone::Battlefield),
    );
    effect::execute(
        &EffectOp::AttachSourceToTarget {
            object: ObjectRef::Target(0),
        },
        &ExecCtx {
            source: equipment,
            ability_source_contract: Some(source_contract),
            controller: PlayerId::P0,
            stack_item_id: None,
            targets: vec![Target::Object(target)],
            target_contracts: vec![target_contract],
            discarded: Vec::new(),
            paid_cost_refs: Vec::new(),
            hidden_ability_source: None,
            kicked: false,
        },
        &mut state,
    );
    assert!(state.objects.get(equipment).v4.attached_to.is_none());
    assert!(state.objects.get(target).attachments.is_empty());
}
