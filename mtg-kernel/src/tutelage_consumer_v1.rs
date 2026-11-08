//! Declared-deck owning consumer for Sphinx's Tutelage native draw and mill choices.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    current_mixed_policy_v1::KaitoSession,
    human_bo3_v1::HumanKaitoSessionV75,
    limited_consumer_wire_v1::ProfileWireV1,
    optional_trigger_public_v1::{IdentityV1, ReferenceV1, RootV1},
    rl::{ActionSemanticV1 as Action, CardStableRefV1, PlayerSeatV1, TargetRefV1},
    rl_session::FastActorSessionV1,
    state::Zone,
    tutelage_public_v1::{self as public, FactsV1, OrderV1, StageV1, TriggerV1, PRINTING},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-tutelage-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/tutelage-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/tutelage-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/tutelage-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("tutelage_consumer_v1.rs"),
            include_str!("human_bo3_v1/tutelage_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_tutelage_env_v1.rs"),
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
    FastActorSessionV1::reset_limited_tutelage_v1(config, false)
        .map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_tutelage_v1(config, true)
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

fn original_source(root: &RootV1, controller: PlayerSeatV1) -> bool {
    root.captured_controller == controller
        && root.original_source.zone == Zone::Battlefield
        && root.original_source.controller == controller
        && matches!(
            root.original_source.identity,
            IdentityV1::Known { card_id: PRINTING } | IdentityV1::FaceDown { .. }
        )
        && root.current_original_source.as_ref().is_none_or(|current| {
            current.zone == Zone::Battlefield
                && current.object == root.original_source.object
                && current.generation == root.original_source.generation
                && current.owner == root.original_source.owner
        })
}

fn order_trigger<'a>(facts: &'a FactsV1, order: &OrderV1) -> Option<&'a TriggerV1> {
    facts
        .draws
        .get(order.draw_ordinal as usize)?
        .observers
        .get(order.observer_ordinal as usize)
}

pub(crate) fn validate_facts(facts: &FactsV1, actor: PlayerSeatV1) -> Result<(), String> {
    if facts.viewer != actor
        || facts.schema != public::SCHEMA
        || facts.meaning != public::printed_meaning()?
    {
        return Err("invalid Tutelage viewer facts".into());
    }
    for draw in &facts.draws {
        if draw.observers.is_empty() {
            return Err("empty Tutelage draw observer roster".into());
        }
        for observer in &draw.observers {
            let has_target = matches!(
                observer.stage,
                StageV1::Published
                    | StageV1::Resolving
                    | StageV1::MillEnded
                    | StageV1::LeftStackBeforeMill
            );
            if !original_source(&observer.source, draw.player)
                || observer.original_target.is_some() != has_target
                || observer.original_target == Some(draw.player)
                || !matches!(observer.stage, StageV1::Resolving | StageV1::MillEnded)
                    && !observer.batches.is_empty()
            {
                return Err("invalid Tutelage observer or target relation".into());
            }
            for (index, batch) in observer.batches.iter().enumerate() {
                if !(1..=2).contains(&batch.attempted_count)
                    || batch.visible_milled_in_move_order.len() > usize::from(batch.attempted_count)
                    || index > 0 && !observer.batches[index - 1].repeated
                    || batch.visible_milled_in_move_order.iter().any(|card| {
                        Some(card.owner) != observer.original_target
                            || !matches!(card.zone, Zone::Graveyard | Zone::Exile | Zone::Battlefield | Zone::Stack)
                            || !matches!(card.identity, IdentityV1::Known { card_id } if crate::card_def::CARD_DEFS.get(usize::from(card_id)).is_some())
                    })
                {
                    return Err("invalid Tutelage public batch".into());
                }
                let repeats = match batch.visible_milled_in_move_order.as_slice() {
                    [a, b] => {
                        let (
                            IdentityV1::Known { card_id: a_id },
                            IdentityV1::Known { card_id: b_id },
                        ) = (&a.identity, &b.identity)
                        else {
                            unreachable!("validated above")
                        };
                        if a.object == b.object {
                            return Err("duplicate Tutelage public batch member".into());
                        }
                        crate::event::tutelage_card_pair_repeats_v1(
                            &crate::card_def::CARD_DEFS[usize::from(*a_id)],
                            &crate::card_def::CARD_DEFS[usize::from(*b_id)],
                        )
                    }
                    _ => false,
                };
                if batch.repeated != repeats {
                    return Err("invalid Tutelage repeat outcome".into());
                }
            }
        }
    }
    if let Some(order) = &facts.current_order {
        let trigger =
            order_trigger(facts, order).ok_or("Tutelage order missing original observer")?;
        if order.chooser != actor
            || trigger.original_target != Some(actor)
            || trigger.stage != StageV1::Resolving
            || order.legal.is_empty()
            || order.selected.len() + order.legal.len() != 2
            || order
                .selected
                .iter()
                .chain(&order.legal)
                .any(|card| card.owner != actor || card.zone != Zone::Library)
        {
            return Err("invalid Tutelage chooser-private order".into());
        }
    }
    if facts
        .activation_discard
        .as_ref()
        .is_some_and(|root| !original_source(root, root.captured_controller))
    {
        return Err("invalid Tutelage activation discard source".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ActionV1 {
    ActivateDrawDiscard,
    TargetOpponent {
        draw_ordinal: u32,
        observer_ordinal: u32,
    },
    OrderMilledCard {
        draw_ordinal: u32,
        observer_ordinal: u32,
        selected_count: u16,
    },
    DiscardAfterDraw,
}

fn binds(card: &ReferenceV1, stable: &CardStableRefV1) -> bool {
    let card_id = match card.identity {
        IdentityV1::Known { card_id } => Some(card_id),
        IdentityV1::FaceDown { disguised } => crate::card_def::card_id_by_name(if disguised {
            "Face-down creature"
        } else {
            "Face-down card"
        }),
    };
    card_id == Some(stable.card_db_id)
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
            source,
            ability_index: 0,
            granted_ability,
            ..
        } if source.card_db_id == PRINTING && granted_ability.is_none() => {
            Ok(Some(ActionV1::ActivateDrawDiscard))
        }
        Action::ChooseTarget {
            actor,
            source,
            target,
            remaining,
        } => {
            for (draw_index, draw) in facts.draws.iter().enumerate() {
                for (observer_index, trigger) in draw.observers.iter().enumerate() {
                    if trigger.stage != StageV1::ChoosingOpponent
                        || !binds(&trigger.source.original_source, source)
                    {
                        continue;
                    }
                    if *actor != draw.player
                        || *remaining != 1
                        || !matches!(target, TargetRefV1::Player { player } if player != actor)
                    {
                        return Err(
                            "Tutelage target action differs from original native choice".into()
                        );
                    }
                    return Ok(Some(ActionV1::TargetOpponent {
                        draw_ordinal: u32::try_from(draw_index)
                            .map_err(|_| "Tutelage draw ordinal overflow")?,
                        observer_ordinal: u32::try_from(observer_index)
                            .map_err(|_| "Tutelage observer ordinal overflow")?,
                    }));
                }
            }
            Ok(None)
        }
        Action::ChooseEffectTarget {
            actor,
            source,
            target,
            selected_count,
            min_targets,
            max_targets,
        } => {
            let Some(order) = &facts.current_order else {
                return Ok(None);
            };
            let row = order_trigger(facts, order).ok_or("Tutelage missing current order root")?;
            if *actor != order.chooser
                || !binds(&row.source.original_source, source)
                || (*min_targets, *max_targets) != (2, 2)
                || usize::from(*selected_count) != order.selected.len()
                || !matches!(target, TargetRefV1::Object { object } if order.legal.iter().any(|card| binds(card, object)))
            {
                return Err("Tutelage action differs from original mill order".into());
            }
            Ok(Some(ActionV1::OrderMilledCard {
                draw_ordinal: order.draw_ordinal,
                observer_ordinal: order.observer_ordinal,
                selected_count: *selected_count,
            }))
        }
        Action::Discard { actor, cards }
            if facts
                .activation_discard
                .as_ref()
                .is_some_and(|root| root.captured_controller == *actor) =>
        {
            if cards.len() != 1 {
                return Err("Tutelage draw/discard action changed".into());
            }
            Ok(Some(ActionV1::DiscardAfterDraw))
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
