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

fn source_menu(state: &mut GameState, active: PlayerId, nomads: ObjectId, sources: &[ObjectId]) {
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
                    sources
                        .iter()
                        .copied()
                        .map(Target::Object)
                        .collect::<Vec<_>>()
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
            source_menu(&mut state, active, nomads, &[red, green]);
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
        source_menu(&mut state, active, nomads, &[red, green]);
        for choices in [vec![green], vec![red, green], vec![red, red]] {
            let mut branch = restored(&state);
            for (index, chosen) in choices.iter().enumerate() {
                if index > 0 {
                    source_menu(&mut branch, active, nomads, &[red, green]);
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

#[test]
fn two_nomads_controllers_choose_their_sources_in_apnap_order() {
    use mtg_kernel::event::{self, ProposedEvent};
    for active in [P0, P1] {
        let mut state = turn_of(active);
        let seats = [active, active.opponent()].map(|owner| {
            let nomads = put(&mut state, owner, "Nomads en-Kor", Zone::Battlefield);
            let ally = put(&mut state, owner, "Llanowar Elves", Zone::Battlefield);
            let red = put(
                &mut state,
                owner.opponent(),
                "Grim Lavamancer",
                Zone::Battlefield,
            );
            let green = put(
                &mut state,
                owner.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            give_priority(&mut state, owner);
            activate(&mut state, nomads, 0, &[Target::Object(ally)]);
            settled(&mut state);
            (owner, nomads, red, green)
        });
        let events = seats
            .iter()
            .rev()
            .flat_map(|&(_, nomads, red, green)| {
                [red, green].map(|id| ProposedEvent::damage(id, Target::Object(nomads), 1))
            })
            .collect();
        event::propose_and_commit_batch(&mut state, events);
        for (owner, nomads, red, green) in seats {
            source_menu(&mut state, owner, nomads, &[red, green]);
            state = restored(&state);
            act(&mut state, Action::ChooseEffectTarget(Target::Object(red)));
        }
        settled(&mut state);
    }
}

#[test]
fn distinct_shields_choose_each_source_and_then_its_destination() {
    use mtg_kernel::event::CommittedEvent;

    for active in [P0, P1] {
        // Either three one-point blockers or two blockers assigning two and one.
        for three_blockers in [true, false] {
            let mut state = turn_of(active);
            let nomads = put(&mut state, active, "Nomads en-Kor", Zone::Battlefield);
            let recipients = ["Wild Mongrel", "Kor Skyfisher"]
                .map(|name| put(&mut state, active, name, Zone::Battlefield));
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
            let mut blockers = vec![red, green];
            let (first, second) = if three_blockers {
                let blue = put(
                    &mut state,
                    active.opponent(),
                    "Spellstutter Sprite",
                    Zone::Battlefield,
                );
                blockers.push(blue);
                (green, blue)
            } else {
                state.objects.get_mut(red).counters.plus1_plus1 = 1;
                (red, green)
            };
            for receiver in recipients {
                give_priority(&mut state, active);
                activate(&mut state, nomads, 0, &[Target::Object(receiver)]);
                settled(&mut state);
            }
            pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
            act(&mut state, Action::DeclareAttackers(vec![nomads]));
            pass_until(&mut state, |s| s.step == Step::DeclareBlockers);
            act(
                &mut state,
                Action::DeclareBlockers(blockers.iter().map(|&b| (b, nomads)).collect()),
            );
            source_menu(&mut state, active, nomads, &blockers);
            for first_destination in [0, 1] {
                let mut branch = restored(&state);
                act(
                    &mut branch,
                    Action::ChooseEffectTarget(Target::Object(first)),
                );
                assert!(matches!(next(&mut branch), Decision::ChooseEffectOption {
                    player, source, option_count: 2
                } if player == active && source == first));
                branch = restored(&branch);
                act(
                    &mut branch,
                    Action::ChooseEffectOption(first_destination as u16),
                );
                let remaining_sources = if three_blockers {
                    vec![red, second]
                } else {
                    vec![red, green]
                };
                source_menu(&mut branch, active, nomads, &remaining_sources);
                act(
                    &mut branch,
                    Action::ChooseEffectTarget(Target::Object(second)),
                );
                pass_until(&mut branch, |s| s.step == Step::EndCombat);
                let mut damage = branch
                    .engine
                    .event_history
                    .iter()
                    .filter_map(|event| match event {
                        CommittedEvent::Damage {
                            source,
                            target: Target::Object(target),
                            amount,
                        } if blockers.contains(source) => Some((*source, *target, *amount)),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                damage.sort_by_key(|(source, target, _)| (source.0, target.0));
                let mut expected = [
                    (red, nomads, 1),
                    (first, recipients[first_destination], 1),
                    (second, recipients[1 - first_destination], 1),
                ];
                expected.sort_by_key(|(source, target, _)| (source.0, target.0));
                assert_eq!(damage, expected);
            }
        }
    }
}
