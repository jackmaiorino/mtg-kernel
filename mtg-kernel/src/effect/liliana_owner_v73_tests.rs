//! Genuine six-mana printing payment and optional genuine Season prerequisite.
use super::*;
pub(crate) fn owning_ready(p: PlayerId, season: bool, hidden: &str) -> (GameState, ObjectId) {
    let mut s = ready(p, hidden);
    if season {
        pay(&mut s, p, "Doubling Season", &[], 5);
        finish(&mut s);
    }
    for player in [p, p.opponent()] {
        for name in [
            "Llanowar Elves",
            "Healer's Hawk",
            "Savannah Lions",
            "Burnished Hart",
            "Goldvein Pick",
            "Paradise Druid",
        ] {
            put(&mut s, player, name, Zone::Battlefield);
        }
    }
    let walker = walker(&mut s, p);
    priority(&mut s, p);
    (s, walker)
}

pub(crate) fn published_source_boundary(
    p: PlayerId,
    concealed: bool,
) -> (GameState, ObjectId, u32) {
    let (mut s, id) = owning_ready(p, false, "Island");
    let original = s.objects.get(id).zone_change_count;
    activate(&mut s, p, id, 0);
    // Explicit native source boundary after authentic paid publication.
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
    (s, id, original)
}

/// Legal initial hidden-slot operands; all Liliana payment/history is then native.
pub(crate) fn sampler_ready(p: PlayerId, hidden: &str) -> (GameState, ObjectId) {
    let library = (0..128)
        .map(|i| ["Forest", "Island", "Mountain", "Plains"][i % 4])
        .collect::<Vec<_>>();
    let mut s = crate::effect::vivien_reid_v1::tests::ready(p, &library, &[], hidden);
    for player in [p, p.opponent()] {
        for name in [
            "Llanowar Elves",
            "Healer's Hawk",
            "Savannah Lions",
            "Burnished Hart",
            "Goldvein Pick",
            "Paradise Druid",
        ] {
            put(&mut s, player, name, Zone::Battlefield);
        }
    }
    let id = walker(&mut s, p);
    priority(&mut s, p);
    (s, id)
}
