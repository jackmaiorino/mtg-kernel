//! Real seven-mana Chandra, independent initialized hidden worlds and actual paid Season.
use super::*;
pub(crate) fn owning_ready(
    p: PlayerId,
    season: bool,
    hidden: &str,
) -> (GameState, ObjectId, ObjectId, ObjectId) {
    let mut s = crate::effect::vivien_reid_v1::tests::ready(
        p,
        &[
            "Forest",
            "Think Twice",
            "Mountain",
            "Island",
            "Forest",
            "Mountain",
            "Island",
            "Forest",
        ],
        &[],
        hidden,
    );
    if season {
        cast(&mut s, p, "Doubling Season", &[], 5);
        finish(&mut s);
    }
    let ours = put(&mut s, p, "Llanowar Elves", Zone::Battlefield);
    let theirs = put(&mut s, p.opponent(), "Axgard Cavalry", Zone::Battlefield);
    let id = walker(&mut s, p);
    priority(&mut s, p);
    (s, id, ours, theirs)
}
pub(crate) fn published_source_boundary(
    p: PlayerId,
    concealed: bool,
) -> (GameState, ObjectId, u32) {
    let (mut s, id, _, _) = owning_ready(p, false, "Island");
    let generation = s.objects.get(id).zone_change_count;
    activate(&mut s, p, id, 0, &[]);
    // Labelled native source boundary after a genuine paid publication.
    if concealed {
        s.objects.get_mut(id).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: false,
            lookers: 0,
            hidden_by: None,
            hidden_by_source: None,
        });
    } else {
        event::propose_and_commit(&mut s, event::ProposedEvent::zone_change(id, Zone::Hand));
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::zone_change_to_battlefield_under_controller(id, p.opponent()),
        );
    }
    (s, id, generation)
}

pub(crate) fn pending_damage_boundary(p: PlayerId) -> GameState {
    let mut s = ready(p);
    cast(&mut s, p, "Twinflame Tyrant", &[], 5);
    finish(&mut s);
    let c = walker(&mut s, p);
    let target = put(&mut s, p.opponent(), "Shivan Dragon", Zone::Battlefield);
    // Native next-damage shield primitive on genuine paid Tyrant/Chandra.
    s.engine.active_replacements.push(event::ActiveReplacement {
        id: 90494,
        source: target,
        kind: event::ReplacementEffectKind::PreventNextDamage {
            target: Target::Object(target),
            remaining: 3,
        },
    });
    activate(&mut s, p, c, 2, &[Target::Object(target); 8]);
    for _ in 0..40 {
        let d = next(&mut s);
        if matches!(d, Decision::ChooseEffectOption { .. }) {
            return s;
        }
        pass(&mut s, d);
    }
    panic!("genuine Chandra damage replacement menu absent")
}
