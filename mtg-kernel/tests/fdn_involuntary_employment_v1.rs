//! Printed acceptance fixtures require true registration and remain unexecuted.
//! No definition injection or root-validation bypass is used.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, CardType, Keywords, TargetSpec, CARD_DEFS};
use mtg_kernel::effect::{self, EffectOp, ExecCtx, ObjectRef, PlayerRef};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{
    GameObject, GameState, ObjectStateV4, StackTargetContractV4, Step, Target, Zone,
};
use mtg_kernel::surface_v2::{
    HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode, SurfaceAction, SurfaceDecision,
};

fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name)
        .unwrap_or_else(|| panic!("printed acceptance requires registration: {name}"));
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner,
        controller: owner,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[owner.index()].hand.push(id),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
        _ => panic!("unsupported fixture zone"),
    }
    id
}
fn ready(actor: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        11005,
        actor,
    );
    state.step = Step::Main1;
    for player in [PlayerId::P0, PlayerId::P1] {
        put(&mut state, player, "Mountain", Zone::Battlefield);
        put(&mut state, player, "Lightning Bolt", Zone::Hand);
        state.players[player.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    }
    state
}
fn surface() -> HarnessSurfaceV2 {
    HarnessSurfaceV2::new_with_priority_mode_v1(
        PriorityModeV1::EngineWindowsV1,
        SuppressionAuditMode::Full,
    )
}
fn next(surface: &mut HarnessSurfaceV2, state: &mut GameState) -> Decision {
    match surface.next_decision(state) {
        SurfaceDecision::Decision(decision) if !matches!(decision, Decision::Halted { .. }) => {
            decision
        }
        other => panic!("unexpected surface result {other:?}"),
    }
}
fn apply(surface: &mut HarnessSurfaceV2, state: &mut GameState, action: Action) {
    surface.apply(state, SurfaceAction::Action(action)).unwrap();
}
fn replay(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let restored: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(bytes, serde_json::to_vec(&restored).unwrap());
    assert_eq!(state.state_hash(), restored.state_hash());
    restored
}
fn announce(
    surface: &mut HarnessSurfaceV2,
    state: &mut GameState,
    actor: PlayerId,
    target: ObjectId,
) -> ObjectId {
    let spell = put(state, actor, "Involuntary Employment", Zone::Hand);
    announce_existing(surface, state, actor, target, spell)
}
fn announce_existing(
    surface: &mut HarnessSurfaceV2,
    state: &mut GameState,
    actor: PlayerId,
    target: ObjectId,
    spell: ObjectId,
) -> ObjectId {
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 2;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
    assert!(
        matches!(next(surface, state), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&spell))
    );
    apply(surface, state, Action::CastSpell(spell));
    let decision = next(surface, state);
    assert!(
        matches!(decision, Decision::ChooseTargets { player, remaining: 1, can_finish: false, ref legal_targets, .. }
        if player == actor && legal_targets.contains(&Target::Object(target)))
    );
    assert!(engine::step(state, Action::FinishEffectSelection).is_err());
    apply(surface, state, Action::ChooseTarget(Target::Object(target)));
    // Selecting a target changes pending_cast; this next call actually finalizes.
    assert!(matches!(
        next(surface, state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(state.stack.iter().any(|item| item.source == spell));
    spell
}
fn resolve(surface: &mut HarnessSurfaceV2, state: &mut GameState) {
    for _ in 0..20 {
        let decision = next(surface, state);
        if state.stack.is_empty() && state.engine.pending_triggers.is_empty() {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => apply(surface, state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => apply(
                surface,
                state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            other => panic!("unexpected resolving decision {other:?}"),
        }
    }
    panic!("spell failed to resolve");
}
fn cleanup(surface: &mut HarnessSurfaceV2, state: &mut GameState) {
    state.step = Step::End;
    let ending_player = state.active_player;
    for _ in 0..40 {
        let decision = next(surface, state);
        if state.active_player != ending_player {
            assert_eq!(state.active_player, ending_player.opponent());
            assert!(state.engine.until_end_of_turn.is_empty());
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => apply(surface, state, Action::Pass),
            other => panic!("unexpected cleanup decision {other:?}"),
        }
    }
    panic!("ending player failed to finish cleanup");
}
fn treasures(state: &GameState, controller: PlayerId) -> usize {
    state.players[controller.index()]
        .battlefield
        .iter()
        .filter(|id| state.objects.get(**id).name == "Treasure Token")
        .count()
}
fn grant(
    state: &mut GameState,
    source: ObjectId,
    controller: PlayerId,
    target: ObjectId,
    keyword: Keywords,
) {
    let mut ctx = ExecCtx::no_targets(source, controller);
    ctx.targets.push(Target::Object(target));
    ctx.target_contracts.push(StackTargetContractV4::capture(
        state,
        Target::Object(target),
    ));
    effect::execute(
        &EffectOp::GrantKeywordTargetUntilEndOfTurn {
            object: ObjectRef::Target(0),
            keyword,
        },
        &ctx,
        state,
    );
}
#[test]
fn employment_definition_matches_exact_pinned_printed_recipe() {
    let def = &CARD_DEFS[card_id_by_name("Involuntary Employment").unwrap() as usize];
    assert_eq!(def.types, &[CardType::Sorcery]);
    assert_eq!(def.cost.generic, 3);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(def.mana_value, 4);
    assert_eq!(def.target_spec, TargetSpec::Creature);
    assert_eq!(
        (def.spell_effect)(),
        Some(EffectOp::Sequence(vec![
            EffectOp::GainControlOfTargetCreatureUntilEndOfTurn { target_index: 0 },
            EffectOp::UntapObject {
                object: ObjectRef::Target(0)
            },
            EffectOp::GrantKeywordTargetUntilEndOfTurn {
                object: ObjectRef::Target(0),
                keyword: Keywords::HASTE
            },
            EffectOp::CreateToken {
                token_def: card_id_by_name("Treasure Token").unwrap(),
                controller: PlayerRef::Controller
            },
        ]))
    );
}
#[test]
fn actual_employment_casts_both_seats_own_or_opposing_creature_restore_and_cleanup() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for owner in [actor, actor.opponent()] {
            let mut state = ready(actor);
            let target = put(&mut state, owner, "Llanowar Elves", Zone::Battlefield);
            state.objects.get_mut(target).tapped = true;
            let incarnation = state.objects.get(target).zone_change_count;
            let mut surface = surface();
            let spell = announce(&mut surface, &mut state, actor, target);
            let mut restored = replay(&state);
            resolve(&mut surface, &mut state);
            resolve(&mut self::surface(), &mut restored);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
            assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
            assert_eq!(state.objects.get(target).controller, actor);
            assert_eq!(state.objects.get(target).owner, owner);
            assert_eq!(state.objects.get(target).zone_change_count, incarnation);
            assert!(!state.objects.get(target).tapped);
            assert!(engine::has_effective_keyword(
                &state,
                target,
                Keywords::HASTE
            ));
            assert_eq!(treasures(&state, actor), 1);
            assert_eq!(treasures(&state, actor.opponent()), 0);
            let mut restored = replay(&state);
            cleanup(&mut surface, &mut state);
            cleanup(&mut self::surface(), &mut restored);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
            assert_eq!(state.objects.get(target).controller, owner);
            assert!(state.players[owner.index()].battlefield.contains(&target));
            assert!(!engine::has_effective_keyword(
                &state,
                target,
                Keywords::HASTE
            ));
            assert_eq!(treasures(&state, actor), 1);
        }
    }
}
#[test]
fn stolen_summoning_sick_creature_can_actually_attack_with_granted_haste() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let mut surface = surface();
        announce(&mut surface, &mut state, actor, target);
        resolve(&mut surface, &mut state);
        assert!(state.objects.get(target).summoning_sick);
        state.step = Step::DeclareAttackers;
        assert!(matches!(
            next(&mut surface, &mut state),
            Decision::DeclareAttackers { .. }
        ));
        assert!(matches!(
            next(&mut surface, &mut state),
            Decision::DeclareAttackers { .. }
        ));
        apply(
            &mut surface,
            &mut state,
            Action::DeclareAttackers(vec![target]),
        );
        assert!(state.engine.combat.attackers.contains(&target));
    }
}
#[test]
fn departed_reentered_or_protected_only_target_fizzles_entire_spell_including_treasure() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for disposition in 0..3 {
            let mut state = ready(actor);
            let target = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            state.objects.get_mut(target).tapped = true;
            let mut surface = surface();
            let spell = announce(&mut surface, &mut state, actor, target);
            if disposition < 2 {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(target, Zone::Hand),
                );
                if disposition == 1 {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(target, Zone::Battlefield),
                    );
                    state.objects.get_mut(target).tapped = true;
                }
            } else {
                grant(
                    &mut state,
                    spell,
                    actor.opponent(),
                    target,
                    Keywords::PROTECTION_FROM_MONOCOLORED,
                );
            }
            let mut restored = replay(&state);
            resolve(&mut surface, &mut state);
            resolve(&mut self::surface(), &mut restored);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
            assert_eq!(state.objects.get(target).controller, actor.opponent());
            assert!(!engine::has_effective_keyword(
                &state,
                target,
                Keywords::HASTE
            ));
            assert_eq!(treasures(&state, actor), 0);
            if disposition > 0 {
                assert!(state.objects.get(target).tapped);
            }
        }
    }
}
#[test]
fn printed_target_menu_respects_opposing_hexproof_and_source_protection() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let own = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let opposing = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let protected = put(
            &mut state,
            actor.opponent(),
            "Guardian of the Guildpact",
            Zone::Battlefield,
        );
        let source = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        grant(&mut state, source, actor, own, Keywords::HEXPROOF);
        grant(
            &mut state,
            source,
            actor.opponent(),
            opposing,
            Keywords::HEXPROOF,
        );
        let spell = put(&mut state, actor, "Involuntary Employment", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 2;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
        let mut surface = surface();
        next(&mut surface, &mut state);
        apply(&mut surface, &mut state, Action::CastSpell(spell));
        let legal = match next(&mut surface, &mut state) {
            Decision::ChooseTargets { legal_targets, .. } => legal_targets,
            other => panic!("expected target menu {other:?}"),
        };
        assert!(legal.contains(&Target::Object(own)));
        assert!(!legal.contains(&Target::Object(opposing)));
        assert!(!legal.contains(&Target::Object(protected)));
        let before = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::ChooseTarget(Target::Object(opposing))).is_err());
        assert_eq!(before, serde_json::to_vec(&state).unwrap());
    }
}
#[test]
fn cleanup_binding_does_not_follow_stolen_creature_after_owner_zone_change() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let mut surface = surface();
        announce(&mut surface, &mut state, actor, target);
        resolve(&mut surface, &mut state);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(target, Zone::Hand));
        assert!(state.players[actor.opponent().index()]
            .hand
            .contains(&target));
        assert!(!state.players[actor.index()].battlefield.contains(&target));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change_to_battlefield_under_controller(target, actor),
        );
        let mut restored = replay(&state);
        cleanup(&mut surface, &mut state);
        cleanup(&mut self::surface(), &mut restored);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
        assert_eq!(state.objects.get(target).controller, actor);
        assert!(!engine::has_effective_keyword(
            &state,
            target,
            Keywords::HASTE
        ));
    }
}

#[test]
fn employment_cast_during_real_madness_cleanup_priority_expires_before_turn_changes() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let anchor = state.players[actor.index()].hand[0];
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(anchor, Zone::Graveyard),
        );
        put(&mut state, actor, "High Fae Trickster", Zone::Battlefield);
        let target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let spell = put(&mut state, actor, "Involuntary Employment", Zone::Hand);
        let temper = put(&mut state, actor, "Fiery Temper", Zone::Hand);
        for _ in 0..7 {
            event::propose_and_commit(&mut state, ProposedEvent::draw(actor));
        }
        assert_eq!(state.players[actor.index()].hand.len(), 9);
        let forest = *state.players[actor.index()].hand.last().unwrap();
        assert_eq!(state.objects.get(forest).name, "Forest");
        state.step = Step::End;
        let ending_round = state.turn;
        let mut surface = surface();
        let mut discarded = false;
        for _ in 0..20 {
            match next(&mut surface, &mut state) {
                Decision::CastSpellOrPass { .. } => apply(&mut surface, &mut state, Action::Pass),
                Decision::Discard {
                    player,
                    count,
                    choices,
                } => {
                    assert_eq!((player, count), (actor, 2));
                    assert_eq!(state.step, Step::Cleanup);
                    assert!(choices.contains(&temper) && choices.contains(&forest));
                    apply(
                        &mut surface,
                        &mut state,
                        Action::Discard(vec![temper, forest]),
                    );
                    discarded = true;
                    break;
                }
                other => panic!("unexpected pre-cleanup decision {other:?}"),
            }
        }
        assert!(discarded);
        assert_eq!(state.objects.get(temper).zone, Zone::Exile);
        assert!(state.stack.iter().any(|item| item.madness_offer));
        assert_eq!(state.players[actor.index()].hand.len(), 7);
        announce_existing(&mut surface, &mut state, actor, target, spell);
        let mut restored = replay(&state);
        for branch in [&mut state, &mut restored] {
            let mut branch_surface = self::surface();
            let mut resolved = false;
            for _ in 0..20 {
                let decision = next(&mut branch_surface, branch);
                if branch.objects.get(spell).zone == Zone::Graveyard {
                    resolved = true;
                    break;
                }
                assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
                apply(&mut branch_surface, branch, Action::Pass);
            }
            assert!(resolved);
            assert_eq!(
                (branch.active_player, branch.turn, branch.step),
                (actor, ending_round, Step::Cleanup)
            );
            assert_eq!(branch.objects.get(target).controller, actor);
            assert!(engine::has_effective_keyword(
                branch,
                target,
                Keywords::HASTE
            ));
            assert_eq!(treasures(branch, actor), 1);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
        let mut restored = replay(&state);
        for branch in [&mut state, &mut restored] {
            let mut branch_surface = self::surface();
            let mut declined = false;
            for _ in 0..20 {
                match next(&mut branch_surface, branch) {
                    Decision::CastSpellOrPass { .. } => {
                        apply(&mut branch_surface, branch, Action::Pass)
                    }
                    Decision::ChooseMadnessCast { player, card } => {
                        assert_eq!((player, card), (actor, temper));
                        apply(
                            &mut branch_surface,
                            branch,
                            Action::ChooseMadnessCast(false),
                        );
                        declined = true;
                        break;
                    }
                    other => panic!("unexpected cleanup Madness decision {other:?}"),
                }
            }
            assert!(declined);
            let mut ended = false;
            for _ in 0..20 {
                let decision = next(&mut branch_surface, branch);
                if branch.active_player != actor {
                    ended = true;
                    break;
                }
                assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
                apply(&mut branch_surface, branch, Action::Pass);
            }
            assert!(ended);
            assert_eq!(branch.active_player, actor.opponent());
            assert_eq!(branch.objects.get(target).controller, actor.opponent());
            assert!(!engine::has_effective_keyword(
                branch,
                target,
                Keywords::HASTE
            ));
            assert!(branch.engine.until_end_of_turn.is_empty());
            assert_eq!(treasures(branch, actor), 1);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}
