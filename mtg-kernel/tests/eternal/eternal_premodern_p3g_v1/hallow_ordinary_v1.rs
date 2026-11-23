//! Replacement order on a real spell, including partial prevention and prohibition.
use super::{
    act, cast, give_priority, life, pool, put, restored, settle, settled, turn_of, Action,
    Decision, GameState, ManaColor, ObjectId, Target, Zone, P0, P1,
};

fn protect(state: &mut GameState, spell: ObjectId) {
    give_priority(state, P0);
    let hallow = put(state, P0, "Hallow", Zone::Hand);
    pool(state, P0, &[ManaColor::W], 0);
    cast(state, hallow, &[Target::Object(spell)]);
}

fn bolt_board(spheres: usize, ojer: bool) -> (GameState, ObjectId) {
    let mut state = turn_of(P1);
    for _ in 0..spheres {
        put(&mut state, P0, "Sphere of Law", Zone::Battlefield);
    }
    if ojer {
        put(
            &mut state,
            P1,
            "Ojer Axonil, Deepest Might",
            Zone::Battlefield,
        );
    }
    let spell = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, spell, &[Target::Player(P0)]);
    protect(&mut state, spell);
    (state, spell)
}

fn outcome(start: &GameState, source: ObjectId, choices: &[(u16, u16)]) -> i32 {
    let mut state = restored(start);
    for &(option, count) in choices {
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseEffectOption {
            player: P0, source: actual, option_count
        }) if actual == source && option_count == count)
        );
        state = restored(&state);
        act(&mut state, Action::ChooseEffectOption(option));
    }
    settled(&mut state);
    assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
    assert_eq!(life(&state, P1), 20);
    life(&state, P0)
}

#[test]
fn two_spheres_allow_hallow_before_between_or_after_them() {
    let (state, bolt) = bolt_board(2, false);
    assert_eq!(outcome(&state, bolt, &[(0, 3)]), 23);
    assert_eq!(outcome(&state, bolt, &[(1, 3), (0, 2)]), 21);
    assert_eq!(outcome(&state, bolt, &[(2, 3), (1, 2)]), 20);
}

#[test]
fn ojer_and_sphere_may_each_precede_the_other_on_a_shielded_bolt() {
    let (state, bolt) = bolt_board(1, true);
    assert_eq!(outcome(&state, bolt, &[(0, 3)]), 23);
    assert_eq!(outcome(&state, bolt, &[(1, 3), (0, 2)]), 21);
    assert_eq!(outcome(&state, bolt, &[(2, 3), (1, 2)]), 22);
    assert_eq!(outcome(&state, bolt, &[(1, 3), (1, 2)]), 24);
}

#[test]
fn mask_of_law_and_grace_competes_with_hallow_for_each_creatures_controller() {
    for caster in [P0, P1] {
        let controller = caster.opponent();
        let mut state = turn_of(caster);
        let creature = put(&mut state, controller, "Elvish Mystic", Zone::Battlefield);
        let mask = put(&mut state, caster, "Mask of Law and Grace", Zone::Hand);
        pool(&mut state, caster, &[ManaColor::W], 0);
        cast(&mut state, mask, &[Target::Object(creature)]);
        settled(&mut state);
        assert_eq!(
            state.objects.get(mask).v4.attached_to.unwrap().object,
            creature
        );

        let pyro = put(&mut state, caster, "Pyroclasm", Zone::Hand);
        pool(&mut state, caster, &[ManaColor::R], 1);
        cast(&mut state, pyro, &[]);
        give_priority(&mut state, caster);
        let hallow = put(&mut state, caster, "Hallow", Zone::Hand);
        pool(&mut state, caster, &[ManaColor::W], 0);
        cast(&mut state, hallow, &[Target::Object(pyro)]);

        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectOption { player, source, option_count: 2 })
                if player == controller && source == pyro
        ));
        // Hallow gains two life for its caster. Protection prevents the same
        // damage without a gain.
        for (option, caster_life) in [(0, 22), (1, 20)] {
            let mut branch = restored(&state);
            assert_eq!(branch.state_hash(), state.state_hash());
            act(&mut branch, Action::ChooseEffectOption(option));
            settled(&mut branch);
            assert_eq!(life(&branch, caster), caster_life);
            assert_eq!(life(&branch, controller), 20);
            let object = branch.objects.get(creature);
            assert_eq!((object.zone, object.damage), (Zone::Battlefield, 0));
            assert_eq!(branch.objects.get(pyro).zone, Zone::Graveyard);
        }
    }
}

#[test]
fn impractical_joke_prohibits_hallow_and_nishoba_still_loses_its_counter() {
    let mut state = turn_of(P1);
    let phantom = put(&mut state, P0, "Phantom Nishoba", Zone::Battlefield);
    state.objects.get_mut(phantom).counters.plus1_plus1 = 7;
    let spell = put(&mut state, P1, "Impractical Joke", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, spell, &[Target::Object(phantom)]);
    if matches!(
        super::next(&mut state),
        Decision::ChooseTargets {
            can_finish: true,
            ..
        }
    ) {
        act(&mut state, Action::FinishEffectSelection);
    }
    protect(&mut state, spell);
    settled(&mut state);
    assert_eq!(life(&state, P0), 20);
    let phantom = state.objects.get(phantom);
    assert_eq!(
        (phantom.zone, phantom.damage, phantom.counters.plus1_plus1),
        (Zone::Battlefield, 3, 6)
    );
}

#[test]
fn nomads_redirect_after_dominate_reveals_an_active_player_choice() {
    let mut state = turn_of(P0);
    let nomads = put(&mut state, P1, "Nomads en-Kor", Zone::Battlefield);
    let acolyte = put(&mut state, P1, "Crimson Acolyte", Zone::Battlefield);
    give_priority(&mut state, P1);
    super::activate(&mut state, nomads, 0, &[Target::Object(acolyte)]);
    settled(&mut state);

    give_priority(&mut state, P0);
    let dominate = put(&mut state, P0, "Dominate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 3);
    cast(&mut state, dominate, &[Target::Object(acolyte)]);
    settled(&mut state);
    assert_eq!(state.objects.get(acolyte).controller, P0);

    let pyro = put(&mut state, P0, "Pyroclasm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, pyro, &[]);
    protect(&mut state, pyro);

    // First P0 takes Acolyte's red protection for its own two damage. P1
    // redirects one damage from Nomads. The redirected point is a new P0
    // choice; Hallow gains one for that point and one for the remainder.
    for (chooser, option) in [(P0, 1), (P1, 1), (P0, 0)] {
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseEffectOption {
            player, source, option_count: 2
        }) if player == chooser && source == pyro)
        );
        state = restored(&state);
        act(&mut state, Action::ChooseEffectOption(option));
    }
    settled(&mut state);
    assert_eq!((life(&state, P0), life(&state, P1)), (22, 20));
    for id in [nomads, acolyte] {
        assert_eq!(state.objects.get(id).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(id).damage, 0);
    }
}

#[test]
fn confirmed_packet_keeps_its_prevention_when_a_later_redirect_consumes_a_shared_charge() {
    let mut state = turn_of(P0);
    let first = put(&mut state, P1, "Nomads en-Kor", Zone::Battlefield);
    let second = put(&mut state, P1, "Nomads en-Kor", Zone::Battlefield);
    let third = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    give_priority(&mut state, P1);
    super::activate(&mut state, first, 0, &[Target::Object(second)]);
    settled(&mut state);
    give_priority(&mut state, P1);
    super::activate(&mut state, second, 0, &[Target::Object(third)]);
    settled(&mut state);
    give_priority(&mut state, P0);
    let dominate = put(&mut state, P0, "Dominate", Zone::Hand);
    // Dominate a one-mana Nomads with exactly enough mana for X = 1.
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 2);
    cast(&mut state, dominate, &[Target::Object(second)]);
    settled(&mut state);
    assert_eq!(state.objects.get(second).controller, P0);
    let pyro = put(&mut state, P0, "Pyroclasm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, pyro, &[]);
    protect(&mut state, pyro);

    // P0 prevents B's original packet without consuming B->C. P1 then
    // redirects A->B and P0 spends B->C on that newly revealed point.
    // The already selected prevention for B's original packet stays applied.
    for (chooser, option) in [(P0, 0), (P1, 1), (P0, 1)] {
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseEffectOption {
            player, source, option_count: 2
        }) if player == chooser && source == pyro)
        );
        state = restored(&state);
        act(&mut state, Action::ChooseEffectOption(option));
    }
    settled(&mut state);
    assert_eq!((life(&state, P0), life(&state, P1)), (26, 20));
    for object in [first, second, third] {
        assert_eq!(state.objects.get(object).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(object).damage, 0);
    }
}

#[test]
fn hallowed_and_uncovered_blockers_share_one_nishoba_counter_for_the_combat_batch() {
    use mtg_kernel::state::Step;

    for shield_sprite in [false, true] {
        for sprite_first in [false, true] {
            let mut state = turn_of(P0);
            let nishoba = put(&mut state, P0, "Phantom Nishoba", Zone::Battlefield);
            state.objects.get_mut(nishoba).counters.plus1_plus1 = 7;
            let mongrel = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
            give_priority(&mut state, P1);
            let sprite = put(&mut state, P1, "Spellstutter Sprite", Zone::Hand);
            pool(&mut state, P1, &[ManaColor::U], 1);
            cast(&mut state, sprite, &[]);
            if shield_sprite {
                protect(&mut state, sprite);
            }
            settled(&mut state);
            assert_eq!(state.objects.get(sprite).zone, Zone::Battlefield);

            super::pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
            act(&mut state, Action::DeclareAttackers(vec![nishoba]));
            super::pass_until(&mut state, |state| state.step == Step::DeclareBlockers);
            let blockers = if sprite_first {
                [sprite, mongrel]
            } else {
                [mongrel, sprite]
            };
            act(
                &mut state,
                Action::DeclareBlockers(
                    blockers
                        .into_iter()
                        .map(|blocker| (blocker, nishoba))
                        .collect(),
                ),
            );

            let mut choices = 0;
            for _ in 0..100 {
                let decision = super::next(&mut state);
                if state.step == Step::EndCombat {
                    break;
                }
                match decision {
                    Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
                    Decision::OrderTriggers { pending, .. } => act(
                        &mut state,
                        Action::OrderTriggers((0..pending.len()).collect()),
                    ),
                    Decision::ChooseEffectOption {
                        player: P0,
                        source,
                        option_count: 2,
                    } if shield_sprite && source == sprite => {
                        // Nishoba prevents Sprite's covered damage; Mongrel's
                        // ordinary damage is simultaneous with that point.
                        state = restored(&state);
                        act(&mut state, Action::ChooseEffectOption(1));
                        choices += 1;
                    }
                    other => panic!("unexpected combat decision {other:?}"),
                }
            }
            assert_eq!(state.step, Step::EndCombat);
            assert_eq!(choices, usize::from(shield_sprite));
            assert_eq!(state.objects.get(nishoba).zone, Zone::Battlefield);
            assert_eq!(state.objects.get(nishoba).damage, 0);
            assert_eq!(
                state.objects.get(nishoba).counters.plus1_plus1,
                6,
                "Sprite and Mongrel deal one simultaneous combat batch"
            );
        }
    }
}
