//! Initial lands/hand/library are labelled native fixture setup. All casts,
//! death and source-removal responses below use the actual paid native engine.
use crate::{
    card_def,
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    state::{GameState, Zone},
};
pub(crate) fn ready(actor: PlayerId, hidden: &str) -> GameState {
    let id = |name: &str| card_def::card_id_by_name(name).unwrap();
    let known = [id("Forest"); 80];
    let concealed = [id(hidden); 80];
    let (a, b) = if actor == PlayerId::P0 {
        (&known, &concealed)
    } else {
        (&concealed, &known)
    };
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        a,
        b,
        crate::rl::card_name,
        110_459,
        actor,
    );
    s.step = crate::state::Step::Main1;
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut s).unwrap();
    for seat in [actor, actor.opponent()] {
        for _ in 0..8 {
            for name in ["Plains", "Island", "Swamp", "Mountain", "Forest"] {
                put(&mut s, seat, name, Zone::Battlefield);
            }
        }
    }
    s
}
pub(crate) fn hand(s: &mut GameState, p: PlayerId, name: &str) -> ObjectId {
    put(s, p, name, Zone::Hand)
}
pub(crate) fn paid_scene(actor: PlayerId, hidden: &str) -> (GameState, ObjectId, ObjectId) {
    let mut s = ready(actor, hidden);
    let call =
        crate::graveyard_return_public_v61::tests::paid(&mut s, actor, "Valkyrie's Call", None);
    let elf =
        crate::graveyard_return_public_v61::tests::paid(&mut s, actor, "Llanowar Elves", None);
    (s, call, elf)
}
#[test]
fn paid_call_complete_roster17_and_legacy_counter_refusals_preserve_native_world_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, _, _) = paid_scene(p, "Forest");
        let land = s.players[p.index()]
            .battlefield
            .iter()
            .copied()
            .find(|&id| card_def::CARD_DEFS[usize::from(s.objects.get(id).card_def)].is_land)
            .unwrap();
        // Labelled full-width native counter storage primitive on a paid scene.
        s.objects.get_mut(land).counters.fellowship = i32::MAX;
        s.objects.get_mut(land).counters.revival = i32::MAX;
        let before = s.clone();
        super::validate_state(&s).unwrap();
        let facts = crate::valkyries_call_public_v62::capture(&s, p).unwrap();
        let row = facts.battlefield[p.index()]
            .iter()
            .find(|r| r.card.object == land.0)
            .unwrap();
        assert_eq!(
            (row.counters.fellowship, row.counters.revival),
            (i32::MAX, i32::MAX)
        );
        assert!(crate::graveyard_return_observation_v61::validate_state(&s).is_err());
        assert!(
            crate::optional_trigger_public_v1::current(&s, s.objects.get(land).card_def).is_err()
        );
        assert_eq!(s, before);
    }
}
#[test]
fn independent_initial_hidden_libraries_paid_call_and_unsupported_visible_family_refuse_readonly_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (a, _, _) = paid_scene(p, "Forest");
        let (b, _, _) = paid_scene(p, "Counterspell");
        assert_ne!(a.state_hash(), b.state_hash());
        assert_eq!(
            crate::valkyries_call_public_v62::capture(&a, p).unwrap(),
            crate::valkyries_call_public_v62::capture(&b, p).unwrap()
        );
        let mut bad = a.clone();
        hand(&mut bad, p, "Perforating Artist");
        let before = bad.clone();
        assert!(super::require_supported(&bad, Some(p)).is_err());
        assert_eq!(bad, before);
        assert!(crate::card_def::preflight_fully_supported_deck(&[459]).is_err());
        assert!(crate::rl::require_declared_cards_for_scope(
            crate::rl::CommonProjectionScopeV42::CallV62,
            &[460]
        )
        .is_err());
        // Explicit hidden-card fixture primitive. Only the authorized looker
        // selects the predecessor refusal, never a concealed opponent identity.
        let mut hidden = ready(p, "Forest");
        let card = hand(&mut hidden, p, "Valkyrie's Call");
        hidden.objects.get_mut(card).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: false,
            lookers: 1 << p.index(),
            hidden_by: None,
            hidden_by_source: None,
        });
        let before = hidden.clone();
        assert!(
            crate::graveyard_return_public_v61::require_call_legacy_visible_v62(&hidden, Some(p))
                .is_err()
        );
        assert!(
            crate::graveyard_return_public_v61::require_call_legacy_visible_v62(
                &hidden,
                Some(p.opponent())
            )
            .is_ok()
        );
        assert_eq!(hidden, before);
    }
}
