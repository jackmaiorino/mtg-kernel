//! Declared-deck owner and wire for Curator's private look and two-pile resolution.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    curator_public_v1::{
        self as public, CardsStageV1, ChoiceV1, FactsV1, PileV1, PilesV1, StageV1, PRINTING,
    },
    current_mixed_policy_v1::KaitoSession,
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

pub const PROFILE_V1: &str = "fdn-curator-of-destinies-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/curator-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/curator-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/curator-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("curator_consumer_v1.rs"),
            include_str!("human_bo3_v1/curator_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_curator_env_v1.rs"),
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
    FastActorSessionV1::reset_limited_curator_v1(config, false)
        .map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_curator_v1(config, true)
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

fn valid_names(names: &[public::NameCountV1]) -> bool {
    names.iter().all(|row| {
        row.copies > 0
            && crate::card_def::CARD_DEFS
                .get(usize::from(row.card_id))
                .is_some()
    }) && names
        .windows(2)
        .all(|pair| pair[0].card_id < pair[1].card_id)
}

fn total(names: &[public::NameCountV1]) -> usize {
    names.iter().map(|row| usize::from(row.copies)).sum()
}

fn valid_piles(piles: &PilesV1) -> bool {
    valid_names(&piles.face_up) && usize::from(piles.face_down_count) + total(&piles.face_up) <= 5
}

pub(crate) fn validate_facts(facts: &FactsV1, actor: PlayerSeatV1) -> Result<(), String> {
    if facts.viewer != actor
        || facts.schema != public::SCHEMA
        || facts.meaning != public::printed_meaning()?
    {
        return Err("Curator facts lost exact viewer or printing".into());
    }
    for entry in &facts.entries {
        if entry.source.original_source.zone != Zone::Battlefield
            || entry.source.original_source.identity != (IdentityV1::Known { card_id: PRINTING })
            || entry.source.original_source.controller != entry.source.captured_controller
        {
            return Err("Curator entry lost its original public source".into());
        }
        match &entry.stage {
            StageV1::Resolving {
                shown_piles,
                chosen,
            } if shown_piles
                .as_ref()
                .is_some_and(|piles| !valid_piles(piles))
                || chosen.is_some() && shown_piles.is_none() =>
            {
                return Err("Curator in-progress public pile count changed".into())
            }
            StageV1::Resolved(outcome) => {
                if !valid_piles(&outcome.shown_piles)
                    || !valid_names(&outcome.revealed_hand_arrivals)
                    || total(&outcome.revealed_hand_arrivals)
                        > usize::from(outcome.actual_hand_arrivals)
                    || usize::from(outcome.actual_hand_arrivals)
                        + outcome.public_moves.len()
                        + usize::from(outcome.hidden_library_moves)
                        > usize::from(outcome.shown_piles.face_down_count)
                            + total(&outcome.shown_piles.face_up)
                    || outcome.public_moves.iter().any(|row| {
                        row.original.zone != Zone::Library
                            || row.original.owner != entry.source.captured_controller
                            || matches!(row.destination, Zone::Library | Zone::Hand)
                    })
                {
                    return Err("Curator resolved public outcomes changed native roster".into());
                }
            }
            _ => {}
        }
    }
    if let Some(choice) = &facts.chooser {
        let (source, chooser) = match choice {
            ChoiceV1::Cards {
                source,
                chooser,
                stage,
                looked,
                selected,
                legal,
                required,
                limit,
                can_finish,
            } => {
                let mut distinct = BTreeSet::new();
                let phase_order = *stage == CardsStageV1::OrderGraveyard;
                if *limit > 5
                    || usize::from(*limit) != selected.len() + legal.len()
                    || *can_finish != (selected.len() >= usize::from(*required))
                    || (phase_order && (*required != *limit || !looked.is_empty()))
                    || (!phase_order && (*required != 0 || looked.len() != usize::from(*limit)))
                    || selected.iter().chain(legal).any(|card| {
                        card.owner != actor
                            || card.zone != Zone::Library
                            || !distinct.insert(card.object)
                            || !phase_order && !looked.contains(card)
                    })
                    || !phase_order
                        && looked
                            .iter()
                            .map(|card| card.object)
                            .collect::<BTreeSet<_>>()
                            != distinct
                {
                    return Err("Curator private card selection changed original candidates".into());
                }
                (source, *chooser)
            }
            ChoiceV1::Pile {
                source,
                chooser,
                piles,
            } => {
                if !valid_piles(piles) || source.controller == actor {
                    return Err(
                        "Curator opponent pile menu exposed invalid public information".into(),
                    );
                }
                (source, *chooser)
            }
        };
        if chooser != actor
            || !facts.entries.iter().any(|entry| {
                entry.source.original_source == *source
                    && matches!(entry.stage, StageV1::Resolving { .. })
            })
        {
            return Err("Curator menu lost its original public resolving occurrence".into());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ActionV1 {
    ChooseCard {
        stage: CardsStageV1,
        selected_count: u16,
    },
    FinishFaceDownPile {
        selected_count: u16,
    },
    ChoosePile {
        pile: PileV1,
    },
}

fn binds(card: &ReferenceV1, stable: &CardStableRefV1) -> bool {
    let id = match card.identity {
        IdentityV1::Known { card_id } => Some(card_id),
        IdentityV1::FaceDown { disguised } => crate::card_def::card_id_by_name(if disguised {
            "Face-down creature"
        } else {
            "Face-down card"
        }),
    };
    id == Some(stable.card_db_id)
        && card.object == stable.arena_id
        && card.generation == stable.zone_change_count
        && card.owner == stable.owner
        && card.controller == stable.controller
        && card.zone == stable.zone
}

pub(crate) fn action(facts: Option<&FactsV1>, action: &Action) -> Result<Option<ActionV1>, String> {
    let Some(facts) = facts else { return Ok(None) };
    let result = match (facts.chooser.as_ref(), action) {
        (
            Some(ChoiceV1::Cards {
                source,
                chooser,
                stage,
                legal,
                selected,
                required,
                limit,
                ..
            }),
            Action::ChooseEffectTarget {
                actor,
                source: actual,
                target,
                selected_count,
                min_targets,
                max_targets,
            },
        ) if *chooser == *actor
            && binds(source, actual)
            && usize::from(*selected_count) == selected.len()
            && *min_targets == *required
            && *max_targets == *limit
            && matches!(target, TargetRefV1::Object { object }
                    if legal.iter().any(|card| binds(card, object))) =>
        {
            Some(ActionV1::ChooseCard {
                stage: *stage,
                selected_count: *selected_count,
            })
        }
        (
            Some(ChoiceV1::Cards {
                source,
                chooser,
                stage: CardsStageV1::SetFaceDownPile,
                selected,
                can_finish: true,
                ..
            }),
            Action::FinishEffectSelection {
                actor,
                source: actual,
                selected_count,
            },
        ) if *chooser == *actor
            && binds(source, actual)
            && usize::from(*selected_count) == selected.len() =>
        {
            Some(ActionV1::FinishFaceDownPile {
                selected_count: *selected_count,
            })
        }
        (
            Some(ChoiceV1::Pile {
                source, chooser, ..
            }),
            Action::ChooseEffectBoolean {
                actor,
                source: actual,
                value,
            },
        ) if *chooser == *actor && binds(source, actual) => Some(ActionV1::ChoosePile {
            pile: if *value {
                PileV1::FaceDown
            } else {
                PileV1::FaceUp
            },
        }),
        (None, _) => None,
        _ => {
            return Err(
                "Curator candidate changed original chooser, source or native option".into(),
            )
        }
    };
    Ok(result)
}

pub(crate) fn validate_candidate_order<'a>(
    facts: &FactsV1,
    mut actions: impl ExactSizeIterator<Item = &'a Action>,
) -> Result<(), String> {
    let valid = match &facts.chooser {
        Some(ChoiceV1::Cards { legal, can_finish, .. }) => {
            actions.len() == legal.len() + usize::from(*can_finish)
                && actions.by_ref().take(legal.len()).zip(legal).all(|(action, card)|
                    matches!(action, Action::ChooseEffectTarget { target: TargetRefV1::Object { object }, .. }
                        if binds(card, object)))
                && (!can_finish || matches!(actions.next(), Some(Action::FinishEffectSelection { .. })))
        }
        Some(ChoiceV1::Pile { .. }) =>
            actions.map(|action| match action {
                Action::ChooseEffectBoolean { value, .. } => Some(*value),
                _ => None,
            }).eq([Some(false), Some(true)]),
        None => true,
    };
    if valid {
        Ok(())
    } else {
        Err("Curator candidate vector changed original native order".into())
    }
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
