use super::*;
use crate::{
    card_def::CARD_DEFS,
    engine::{self, Action, Decision},
    event::{self, ProposedEvent},
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::Step,
};
pub(crate) fn ready(actor: PlayerId) -> GameState {
    let mut s = crate::extra_turn_v1::tests::ready(actor);
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut s).unwrap();
    for p in [actor, actor.opponent()] {
        for name in ["Plains", "Island", "Swamp", "Mountain", "Forest"] {
            for _ in 0..8 {
                put(&mut s, p, name, Zone::Battlefield);
            }
        }
    }
    assert!(
        matches!(engine::advance_until_decision(&mut s),Decision::CastSpellOrPass{player,..}if player==actor)
    );
    s
}
pub(crate) fn next(s: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(s);
    assert!(s.engine.halted.is_none(), "{d:?}");
    d
}
pub(crate) fn quiet(s: &mut GameState, d: Decision) {
    let a = match d {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
        Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
        Decision::ChooseEntryColorV36 { .. } => Action::ChooseEffectOption(0),
        other => panic!("unexpected native fixture prompt {other:?}"),
    };
    engine::step(s, a).unwrap();
}
pub(crate) fn settle(s: &mut GameState) {
    for _ in 0..150 {
        let d = next(s);
        if s.stack.is_empty()
            && s.engine.pending_cast.is_none()
            && s.engine.pending_activation.is_none()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_triggers.is_empty()
        {
            assert!(matches!(d, Decision::CastSpellOrPass { .. }));
            return;
        }
        quiet(s, d);
    }
    panic!("native resolution did not settle");
}
pub(crate) fn cast(
    s: &mut GameState,
    actor: PlayerId,
    name: &str,
    target: Option<ObjectId>,
    x: Option<u16>,
) -> ObjectId {
    let source = put(s, actor, name, Zone::Hand);
    assert!(
        matches!(next(s),Decision::CastSpellOrPass{player,castable_spells,..}if player==actor&&castable_spells.contains(&source))
    );
    let tapped = s.players[actor.index()]
        .battlefield
        .iter()
        .filter(|id| s.objects.get(**id).tapped)
        .count();
    engine::step(s, Action::CastSpell(source)).unwrap();
    if let Some(target) = target {
        assert!(
            matches!(next(s),Decision::ChooseTargets{legal_targets,..}if legal_targets.contains(&Target::Object(target)))
        );
        engine::step(s, Action::ChooseTarget(Target::Object(target))).unwrap();
    }
    if let Some(x) = x {
        assert!(matches!(next(s),Decision::ChooseEffectOption{option_count,..}if option_count>x));
        engine::step(s, Action::ChooseEffectOption(x)).unwrap();
    }
    assert!(matches!(next(s),Decision::CastSpellOrPass{player,..}if player==actor));
    assert!(s.engine.pending_cast.is_none());
    assert_eq!(s.objects.get(source).zone, Zone::Stack);
    let cost = &CARD_DEFS[usize::from(s.objects.get(source).card_def)].cost;
    let after = s.players[actor.index()]
        .battlefield
        .iter()
        .filter(|id| s.objects.get(**id).tapped)
        .count();
    assert_eq!(
        after - tapped,
        usize::from(cost.generic) + cost.pips.len() + usize::from(x.unwrap_or(0))
    );
    source
}
fn roundtrip(s: &GameState, viewer: PlayerId) -> FactsV59 {
    let before = serde_json::to_vec(s).unwrap();
    let f = capture(s, viewer).unwrap();
    validate(s, viewer, &f).unwrap();
    let json = serde_json::to_value(&f).unwrap();
    let back: FactsV59 = serde_json::from_value(json.clone()).unwrap();
    validate(s, viewer, &back).unwrap();
    let restored: GameState = serde_json::from_slice(&before).unwrap();
    assert_eq!(capture(&restored, viewer).unwrap(), f);
    let mut extra = json;
    extra
        .as_object_mut()
        .unwrap()
        .insert("private_world".into(), serde_json::json!({}));
    assert!(serde_json::from_value::<FactsV59>(extra).is_err());
    let mut forged = f.clone();
    forged.viewer = viewer.opponent().into();
    assert!(validate(s, viewer, &forged).is_err());
    let text = serde_json::to_string(&f).unwrap();
    for forbidden in [
        "event_history",
        "event_log",
        "inline_effect",
        "resolving_item",
        "source_contract",
        "history_index",
        "paid_cost_refs",
        "original_world",
        "remaining_frames",
    ] {
        assert!(!text.contains(forbidden), "{forbidden}");
    }
    assert_eq!(serde_json::to_vec(s).unwrap(), before);
    f
}
#[test]
fn genuine_goblin_pending_target_and_x_are_staged_before_finalized_quote() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let recipient = put(
            &mut s,
            actor.opponent(),
            "Axgard Cavalry",
            Zone::Battlefield,
        );
        let spell = put(&mut s, actor, "Goblin Negotiation", Zone::Hand);
        assert!(
            matches!(next(&mut s),Decision::CastSpellOrPass{player,castable_spells,..}if player==actor&&castable_spells.contains(&spell))
        );
        engine::step(&mut s, Action::CastSpell(spell)).unwrap();
        assert!(
            matches!(next(&mut s),Decision::ChooseTargets{legal_targets,..}if legal_targets.contains(&Target::Object(recipient)))
        );
        let f = roundtrip(&s, actor);
        let cast = f.casts.iter().find(|c| c.source.object == spell.0).unwrap();
        assert!(cast.payment_pending && cast.targets.is_empty() && cast.x.is_none());
        assert!(f.goblin_quotes.is_empty());
        engine::step(&mut s, Action::ChooseTarget(Target::Object(recipient))).unwrap();
        assert!(
            matches!(next(&mut s),Decision::ChooseEffectOption{option_count,..}if option_count>3)
        );
        let f = roundtrip(&s, actor);
        let cast = f.casts.iter().find(|c| c.source.object == spell.0).unwrap();
        assert!(cast.payment_pending && cast.x.is_none());
        assert!(matches!(cast.targets.as_slice(),[TargetV59::Object(r)]if r.object==recipient.0));
        assert!(f.goblin_quotes.is_empty());
        engine::step(&mut s, Action::ChooseEffectOption(3)).unwrap();
        let f = roundtrip(&s, actor);
        assert_eq!(
            f.casts
                .iter()
                .find(|c| c.source.object == spell.0)
                .unwrap()
                .x,
            Some(3)
        );
        assert!(f.goblin_quotes.is_empty());
        assert!(matches!(next(&mut s),Decision::CastSpellOrPass{player,..}if player==actor));
        let f = roundtrip(&s, actor);
        let cast = f.casts.iter().find(|c| c.source.object == spell.0).unwrap();
        assert!(!cast.payment_pending);
        assert_eq!(cast.x, Some(3));
        assert_eq!(f.goblin_quotes[0].original_x, 3);
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        settle(&mut s);
        settle(&mut replay);
        assert_eq!(
            serde_json::to_vec(&s).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        assert_eq!(roundtrip(&s, actor).goblin_completed[0].excess, 1);
    }
}
#[test]
fn all_five_exact_meanings_keep_order_costs_and_partial_truth_both_seats() {
    for family in FamilyV59::ALL {
        let m = meaning::printed_meaning(family).unwrap();
        meaning::validate(&m).unwrap();
        assert!(!m.full_engine_admitted && !m.model_admitted && !m.sampler_admitted);
        assert!(card_def::preflight_fully_supported_deck(&[m.card_id]).is_err());
        let d = &CARD_DEFS[usize::from(m.card_id)];
        let mut foreign = crate::midnight_snack_v1::tests::definition_fixture(d);
        foreign.ward_cost = Some(crate::card_def::WardCostDef::Generic(1));
        assert!(!meaning::owns_definition(family, m.card_id, &foreign));
        let mut wrong = m.clone();
        wrong.ordered_rules.reverse();
        if wrong != m {
            assert!(meaning::validate(&wrong).is_err());
        }
        wrong.printed.mana.generic = wrong.printed.mana.generic.saturating_add(1);
        assert!(meaning::validate(&wrong).is_err());
    }
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let target = put(
            &mut s,
            actor.opponent(),
            "Axgard Cavalry",
            Zone::Battlefield,
        );
        let spell = cast(&mut s, actor, "Goblin Negotiation", Some(target), Some(5));
        let f = roundtrip(&s, actor);
        let q = &f.goblin_quotes[0];
        assert_eq!(q.original_x, 5);
        assert_eq!(q.creature_lethal_damage_now, Some(2));
        assert_eq!(q.source.object, spell.0);
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        settle(&mut s);
        settle(&mut restored);
        assert_eq!(
            serde_json::to_vec(&s).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
        let f = roundtrip(&s, actor);
        assert!(!f.casts.iter().any(|c| c.source.object == spell.0));
        assert_eq!(f.goblin_completed.len(), 1);
        let complete = &f.goblin_completed[0];
        assert_eq!(
            (
                complete.original_x,
                complete.actual_damage,
                complete.excess,
                complete.tokens_created
            ),
            (5, 5, 3, 3)
        );
        assert_eq!(complete.source.arena_id, spell.0);
        assert_eq!(complete.target.arena_id, target.0);
        let mut forged = f.clone();
        forged.goblin_completed[0].tokens_created += 1;
        assert!(validate(&s, actor, &forged).is_err());
        assert_eq!(
            s.players[actor.index()]
                .battlefield
                .iter()
                .filter(
                    |id| CARD_DEFS[usize::from(s.objects.get(**id).card_def)].name
                        == "Goblin Token"
                )
                .count(),
            3
        );
    }
}
#[test]
fn genuine_paid_goblin_ordering_projects_exact_options_and_completed_excess() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (first, damage, excess) in [("shield", 4, 2), ("multiplier", 5, 3)] {
            let mut s = ready(actor);
            let tyrant = cast(&mut s, actor, "Twinflame Tyrant", None, None);
            settle(&mut s);
            let recipient = put(
                &mut s,
                actor.opponent(),
                "Axgard Cavalry",
                Zone::Battlefield,
            );
            // The existing numeric prevention operand has no paid issuer in this catalog.
            let id = s.engine.next_replacement_id;
            s.engine.next_replacement_id += 1;
            s.engine.active_replacements.push(event::ActiveReplacement {
                id,
                source: tyrant,
                kind: event::ReplacementEffectKind::PreventNextDamage {
                    target: Target::Object(recipient),
                    remaining: 1,
                },
            });
            let source = cast(
                &mut s,
                actor,
                "Goblin Negotiation",
                Some(recipient),
                Some(3),
            );
            let mut replay: GameState =
                serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
            let mut first = Some(first);
            let mut seen = false;
            for _ in 0..30 {
                let d = next(&mut s);
                assert_eq!(next(&mut replay), d);
                if let Decision::ChooseEffectOption {
                    player,
                    option_count,
                    ..
                } = d
                {
                    let f = roundtrip(&s, actor);
                    let stage = f.goblin_ordering.as_ref().unwrap();
                    seen = true;
                    assert_eq!(stage.controller, actor.into());
                    assert_eq!(stage.chooser, actor.opponent().into());
                    assert_eq!(player, actor.opponent());
                    assert_eq!(stage.source.arena_id, source.0);
                    assert_eq!(stage.target.arena_id, recipient.0);
                    assert_eq!(stage.original_x, 3);
                    assert_eq!(stage.lethal_damage, 2);
                    assert_eq!(f.goblin_ordering_options.len(), usize::from(option_count));
                    let selected = first.take().map_or(0, |kind| {
                        f.goblin_ordering_options
                            .iter()
                            .position(|o| o.kind == kind)
                            .unwrap()
                    });
                    let mut forged = f.clone();
                    forged.goblin_ordering_options.reverse();
                    if forged != f {
                        assert!(validate(&s, actor, &forged).is_err());
                    }
                    for branch in [&mut s, &mut replay] {
                        engine::step(
                            branch,
                            Action::ChooseEffectOption(u16::try_from(selected).unwrap()),
                        )
                        .unwrap();
                    }
                } else if s.stack.is_empty()
                    && s.engine.pending_effect.is_none()
                    && s.engine.pending_triggers.is_empty()
                {
                    assert!(matches!(d, Decision::CastSpellOrPass { .. }));
                    break;
                } else {
                    quiet(&mut s, d.clone());
                    quiet(&mut replay, d);
                }
                assert_eq!(
                    serde_json::to_vec(&s).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
            }
            assert!(seen);
            assert!(s.stack.is_empty() && s.engine.pending_effect.is_none());
            assert_eq!(
                serde_json::to_vec(&s).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
            let f = roundtrip(&s, actor);
            assert!(f.goblin_ordering.is_none() && f.goblin_ordering_options.is_empty());
            assert_eq!(
                (
                    f.goblin_completed[0].actual_damage,
                    f.goblin_completed[0].excess,
                    f.goblin_completed[0].tokens_created
                ),
                (damage, excess, u64::from(excess))
            );
        }
    }
}
#[test]
fn actual_paid_snack_and_infernal_death_owner_return_have_safe_original_refs() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let snack = cast(&mut s, actor, "Midnight Snack", None, None);
        settle(&mut s);
        let vessel = cast(&mut s, actor, "Infernal Vessel", None, None);
        settle(&mut s);
        let generation = s.objects.get(vessel).zone_change_count;
        let f = roundtrip(&s, actor);
        assert!(f.snack.current.iter().any(|x| x.source.object == snack.0));
        // A declared primitive death, not a claim that an uncast removal spell paid.
        event::propose_and_commit(&mut s, ProposedEvent::zone_change(vessel, Zone::Graveyard));
        next(&mut s);
        let f = roundtrip(&s, actor);
        assert_eq!(f.infernal_deaths.len(), 1);
        let death = &f.infernal_deaths[0];
        assert_eq!(death.source.original_source.generation, generation);
        assert_eq!(death.owner_on_return, actor.into());
        assert!(
            death.return_incarnation_available_now
                && death.captured_not_demon
                && death.collection_completed
        );
        let mut changed = f.clone();
        changed.infernal_deaths[0].owner_on_return = actor.opponent().into();
        assert!(validate(&s, actor, &changed).is_err());
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        settle(&mut s);
        settle(&mut restored);
        assert_eq!(
            serde_json::to_vec(&s).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
        assert_eq!(s.objects.get(vessel).zone, Zone::Battlefield);
        assert_eq!(s.objects.get(vessel).counters.plus1_plus1, 2);
        assert!(engine::has_effective_subtype(&s, vessel, Subtype::Demon));
        let f = roundtrip(&s, actor);
        assert!(!f.infernal_deaths[0].return_incarnation_available_now);
    }
}
#[test]
fn paid_infernal_stolen_predeath_controller_does_not_replace_return_owner() {
    for owner in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(owner);
        let vessel = cast(&mut s, owner, "Infernal Vessel", None, None);
        settle(&mut s);
        // An existing control primitive is the declared setup operand, not a paid spell.
        let control_source = put(&mut s, owner.opponent(), "Forest", Zone::Hand);
        let mut ctx = effect::ExecCtx::no_targets(control_source, owner.opponent());
        ctx.ability_source_contract = Some(crate::state::AbilitySourceContractV4::capture(
            &s,
            control_source,
        ));
        ctx.targets = vec![Target::Object(vessel)];
        ctx.target_contracts = vec![StackTargetContractV4::capture(&s, Target::Object(vessel))];
        effect::execute(
            &effect::EffectOp::GainControlOfTargetCreatureUntilEndOfTurn { target_index: 0 },
            &ctx,
            &mut s,
        );
        assert!(s.engine.halted.is_none());
        assert_eq!(s.objects.get(vessel).controller, owner.opponent());
        event::propose_and_commit(&mut s, ProposedEvent::zone_change(vessel, Zone::Graveyard));
        next(&mut s);
        let f = roundtrip(&s, owner);
        let d = &f.infernal_deaths[0];
        assert_eq!(d.source.captured_controller, owner.opponent().into());
        assert_eq!(d.source.original_source.owner, owner.into());
        assert_eq!(d.owner_on_return, owner.into());
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        settle(&mut s);
        settle(&mut restored);
        assert_eq!(
            serde_json::to_vec(&s).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
        assert_eq!(s.objects.get(vessel).controller, owner);
        assert_eq!(s.objects.get(vessel).counters.plus1_plus1, 2);
        assert!(engine::has_effective_subtype(&s, vessel, Subtype::Demon));
    }
}
#[test]
fn paid_giada_actual_angel_entry_and_floating_restriction_survive_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let giada = cast(&mut s, actor, "Giada, Font of Hope", None, None);
        settle(&mut s);
        let angel = cast(&mut s, actor, "Youthful Valkyrie", None, None);
        settle(&mut s);
        assert_eq!(s.objects.get(angel).counters.plus1_plus1, 1);
        let f = roundtrip(&s, actor);
        assert_eq!(f.angel_replacements[0].source.object, giada.0);
        assert_eq!(f.angel_replacements[0].original_angel_count_now, 2);
        let turn = s.turn_key_v1();
        for _ in 0..250 {
            let d = next(&mut s);
            if s.turn_key_v1() != turn && s.active_player == actor && s.step == Step::Main1 {
                assert!(matches!(d,Decision::CastSpellOrPass{player,..}if player==actor));
                break;
            }
            quiet(&mut s, d);
        }
        assert!(s.turn_key_v1() != turn && s.active_player == actor && s.step == Step::Main1);
        assert!(!s.objects.get(giada).summoning_sick);
        assert!(
            matches!(next(&mut s),Decision::CastSpellOrPass{mana_abilities,..}if mana_abilities.contains(&giada))
        );
        engine::step(&mut s, Action::ActivateManaAbility(giada)).unwrap();
        let f = roundtrip(&s, actor);
        assert_eq!(f.angel_mana[actor.index()].len(), 1);
        assert!(f.angel_mana[actor.index()][0].only_cast_angel_spell);
        assert_eq!(
            f.angel_mana[actor.index()][0]
                .current_original_source
                .as_ref()
                .unwrap()
                .object,
            giada.0
        );
        let mut wrong = f.clone();
        wrong.angel_mana[actor.index()][0].only_cast_angel_spell = false;
        assert!(validate(&s, actor, &wrong).is_err());
    }
}
#[test]
fn unknown_private_printings_and_face_down_family_identity_do_not_change_facts() {
    for viewer in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(viewer);
        let spell = cast(&mut s, viewer, "Midnight Snack", None, None);
        settle(&mut s);
        assert_eq!(s.objects.get(spell).zone, Zone::Battlefield);
        let hidden = put(&mut s, viewer.opponent(), "Goblin Negotiation", Zone::Hand);
        let original = roundtrip(&s, viewer);
        let mut changed = s.clone();
        changed.objects.get_mut(hidden).card_def = card_def::card_id_by_name("Mountain").unwrap();
        assert_eq!(capture(&changed, viewer).unwrap(), original);
        // Public concealed form fixture. Underlying printings never select a V59 fact.
        let hidden = put(
            &mut s,
            viewer.opponent(),
            "Infernal Vessel",
            Zone::Battlefield,
        );
        s.objects.get_mut(hidden).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: true,
            lookers: 0,
            hidden_by: None,
            hidden_by_source: None,
        });
        let original = roundtrip(&s, viewer);
        let mut changed = s.clone();
        changed.objects.get_mut(hidden).card_def =
            card_def::card_id_by_name("Llanowar Elves").unwrap();
        assert_eq!(capture(&changed, viewer).unwrap(), original);
    }
}
#[test]
fn actual_paid_replication_target_and_multi_host_aura_progress_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        cast(&mut s, actor, "Doubling Season", None, None);
        settle(&mut s);
        let replica = cast(&mut s, actor, "Extravagant Replication", None, None);
        settle(&mut s);
        let elf = cast(&mut s, actor, "Llanowar Elves", None, None);
        settle(&mut s);
        let elf2 = cast(&mut s, actor, "Llanowar Elves", None, None);
        settle(&mut s);
        let aura = cast(&mut s, actor, "Witness Protection", Some(elf), None);
        settle(&mut s);
        for _ in 0..250 {
            let d = next(&mut s);
            if let Decision::ChooseTargets {
                spell: source,
                legal_targets,
                ..
            } = &d
            {
                if *source == replica {
                    assert!(legal_targets.contains(&Target::Object(aura)));
                    engine::step(&mut s, Action::ChooseTarget(Target::Object(aura))).unwrap();
                    next(&mut s);
                    break;
                }
            }
            quiet(&mut s, d);
        }
        let f = roundtrip(&s, actor);
        assert!(f
            .triggers
            .iter()
            .any(|t| t.family == FamilyV59::Replication && t.stage == TriggerStageV59::Published));
        let mut host_answers = 0;
        for _ in 0..30 {
            let d = next(&mut s);
            if matches!(d, Decision::ChooseEffectTargets { .. }) {
                let f = roundtrip(&s, actor);
                let c = f.aura_copy.as_ref().unwrap();
                assert_eq!(c.source.original_source.object, replica.0);
                assert_eq!(c.expanded_count, 2);
                assert_eq!(c.chosen_hosts.len(), host_answers);
                assert!(c.candidate_hosts.iter().any(|r| r.object == elf2.0));
                assert!(c.untargeted);
                assert_eq!(c.remaining_count, 2 - host_answers as u64);
                let mut wrong = f.clone();
                wrong.aura_copy.as_mut().unwrap().expanded_count += 1;
                assert!(validate(&s, actor, &wrong).is_err());
                engine::step(&mut s, Action::ChooseEffectTarget(Target::Object(elf2))).unwrap();
                host_answers += 1;
                let accepted = roundtrip(&s, actor);
                assert_eq!(
                    accepted
                        .aura_copy
                        .as_ref()
                        .unwrap()
                        .accepted_host_awaiting_advance
                        .as_ref()
                        .unwrap()
                        .object,
                    elf2.0
                );
                if host_answers == 2 {
                    let mut restored: GameState =
                        serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
                    settle(&mut s);
                    settle(&mut restored);
                    assert_eq!(
                        serde_json::to_vec(&s).unwrap(),
                        serde_json::to_vec(&restored).unwrap()
                    );
                    assert!(capture(&s, actor).unwrap().aura_copy.is_none());
                    assert_eq!(
                        s.players[actor.index()]
                            .battlefield
                            .iter()
                            .filter(|id| s.objects.get(**id).v4.is_token
                                && s.objects.get(**id).card_def
                                    == card_def::card_id_by_name("Witness Protection").unwrap())
                            .count(),
                        2
                    );
                    break;
                }
                continue;
            }
            quiet(&mut s, d);
        }
        assert_eq!(host_answers, 2);
    }
}
