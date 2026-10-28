//! Genuine paid departure/reentry and control transitions, not reordered operands.
use super::super::*;
use crate::{policy_surface_v5::PolicySurfaceV5, tinybones_observation_v72 as observation};

fn complete(s: &mut GameState) -> observation::TinybonesObservationV72 {
    let mut surface = PolicySurfaceV5::new_with_engine_priority_v1();
    let origin = surface.next_decision(s).unwrap();
    let actor = origin.actor(s).unwrap();
    let before = s.clone();
    let value =
        observation::observe_v72(s, &surface, (actor, &origin), 0, 0, origin.substep()).unwrap();
    observation::validate_current(
        s,
        &surface,
        (actor, &origin),
        0,
        0,
        origin.substep(),
        &value,
    )
    .unwrap();
    assert_eq!(*s, before);
    for seat in [PlayerId::P0, PlayerId::P1] {
        assert_eq!(
            value.tinybones.battlefield[seat.index()]
                .iter()
                .map(|r| r.card.object)
                .collect::<Vec<_>>(),
            s.players[seat.index()]
                .battlefield
                .iter()
                .map(|id| id.0)
                .collect::<Vec<_>>()
        );
    }
    value
}

#[test]
fn paid_fake_death_reentry_and_employment_preserve_native_battlefield_order_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let tiny = put(&mut s, p, NAME, Zone::Hand);
        let elf = put(&mut s, p, "Llanowar Elves", Zone::Hand);
        let fake = put(&mut s, p, "Fake Your Own Death", Zone::Hand);
        let bolt = put(&mut s, p.opponent(), "Lightning Bolt", Zone::Hand);
        let employment = put(&mut s, p.opponent(), "Involuntary Employment", Zone::Hand);
        paid(&mut s, p, tiny);
        let paid_elf = cast(&mut s, p, elf, None);
        assert_eq!(paid_elf.controller, p);
        finish(&mut s);
        assert_eq!(s.objects.get(elf).zone, Zone::Battlefield);
        let original_generation = s.objects.get(tiny).zone_change_count;
        cast(&mut s, p, fake, Some(Target::Object(tiny)));
        finish(&mut s);
        cast(&mut s, p.opponent(), bolt, Some(Target::Object(tiny)));
        finish(&mut s);
        assert_eq!(s.objects.get(tiny).zone, Zone::Battlefield);
        assert_eq!(
            s.objects.get(tiny).zone_change_count,
            original_generation + 2
        );
        let arena = s
            .objects
            .iter()
            .filter(|(_, o)| o.zone == Zone::Battlefield && o.controller == p)
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
        assert_ne!(arena, s.players[p.index()].battlefield);
        assert!(
            s.players[p.index()]
                .battlefield
                .iter()
                .position(|id| *id == elf)
                .unwrap()
                < s.players[p.index()]
                    .battlefield
                    .iter()
                    .position(|id| *id == tiny)
                    .unwrap()
        );
        let before = complete(&mut s);
        let mut restored = restore(&s);
        assert_eq!(complete(&mut restored), before);
        turn(&mut s, p.opponent());
        let paid_control = cast(&mut s, p.opponent(), employment, Some(Target::Object(tiny)));
        assert_eq!(paid_control.controller, p.opponent());
        finish(&mut s);
        assert_eq!(s.objects.get(tiny).owner, p);
        assert_eq!(s.objects.get(tiny).controller, p.opponent());
        let controlled = complete(&mut s);
        assert!(controlled.tinybones.battlefield[p.opponent().index()]
            .iter()
            .any(|r| r.card.object == tiny.0
                && r.card.owner == p.into()
                && r.card.controller == p.opponent().into()));
        let mut restored = restore(&s);
        assert_eq!(complete(&mut restored), controlled);
        let mut duplicate = s.clone();
        duplicate.players[p.opponent().index()]
            .battlefield
            .push(tiny);
        let unchanged = duplicate.clone();
        assert!(crate::tinybones_public_v72::capture(&duplicate, p).is_err());
        assert_eq!(duplicate, unchanged);
        let mut erased = s.clone();
        erased.players[p.opponent().index()]
            .battlefield
            .retain(|id| *id != tiny);
        let unchanged = erased.clone();
        assert!(crate::tinybones_public_v72::capture(&erased, p).is_err());
        assert_eq!(erased, unchanged);
    }
}
