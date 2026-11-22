//! Observable paid-Circle consumption without Hallow.
use super::prevention_order_no_hallow_v1::choose_public;
use super::{
    activate_paying, cast, decoy, give_priority, life, link, pool, put, restored, settle, settled,
    shield, shields, turn_of, CircleColorV1, Decision, GameState, ManaColor, ObjectId, Target,
    Zone, BLUE_CIRCLE, P0, P1, RED_CIRCLE,
};
use mtg_kernel::ids::PlayerId;

fn menu(state: &mut GameState, actor: PlayerId, source: ObjectId, count: u16) {
    let decision = settle(state);
    assert!(
        matches!(decision, Some(Decision::ChooseEffectOption {
        player, source: actual, option_count,
    }) if player == actor && actual == source && option_count == count),
        "{decision:?}"
    );
}

fn spell(
    state: &mut GameState,
    actor: PlayerId,
    name: &str,
    colors: &[ManaColor],
    generic: u8,
    target: Target,
) {
    give_priority(state, actor);
    let source = put(state, actor, name, Zone::Hand);
    pool(state, actor, colors, generic);
    cast(state, source, &[target]);
}

fn protected_pillar(actor: PlayerId, name: &str) -> (GameState, ObjectId, ObjectId) {
    let mut state = turn_of(actor);
    let pillar = put(
        &mut state,
        actor.opponent(),
        "Pyrostatic Pillar",
        Zone::Battlefield,
    );
    let circle = put(&mut state, actor, RED_CIRCLE, Zone::Battlefield);
    decoy(&mut state, actor.opponent(), CircleColorV1::Red);
    let protection = if name == "The One Ring" {
        let ring = put(&mut state, actor, name, Zone::Hand);
        pool(&mut state, actor, &[], 4);
        cast(&mut state, ring, &[]);
        settled(&mut state);
        ring
    } else {
        put(&mut state, actor, name, Zone::Battlefield)
    };
    let source = link(&state, pillar);
    shield(&mut state, actor, circle, source);
    (state, pillar, protection)
}

#[test]
fn confinement_can_save_the_exact_circle_for_damage_after_it_leaves_both_seats() {
    for actor in [P0, P1] {
        let (mut start, pillar, confinement) = protected_pillar(actor, "Solitary Confinement");
        spell(
            &mut start,
            actor,
            "Disenchant",
            &[ManaColor::W],
            1,
            Target::Object(confinement),
        );
        menu(&mut start, actor, pillar, 2);
        for option in 0..2 {
            let mut state = restored(&start);
            choose_public(&mut state, actor, pillar, option, 2);
            settled(&mut state);
            assert_eq!(state.objects.get(confinement).zone, Zone::Graveyard);
            assert_eq!(life(&state, actor), 20);
            assert_eq!(shields(&state), usize::from(option == 1));
            spell(
                &mut state,
                actor,
                "Lightning Bolt",
                &[ManaColor::R],
                0,
                Target::Player(actor.opponent()),
            );
            settled(&mut state);
            assert_eq!(life(&state, actor), if option == 1 { 20 } else { 18 });
            assert_eq!(life(&state, actor.opponent()), 17);
            assert_eq!(shields(&state), 0);
        }
    }
}

#[test]
fn ring_protection_persists_after_a_bounce_and_the_saved_circle_is_offered_again() {
    for actor in [P0, P1] {
        let (mut start, pillar, ring) = protected_pillar(actor, "The One Ring");
        // The Ring is indestructible; bounce it with a real cheap spell.
        spell(
            &mut start,
            actor,
            "Echoing Truth",
            &[ManaColor::U],
            1,
            Target::Object(ring),
        );
        menu(&mut start, actor, pillar, 2);
        for option in 0..2 {
            let mut state = restored(&start);
            choose_public(&mut state, actor, pillar, option, 2);
            settled(&mut state);
            assert_eq!(state.objects.get(ring).zone, Zone::Hand);
            assert!(mtg_kernel::engine::damage_is_prevented_by_protection(
                &state,
                pillar,
                Target::Player(actor)
            ));
            spell(
                &mut state,
                actor,
                "Lightning Bolt",
                &[ManaColor::R],
                0,
                Target::Player(actor.opponent()),
            );
            if option == 1 {
                menu(&mut state, actor, pillar, 2);
                choose_public(&mut state, actor, pillar, 0, 2);
            }
            settled(&mut state);
            assert_eq!(life(&state, actor), 20);
            assert_eq!(life(&state, actor.opponent()), 17);
            assert_eq!(shields(&state), 0);
        }
    }
}

#[test]
fn a_sphere_can_save_the_circle_for_a_later_packet_after_the_sphere_leaves() {
    for actor in [P0, P1] {
        let attacker = actor.opponent();
        let mut start = turn_of(attacker);
        let source = put(
            &mut start,
            attacker,
            "Goblin Bombardment",
            Zone::Battlefield,
        );
        let sphere = put(&mut start, actor, "Sphere of Law", Zone::Battlefield);
        let circle = put(&mut start, actor, RED_CIRCLE, Zone::Battlefield);
        let first = put(&mut start, attacker, "Wild Mongrel", Zone::Battlefield);
        let second = put(&mut start, attacker, "Wild Mongrel", Zone::Battlefield);
        decoy(&mut start, attacker, CircleColorV1::Red);
        let binding = link(&start, source);
        shield(&mut start, actor, circle, binding);
        give_priority(&mut start, attacker);
        activate_paying(&mut start, source, 0, &[Target::Player(actor)], Some(first));
        menu(&mut start, actor, source, 2);
        for option in 0..2 {
            let mut state = restored(&start);
            choose_public(&mut state, actor, source, option, 2);
            settled(&mut state);
            assert_eq!(shields(&state), usize::from(option == 1));
            spell(
                &mut state,
                actor,
                "Disenchant",
                &[ManaColor::W],
                1,
                Target::Object(sphere),
            );
            settled(&mut state);
            assert_eq!(state.objects.get(sphere).zone, Zone::Graveyard);
            give_priority(&mut state, attacker);
            activate_paying(
                &mut state,
                source,
                0,
                &[Target::Player(actor)],
                Some(second),
            );
            settled(&mut state);
            assert_eq!(life(&state, actor), if option == 1 { 20 } else { 19 });
            assert_eq!(shields(&state), 0);
        }
    }
}

#[test]
fn distinct_paid_colors_are_independent_after_painter_is_sacrificed() {
    for attacker in [P0, P1] {
        let actor = attacker.opponent();
        let mut start = turn_of(attacker);
        let red = put(&mut start, actor, RED_CIRCLE, Zone::Battlefield);
        let blue = put(&mut start, actor, BLUE_CIRCLE, Zone::Battlefield);
        let spare = decoy(&mut start, attacker, CircleColorV1::Red).object;
        let painter = put(&mut start, attacker, "Painter's Servant", Zone::Hand);
        pool(&mut start, attacker, &[], 2);
        cast(&mut start, painter, &[]);
        super::settle_option(&mut start, 1);
        let source = put(&mut start, attacker, "Goblin Bombardment", Zone::Hand);
        pool(&mut start, attacker, &[ManaColor::R], 1);
        cast(&mut start, source, &[]);
        settled(&mut start);
        let binding = link(&start, source);
        for circle in [red, blue, blue] {
            shield(&mut start, actor, circle, binding);
        }
        give_priority(&mut start, attacker);
        activate_paying(&mut start, source, 0, &[Target::Player(actor)], Some(spare));
        // Equal blue records are coalesced, but red and blue remain distinct.
        menu(&mut start, actor, source, 2);
        for option in 0..2 {
            let mut state = restored(&start);
            choose_public(&mut state, actor, source, option, 2);
            settled(&mut state);
            assert_eq!(shields(&state), 2);
            give_priority(&mut state, attacker);
            activate_paying(
                &mut state,
                source,
                0,
                &[Target::Player(actor)],
                Some(painter),
            );
            settled(&mut state);
            assert_eq!(state.objects.get(painter).zone, Zone::Graveyard);
            assert_eq!(life(&state, actor), if option == 0 { 19 } else { 20 });
            assert_eq!(shields(&state), if option == 0 { 2 } else { 1 });
        }
    }
}

#[test]
fn an_inevitable_single_circle_does_not_offer_irrelevant_numeric_orders() {
    for attacker in [P0, P1] {
        let actor = attacker.opponent();
        let mut state = turn_of(attacker);
        put(&mut state, actor, "Sphere of Law", Zone::Battlefield);
        put(
            &mut state,
            attacker,
            "Ojer Axonil, Deepest Might",
            Zone::Battlefield,
        );
        let circle = put(&mut state, actor, RED_CIRCLE, Zone::Battlefield);
        decoy(&mut state, attacker, CircleColorV1::Red);
        let source = put(&mut state, attacker, "Lightning Bolt", Zone::Hand);
        pool(&mut state, attacker, &[ManaColor::R], 0);
        cast(&mut state, source, &[Target::Player(actor)]);
        let binding = link(&state, source);
        shield(&mut state, actor, circle, binding);
        settled(&mut state);
        assert_eq!(life(&state, actor), 20);
        assert_eq!(shields(&state), 0);
        assert!(state.engine.linked_aura_operation_v1.is_empty());
    }
}

#[test]
fn a_full_or_partial_registered_shield_can_be_consumed_before_the_circle() {
    use mtg_kernel::event::{ActiveReplacement, ReplacementEffectKind};
    for initial in [1, 3] {
        let actor = P0;
        let mut start = turn_of(actor);
        let pillar = put(&mut start, P1, "Pyrostatic Pillar", Zone::Battlefield);
        let circle = put(&mut start, actor, RED_CIRCLE, Zone::Battlefield);
        decoy(&mut start, P1, CircleColorV1::Red);
        let source = link(&start, pillar);
        shield(&mut start, actor, circle, source);
        // Existing private representation; no printed card producer is claimed.
        start.engine.active_replacements.push(ActiveReplacement {
            id: 900,
            source: pillar,
            kind: ReplacementEffectKind::PreventNextDamage {
                target: Target::Player(actor),
                remaining: initial,
            },
        });
        spell(
            &mut start,
            actor,
            "Lightning Bolt",
            &[ManaColor::R],
            0,
            Target::Player(P1),
        );
        menu(&mut start, actor, pillar, 2);
        for option in 0..2 {
            let mut state = restored(&start);
            choose_public(&mut state, actor, pillar, option, 2);
            settled(&mut state);
            assert_eq!(life(&state, actor), 20);
            assert_eq!(shields(&state), usize::from(option == 1 && initial >= 2));
            let remaining = state
                .engine
                .active_replacements
                .iter()
                .find(|replacement| replacement.id == 900)
                .map(|replacement| match replacement.kind {
                    ReplacementEffectKind::PreventNextDamage { remaining, .. } => remaining,
                    _ => panic!("wrong registered shield"),
                })
                .unwrap_or(0);
            assert_eq!(
                remaining,
                if option == 0 {
                    initial
                } else {
                    (initial - 2).max(0)
                }
            );
        }
    }
}

#[test]
fn several_spheres_can_save_a_circle_without_an_equivalent_first_menu_entry() {
    for attacker in [P0, P1] {
        let actor = attacker.opponent();
        let mut start = turn_of(attacker);
        for _ in 0..2 {
            put(&mut start, actor, "Sphere of Law", Zone::Battlefield);
        }
        let circle = put(&mut start, actor, RED_CIRCLE, Zone::Battlefield);
        decoy(&mut start, attacker, CircleColorV1::Red);
        let source = put(&mut start, attacker, "Lightning Bolt", Zone::Hand);
        pool(&mut start, attacker, &[ManaColor::R], 0);
        cast(&mut start, source, &[Target::Player(actor)]);
        let binding = link(&start, source);
        shield(&mut start, actor, circle, binding);
        menu(&mut start, actor, source, 2);
        for option in 0..2 {
            let mut state = restored(&start);
            choose_public(&mut state, actor, source, option, 2);
            if option == 1 {
                menu(&mut state, actor, source, 2);
                choose_public(&mut state, actor, source, 1, 2);
            }
            settled(&mut state);
            assert_eq!(life(&state, actor), 20);
            assert_eq!(shields(&state), usize::from(option == 1));
        }
    }
}
