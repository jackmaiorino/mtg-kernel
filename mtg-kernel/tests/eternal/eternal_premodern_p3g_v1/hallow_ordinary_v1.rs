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
        let mut caster_lives = [0; 2];
        for (option, caster_life) in caster_lives.iter_mut().enumerate() {
            let mut branch = restored(&state);
            assert_eq!(branch.state_hash(), state.state_hash());
            act(&mut branch, Action::ChooseEffectOption(option as u16));
            settled(&mut branch);
            *caster_life = life(&branch, caster);
            assert_eq!(life(&branch, controller), 20);
            let object = branch.objects.get(creature);
            assert_eq!((object.zone, object.damage), (Zone::Battlefield, 0));
            assert_eq!(branch.objects.get(pyro).zone, Zone::Graveyard);
        }
        caster_lives.sort_unstable();
        assert_eq!(caster_lives, [20, 22]);
    }
}

#[test]
fn soul_scar_mage_and_hallow_offer_the_victims_controller_a_choice_both_seats() {
    for caster in [P0, P1] {
        let victim_controller = caster.opponent();
        let mut state = turn_of(caster);
        put(&mut state, caster, "Soul-Scar Mage", Zone::Battlefield);
        let victim = put(
            &mut state,
            victim_controller,
            "Gurmag Angler",
            Zone::Battlefield,
        );
        let bolt = put(&mut state, caster, "Lightning Bolt", Zone::Hand);
        pool(&mut state, caster, &[ManaColor::R], 0);
        cast(&mut state, bolt, &[Target::Object(victim)]);
        give_priority(&mut state, caster);
        let hallow = put(&mut state, caster, "Hallow", Zone::Hand);
        pool(&mut state, caster, &[ManaColor::W], 0);
        cast(&mut state, hallow, &[Target::Object(bolt)]);

        let choice = settle(&mut state);
        assert!(matches!(
            &choice,
            Some(Decision::ChooseEffectOption { player, source, option_count: 2 })
                if *player == victim_controller && *source == bolt
        ));
        let mut outcomes = [(0, 0); 2];
        for (option, outcome) in outcomes.iter_mut().enumerate() {
            let mut branch = restored(&state);
            assert_eq!(settle(&mut branch), choice);
            act(&mut branch, Action::ChooseEffectOption(option as u16));
            settled(&mut branch);

            let creature = branch.objects.get(victim);
            assert_eq!((creature.zone, creature.damage), (Zone::Battlefield, 0));
            assert_eq!(life(&branch, victim_controller), 20);
            assert_eq!(branch.objects.get(bolt).zone, Zone::Graveyard);
            *outcome = (life(&branch, caster), creature.counters.minus1_minus1);
        }
        outcomes.sort_unstable();
        assert_eq!(outcomes, [(20, 3), (23, 0)]);
    }
}

#[test]
fn soul_scar_mage_and_protection_offer_a_pyroclasm_batch_choice_both_seats() {
    for caster in [P0, P1] {
        let victim_controller = caster.opponent();
        let mut state = turn_of(caster);
        let mage = put(&mut state, caster, "Soul-Scar Mage", Zone::Battlefield);
        let victim = put(
            &mut state,
            victim_controller,
            "Gurmag Angler",
            Zone::Battlefield,
        );
        let mask = put(&mut state, caster, "Mask of Law and Grace", Zone::Hand);
        pool(&mut state, caster, &[ManaColor::W], 0);
        cast(&mut state, mask, &[Target::Object(victim)]);
        settled(&mut state);
        assert_eq!(
            state.objects.get(mask).v4.attached_to.unwrap().object,
            victim
        );

        let pyroclasm = put(&mut state, caster, "Pyroclasm", Zone::Hand);
        pool(&mut state, caster, &[ManaColor::R], 1);
        cast(&mut state, pyroclasm, &[]);

        let choice = settle(&mut state);
        assert!(matches!(
            &choice,
            Some(Decision::ChooseEffectOption { player, source, option_count: 2 })
                if *player == victim_controller && *source == pyroclasm
        ));
        let mut counters = [0; 2];
        for (option, count) in counters.iter_mut().enumerate() {
            let mut branch = restored(&state);
            assert_eq!(settle(&mut branch), choice);
            act(&mut branch, Action::ChooseEffectOption(option as u16));
            settled(&mut branch);

            let creature = branch.objects.get(victim);
            assert_eq!((creature.zone, creature.damage), (Zone::Battlefield, 0));
            assert_eq!(branch.objects.get(pyroclasm).zone, Zone::Graveyard);
            *count = creature.counters.minus1_minus1;
            let mage = branch.objects.get(mage);
            assert_eq!(
                (mage.zone, mage.damage, mage.counters.minus1_minus1),
                (Zone::Battlefield, 2, 0)
            );
        }
        counters.sort_unstable();
        assert_eq!(counters, [0, 2]);
    }
}

#[test]
fn soul_scar_hallow_batch_resumes_the_meddling_mage_entry_after_restore() {
    use mtg_kernel::event::{self, ProposedEvent};

    let mut state = turn_of(P0);
    put(&mut state, P0, "Soul-Scar Mage", Zone::Battlefield);
    let victim = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let entrant = put(&mut state, P0, "Meddling Mage", Zone::Graveyard);
    let fanatic = put(&mut state, P0, "Mogg Fanatic", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, fanatic, &[]);
    protect(&mut state, fanatic);
    settled(&mut state);
    assert_eq!(state.objects.get(fanatic).zone, Zone::Battlefield);

    event::propose_and_commit_batch(
        &mut state,
        vec![
            ProposedEvent::damage(fanatic, Target::Object(victim), 1),
            ProposedEvent::zone_change(entrant, Zone::Battlefield),
        ],
    );
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectOption { player: P1, source, option_count: 2 })
            if source == fanatic
    ));

    let mut outcomes = [(0, 0); 2];
    for (option, outcome) in outcomes.iter_mut().enumerate() {
        let mut live = state.clone();
        act(&mut live, Action::ChooseEffectOption(option as u16));
        let entry_choice = settle(&mut live);
        let Some(Decision::ChooseMeddlingMageNameV1 {
            player: P0,
            group: None,
            options,
            ..
        }) = &entry_choice
        else {
            panic!("expected Meddling Mage's entry choice after damage, got {entry_choice:?}");
        };
        assert_eq!(live.objects.get(entrant).zone, Zone::Graveyard);
        let group = options.iter().position(|name| name == "L").unwrap() as u16;

        let mut saved = restored(&live);
        assert_eq!(settle(&mut saved), entry_choice);
        for branch in [&mut live, &mut saved] {
            act(branch, Action::ChooseEffectOption(group));
            let Some(Decision::ChooseMeddlingMageNameV1 {
                player: P0,
                group: Some(selected),
                options,
                ..
            }) = settle(branch)
            else {
                panic!("expected the names in Meddling Mage's chosen group");
            };
            assert_eq!(selected, group);
            let name = options
                .iter()
                .position(|name| name == "Lightning Bolt")
                .unwrap();
            act(branch, Action::ChooseEffectOption(name as u16));
            settled(branch);
            assert_eq!(branch.objects.get(entrant).zone, Zone::Battlefield);
            let creature = branch.objects.get(victim);
            assert_eq!((creature.zone, creature.damage), (Zone::Battlefield, 0));
        }
        assert_eq!(saved.state_hash(), live.state_hash());
        *outcome = (
            life(&live, P0),
            live.objects.get(victim).counters.minus1_minus1,
        );
    }
    outcomes.sort_unstable();
    assert_eq!(outcomes, [(20, 1), (21, 0)]);
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
fn impractical_joke_leaves_soul_scar_and_nishoba_order_choices() {
    let mut state = turn_of(P1);
    put(&mut state, P1, "Soul-Scar Mage", Zone::Battlefield);
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

    let choice = settle(&mut state);
    assert!(matches!(
        &choice,
        Some(Decision::ChooseEffectOption { player: P0, source, option_count: 2 })
            if *source == spell
    ));
    let mut remaining_counters = [0; 2];
    for (option, remaining) in remaining_counters.iter_mut().enumerate() {
        let mut branch = restored(&state);
        assert_eq!(settle(&mut branch), choice);
        act(&mut branch, Action::ChooseEffectOption(option as u16));
        settled(&mut branch);

        let phantom = branch.objects.get(phantom);
        assert_eq!((phantom.zone, phantom.damage), (Zone::Battlefield, 0));
        assert_eq!(phantom.counters.minus1_minus1, 0);
        assert_eq!(life(&branch, P0), 20);
        *remaining = phantom.counters.plus1_plus1;
    }
    // The spell's three counters cancel three of Nishoba's. Choosing its
    // own effect first also spends one even though prevention is prohibited.
    remaining_counters.sort_unstable();
    assert_eq!(remaining_counters, [3, 4]);
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
