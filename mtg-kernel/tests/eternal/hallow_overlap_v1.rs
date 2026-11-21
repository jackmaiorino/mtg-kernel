//! CR 616.1 and Hallow: the affected player or permanent's controller chooses prevention.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{propose_and_commit, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
use mtg_kernel::rl::{
    observe_policy_v5, policy_legal_action_candidates_v5, ActionSemanticV1, TargetRefV1,
};
use mtg_kernel::state::{GameState, Step, Target, Zone};

const SEATS: [PlayerId; 2] = [PlayerId::P0, PlayerId::P1];

fn board(active: PlayerId) -> GameState {
    let deck = [
        "Lightning Bolt",
        "Crypt Rats",
        "Hallow",
        "Hallow",
        "Plains",
        "Plains",
    ]
    .map(|name| card_id_by_name(name).unwrap());
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &deck,
        &deck,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x48414C4C,
        active,
    );
    state.step = Step::Main1;
    state
}

fn give(state: &mut GameState, seat: PlayerId, name: &str) -> ObjectId {
    let card = card_id_by_name(name).unwrap();
    let object = *state.players[seat.index()]
        .library
        .iter()
        .find(|&&id| state.objects.get(id).card_def == card)
        .unwrap();
    propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Hand));
    assert_eq!(state.objects.get(object).zone, Zone::Hand);
    object
}

fn act(state: &mut GameState, action: Action) {
    engine::step(state, action).unwrap();
}

fn cast(state: &mut GameState, caster: PlayerId, name: &str, target: Option<Target>) -> ObjectId {
    let spell = give(state, caster, name);
    let Decision::CastSpellOrPass {
        player,
        castable_spells,
        ..
    } = engine::advance_until_decision(state)
    else {
        panic!("expected priority before casting {name}")
    };
    assert_eq!(player, caster);
    assert!(castable_spells.contains(&spell), "{name} is not castable");
    act(state, Action::CastSpell(spell));
    if let Some(target) = target {
        match engine::advance_until_decision(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&target));
                act(state, Action::ChooseTarget(target));
            }
            Decision::ChooseEffectTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&target));
                act(state, Action::ChooseEffectTarget(target));
            }
            decision => panic!("expected target of {name}, got {decision:?}"),
        }
    }
    spell
}

fn pass(state: &mut GameState, expected: PlayerId) {
    let Decision::CastSpellOrPass { player, .. } = engine::advance_until_decision(state) else {
        panic!("expected priority")
    };
    assert_eq!(player, expected);
    act(state, Action::Pass);
}

fn shields(state: &mut GameState, source: ObjectId, casters: &[PlayerId]) {
    for &caster in casters {
        let holder = state.priority_player;
        if holder != caster {
            pass(state, holder);
        }
        state.players[caster.index()].mana_pool[ManaColor::W.pool_index()] += 1;
        cast(state, caster, "Hallow", Some(Target::Object(source)));
    }
}

fn settle(state: &mut GameState, mut choice: impl FnMut(&mut GameState, Decision)) {
    for _ in 0..80 {
        match engine::advance_until_decision(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            decision @ Decision::ChooseEffectTargets { .. } => choice(state, decision),
            other => panic!("unexpected decision {other:?}"),
        }
    }
    panic!("stack did not settle");
}

fn choose(
    state: &mut GameState,
    decision: Decision,
    source: ObjectId,
    recipient: PlayerId,
    beneficiary: PlayerId,
) {
    let Decision::ChooseEffectTargets {
        player,
        source: actual_source,
        selected_count: 0,
        min_targets: 1,
        max_targets: 1,
        can_finish: false,
        legal_targets,
    } = decision
    else {
        panic!("expected one mandatory prevention choice")
    };
    assert_eq!((player, actual_source), (recipient, source));
    assert_eq!(legal_targets, SEATS.map(Target::Player));
    // Restore at the pending choice and submit the public schema-5 action on the restored state.
    let mut restored: GameState =
        serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
    assert_eq!(restored.state_hash(), state.state_hash());
    let mut restored_surface = PolicySurfaceV5::new();
    let restored_decision = restored_surface.next_decision(&mut restored).unwrap();
    let mut surface = PolicySurfaceV5::new();
    let live_decision = surface.next_decision(state).unwrap();
    let observation = observe_policy_v5(state, &surface, player, 0, 0, 0, 1).unwrap();
    assert_eq!(observation.schema_version, 5);
    assert_eq!(observation.acting_player, player.into());
    assert_eq!(
        observation,
        observe_policy_v5(&restored, &restored_surface, player, 0, 0, 0, 1).unwrap()
    );
    let live = policy_legal_action_candidates_v5(&live_decision, &surface, state).unwrap();
    let replay =
        policy_legal_action_candidates_v5(&restored_decision, &restored_surface, &restored)
            .unwrap();
    assert_eq!(live.len(), 2);
    assert_eq!(replay.len(), 2);
    for ((original, restored), seat) in live.iter().zip(&replay).zip(SEATS) {
        assert_eq!(original.record, restored.record);
        assert_eq!(original.record.schema_version, 5);
        let ActionSemanticV1::ChooseEffectTarget {
            actor,
            source: reference,
            target: TargetRefV1::Player { player: target },
            selected_count: 0,
            min_targets: 1,
            max_targets: 1,
        } = &original.record.semantic
        else {
            panic!("expected a public seat target")
        };
        assert_eq!(*actor, recipient.into());
        assert_eq!(reference.arena_id, source.0);
        assert_eq!(*target, seat.into());
    }
    restored_surface
        .apply(
            &mut restored,
            replay[beneficiary.index()].policy_action.clone(),
        )
        .unwrap();
    act(
        state,
        Action::ChooseEffectTarget(Target::Player(beneficiary)),
    );
    assert_eq!(state.state_hash(), restored.state_hash());
}

fn bolt(active: PlayerId, target: PlayerId, controllers: &[PlayerId]) -> (GameState, ObjectId) {
    let mut state = board(active);
    state.players[active.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    let source = cast(
        &mut state,
        active,
        "Lightning Bolt",
        Some(Target::Player(target)),
    );
    shields(&mut state, source, controllers);
    (state, source)
}

#[test]
fn opposing_hallows_allow_either_beneficiary_on_bolt_and_restore_the_choice() {
    for active in SEATS {
        for target in SEATS {
            for beneficiary in SEATS {
                let (mut state, source) = bolt(active, target, &[active, active.opponent()]);
                let mut count = 0;
                settle(&mut state, |state, decision| {
                    choose(state, decision, source, target, beneficiary);
                    count += 1;
                });
                assert_eq!(count, 1);
                assert_eq!(state.players[beneficiary.index()].life, 23);
                assert_eq!(state.players[beneficiary.opponent().index()].life, 20);
            }
        }
    }
}

#[test]
fn no_prevention_menu_for_zero_one_or_two_identical_beneficiaries() {
    for active in SEATS {
        let other = active.opponent();
        for count in 0..=2 {
            let (mut state, _) = bolt(active, other, &[active; 2][..count]);
            settle(&mut state, |_, decision| {
                panic!("unexpected prevention menu {decision:?}")
            });
            assert_eq!(
                state.players[active.index()].life,
                if count == 0 { 20 } else { 23 }
            );
            assert_eq!(
                state.players[other.index()].life,
                if count == 0 { 17 } else { 20 }
            );
        }
    }
}

#[test]
fn opposing_hallows_choose_separately_for_each_crypt_rats_damage_recipient() {
    for active in SEATS {
        let other = active.opponent();
        let mut state = board(active);
        state.players[active.index()].mana_pool[ManaColor::B.pool_index()] = 1;
        state.players[active.index()].mana_pool[ManaColor::C.pool_index()] = 2;
        let rats = cast(&mut state, active, "Crypt Rats", None);
        shields(&mut state, rats, &[active, other]);
        settle(&mut state, |_, decision| {
            panic!("unexpected choice before activation {decision:?}")
        });
        assert_eq!(state.objects.get(rats).zone, Zone::Battlefield);
        for activation in 1..=2 {
            state.players[active.index()].mana_pool[ManaColor::B.pool_index()] += 1;
            let Decision::CastSpellOrPass {
                player,
                activatable_abilities,
                ..
            } = engine::advance_until_decision(&mut state)
            else {
                panic!("expected priority for Rats")
            };
            assert_eq!(player, active);
            assert!(activatable_abilities.contains(&(rats, 0)));
            act(&mut state, Action::ActivateAbility(rats, 0));
            let mut seen = [0; 2];
            settle(&mut state, |state, decision| {
                let Decision::ChooseEffectTargets { player, .. } = &decision else {
                    unreachable!()
                };
                let player = *player;
                seen[player.index()] += 1;
                // On the first activation the controller of Rats chooses a different
                // beneficiary for each of their two events. On the second,
                // each chooser gives the life to the opposing player.
                let beneficiary = if activation == 1 {
                    if player == active && seen[player.index()] == 1 {
                        other
                    } else {
                        active
                    }
                } else {
                    player.opponent()
                };
                choose(state, decision, rats, player, beneficiary);
            });
            assert_eq!(seen[active.index()], 2, "Rats and their controller");
            assert_eq!(seen[other.index()], 1);
            let totals = (
                state.players[active.index()].life,
                state.players[other.index()].life,
            );
            assert_eq!(totals, if activation == 1 { (22, 21) } else { (23, 23) });
            assert_eq!(state.objects.get(rats).zone, Zone::Battlefield);
            assert_eq!(state.objects.get(rats).damage, 0);
        }
    }
}
