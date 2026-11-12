//! Declared-deck owner and wire for Etali's native attack exile and free-cast menu.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    current_mixed_policy_v1::KaitoSession,
    etali_primal_storm_public_v1::{self as public, ChoiceV1, FactsV1, PRINTING},
    human_bo3_v1::HumanKaitoSessionV75,
    limited_consumer_wire_v1::ProfileWireV1,
    optional_trigger_public_v1::{IdentityV1, ReferenceV1},
    rl::{ActionSemanticV1 as Action, CardStableRefV1, PlayerSeatV1, TargetRefV1},
    rl_session::FastActorSessionV1,
    state::Zone,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const PROFILE_V1: &str = "fdn-etali-primal-storm-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/etali-primal-storm-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/etali-primal-storm-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/etali-primal-storm-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("etali_primal_storm_consumer_v1.rs"),
            include_str!("human_bo3_v1/etali_primal_storm_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_etali_primal_storm_env_v1.rs"),
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
    FastActorSessionV1::reset_limited_etali_v1(config, false).map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_etali_v1(config, true)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ActionV1 {
    CastEtali,
    CastOriginalExile,
    DeclineRemaining,
    ChildCastStep,
}

fn known(card: &ReferenceV1, zone: Zone) -> Option<u16> {
    match card.identity {
        IdentityV1::Known { card_id }
            if card.zone == zone
                && crate::card_def::CARD_DEFS
                    .get(usize::from(card_id))
                    .is_some() =>
        {
            Some(card_id)
        }
        _ => None,
    }
}

fn binds(card: &ReferenceV1, native: &CardStableRefV1) -> bool {
    known(card, card.zone) == Some(native.card_db_id)
        && card.object == native.arena_id
        && card.generation == native.zone_change_count
        && card.owner == native.owner
        && card.controller == native.controller
        && card.zone == native.zone
}

pub(crate) fn validate_facts(facts: &FactsV1, viewer: PlayerSeatV1) -> Result<(), String> {
    if facts.viewer != viewer
        || facts.schema != public::SCHEMA
        || facts.meaning != public::printed_meaning()?
    {
        return Err("Etali facts changed exact printing or viewer".into());
    }
    let exact_root = |root: &crate::optional_trigger_public_v1::RootV1| {
        known(&root.original_source, Zone::Battlefield) == Some(PRINTING)
            && root.original_source.controller == root.captured_controller
    };
    if facts.held.iter().any(|root| !exact_root(root)) {
        return Err("Etali held original attack changed".into());
    }
    for row in &facts.attacks {
        let mut owners = BTreeSet::new();
        let mut cast = BTreeSet::new();
        if !exact_root(&row.source)
            || row.stack_position.is_some() == (row.finished.is_some() || row.countered)
            || row.countered && (row.batch_started || row.finished.is_some())
            || !row.batch_started
                && (!row.original_exiles.is_empty()
                    || !row.actual_casts.is_empty()
                    || row.finished.is_some())
            || row.original_exiles.len() > 2
            || row.original_exiles.iter().any(|card| {
                known(card, Zone::Exile).is_none()
                    || card.owner != card.controller
                    || !owners.insert(card.owner)
            })
            || row.actual_casts.iter().any(|spell| {
                !row.original_exiles.contains(&spell.original_exile)
                    || !cast.insert(spell.original_exile.object)
                    || known(&spell.spell, Zone::Stack) != known(&spell.original_exile, Zone::Exile)
                    || spell.spell.object != spell.original_exile.object
                    || spell.spell.owner != spell.original_exile.owner
                    || spell.spell.controller != row.source.captured_controller
                    || spell.original_exile.generation.checked_add(1)
                        != Some(spell.spell.generation)
                    || spell.x != 0
            })
        {
            return Err("Etali actual exile or free cast facts changed".into());
        }
    }
    if let Some(choice) = &facts.chooser {
        let source = match choice {
            ChoiceV1::CastOrDecline { source, .. } | ChoiceV1::ChildCast { source, .. } => source,
        };
        let row = facts
            .attacks
            .iter()
            .find(|row| {
                row.source.original_source == *source
                    && row.batch_started
                    && row.stack_position.is_some()
            })
            .ok_or("Etali current choice lost its original native attack")?;
        let uncast_nonland = |card: &ReferenceV1| {
            row.original_exiles.contains(card)
                && !row
                    .actual_casts
                    .iter()
                    .any(|spell| spell.original_exile == *card)
                && known(card, Zone::Exile)
                    .is_some_and(|id| !crate::card_def::CARD_DEFS[usize::from(id)].is_land)
        };
        let valid = match choice {
            ChoiceV1::CastOrDecline { legal, .. } => {
                !legal.is_empty()
                    && legal.iter().all(uncast_nonland)
                    && legal
                        .iter()
                        .map(|card| card.object)
                        .collect::<BTreeSet<_>>()
                        .len()
                        == legal.len()
            }
            ChoiceV1::ChildCast {
                original_exile,
                placeholder,
                ..
            } => {
                uncast_nonland(original_exile)
                    && known(placeholder, Zone::Stack) == known(original_exile, Zone::Exile)
                    && placeholder.object == original_exile.object
                    && placeholder.owner == original_exile.owner
                    && placeholder.controller == viewer
                    && original_exile.generation.checked_add(1) == Some(placeholder.generation)
            }
        };
        if !valid || source.controller != viewer {
            return Err("Etali private menu does not match original public exile bindings".into());
        }
    }
    Ok(())
}

pub(crate) fn action(facts: Option<&FactsV1>, action: &Action) -> Option<ActionV1> {
    let facts = facts?;
    match (&facts.chooser, action) {
        (Some(ChoiceV1::ChildCast { .. }), _) => Some(ActionV1::ChildCastStep),
        (
            Some(ChoiceV1::CastOrDecline { source, legal }),
            Action::ChooseEffectTarget {
                source: native,
                target: TargetRefV1::Object { object },
                selected_count: 0,
                min_targets: 0,
                max_targets: 1,
                ..
            },
        ) if binds(source, native) && legal.iter().any(|card| binds(card, object)) => {
            Some(ActionV1::CastOriginalExile)
        }
        (
            Some(ChoiceV1::CastOrDecline { source, .. }),
            Action::FinishEffectSelection {
                source: native,
                selected_count: 0,
                ..
            },
        ) if binds(source, native) => Some(ActionV1::DeclineRemaining),
        (None, Action::CastSpell { source, .. }) if source.card_db_id == PRINTING => {
            Some(ActionV1::CastEtali)
        }
        _ => None,
    }
}

pub(crate) fn validate_candidate_order<'a>(
    facts: &FactsV1,
    actions: impl Iterator<Item = &'a Action>,
) -> Result<(), String> {
    let Some(ChoiceV1::CastOrDecline { legal, .. }) = &facts.chooser else {
        return Ok(());
    };
    let actions: Vec<_> = actions.collect();
    if actions.len() != legal.len() + 1
        || !actions.iter().zip(legal).all(|(row, expected)| {
            matches!(
                row, Action::ChooseEffectTarget { target: TargetRefV1::Object { object }, .. }
                    if binds(expected, object)
            ) && action(Some(facts), row) == Some(ActionV1::CastOriginalExile)
        })
        || actions.last().and_then(|row| action(Some(facts), row))
            != Some(ActionV1::DeclineRemaining)
    {
        return Err("Etali candidate order changed exact native cast/decline menu".into());
    }
    Ok(())
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
