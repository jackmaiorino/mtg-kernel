//! Real paid casts/native abilities through all four model owners. Lands,
//! libraries and the colored battlefield roster are explicit setup operands.
//! The empty-union case labels its effective-color primitive on a paid source.
use super::{
    entry_attack::{choice_guards, finish_entry, pay_cast, tap},
    *,
};
use crate::{
    card_def::card_id_by_name,
    event::CommittedEvent,
    mana::ManaColor,
    state::{StackItemKind, Step},
};
fn printing<O: Owner>(s: &O) {
    assert!(
        fact_count(
            O::packet(&s.encode()).state(),
            "registered_permanent_printing"
        ) >= 1
    );
}
fn quiet<O: Owner>(s: &mut O) {
    let actions = s.actions();
    let chosen = actions
        .iter()
        .find(|a| quiet_action(a))
        .or_else(|| (actions.len() == 1).then(|| &actions[0]))
        .unwrap_or_else(|| panic!("unexpected real turn choice:{actions:?}"))
        .clone();
    step(s, |a| a == &chosen);
}
fn at_pick<O: Owner>(s: &mut O) {
    for _ in 0..32 {
        if s.actions().iter().any(|a| {
            matches!(
                a,
                ActionSemanticV1::ChooseEffectTarget { .. }
                    | ActionSemanticV1::FinishEffectSelection { .. }
            )
        }) {
            return;
        }
        pass(s);
    }
    panic!("real Squad pick absent");
}
fn chosen(a: &ActionSemanticV1, id: ObjectId) -> bool {
    matches!(a,ActionSemanticV1::ChooseEffectTarget{target:TargetRefV1::Object{object},..}if object.arena_id==id.0)
}
fn sorted(mut ids: Vec<ObjectId>) -> Vec<ObjectId> {
    ids.sort();
    ids
}
fn rallier<O: Owner>(actor: PlayerId, take: bool) {
    let names = [
        "Llanowar Elves",
        "Bloom Tender",
        "Squad Rallier",
        "Forest",
        "Forest",
        "Forest",
    ];
    let cards: Vec<_> = names.iter().map(|n| card_id_by_name(n).unwrap()).collect();
    let forest = card_id_by_name("Forest").unwrap();
    let other = [forest; 40];
    let mut raw = GameState::new_from_libraries_with_starting_player_v1(
        if actor == PlayerId::P0 {
            &cards
        } else {
            &other
        },
        if actor == PlayerId::P1 {
            &cards
        } else {
            &other
        },
        crate::rl::card_name,
        110_365,
        actor,
    );
    raw.step = Step::Main1;
    let prefix = raw.players[actor.index()].library.clone();
    let source = put(&mut raw, actor, "Squad Rallier", Zone::Hand);
    let lands: Vec<_> = (0..7)
        .map(|_| put(&mut raw, actor, "Plains", Zone::Battlefield))
        .collect();
    let mut s = O::own(raw);
    printing(&s);
    pay_cast(&mut s, actor, source, &lands[..4]);
    finish_entry(&mut s, source);
    assert!(s.world().objects.get(source).summoning_sick);
    for land in &lands[4..] {
        tap(&mut s, *land);
    }
    let activation_index=s.actions().iter().position(|a|matches!(a,ActionSemanticV1::ActivateAbility{source:host,ability_index:0,..}if host.arena_id==source.0)).unwrap();
    choice_guards(&mut s, activation_index);
    step(
        &mut s,
        |a| matches!(a,ActionSemanticV1::ActivateAbility{source:host,ability_index:0,..}if host.arena_id==source.0),
    );
    assert_eq!(s.world().players[actor.index()].mana_pool, [0; 6]);
    assert!(!s.world().objects.get(source).tapped);
    assert!(s.world().stack.iter().any(|r| r.source == source
        && r.controller == actor
        && r.kind == StackItemKind::ActivatedAbility));
    // Independent hidden library worlds before the real private look must have
    // identical numeric packets; neither private root binding is exported.
    let mut alternative = s.world().clone();
    let hidden = alternative.objects.get_mut(prefix[1]);
    hidden.card_def = card_id_by_name("Llanowar Elves").unwrap();
    hidden.name = "Llanowar Elves".into();
    hidden.v4 = crate::state::ObjectStateV4::from_card_def(hidden.card_def);
    let alternative = O::own(alternative);
    assert_eq!(O::packet(&s.encode()), O::packet(&alternative.encode()));
    // Exact counter primitive on this genuinely paid row. No fictional
    // counterspell/model action is used, and the original journals are retained.
    let mut stopped = s.world().clone();
    let row = stopped
        .stack
        .iter()
        .find(|r| r.source == source)
        .unwrap()
        .v4
        .stack_item_id;
    assert!(crate::engine::counter_stack_item_by_id(&mut stopped, row)
        .unwrap()
        .is_some());
    let mut stopped = O::own(stopped);
    drain(&mut stopped);
    assert_eq!(stopped.world().players[actor.index()].library, prefix);
    assert_eq!(stopped.world().players[actor.index()].mana_pool, [0; 6]);
    assert!(stopped.world().known_library_cards(actor, actor).is_empty());
    let paid = s.snapshot();
    let mut result = None;
    for replay in 0..2 {
        if replay != 0 {
            s.restore(&paid).unwrap();
        }
        at_pick(&mut s);
        assert_eq!(s.world().known_library_cards(actor, actor).len(), 4);
        assert!(s
            .world()
            .known_library_cards(actor.opponent(), actor)
            .is_empty());
        assert!(s.actions().iter().any(|a| chosen(a, prefix[0])));
        assert!(s.actions().iter().any(|a| chosen(a, prefix[1])));
        assert!(!s
            .actions()
            .iter()
            .any(|a| chosen(a, prefix[2]) || chosen(a, prefix[3]) || chosen(a, prefix[4])));
        let index = s
            .actions()
            .iter()
            .position(|a| {
                if take {
                    chosen(a, prefix[1])
                } else {
                    matches!(
                        a,
                        ActionSemanticV1::FinishEffectSelection {
                            selected_count: 0,
                            ..
                        }
                    )
                }
            })
            .unwrap();
        choice_guards(&mut s, index);
        let prompt = s.snapshot();
        let rng = serde_json::to_vec(s.world().legacy_rng().unwrap()).unwrap();
        for prompt_replay in 0..2 {
            if prompt_replay != 0 {
                s.restore(&prompt).unwrap();
            }
            if take {
                step(&mut s, |a| chosen(a, prefix[1]));
            } else {
                step(&mut s, |a| {
                    matches!(
                        a,
                        ActionSemanticV1::FinishEffectSelection {
                            selected_count: 0,
                            ..
                        }
                    )
                });
            }
            drain(&mut s);
            let w = s.world();
            assert_eq!(&w.players[actor.index()].library[..2], &prefix[4..]);
            let rest = prefix[..4]
                .iter()
                .copied()
                .filter(|id| !take || *id != prefix[1])
                .collect();
            assert_eq!(
                sorted(w.players[actor.index()].library[2..].to_vec()),
                sorted(rest)
            );
            assert_eq!(
                w.objects.get(prefix[1]).zone,
                if take { Zone::Hand } else { Zone::Library }
            );
            assert_eq!(
                w.known_hand_cards(actor.opponent(), actor)
                    .iter()
                    .any(|c| c.object == prefix[1]),
                take
            );
            assert_ne!(serde_json::to_vec(w.legacy_rng().unwrap()).unwrap(), rng);
            assert_eq!(w.players[actor.index()].mana_pool, [0; 6]);
            let actual = (gameplay(&s), O::packet(&s.encode()).clone());
            if let Some(expected) = &result {
                assert_eq!(&actual, expected);
            } else {
                result = Some(actual);
            }
        }
    }
}
fn bloom<O: Owner>(actor: PlayerId, mode: u8) {
    let mut raw = crate::extra_turn_v1::tests::ready(actor);
    let source = put(&mut raw, actor, "Bloom Tender", Zone::Hand);
    let forest = put(&mut raw, actor, "Forest", Zone::Battlefield);
    let plains = put(&mut raw, actor, "Plains", Zone::Battlefield);
    for name in ["Island", "Swamp", "Mountain"] {
        put(&mut raw, actor, name, Zone::Battlefield);
    }
    put(
        &mut raw,
        actor.opponent(),
        "Frenzied Goblin",
        Zone::Battlefield,
    );
    // These are actual immutable current colored public permanents, supplied
    // as battlefield operands. This fixture does not claim their cast/token issuer.
    if mode == 1 {
        for name in [
            "Healer's Hawk",
            "Healer's Hawk",
            "Drake Token",
            "Vampire Gourmand",
            "Frenzied Goblin",
        ] {
            put(&mut raw, actor, name, Zone::Battlefield);
        }
    }
    let mut s = O::own(raw);
    printing(&s);
    pay_cast(&mut s, actor, source, &[forest, plains]);
    finish_entry(&mut s, source);
    assert!(!s.actions().iter().any(|a|matches!(a,ActionSemanticV1::ActivateManaAbility{source:host,..}if host.arena_id==source.0)),"real summoning sickness applies");
    for _ in 0..512 {
        if !s.world().objects.get(source).summoning_sick
            && s.world().step == Step::Main1
            && s.world().active_player == actor
            && s.world().priority_player == actor
            && s.world().stack.is_empty()
        {
            break;
        }
        quiet(&mut s);
    }
    assert!(!s.world().objects.get(source).summoning_sick);
    assert_eq!(s.world().players[actor.index()].mana_pool, [0; 6]);
    if mode == 2 {
        // Explicit current effective-color primitive on the genuinely paid,
        // naturally matured source; no invented color-changing printed issuer.
        let mut w = s.world().clone();
        w.objects.get_mut(source).v4.effective_color_mask = 1 << ManaColor::C.pool_index();
        s = O::own(w);
    }
    let colors = if mode == 1 {
        vec![
            ManaColor::W,
            ManaColor::U,
            ManaColor::B,
            ManaColor::R,
            ManaColor::G,
        ]
    } else if mode == 0 {
        vec![ManaColor::G]
    } else {
        vec![]
    };
    assert_eq!(
        crate::bloom_tender_mana_v1::colors(s.world(), actor),
        colors
    );
    let actions = s.actions();
    let i=actions.iter().position(|a|matches!(a,ActionSemanticV1::ActivateManaAbility{source:host,mana_choice:Some(ManaColor::G),cost_target:None,..}if host.arena_id==source.0)).unwrap();
    assert_eq!(actions.iter().filter(|a|matches!(a,ActionSemanticV1::ActivateManaAbility{source:host,..}if host.arena_id==source.0)).count(),1,"one complete bundle, no color choice menu");
    choice_guards(&mut s, i);
    let prior = s.snapshot();
    let mut result = None;
    for replay in 0..2 {
        if replay != 0 {
            s.restore(&prior).unwrap();
        }
        let start = s.world().engine.event_history.len();
        step(
            &mut s,
            |a| matches!(a,ActionSemanticV1::ActivateManaAbility{source:host,..}if host.arena_id==source.0),
        );
        assert!(s.world().stack.is_empty());
        assert!(s.world().objects.get(source).tapped);
        let mut pool = [0; 6];
        for color in &colors {
            pool[color.pool_index()] = 1;
        }
        assert_eq!(s.world().players[actor.index()].mana_pool, pool);
        let added = s.world().engine.event_history[start..]
            .iter()
            .filter_map(|e| {
                if let CommittedEvent::ManaAdded { colors, .. } = e {
                    Some(colors)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if colors.is_empty() {
            assert!(
                added.is_empty(),
                "tap cost still paid without a mana-add event"
            );
        } else {
            assert_eq!(
                added,
                vec![&colors],
                "one simultaneous colored bundle event"
            );
        }
        let actual = (gameplay(&s), O::packet(&s.encode()).clone());
        if let Some(expected) = &result {
            assert_eq!(&actual, expected);
        } else {
            result = Some(actual);
        }
    }
}
pub(super) fn paid<O: Owner>() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for take in [false, true] {
            rallier::<O>(actor, take);
        }
        for mode in 0..3 {
            bloom::<O>(actor, mode);
        }
    }
}
