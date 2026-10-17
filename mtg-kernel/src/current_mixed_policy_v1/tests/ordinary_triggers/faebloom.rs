//! Paid targetless instant and distinct native reflexive choices through all
//! four sealed owners. Basic lands/libraries/vanilla creatures are setup.
//! Doubling Season and the lower Think Twice are genuinely paid. The already
//! tapped target is tapped by its real mana ability. Legality branches label
//! protection/control/reentry and counter primitives on the actual paid history.
use super::{
    entry_attack::{choice_guards, finish_entry, pay_cast, tap},
    *,
};
use crate::{
    card_def::{card_id_by_name, Keywords},
    event::{self, CommittedEvent, ProposedEvent},
    faebloom_trick_v43::JournalV43,
    state::{FaceDownV1, StackItemKind, Step, Target},
};
fn printing<O: Owner>(s: &O) {
    let d = s.encode();
    assert_eq!(
        fact_count(O::packet(&d).state(), "registered_instant_printing"),
        1
    );
    assert!(fact_count(O::packet(&d).state(), "ordered_reflexive_roots") > 0);
}
fn quiet<O: Owner>(s: &mut O) {
    let actions = s.actions();
    let chosen = actions
        .iter()
        .find(|a| quiet_action(a))
        .or_else(|| (actions.len() == 1).then(|| &actions[0]))
        .unwrap_or_else(|| panic!("unexpected real choice:{actions:?}"))
        .clone();
    step(s, |a| a == &chosen);
}
fn faeries<O: Owner>(s: &O, actor: PlayerId) -> Vec<ObjectId> {
    let def = card_id_by_name("Faerie Token").unwrap();
    s.world().players[actor.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|id| s.world().objects.get(*id).card_def == def)
        .collect()
}
fn assert_creation<O: Owner>(s: &O, actor: PlayerId, count: usize) {
    let tokens = faeries(s, actor);
    assert_eq!(tokens.len(), count);
    for id in tokens {
        let o = s.world().objects.get(id);
        assert_eq!(o.owner, actor);
        assert_eq!(o.controller, actor);
        assert_eq!(o.zone, Zone::Battlefield);
        assert_eq!(o.zone_change_count, 0);
        assert!(o.summoning_sick);
        assert_eq!(
            crate::engine::checked_effective_pt(s.world(), id).unwrap(),
            (1, 1)
        );
        assert!(crate::engine::has_effective_keyword(
            s.world(),
            id,
            Keywords::FLYING
        ));
    }
    assert_eq!(s.world().engine.event_history.iter().filter(|e|matches!(e,CommittedEvent::CreateToken{token_def,controller,..}if Some(*token_def)==card_id_by_name("Faerie Token") && *controller==actor)).count(),count);
    assert_eq!(s.world().engine.event_history.iter().filter(|e|matches!(e,CommittedEvent::FaebloomJournalV43{journal}if matches!(journal.as_ref(),JournalV43::ReflexiveCaptured{..}))).count(),1);
}
fn guarded_cast<O: Owner>(s: &mut O, actor: PlayerId, source: ObjectId, lands: &[ObjectId]) {
    for land in lands {
        tap(s, *land);
    }
    printing(s);
    let index = s
        .actions()
        .iter()
        .position(
            |a| matches!(a,ActionSemanticV1::CastSpell{source:host,..}if host.arena_id==source.0),
        )
        .unwrap();
    choice_guards(s, index);
    let hand = s.snapshot();
    let mut foreign = O::own(s.world().clone());
    let before = gameplay(&foreign);
    assert!(matches!(
        foreign.restore(&hand),
        Err(Error::ForeignSnapshot)
    ));
    assert_eq!(gameplay(&foreign), before);
    let mut result = None;
    for replay in 0..2 {
        if replay != 0 {
            s.restore(&hand).unwrap();
        }
        cast(s, source);
        assert!(s.world().engine.pending_cast.is_none());
        assert_eq!(s.world().players[actor.index()].mana_pool, [0; 6]);
        assert!(lands.iter().all(|id| s.world().objects.get(*id).tapped));
        assert_eq!(s.world().objects.get(source).zone, Zone::Stack);
        let item = s
            .world()
            .stack
            .iter()
            .find(|item| item.source == source)
            .unwrap();
        assert_eq!(item.kind, StackItemKind::Spell);
        assert_eq!(item.controller, actor);
        assert!(item.targets.is_empty() && item.v4.target_contracts.is_empty());
        assert_eq!(item.v4.target_spec, Some(crate::card_def::TargetSpec::None));
        let output = (gameplay(s), O::packet(&s.encode()).clone());
        if let Some(old) = &result {
            assert_eq!(&output, old);
        } else {
            result = Some(output);
        }
    }
}
fn legality<O: Owner>(s: &O, actor: PlayerId, target: ObjectId, count: usize) {
    // These three labelled kernel primitives modify an actually published,
    // originally paid reflexive. They claim no printed response spell.
    for mode in 0..3 {
        let mut raw = s.world().clone();
        if mode == 0 {
            let binding = crate::effect::EffectObjectBinding {
                object: target,
                expected_zone: Zone::Battlefield,
                expected_zone_change_count: raw.objects.get(target).zone_change_count,
            };
            crate::effect::install_temporary_boost(
                &mut raw,
                binding,
                0,
                0,
                Keywords::PROTECTION_FROM_MONOCOLORED,
            );
        } else if mode == 1 {
            crate::standard_cards_v1::gain_control(&mut raw, target, actor);
        } else {
            event::propose_and_commit(&mut raw, ProposedEvent::zone_change(target, Zone::Hand));
            event::propose_and_commit(
                &mut raw,
                ProposedEvent::zone_change(target, Zone::Battlefield),
            );
        }
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&raw).unwrap()).unwrap();
        let mut result = None;
        for world in [raw, restored] {
            let mut owned = O::own(world);
            printing(&owned);
            let waiting = owned.snapshot();
            drain(&mut owned);
            assert!(!owned.world().objects.get(target).tapped);
            assert_creation(&owned, actor, count);
            assert_eq!(
                crate::faebloom_public_v43::capture(owned.world())
                    .unwrap()
                    .reflexives[0]
                    .stage,
                crate::faebloom_public_v43::StageV43::NoLegalTarget
            );
            let output = (gameplay(&owned), O::packet(&owned.encode()).clone());
            owned.restore(&waiting).unwrap();
            drain(&mut owned);
            assert_eq!(
                (gameplay(&owned), O::packet(&owned.encode()).clone()),
                output
            );
            if let Some(old) = &result {
                assert_eq!(&output, old);
            } else {
                result = Some(output);
            }
        }
    }
}
fn run<O: Owner>(actor: PlayerId, season: bool, already_tapped: bool, with_targets: bool) {
    let mut raw = crate::extra_turn_v1::tests::ready(actor);
    let source = put(&mut raw, actor, "Faebloom Trick", Zone::Hand);
    let lower = put(&mut raw, actor.opponent(), "Think Twice", Zone::Hand);
    let own = put(&mut raw, actor, "Llanowar Elves", Zone::Battlefield);
    let targets: Vec<_> = (0..if with_targets { 2 } else { 0 })
        .map(|_| {
            put(
                &mut raw,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            )
        })
        .collect();
    let lands: Vec<_> = ["Island", "Forest", "Forest"]
        .into_iter()
        .map(|name| put(&mut raw, actor, name, Zone::Battlefield))
        .collect();
    let islands = [
        put(&mut raw, actor.opponent(), "Island", Zone::Battlefield),
        put(&mut raw, actor.opponent(), "Island", Zone::Battlefield),
    ];
    let season_source = season.then(|| put(&mut raw, actor, "Doubling Season", Zone::Hand));
    let forests: Vec<_> = (0..if season { 5 } else { 0 })
        .map(|_| put(&mut raw, actor, "Forest", Zone::Battlefield))
        .collect();
    let mut s = O::own(raw);
    printing(&s);
    if let Some(season) = season_source {
        pay_cast(&mut s, actor, season, &forests);
        finish_entry(&mut s, season);
    }
    for _ in 0..128 {
        if s.world().active_player == actor.opponent()
            && s.world().priority_player == actor.opponent()
            && s.world().step == Step::Main1
            && s.world().stack.is_empty()
        {
            break;
        }
        quiet(&mut s);
    }
    assert_eq!(s.world().active_player, actor.opponent());
    assert_eq!(s.world().step, Step::Main1);
    pay_cast(&mut s, actor.opponent(), lower, &islands);
    if already_tapped {
        tap(&mut s, targets[0]);
        assert!(s.world().objects.get(targets[0]).tapped);
    }
    guarded_cast(&mut s, actor, source, &lands);
    assert_eq!(s.world().active_player, actor.opponent());
    assert_eq!(s.world().stack.len(), 2);
    let original = s
        .world()
        .stack
        .iter()
        .find(|item| item.source == source)
        .unwrap()
        .v4
        .source_contract
        .unwrap();
    assert!(faeries(&s, actor).is_empty());
    if !season && !already_tapped && with_targets {
        // Honest exact-stack-ID counter primitive on the genuine paid spell.
        let mut countered = s.world().clone();
        let id = countered
            .stack
            .iter()
            .find(|item| item.source == source)
            .unwrap()
            .v4
            .stack_item_id;
        crate::engine::counter_stack_item_by_id(&mut countered, id).unwrap();
        let mut countered = O::own(countered);
        drain(&mut countered);
        assert!(faeries(&countered, actor).is_empty());
        assert!(crate::faebloom_public_v43::capture(countered.world())
            .unwrap()
            .reflexives
            .is_empty());
    }
    let paid = s.snapshot();
    let mut creation = None;
    let count = if season { 4 } else { 2 };
    for replay in 0..2 {
        if replay != 0 {
            s.restore(&paid).unwrap();
        }
        if !with_targets {
            drain(&mut s);
            assert_creation(&s, actor, count);
            assert_eq!(s.world().objects.get(source).zone, Zone::Graveyard);
            let facts = crate::faebloom_public_v43::capture(s.world()).unwrap();
            assert_eq!(facts.reflexives.len(), 1);
            assert_eq!(
                facts.reflexives[0].stage,
                crate::faebloom_public_v43::StageV43::Dropped
            );
            let output = (gameplay(&s), O::packet(&s.encode()).clone());
            if let Some(old) = &creation {
                assert_eq!(&output, old);
            } else {
                creation = Some(output);
            }
            continue;
        }
        for _ in 0..32 {
            if s.actions().iter().any(|a| target(a, targets[0])) {
                break;
            }
            pass(&mut s);
        }
        assert_eq!(s.world().objects.get(source).zone, Zone::Graveyard);
        assert!(s.world().stack.iter().any(|item| item.source == lower));
        assert_creation(&s, actor, count);
        printing(&s);
        let options: Vec<_> = s
            .actions()
            .iter()
            .filter_map(|a| match a {
                ActionSemanticV1::ChooseTarget {
                    target: TargetRefV1::Object { object },
                    remaining: 1,
                    ..
                } => Some(object.arena_id),
                _ => None,
            })
            .collect();
        let mut expected = targets.iter().map(|id| id.0).collect::<Vec<_>>();
        expected.sort();
        let mut actual = options;
        actual.sort();
        assert_eq!(actual, expected);
        assert!(!s.actions().iter().any(|a| target(a, own)));
        for id in faeries(&s, actor) {
            assert!(!s.actions().iter().any(|a| target(a, id)));
        }
        let index = s
            .actions()
            .iter()
            .position(|a| target(a, targets[0]))
            .unwrap();
        choice_guards(&mut s, index);
        let prompt = s.snapshot();
        let mut result = None;
        for answer in 0..2 {
            if answer != 0 {
                s.restore(&prompt).unwrap();
            }
            step(&mut s, |a| target(a, targets[0]));
            let trigger = s
                .world()
                .stack
                .iter()
                .find(|item| item.source == source && item.kind == StackItemKind::TriggeredAbility)
                .unwrap();
            assert_eq!(trigger.controller, actor);
            assert_eq!(trigger.targets, vec![Target::Object(targets[0])]);
            assert_eq!(trigger.v4.source_contract, Some(original));
            assert_eq!(trigger.v4.target_contracts.len(), 1);
            if !season && !already_tapped {
                legality(&s, actor, targets[0], count);
            }
            let published = s.snapshot();
            drain(&mut s);
            assert!(s.world().objects.get(targets[0]).tapped);
            assert!(!s.world().objects.get(targets[1]).tapped);
            assert_creation(&s, actor, count);
            assert_eq!(
                crate::faebloom_public_v43::capture(s.world())
                    .unwrap()
                    .reflexives[0]
                    .stage,
                crate::faebloom_public_v43::StageV43::Tapped
            );
            let output = (gameplay(&s), O::packet(&s.encode()).clone());
            s.restore(&published).unwrap();
            drain(&mut s);
            assert_eq!((gameplay(&s), O::packet(&s.encode()).clone()), output);
            if let Some(old) = &result {
                assert_eq!(&output, old);
            } else {
                result = Some(output);
            }
        }
        let output = result.unwrap();
        if let Some(old) = &creation {
            assert_eq!(&output, old);
        } else {
            creation = Some(output);
        }
    }
}
pub(super) fn paid<O: Owner>() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        run::<O>(actor, false, false, true);
        run::<O>(actor, true, false, true);
        run::<O>(actor, false, true, true);
        run::<O>(actor, false, false, false);
    }
}
pub(super) fn privacy<O: Owner>() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for known in [false, true] {
            let build = |hidden: &str| {
                let mut raw = crate::extra_turn_v1::tests::ready(actor);
                if known {
                    put(&mut raw, actor, "Faebloom Trick", Zone::Hand);
                }
                put(&mut raw, actor.opponent(), hidden, Zone::Hand);
                let face = put(&mut raw, actor.opponent(), hidden, Zone::Battlefield);
                raw.objects.get_mut(face).v4.face_down_v1 = Some(FaceDownV1 {
                    disguised: true,
                    lookers: 0,
                    hidden_by: None,
                    hidden_by_source: None,
                });
                O::own(raw)
            };
            let a = build("Faebloom Trick");
            let b = build("Llanowar Elves");
            assert_eq!(O::packet(&a.encode()), O::packet(&b.encode()));
            assert_eq!(
                fact_count(
                    O::packet(&a.encode()).state(),
                    "registered_instant_printing"
                ),
                usize::from(known)
            );
        }
    }
}
