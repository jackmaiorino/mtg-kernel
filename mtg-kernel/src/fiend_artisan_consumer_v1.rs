//! Declared-deck owner and wire for Fiend Artisan's payment and private search.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    current_mixed_policy_v1::KaitoSession,
    fiend_artisan_public_v1::{self as public, ChoiceV1, FactsV1, PRINTING},
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

pub const PROFILE_V1: &str = "fdn-fiend-artisan-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/fiend-artisan-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/fiend-artisan-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/fiend-artisan-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("fiend_artisan_consumer_v1.rs"),
            include_str!("human_bo3_v1/fiend_artisan_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_fiend_artisan_env_v1.rs"),
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
        config, PRINTING, public::selected_id, public::NAME,
    )
}

fn native(config: &ResetV1) -> Result<FastActorSessionV1, String> {
    FastActorSessionV1::reset_limited_fiend_artisan_v1(config, false)
        .map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_fiend_artisan_v1(config, true)
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
    Cast,
    Activate,
    PayX { x: u8 },
    Sacrifice,
    ChooseCreature,
    FinishSearch { selected_creature: bool },
}

fn exact_source(card: &ReferenceV1) -> bool {
    card.zone == Zone::Battlefield && card.identity == (IdentityV1::Known { card_id: PRINTING })
}

fn binds(card: &ReferenceV1, native: &CardStableRefV1) -> bool {
    let printing = match card.identity {
        IdentityV1::Known { card_id } => Some(card_id),
        IdentityV1::FaceDown { disguised } => crate::card_def::card_id_by_name(if disguised {
            "Face-down creature"
        } else {
            "Face-down card"
        }),
    };
    printing == Some(native.card_db_id)
        && card.object == native.arena_id
        && card.generation == native.zone_change_count
        && card.zone == native.zone
        && card.owner == native.owner
        && card.controller == native.controller
}

pub(crate) fn validate_facts(facts: &FactsV1, viewer: PlayerSeatV1) -> Result<(), String> {
    if facts.viewer != viewer || facts.schema != public::SCHEMA
        || facts.meaning != public::printed_meaning()?
        || facts.battlefield.iter().any(|row| !exact_source(&row.current.card))
    {
        return Err("Fiend Artisan facts changed exact printing or viewer".into());
    }
    for row in &facts.activations {
        let source = &row.source.original_source;
        let sacrifice = &row.original_sacrifice;
        if !exact_source(source) || source.controller != row.source.captured_controller
            || sacrifice.zone != Zone::Battlefield || sacrifice.object == source.object
            || sacrifice.controller != source.controller
            || row.searching && row.stack_position.is_none()
            || row.stack_position.is_some() == (row.shuffled || row.countered)
            || row.shuffled && row.countered
            || row.actual_public_arrival.as_ref().is_some_and(|card|
                !row.shuffled || matches!(card.zone, Zone::Hand | Zone::Library)
                    || card.owner != source.controller)
        {
            return Err("Fiend Artisan paid or realized fact changed shape".into());
        }
    }
    if let Some(choice) = &facts.chooser {
        let source = match choice {
            ChoiceV1::PayX { source, .. } => source,
            ChoiceV1::Sacrifice { source, legal, .. } => {
                let mut seen = BTreeSet::new();
                if legal.is_empty() || legal.iter().any(|card|
                    card.zone != Zone::Battlefield || card.controller != viewer
                        || card.object == source.object || !seen.insert(card.object))
                {
                    return Err("Fiend Artisan sacrifice menu changed exact other creature roster".into());
                }
                source
            }
            ChoiceV1::Search { source, paid_x, selected, legal } => {
                let mut seen = BTreeSet::new();
                if selected.len() > 1
                    || !facts.activations.iter().any(|row|
                        row.searching && row.paid_x == *paid_x && row.source.original_source == *source)
                    || selected.iter().chain(legal).any(|card| {
                        let IdentityV1::Known { card_id } = card.identity else { return true };
                        card.zone != Zone::Library || card.owner != viewer || !seen.insert(card.object)
                            || crate::card_def::CARD_DEFS.get(usize::from(card_id))
                                .is_none_or(|def| !def.has_type(crate::card_def::CardType::Creature)
                                    || def.mana_value > u16::from(*paid_x))
                    })
                {
                    return Err("Fiend Artisan private search menu changed original paid bound".into());
                }
                source
            }
        };
        if !exact_source(source) || source.controller != viewer {
            return Err("Fiend Artisan private choice changed its controller".into());
        }
    }
    Ok(())
}

pub(crate) fn action(facts: Option<&FactsV1>, action: &Action) -> Option<ActionV1> {
    let facts = facts?;
    match (&facts.chooser, action) {
        (_, Action::CastSpell { source, .. }) if source.card_db_id == PRINTING => Some(ActionV1::Cast),
        (_, Action::ActivateAbility { source, ability_index: 0, granted_ability, .. })
            if source.card_db_id == PRINTING && granted_ability.is_none() =>
            Some(ActionV1::Activate),
        (Some(ChoiceV1::PayX { source, maximum }),
            Action::ChooseEffectOption { source: native, option_index, option_count, .. })
            if binds(source, native) && *option_count == u16::from(*maximum) + 1
                && *option_index <= u16::from(*maximum) =>
            Some(ActionV1::PayX { x: *option_index as u8 }),
        (Some(ChoiceV1::PayX { source, maximum: 0 }),
            Action::ChooseEffectNumber { source: native, minimum: 0, maximum: 0, number: 0, .. })
            if binds(source, native) => Some(ActionV1::PayX { x: 0 }),
        (Some(ChoiceV1::Sacrifice { source, legal, .. }),
            Action::ChooseCostTarget { source: native, candidate, remaining: 1,
                cost_kind: crate::engine::CostKind::SacrificeCreatures, .. })
            if binds(source, native) && legal.iter().any(|card| binds(card, candidate)) =>
            Some(ActionV1::Sacrifice),
        (Some(ChoiceV1::Search { source, selected, legal, .. }),
            Action::ChooseEffectTarget { source: native, target: TargetRefV1::Object { object },
                selected_count, min_targets: 0, max_targets: 1, .. })
            if binds(source, native) && usize::from(*selected_count) == selected.len()
                && legal.iter().any(|card| binds(card, object)) => Some(ActionV1::ChooseCreature),
        (Some(ChoiceV1::Search { source, selected, .. }),
            Action::FinishEffectSelection { source: native, selected_count, .. })
            if binds(source, native) && usize::from(*selected_count) == selected.len() =>
            Some(ActionV1::FinishSearch { selected_creature: !selected.is_empty() }),
        _ => None,
    }
}

pub(crate) fn validate_candidate_order<'a>(
    facts: &FactsV1,
    actions: impl Iterator<Item = &'a Action>,
) -> Result<(), String> {
    let Some(choice) = &facts.chooser else { return Ok(()) };
    let actions: Vec<_> = actions.collect();
    let annotations = actions.iter().map(|row| action(Some(facts), row));
    let valid = match choice {
        ChoiceV1::PayX { maximum, .. } =>
            annotations.eq((0..=*maximum).map(|x| Some(ActionV1::PayX { x }))),
        ChoiceV1::Sacrifice { legal, .. } => actions.len() == legal.len()
            && actions.iter().zip(legal).all(|(row, expected)| matches!(
                row, Action::ChooseCostTarget { candidate, .. } if binds(expected, candidate)
            ) && action(Some(facts), row) == Some(ActionV1::Sacrifice)),
        ChoiceV1::Search { selected, legal, .. } => actions.len() == legal.len() + 1
            && actions.iter().zip(legal).all(|(row, expected)| matches!(
                row, Action::ChooseEffectTarget { target: TargetRefV1::Object { object }, .. }
                    if binds(expected, object)
            ) && action(Some(facts), row) == Some(ActionV1::ChooseCreature))
            && annotations.last() == Some(Some(ActionV1::FinishSearch {
                selected_creature: !selected.is_empty(),
            })),
    };
    if !valid {
        return Err("Fiend Artisan candidate order differs from authenticated native menu".into());
    }
    Ok(())
}

#[cfg(all(test, feature = "limited-fdn-fixtures", not(feature = "standard-magezero-fixtures")))]
mod tests;
