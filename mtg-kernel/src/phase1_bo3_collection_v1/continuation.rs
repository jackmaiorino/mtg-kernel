//! Coordinator-only conditional continuation of one confirmed archived root.
//! Hidden engine state is private and is never supplied to a playing policy.
use super::*;
use crate::rl_session::FastActorSessionV1;
use serde_json::{json, Value};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3ContinuationOptionsV1 {
    pub match_id: String,
    pub game_index: u8,
    pub decision_index: u64,
    pub actor: PlayerSeatV1,
    pub selected_index: u32,
    /// SHA-256 of serde_json serialization of the complete archived record.
    pub record_sha256: String,
    /// None preserves advanced streams for engineering replay only.
    pub policy_seeds: Vec<Option<[u64; 2]>>,
    pub retain_records: bool,
}
impl Bo3ContinuationOptionsV1 {
    pub fn from_json_v1(text: &str) -> Result<Self, String> {
        ensure(text.len() <= 32768, "continuation options exceed 32 KiB")?;
        crate::rl::parse_strict_json_value(text).map_err(|e| e.to_string())?;
        let options: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        options.validate()?;
        Ok(options)
    }
    fn validate(&self) -> Result<(), String> {
        ensure((1..=3).contains(&self.game_index), "continuation game outside BO3")?;
        ensure(!self.match_id.is_empty(), "continuation match ID missing")?;
        ensure(self.record_sha256.len() == 64 && self.record_sha256.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), "invalid root record hash")?;
        ensure((1..=200).contains(&self.policy_seeds.len()), "continuation batch outside bounds")?;
        ensure(!self.retain_records || self.policy_seeds.len() == 1,
            "full records are restricted to single-continuation engineering checks")
    }
}

pub(super) struct ContinuationCapture {
    pub session: FastActorSessionV1,
    policies: [FrozenPlayPolicyV1; 2],
    record: Bo3DecisionRecordV1,
    headroom_before: [u64; 2],
}
pub(super) struct ContinuationSink {
    options: Bo3ContinuationOptionsV1,
    committed: Option<ContinuationCapture>,
}
impl ContinuationSink {
    pub(super) fn prepare(&self, input: &PairedBo1PolicyInputV1<'_>,
        record: &Bo3DecisionRecordV1, game: u8, policies: &[FrozenPlayPolicyV1; 2],
    ) -> Result<Option<ContinuationCapture>, String> {
        let o = &self.options;
        if game != o.game_index || record.decision_index != o.decision_index { return Ok(None); }
        ensure(self.committed.is_none(), "duplicate continuation root")?;
        ensure(input.decision().acting_player == o.actor, "continuation actor differs")?;
        ensure(record.behavior.selected_index_v1() == o.selected_index as usize, "continuation action differs")?;
        ensure(format!("{:x}", Sha256::digest(serde_json::to_vec(record).map_err(|e| e.to_string())?))
            == o.record_sha256, "continuation archived record differs")?;
        // Called AFTER sampling the selected root action. Both copies retain
        // current positions; the original policies and session are untouched.
        let forks = [policies[0].fork_for_continuation_v1()?, policies[1].fork_for_continuation_v1()?];
        let headroom_before = input.diagnostic_continuation_headroom_v1();
        let session = input.diagnostic_continuation_after_action_v1(o.selected_index)?;
        Ok(Some(ContinuationCapture { session, policies: forks, record: record.clone(), headroom_before }))
    }
    pub(super) fn commit(&mut self, capture: ContinuationCapture) -> Result<(), String> {
        ensure(self.committed.is_none(), "continuation root already committed")?;
        self.committed = Some(capture);
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct Bo3ContinuationResultV1 {
    pub schema: &'static str,
    pub collection: Bo3CollectionResultV1,
    pub continuation: Value,
}

pub fn collect_bo3_with_continuation_v1(config: Bo3CollectionConfigV1,
    packages: [CompleteAgentPackageV1; 2], options: Bo3ContinuationOptionsV1,
) -> Result<Bo3ContinuationResultV1, String> {
    ensure(cfg!(feature = "experimental-burn-net8-packed-cuda-v1"), "continuation feature unavailable")?;
    options.validate()?;
    ensure(config.match_id == options.match_id, "continuation match ID differs")?;
    let mut sink = ContinuationSink { options: options.clone(), committed: None };
    let collection = collect_public_observed(config, packages, None, None, Some(&mut sink))?;
    let mut rows = Vec::new();
    let mut root = Value::Null;
    if let Some(capture) = sink.committed {
        root = json!({"record":capture.record,"headroom_before":capture.headroom_before,
            "headroom_after":capture.session.diagnostic_remaining_headroom_v1()});
        let hashes = collection.packages.each_ref().map(|p| p.package_sha256_v1()).into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        for seeds in &options.policy_seeds {
            rows.push(run_one(&capture, *seeds, &hashes, options.retain_records)?);
        }
    }
    let complete = !rows.is_empty() && rows.iter().all(|r| r["natural"] == true);
    Ok(Bo3ContinuationResultV1 { schema: "mtg-kernel-bo3-continuation/v1", collection,
        continuation: json!({"options":options,"root":root,"rows":rows,"complete":complete,
            "non_claim":"Conditional incumbent continuation risk at a fixed hidden state, not first-action causal regret, population prevalence or strength."}) })
}

fn run_one(capture: &ContinuationCapture, seeds: Option<[u64; 2]>, hashes: &[String],
    retain_records: bool,
) -> Result<Value, String> {
    let fork = |p: &FrozenPlayPolicyV1| match seeds {
        Some(s) => p.fork_for_seeded_continuation_v1(s), None => p.fork_for_continuation_v1(),
    };
    let mut policies = [fork(&capture.policies[0])?, fork(&capture.policies[1])?];
    let mut session = capture.session.clone();
    let mut records = Vec::new();
    let mut digest = Sha256::new();
    let mut next_index = capture.record.decision_index + 1;
    let mut count = 0u64;
    let mut bytes = 0u64;
    let mut error: Option<String> = None;
    // No game-boundary reset and no BO3 advancement. Natural/capped terminals
    // retain their actual classification; errors never become natural losses.
    while let FastActorResponseV1::Decision(decision) = session.current_response() {
        let result = (|| -> Result<Bo3DecisionRecordV1, String> {
            let input = PairedBo1PolicyInputV1::new(&session, decision);
            let acting = seat(decision.acting_player);
            let (selected, scores) = policies[acting].select_paired_with_scores_v1(&input)
                .map_err(|e| e.to_string())?;
            let behavior = BehaviorDistributionV1::hamilton_from_logits_v1(&scores.logits, selected)?;
            let record = input.capture_bo3_gameplay_v4(next_index, hashes[acting].clone(), behavior)?;
            session.step(decision.episode_id, decision.step, selected).map_err(|e| e.to_string())?;
            Ok(record)
        })();
        let record = match result { Ok(r) => r, Err(e) => { error = Some(e); break; } };
        let encoded = serde_json::to_vec(&record).map_err(|e| e.to_string())?;
        bytes += encoded.len() as u64;
        digest.update((encoded.len() as u64).to_le_bytes());
        digest.update(&encoded);
        count += 1;
        next_index += 1;
        if retain_records { records.push(record); }
        if retain_records && bytes > MAX_RECORD_BYTES {
            error = Some("continuation engineering record byte limit exceeded".into()); break;
        }
    }
    let terminal = match session.current_response() { FastActorResponseV1::Terminal(t) => Some(t), _ => None };
    let natural = error.is_none() && terminal.as_ref().is_some_and(|t|
        t.terminal_classification == TerminalClassificationV1::Natural);
    digest.update(serde_json::to_vec(&terminal).map_err(|e| e.to_string())?);
    Ok(json!({"policy_seeds":seeds,"natural":natural,"terminal":terminal,"error":error,
        "committed_steps":count,"record_bytes":bytes,"records":records,
        "trajectory_sha256":format!("{:x}",digest.finalize()),
        "remaining_headroom":session.diagnostic_remaining_headroom_v1()}))
}

#[cfg(all(test, feature = "experimental-burn-net8-packed-cuda-v1"))]
mod tests {
    use super::*;
    use crate::phase1_bo3_collection_v1::tests::{config, fixtures_v4};

    #[test]
    fn continuation_fork_confirmed_capture_reproduces_remaining_game() {
        let cfg = config("continuation-fork-replay");
        let (mut policies, packages) = fixtures_v4([PlayDrawChoiceV1::Play; 2]);
        let original = collect_loaded_inner(&cfg, packages.each_ref(), &mut policies,
            [None, None], None).unwrap();
        let game = &original.trajectory.games[0];
        let gameplay: Vec<_> = game.decisions.iter().filter(|r|
            matches!(r.visible, ActorVisibleDecisionV1::Gameplay { .. })).collect();
        let root = gameplay[gameplay.len() / 2];
        let options = Bo3ContinuationOptionsV1 { match_id: cfg.match_id.clone(),
            game_index: game.game_index, decision_index: root.decision_index, actor: root.actor,
            selected_index: root.behavior.selected_index_v1() as u32,
            record_sha256: format!("{:x}", Sha256::digest(serde_json::to_vec(root).unwrap())),
            policy_seeds: vec![None], retain_records: true };
        let mut sink = ContinuationSink { options, committed: None };
        let observed = collect_loaded_with_continuation(&cfg, packages.each_ref(), &mut policies,
            [None, None], None, None, Some(&mut sink)).unwrap();
        assert_eq!(serde_json::to_vec(&original).unwrap(), serde_json::to_vec(&observed).unwrap());
        let capture = sink.committed.unwrap();
        let hashes: Vec<_> = packages.iter().map(|p| p.package_sha256_v1().unwrap()).collect();
        let replay = run_one(&capture, None, &hashes, true).unwrap();
        let expected: Vec<_> = game.decisions.iter().filter(|r| r.decision_index > root.decision_index).collect();
        assert_eq!(replay["records"], serde_json::to_value(expected).unwrap());
        assert_eq!(replay["terminal"], serde_json::to_value(&original.games[0].observed_terminal).unwrap());
        assert_eq!(replay["natural"], true);
        let a = run_one(&capture, Some([13, 47]), &hashes, true).unwrap();
        let b = run_one(&capture, Some([13, 47]), &hashes, true).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn continuation_fork_rejected_root_never_admits_capture() {
        let cfg = config("continuation-fork-reject");
        let (mut policies, packages) = fixtures_v4([PlayDrawChoiceV1::Play; 2]);
        let original = collect_loaded_inner(&cfg, packages.each_ref(), &mut policies,
            [None, None], None).unwrap();
        let root = original.trajectory.games[0].decisions.iter().find(|r|
            matches!(r.visible, ActorVisibleDecisionV1::Gameplay { .. })).unwrap();
        let options = Bo3ContinuationOptionsV1 { match_id: cfg.match_id.clone(), game_index: 1,
            decision_index: root.decision_index, actor: root.actor,
            selected_index: root.behavior.selected_index_v1() as u32,
            record_sha256: "0".repeat(64), policy_seeds: vec![None], retain_records: true };
        let mut sink = ContinuationSink { options, committed: None };
        let observed = collect_loaded_with_continuation(&cfg, packages.each_ref(), &mut policies,
            [None, None], None, None, Some(&mut sink)).unwrap();
        assert!(sink.committed.is_none());
        assert!(matches!(observed.trajectory.ending, Bo3TrajectoryEndingV1::Incomplete { .. }));
    }
}
