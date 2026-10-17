//! Explicit ordinary-only one/two-member comparison. Existing census rows and
//! default policies are unchanged. `POLICY_ENSEMBLE_MEMBERS` names strict JSON:
//! {"schema":"ordinary-policy-members-v1","members":[
//!   {"source":"existing source descriptor","source_sha256":"64 hex chars"}
//! ]}. One member is the ordinary parent control; two always execute two
//! independent forwards, including duplicate members. `source=` must name the
//! first member. BANK_SPEC, OPPONENTS and BUDGET_CASES retain their semantics.
//! Focal policy identity is deliberately separate from the bank's lineage.

use super::bank::{self, BankShared, SourceSpec};
use super::play::{acting, apply, decision, Meter};
use super::{load_policy_v1, sha, CensusConfigV1};
use crate::fast_sampler::{WideCategoricalScratchV1, WIDE_CATEGORICAL_MAX_ACTIONS_V1};
use crate::ids::PlayerId;
use crate::rl_session::{FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1};
use crate::sideboard_play_policy_v1::{FrozenPlayDecisionScoresV1, FrozenPlayPolicyV1};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Instant;

pub(super) const MODE: &str = "s4a-policy-ensemble";
const MIXING: &str = "q64-half-floor-odd-index-v1";
const Q64: u128 = 1_u128 << 64;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Member {
    source: String,
    source_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Members {
    schema: String,
    members: Vec<Member>,
}

impl Members {
    fn validate(&self) -> Result<(), String> {
        if self.schema != "ordinary-policy-members-v1" || !(1..=2).contains(&self.members.len()) {
            return Err("ordinary-policy-members-v1 requires exactly one or two members".into());
        }
        for member in &self.members {
            if member.source.is_empty()
                || member.source_sha256.len() != 64
                || !member
                    .source_sha256
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            {
                return Err("members require a source and lowercase SHA256 pin".into());
            }
        }
        Ok(())
    }
}

/// Rounded half of the two deployed Q64 distributions, with no floating-point
/// arithmetic. Every odd entry is half a unit short before residual placement.
/// Both input sums are even, so the odd-entry count is even. Raising its first
/// half in legal order conserves Q64 with at most half a unit error per entry.
fn mix(a: &[u128], b: &[u128]) -> Result<Vec<u128>, String> {
    if a.is_empty() || a.len() != b.len() || a.len() > WIDE_CATEGORICAL_MAX_ACTIONS_V1 {
        return Err("ensemble masses need equal admitted nonempty widths".into());
    }
    for masses in [a, b] {
        if masses.iter().any(|&n| n > Q64)
            || masses.iter().try_fold(0_u128, |sum, &n| sum.checked_add(n)) != Some(Q64)
        {
            return Err("each ensemble member must have total Q64 mass".into());
        }
    }
    let mut mixed: Vec<u128> = a.iter().zip(b).map(|(&x, &y)| (x + y) / 2).collect();
    let mut residual = Q64 - mixed.iter().sum::<u128>();
    for ((&x, &y), mass) in a.iter().zip(b).zip(&mut mixed) {
        if residual != 0 && (x + y) % 2 == 1 {
            *mass += 1;
            residual -= 1;
        }
    }
    if residual != 0 || mixed.iter().sum::<u128>() != Q64 {
        return Err("ensemble residual is not conserved".into());
    }
    Ok(mixed)
}

fn masses_hash(masses: &[u128]) -> String {
    sha(&masses
        .iter()
        .flat_map(|n| n.to_le_bytes())
        .collect::<Vec<_>>())
}

fn logits_hash(scores: &FrozenPlayDecisionScoresV1) -> String {
    sha(&scores
        .logits
        .iter()
        .flat_map(|n| n.to_bits().to_le_bytes())
        .collect::<Vec<_>>())
}

fn l1(a: &[u128], b: &[u128]) -> String {
    a.iter()
        .zip(b)
        .map(|(&x, &y)| x.abs_diff(y))
        .sum::<u128>()
        .to_string()
}

fn identity(policy: &FrozenPlayPolicyV1) -> Value {
    json!({"origin":policy.identity_v1(),"actual":policy.actual_model_identity_v1(),
        "sampler":policy.runtime_sampler_identity_v1(),
        "max_actions":policy.runtime_sampler_max_actions_v1(),
        "training_math":policy.training_math_v1()})
}

struct Player {
    members: Vec<FrozenPlayPolicyV1>,
    scratch: [WideCategoricalScratchV1; 2],
    binding: Value,
    inference_calls: u64,
    draws: u64,
}

impl Player {
    fn fork(
        policies: &[FrozenPlayPolicyV1],
        binding: Value,
        seeds: [u64; 2],
    ) -> Result<Self, String> {
        if !(1..=2).contains(&policies.len()) {
            return Err("ordinary ensemble needs one or two members".into());
        }
        let mut members = Vec::new();
        for policy in policies {
            policy.validate_ensemble_member_v1()?;
            let mut member = policy.fork_for_collection_v3()?;
            member.reset_sampling_v1(seeds);
            members.push(member);
        }
        Ok(Self {
            members,
            scratch: [
                WideCategoricalScratchV1::default(),
                WideCategoricalScratchV1::default(),
            ],
            binding,
            inference_calls: 0,
            draws: 0,
        })
    }

    fn rng(&self) -> String {
        self.members[0].ordinary_sampling_commitment_v1()
    }

    fn act(
        &mut self,
        session: &FastActorSessionV1,
        d: &FastActorDecisionV1,
    ) -> Result<(u32, Value), String> {
        if d.legal_action_count < 2 {
            return Ok((0, Value::Null));
        }
        // The one-member control executes the existing deployed sampler.
        let (selected, scores) = if self.members.len() == 1 {
            let (selected, scores) = self.members[0].select_ordinary_comparison_v1(session)?;
            (Some(selected), vec![scores])
        } else {
            // Always score twice, including r1+r1 and r2+r2. No member draws.
            let second_rng = self.members[1].ordinary_sampling_commitment_v1();
            let first = self.members[0].score_fast_session_v1(session)?;
            let second = self.members[1].score_fast_session_v1(session)?;
            if self.members[1].ordinary_sampling_commitment_v1() != second_rng {
                return Err("passive second member advanced its sampling stream".into());
            }
            (None, vec![first, second])
        };
        self.inference_calls += scores.len() as u64;
        let mut component = Vec::new();
        for (scratch, score) in self.scratch.iter_mut().zip(&scores) {
            if score.logits.len() != d.legal_action_count as usize {
                return Err("member and live legal action counts differ".into());
            }
            component.push(
                scratch
                    .apportion(&score.logits)
                    .map_err(|e| e.to_string())?
                    .to_vec(),
            );
        }
        let mixed = if component.len() == 1 {
            component[0].clone()
        } else {
            mix(&component[0], &component[1])?
        };
        let action = match selected {
            Some(action) => action,
            None => self.members[0].sample_ordinary_ensemble_v1(
                &mixed,
                d.acting_player,
                d.legal_action_count,
            )?,
        };
        self.draws += 1;
        let diagnostic = json!({"component_logits_sha256":scores.iter().map(logits_hash).collect::<Vec<_>>(),
            "component_value_bits":scores.iter().map(|s| s.value.to_bits()).collect::<Vec<_>>(),
            "component_masses_sha256":component.iter().map(|m| masses_hash(m)).collect::<Vec<_>>(),
            "mixed_masses_sha256":masses_hash(&mixed),
            "component_l1_q64":(component.len() == 2).then(|| l1(&component[0], &component[1])),
            "mixed_member_l1_q64":component.iter().map(|m| l1(&mixed, m)).collect::<Vec<_>>(),
            "l1_units":"integer mass units; divide by 2^65 for total variation",
            "member_forwards":scores.len(),"sampler_draws":1});
        Ok((action, diagnostic))
    }
}

fn validate_settings(cfg: &CensusConfigV1) -> Result<(), String> {
    if cfg.mode != MODE || cfg.workers != 1 || cfg.games == 0 {
        return Err("s4a-policy-ensemble requires one native worker and positive games".into());
    }
    for (key, admitted) in [
        ("BUDGET_TOTAL", "0"),
        ("BUDGET_ARM", "plain-v1"),
        ("S4A_INTERCHANGE", "0"),
        ("S4A_REUSE_WITNESS", "0"),
        ("S4A_FAST_FORWARD", "0"),
        ("S4A_BACKUP", "mean"),
    ] {
        match std::env::var(key) {
            Ok(value) if value != admitted => return Err(format!("{MODE} refuses {key}={value}")),
            Err(std::env::VarError::NotUnicode(_)) => return Err(format!("invalid {key}")),
            _ => {}
        }
    }
    for key in [
        "S4A_EXECUTOR",
        "S4A_LIMITS",
        "S4A_TRACE",
        "S4A_RUNTIME",
        "S4A_SELECT",
        "BUDGET_KEY_MODE",
    ] {
        if std::env::var_os(key).is_some() {
            return Err(format!("{MODE} refuses unused search setting {key}"));
        }
    }
    Ok(())
}

pub(super) fn run(
    cfg: &CensusConfigV1,
    policy: &FrozenPlayPolicyV1,
    sh: &BankShared,
    bank_bytes: &[u8],
) -> Result<(), String> {
    validate_settings(cfg)?;
    sh.spec.validate()?;
    if sh.labels.len() != sh.opponents.len()
        || sh
            .labels
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != sh.labels.len()
    {
        return Err("ordinary opponent labels must be unique".into());
    }
    let members_path = std::env::var("POLICY_ENSEMBLE_MEMBERS")
        .map_err(|_| "POLICY_ENSEMBLE_MEMBERS is required")?;
    let descriptor_bytes =
        std::fs::read(&members_path).map_err(|e| format!("{members_path}: {e}"))?;
    let descriptors: Members =
        serde_json::from_slice(&descriptor_bytes).map_err(|e| e.to_string())?;
    descriptors.validate()?;
    if std::fs::canonicalize(&cfg.source).map_err(|e| e.to_string())?
        != std::fs::canonicalize(&descriptors.members[0].source).map_err(|e| e.to_string())?
    {
        return Err("source= must name the first member's source descriptor".into());
    }
    let mut policies = Vec::new();
    let mut member_bindings = Vec::new();
    for (index, member) in descriptors.members.iter().enumerate() {
        let source_bytes = std::fs::read(&member.source).map_err(|e| e.to_string())?;
        if sha(&source_bytes) != member.source_sha256 {
            return Err("ensemble member source SHA256 differs".into());
        }
        let loaded = load_policy_v1(&member.source)?;
        loaded.validate_ensemble_member_v1()?;
        if std::fs::read(&member.source).map_err(|e| e.to_string())? != source_bytes {
            return Err("ensemble member descriptor changed during load".into());
        }
        let bound = identity(&loaded);
        if index == 0 && bound != identity(policy) {
            return Err("first member differs from the source= policy loaded at startup".into());
        }
        if let Some(first) = member_bindings.first() {
            let first: &Value = first;
            for key in [
                "feature_contract_digest",
                "feature_encoding_digest",
                "card_db_hash",
            ] {
                if first["model"]["actual"][key] != bound["actual"][key] {
                    return Err("ensemble member feature/card database contracts differ".into());
                }
            }
            if first["model"]["training_math"] != bound["training_math"] {
                return Err("ensemble members must use the same declared forward math".into());
            }
        }
        member_bindings.push(json!({"source_sha256":member.source_sha256,"model":bound}));
        policies.push(loaded);
    }
    let binding = json!({"schema":"ordinary-policy-binding-v1",
        "members":member_bindings,"mixing_rule":if policies.len() == 2 { MIXING } else { "single-deployed-v1" }});
    let cases_path = std::env::var("BUDGET_CASES").map_err(|_| "BUDGET_CASES is required")?;
    let case_bytes = std::fs::read(&cases_path).map_err(|e| e.to_string())?;
    let cases: Vec<SourceSpec> = std::str::from_utf8(&case_bytes)
        .map_err(|e| e.to_string())?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    let first = usize::try_from(cfg.first_game).map_err(|e| e.to_string())?;
    let end = first
        .checked_add(usize::try_from(cfg.games).map_err(|e| e.to_string())?)
        .ok_or("case range overflow")?;
    if end > cases.len() {
        return Err("ordinary case range is outside the frozen schedule".into());
    }
    let mut ids = std::collections::HashSet::new();
    for src in &cases {
        if !ids.insert(&src.id)
            || src.own >= sh.spec.decks.len()
            || src.other >= sh.spec.decks.len()
            || src.seat > 1
            || src.starting_player > 1
            || src.anchor_draw != 0
            || !sh.labels.contains(&src.opp)
        {
            return Err(format!("invalid ordinary-opening case {}", src.id));
        }
    }
    let context = json!({"schema":"ordinary-policy-case-binding-v1","bank_sha256":sha(bank_bytes),
        "bank":sh.spec,"schedule_sha256":sha(&case_bytes),
        "opponents":sh.labels.iter().zip(&sh.opponents).map(|(label, p)| json!({"label":label,"model":identity(p)})).collect::<Vec<_>>(),
        "constructor":"explicit-decks-limits-flat-action-v3-environment-v2-starting-player-v1",
        "git_head":env!("MTG_KERNEL_BUILD_GIT_HEAD"),
        "tracked_tree_sha256":env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256")});
    let mut output = String::new();
    for src in &cases[first..end] {
        let row = std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn_scoped(scope, || {
                    run_case(sh, src, &policies, binding.clone(), context.clone())
                })
                .map_err(|e| e.to_string())?
                .join()
                .map_err(|_| "ordinary ensemble worker panicked".to_owned())?
        })?;
        output.push_str(&serde_json::to_string(&row).map_err(|e| e.to_string())?);
        output.push('\n');
        // Publish completed/partial records before refusing an invalid game.
        std::fs::write(&cfg.out, &output).map_err(|e| e.to_string())?;
        if row["scientific"]["common"]["natural"] != true {
            return Err(format!(
                "ordinary ensemble case {} did not complete naturally",
                src.id
            ));
        }
    }
    Ok(())
}

fn run_case(
    sh: &BankShared,
    src: &SourceSpec,
    policies: &[FrozenPlayPolicyV1],
    binding: Value,
    context: Value,
) -> Result<Value, String> {
    let started = Instant::now();
    let mut player = Player::fork(policies, binding, src.guide_policy_seeds)?;
    let opponent_index = sh
        .labels
        .iter()
        .position(|label| label == &src.opp)
        .ok_or("unknown ordinary opponent")?;
    let mut opponent = sh.opponents[opponent_index].fork_for_collection_v3()?;
    opponent.reset_sampling_v1(src.guide_policy_seeds);
    let mut session = bank::source_session(&sh.spec, src)?;
    let focal = PlayerId(src.seat);
    let mut meter = Meter::new(u64::MAX);
    let defs = super::labels::spy_defs();
    let mut actions = Vec::new();
    let mut diagnostics = Vec::new();
    let mut opponent_draws = 0_u64;
    let failure = (|| -> Result<(), String> {
        while let Some(d) = decision(&session) {
            let mut record = bank::decision_identity(&session)?;
            record["actor"] = json!(acting(&d).0);
            record["environment_revision"] = json!(d.environment_revision);
            record["substep_count"] = json!(d.substep_count);
            record["rng_before"] =
                json!({"focal":player.rng(),"opponent":opponent.ordinary_sampling_commitment_v1()});
            let (action, diagnostic) = if acting(&d) == focal {
                player.act(&session, &d)?
            } else if d.legal_action_count < 2 {
                (0, Value::Null)
            } else {
                opponent_draws += 1;
                (opponent.select_fast_session_v1(&session)?, Value::Null)
            };
            record["action"] = json!(action);
            record["rng_after"] =
                json!({"focal":player.rng(),"opponent":opponent.ordinary_sampling_commitment_v1()});
            if !diagnostic.is_null() {
                diagnostics
                    .push(json!({"step":d.step,"actor":acting(&d).0,"distribution":diagnostic}));
            }
            apply(&mut session, &d, action, &mut meter, focal, None, &defs).map_err(bank::err)?;
            actions.push(record);
        }
        Ok(())
    })()
    .err();
    let terminal = match session.current_response() {
        FastActorResponseV1::Terminal(terminal) => {
            serde_json::to_value(terminal).map_err(|e| e.to_string())?
        }
        FastActorResponseV1::Decision(_) => Value::Null,
    };
    let end = if failure.is_some() {
        "fault"
    } else {
        bank::outcome(&session, focal).unwrap_or("incomplete")
    };
    let common = json!({"binding":context,"case":src,"actions":actions,
        "terminal":terminal,"end":end,"natural":matches!(end,"win"|"draw"|"loss"),"failure":failure,
        "transitions":meter.spent,"search_transitions":0,
        "final_state_hash":format!("{:016x}", session.diagnostic_state_hash()),
        "final_core_environment_hash":format!("{:016x}",session.privileged_core_environment_hash()),
        "sampler_continuation":{"focal":player.rng(),"opponent":opponent.ordinary_sampling_commitment_v1()},
        "sampler_draws":{"focal":player.draws,"opponent":opponent_draws}});
    let scientific = json!({"common":common,"policy":player.binding,
        "diagnostics":diagnostics,"focal_inference_calls":player.inference_calls});
    Ok(
        json!({"kind":"ordinary_policy_game","schema":"ordinary-policy-ensemble-v1/game",
        "id":src.id,"scientific":scientific,
        "timing":{"wall_seconds":started.elapsed().as_secs_f64()}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_ensemble_rounding_symmetry_duplicate_and_bound() {
        let a = [1, 2, 3, Q64 - 6];
        let b = [2, 3, 4, Q64 - 9];
        let mixed = mix(&a, &b).unwrap();
        assert_eq!(mixed, vec![2, 3, 3, Q64 - 8]);
        assert_eq!(mixed, mix(&b, &a).unwrap());
        assert_eq!(mix(&a, &a).unwrap(), a);
        assert_eq!(mixed.iter().sum::<u128>(), Q64);
        for ((&x, &y), &m) in a.iter().zip(&b).zip(&mixed) {
            assert!((2 * m).abs_diff(x + y) <= 1);
        }
    }

    #[test]
    fn policy_ensemble_rejects_invalid_masses_and_widths() {
        for (a, b) in [
            (vec![], vec![]),
            (vec![Q64], vec![0]),
            (vec![Q64], vec![Q64, 0]),
            (vec![u128::MAX, 2], vec![Q64, 0]),
        ] {
            assert!(mix(&a, &b).is_err());
        }
        let mut too_wide = vec![0; WIDE_CATEGORICAL_MAX_ACTIONS_V1 + 1];
        too_wide[0] = Q64;
        assert!(mix(&too_wide, &too_wide).is_err());
    }

    #[test]
    fn policy_ensemble_narrow_and_wide_deployed_mass_identity() {
        for width in [1, 2, 64, 65, 5040, 65536] {
            let logits: Vec<f32> = (0..width).map(|i| (i % 17) as f32 - 8.0).collect();
            let mut scratch = WideCategoricalScratchV1::default();
            let masses = scratch.apportion(&logits).unwrap();
            assert_eq!(mix(masses, masses).unwrap(), masses);
            let reversed: Vec<_> = masses.iter().copied().rev().collect();
            let mixed = mix(masses, &reversed).unwrap();
            assert_eq!(mixed, mix(&reversed, masses).unwrap());
            assert_eq!(mixed.iter().sum::<u128>(), Q64);
            for ((&x, &y), &m) in masses.iter().zip(&reversed).zip(&mixed) {
                assert!((2 * m).abs_diff(x + y) <= 1);
            }
        }
    }

    #[test]
    fn policy_ensemble_member_descriptor_is_strict() {
        let member = json!({"source":"parent.json","source_sha256":"a".repeat(64)});
        let valid =
            json!({"schema":"ordinary-policy-members-v1","members":[member.clone(),member]});
        serde_json::from_value::<Members>(valid.clone())
            .unwrap()
            .validate()
            .unwrap();
        let mut bad = valid.clone();
        bad["weights"] = json!([0.4, 0.6]);
        assert!(serde_json::from_value::<Members>(bad).is_err());
        let mut bad = valid.clone();
        bad["members"] = json!([]);
        assert!(serde_json::from_value::<Members>(bad)
            .unwrap()
            .validate()
            .is_err());
        let mut bad = valid;
        bad["members"][0]["source_sha256"] = json!("A".repeat(64));
        assert!(serde_json::from_value::<Members>(bad)
            .unwrap()
            .validate()
            .is_err());
    }

    #[test]
    fn policy_ensemble_requires_ordinary_v4_and_one_live_draw() {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(|| {
                use crate::rl::PlayerSeatV1;
                use crate::state::SplitMix64;
                let mut policy = FrozenPlayPolicyV1::training_fixture_v4();
                policy.validate_ensemble_member_v1().unwrap();
                assert!(FrozenPlayPolicyV1::training_fixture_v3()
                    .validate_ensemble_member_v1()
                    .is_err());
                policy.reset_sampling_v1([17, 23]);
                let initial = policy.ordinary_sampling_commitment_v1();
                assert!(policy
                    .sample_ordinary_ensemble_v1(&[Q64 - 1], PlayerSeatV1::P1, 1)
                    .is_err());
                assert_eq!(initial, policy.ordinary_sampling_commitment_v1());
                let mut streams = [SplitMix64::seed(17), SplitMix64::seed(23)];
                let mut scratch = WideCategoricalScratchV1::default();
                let logits = [-1.0, 0.0, 3.0];
                let masses = scratch.apportion(&logits).unwrap().to_vec();
                for seat in [PlayerSeatV1::P0, PlayerSeatV1::P1, PlayerSeatV1::P0] {
                    let index = if seat == PlayerSeatV1::P0 { 0 } else { 1 };
                    let expected = scratch.sample(&logits, streams[index].next_u64()).unwrap();
                    assert_eq!(
                        policy
                            .sample_ordinary_ensemble_v1(&masses, seat, 3)
                            .unwrap() as usize,
                        expected
                    );
                    assert_eq!(
                        policy.ordinary_sampling_commitment_v1(),
                        sha(&serde_json::to_vec(&streams).unwrap())
                    );
                }
                policy.enable_unclamped_collection_sampler_v1();
                assert!(policy.validate_ensemble_member_v1().is_err());
            })
            .unwrap()
            .join()
            .unwrap();
    }

    /// The parent selects through the existing deployed sampler; duplicate
    /// members each score the live session. Compare the entire common record
    /// and independently collected plain actions/streams for both seats/starts.
    #[test]
    #[ignore = "native ensemble qualification: four bounded ordinary fixtures, two forwards per ensemble decision"]
    fn policy_ensemble_duplicate_full_common_and_independent_plain_parity() {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(|| {
                use super::super::arms::Limits;
                use super::super::bank::{BankSpec, DeckSpec, SPEC_SCHEMA};
                use super::super::world::DeckPrior;
                use crate::runtime_decks::RUNTIME_DECKS;
                let policy = FrozenPlayPolicyV1::training_fixture_v4();
                for seat in 0..2 {
                    for starting_player in 0..2 {
                        let src = SourceSpec {
                            id: format!("ensemble-fixture-{seat}-{starting_player}"),
                            own: 5,
                            other: 1,
                            seat,
                            opp: "fixture".into(),
                            seed: 101,
                            starting_player,
                            source_policy_seeds: [101, 103],
                            anchor_draw: 0,
                            guide_policy_seeds: [7, 9],
                            search_root: "unused".into(),
                        };
                        let sh = BankShared {
                            spec: BankSpec {
                                schema: SPEC_SCHEMA.into(),
                                namespace: "spy-reset-pilot-v1".into(),
                                lineage: "case-lineage-independent-of-focal".into(),
                                runtime_rules: "resolution-boundary-v1".into(),
                                select_rule: "fpu-1.5".into(),
                                max_physical_decisions: 12,
                                max_policy_steps: 40,
                                decks: RUNTIME_DECKS
                                    .iter()
                                    .map(|d| DeckSpec {
                                        label: d.id.into(),
                                        mainboard: d.card_ids.to_vec(),
                                    })
                                    .collect(),
                                sources: vec![src.clone()],
                            },
                            labels: vec!["fixture".into()],
                            opponents: vec![policy.fork_for_collection_v3().unwrap()],
                            prior: DeckPrior::new(&(0..RUNTIME_DECKS.len()).collect::<Vec<_>>()),
                            limits: Limits::FORMAL,
                        };
                        let single = run_case(
                            &sh,
                            &src,
                            &[policy.fork_for_collection_v3().unwrap()],
                            json!("parent"),
                            json!("context"),
                        )
                        .unwrap();
                        let duplicate = run_case(
                            &sh,
                            &src,
                            &[
                                policy.fork_for_collection_v3().unwrap(),
                                policy.fork_for_collection_v3().unwrap(),
                            ],
                            json!("duplicate"),
                            json!("context"),
                        )
                        .unwrap();
                        assert_eq!(
                            single["scientific"]["common"],
                            duplicate["scientific"]["common"]
                        );
                        let again = run_case(
                            &sh,
                            &src,
                            &[
                                policy.fork_for_collection_v3().unwrap(),
                                policy.fork_for_collection_v3().unwrap(),
                            ],
                            json!("duplicate"),
                            json!("context"),
                        )
                        .unwrap();
                        assert_eq!(duplicate["scientific"], again["scientific"]);
                        let single_calls = single["scientific"]["focal_inference_calls"]
                            .as_u64()
                            .unwrap();
                        assert!(single_calls > 0);
                        assert_eq!(
                            duplicate["scientific"]["focal_inference_calls"].as_u64(),
                            Some(2 * single_calls)
                        );
                        for action in single["scientific"]["common"]["actions"]
                            .as_array()
                            .unwrap()
                        {
                            if action["k"] == 1 {
                                assert_eq!(action["rng_before"], action["rng_after"]);
                            }
                        }
                        let mut session = bank::source_session(&sh.spec, &src).unwrap();
                        let mut focal = policy.fork_for_collection_v3().unwrap();
                        let mut opponent = sh.opponents[0].fork_for_collection_v3().unwrap();
                        focal.reset_sampling_v1(src.guide_policy_seeds);
                        opponent.reset_sampling_v1(src.guide_policy_seeds);
                        let mut actions = Vec::new();
                        bank::plain_logged(
                            &mut session,
                            PlayerId(seat),
                            &mut focal,
                            &mut opponent,
                            &mut actions,
                        )
                        .unwrap();
                        let recorded: Vec<u32> = single["scientific"]["common"]["actions"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|r| r["action"].as_u64().unwrap() as u32)
                            .collect();
                        assert_eq!(recorded, actions);
                        assert_eq!(
                            single["scientific"]["common"]["sampler_continuation"]["focal"],
                            focal.ordinary_sampling_commitment_v1()
                        );
                        assert_eq!(
                            single["scientific"]["common"]["sampler_continuation"]["opponent"],
                            opponent.ordinary_sampling_commitment_v1()
                        );
                    }
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
