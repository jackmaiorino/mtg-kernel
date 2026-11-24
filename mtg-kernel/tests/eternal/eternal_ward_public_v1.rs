//! Limited schema 5 (the public shroud contract) follows Ward payments: its
//! policy observation reports each pending or queued Ward payment
//! (`ward_payments`) with the Ward source, the exact targeting stack item,
//! the payer and the typed cost. Schema 4 keeps refusing the states the
//! legacy observation cannot represent: a life-payment Ward (Sedgemoor
//! Witch) and a generic Ward whose source two live stack items target
//! (Kappa Cannoneer).
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::CARD_DEFS;
use mtg_kernel::limited_session_v1::LimitedJsonlServerV1;
use serde_json::{json, Value};

const MAX_STEPS: usize = 3000;

fn deck(land: &str, card: &str) -> Value {
    json!({"cards": [{"name": land, "count": 17}, {"name": card, "count": 23}]})
}

fn name_of(card: &Value) -> Option<&'static str> {
    card["card_db_id"]
        .as_u64()
        .map(|id| CARD_DEFS[id as usize].name)
}

fn other(seat: &str) -> &'static str {
    if seat == "p0" {
        "p1"
    } else {
        "p0"
    }
}

/// Whether `seat` controls a battlefield permanent named `name`.
fn controls(decision: &Value, seat: &str, name: &str) -> bool {
    decision["observation"]["projection"]["battlefield"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|cards| cards.as_array().into_iter().flatten())
        .any(|card| card["stable"]["controller"] == seat && name_of(&card["stable"]) == Some(name))
}

fn stack(decision: &Value) -> &Vec<Value> {
    decision["observation"]["projection"]["stack"]
        .as_array()
        .unwrap()
}

/// The public stack rows whose targets include a card named `name`.
fn stack_rows_targeting(decision: &Value, name: &str) -> Vec<u64> {
    stack(decision)
        .iter()
        .filter(|row| {
            row["targets"].as_array().unwrap().iter().any(|target| {
                target["target_kind"] == "object" && name_of(&target["object"]) == Some(name)
            })
        })
        .map(|row| row["stack_index"].as_u64().unwrap())
        .collect()
}

/// A scripted game: the warded deck plays its lands and casts its card; the
/// burn deck holds its spells until the warded card is on the opposing
/// battlefield, then casts them at it (as often as it can), and answers
/// every Ward payment with `pay`.
struct Script {
    warded: &'static str,
    burn: &'static str,
    pay: bool,
}

impl Script {
    fn rank(&self, decision: &Value, action: &Value) -> u8 {
        let s = &action["semantic"];
        let actor = s["actor"].as_str().unwrap_or("p0");
        match s["action_kind"].as_str().unwrap() {
            "choose_london_mulligan" if s["mulligan"] == false => 0,
            "choose_effect_boolean" if s["value"] == self.pay => 0,
            "choose_effect_boolean" => 20,
            "choose_target" | "choose_effect_target"
                if s["target"]["target_kind"] == "object"
                    && name_of(&s["target"]["object"]) == Some(self.warded) =>
            {
                0
            }
            "choose_target" | "choose_effect_target" => 6,
            "play_land" => 0,
            "cast_spell" if name_of(&s["source"]) == Some(self.burn) => {
                if controls(decision, other(actor), self.warded) {
                    1
                } else {
                    30
                }
            }
            "cast_spell" => 1,
            "choose_attacker_inclusion" if s["include"] == false => 2,
            "pass" => 10,
            "activate_mana_ability" | "activate_ability" => 11,
            _ => 5,
        }
    }
}

struct Outcome {
    /// Every decision, in order.
    decisions: Vec<Value>,
    /// The action taken at each decision.
    actions: Vec<Value>,
    /// The terminal reason, if the game ended within the step bound.
    terminal_reason: Option<String>,
}

fn play(schema: u32, decks: [Value; 2], script: &Script) -> Outcome {
    let mut server = match schema {
        4 => LimitedJsonlServerV1::new_with_london_mulligans_v1(),
        5 => LimitedJsonlServerV1::new_with_public_shroud_v1(),
        _ => unreachable!(),
    };
    let mut request = json!({"schema_version": schema, "request_type": "reset",
        "request_id": "reset", "decks": decks,
        "episode_id": 11, "env_seed": 0x5741_5244_u64,
        "max_physical_decisions": 65536, "max_policy_steps": MAX_STEPS})
    .to_string();
    let mut outcome = Outcome {
        decisions: Vec::new(),
        actions: Vec::new(),
        terminal_reason: None,
    };
    for index in 0..MAX_STEPS {
        let reply: Value = serde_json::from_str(&server.handle_line(&request)).unwrap();
        assert_ne!(reply["response_type"], "error", "step {index}: {reply}");
        if reply["response_type"] == "terminal" {
            outcome.terminal_reason = reply["terminal"]["terminal_reason"]
                .as_str()
                .map(str::to_owned);
            return outcome;
        }
        let decision = reply["decision"].clone();
        let action = decision["legal_actions"]
            .as_array()
            .unwrap()
            .iter()
            .min_by_key(|action| script.rank(&decision, action))
            .unwrap()
            .clone();
        request = json!({"schema_version": schema, "request_type": "step",
            "request_id": format!("step-{index}"), "episode_id": decision["episode_id"],
            "expected_step": decision["step"], "selected_index": action["selected_index"],
            "selected_action_id": action["stable_id"]})
        .to_string();
        outcome.decisions.push(decision);
        outcome.actions.push(action);
    }
    outcome
}

fn assert_followed(outcome: &Outcome) {
    if let Some(reason) = &outcome.terminal_reason {
        assert!(!reason.starts_with("fail_closed"), "{reason}");
    }
}

fn ward_payments(decision: &Value) -> &Value {
    &decision["observation"]["projection"]["ward_payments"]
}

/// Checks one Ward payment record against the public stack: its source is
/// the warded card, its targeting stack item targets that card and is
/// controlled by the payer, and its cost is `cost`.
fn check_payment(decision: &Value, payment: &Value, warded: &str, cost: &Value) {
    assert_eq!(name_of(&payment["ward_source"]), Some(warded), "{payment}");
    let index = payment["targeting_stack_index"].as_u64().unwrap();
    let row = stack(decision)
        .iter()
        .find(|row| row["stack_index"] == index)
        .unwrap_or_else(|| panic!("no stack row {index}: {payment}"));
    assert!(stack_rows_targeting(decision, warded).contains(&index));
    assert_eq!(row["controller"], payment["payer"]);
    assert_ne!(payment["payer"], payment["ward_source"]["controller"]);
    assert_eq!(&payment["cost"], cost);
}

#[test]
fn schema_5_follows_a_life_payment_ward_that_schema_4_refuses() {
    let decks = || {
        [
            deck("Swamp", "Sedgemoor Witch"),
            deck("Mountain", "Lightning Bolt"),
        ]
    };
    let life = json!({"kind": "life", "amount": 3});
    for pay in [true, false] {
        let script = Script {
            warded: "Sedgemoor Witch",
            burn: "Lightning Bolt",
            pay,
        };
        let public = play(5, decks(), &script);
        assert_followed(&public);
        let mut queued_seen = false;
        let mut answered = 0;
        for (index, decision) in public.decisions.iter().enumerate() {
            let payments = ward_payments(decision);
            for queued in payments["queued"].as_array().into_iter().flatten() {
                let trigger = queued["trigger_stack_index"].as_u64().unwrap();
                let row = stack(decision)
                    .iter()
                    .find(|row| row["stack_index"] == trigger)
                    .unwrap();
                assert_eq!(row["stack_item_kind"], "triggered_ability");
                assert_eq!(row["source"], queued["payment"]["ward_source"]);
                check_payment(decision, &queued["payment"], "Sedgemoor Witch", &life);
                queued_seen = true;
            }
            let action = &public.actions[index];
            if action["semantic"]["action_kind"] != "choose_effect_boolean" {
                continue;
            }
            // The payer chooses between paying and declining.
            let pending = &payments["pending"];
            check_payment(decision, pending, "Sedgemoor Witch", &life);
            let values: Vec<&Value> = decision["legal_actions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|action| &action["semantic"]["value"])
                .collect();
            assert_eq!(values, [&json!(false), &json!(true)]);
            let payer = usize::from(pending["payer"] == "p1");
            let targeter = pending["targeting_stack_index"].as_u64().unwrap();
            let life_before = decision["observation"]["projection"]["life_totals"][payer]
                .as_i64()
                .unwrap();
            let after = &public.decisions[index + 1];
            let life_after = after["observation"]["projection"]["life_totals"][payer]
                .as_i64()
                .unwrap();
            let graveyard = &after["observation"]["projection"]["graveyards"][payer];
            let bolts_before = decision["observation"]["projection"]["graveyards"][payer]
                .as_array()
                .unwrap()
                .len();
            if pay {
                // Paying 3 life lets the Bolt resolve.
                assert_eq!(life_after, life_before - 3);
            } else {
                // Declining counters the Bolt: no life is paid, and the
                // countered Bolt goes to its owner's graveyard.
                assert_eq!(life_after, life_before);
                assert!(graveyard.as_array().unwrap().len() > bolts_before);
                assert!(!stack_rows_targeting(after, "Sedgemoor Witch").contains(&targeter));
            }
            answered += 1;
        }
        assert!(queued_seen, "pay={pay}: no queued life Ward");
        assert!(answered > 0, "pay={pay}: no life Ward answered");
        // No Ward record outside Ward states.
        for decision in &public.decisions {
            if stack_rows_targeting(decision, "Sedgemoor Witch").is_empty() {
                assert!(
                    ward_payments(decision).is_null(),
                    "{}",
                    ward_payments(decision)
                );
            }
        }

        let legacy = play(4, decks(), &script);
        assert_eq!(
            legacy.terminal_reason.as_deref(),
            Some("fail_closed:observation:life-payment Ward requires its typed observation successor")
        );
        for decision in &legacy.decisions {
            assert!(!decision.to_string().contains("ward_payments"));
        }
    }
}

#[test]
fn schema_5_binds_each_generic_ward_trigger_when_two_spells_target_its_source() {
    let decks = || {
        [
            deck("Island", "Kappa Cannoneer"),
            deck("Mountain", "Lightning Bolt"),
        ]
    };
    let script = Script {
        warded: "Kappa Cannoneer",
        burn: "Lightning Bolt",
        pay: false,
    };
    let generic = json!({"kind": "generic", "amount": 4});
    let public = play(5, decks(), &script);
    assert_followed(&public);
    let mut doubly_targeted = 0;
    for decision in &public.decisions {
        let targeters = stack_rows_targeting(decision, "Kappa Cannoneer");
        let queued = ward_payments(decision)["queued"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for record in &queued {
            check_payment(decision, &record["payment"], "Kappa Cannoneer", &generic);
            // Each trigger sits directly above the spell it is bound to.
            assert_eq!(
                record["trigger_stack_index"].as_u64().unwrap(),
                record["payment"]["targeting_stack_index"].as_u64().unwrap() + 1
            );
        }
        if targeters.len() >= 2 && queued.len() >= 2 {
            let mut bound: Vec<u64> = queued
                .iter()
                .map(|record| record["payment"]["targeting_stack_index"].as_u64().unwrap())
                .collect();
            bound.sort_unstable();
            bound.dedup();
            assert_eq!(bound.len(), queued.len(), "one trigger per targeter");
            doubly_targeted += 1;
        }
    }
    assert!(doubly_targeted > 0, "no position with two targeting spells");

    let legacy = play(4, decks(), &script);
    assert_eq!(
        legacy.terminal_reason.as_deref(),
        Some(
            "fail_closed:observation:legacy observation cannot identify the Ward-bound stack item unambiguously; the Ward observation successor is required"
        )
    );
}

// ---- Engine positions through the public policy surface ----------------------

mod positions {
    use super::name_of;
    use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
    use mtg_kernel::effect::EffectBooleanChoicePurpose;
    use mtg_kernel::engine::{self, Action, Decision};
    use mtg_kernel::event::{propose_and_commit, ProposedEvent};
    use mtg_kernel::ids::{ObjectId, PlayerId};
    use mtg_kernel::mana::ManaColor;
    use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
    use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
    use serde_json::{json, Value};

    const P0: PlayerId = PlayerId::P0;
    const P1: PlayerId = PlayerId::P1;

    fn id(name: &str) -> u16 {
        card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
    }

    /// P0's first main phase under the schema 5 public contract, P0 holding
    /// priority.
    fn game() -> GameState {
        let ids = vec![id("Island"); 20];
        let mut state = GameState::new_from_libraries(
            &ids,
            &ids,
            |id| CARD_DEFS[id as usize].object_name.into(),
            0x5741_5244_0001,
        );
        state.active_player = P0;
        state.priority_player = P0;
        state.step = Step::Main1;
        mtg_kernel::rl::enable_public_shroud_v1(&mut state);
        state
    }

    fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
        let card_def = id(name);
        let object = state.objects.push(GameObject {
            card_def,
            name: CARD_DEFS[card_def as usize].object_name.into(),
            owner: player,
            controller: player,
            zone,
            tapped: false,
            summoning_sick: false,
            damage: 0,
            counters: Default::default(),
            attachments: Vec::new(),
            v4: ObjectStateV4::from_card_def(card_def),
            spell_copy_origin: None,
            plotted_turn: None,
            zone_change_count: 0,
        });
        let seat = &mut state.players[player.index()];
        match zone {
            Zone::Hand => seat.hand.push(object),
            Zone::Battlefield => seat.battlefield.push(object),
            _ => panic!("helper zone"),
        }
        object
    }

    fn pool(state: &mut GameState, player: PlayerId, colored: &[ManaColor], colorless: u8) {
        let pool = &mut state.players[player.index()].mana_pool;
        for color in colored {
            pool[color.pool_index()] += 1;
        }
        pool[ManaColor::C.pool_index()] += colorless;
    }

    fn next(state: &mut GameState) -> Decision {
        let decision = engine::advance_until_decision(state);
        assert!(
            !matches!(decision, Decision::Halted { .. }),
            "{decision:?}\nstack: {:#?}",
            state.stack
        );
        decision
    }

    fn act(state: &mut GameState, action: Action) {
        engine::step(state, action.clone()).unwrap_or_else(|error| panic!("{action:?}: {error}"));
    }

    /// Passes priority and orders triggers until `stop` holds at a decision.
    fn run_until(state: &mut GameState, stop: impl Fn(&GameState, &Decision) -> bool) -> Decision {
        for _ in 0..200 {
            let decision = next(state);
            if stop(state, &decision) {
                return decision;
            }
            match decision {
                Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
                Decision::OrderTriggers { pending, .. } => {
                    act(state, Action::OrderTriggers((0..pending.len()).collect()))
                }
                other => panic!("unexpected decision: {other:?}"),
            }
        }
        panic!("never reached the expected decision");
    }

    fn priority_for(player: PlayerId) -> impl Fn(&GameState, &Decision) -> bool {
        move |_, decision| matches!(decision, Decision::CastSpellOrPass { player: p, .. } if *p == player)
    }

    /// P0 controls `warded`; P1 casts Lightning Bolt at it and passes, so P0
    /// holds priority with the Bolt and its Ward trigger on the stack.
    fn bolt_at(state: &mut GameState, warded: ObjectId) -> ObjectId {
        let bolt = put(state, P1, "Lightning Bolt", Zone::Hand);
        pool(state, P1, &[ManaColor::R], 0);
        run_until(state, priority_for(P1));
        act(state, Action::CastSpell(bolt));
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(warded)));
                act(state, Action::ChooseTarget(Target::Object(warded)));
            }
            other => panic!("expected the Bolt's target, got {other:?}"),
        }
        run_until(state, priority_for(P1));
        assert_eq!(state.stack.len(), 2, "the Bolt and its Ward trigger");
        act(state, Action::Pass);
        run_until(state, priority_for(P0));
        bolt
    }

    fn is_ward_boolean(decision: &Decision) -> bool {
        matches!(
            decision,
            Decision::ChooseEffectBoolean {
                purpose: EffectBooleanChoicePurpose::CounterUnlessPaysGeneric { .. }
                    | EffectBooleanChoicePurpose::CounterUnlessPaysLife { .. },
                ..
            }
        )
    }

    /// The public schema 5 observation and the policy action records at the
    /// current decision.
    fn public_view(state: &GameState) -> (Value, Vec<Value>) {
        let mut state = state.clone();
        let mut surface = PolicySurfaceV5::new();
        let decision = surface.next_decision(&mut state).unwrap();
        let actor = decision.actor(&state).unwrap();
        let observation =
            mtg_kernel::rl::observe_policy_v5(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        let actions = mtg_kernel::rl::legal_action_candidates_v5(&decision, &state)
            .unwrap()
            .into_iter()
            .map(|candidate| serde_json::to_value(&candidate.record).unwrap())
            .collect();
        (serde_json::to_value(&observation).unwrap(), actions)
    }

    /// `object`'s current stable public reference.
    fn card_stable(state: &GameState, object: ObjectId) -> Value {
        let live = state.objects.get(object);
        json!({"arena_id": object.0, "card_db_id": live.card_def,
            "controller": if live.controller == P0 { "p0" } else { "p1" },
            "owner": if live.owner == P0 { "p0" } else { "p1" },
            "zone": serde_json::to_value(live.zone).unwrap(),
            "zone_change_count": live.zone_change_count})
    }

    /// Review finding 1: the Ward source leaves the battlefield while its
    /// trigger waits, then (privately) stays in hand or goes into its
    /// owner's library (Brainstorm). Both Ward answers name the source by
    /// its public battlefield incarnation, the one `ward_payments` reports,
    /// so the two private outcomes give identical public actions.
    #[test]
    fn ward_answers_name_the_departed_source_by_its_public_incarnation() {
        for warded_name in ["Sedgemoor Witch", "Kappa Cannoneer"] {
            let mut views = Vec::new();
            for into_library in [false, true] {
                let mut state = game();
                let warded = put(&mut state, P0, warded_name, Zone::Battlefield);
                let on_battlefield = card_stable(&state, warded);
                bolt_at(&mut state, warded);
                propose_and_commit(&mut state, ProposedEvent::zone_change(warded, Zone::Hand));
                if into_library {
                    propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(warded, Zone::Library),
                    );
                }
                pool(&mut state, P1, &[], 4);
                run_until(&mut state, |_, decision| is_ward_boolean(decision));
                let (observation, actions) = public_view(&state);
                let pending = &observation["projection"]["ward_payments"]["pending"];
                assert_eq!(pending["ward_source"], on_battlefield, "{warded_name}");
                let sources: Vec<&Value> = actions
                    .iter()
                    .map(|action| &action["semantic"]["source"])
                    .collect();
                assert_eq!(sources, [&on_battlefield, &on_battlefield], "{warded_name}");
                views.push(actions);
            }
            assert_eq!(
                views[0], views[1],
                "{warded_name}: the private move stays hidden"
            );
        }
    }

    /// The pending Ward record, both answers and their outcomes, for a
    /// generic and a life-payment Ward (review non-blocking finding 1).
    #[test]
    fn pending_ward_payment_answers_pay_or_counter() {
        for (warded_name, cost) in [
            ("Kappa Cannoneer", json!({"kind": "generic", "amount": 4})),
            ("Sedgemoor Witch", json!({"kind": "life", "amount": 3})),
        ] {
            for pay in [false, true] {
                let mut state = game();
                let warded = put(&mut state, P0, warded_name, Zone::Battlefield);
                let source = card_stable(&state, warded);
                let bolt = bolt_at(&mut state, warded);
                pool(&mut state, P1, &[], 4);
                run_until(&mut state, |_, decision| is_ward_boolean(decision));
                let (observation, actions) = public_view(&state);
                let pending = &observation["projection"]["ward_payments"]["pending"];
                assert_eq!(pending["ward_source"], source);
                assert_eq!(pending["payer"], "p1");
                assert_eq!(pending["cost"], cost);
                let index = pending["targeting_stack_index"].as_u64().unwrap() as usize;
                let row = &observation["projection"]["stack"][index];
                assert_eq!(row["source"]["arena_id"], bolt.0);
                assert_eq!(name_of(&row["source"]), Some("Lightning Bolt"));
                let values: Vec<&Value> = actions
                    .iter()
                    .map(|action| &action["semantic"]["value"])
                    .collect();
                assert_eq!(values, [&json!(false), &json!(true)]);
                assert!(actions.iter().all(|action| {
                    action["semantic"]["action_kind"] == "choose_effect_boolean"
                        && action["semantic"]["actor"] == "p1"
                }));
                let life = state.players[1].life;
                let mana: u8 = state.players[1].mana_pool.iter().sum();
                act(&mut state, Action::ChooseEffectBoolean(pay));
                run_until(&mut state, |state, decision| {
                    matches!(decision, Decision::CastSpellOrPass { .. }) && state.stack.is_empty()
                });
                assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
                let damaged = state.objects.get(warded).damage == 3
                    || state.objects.get(warded).zone != Zone::Battlefield;
                assert_eq!(damaged, pay, "{warded_name} pay={pay}: the Bolt resolved");
                let paid_mana = mana - state.players[1].mana_pool.iter().sum::<u8>();
                let paid_life = life - state.players[1].life;
                match (cost["kind"].as_str().unwrap(), pay) {
                    ("generic", true) => assert_eq!((paid_mana, paid_life), (4, 0)),
                    ("life", true) => assert_eq!((paid_mana, paid_life), (0, 3)),
                    _ => assert_eq!((paid_mana, paid_life), (0, 0)),
                }
            }
        }
    }

    /// Review finding 5: Counterspell removes the Bolt before the Witch's
    /// Ward trigger resolves. Today the trigger reports no payment and
    /// resolves doing nothing: no payment choice and no halt.
    #[test]
    fn ward_trigger_whose_targeter_was_countered_resolves_without_a_choice() {
        let mut state = game();
        let witch = put(&mut state, P0, "Sedgemoor Witch", Zone::Battlefield);
        let bolt = bolt_at(&mut state, witch);
        let counterspell = put(&mut state, P0, "Counterspell", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
        act(&mut state, Action::CastSpell(counterspell));
        match next(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(bolt)));
                act(&mut state, Action::ChooseTarget(Target::Object(bolt)));
            }
            other => panic!("expected Counterspell's target, got {other:?}"),
        }
        // Counterspell resolves; the Ward trigger waits with no live targeter.
        let decision = run_until(&mut state, |state, _| state.stack.len() == 1);
        assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
        assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
        let (observation, _) = public_view(&state);
        assert!(observation["projection"]["ward_payments"].is_null());
        let life = state.players[1].life;
        run_until(&mut state, |state, decision| {
            assert!(!is_ward_boolean(decision), "no payment choice is offered");
            matches!(decision, Decision::CastSpellOrPass { .. }) && state.stack.is_empty()
        });
        assert_eq!(state.players[1].life, life);
        assert_eq!(state.objects.get(witch).zone, Zone::Battlefield);
    }

    /// Review finding 4: Phyrexian Metamorph copying Kappa Cannoneer leaves
    /// the battlefield while the copy's Ward trigger waits. It stops being a
    /// copy as it leaves, and the Ward trigger still resolves (113.7a).
    #[test]
    fn ward_of_a_copy_that_left_the_battlefield_still_resolves() {
        let mut state = game();
        let kappa = put(&mut state, P0, "Kappa Cannoneer", Zone::Battlefield);
        let metamorph = put(&mut state, P0, "Phyrexian Metamorph", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::U], 3);
        act(&mut state, Action::CastSpell(metamorph));
        for _ in 0..40 {
            match next(&mut state) {
                Decision::ChooseEffectTargets { legal_targets, .. }
                    if legal_targets.contains(&Target::Object(kappa)) =>
                {
                    act(
                        &mut state,
                        Action::ChooseEffectTarget(Target::Object(kappa)),
                    )
                }
                Decision::ChooseEffectTargets {
                    can_finish: true, ..
                } => act(&mut state, Action::FinishEffectSelection),
                Decision::ChooseCastMode { options, .. } => {
                    act(&mut state, Action::ChooseCastMode(options[0]))
                }
                Decision::OrderTriggers { pending, .. } => act(
                    &mut state,
                    Action::OrderTriggers((0..pending.len()).collect()),
                ),
                Decision::CastSpellOrPass { .. }
                    if state.stack.is_empty()
                        && state.objects.get(metamorph).zone == Zone::Battlefield =>
                {
                    break
                }
                Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
                other => panic!("Metamorph: {other:?}"),
            }
        }
        assert_eq!(state.objects.get(metamorph).card_def, id("Kappa Cannoneer"));
        let bolt = bolt_at(&mut state, metamorph);
        propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(metamorph, Zone::Hand),
        );
        assert_eq!(
            state.objects.get(metamorph).card_def,
            id("Phyrexian Metamorph")
        );
        public_view(&state);
        pool(&mut state, P1, &[], 4);
        run_until(&mut state, |_, decision| is_ward_boolean(decision));
        let (observation, _) = public_view(&state);
        assert_eq!(
            observation["projection"]["ward_payments"]["pending"]["cost"],
            json!({"kind": "generic", "amount": 4})
        );
        act(&mut state, Action::ChooseEffectBoolean(false));
        run_until(&mut state, |state, decision| {
            matches!(decision, Decision::CastSpellOrPass { .. }) && state.stack.is_empty()
        });
        assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard, "countered");
    }

    /// Review finding 4: Talon Gates of Madara phases Kappa Cannoneer out
    /// while its Ward trigger waits. Phasing out is not leaving the
    /// battlefield (702.26d) and the trigger still resolves (113.7a).
    #[test]
    fn ward_of_a_phased_out_source_still_resolves() {
        let mut state = game();
        let kappa = put(&mut state, P0, "Kappa Cannoneer", Zone::Battlefield);
        let bolt = bolt_at(&mut state, kappa);
        let gates = put(&mut state, P0, "Talon Gates of Madara", Zone::Hand);
        pool(&mut state, P0, &[], 4);
        act(&mut state, Action::ActivateAbility(gates, 0));
        for _ in 0..40 {
            match next(&mut state) {
                Decision::ChooseTargets { legal_targets, .. } => {
                    assert!(legal_targets.contains(&Target::Object(kappa)));
                    act(&mut state, Action::ChooseTarget(Target::Object(kappa)));
                }
                Decision::OrderTriggers { pending, .. } => act(
                    &mut state,
                    Action::OrderTriggers((0..pending.len()).collect()),
                ),
                Decision::CastSpellOrPass { .. }
                    if state.objects.get(kappa).zone == Zone::Command && state.stack.len() == 2 =>
                {
                    break
                }
                Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
                other => panic!("phasing: {other:?}"),
            }
        }
        assert_eq!(state.objects.get(kappa).zone, Zone::Command, "phased out");
        public_view(&state);
        pool(&mut state, P1, &[], 4);
        run_until(&mut state, |_, decision| is_ward_boolean(decision));
        act(&mut state, Action::ChooseEffectBoolean(false));
        run_until(&mut state, |state, decision| {
            matches!(decision, Decision::CastSpellOrPass { .. }) && state.stack.is_empty()
        });
        assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard, "countered");
    }
}
