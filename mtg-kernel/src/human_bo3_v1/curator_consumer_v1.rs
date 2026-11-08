//! Prompt-local Curator piles and actual public destinations.
use super::{
    muldrotha_v71::href,
    tinybones_v72::{HumanTinybonesRelationV72, Incarnations},
    visible::Handles,
    HumanCardRefV1, HumanDecisionErrorV1 as Error, PlayerSeatV1,
};
use crate::{
    curator_consumer_v1::ActionV1,
    curator_public_v1::{CardsStageV1, ChoiceV1, FactsV1, MeaningV1, NameCountV1, PileV1, PilesV1, StageV1},
    optional_trigger_public_v1::ReferenceV1,
    rl::{ActionSemanticV1 as Action, TargetRefV1},
    state::Zone,
    tinybones_public_v72::IncarnationV72,
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFactsV1 {
    meaning: MeaningV1,
    entries: Vec<HumanEntryV1>,
    chooser: Option<HumanChoiceV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanNameCountV1 {
    name: String,
    copies: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanPilesV1 {
    face_down_count: u8,
    face_up: Vec<HumanNameCountV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanEntryV1 {
    original_source: HumanTinybonesRelationV72,
    current_original_source: Option<HumanCardRefV1>,
    stage: HumanStageV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
enum HumanStageV1 {
    Entered,
    Held { position: u32 },
    Published { stack_position: u32 },
    Resolving { shown_piles: Option<HumanPilesV1>, chosen: Option<PileV1> },
    Resolved {
        chosen: PileV1,
        shown_piles: HumanPilesV1,
        actual_hand_arrivals: u8,
        revealed_hand_arrivals: Vec<HumanNameCountV1>,
        public_moves: Vec<HumanMoveV1>,
        hidden_library_moves: u8,
    },
    Countered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanMoveV1 {
    original: HumanTinybonesRelationV72,
    destination: Zone,
    name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
enum HumanChoiceV1 {
    Cards {
        source: HumanTinybonesRelationV72,
        chooser: PlayerSeatV1,
        stage: CardsStageV1,
        looked: Vec<HumanCardRefV1>,
        selected: Vec<HumanCardRefV1>,
        legal: Vec<HumanCardRefV1>,
        required: u16,
        limit: u16,
        can_finish: bool,
    },
    Pile { source: HumanTinybonesRelationV72, chooser: PlayerSeatV1, piles: HumanPilesV1 },
}

fn name(id: u16) -> Result<String, Error> {
    crate::card_def::CARD_DEFS.get(usize::from(id))
        .map(|definition| definition.name.to_string()).ok_or(Error::InvalidVisibleReference)
}

fn names(rows: &[NameCountV1]) -> Result<Vec<HumanNameCountV1>, Error> {
    rows.iter().map(|row| Ok(HumanNameCountV1 {
        name: name(row.card_id)?, copies: row.copies,
    })).collect()
}

fn piles(rows: &PilesV1) -> Result<HumanPilesV1, Error> {
    Ok(HumanPilesV1 { face_down_count: rows.face_down_count, face_up: names(&rows.face_up)? })
}

fn original(card: &ReferenceV1, roles: &mut Incarnations) -> Result<HumanTinybonesRelationV72, Error> {
    roles.relation(&IncarnationV72 {
        object: card.object, generation: card.generation, owner: card.owner,
        controller: card.controller, zone: card.zone,
    })
}

pub(super) fn project(
    facts: &FactsV1, handles: &mut Handles, roles: &mut Incarnations,
) -> Result<HumanFactsV1, Error> {
    let entries = facts.entries.iter().map(|entry| {
        let stage = match &entry.stage {
            StageV1::Entered => HumanStageV1::Entered,
            StageV1::Held { position } => HumanStageV1::Held { position: *position },
            StageV1::Published { stack_position } => HumanStageV1::Published { stack_position: *stack_position },
            StageV1::Resolving { shown_piles, chosen } => HumanStageV1::Resolving {
                shown_piles: shown_piles.as_ref().map(piles).transpose()?, chosen: *chosen,
            },
            StageV1::Resolved(row) => HumanStageV1::Resolved {
                chosen: row.chosen,
                shown_piles: piles(&row.shown_piles)?,
                actual_hand_arrivals: row.actual_hand_arrivals,
                revealed_hand_arrivals: names(&row.revealed_hand_arrivals)?,
                public_moves: row.public_moves.iter().map(|move_row| Ok(HumanMoveV1 {
                    original: roles.relation(&move_row.original)?,
                    destination: move_row.destination,
                    name: move_row.card_id.map(name).transpose()?,
                })).collect::<Result<_, Error>>()?,
                hidden_library_moves: row.hidden_library_moves,
            },
            StageV1::Countered => HumanStageV1::Countered,
        };
        Ok(HumanEntryV1 {
            original_source: original(&entry.source.original_source, roles)?,
            current_original_source: entry.source.current_original_source.as_ref()
                .map(|card| href(card, handles)).transpose()?,
            stage,
        })
    }).collect::<Result<_, Error>>()?;
    let chooser = facts.chooser.as_ref().map(|choice| -> Result<_, Error> {
        match choice {
            ChoiceV1::Cards { source, chooser, stage, looked, selected, legal, required, limit, can_finish } => {
                let source = original(source, roles)?;
                let mut cards = |rows: &[ReferenceV1]| -> Result<Vec<_>, Error> {
                    rows.iter().map(|row| href(row, handles)).collect()
                };
                Ok(HumanChoiceV1::Cards {
                    source, chooser: *chooser, stage: *stage,
                    looked: cards(looked)?, selected: cards(selected)?, legal: cards(legal)?,
                    required: *required, limit: *limit, can_finish: *can_finish,
                })
            }
            ChoiceV1::Pile { source, chooser, piles: rows } =>
                Ok(HumanChoiceV1::Pile { source: original(source, roles)?, chooser: *chooser, piles: piles(rows)? }),
        }
    }).transpose()?;
    Ok(HumanFactsV1 { meaning: facts.meaning.clone(), entries, chooser })
}

pub(super) fn label(
    facts: Option<&FactsV1>, action: &Action, handles: &Handles,
) -> Result<Option<String>, Error> {
    let annotated = crate::curator_consumer_v1::action(facts, action)
        .map_err(|_| Error::InvalidVisibleReference)?;
    Ok(match (annotated, action) {
        (None, _) => None,
        (Some(ActionV1::ChooseCard { stage, .. }), Action::ChooseEffectTarget {
            target: TargetRefV1::Object { object }, ..
        }) => {
            let card = handles.name(object)?;
            Some(match stage {
                CardsStageV1::SetFaceDownPile => format!("Put {card} in the face-down pile"),
                CardsStageV1::OrderGraveyard => format!("Put {card} next in your graveyard order"),
            })
        }
        (Some(ActionV1::FinishFaceDownPile { .. }), _) => Some("Finish dividing the two piles".into()),
        (Some(ActionV1::ChoosePile { pile }), _) => Some(match pile {
            PileV1::FaceUp => "Choose the face-up pile for Curator's controller",
            PileV1::FaceDown => "Choose the face-down pile for Curator's controller",
        }.into()),
        _ => return Err(Error::InvalidVisibleReference),
    })
}
