use super::*;

#[test]
fn a_blocker_can_redirect_or_use_its_prevention_on_nomads() {
    for active in [P0, P1] {
        for (hallow, third_blocker) in [(false, false), (true, false), (true, true)] {
            let mut state = turn_of(active);
            let nomads = put(&mut state, active, "Nomads en-Kor", Zone::Battlefield);
            let ally = put(&mut state, active, "Wild Mongrel", Zone::Battlefield);
            let green = put(
                &mut state,
                active.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let (covered, mother) = if hallow {
                // Sprite has flash, so Hallow can target it as a spell this turn.
                give_priority(&mut state, active.opponent());
                let sprite = put(
                    &mut state,
                    active.opponent(),
                    "Spellstutter Sprite",
                    Zone::Hand,
                );
                pool(&mut state, active.opponent(), &[ManaColor::U], 1);
                cast(&mut state, sprite, &[]);
                give_priority(&mut state, active);
                let hallow = put(&mut state, active, "Hallow", Zone::Hand);
                pool(&mut state, active, &[ManaColor::W], 0);
                cast(&mut state, hallow, &[Target::Object(sprite)]);
                settled(&mut state);
                (sprite, None)
            } else {
                let red = put(
                    &mut state,
                    active.opponent(),
                    "Grim Lavamancer",
                    Zone::Battlefield,
                );
                let mother = put(&mut state, active, "Mother of Runes", Zone::Battlefield);
                (red, Some(mother))
            };
            let mut sources = vec![green, covered];
            let third = third_blocker.then(|| {
                put(
                    &mut state,
                    active.opponent(),
                    "Grim Lavamancer",
                    Zone::Battlefield,
                )
            });
            sources.extend(third);
            give_priority(&mut state, active);
            activate(&mut state, nomads, 0, &[Target::Object(ally)]);
            settled(&mut state);
            pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
            act(&mut state, Action::DeclareAttackers(vec![nomads]));
            pass_until(&mut state, |s| s.step == Step::DeclareBlockers);
            act(
                &mut state,
                Action::DeclareBlockers(sources.iter().map(|&blocker| (blocker, nomads)).collect()),
            );
            if let Some(mother) = mother {
                // Red can legally block first. Mother grants protection afterwards.
                give_priority(&mut state, active);
                activate(&mut state, mother, 0, &[Target::Object(nomads)]);
                super::super::settle_option(&mut state, 3);
            }
            source_menu(&mut state, active, nomads, &sources);
            for redirect_covered in [false, true] {
                let mut branch = restored(&state);
                act(
                    &mut branch,
                    Action::ChooseEffectTarget(Target::Object(covered)),
                );
                assert!(matches!(next(&mut branch), Decision::ChooseEffectOption {
                    player, source, option_count: 2
                } if player == active && source == covered));
                branch = restored(&branch);
                // The published Hallow option precedes ordinary redirection, while
                // ordinary redirection precedes protection in a no-Hallow menu.
                let option = u16::from(redirect_covered == hallow);
                act(&mut branch, Action::ChooseEffectOption(option));
                if let Some(third) = third.filter(|_| !redirect_covered) {
                    // Hallow finished Sprite's packet without spending the shared shield.
                    source_menu(&mut branch, active, nomads, &[green, third]);
                    act(
                        &mut branch,
                        Action::ChooseEffectTarget(Target::Object(green)),
                    );
                }
                pass_until(&mut branch, |s| s.step == Step::EndCombat);
                assert_eq!(
                    branch.objects.get(nomads).zone,
                    if redirect_covered || third_blocker {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(
                    branch.objects.get(ally).damage,
                    u32::from(!hallow || !redirect_covered)
                );
                assert_eq!(life(&branch, active), 20 + i32::from(hallow));
            }
        }
    }
}

#[test]
fn self_redirection_spends_its_charge_and_leaves_both_sources_for_the_live_destination() {
    for active in [P0, P1] {
        let (mut state, [nomads, ally, red, green]) = block(active, true, 1);
        give_priority(&mut state, active);
        activate(&mut state, nomads, 0, &[Target::Object(nomads)]);
        settled(&mut state);
        source_menu(&mut state, active, nomads, &[red, green]);
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(green)),
        );
        assert!(matches!(next(&mut state), Decision::ChooseEffectOption {
            player, source, option_count: 2
        } if player == active && source == green));
        act(&mut state, Action::ChooseEffectOption(1));
        source_menu(&mut state, active, nomads, &[red, green]);
        for chosen in [red, green] {
            let mut branch = restored(&state);
            act(
                &mut branch,
                Action::ChooseEffectTarget(Target::Object(chosen)),
            );
            pass_until(&mut branch, |s| s.step == Step::EndCombat);
            assert!(branch.eternal_v1.as_ref().unwrap().e9a.redirects.is_empty());
            assert_eq!(
                branch.objects.get(ally).zone,
                if chosen == red {
                    Zone::Battlefield
                } else {
                    Zone::Graveyard
                }
            );
        }
    }
}

#[test]
fn one_point_spends_each_distinct_self_redirection_once() {
    use mtg_kernel::event::CommittedEvent;

    for owner in [P0, P1] {
        let caster = owner.opponent();
        let mut state = turn_of(caster);
        let nomads = put(&mut state, owner, "Nomads en-Kor", Zone::Battlefield);
        state.objects.get_mut(nomads).counters.plus1_plus1 = 1;
        for _ in 0..2 {
            give_priority(&mut state, owner);
            activate(&mut state, nomads, 0, &[Target::Object(nomads)]);
            settled(&mut state);
        }
        assert_eq!(state.eternal_v1.as_ref().unwrap().e9a.redirects.len(), 2);
        give_priority(&mut state, caster);
        let dart = put(&mut state, caster, "Lava Dart", Zone::Hand);
        pool(&mut state, caster, &[ManaColor::R], 0);
        cast(&mut state, dart, &[Target::Object(nomads)]);
        state = restored(&state);
        settled(&mut state);
        assert!(state.eternal_v1.as_ref().unwrap().e9a.redirects.is_empty());
        assert_eq!(state.objects.get(nomads).damage, 1);
        let hits = state
            .engine
            .event_history
            .iter()
            .filter(|event| {
                matches!(event,
                    CommittedEvent::Damage { source, target: Target::Object(receiver), amount: 1 }
                        if *source == dart && *receiver == nomads
                )
            })
            .count();
        assert_eq!(hits, 1);
    }
}
