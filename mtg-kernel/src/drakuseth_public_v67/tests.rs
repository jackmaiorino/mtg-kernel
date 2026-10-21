//! Genuine paid native combat and response fixtures. Support operands are labelled.
use super::*;
use crate::{
    card_def::{self, CardCapability, Keywords, TargetSpec},
    effect::EffectOp,
    engine::{Action, Decision},
    event::{self, ProposedEvent},
    mana::ManaColor,
    policy_observation_v6::tests::put,
    state::FaceDownV1,
};
const NAME: &str = meaning::NAME;
fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}
fn quiet(state: &mut GameState, decision: Decision) {
    match decision {
        Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
        Decision::DeclareAttackers { .. } => {
            engine::step(state, Action::DeclareAttackers(Vec::new())).unwrap()
        }
        Decision::DeclareBlockers { .. } => {
            engine::step(state, Action::DeclareBlockers(Vec::new())).unwrap()
        }
        Decision::OrderTriggers { pending, .. } => {
            engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
        }
        other => panic!("unexpected actual native choice: {other:?}"),
    }
}
fn settle(state: &mut GameState) {
    for _ in 0..64 {
        let decision = next(state);
        if matches!(decision, Decision::CastSpellOrPass { .. })
            && state.stack.is_empty()
            && state.engine.pending_cast.is_none()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        quiet(state, decision);
    }
    panic!("genuine Drakuseth operation did not settle");
}
fn restored(state: &GameState) -> GameState {
    let restored = serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
    assert_eq!(&restored, state);
    for viewer in [PlayerId::P0, PlayerId::P1] {
        let facts = capture(state, viewer).unwrap();
        assert_eq!(capture(&restored, viewer).unwrap(), facts);
        validate(state, viewer, &facts).unwrap();
        assert_eq!(
            serde_json::from_slice::<FactsV67>(&serde_json::to_vec(&facts).unwrap()).unwrap(),
            facts
        );
    }
    restored
}
fn paid_in(mut state: GameState, actor: PlayerId) -> (GameState, ObjectId) {
    let source = put(&mut state, actor, NAME, Zone::Hand);
    let lands = (0..7)
        .map(|_| put(&mut state, actor, "Mountain", Zone::Battlefield))
        .collect::<Vec<_>>();
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
    );
    for &land in &lands {
        engine::step(&mut state, Action::ActivateManaAbility(land)).unwrap();
    }
    assert_eq!(
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()],
        7
    );
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&source))
    );
    engine::step(&mut state, Action::CastSpell(source)).unwrap();
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    let facts = capture(&state, actor).unwrap();
    assert_eq!(facts.casts.len(), 1);
    assert_eq!(facts.casts[0].source.object, source.0);
    assert_eq!(facts.casts[0].original_controller, actor.into());
    assert!(facts.casts[0].genuine_cast_finalized);
    assert!(!facts.casts[0].is_uncast_copy && !facts.casts[0].payment_pending);
    restored(&state);
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    assert!(lands.iter().all(|land| state.objects.get(*land).tapped));
    assert!(state.engine.event_history.iter().any(|event| matches!(event,
        CommittedEvent::SpellCast { spell, controller } if *spell == source && *controller == actor)));
    settle(&mut state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    assert!(state.objects.get(source).summoning_sick);
    (state, source)
}
fn attack(state: &mut GameState, source: ObjectId, actor: PlayerId) {
    let cast_turn = state.turn_key_v1();
    let generation = state.objects.get(source).zone_change_count;
    let mut saw_sick_declaration = false;
    for _ in 0..512 {
        let decision = next(state);
        if let Decision::DeclareAttackers { player, eligible } = &decision {
            if *player == actor {
                if state.turn_key_v1() == cast_turn {
                    assert!(!eligible.contains(&source));
                    saw_sick_declaration = true;
                } else if eligible.contains(&source) {
                    assert!(saw_sick_declaration);
                    engine::step(state, Action::DeclareAttackers(vec![source])).unwrap();
                    assert_eq!(state.engine.combat.attackers.as_slice(), &[source]);
                    assert!(state.engine.event_history.iter().any(|event| matches!(event,
                        CommittedEvent::DeclaredAttacker { source: actual, source_zone_change_count, controller }
                        if *actual == source && *source_zone_change_count == generation && *controller == actor)));
                    for _ in 0..16 {
                        let decision = next(state);
                        if matches!(decision, Decision::ChooseTargets { player, spell, remaining: 3, can_finish: false, .. }
                            if player == actor && spell == source)
                        {
                            return;
                        }
                        quiet(state, decision);
                    }
                    panic!("genuine attack did not publish mandatory first target");
                }
            }
        }
        quiet(state, decision);
    }
    panic!("genuinely cast source did not reach its next legal attack");
}
fn choose(state: &mut GameState, source: ObjectId, targets: &[Target]) {
    for (index, target) in targets.iter().enumerate() {
        assert!(
            matches!(next(state), Decision::ChooseTargets { spell, legal_targets, can_finish, .. }
            if spell == source && legal_targets.contains(target) && can_finish == (index != 0))
        );
        engine::step(state, Action::ChooseTarget(*target)).unwrap();
    }
    if targets.len() < 3 {
        assert!(matches!(
            next(state),
            Decision::ChooseTargets {
                can_finish: true,
                ..
            }
        ));
        engine::step(state, Action::FinishEffectSelection).unwrap();
    }
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    let row = state.stack.last().unwrap();
    assert_eq!(row.kind, StackItemKind::TriggeredAbility);
    assert_eq!(row.targets.as_slice(), targets);
    assert_eq!(row.v4.target_contracts.len(), targets.len());
    assert_eq!(
        row.inline_effect,
        Some(standard_cards_v1::drakuseth_attack_v1())
    );
    assert_eq!(
        row.v4.target_spec,
        Some(TargetSpec::StandardV1(
            crate::standard_cards_v1::StandardTargetV1::OneToThreeDistinctAnyTargetsV1
        ))
    );
}
fn damage_since(state: &GameState, start: usize, source: ObjectId) -> Vec<(Target, i32)> {
    state.engine.event_history[start..]
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::Damage {
                source: actual,
                target,
                amount,
            } if *actual == source => Some((*target, *amount)),
            _ => None,
        })
        .collect()
}

fn paid(actor: PlayerId) -> (GameState, ObjectId) {
    paid_in(crate::extra_turn_v1::tests::ready(actor), actor)
}
fn published(f: &FactsV67) -> &TriggerV67 {
    f.triggers
        .iter()
        .find(|r| r.stage == StageV67::Published)
        .unwrap()
}
fn priority(s: &mut GameState, actor: PlayerId) {
    for _ in 0..24 {
        let d = next(s);
        if matches!(d,Decision::CastSpellOrPass{player,..}if player==actor) {
            return;
        }
        quiet(s, d)
    }
    panic!("response priority absent")
}
fn response(
    s: &mut GameState,
    actor: PlayerId,
    name: &str,
    targets: &[Target],
    mana: usize,
) -> ObjectId {
    priority(s, actor);
    let spell = put(s, actor, name, Zone::Hand);
    let before = s.players[actor.index()]
        .battlefield
        .iter()
        .filter(|id| s.objects.get(**id).tapped)
        .count();
    engine::step(s, Action::CastSpell(spell)).unwrap();
    for target in targets {
        assert!(
            matches!(next(s),Decision::ChooseTargets{player,legal_targets,..}if player==actor&&legal_targets.contains(target))
        );
        engine::step(s, Action::ChooseTarget(*target)).unwrap();
    }
    assert!(matches!(next(s),Decision::CastSpellOrPass{player,..}if player==actor));
    assert!(s.engine.pending_cast.is_none());
    assert_eq!(
        s.players[actor.index()]
            .battlefield
            .iter()
            .filter(|id| s.objects.get(**id).tapped)
            .count()
            - before,
        mana
    );
    assert_eq!(s.players[actor.index()].mana_pool, [0; 6]);
    assert!(s
        .stack
        .iter()
        .any(|r| r.source == spell && r.kind == StackItemKind::Spell));
    spell
}
fn resolve_response(s: &mut GameState, spell: ObjectId) {
    for _ in 0..32 {
        let d = next(s);
        if s.objects.get(spell).zone == Zone::Graveyard && s.engine.pending_effect.is_none() {
            return;
        }
        quiet(s, d)
    }
    panic!("paid response did not resolve")
}
#[test]
fn complete_closed_printed_program_partial_and_unknown_fields_refuse() {
    let f = meaning::printed_meaning().unwrap();
    assert_eq!(f.card_id, 464);
    assert_eq!(f.printed.colored_mana, vec![ManaColor::R; 3]);
    assert_eq!(
        (
            f.printed.generic_mana,
            f.printed.mana_value,
            f.printed.power,
            f.printed.toughness
        ),
        (4, 7, 7, 7)
    );
    assert_eq!(card_def::CARD_DEFS[464].capability, CardCapability::Partial);
    assert!(card_def::preflight_fully_supported_deck(&[464]).is_err());
    assert!(
        !f.full_engine_admitted
            && !f.owning_consumer_admitted
            && !f.model_admitted
            && !f.sampler_admitted
    );
    let mut changed = f.clone();
    changed.ordered_rules.reverse();
    assert!(meaning::validate(&changed).is_err());
    let mut changed = f;
    changed.printed.colored_mana.pop();
    assert!(meaning::validate(&changed).is_err());
    let mut wire = serde_json::to_value(meaning::printed_meaning().unwrap()).unwrap();
    wire["execution_permission"] = true.into();
    assert!(serde_json::from_value::<meaning::MeaningV67>(wire).is_err());
}
#[test]
fn genuine_paid_attack_zero_one_two_other_slots_and_every_target_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for count in 1..=3 {
            let (mut s, source) = paid(actor);
            let first = put(&mut s, actor.opponent(), "Serra Angel", Zone::Battlefield);
            let second = put(&mut s, actor.opponent(), "Serra Angel", Zone::Battlefield);
            attack(&mut s, source, actor);
            let facts = capture(&s, actor).unwrap();
            let row = &facts.triggers[0];
            assert_eq!(row.stage, StageV67::ChoosingTargets);
            assert!(!row.mandatory_target_selected && !row.placement_complete);
            assert_eq!(row.matching_native_declarations, 1);
            let before = s.clone();
            assert!(engine::step(&mut s, Action::FinishEffectSelection).is_err());
            assert_eq!(s, before);
            let targets = [
                Target::Player(actor.opponent()),
                Target::Object(first),
                Target::Object(second),
            ];
            for target in &targets[..count] {
                let mut replay = restored(&s);
                engine::step(&mut s, Action::ChooseTarget(*target)).unwrap();
                engine::step(&mut replay, Action::ChooseTarget(*target)).unwrap();
                assert_eq!(s, replay);
                restored(&s);
                if s.engine
                    .pending_triggers
                    .first()
                    .is_some_and(|p| p.targets.len() < 3)
                {
                    let before = s.clone();
                    assert!(engine::step(&mut s, Action::ChooseTarget(*target)).is_err());
                    assert_eq!(s, before);
                }
            }
            if count < 3 {
                engine::step(&mut s, Action::FinishEffectSelection).unwrap();
            }
            assert!(matches!(next(&mut s), Decision::CastSpellOrPass { .. }));
            let facts = capture(&s, actor).unwrap();
            let row = published(&facts);
            assert!(row.mandatory_target_selected && row.placement_complete);
            assert_eq!(row.original_target_slots.len(), count);
            for (i, slot) in row.original_target_slots.iter().enumerate() {
                match slot {
                    TargetV67::Player {
                        slot,
                        damage,
                        currently_legal,
                        ..
                    }
                    | TargetV67::Object {
                        slot,
                        damage,
                        currently_legal,
                        ..
                    } => assert_eq!(
                        (*slot, *damage, *currently_legal),
                        (i as u32, if i == 0 { 4 } else { 3 }, true)
                    ),
                }
            }
            let start = s.engine.event_history.len();
            let mut replay = restored(&s);
            settle(&mut s);
            settle(&mut replay);
            assert_eq!(s, replay);
            assert_eq!(
                damage_since(&s, start, source),
                targets[..count]
                    .iter()
                    .enumerate()
                    .map(|(i, t)| (*t, if i == 0 { 4 } else { 3 }))
                    .collect::<Vec<_>>()
            );
            assert!(capture(&s, actor).unwrap().triggers.is_empty()); // No invented completion receipt.
        }
    }
}
#[test]
fn genuine_paid_bounce_or_control_response_keeps_original_source_controller_and_restore_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        for bounce in [true, false] {
            let (mut s, source) = paid(actor);
            let responder = actor.opponent();
            let host = put(&mut s, responder, "Llanowar Elves", Zone::Battlefield);
            put(&mut s, responder, "High Fae Trickster", Zone::Battlefield);
            for _ in 0..4 {
                put(&mut s, responder, "Mountain", Zone::Battlefield);
            }
            for _ in 0..2 {
                put(&mut s, responder, "Island", Zone::Battlefield);
            }
            attack(&mut s, source, actor);
            choose(&mut s, source, &[Target::Player(responder)]);
            let original = capture(&s, actor).unwrap();
            let original = published(&original).clone();
            let spell = if bounce {
                response(
                    &mut s,
                    responder,
                    "Run Away Together",
                    &[Target::Object(source), Target::Object(host)],
                    2,
                )
            } else {
                response(
                    &mut s,
                    responder,
                    "Involuntary Employment",
                    &[Target::Object(source)],
                    4,
                )
            };
            let mut replay = restored(&s);
            for b in [&mut s, &mut replay] {
                resolve_response(b, spell);
                let f = capture(b, actor).unwrap();
                let r = published(&f);
                assert_eq!(r.source.captured_controller, actor.into());
                assert_eq!(r.source.original_source, original.source.original_source);
                assert_eq!(r.original_target_slots, original.original_target_slots);
                if bounce {
                    assert!(r.source.current_original_source.is_none());
                    assert_eq!(b.objects.get(source).zone, Zone::Hand);
                } else {
                    assert_eq!(
                        r.source
                            .current_original_source
                            .as_ref()
                            .unwrap()
                            .controller,
                        responder.into()
                    );
                    assert_eq!(b.objects.get(source).controller, responder);
                }
                let start = b.engine.event_history.len();
                settle(b);
                assert_eq!(
                    damage_since(b, start, source),
                    vec![(Target::Player(responder), 4)]
                );
            }
            assert_eq!(s, replay);
        }
    }
}
#[test]
fn genuine_paid_target_bounce_reentry_keeps_illegal_first_slot_and_three_for_other_slot_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source) = paid(actor);
        let first = put(&mut s, actor.opponent(), "Serra Angel", Zone::Battlefield);
        let support = put(&mut s, actor, "Llanowar Elves", Zone::Battlefield);
        for _ in 0..2 {
            put(&mut s, actor.opponent(), "Island", Zone::Battlefield);
        }
        attack(&mut s, source, actor);
        choose(
            &mut s,
            source,
            &[Target::Object(first), Target::Player(actor.opponent())],
        );
        let generation = s.objects.get(first).zone_change_count;
        let spell = response(
            &mut s,
            actor.opponent(),
            "Run Away Together",
            &[Target::Object(first), Target::Object(support)],
            2,
        );
        resolve_response(&mut s, spell);
        // Labelled native reentry primitive after the genuine paid bounce response.
        event::propose_and_commit(&mut s, ProposedEvent::zone_change(first, Zone::Battlefield));
        let f = capture(&s, actor).unwrap();
        let r = published(&f);
        assert!(
            matches!(&r.original_target_slots[0],TargetV67::Object{slot:0,generation:g,damage:4,currently_legal:false,current_original_target:None,..}if *g==generation)
        );
        assert!(matches!(
            &r.original_target_slots[1],
            TargetV67::Player {
                slot: 1,
                damage: 3,
                currently_legal: true,
                ..
            }
        ));
        let start = s.engine.event_history.len();
        let mut replay = restored(&s);
        settle(&mut s);
        settle(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(
            damage_since(&s, start, source),
            vec![(Target::Player(actor.opponent()), 3)]
        );
    }
}
#[test]
fn genuine_paid_native_origin_target_and_complete_facts_forgeries_refuse_readonly_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source) = paid(actor);
        attack(&mut s, source, actor);
        choose(&mut s, source, &[Target::Player(actor.opponent())]);
        let f = capture(&s, actor).unwrap();
        for edit in 0..6 {
            let mut bad = restored(&s);
            match edit {
                0 => bad.stack.last_mut().unwrap().controller = actor.opponent(),
                1 => {
                    bad.stack
                        .last_mut()
                        .unwrap()
                        .v4
                        .ability_source_contract
                        .as_mut()
                        .unwrap()
                        .zone_change_count += 1
                }
                2 => bad.stack.last_mut().unwrap().v4.target_contracts.clear(),
                3 => bad.stack.last_mut().unwrap().inline_effect = Some(EffectOp::Sequence(vec![])),
                4 => bad.engine.event_history.retain(
                    |e| !matches!(e,CommittedEvent::DeclaredAttacker{source:id,..}if *id==source),
                ),
                _ => {
                    let r = bad.stack.last_mut().unwrap();
                    r.targets.push(r.targets[0]);
                    r.v4.target_contracts.push(r.v4.target_contracts[0]);
                }
            }
            let before = serde_json::to_vec(&bad).unwrap();
            assert!(capture(&bad, actor).is_err());
            assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
        }
        let before = s.clone();
        let mut changed = f.clone();
        changed.triggers[0].matching_native_declarations += 1;
        assert!(validate(&s, actor, &changed).is_err());
        let mut changed = f.clone();
        changed.triggers[0].source.captured_controller = actor.opponent().into();
        assert!(validate(&s, actor, &changed).is_err());
        let mut wire = serde_json::to_value(&f).unwrap();
        wire["triggers"][0]["claimed_completion"] = true.into();
        assert!(serde_json::from_value::<FactsV67>(wire).is_err());
        assert_eq!(s, before);
    }
}
#[test]
fn genuine_paid_all17_counter_widths_and_neutral_copy_suppression_primitives_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source) = paid(actor);
        // Explicit storage/form operands on genuinely paid native history, not paid counter producers.
        let land = put(&mut s, actor, "Forest", Zone::Battlefield);
        let o = s.objects.get_mut(land);
        o.counters = Counters {
            plus1_plus1: i32::MAX,
            minus1_minus1: i16::MAX,
            minus0_minus1: i16::MAX,
            stun: i16::MAX,
            lore: i16::MAX,
            oil: i16::MAX,
            charge: i32::MAX,
            net: i32::MAX,
            bait: i32::MAX,
            soul: i32::MAX,
            incubation: i32::MAX,
            revival: i32::MAX,
            fellowship: i32::MAX,
            stash: 0,
        };
        o.v4.lifelink_keyword_counters = i16::MAX;
        o.v4.time_counters_v1 = u8::MAX;
        o.v4.creature_upgrade
            .get_or_insert_with(Default::default)
            .finality = u16::MAX;
        let ajani = put(
            &mut s,
            actor,
            "Ajani, Caller of the Pride",
            Zone::Battlefield,
        );
        let old = crate::planeswalker_v1::loyalty(&s, ajani).unwrap_or(0);
        crate::planeswalker_v1::add_loyalty_counters_checked_v1(&mut s, ajani, u32::MAX - old)
            .unwrap();
        let f = capture(&s, actor).unwrap();
        let c = &f.battlefield[actor.index()]
            .iter()
            .find(|r| r.card.object == land.0)
            .unwrap()
            .counters;
        assert_eq!(
            (
                c.plus_one,
                c.charge,
                c.net,
                c.bait,
                c.soul,
                c.incubation,
                c.revival,
                c.fellowship
            ),
            (
                i32::MAX,
                i32::MAX,
                i32::MAX,
                i32::MAX,
                i32::MAX,
                i32::MAX,
                i32::MAX,
                i32::MAX
            )
        );
        assert_eq!(
            (
                c.minus_one,
                c.minus_toughness,
                c.stun,
                c.lore,
                c.oil,
                c.lifelink
            ),
            (i16::MAX, i16::MAX, i16::MAX, i16::MAX, i16::MAX, i16::MAX)
        );
        assert_eq!((c.time, c.finality), (u8::MAX, u16::MAX));
        assert_eq!(
            f.battlefield[actor.index()]
                .iter()
                .find(|r| r.card.object == ajani.0)
                .unwrap()
                .counters
                .loyalty,
            u32::MAX
        );
        restored(&s);
        s.objects.get_mut(land).counters.charge = -1;
        let before = s.clone();
        assert!(capture(&s, actor).is_err());
        assert_eq!(s, before);
        s.objects.get_mut(land).counters.charge = 0;
        // Genuine native copy primitive. This is not a paid copy-spell claim.
        let mut copied = restored(&s);
        let token =
            standard_cards_v1::create_token_copy(&mut copied, source, actor.opponent()).unwrap();
        let f = capture(&copied, actor).unwrap();
        let r = f.battlefield[actor.opponent().index()]
            .iter()
            .find(|r| r.card.object == token.0)
            .unwrap();
        assert!(r.is_token);
        assert_ne!(r.card.object, source.0);
        assert_eq!((r.form.power, r.form.toughness), (Some(7), Some(7)));
        assert!(r.form.keyword_bits & Keywords::FLYING.0 != 0);
        restored(&copied);
        // Explicit native suppression operand, not an ability of this support printing.
        let mut suppressed = restored(&s);
        let suppressor = put(
            &mut suppressed,
            actor.opponent(),
            "High Fae Trickster",
            Zone::Battlefield,
        );
        let link = crate::state::ObjectLinkV4 {
            object: suppressor,
            zone_change_count: suppressed.objects.get(suppressor).zone_change_count,
        };
        let timestamp = engine::next_timestamp(&mut suppressed);
        suppressed
            .objects
            .get_mut(source)
            .v4
            .creature_upgrade
            .get_or_insert_with(Default::default)
            .suppressed_by
            .push(crate::standard_creatures_v1::TidebinderSuppressionV1 {
                source: link,
                timestamp,
            });
        let f = capture(&suppressed, actor).unwrap();
        let form = &f.battlefield[actor.index()]
            .iter()
            .find(|r| r.card.object == source.0)
            .unwrap()
            .form;
        assert!(!form.printed_abilities_active);
        assert_eq!(form.keyword_bits & Keywords::FLYING.0, 0);
        restored(&suppressed);
        event::propose_and_commit(
            &mut suppressed,
            ProposedEvent::zone_change(suppressor, Zone::Hand),
        );
        let f = capture(&suppressed, actor).unwrap();
        let form = &f.battlefield[actor.index()]
            .iter()
            .find(|r| r.card.object == source.0)
            .unwrap()
            .form;
        assert!(form.printed_abilities_active);
        assert!(form.keyword_bits & Keywords::FLYING.0 != 0);
        restored(&suppressed);
        s.objects.get_mut(source).v4.face_down_v1 = Some(FaceDownV1 {
            disguised: true,
            lookers: 0,
            hidden_by: None,
            hidden_by_source: None,
        });
        assert_eq!(engine::checked_effective_pt(&s, source).unwrap(), (2, 2));
        let f = capture(&s, actor).unwrap();
        let form = &f.battlefield[actor.index()]
            .iter()
            .find(|r| r.card.object == source.0)
            .unwrap()
            .form;
        assert_eq!((form.power, form.toughness), (Some(2), Some(2)));
        assert!(!form.printed_abilities_active);
        assert_eq!(form.keyword_bits, 0);
        assert!(!f.authorized_printings.iter().any(|r| r.object == source.0));
        restored(&s);
    }
}
#[test]
fn independent_initial_hidden_worlds_before_genuine_paid_attack_keep_public_facts_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let build = |unknown: &str| {
            let mut s = crate::extra_turn_v1::tests::ready(actor);
            let secret = put(&mut s, actor.opponent(), unknown, Zone::Hand);
            let hidden = put(&mut s, actor.opponent(), unknown, Zone::Battlefield);
            s.objects.get_mut(hidden).v4.face_down_v1 = Some(FaceDownV1 {
                disguised: false,
                lookers: 0,
                hidden_by: None,
                hidden_by_source: None,
            });
            let id = card_def::card_id_by_name(unknown).unwrap();
            for &card in &s.players[actor.opponent().index()].library.clone() {
                s.objects.get_mut(card).card_def = id;
            }
            let (mut s, source) = paid_in(s, actor);
            attack(&mut s, source, actor);
            choose(&mut s, source, &[Target::Player(actor.opponent())]);
            (s, secret, hidden)
        };
        let (a, secret, hidden) = build(NAME);
        let (b, other_secret, other_hidden) = build("Island");
        assert_eq!((secret, hidden), (other_secret, other_hidden));
        assert_eq!(capture(&a, actor).unwrap(), capture(&b, actor).unwrap());
        restored(&a);
        restored(&b);
        let own = capture(&a, actor.opponent()).unwrap();
        assert!(own
            .authorized_printings
            .iter()
            .any(|r| r.object == secret.0));
        assert!(!own
            .authorized_printings
            .iter()
            .any(|r| r.object == hidden.0));
    }
}
#[test]
fn genuine_paid_crystal_does_not_prevent_damage_to_its_controllers_opponent_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source) = paid(actor);
        put(
            &mut s,
            actor.opponent(),
            "Crystal Barricade",
            Zone::Battlefield,
        );
        attack(&mut s, source, actor);
        choose(&mut s, source, &[Target::Player(actor)]);
        let row = s.stack.last().unwrap().clone();
        let facts = capture(&s, actor).unwrap();
        assert_eq!(published(&facts).stage, StageV67::Published);
        let start = s.engine.event_history.len();
        let mut replay = restored(&s);
        for branch in [&mut s, &mut replay] {
            settle(branch);
            assert!(branch.engine.halted.is_none());
            assert!(!branch.stack.contains(&row));
            assert_eq!(
                damage_since(branch, start, source),
                vec![(Target::Player(actor), 4)]
            );
            assert_eq!(branch.players[actor.index()].life, 16);
            assert_eq!(branch.players[actor.opponent().index()].life, 20);
            assert!(capture(branch, actor).unwrap().triggers.is_empty());
        }
        assert_eq!(s, replay);
    }
}

#[test]
fn genuine_paid_mixed_combat_held_and_ordered_native_positions_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source) = paid(actor);
        // Ordinary second attacker support makes a genuine simultaneous trigger order choice.
        let other = put(&mut s, actor, "Ashroot Animist", Zone::Battlefield);
        let cast_turn = s.turn_key_v1();
        let mut declared = false;
        for _ in 0..512 {
            let d = next(&mut s);
            if matches!(&d,Decision::DeclareAttackers{player,eligible}if *player==actor&&s.turn_key_v1()!=cast_turn&&eligible.contains(&source)&&eligible.contains(&other))
            {
                engine::step(&mut s, Action::DeclareAttackers(vec![source, other])).unwrap();
                declared = true;
                break;
            }
            quiet(&mut s, d);
        }
        assert!(declared);
        assert!(
            matches!(next(&mut s),Decision::OrderTriggers{player,ref pending}if player==actor&&pending.len()==2)
        );
        let position = s
            .engine
            .pending_triggers
            .iter()
            .position(|p| p.source == source)
            .unwrap();
        let f = capture(&s, actor).unwrap();
        assert_eq!(f.triggers.len(), 1);
        let row = &f.triggers[0];
        assert_eq!(row.stage, StageV67::Held);
        assert_eq!(row.native_position, position as u32);
        assert_eq!(row.matching_native_declarations, 1);
        assert!(row.original_target_slots.is_empty());
        let mut copy = restored(&s);
        let order = vec![position, 1 - position];
        for b in [&mut s, &mut copy] {
            engine::step(b, Action::OrderTriggers(order.clone())).unwrap();
            assert!(matches!(next(b),Decision::ChooseTargets{spell,..}if spell==source));
            let f = capture(b, actor).unwrap();
            let row = &f.triggers[0];
            assert_eq!(row.stage, StageV67::ChoosingTargets);
            assert_eq!(row.native_position, 0);
            assert!(!row.placement_complete);
        }
        assert_eq!(s, copy);
        restored(&s);
    }
}
