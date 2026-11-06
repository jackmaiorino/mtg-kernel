//! The equipment effect list follows public battlefield order, never arena
//! order: two states with the same zone lists and different arena numbering
//! (as the W1 information-set import renumbers a native state) observe the
//! same effect sequence.

use super::*;
use crate::policy_observation_v6::tests::{put, ready_state};

fn attach(state: &mut GameState, equipment: ObjectId, host: ObjectId) {
    let link = crate::state::ObjectLinkV4 {
        object: host,
        zone_change_count: state.objects.get(host).zone_change_count,
    };
    state.objects.get_mut(equipment).v4.attached_to = Some(link);
    state.objects.get_mut(host).attachments.push(equipment);
}

/// The effect sources (card names) in list order, with the hosts they affect.
fn effect_sequence(state: &GameState) -> Vec<(String, Vec<String>)> {
    let name = |r: &CardStableRefV1| state.objects.get(ObjectId(r.arena_id)).name.to_string();
    continuous_effects_common_v2(state, PlayerId::P0, (false, false), false, false)
        .unwrap()
        .iter()
        .map(|e| {
            (
                e.source.as_ref().map(&name).unwrap_or_default(),
                e.affected_objects.iter().map(&name).collect(),
            )
        })
        .collect()
}

/// One board: Blowgun on the Shaman, Rod on the Enforcer, the Enforcer
/// entered first. `shaman_first` only decides which host gets the lower
/// arena id.
fn board(shaman_first: bool) -> GameState {
    let mut state = ready_state();
    let blowgun = put(
        &mut state,
        PlayerId::P0,
        "Hunter's Blowgun",
        Zone::Battlefield,
    );
    let rod = put(
        &mut state,
        PlayerId::P0,
        "Black Mage's Rod",
        Zone::Battlefield,
    );
    let (shaman, enforcer) = if shaman_first {
        let s = put(
            &mut state,
            PlayerId::P0,
            "Krark-Clan Shaman",
            Zone::Battlefield,
        );
        (
            s,
            put(&mut state, PlayerId::P0, "Myr Enforcer", Zone::Battlefield),
        )
    } else {
        let e = put(&mut state, PlayerId::P0, "Myr Enforcer", Zone::Battlefield);
        (
            put(
                &mut state,
                PlayerId::P0,
                "Krark-Clan Shaman",
                Zone::Battlefield,
            ),
            e,
        )
    };
    attach(&mut state, blowgun, shaman);
    attach(&mut state, rod, enforcer);
    let bf = &mut state.players[0].battlefield;
    bf.retain(|id| *id != shaman && *id != enforcer);
    bf.extend([enforcer, shaman]);
    state
}

#[test]
fn equipment_effects_follow_battlefield_order_not_arena_ids() {
    let low_shaman = board(true);
    let low_enforcer = board(false);
    let expected = vec![
        (
            "Black Mage's Rod".to_string(),
            vec!["Myr Enforcer".to_string()],
        ),
        (
            "Hunter's Blowgun".to_string(),
            vec!["Krark-Clan Shaman".to_string()],
        ),
    ];
    // The Enforcer entered first, so its Rod effect comes first whichever
    // host holds the lower arena id (arena order put the Blowgun first in
    // the low-Shaman board: g-1ace0997f6087956 step 97).
    assert_eq!(effect_sequence(&low_shaman), expected);
    assert_eq!(effect_sequence(&low_enforcer), expected);
}

/// One host with both equipment, the Rod entered first. `blowgun_first`
/// only decides which equipment gets the lower arena id; the host's
/// `attachments` list is kept in arena order, as the engine keeps it.
fn two_on_one_host(blowgun_first: bool) -> GameState {
    let mut state = ready_state();
    let host = put(&mut state, PlayerId::P0, "Myr Enforcer", Zone::Battlefield);
    let (blowgun, rod) = if blowgun_first {
        let b = put(
            &mut state,
            PlayerId::P0,
            "Hunter's Blowgun",
            Zone::Battlefield,
        );
        (
            b,
            put(
                &mut state,
                PlayerId::P0,
                "Black Mage's Rod",
                Zone::Battlefield,
            ),
        )
    } else {
        let r = put(
            &mut state,
            PlayerId::P0,
            "Black Mage's Rod",
            Zone::Battlefield,
        );
        (
            put(
                &mut state,
                PlayerId::P0,
                "Hunter's Blowgun",
                Zone::Battlefield,
            ),
            r,
        )
    };
    attach(&mut state, blowgun, host);
    attach(&mut state, rod, host);
    state.objects.get_mut(host).attachments.sort();
    let bf = &mut state.players[0].battlefield;
    bf.retain(|id| *id != blowgun && *id != rod);
    bf.extend([rod, blowgun]);
    state
}

#[test]
fn one_hosts_equipment_effects_follow_battlefield_order_not_arena_ids() {
    let expected = vec![
        (
            "Black Mage's Rod".to_string(),
            vec!["Myr Enforcer".to_string()],
        ),
        (
            "Hunter's Blowgun".to_string(),
            vec!["Myr Enforcer".to_string()],
        ),
    ];
    assert_eq!(effect_sequence(&two_on_one_host(true)), expected);
    assert_eq!(effect_sequence(&two_on_one_host(false)), expected);
}
