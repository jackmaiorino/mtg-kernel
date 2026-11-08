//! Declared-deck owning consumer for Blasphemous Edict's native payment and APNAP choices.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    current_mixed_policy_v1::KaitoSession,
    edict_public_v1::{self as public, FactsV1, PRINTING},
    human_bo3_v1::HumanKaitoSessionV75,
    limited_consumer_wire_v1::ProfileWireV1,
    optional_trigger_public_v1::{IdentityV1, ReferenceV1},
    rl::{ActionSemanticV1 as Action, CardStableRefV1, PlayerSeatV1, TargetRefV1},
    rl_session::FastActorSessionV1,
    state::Zone,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-blasphemous-edict-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/edict-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/edict-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/edict-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("edict_consumer_v1.rs"),
            include_str!("human_bo3_v1/edict_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_edict_env_v1.rs"),
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
    FastActorSessionV1::reset_limited_edict_v1(config, false).map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_edict_v1(config, true)
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

fn payment_valid(payment: public::PaymentV1) -> bool {
    match payment.method {
        crate::state::CastMethodV4::Normal => true,
        crate::state::CastMethodV4::Alternative => payment.creatures_before >= 13,
        _ => false,
    }
}

pub(crate) fn validate_facts(facts: &FactsV1, actor: PlayerSeatV1) -> Result<(), String> {
    if facts.viewer != actor
        || facts.schema != public::SCHEMA
        || facts.meaning != public::printed_meaning()?
        || facts.alternate_condition_now != (facts.effective_creatures_now >= 13)
    {
        return Err("invalid Edict actor facts".into());
    }
    for cast in &facts.casts {
        if printing(&cast.card) != Some(PRINTING)
            || cast.card.zone != Zone::Stack
            || cast.uncast_copy != cast.origin_zone.is_none()
            || (!cast.announcing && !cast.uncast_copy) != cast.payment.is_some()
            || cast.announcing && (cast.uncast_copy || cast.resolving)
            || cast.payment.is_some_and(|payment| !payment_valid(payment))
        {
            return Err("invalid Edict original paid spell".into());
        }
    }
    if facts.chooser_cast_quotes.is_some() && facts.chooser_sacrifice_or_order.is_some() {
        return Err("Edict cannot announce payment and resolve simultaneously".into());
    }
    if let Some(quotes) = &facts.chooser_cast_quotes {
        if quotes.card.controller != actor
            || !facts
                .casts
                .iter()
                .any(|cast| cast.announcing && cast.card == quotes.card)
            || quotes.modes.iter().map(|quote| quote.mode).ne([
                crate::engine::CastMode::Normal,
                crate::engine::CastMode::Alternative,
            ])
            || !facts.alternate_condition_now
        {
            return Err("invalid Edict chooser-only cast quote".into());
        }
    }
    if let Some(choice) = &facts.chooser_sacrifice_or_order {
        let mut seen = std::collections::BTreeSet::new();
        let ordering = choice.stage == public::ChoiceStageV1::PhysicalOwnerGraveyardOrder;
        if choice.chooser != actor
            || printing(&choice.source) != Some(PRINTING)
            || choice.source.zone != Zone::Stack
            || choice.legal.is_empty()
            || choice.selected.len() >= usize::from(choice.required)
            || if ordering {
                choice.selected.len() + choice.legal.len() != usize::from(choice.required)
            } else {
                choice.required != 13 || choice.selected.len() + choice.legal.len() <= 13
            }
            || choice.selected.iter().chain(&choice.legal).any(|card| {
                card.zone != Zone::Battlefield
                    || !seen.insert(card.object)
                    || (if ordering {
                        card.owner
                    } else {
                        card.controller
                    }) != actor
            })
            || choice
                .completed_controller_choices
                .iter()
                .enumerate()
                .any(|(index, completed)| {
                    let controller = [PlayerSeatV1::P0, PlayerSeatV1::P1][index];
                    completed.len() > 13
                        || completed.iter().any(|card| {
                            card.zone != Zone::Battlefield || card.controller != controller
                        })
                        || (!ordering
                            && (choice.chooser == choice.active || controller != choice.active)
                            && !completed.is_empty())
                })
        {
            return Err("invalid Edict chooser-only original selection".into());
        }
    }
    for row in &facts.resolved {
        let mut victims = std::collections::BTreeMap::new();
        if row.original_spell.zone != Zone::Stack
            || row.copied_spell == row.payment.is_some()
            || row.payment.is_some_and(|payment| !payment_valid(payment))
            || row.sacrifices_in_apnap_order.len() > 26
            || row.sacrifices_in_apnap_order.iter().any(|victim| {
                victim.original.zone != Zone::Battlefield
                    || victims.insert(victim.original.object, victim).is_some()
            })
        {
            return Err("invalid Edict realized simultaneous sacrifice".into());
        }
        let mut ordered = std::collections::BTreeSet::new();
        for (index, entries) in row.actual_graveyard_order.iter().enumerate() {
            let owner = [PlayerSeatV1::P0, PlayerSeatV1::P1][index];
            if entries.iter().any(|entry| {
                entry.owner != owner
                    || !ordered.insert(entry.object)
                    || !victims.get(&entry.object).is_some_and(|victim| {
                        victim.original == *entry
                            && victim.committed_destination == Some(Zone::Graveyard)
                    })
            }) {
                return Err("invalid Edict realized physical-owner graveyard order".into());
            }
        }
        if row.sacrifices_in_apnap_order.iter().any(|victim| {
            (victim.committed_destination == Some(Zone::Graveyard))
                != ordered.contains(&victim.original.object)
        }) {
            return Err("Edict realized graveyard roster is incomplete".into());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ActionV1 {
    ChooseCastPayment {
        mode: crate::engine::CastMode,
        payable_mana_now: u32,
    },
    ChoosePermanent {
        stage: public::ChoiceStageV1,
        selected_count: u16,
    },
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
        Action::ChooseCastMode {
            actor,
            source,
            mode,
        } if source.card_db_id == PRINTING => {
            let quotes = facts
                .chooser_cast_quotes
                .as_ref()
                .ok_or("Edict cast action lost its original quote")?;
            if *actor != facts.viewer || !binds(&quotes.card, source) {
                return Err("Edict cast action changed original caster or source".into());
            }
            let quote = quotes
                .modes
                .iter()
                .find(|quote| quote.mode == *mode)
                .ok_or("Edict action not in original payable mode order")?;
            Ok(Some(ActionV1::ChooseCastPayment {
                mode: quote.mode,
                payable_mana_now: quote.payable_mana_now,
            }))
        }
        Action::ChooseEffectTarget {
            actor,
            source,
            target,
            selected_count,
            min_targets,
            max_targets,
        } => {
            let Some(choice) = &facts.chooser_sacrifice_or_order else {
                return if source.card_db_id == PRINTING {
                    Err("Edict candidate lost its chooser's original selection".into())
                } else {
                    Ok(None)
                };
            };
            if choice.chooser != *actor
                || !binds(&choice.source, source)
                || usize::from(*selected_count) != choice.selected.len()
                || *min_targets != choice.required
                || *max_targets != choice.required
                || !matches!(target, TargetRefV1::Object { object } if choice.legal.iter().any(|card| binds(card, object)))
            {
                return Err("Edict action changed its original chooser or candidate".into());
            }
            Ok(Some(ActionV1::ChoosePermanent {
                stage: choice.stage,
                selected_count: *selected_count,
            }))
        }
        _ => Ok(None),
    }
}

pub(crate) fn validate_candidate_order<'a>(
    facts: &FactsV1,
    actions: impl ExactSizeIterator<Item = &'a Action>,
) -> Result<(), String> {
    if let Some(quotes) = &facts.chooser_cast_quotes {
        if actions.len() != quotes.modes.len()
            || !actions.zip(&quotes.modes).all(|(action, quote)| {
                matches!(action,
                Action::ChooseCastMode { mode, .. } if *mode == quote.mode)
            })
        {
            return Err("Edict cast options changed native order".into());
        }
    } else if let Some(choice) = &facts.chooser_sacrifice_or_order {
        if actions.len() != choice.legal.len()
            || !actions.zip(&choice.legal).all(|(action, card)| {
                matches!(action,
                Action::ChooseEffectTarget { target: TargetRefV1::Object { object }, .. }
                    if binds(card, object))
            })
        {
            return Err("Edict legal selections changed native order".into());
        }
    }
    Ok(())
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
