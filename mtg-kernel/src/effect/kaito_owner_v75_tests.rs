//! Genuine three-mana Kaito and paid creature, independent initial hidden worlds.
use super::*;
pub(crate) fn owning_ready(
    p: PlayerId,
    season: bool,
    hidden: &str,
) -> (GameState, ObjectId, ObjectId) {
    let mut s = crate::effect::vivien_reid_v1::tests::ready(
        p,
        &["Forest"; 64],
        &["Forest", "Island"],
        hidden,
    );
    if season {
        cast(&mut s, p, "Doubling Season", &[], 5);
        finish(&mut s);
    }
    let k = cast(&mut s, p, NAME, &[], 3);
    finish(&mut s);
    assert_eq!(s.objects.get(k).zone, Zone::Battlefield);
    assert!(definition(s.objects.get(k).card_def));
    assert_eq!(
        crate::planeswalker_v1::loyalty(&s, k),
        Some(if season { 6 } else { 3 })
    );
    let c = cast(&mut s, p, "Axgard Cavalry", &[], 2);
    finish(&mut s);
    priority(&mut s, p);
    (s, k, c)
}
pub(crate) fn prepare_ultimate(s: &mut GameState, k: ObjectId) {
    // Exact native checked counter-placement setup, not a paid +1 sequence claim.
    let old = crate::planeswalker_v1::loyalty(s, k).unwrap();
    if old < 9 {
        crate::planeswalker_v1::add_loyalty_counters_checked_v1(s, k, 9 - old).unwrap();
    }
}
pub(crate) fn published_source_boundary(
    p: PlayerId,
    concealed: bool,
) -> (GameState, ObjectId, u32) {
    let (mut s, k, _) = owning_ready(p, false, "Island");
    let generation = s.objects.get(k).zone_change_count;
    activate(&mut s, p, k, 0, None);
    // Labelled native source-zone/concealment boundary after actual paid publication.
    if concealed {
        s.objects.get_mut(k).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: false,
            lookers: 0,
            hidden_by: None,
            hidden_by_source: None,
        });
    } else {
        event::propose_and_commit(&mut s, event::ProposedEvent::zone_change(k, Zone::Hand));
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::zone_change_to_battlefield_under_controller(k, p.opponent()),
        );
    }
    (s, k, generation)
}
pub(crate) fn combat_boundary(p: PlayerId, departing: bool) -> GameState {
    let (mut s, k, c) = owning_ready(p, false, "Island");
    next_own_main(&mut s, p);
    if departing {
        // Existing native final-combat-wave primitive with simultaneous source departure.
        let mut damage = event::ProposedEvent::damage(c, Target::Player(p.opponent()), 2);
        if let event::ProposedEvent::Damage(d) = &mut damage {
            d.is_combat = true;
        }
        engine::commit_combat_damage_events(
            &mut s,
            vec![
                damage,
                event::ProposedEvent::zone_change(k, Zone::Graveyard),
            ],
        );
    } else {
        combat_trigger(&mut s, p, c);
    }
    s
}
pub(crate) fn emblem_cast_boundary(p: PlayerId) -> GameState {
    let (mut s, k, _) = owning_ready(p, false, "Island");
    ultimate(&mut s, p, k);
    let spell = cast(&mut s, p, "Think Twice", &[], 2);
    // Real native virtual-copy ancestry, explicitly not a paid copy-spell issuer.
    let (_, copy) = engine::copy_spell_for(&mut s, spell, p).unwrap();
    assert!(s
        .stack
        .iter()
        .any(|r| r.v4.stack_item_id == copy && r.is_copy));
    cast(
        &mut s,
        p.opponent(),
        "Counterspell",
        &[Target::Object(spell)],
        2,
    );
    s
}
