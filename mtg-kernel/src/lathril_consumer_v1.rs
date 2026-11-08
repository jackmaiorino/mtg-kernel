//! Declared-deck owning consumer for Lathril's native combat and Elf-payment choices.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    current_mixed_policy_v1::KaitoSession,
    human_bo3_v1::HumanKaitoSessionV75,
    lathril_public_v1::{self as public, FactsV1, PRINTING},
    limited_consumer_wire_v1::ProfileWireV1,
    optional_trigger_public_v1::{IdentityV1, ReferenceV1},
    rl::{ActionSemanticV1 as Action, CardStableRefV1, PlayerSeatV1},
    rl_session::FastActorSessionV1,
    state::Zone,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-lathril-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/lathril-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/lathril-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/lathril-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("lathril_consumer_v1.rs"),
            include_str!("human_bo3_v1/lathril_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_lathril_env_v1.rs"),
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
    FastActorSessionV1::reset_limited_lathril_v1(config, false)
        .map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_lathril_v1(config, true)
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

fn printing(card: &ReferenceV1) -> Option<u16> {
    match card.identity {
        IdentityV1::Known { card_id } => Some(card_id),
        IdentityV1::FaceDown { disguised } => crate::card_def::card_id_by_name(if disguised {
            "Face-down creature"
        } else {
            "Face-down card"
        }),
    }
}

fn source_valid(source: &crate::optional_trigger_public_v1::RootV1) -> bool {
    let original = &source.original_source;
    original.identity == (IdentityV1::Known { card_id: PRINTING })
        && original.zone == Zone::Battlefield
        && original.controller == source.captured_controller
        && source
            .current_original_source
            .as_ref()
            .is_none_or(|current| {
                current.object == original.object
                    && current.generation == original.generation
                    && current.owner == original.owner
                    && current.zone == original.zone
            })
}

pub(crate) fn validate_facts(facts: &FactsV1, actor: PlayerSeatV1) -> Result<(), String> {
    if facts.viewer != actor
        || facts.schema != public::SCHEMA
        || facts.meaning != public::printed_meaning()?
    {
        return Err("invalid Lathril actor facts".into());
    }
    for row in &facts.battlefield {
        let id = printing(&row.card);
        if row.card.zone != Zone::Battlefield
            || !(id == Some(PRINTING) || (id == Some(facts.meaning.token_id) && row.token))
        {
            return Err("invalid Lathril battlefield identity".into());
        }
        crate::tinybones_public_v72::nonnegative(&row.counters)?;
    }
    for cast in &facts.casts {
        if printing(&cast.spell) != Some(PRINTING)
            || cast.spell.zone != Zone::Stack
            || cast.uncast_copy != cast.origin_zone.is_none()
            || (!cast.announcing && !cast.uncast_copy) != cast.finalized_method.is_some()
            || cast.uncast_copy && cast.announcing
        {
            return Err("invalid Lathril native cast".into());
        }
    }
    for trigger in &facts.combat {
        if trigger.final_damage == 0 || !source_valid(&trigger.source) {
            return Err("invalid Lathril original combat occurrence".into());
        }
        if let public::CombatStageV1::Resolved { tokens } = &trigger.stage {
            let mut seen = std::collections::BTreeSet::new();
            if tokens.iter().any(|token| {
                token.zone != Zone::Battlefield
                    || token.generation != 0
                    || token.owner != trigger.source.captured_controller
                    || token.controller != trigger.source.captured_controller
                    || !seen.insert(token.object)
            }) {
                return Err("invalid Lathril realized original token roster".into());
            }
        }
    }
    for drain in &facts.drains {
        if !source_valid(&drain.source)
            || drain.original_paid_elves.len() != usize::from(crate::lathril_receipts_v1::ELVES)
            || drain
                .original_paid_elves
                .iter()
                .enumerate()
                .any(|(i, elf)| {
                    elf.zone != Zone::Battlefield
                        || elf.controller != drain.source.captured_controller
                        || elf.object == drain.source.original_source.object
                        || drain.original_paid_elves[..i]
                            .iter()
                            .any(|prior| prior.object == elf.object)
                })
        {
            return Err("invalid Lathril original paid Elf roster".into());
        }
    }
    if let Some(payment) = &facts.payment {
        let mut seen = std::collections::BTreeSet::new();
        if payment.chooser != actor
            || payment.source.controller != actor
            || payment.source.zone != Zone::Battlefield
            || printing(&payment.source) != Some(PRINTING)
            || payment.remaining == 0
            || payment.selected.len() + usize::from(payment.remaining)
                != usize::from(crate::lathril_receipts_v1::ELVES)
            || payment.candidates.len() < usize::from(payment.remaining)
            || payment
                .selected
                .iter()
                .chain(&payment.candidates)
                .any(|card| {
                    card.controller != actor
                        || card.zone != Zone::Battlefield
                        || card.object == payment.source.object
                        || !seen.insert(card.object)
                })
        {
            return Err("invalid Lathril chooser-only Elf payment".into());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ActionV1 {
    ActivateDrain { other_elves: u8 },
    ChooseElfPayment { remaining: u8 },
}

fn binds(card: &ReferenceV1, stable: &CardStableRefV1) -> bool {
    printing(card) == Some(stable.card_db_id)
        && card.object == stable.arena_id
        && card.generation == stable.zone_change_count
        && card.owner == stable.owner
        && card.controller == stable.controller
        && card.zone == stable.zone
}

pub(crate) fn action(facts: Option<&FactsV1>, action: &Action) -> Result<Option<ActionV1>, String> {
    let Some(facts) = facts else { return Ok(None) };
    match action {
        Action::ActivateAbility {
            actor,
            source,
            ability_index: 0,
            granted_ability,
        } if source.card_db_id == PRINTING && granted_ability.is_none() => {
            if !facts
                .battlefield
                .iter()
                .any(|row| row.card.controller == *actor && binds(&row.card, source))
            {
                return Err("Lathril drain action lost its native source".into());
            }
            Ok(Some(ActionV1::ActivateDrain {
                other_elves: crate::lathril_receipts_v1::ELVES,
            }))
        }
        Action::ChooseCostTarget {
            actor,
            source,
            cost_kind,
            remaining,
            candidate,
        } if source.card_db_id == PRINTING
            && *cost_kind == crate::engine::CostKind::TapPermanents =>
        {
            let payment = facts
                .payment
                .as_ref()
                .ok_or("Lathril actor lost chooser-only Elf payment")?;
            if *actor != payment.chooser
                || *remaining != payment.remaining
                || !binds(&payment.source, source)
                || !payment.candidates.iter().any(|elf| binds(elf, candidate))
            {
                return Err("Lathril Elf payment action lost its native original menu".into());
            }
            Ok(Some(ActionV1::ChooseElfPayment {
                remaining: *remaining,
            }))
        }
        _ => Ok(None),
    }
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
