//! Every active continuous effect that changes a projected characteristic
//! must appear in the observation's `continuous_effects` with its real
//! duration. Throne of the Dead Three's "It gains hexproof until your next
//! turn" showed on the creature (`effective_keywords.hexproof`) with no
//! effect granting it (found by shadowing mtg-kernel with the gorge engine:
//! SpellBench Elves-800 step 185).

use mtg_kernel::card_def::{card_id_by_name, Keywords, CARD_DEFS};
use mtg_kernel::engine::UntilNextTurnKeywordEffectV1;
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::rl::observe_v2;
use mtg_kernel::state::{Counters, GameObject, GameState, ObjectStateV4, Step, Zone};
use mtg_kernel::surface_v2::HarnessSurfaceV2;

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn put_creature(state: &mut GameState, owner: PlayerId, name: &str) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"));
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.to_string(),
        owner,
        controller: owner,
        zone: Zone::Battlefield,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 1,
    });
    state.players[owner.index()].battlefield.push(id);
    id
}

/// Elves-800 step 185: Throne put Generous Ent onto the battlefield with
/// three +1/+1 counters and hexproof until p1's next turn.
#[test]
fn an_until_next_turn_hexproof_grant_is_a_projected_continuous_effect() {
    let mut state = GameState::new_from_libraries(&[], &[], card_name, 0x4b53_4841_444f_570c);
    state.active_player = PlayerId::P1;
    state.priority_player = PlayerId::P1;
    state.step = Step::Main1;
    let ent = put_creature(&mut state, PlayerId::P1, "Generous Ent");
    state.objects.get_mut(ent).counters.plus1_plus1 = 3;
    state
        .engine
        .until_next_turn_keywords
        .push(UntilNextTurnKeywordEffectV1 {
            object_id: ent,
            object_zone_change_count: 1,
            holder: PlayerId::P1,
            expires_at_turn: state.turn + 1,
            keywords: Keywords::HEXPROOF,
            timestamp: None,
        });

    for observer in [PlayerId::P0, PlayerId::P1] {
        let observation = observe_v2(&state, &HarnessSurfaceV2::new(), observer, 0).unwrap();
        let value = serde_json::to_value(&observation).unwrap();
        let projection = &value["projection"];
        let card = projection["battlefield"][1]
            .as_array()
            .unwrap()
            .iter()
            .find(|card| card["stable"]["arena_id"] == ent.0)
            .expect("the Ent is on p1's battlefield");
        assert_eq!(
            card["characteristics"]["effective_keywords"]["hexproof"],
            true
        );
        let granting: Vec<_> = projection["continuous_effects"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|effect| {
                effect["affected_objects"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|object| object["arena_id"] == ent.0)
                    && effect["add_keyword_mask"].as_u64().unwrap()
                        & u64::from(Keywords::HEXPROOF.0)
                        != 0
            })
            .collect();
        assert_eq!(granting.len(), 1, "{observer:?} sees the hexproof grant");
        assert_eq!(granting[0]["duration"], "until_controllers_next_turn");
        assert_eq!(granting[0]["controller"], "p1");
    }

    // The grant ends as its holder's next turn begins; so does the effect.
    let mut expired = state.clone();
    expired.engine.until_next_turn_keywords.clear();
    let value = serde_json::to_value(
        observe_v2(&expired, &HarnessSurfaceV2::new(), PlayerId::P0, 0).unwrap(),
    )
    .unwrap();
    assert!(value["projection"]["continuous_effects"]
        .as_array()
        .unwrap()
        .is_empty());
}
