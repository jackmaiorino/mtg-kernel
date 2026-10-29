use super::*;
use crate::rl::{CardStableRefV1, GrantedAbilityPublicV1, PlayerSeatV1, TargetRefV1};
use crate::state::Zone;
use serde_json::Value;

fn card() -> CardStableRefV1 {
    CardStableRefV1 {
        arena_id: 1,
        card_db_id: 2,
        owner: PlayerSeatV1::P0,
        controller: PlayerSeatV1::P0,
        zone: Zone::Hand,
        zone_change_count: 0,
    }
}

fn actor() -> PlayerSeatV1 {
    PlayerSeatV1::P0
}

/// The checker's precedence: a target choice wins over a cast or activation,
/// which wins over everything else; mana abilities and plots are other.
#[test]
fn stratum_follows_the_checker_precedence() {
    use ActionSemanticV1 as A;
    let pass = A::Pass { actor: actor() };
    let land = A::PlayLand {
        actor: actor(),
        source: card(),
    };
    let cast = A::CastSpell {
        actor: actor(),
        source: card(),
    };
    let activate = A::ActivateAbility {
        actor: actor(),
        source: card(),
        ability_index: 0,
        granted_ability: GrantedAbilityPublicV1(None),
    };
    let mana = A::ActivateManaAbility {
        actor: actor(),
        source: card(),
        mana_choice: None,
        cost_target: None,
    };
    let plot = A::PlotSpell {
        actor: actor(),
        source: card(),
    };
    let target = A::ChooseTarget {
        actor: actor(),
        source: card(),
        remaining: 1,
        target: TargetRefV1::Player {
            player: PlayerSeatV1::P1,
        },
    };
    let effect_target = A::ChooseEffectTarget {
        actor: actor(),
        source: card(),
        target: TargetRefV1::Player {
            player: PlayerSeatV1::P0,
        },
        selected_count: 0,
        min_targets: 1,
        max_targets: 1,
    };
    let cost_target = A::ChooseCostTarget {
        actor: actor(),
        source: card(),
        cost_kind: crate::engine::CostKind::SacrificeCreatures,
        remaining: 1,
        candidate: card(),
    };
    use Ex1StratumV1::*;
    let cases: Vec<(Vec<ActionSemanticV1>, Ex1StratumV1)> = vec![
        (vec![], Other),
        (vec![pass.clone(), land.clone()], Other),
        (vec![pass.clone(), mana.clone(), plot.clone()], Other),
        (vec![pass.clone(), cost_target.clone()], Other),
        (vec![pass.clone(), cast.clone()], CastOrActivate),
        (vec![mana.clone(), activate.clone()], CastOrActivate),
        (vec![cast.clone(), target.clone()], Target),
        (vec![pass.clone(), effect_target.clone()], Target),
        (
            vec![activate, cast, effect_target, target, mana, plot, pass],
            Target,
        ),
    ];
    for (menu, want) in cases {
        assert_eq!(ex1_stratum_v1(&menu), want, "{menu:?}");
    }
    assert_eq!(
        Ex1StratumV1::ALL.map(Ex1StratumV1::as_str),
        ["target", "cast_or_activate", "other"]
    );
}

/// Section 4's rates are (E / 3) / n(S) at E = 3, to the table's rounding.
#[test]
fn placement_rates_are_the_frozen_table() {
    for stratum in Ex1StratumV1::ALL {
        let derived = 1.0 / stratum.expected_decisions_v1();
        let rate = stratum.placement_rate_v1();
        assert!((derived - rate).abs() / rate < 0.005, "{stratum:?}");
    }
    assert_eq!(Ex1StratumV1::Target.placement_rate_v1(), 0.155);
    assert_eq!(Ex1StratumV1::CastOrActivate.placement_rate_v1(), 0.0311);
    assert_eq!(Ex1StratumV1::Other.placement_rate_v1(), 0.0261);
}

const FIXTURES: &str = include_str!("target_fixtures_v1.json");

fn u64s(value: &Value) -> Vec<u64> {
    serde_json::from_value(value.clone()).unwrap()
}

/// Section 3's binding: the engine's target equals the Python mirror
/// (`tools/ex1/target_ref.py`) bit for bit, no-target reasons included.
#[test]
fn targets_equal_the_mirrored_reference_bit_for_bit() {
    let fixtures: Value = serde_json::from_str(FIXTURES).unwrap();
    assert_eq!(fixtures["version"], EX1_TARGET_VERSION_V1);
    let rows = fixtures["targets"].as_array().unwrap();
    assert!(rows.len() >= 10);
    for f in rows {
        let name = f["name"].as_str().unwrap();
        let bits: Vec<u32> = serde_json::from_value(f["logit_bits"].clone()).unwrap();
        let (n, wins, draws) = (u64s(&f["n"]), u64s(&f["wins"]), u64s(&f["draws"]));
        let counts: Vec<Ex1EdgeCountsV1> = (0..n.len())
            .map(|a| Ex1EdgeCountsV1 {
                n: n[a],
                wins: wins[a],
                draws: draws[a],
            })
            .collect();
        let result = ex1_target_v1(&bits, &counts).unwrap();
        match (f.get("no_target"), result) {
            (Some(reason), Err(no)) => assert_eq!(reason, no.as_str(), "{name}"),
            (None, Ok(t)) => {
                let q: Vec<u64> = t.target.probabilities.iter().map(|x| x.to_bits()).collect();
                let lq: Vec<u64> = t
                    .target
                    .log_probabilities
                    .iter()
                    .map(|x| x.to_bits())
                    .collect();
                assert_eq!(json!(q), f["q_bits"], "{name}");
                assert_eq!(json!(lq), f["log_q_bits"], "{name}");
                assert_eq!(json!(t.mass.to_bits()), f["mass_bits"], "{name}");
                assert_eq!(json!(t.support), f["support"], "{name}");
                let qh: Vec<Option<u64>> = t.qhat.iter().map(|x| x.map(f64::to_bits)).collect();
                assert_eq!(json!(qh), f["qhat_bits"], "{name}");
                assert_eq!(
                    json!(t.total_variation_v1().to_bits()),
                    f["tv_bits"],
                    "{name}"
                );
                // Unsupported edges keep the student's probability; the
                // supported ones share exactly the student's mass on S.
                for a in 0..bits.len() {
                    if !t.support.contains(&a) {
                        assert_eq!(
                            t.target.probabilities[a].to_bits(),
                            t.student.probabilities[a].to_bits(),
                            "{name}"
                        );
                    }
                }
                let lifted: f64 = t.support.iter().map(|&a| t.target.probabilities[a]).sum();
                assert!((lifted - t.mass).abs() <= 1e-15 * t.mass.max(1.0), "{name}");
            }
            (want, got) => panic!("{name}: fixture {want:?}, engine {got:?}"),
        }
    }
    let by_name = |name: &str| rows.iter().find(|f| f["name"] == name).unwrap();
    // A large common logit offset leaves the target unchanged.
    assert_eq!(
        by_name("large_offset")["q_bits"],
        by_name("zero_offset")["q_bits"]
    );
    assert_eq!(by_name("mass_below_floor")["no_target"], "mass");
    assert_eq!(by_name("one_supported")["no_target"], "support");
}

#[test]
fn hashes_equal_the_mirrored_reference() {
    let fixtures: Value = serde_json::from_str(FIXTURES).unwrap();
    for f in fixtures["hashes"].as_array().unwrap() {
        let run = f["run"].as_str().unwrap();
        let (update, seed, decision) = (
            f["update"].as_u64().unwrap(),
            f["seed"].as_u64().unwrap(),
            f["decision"].as_u64().unwrap(),
        );
        let placement = ex1_hash_u64_v1("ex1/placement", run, update, seed, decision);
        assert_eq!(json!(placement), f["placement"]);
        assert_eq!(
            json!(ex1_rowcap_key_v1(run, update, seed, decision)),
            f["rowcap"]
        );
        assert_ne!(f["placement"], f["rowcap"]);
        assert_eq!(
            json!(ex1_uniform_v1(placement).to_bits()),
            f["uniform_bits"]
        );
    }
    assert_eq!(ex1_uniform_v1(0), 0.0);
    assert!(ex1_uniform_v1(u64::MAX) < 1.0);
}

/// Malformed inputs are errors, not no-target roots.
#[test]
fn malformed_targets_are_refused() {
    let bits = [0_u32, 0];
    let ok = Ex1EdgeCountsV1 {
        n: 8,
        wins: 4,
        draws: 4,
    };
    assert!(ex1_target_v1(&bits, &[ok]).is_err());
    let over = Ex1EdgeCountsV1 {
        n: 8,
        wins: 5,
        draws: 4,
    };
    assert!(ex1_target_v1(&bits, &[ok, over]).is_err());
    assert!(ex1_target_v1(&[f32::NAN.to_bits(), 0], &[ok, ok]).is_err());
    assert!(ex1_target_v1(&bits, &[ok, ok]).unwrap().is_ok());
}

/// The lift of any edge is bounded by m_S, and tau = 0.25 bounds the odds
/// change between supported edges by e^4.
#[test]
fn lift_is_bounded_by_the_student_mass() {
    let bits: Vec<u32> = [3.0_f32, 0.0, -2.0].iter().map(|z| z.to_bits()).collect();
    let counts = [
        Ex1EdgeCountsV1 {
            n: 100,
            wins: 0,
            draws: 0,
        },
        Ex1EdgeCountsV1 {
            n: 100,
            wins: 100,
            draws: 0,
        },
        Ex1EdgeCountsV1 {
            n: 2,
            wins: 2,
            draws: 0,
        },
    ];
    let t = ex1_target_v1(&bits, &counts).unwrap().unwrap();
    assert_eq!(t.support, vec![0, 1]);
    for &a in &t.support {
        assert!(t.target.probabilities[a] - t.student.probabilities[a] <= t.mass);
    }
    let odds = t.target.probabilities[1] / t.target.probabilities[0];
    assert!((odds - 4.0_f64.exp()).abs() < 1e-9);
    assert_eq!(t.target_argmax_v1(), 1);
    assert_eq!(EX1_DIVERGENCE_V1, LineBDivergenceV1::Forward);
}
