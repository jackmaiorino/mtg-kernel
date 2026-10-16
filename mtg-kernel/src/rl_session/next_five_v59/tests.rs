//! Genuine native paid stages through the distinct owner. Support operands are
//! labelled and never substituted for a paid printing or a private journal.
use super::*;
use crate::{
    card_def::{self, CARD_DEFS},
    engine::{self, Action, Decision},
    event::{self, ProposedEvent},
    next_five_public_v59::tests as native,
    policy_observation_v6::tests::put,
    state::{GameState, Target, Zone},
};

fn ready(actor: PlayerId) -> GameState {
    let mut state = native::ready(actor);
    // A genuine available land drop keeps the intended priority windows open.
    for player in [actor, actor.opponent()] {
        put(&mut state, player, "Forest", Zone::Hand);
    }
    state
}
fn own(state: GameState) -> NextFiveSessionV59 {
    NextFiveSessionV59::from_session(FastActorSessionV1::from_next_five_v59_fixture_state(state))
        .unwrap()
}
fn state(owner: &NextFiveSessionV59) -> &GameState {
    owner.fixture_state_v59()
}
fn gameplay(owner: &NextFiveSessionV59) -> (Vec<u8>, (u64, u64)) {
    (
        serde_json::to_vec(state(owner)).unwrap(),
        owner.fixture_counts_v59(),
    )
}
fn act(owner: &mut NextFiveSessionV59, predicate: impl Fn(&ActionSemanticV1) -> bool) {
    let current = owner.decision_v59().unwrap();
    let index = current
        .actions()
        .iter()
        .position(predicate)
        .unwrap_or_else(|| panic!("actual V59 action absent: {:?}", current.actions()));
    owner.validate_decision_v59(&current).unwrap();
    let mut reference = owner.inner.clone();
    let expected = current.expected();
    let actual = owner
        .step_v59(&current, u32::try_from(index).unwrap())
        .unwrap();
    assert_eq!(
        actual,
        reference
            .step(
                expected.episode_id,
                expected.step,
                u32::try_from(index).unwrap()
            )
            .unwrap()
    );
    assert_eq!(state(owner), reference.next_five_fixture_state());
    assert_eq!(
        owner.fixture_counts_v59(),
        (
            reference.policy_step_count(),
            reference.physical_decision_count()
        )
    );
}
fn drain(owner: &mut NextFiveSessionV59) {
    for _ in 0..80 {
        let s = state(owner);
        if s.stack.is_empty()
            && s.engine.pending_cast.is_none()
            && s.engine.pending_activation.is_none()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_triggers.is_empty()
        {
            return;
        }
        act(owner, |a| matches!(a, ActionSemanticV1::Pass { .. }));
    }
    panic!("actual paid resolution did not drain");
}
fn tapped(s: &GameState, actor: PlayerId) -> usize {
    s.players[actor.index()]
        .battlefield
        .iter()
        .filter(|id| s.objects.get(**id).tapped)
        .count()
}
fn cast(owner: &mut NextFiveSessionV59, spell: ObjectId, target: Option<ObjectId>, x: Option<u16>) {
    let actor = state(owner).objects.get(spell).owner;
    let before = tapped(state(owner), actor);
    act(
        owner,
        |a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==spell.0),
    );
    if let Some(target) = target {
        let current = owner.decision_v59().unwrap();
        assert!(current.observation().next_five.goblin_quotes.is_empty());
        act(
            owner,
            |a| matches!(a,ActionSemanticV1::ChooseTarget{target:TargetRefV1::Object{object},..}if object.arena_id==target.0),
        );
    }
    if let Some(x) = x {
        let current = owner.decision_v59().unwrap();
        assert!(current.observation().next_five.goblin_quotes.is_empty());
        act(
            owner,
            |a| matches!(a,ActionSemanticV1::ChooseEffectOption{option_index,..}if *option_index==x),
        );
    }
    assert!(state(owner).engine.pending_cast.is_none());
    assert_eq!(state(owner).objects.get(spell).zone, Zone::Stack);
    let definition = &CARD_DEFS[usize::from(state(owner).objects.get(spell).card_def)];
    assert_eq!(
        tapped(state(owner), actor) - before,
        usize::from(definition.cost.generic)
            + definition.cost.pips.len()
            + usize::from(x.unwrap_or(0))
    );
}
fn check_artifact(owner: &NextFiveSessionV59) {
    let current = owner.decision_v59().unwrap();
    let observation = current.observation();
    crate::next_five_observation_v59::validate_artifact(observation).unwrap();
    assert_eq!(
        observation.next_five.viewer,
        current.expected().acting_player
    );
    assert_eq!(observation.action_meanings.len(), current.actions().len());
    assert_ne!(
        observation.common.schema,
        crate::prepared_observation_v57::ELENDA_COMPAT_SCHEMA
    );
    assert_ne!(
        observation.common.common.schema,
        crate::prepared_observation_v57::COMMON_SCHEMA
    );
    assert!(crate::prepared_observation_v57::validate_artifact(
        &crate::prepared_observation_v57::PreparedObservationV57 {
            schema: observation.schema.clone(),
            producer_sha256: observation.producer_sha256.clone(),
            common: observation.common.clone(),
            drake: observation.drake.clone(),
            spinner: observation.spinner.clone(),
            quandary: observation.quandary.clone(),
            phoenix: observation.phoenix.clone(),
            action_meanings: vec![],
            projection_sha256: observation.projection_sha256.clone(),
        }
    )
    .is_err());
    let value = serde_json::to_value(observation).unwrap();
    let parsed: NextFiveObservationV59 = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(parsed, *observation);
    let mut extra = value;
    extra
        .as_object_mut()
        .unwrap()
        .insert("original_world".into(), serde_json::json!({}));
    assert!(serde_json::from_value::<NextFiveObservationV59>(extra).is_err());
}

#[test]
fn genuine_reference_inputs_stay_partial_and_full_only_reset_refuses_both_seats() {
    for text in [
        include_str!("../../../../data/limited/fdn_v1/FDN_reference_midnight_snack_partial.dck"),
        include_str!(
            "../../../../data/limited/fdn_v1/FDN_reference_extravagant_replication_partial.dck"
        ),
        include_str!("../../../../data/limited/fdn_v1/FDN_reference_infernal_vessel_partial.dck"),
        include_str!("../../../../data/limited/fdn_v1/FDN_reference_giada_partial.dck"),
        include_str!(
            "../../../../data/limited/fdn_v1/FDN_reference_goblin_negotiation_partial.dck"
        ),
    ] {
        let mut deck = vec![];
        for line in text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with("//"))
        {
            let (count, name) = line.split_once(' ').unwrap();
            let name = if name.starts_with('[') {
                name.split_once(']').unwrap().1.trim()
            } else {
                name.trim()
            };
            let id = card_def::card_id_by_name(name).unwrap();
            deck.extend(std::iter::repeat_n(id, count.parse::<usize>().unwrap()));
        }
        assert_eq!(deck.len(), 40);
        card_def::preflight_fully_supported_deck(&deck).unwrap();
        for actor in [PlayerId::P0, PlayerId::P1] {
            let owner = NextFiveSessionV59::reset(
                59,
                123,
                (10_000, 10_000),
                [
                    "actual-partial-reference".into(),
                    "actual-partial-reference".into(),
                ],
                [deck.clone(), deck.clone()],
                actor,
            )
            .unwrap();
            assert!(matches!(
                owner.current_response(),
                FastActorResponseV1::Decision(_)
            ));
        }
    }
}

#[test]
fn all_five_genuine_paid_casts_use_sealed_native_order_and_distinct_artifacts_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for name in [
            "Midnight Snack",
            "Extravagant Replication",
            "Infernal Vessel",
            "Giada, Font of Hope",
            "Goblin Negotiation",
        ] {
            let mut raw = ready(actor);
            let target = put(
                &mut raw,
                actor.opponent(),
                "Axgard Cavalry",
                Zone::Battlefield,
            );
            let spell = put(&mut raw, actor, name, Zone::Hand);
            let id = raw.objects.get(spell).card_def;
            assert!(card_def::preflight_fully_supported_deck(&[id]).is_err());
            let mut owner = own(raw);
            check_artifact(&owner);
            let is_goblin = name == "Goblin Negotiation";
            cast(
                &mut owner,
                spell,
                is_goblin.then_some(target),
                is_goblin.then_some(3),
            );
            check_artifact(&owner);
            let snapshot = owner.snapshot_v59();
            let old = owner.decision_v59().unwrap();
            drain(&mut owner);
            let result = gameplay(&owner);
            assert_eq!(
                state(&owner).objects.get(spell).zone,
                if is_goblin {
                    Zone::Graveyard
                } else {
                    Zone::Battlefield
                }
            );
            if is_goblin {
                assert_eq!(
                    owner
                        .decision_v59()
                        .unwrap()
                        .observation()
                        .next_five
                        .goblin_completed[0]
                        .excess,
                    1
                );
            }
            owner.restore_v59(&snapshot).unwrap();
            assert!(
                owner
                    .decision_v59()
                    .unwrap()
                    .expected()
                    .environment_revision
                    > old.expected().environment_revision
            );
            assert!(owner.step_v59(&old, 0).is_err());
            drain(&mut owner);
            assert_eq!(gameplay(&owner), result);
        }
    }
}

#[test]
fn independent_hidden_worlds_exact_wide_roster_and_whole_binding_forgery_refuse_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut first = ready(actor);
        let hidden = put(
            &mut first,
            actor.opponent(),
            "Goblin Negotiation",
            Zone::Hand,
        );
        // These are independent initial fixture operands, before any owned operation.
        let wide = put(&mut first, actor, "Forest", Zone::Battlefield);
        first.objects.get_mut(wide).counters.plus1_plus1 = i32::MAX;
        first.objects.get_mut(wide).counters.soul = i32::MAX;
        first.objects.get_mut(wide).counters.incubation = i32::MAX;
        first.objects.get_mut(wide).damage = u32::MAX;
        let mut second = first.clone();
        second.objects.get_mut(hidden).card_def = card_def::card_id_by_name("Mountain").unwrap();
        second.objects.get_mut(hidden).name = "Mountain".into();
        let mut owner = own(first);
        let other = own(second);
        let current = owner.decision_v59().unwrap();
        assert_eq!(
            current.observation(),
            other.decision_v59().unwrap().observation()
        );
        assert_eq!(current.actions(), other.decision_v59().unwrap().actions());
        let row = current
            .observation()
            .drake
            .battlefield
            .iter()
            .flatten()
            .find(|r| r.reference.incarnation.object == wide.0)
            .unwrap();
        assert_eq!(
            (
                row.counters.plus_one,
                row.counters.soul,
                row.counters.incubation,
                row.marked_damage
            ),
            (i32::MAX, i32::MAX, i32::MAX, u32::MAX)
        );
        let original = gameplay(&owner);
        assert!(owner.restore_v59(&other.snapshot_v59()).is_err());
        assert!(owner.step_v59(&other.decision_v59().unwrap(), 0).is_err());
        for changed in 0..5 {
            let mut forged = current.clone();
            match changed {
                0 => forged.binding.candidates.reverse(),
                1 => forged.actions.reverse(),
                2 => forged.observation.drake.battlefield[actor.index()].clear(),
                3 => forged.observation.next_five.viewer = actor.opponent().into(),
                _ => forged.expected.physical_decision_id += 1,
            }
            assert!(owner.step_v59(&forged, 0).is_err());
            assert_eq!(gameplay(&owner), original);
        }
        let core = owner.inner.current.as_ref().unwrap();
        assert!(
            flat_validate_expected_decision_v1(&owner.inner, core, current.expected()).is_err()
        );
        let snapshot = owner.snapshot_v59();
        let mut forged = snapshot.clone();
        forged
            .inner
            .0
            .current
            .as_mut()
            .unwrap()
            .candidates
            .reverse();
        assert!(owner.restore_v59(&forged).is_err());
        assert_eq!(gameplay(&owner), original);
    }
}

#[test]
fn paid_snack_and_infernal_death_keep_original_owner_controller_and_restore_both_seats() {
    for owner in [PlayerId::P0, PlayerId::P1] {
        let mut raw = ready(owner);
        native::cast(&mut raw, owner, "Midnight Snack", None, None);
        native::settle(&mut raw);
        let vessel = native::cast(&mut raw, owner, "Infernal Vessel", None, None);
        native::settle(&mut raw);
        let source = put(&mut raw, owner.opponent(), "Forest", Zone::Hand);
        // Existing control and death primitives supply this prebatch operand.
        let mut ctx = crate::effect::ExecCtx::no_targets(source, owner.opponent());
        ctx.ability_source_contract =
            Some(crate::state::AbilitySourceContractV4::capture(&raw, source));
        ctx.targets = vec![Target::Object(vessel)];
        ctx.target_contracts = vec![crate::state::StackTargetContractV4::capture(
            &raw,
            Target::Object(vessel),
        )];
        crate::effect::execute(
            &crate::effect::EffectOp::GainControlOfTargetCreatureUntilEndOfTurn { target_index: 0 },
            &ctx,
            &mut raw,
        );
        event::propose_and_commit(
            &mut raw,
            ProposedEvent::zone_change(vessel, Zone::Graveyard),
        );
        let mut session = own(raw);
        let current = session.decision_v59().unwrap();
        let death = &current.observation().next_five.infernal_deaths[0];
        assert_eq!(death.source.captured_controller, owner.opponent().into());
        assert_eq!(death.owner_on_return, owner.into());
        assert!(death.return_incarnation_available_now);
        let saved = session.snapshot_v59();
        drain(&mut session);
        let result = gameplay(&session);
        assert_eq!(state(&session).objects.get(vessel).controller, owner);
        assert_eq!(state(&session).objects.get(vessel).counters.plus1_plus1, 2);
        session.restore_v59(&saved).unwrap();
        drain(&mut session);
        assert_eq!(gameplay(&session), result);
    }
}

#[test]
fn paid_giada_entry_restricted_mana_and_original_source_callback_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut raw = ready(actor);
        let giada = native::cast(&mut raw, actor, "Giada, Font of Hope", None, None);
        native::settle(&mut raw);
        let angel = native::cast(&mut raw, actor, "Youthful Valkyrie", None, None);
        native::settle(&mut raw);
        assert_eq!(raw.objects.get(angel).counters.plus1_plus1, 1);
        let turn = raw.turn_key_v1();
        for _ in 0..250 {
            let d = native::next(&mut raw);
            if raw.turn_key_v1() != turn
                && raw.active_player == actor
                && raw.step == crate::state::Step::Main1
            {
                assert!(matches!(d,Decision::CastSpellOrPass{player,..}if player==actor));
                break;
            }
            native::quiet(&mut raw, d);
        }
        assert!(raw.turn_key_v1() != turn && !raw.objects.get(giada).summoning_sick);
        let mut owner = own(raw);
        let saved = owner.snapshot_v59();
        act(
            &mut owner,
            |a| matches!(a,ActionSemanticV1::ActivateManaAbility{source,..}if source.arena_id==giada.0),
        );
        let facts = owner.decision_v59().unwrap();
        assert_eq!(
            facts.observation().next_five.angel_mana[actor.index()].len(),
            1
        );
        assert!(facts.observation().next_five.angel_mana[actor.index()][0].only_cast_angel_spell);
        let result = gameplay(&owner);
        owner.restore_v59(&saved).unwrap();
        act(
            &mut owner,
            |a| matches!(a,ActionSemanticV1::ActivateManaAbility{source,..}if source.arena_id==giada.0),
        );
        assert_eq!(gameplay(&owner), result);
    }
}

#[test]
fn paid_replication_season_aura_owner_menu_and_each_original_index_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut raw = ready(actor);
        native::cast(&mut raw, actor, "Doubling Season", None, None);
        native::settle(&mut raw);
        let replica = native::cast(&mut raw, actor, "Extravagant Replication", None, None);
        native::settle(&mut raw);
        let elf = native::cast(&mut raw, actor, "Llanowar Elves", None, None);
        native::settle(&mut raw);
        let elf2 = native::cast(&mut raw, actor, "Llanowar Elves", None, None);
        native::settle(&mut raw);
        let aura = native::cast(&mut raw, actor, "Witness Protection", Some(elf), None);
        native::settle(&mut raw);
        let mut reached = false;
        for _ in 0..250 {
            let d = native::next(&mut raw);
            if matches!(&d,Decision::ChooseTargets{spell,..}if *spell==replica) {
                reached = true;
                break;
            }
            native::quiet(&mut raw, d);
        }
        assert!(reached);
        let mut owner = own(raw);
        act(
            &mut owner,
            |a| matches!(a,ActionSemanticV1::ChooseTarget{source,target:TargetRefV1::Object{object},..}if source.arena_id==replica.0&&object.arena_id==aura.0),
        );
        for _ in 0..20 {
            if owner
                .decision_v59()
                .unwrap()
                .observation()
                .next_five
                .aura_copy
                .is_some()
            {
                break;
            }
            act(&mut owner, |a| matches!(a, ActionSemanticV1::Pass { .. }));
        }
        assert!(owner
            .decision_v59()
            .unwrap()
            .observation()
            .next_five
            .aura_copy
            .is_some());
        for chosen in 0..2 {
            let current = owner.decision_v59().unwrap();
            let copy = current.observation().next_five.aura_copy.as_ref().unwrap();
            assert_eq!((copy.expanded_count, copy.chosen_hosts.len()), (2, chosen));
            assert_eq!(copy.chooser, actor.into());
            assert!(copy.untargeted);
            let saved = owner.snapshot_v59();
            act(
                &mut owner,
                |a| matches!(a,ActionSemanticV1::ChooseEffectTarget{target:TargetRefV1::Object{object},..}if object.arena_id==elf2.0),
            );
            let result = gameplay(&owner);
            owner.restore_v59(&saved).unwrap();
            assert!(owner.step_v59(&current, 0).is_err());
            act(
                &mut owner,
                |a| matches!(a,ActionSemanticV1::ChooseEffectTarget{target:TargetRefV1::Object{object},..}if object.arena_id==elf2.0),
            );
            assert_eq!(gameplay(&owner), result);
        }
        drain(&mut owner);
        assert_eq!(
            state(&owner)
                .objects
                .iter()
                .filter(|(_, o)| o.zone == Zone::Battlefield
                    && o.v4.is_token
                    && o.card_def == card_def::card_id_by_name("Witness Protection").unwrap())
                .count(),
            2
        );
    }
}

#[test]
fn genuine_paid_goblin_prowess_combat_hatcher_remove_three_and_token_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut raw = ready(actor);
        let hatcher = native::cast(&mut raw, actor, "Drake Hatcher", None, None);
        native::settle(&mut raw);
        let turn = raw.turn_key_v1();
        for _ in 0..250 {
            let d = native::next(&mut raw);
            if raw.turn_key_v1() != turn
                && raw.active_player == actor
                && raw.step == crate::state::Step::Main1
            {
                break;
            }
            native::quiet(&mut raw, d);
        }
        assert!(raw.turn_key_v1() != turn && !raw.objects.get(hatcher).summoning_sick);
        let printed_power = engine::checked_effective_pt(&raw, hatcher).unwrap().0;
        for _ in 0..2 {
            native::cast(
                &mut raw,
                actor,
                "Goblin Negotiation",
                Some(hatcher),
                Some(0),
            );
            native::settle(&mut raw);
        }
        assert_eq!(
            engine::checked_effective_pt(&raw, hatcher).unwrap().0,
            printed_power + 2
        );
        let mut attacked = false;
        for _ in 0..200 {
            let d = native::next(&mut raw);
            if raw.objects.get(hatcher).counters.incubation >= 3 {
                break;
            }
            if let Decision::DeclareAttackers {
                player,
                ref eligible,
            } = d
            {
                if player == actor && !attacked {
                    assert!(eligible.contains(&hatcher));
                    engine::step(&mut raw, Action::DeclareAttackers(vec![hatcher])).unwrap();
                    attacked = true;
                    continue;
                }
            }
            native::quiet(&mut raw, d);
        }
        assert!(attacked && raw.objects.get(hatcher).counters.incubation >= 3);
        let mut owner = own(raw);
        for _ in 0..8 {
            if owner.decision_v59().unwrap().actions().iter().any(|a|matches!(a,ActionSemanticV1::ActivateAbility{source,ability_index:0,..}if source.arena_id==hatcher.0)){break;}
            act(&mut owner, |a| matches!(a, ActionSemanticV1::Pass { .. }));
        }
        let current = owner.decision_v59().unwrap();
        let index=current.actions().iter().position(|a|matches!(a,ActionSemanticV1::ActivateAbility{source,ability_index:0,..}if source.arena_id==hatcher.0)).unwrap();
        let annotation = current.observation().action_meanings[index]
            .as_ref()
            .unwrap();
        assert_eq!(annotation.count, 3);
        assert_eq!(annotation.after, annotation.before - 3);
        assert!(!annotation.countering_refunds_costs);
        let before = state(&owner).objects.get(hatcher).counters.incubation;
        let saved = owner.snapshot_v59();
        act(
            &mut owner,
            |a| matches!(a,ActionSemanticV1::ActivateAbility{source,ability_index:0,..}if source.arena_id==hatcher.0),
        );
        assert_eq!(
            state(&owner).objects.get(hatcher).counters.incubation,
            before - 3
        );
        drain(&mut owner);
        let result = gameplay(&owner);
        let token = card_def::card_id_by_name("Drake Token").unwrap();
        assert_eq!(
            state(&owner)
                .objects
                .iter()
                .filter(|(_, o)| o.zone == Zone::Battlefield
                    && o.card_def == token
                    && o.owner == actor)
                .count(),
            1
        );
        owner.restore_v59(&saved).unwrap();
        act(
            &mut owner,
            |a| matches!(a,ActionSemanticV1::ActivateAbility{source,ability_index:0,..}if source.arena_id==hatcher.0),
        );
        drain(&mut owner);
        assert_eq!(gameplay(&owner), result);
    }
}

#[test]
fn paid_goblin_affected_actor_ordering_native_indices_and_full_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut raw = ready(actor);
        let tyrant = native::cast(&mut raw, actor, "Twinflame Tyrant", None, None);
        native::settle(&mut raw);
        let target = put(
            &mut raw,
            actor.opponent(),
            "Axgard Cavalry",
            Zone::Battlefield,
        );
        // Existing numeric shield primitive, without a paid issuer claim.
        let id = raw.engine.next_replacement_id;
        raw.engine.next_replacement_id += 1;
        raw.engine
            .active_replacements
            .push(event::ActiveReplacement {
                id,
                source: tyrant,
                kind: event::ReplacementEffectKind::PreventNextDamage {
                    target: Target::Object(target),
                    remaining: 1,
                },
            });
        let spell = native::cast(&mut raw, actor, "Goblin Negotiation", Some(target), Some(3));
        let mut reached = false;
        for _ in 0..20 {
            let d = native::next(&mut raw);
            if matches!(d, Decision::ChooseEffectOption { .. }) {
                reached = true;
                break;
            }
            native::quiet(&mut raw, d);
        }
        assert!(reached);
        let mut owner = own(raw);
        let saved = owner.snapshot_v59();
        let original = owner.decision_v59().unwrap();
        assert_eq!(original.expected().acting_player, actor.opponent().into());
        let stage = original
            .observation()
            .next_five
            .goblin_ordering
            .as_ref()
            .unwrap();
        assert_eq!(stage.controller, actor.into());
        assert_eq!(stage.chooser, actor.opponent().into());
        assert_eq!(stage.source.arena_id, spell.0);
        let slot = original
            .observation()
            .next_five
            .goblin_ordering_options
            .iter()
            .position(|o| o.kind == "shield")
            .unwrap() as u16;
        act(
            &mut owner,
            |a| matches!(a,ActionSemanticV1::ChooseEffectOption{option_index,..}if *option_index==slot),
        );
        if owner
            .decision_v59()
            .unwrap()
            .observation()
            .next_five
            .goblin_ordering
            .is_some()
        {
            act(&mut owner, |a| {
                matches!(
                    a,
                    ActionSemanticV1::ChooseEffectOption {
                        option_index: 0,
                        ..
                    }
                )
            });
        }
        drain(&mut owner);
        let result = gameplay(&owner);
        let complete = owner.decision_v59().unwrap();
        assert_eq!(
            (
                complete.observation().next_five.goblin_completed[0].actual_damage,
                complete.observation().next_five.goblin_completed[0].tokens_created
            ),
            (4, 2)
        );
        owner.restore_v59(&saved).unwrap();
        assert!(owner.step_v59(&original, 0).is_err());
        act(
            &mut owner,
            |a| matches!(a,ActionSemanticV1::ChooseEffectOption{option_index,..}if *option_index==slot),
        );
        if owner
            .decision_v59()
            .unwrap()
            .observation()
            .next_five
            .goblin_ordering
            .is_some()
        {
            act(&mut owner, |a| {
                matches!(
                    a,
                    ActionSemanticV1::ChooseEffectOption {
                        option_index: 0,
                        ..
                    }
                )
            });
        }
        drain(&mut owner);
        assert_eq!(gameplay(&owner), result);
    }
}

#[test]
fn paid_graveyard_return_sources_refuse_older_session_step_and_terminal_restore_readonly_both_seats(
) {
    use crate::graveyard_return_public_v61::tests as graveyard;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut paid = graveyard::ready(actor);
        let alesha = graveyard::paid(&mut paid, actor, "Alesha, Who Laughs at Fate", None);
        graveyard::paid(&mut paid, actor, "Snap", Some(alesha));
        assert_eq!(paid.objects.get(alesha).zone, Zone::Hand);
        let original = paid.clone();
        let refused = FastActorSessionV1::from_next_five_v59_fixture_state(paid);
        assert!(matches!(
            refused.current_response(),
            FastActorResponseV1::Terminal(_)
        ));
        assert_eq!(refused.state, original);
        let snapshot = refused.snapshot_v1();
        let mut old = FastActorSessionV1::from_next_five_v59_fixture_state(graveyard::ready(actor));
        let before = (
            old.state.clone(),
            old.current_response(),
            old.environment_revision,
        );
        assert!(old.next_five_restore_v59(&snapshot).is_err());
        assert_eq!(
            (
                old.state.clone(),
                old.current_response(),
                old.environment_revision
            ),
            before
        );
        old.restore_v1(&snapshot);
        assert_eq!(
            (
                old.state.clone(),
                old.current_response(),
                old.environment_revision
            ),
            before
        );
        // A genuine foreign mechanic arriving after a published old prompt must
        // refuse before stale bindings or the surface can mutate that world.
        old.state = original;
        let before = (
            old.state.clone(),
            old.current_response(),
            old.environment_revision,
        );
        assert!(old.step(old.episode_id, old.policy_step_count, 0).is_err());
        assert_eq!(
            (
                old.state.clone(),
                old.current_response(),
                old.environment_revision
            ),
            before
        );
    }
}
