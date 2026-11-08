//! Declared-deck owner for Kykar modes and one-shot native return schedules.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    current_mixed_policy_v1::KaitoSession,
    human_bo3_v1::HumanKaitoSessionV75,
    limited_consumer_wire_v1::ProfileWireV1,
    optional_trigger_public_v1::{IdentityV1, ReferenceV1},
    kykar_public_v1::{self as public, FactsV1, PRINTING},
    rl::{ActionSemanticV1 as Action, CardStableRefV1, PlayerSeatV1, TargetRefV1},
    rl_session::FastActorSessionV1,
    state::Zone,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-kykar-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/kykar-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/kykar-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/kykar-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("kykar_consumer_v1.rs"),
            include_str!("human_bo3_v1/kykar_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_kykar_env_v1.rs"),
        ] {
            hash.update((source.len() as u64).to_le_bytes());
            hash.update(source.as_bytes());
        }
        hash.update(public::producer_sha256());
        hash.update(crate::limited_kaito_session_v16::producer_sha256_v16());
        hash.update(crate::current_mixed_policy_v1::feature_identity());
        format!("{:x}", hash.finalize())
    })
    .clone()
}

pub(crate) fn preflight(config: &ResetV1) -> Result<(), String> {
    crate::limited_consumer_wire_v1::preflight_single_appendix(
        config,
        PRINTING,
        public::selected_id,
        public::NAME,
    )
}

fn native(config: &ResetV1) -> Result<FastActorSessionV1, String> {
    FastActorSessionV1::reset_limited_kykar_v1(config, false)
        .map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_kykar_v1(config, true)
        .map_err(|error| format!("{error:?}"))?;
    KaitoSession::from_session(owner).map_err(|error| error.to_string())
}

pub fn reset_human_v1(
    config: &ResetV1,
    seat: PlayerSeatV1,
) -> Result<HumanKaitoSessionV75, String> {
    HumanKaitoSessionV75::from_session(native(config)?, seat).map_err(|error| error.to_string())
}

pub struct JsonlServerV1(ProfileWireV1);

impl Default for JsonlServerV1 {
    fn default() -> Self {
        Self(ProfileWireV1::new(PROFILE_V1, producer_sha256_v1, native))
    }
}

impl JsonlServerV1 {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle_line(&mut self, line: &str) -> String {
        self.0.handle_line(line)
    }

    pub fn handle_request(&mut self, request: RequestV1) -> ReplyV1 {
        self.0.handle_request(request)
    }
}

fn original(root: &crate::optional_trigger_public_v1::RootV1) -> bool {
    root.original_source.zone == Zone::Battlefield
        && root.original_source.controller == root.captured_controller
        && matches!(root.original_source.identity, IdentityV1::Known { card_id: PRINTING } | IdentityV1::FaceDown { .. })
        && root.current_original_source.as_ref().is_none_or(|current|
            current.object == root.original_source.object && current.generation == root.original_source.generation
                && current.owner == root.original_source.owner)
}
fn same_original(link: &crate::tinybones_public_v72::IncarnationV72, card: &ReferenceV1) -> bool {
    link.object == card.object && link.generation == card.generation && link.owner == card.owner && link.zone == card.zone
}
pub(crate) fn validate_facts(facts: &FactsV1, actor: PlayerSeatV1) -> Result<(), String> {
    use public::{ModeV1, StageV1};
    if facts.viewer != actor || facts.schema != public::SCHEMA || facts.meaning != public::printed_meaning()? {
        return Err("invalid Kykar viewer or printed meaning".into());
    }
    let mut permanent_ids = std::collections::BTreeSet::new();
    for card in &facts.battlefield {
        if card.card.zone != Zone::Battlefield || !permanent_ids.insert(card.card.object)
            || !matches!(card.card.identity, IdentityV1::Known { card_id } if card_id == PRINTING || card_id == facts.meaning.spirit_card_id) {
            return Err("invalid Kykar public permanent roster".into());
        }
        crate::tinybones_public_v72::nonnegative(&card.counters)?;
    }
    let mut schedules = std::collections::BTreeSet::new();
    for row in &facts.schedules {
        if !original(&row.source) || row.original_exile.zone != Zone::Exile
            || row.original_exile.owner != row.physical_owner || row.original_exile.controller != row.physical_owner
            || !schedules.insert((row.original_exile.object, row.original_exile.generation))
            || row.current_original_exile.as_ref().is_some_and(|card| !same_original(&row.original_exile, card)) {
            return Err("invalid Kykar original exile schedule".into());
        }
    }
    let mut positions = std::collections::BTreeSet::new();
    for ability in &facts.abilities {
        let stage = match ability.stage { StageV1::Held => 0, StageV1::Ordered => 1, StageV1::Published => 2 };
        if !original(&ability.source) || !positions.insert((stage, ability.position))
            || ability.stage == StageV1::Published && ability.mode == ModeV1::Unchosen
            || ability.mode != ModeV1::Exile && ability.target.is_some()
            || ability.stage == StageV1::Published && ability.mode == ModeV1::Exile && ability.target.is_none() {
            return Err("invalid Kykar ability stage or source".into());
        }
        if let ModeV1::Return { schedule } = ability.mode {
            let row = facts.schedules.get(schedule as usize).ok_or("Kykar original schedule absent")?;
            if row.source != ability.source || !row.collected_at_first_end_step {
                return Err("Kykar return lost one-shot collection".into());
            }
        }
        if let Some(target) = &ability.target {
            if target.original.zone != Zone::Battlefield
                || target.original.controller != ability.source.captured_controller
                || target.original.object == ability.source.original_source.object
                || target.current_original.as_ref().is_some_and(|card| !same_original(&target.original, card))
                || target.currently_legal && target.current_original.is_none() {
                return Err("invalid Kykar original target incarnation".into());
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ActionV1 { ChooseExileMode, ChooseSpiritMode, ChooseExileTarget }
fn binds(card: &ReferenceV1, stable: &CardStableRefV1) -> bool {
    let id = match card.identity {
        IdentityV1::Known { card_id } => Some(card_id),
        IdentityV1::FaceDown { disguised } => crate::card_def::card_id_by_name(if disguised { "Face-down creature" } else { "Face-down card" }),
    };
    id == Some(stable.card_db_id) && card.object == stable.arena_id && card.generation == stable.zone_change_count
        && card.owner == stable.owner && card.controller == stable.controller && card.zone == stable.zone
}
pub(crate) fn action(facts: Option<&FactsV1>, action: &Action) -> Result<Option<ActionV1>, String> {
    let Some(facts) = facts else { return Ok(None); };
    let (actor, source) = match action {
        Action::ChooseSpellMode { actor, source, .. } | Action::ChooseTarget { actor, source, .. } => (*actor, source),
        _ => return Ok(None),
    };
    let matching: Vec<_> = facts.abilities.iter().filter(|ability| ability.stage == public::StageV1::Ordered
        && ability.position == 0 && binds(&ability.source.original_source, source)).collect();
    let [ability] = matching.as_slice() else {
        if source.card_db_id == PRINTING { return Err("Kykar action lost original pending trigger".into()); }
        return Ok(None);
    };
    if actor != facts.viewer || actor != ability.source.captured_controller {
        return Err("Kykar mode or target actor changed".into());
    }
    match action {
        Action::ChooseSpellMode { mode_count: 2, mode_index: 0, .. } if ability.mode == public::ModeV1::Unchosen => Ok(Some(ActionV1::ChooseExileMode)),
        Action::ChooseSpellMode { mode_count: 2, mode_index: 1, .. } if ability.mode == public::ModeV1::Unchosen => Ok(Some(ActionV1::ChooseSpiritMode)),
        Action::ChooseTarget { target: TargetRefV1::Object { object }, .. }
            if ability.mode == public::ModeV1::Exile && ability.target.is_none()
                && object.zone == Zone::Battlefield && object.controller == actor
                && object.arena_id != ability.source.original_source.object => Ok(Some(ActionV1::ChooseExileTarget)),
        _ => Err("Kykar action changed original native mode or target".into()),
    }
}

#[cfg(all(test, feature = "limited-fdn-fixtures", not(feature = "standard-magezero-fixtures")))]
mod tests;
