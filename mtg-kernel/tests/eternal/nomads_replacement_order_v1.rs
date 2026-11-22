//! The affected controller orders established Nomads en-Kor redirections.
use super::prevention_order_no_hallow_v1::choose_public;
use super::{
    activate, cast, give_priority, pool, put, settle, settled, turn_of, Decision, GameState,
    ManaColor, ObjectId, Target, Zone, P0, P1,
};
use mtg_kernel::ids::PlayerId;

fn shields(state: &mut GameState, owner: PlayerId, nomads: ObjectId, recipients: &[ObjectId]) {
    for &target in recipients {
        give_priority(state, owner);
        activate(state, nomads, 0, &[Target::Object(target)]);
        settled(state);
    }
}

fn burn(state: &mut GameState, player: PlayerId, name: &str, nomads: ObjectId) -> ObjectId {
    give_priority(state, player);
    let spell = put(state, player, name, Zone::Hand);
    pool(state, player, &[ManaColor::R], 0);
    cast(state, spell, &[Target::Object(nomads)]);
    spell
}

#[test]
fn two_redirects_offer_the_affected_controller_either_recipient() {
    for owner in [P0, P1] {
        for choice in [0u16, 1] {
            let mut state = turn_of(owner.opponent());
            let nomads = put(&mut state, owner, "Nomads en-Kor", Zone::Battlefield);
            let a = put(&mut state, owner, "Wild Mongrel", Zone::Battlefield);
            let b = put(&mut state, owner, "Kor Skyfisher", Zone::Battlefield);
            shields(&mut state, owner, nomads, &[a, b]);
            let spell = burn(&mut state, owner.opponent(), "Lava Dart", nomads);
            assert!(
                matches!(settle(&mut state), Some(Decision::ChooseEffectOption {
                player, source, option_count: 2,
            }) if player == owner && source == spell)
            );
            choose_public(&mut state, owner, spell, choice, 2);
            settled(&mut state);
            let (first, other) = if choice == 0 { (a, b) } else { (b, a) };
            assert_eq!(state.objects.get(first).damage, 1);
            assert_eq!(state.objects.get(other).damage, 0);
            burn(&mut state, owner.opponent(), "Lava Dart", nomads);
            settled(&mut state);
            assert_eq!(state.objects.get(other).damage, 1);
            assert_eq!(state.objects.get(nomads).damage, 0);
        }
    }
}

#[test]
fn multipoint_bolt_uses_both_redirects_and_deals_the_remaining_point() {
    let mut state = turn_of(P1);
    let nomads = put(&mut state, P0, "Nomads en-Kor", Zone::Battlefield);
    let a = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    let b = put(&mut state, P0, "Kor Skyfisher", Zone::Battlefield);
    shields(&mut state, P0, nomads, &[a, b]);
    burn(&mut state, P1, "Lightning Bolt", nomads);
    settled(&mut state);
    assert_eq!(state.objects.get(a).damage, 1);
    assert_eq!(state.objects.get(b).damage, 1);
    assert_eq!(state.objects.get(nomads).zone, Zone::Graveyard);
}

#[test]
fn redirect_can_be_chosen_for_unpreventable_damage() {
    let mut state = turn_of(P1);
    let nomads = put(&mut state, P0, "Nomads en-Kor", Zone::Battlefield);
    let a = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    let b = put(&mut state, P0, "Kor Skyfisher", Zone::Battlefield);
    shields(&mut state, P0, nomads, &[a, b]);
    give_priority(&mut state, P1);
    let pain = put(&mut state, P1, "Flaring Pain", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 1);
    cast(&mut state, pain, &[]);
    settled(&mut state);
    let spell = burn(&mut state, P1, "Lava Dart", nomads);
    assert!(
        matches!(settle(&mut state), Some(Decision::ChooseEffectOption {
        player: P0, source, option_count: 2
    }) if source == spell)
    );
    choose_public(&mut state, P0, spell, 1, 2);
    settled(&mut state);
    assert_eq!(state.objects.get(b).damage, 1);
    assert_eq!(state.objects.get(a).damage, 0);
}

#[test]
fn pyroclasm_allows_protection_to_preserve_or_spend_a_redirect() {
    let mut state = turn_of(P0);
    let nomads = put(&mut state, P0, "Nomads en-Kor", Zone::Battlefield);
    let ally = put(&mut state, P0, "Kor Skyfisher", Zone::Battlefield);
    let mask = put(&mut state, P0, "Mask of Law and Grace", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, mask, &[Target::Object(nomads)]);
    settled(&mut state);
    shields(&mut state, P0, nomads, &[ally]);
    let pyro = put(&mut state, P0, "Pyroclasm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, pyro, &[]);
    assert!(
        matches!(settle(&mut state), Some(Decision::ChooseEffectOption {player: P0, source, option_count: 2}) if source == pyro)
    );
    for choice in [0, 1] {
        let mut branch = super::restored(&state);
        choose_public(&mut branch, P0, pyro, choice, 2);
        settled(&mut branch);
        assert_eq!(branch.objects.get(nomads).damage, 0);
        assert_eq!(
            branch.eternal_v1.as_ref().unwrap().e9a.redirects.len(),
            usize::from(choice)
        );
        assert_eq!(
            branch.objects.get(ally).zone,
            if choice == 0 {
                Zone::Graveyard
            } else {
                Zone::Battlefield
            }
        );
    }
}

#[test]
fn unpreventable_redirected_points_remove_one_nishoba_counter_per_event() {
    let mut state = turn_of(P0);
    let nomads = put(&mut state, P0, "Nomads en-Kor", Zone::Battlefield);
    let nishoba = put(&mut state, P0, "Phantom Nishoba", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::W], 5);
    cast(&mut state, nishoba, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(nishoba).counters.plus1_plus1, 7);
    shields(&mut state, P0, nomads, &[nishoba, nishoba]);
    let pain = put(&mut state, P0, "Flaring Pain", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, pain, &[]);
    settled(&mut state);
    burn(&mut state, P0, "Lightning Bolt", nomads);
    settled(&mut state);
    let creature = state.objects.get(nishoba);
    assert_eq!((creature.damage, creature.counters.plus1_plus1), (2, 6));
    assert_eq!(state.objects.get(nomads).zone, Zone::Graveyard);
}

#[test]
fn one_redirect_splits_lavamancer_damage_but_lifelink_gains_life_once() {
    let mut state = turn_of(P0);
    let lavamancer = put(&mut state, P0, "Grim Lavamancer", Zone::Battlefield);
    let aura = put(&mut state, P0, "Lifelink", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, aura, &[Target::Object(lavamancer)]);
    settled(&mut state);
    let nomads = put(&mut state, P1, "Nomads en-Kor", Zone::Battlefield);
    let ally = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    shields(&mut state, P1, nomads, &[ally]);
    give_priority(&mut state, P0);
    for _ in 0..2 {
        put(&mut state, P0, "Mountain", Zone::Graveyard);
    }
    pool(&mut state, P0, &[ManaColor::R], 0);
    activate(&mut state, lavamancer, 0, &[Target::Object(nomads)]);
    settled(&mut state);
    assert_eq!(state.objects.get(nomads).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(ally).damage, 1);
    assert_eq!(super::life(&state, P0), 22);
    let gains = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            mtg_kernel::event::CommittedEvent::LifeGain { player: P0, amount } => Some(*amount),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(gains, [2]);
}
