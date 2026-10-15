use super::*;
use crate::card_def::card_id_by_name;
use crate::state::{GameObject, ObjectStateV4, Step};

// Explicit primitive inputs matching the pinned Elenda static text. These are
// never installed on a different CARD_DEFS entry or treated as a printed card.
const RULES: &[StartingLifeSelfBonusDefV1] = &[
    StartingLifeSelfBonusDefV1 {
        minimum_above_start: 1,
        power: 1,
        toughness: 1,
        keywords: Keywords::MENACE,
    },
    StartingLifeSelfBonusDefV1 {
        minimum_above_start: 10,
        power: 5,
        toughness: 5,
        keywords: Keywords::NONE,
    },
];

fn expected(delta: i32, menace: bool) -> StartingLifeSelfBonusV1 {
    StartingLifeSelfBonusV1 {
        power: delta,
        toughness: delta,
        keywords: if menace {
            Keywords::MENACE
        } else {
            Keywords::NONE
        },
    }
}

fn ready() -> GameState {
    let island = card_id_by_name("Island").unwrap();
    let mut state =
        GameState::new_from_libraries(&[island; 40], &[island; 40], |_| "Island".into(), 110119);
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[usize::from(card_def)].object_name.into(),
        owner,
        controller: owner,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[owner.index()].hand.push(id),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
        _ => panic!("test fixture zone"),
    }
    id
}

#[test]
fn additive_thresholds_include_exact_boundary_and_extreme_life() {
    assert_eq!(STARTING_LIFE, 20);
    for (life, delta, menace) in [
        (i32::MIN, 0, false),
        (-1, 0, false),
        (19, 0, false),
        (20, 0, false),
        (21, 1, true),
        (29, 1, true),
        (30, 6, true),
        (31, 6, true),
        (i32::MAX, 6, true),
    ] {
        assert_eq!(
            bonus_for_life(RULES, life),
            expected(delta, menace),
            "life={life}"
        );
    }
    assert_eq!(bonus_for_life(&[], i32::MAX), expected(0, false));
}

#[test]
fn primitive_reads_current_controller_and_recomputes_downward_after_restore() {
    for owner in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready();
        let context = put(&mut state, owner, "Voldaren Epicure", Zone::Battlefield);
        let original = state.objects.get(context).clone();
        for (life, delta, menace) in [
            (20, 0, false),
            (21, 1, true),
            (29, 1, true),
            (30, 6, true),
            (29, 1, true),
            (20, 0, false),
        ] {
            state.players[owner.index()].life = life;
            let before = state.state_hash();
            let facts = facts_for_rules(&state, context, RULES).unwrap();
            assert_eq!(facts.controller, owner);
            assert_eq!(facts.starting_life, 20);
            assert_eq!(facts.controller_life, life);
            assert!(facts.printed_abilities_active);
            assert_eq!(facts.rules, RULES);
            assert_eq!(facts.bonus, expected(delta, menace));
            let bytes = serde_json::to_vec(&state).unwrap();
            let restored: GameState = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(facts_for_rules(&restored, context, RULES), Some(facts));
            assert_eq!(before, state.state_hash());
            assert_eq!(before, restored.state_hash());
            assert_eq!(state.objects.get(context), &original);
            // The actual Epicure definition never acquires the supplied rules.
            assert!(characteristics(&state, context).is_none());
            assert_eq!(crate::engine::effective_power(&state, context), 1);
            assert!(!crate::engine::has_effective_keyword(
                &state,
                context,
                Keywords::MENACE
            ));
        }

        // State-input test of the primitive's current-controller reader. No
        // injected spell or invented Elenda control-change receipt is used.
        let other = owner.opponent();
        state.players[owner.index()].life = 30;
        state.players[other.index()].life = 20;
        state.players[owner.index()]
            .battlefield
            .retain(|&id| id != context);
        state.players[other.index()].battlefield.push(context);
        state.objects.get_mut(context).controller = other;
        let facts = facts_for_rules(&state, context, RULES).unwrap();
        assert_eq!(facts.controller, other);
        assert_eq!(facts.bonus, expected(0, false));
        assert_eq!(state.objects.get(context).owner, owner);
        state.players[other.index()].life = 30;
        assert_eq!(
            facts_for_rules(&state, context, RULES).unwrap().bonus,
            expected(6, true)
        );
    }
}

#[test]
fn nonbattlefield_and_foreign_face_never_apply() {
    let mut state = ready();
    state.players[0].life = 30;
    let context = put(&mut state, PlayerId::P0, "Voldaren Epicure", Zone::Hand);
    assert!(facts_for_rules(&state, context, RULES).is_none());
    state.objects.get_mut(context).zone = Zone::Battlefield;
    state.objects.get_mut(context).v4.face_index = 1;
    assert!(facts_for_rules(&state, context, RULES).is_none());
    state.objects.get_mut(context).v4.face_index = 0;
    assert_eq!(
        facts_for_rules(&state, context, RULES).unwrap().bonus,
        expected(6, true)
    );
    assert!(facts_for_rules(&state, context, &[]).is_none());
}

#[test]
fn source_preparation_leaves_every_catalog_descriptor_empty() {
    assert!(card_id_by_name("Elenda, Saint of Dusk").is_none());
    assert!(CARD_DEFS
        .iter()
        .all(|def| def.starting_life_self_bonuses_v1.is_empty()));
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn primitive_suppression_follows_real_paid_witness_and_restores_after_departure() {
    use crate::engine::{self, Action, Decision};
    use crate::event::{self, ProposedEvent};
    use crate::state::Target;

    let mut state = ready();
    state.players[0].life = 30;
    let context = put(
        &mut state,
        PlayerId::P0,
        "Voldaren Epicure",
        Zone::Battlefield,
    );
    let aura = put(&mut state, PlayerId::P0, "Witness Protection", Zone::Hand);
    assert_eq!(
        facts_for_rules(&state, context, RULES).unwrap().bonus,
        expected(6, true)
    );
    state.players[0].mana_pool = [5; 6];
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ));
    engine::step(&mut state, Action::CastSpell(aura)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseTargets { .. }
    ));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(context))).unwrap();
    for _ in 0..30 {
        match engine::advance_until_decision(&mut state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            other => panic!("unexpected Witness decision: {other:?}"),
        }
    }
    assert_eq!(state.objects.get(aura).zone, Zone::Battlefield);
    assert_eq!(
        state.objects.get(aura).v4.attached_to.unwrap().object,
        context
    );
    let facts = facts_for_rules(&state, context, RULES).unwrap();
    assert!(!facts.printed_abilities_active);
    assert_eq!(facts.rules, RULES);
    assert_eq!(facts.bonus, expected(0, false));
    let restored: GameState = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(facts_for_rules(&restored, context, RULES), Some(facts));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(aura, Zone::Graveyard),
    );
    assert_eq!(
        facts_for_rules(&state, context, RULES).unwrap().bonus,
        expected(6, true)
    );
    // The recipient remains the real 1/1 Epicure throughout this primitive
    // probe. Actual Elenda casting, combat and lifelink remain pending.
    assert_eq!(engine::effective_power(&state, context), 1);
}
