//! Source preparation. Bloom Tender is unregistered and these games are unrun.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::{
    card_def::{card_id_by_name, ManaAbilityAmountDef, ManaAbilityCostDef, Subtype, CARD_DEFS},
    engine::{self, Action, Decision},
    event::{self, ProposedEvent},
    ids::{ObjectId, PlayerId},
    mana::{ManaColor, Pip},
    state::{GameObject, GameState, ObjectStateV4, Step, Zone},
};

fn ready(actor: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        124,
        actor,
    );
    s.step = Step::Main1;
    s
}
fn put(s: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let def = card_id_by_name(name).expect("future registered fixture definition");
    let id = s.objects.push(GameObject {
        card_def: def,
        name: name.into(),
        owner,
        controller: owner,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => s.players[owner.index()].hand.push(id),
        Zone::Battlefield => s.players[owner.index()].battlefield.push(id),
        _ => panic!("fixture zone"),
    }
    id
}
fn restore(s: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(s).unwrap()).unwrap()
}
fn next(s: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(s);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}
fn pass(s: &mut GameState, d: Decision) {
    let a = match d {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
        Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        other => panic!("unexpected {other:?}"),
    };
    engine::step(s, a).unwrap();
}
fn cast(s: &mut GameState, actor: PlayerId) -> ObjectId {
    let source = put(s, actor, "Bloom Tender", Zone::Hand);
    s.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
    s.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
    next(s);
    engine::step(s, Action::CastSpell(source)).unwrap();
    next(s);
    assert!(s.stack.iter().any(|item| item.source == source));
    for _ in 0..32 {
        let d = next(s);
        if s.objects.get(source).zone == Zone::Battlefield && s.stack.is_empty() {
            return source;
        }
        pass(s, d);
    }
    panic!("real cast did not resolve");
}
fn mature(s: &mut GameState, source: ObjectId, actor: PlayerId) {
    for _ in 0..160 {
        let d = next(s);
        if !s.objects.get(source).summoning_sick
            && s.step == Step::Main1
            && matches!(d,Decision::CastSpellOrPass{player,..} if player==actor)
        {
            return;
        }
        pass(s, d);
    }
    panic!("next own turn not reached");
}
fn activate(s: &mut GameState, source: ObjectId) {
    engine::step(s, Action::ActivateManaAbility(source)).unwrap();
}

#[test]
fn printed_cost_recipe_real_cast_sickness_and_immediate_activation_both_seats() {
    let def = &CARD_DEFS[card_id_by_name("Bloom Tender").unwrap() as usize];
    assert_eq!(def.cost.generic, 1);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::G)]);
    assert_eq!((def.power, def.toughness), (Some(1), Some(1)));
    assert_eq!(def.subtypes, &[Subtype::Elf, Subtype::Druid]);
    let ability = def.mana_ability_def.unwrap();
    assert_eq!(ability.cost, ManaAbilityCostDef::TapSelf);
    assert_eq!(
        ability.amount,
        ManaAbilityAmountDef::EachControlledPermanentColor
    );
    assert!(!def.is_automatic_payment_mana_source());
    assert!(def.mana_ability_program_for(ManaColor::G, None).is_none());
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let source = cast(&mut s, actor);
        let before = serde_json::to_vec(&s).unwrap();
        assert!(engine::step(&mut s, Action::ActivateManaAbility(source)).is_err());
        assert_eq!(serde_json::to_vec(&s).unwrap(), before);
        mature(&mut s, source, actor);
        let mut replay = restore(&s);
        activate(&mut s, source);
        activate(&mut replay, source);
        assert_eq!(
            serde_json::to_vec(&s).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        assert_eq!(s.players[actor.index()].mana_pool, [0, 0, 0, 0, 1, 0]);
        assert!(s.objects.get(source).tapped);
        assert!(s.stack.is_empty());
        assert_eq!(s.engine.last_mana_ability_activator, Some(actor));
        assert_eq!(s.objects.get(source).v4.ability_uses_this_turn[0].uses, 1);
    }
}
#[test]
fn every_distinct_color_is_added_once_in_one_real_mana_event() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let source = cast(&mut s, actor);
        mature(&mut s, source, actor);
        for name in [
            "Healer's Hawk",
            "High Fae Trickster",
            "Vengeful Bloodwitch",
            "Guttersnipe",
            "Llanowar Elves",
            "Healer's Hawk",
        ] {
            put(&mut s, actor, name, Zone::Battlefield);
        }
        let start = s.engine.event_history.len();
        let mut replay = restore(&s);
        activate(&mut s, source);
        activate(&mut replay, source);
        assert_eq!(s.players[actor.index()].mana_pool, [1, 1, 1, 1, 1, 0]);
        assert_eq!(
            serde_json::to_vec(&s).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        let events = s.engine.event_history[start..]
            .iter()
            .filter_map(|e| {
                if let event::CommittedEvent::ManaAdded { colors, .. } = e {
                    Some(colors)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            events,
            vec![&vec![
                ManaColor::W,
                ManaColor::U,
                ManaColor::B,
                ManaColor::R,
                ManaColor::G
            ]]
        );
    }
}
#[test]
fn opponent_hand_colorless_and_departed_permanents_do_not_contribute() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let source = cast(&mut s, actor);
        mature(&mut s, source, actor);
        put(&mut s, actor.opponent(), "Guttersnipe", Zone::Battlefield);
        put(&mut s, actor, "Counterspell", Zone::Hand);
        put(&mut s, actor, "Forest", Zone::Battlefield);
        let white = put(&mut s, actor, "Healer's Hawk", Zone::Battlefield);
        event::propose_and_commit(&mut s, ProposedEvent::zone_change(white, Zone::Hand));
        activate(&mut s, source);
        assert_eq!(s.players[actor.index()].mana_pool, [0, 0, 0, 0, 1, 0]);
    }
}
#[test]
fn overflow_any_output_refuses_before_tap_use_or_mana_and_capacity_edge_replays() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pool in [254, 255] {
            let mut s = ready(actor);
            let source = cast(&mut s, actor);
            mature(&mut s, source, actor);
            put(&mut s, actor, "Healer's Hawk", Zone::Battlefield);
            s.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = pool;
            let before = serde_json::to_vec(&s).unwrap();
            let mut replay = restore(&s);
            if pool == 255 {
                assert!(engine::step(&mut s, Action::ActivateManaAbility(source)).is_err());
                assert_eq!(serde_json::to_vec(&s).unwrap(), before);
            } else {
                activate(&mut s, source);
                activate(&mut replay, source);
                assert_eq!(s.players[actor.index()].mana_pool, [255, 0, 0, 0, 1, 0]);
                assert_eq!(
                    serde_json::to_vec(&s).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
            }
        }
    }
}
#[test]
fn real_witness_suppression_and_removal_restores_printed_mana_ability() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let source = cast(&mut s, actor);
        mature(&mut s, source, actor);
        let aura = put(&mut s, actor, "Witness Protection", Zone::Hand);
        s.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
        engine::step(&mut s, Action::CastSpell(aura)).unwrap();
        engine::step(
            &mut s,
            Action::ChooseTarget(mtg_kernel::state::Target::Object(source)),
        )
        .unwrap();
        for _ in 0..32 {
            let d = next(&mut s);
            if s.objects.get(aura).zone == Zone::Battlefield && s.stack.is_empty() {
                break;
            }
            pass(&mut s, d);
        }
        assert_eq!(s.objects.get(aura).zone, Zone::Battlefield);
        let before = serde_json::to_vec(&s).unwrap();
        assert!(engine::step(&mut s, Action::ActivateManaAbility(source)).is_err());
        assert_eq!(serde_json::to_vec(&s).unwrap(), before);
        event::propose_and_commit(&mut s, ProposedEvent::zone_change(aura, Zone::Graveyard));
        let mut replay = restore(&s);
        activate(&mut s, source);
        activate(&mut replay, source);
        assert_eq!(s.players[actor.index()].mana_pool, [0, 0, 0, 0, 1, 0]);
        assert_eq!(
            serde_json::to_vec(&s).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
fn precise_selected_meaning_reconstructs_recipe_and_refuses_forged_facts() {
    use mtg_kernel::rules_meaning_bloom_tender_v1::{self as meaning, ErrorV1};
    let id = card_id_by_name("Bloom Tender").unwrap();
    let selector = meaning::live_selector().unwrap();
    let original = meaning::meaning(&selector, id).unwrap();
    meaning::validate(&original).unwrap();
    let mut changed = original.clone();
    changed.source_included = false;
    assert_eq!(meaning::validate(&changed), Err(ErrorV1::IdentityMismatch));
    let mut changed = original.clone();
    changed.one_unit_of_each_distinct_color_added_simultaneously = false;
    assert_eq!(meaning::validate(&changed), Err(ErrorV1::IdentityMismatch));
    let mut changed = original.clone();
    changed.selector.producer_sha256.push('x');
    assert_eq!(meaning::validate(&changed), Err(ErrorV1::WrongSelector));
    let mut json = serde_json::to_value(original).unwrap();
    json["unrecognized"] = true.into();
    assert!(serde_json::from_value::<meaning::MeaningV1>(json).is_err());
}
