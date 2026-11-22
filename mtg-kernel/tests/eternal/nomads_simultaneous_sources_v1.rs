use super::{
    act, activate, cast, give_priority, life, next, pass_until, pool, put, restored, settled,
    turn_of, Action, Decision, GameState, ManaColor, ObjectId, PlayerId, Step, Target, Zone, P0,
    P1,
};

fn block(active: PlayerId, green_first: bool, charges: u8) -> (GameState, [ObjectId; 4]) {
    let mut state = turn_of(active);
    let nomads = put(&mut state, active, "Nomads en-Kor", Zone::Battlefield);
    let ally = put(&mut state, active, "Llanowar Elves", Zone::Battlefield);
    let red = put(
        &mut state,
        active.opponent(),
        "Grim Lavamancer",
        Zone::Battlefield,
    );
    let green = put(
        &mut state,
        active.opponent(),
        "Llanowar Elves",
        Zone::Battlefield,
    );
    for (name, creature) in [("Mask of Law and Grace", ally), ("Lifelink", red)] {
        let aura = put(&mut state, active, name, Zone::Hand);
        pool(&mut state, active, &[ManaColor::W], 0);
        cast(&mut state, aura, &[Target::Object(creature)]);
        settled(&mut state);
        assert_eq!(
            state.objects.get(aura).v4.attached_to.unwrap().object,
            creature
        );
    }
    // With two shields, let Lavamancer assign two points and Elves one.
    state.objects.get_mut(red).counters.plus1_plus1 = i32::from(charges - 1);
    for _ in 0..charges {
        give_priority(&mut state, active);
        activate(&mut state, nomads, 0, &[Target::Object(ally)]);
        settled(&mut state);
    }
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![nomads]));
    pass_until(&mut state, |state| state.step == Step::DeclareBlockers);
    let blockers = if green_first {
        [green, red]
    } else {
        [red, green]
    };
    act(
        &mut state,
        Action::DeclareBlockers(blockers.map(|blocker| (blocker, nomads)).to_vec()),
    );
    (state, [nomads, ally, red, green])
}

fn source_menu(
    state: &mut GameState,
    active: PlayerId,
    nomads: ObjectId,
    red: ObjectId,
    green: ObjectId,
) {
    for _ in 0..24 {
        match next(state) {
            Decision::CastSpellOrPass { .. } if state.step != Step::EndCombat => {
                act(state, Action::Pass)
            }
            Decision::ChooseEffectTargets {
                player,
                source,
                legal_targets,
                min_targets: 1,
                max_targets: 1,
                can_finish: false,
                ..
            } => {
                assert_eq!((player, source), (active, nomads));
                assert_eq!(
                    legal_targets,
                    vec![Target::Object(red), Target::Object(green)]
                );
                return;
            }
            other => panic!("expected the simultaneous damage sources, got {other:?}"),
        }
    }
    panic!("missing redirect source menu");
}

#[test]
fn one_charge_chooses_which_blocker_deals_its_point_to_the_protected_ally() {
    for active in [P0, P1] {
        for green_first in [false, true] {
            let (mut state, [nomads, ally, red, green]) = block(active, green_first, 1);
            source_menu(&mut state, active, nomads, red, green);
            let mut invalid = restored(&state);
            let before = invalid.clone();
            assert!(mtg_kernel::engine::step(
                &mut invalid,
                Action::ChooseEffectTarget(Target::Object(ally))
            )
            .is_err());
            assert_eq!(invalid, before);
            for chosen in [red, green] {
                let mut branch = restored(&state);
                act(
                    &mut branch,
                    Action::ChooseEffectTarget(Target::Object(chosen)),
                );
                pass_until(&mut branch, |state| state.step == Step::EndCombat);
                assert_eq!(
                    branch.objects.get(ally).zone,
                    if chosen == red {
                        Zone::Battlefield
                    } else {
                        Zone::Graveyard
                    }
                );
                assert_eq!(branch.objects.get(nomads).zone, Zone::Graveyard);
                assert_eq!(
                    life(&branch, active.opponent()),
                    20 + i32::from(chosen == green)
                );
            }
        }
    }
}

#[test]
fn two_charges_recompute_which_sources_still_have_redirectable_damage() {
    for active in [P0, P1] {
        let (mut state, [nomads, ally, red, green]) = block(active, true, 2);
        source_menu(&mut state, active, nomads, red, green);
        for choices in [vec![green], vec![red, green], vec![red, red]] {
            let mut branch = restored(&state);
            for (index, chosen) in choices.iter().enumerate() {
                if index > 0 {
                    source_menu(&mut branch, active, nomads, red, green);
                    branch = restored(&branch);
                }
                act(
                    &mut branch,
                    Action::ChooseEffectTarget(Target::Object(*chosen)),
                );
            }
            pass_until(&mut branch, |state| state.step == Step::EndCombat);
            let spent_on_green = choices.contains(&green);
            assert_eq!(
                branch.objects.get(ally).zone,
                if spent_on_green {
                    Zone::Graveyard
                } else {
                    Zone::Battlefield
                }
            );
            assert_eq!(
                life(&branch, active.opponent()),
                20 + i32::from(spent_on_green)
            );
        }
    }
}
