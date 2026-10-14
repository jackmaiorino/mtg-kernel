use super::*;
use crate::doubling_season_public_v48 as public;
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanSeasonCreationV48 {
    pub occurrence: usize,
    pub controller: PlayerSeatV1,
    pub token_name: String,
    pub requested: usize,
    pub created: Vec<String>,
    pub replacement_sources: Vec<HumanCardRefV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanSeasonCounterV48 {
    pub occurrence: usize,
    pub kind: public::CounterKindV48,
    pub recipient: String,
    pub actor: PlayerSeatV1,
    pub recipient_controller: PlayerSeatV1,
    pub original: i32,
    pub placed: i32,
    pub replacement_sources: Vec<HumanCardRefV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanSeasonAttachmentV48 {
    pub occurrence: usize,
    pub original_equipment: HumanCardRefV1,
    pub created_controller: PlayerSeatV1,
    pub selected: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanSeasonAttachmentMenuV48 {
    pub equipment: HumanCardRefV1,
    pub chooser: PlayerSeatV1,
    pub ordered_created: Vec<HumanCardRefV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanSeasonFactsV48 {
    pub active_sources: Vec<HumanCardRefV1>,
    pub creations: Vec<HumanSeasonCreationV48>,
    pub counter_placements: Vec<HumanSeasonCounterV48>,
    pub attachment_menu: Option<HumanSeasonAttachmentMenuV48>,
    pub attachments: Vec<HumanSeasonAttachmentV48>,
}
pub(super) fn register_season(
    facts: &public::FactsV48,
    handles: &mut Handles,
) -> Result<(), Error> {
    for source in facts
        .active_sources
        .iter()
        .chain(
            facts
                .creations
                .iter()
                .flat_map(|row| &row.replacement_sources),
        )
        .chain(
            facts
                .counter_placements
                .iter()
                .flat_map(|row| &row.replacement_sources),
        )
    {
        handles.insert(source)?;
    }
    for row in &facts.attachments {
        handles.insert(&row.original_equipment)?;
    }
    if let Some(menu) = &facts.attachment_menu {
        handles.insert(&menu.equipment)?;
        for token in &menu.ordered_created {
            handles.insert(token)?;
        }
    }
    Ok(())
}
pub(super) fn project_season(
    facts: &public::FactsV48,
    handles: &Handles,
) -> Result<HumanSeasonFactsV48, Error> {
    let sources = |rows: &[crate::rl::CardStableRefV1]| {
        rows.iter()
            .map(|source| handles.get(source).cloned())
            .collect::<Result<Vec<_>, Error>>()
    };
    let mut incarnations = Vec::new();
    let mut role = |recipient: public::IncarnationV48| {
        let n = incarnations
            .iter()
            .position(|saved| *saved == recipient)
            .unwrap_or_else(|| {
                let n = incarnations.len();
                incarnations.push(recipient);
                n
            });
        format!("replacement_incarnation_{}", n + 1)
    };
    let creations = facts
        .creations
        .iter()
        .map(|row| {
            let def = &crate::card_def::CARD_DEFS[usize::from(row.token_definition)];
            let name = if row.face_index == Some(1) {
                def.transform_face
                    .as_ref()
                    .map_or(def.object_name, |face| face.name)
            } else {
                def.object_name
            };
            Ok(HumanSeasonCreationV48 {
                occurrence: row.occurrence,
                controller: row.controller,
                token_name: if name.is_empty() {
                    "nameless creature token".into()
                } else {
                    name.into()
                },
                requested: row.requested,
                created: row.created.iter().copied().map(&mut role).collect(),
                replacement_sources: sources(&row.replacement_sources)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let counter_placements = facts
        .counter_placements
        .iter()
        .map(|row| {
            Ok(HumanSeasonCounterV48 {
                occurrence: row.occurrence,
                kind: row.kind,
                recipient: role(row.recipient),
                actor: row.actor,
                recipient_controller: row.recipient_controller,
                original: row.original,
                placed: row.placed,
                replacement_sources: sources(&row.replacement_sources)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let attachments = facts
        .attachments
        .iter()
        .map(|row| {
            Ok(HumanSeasonAttachmentV48 {
                occurrence: row.occurrence,
                original_equipment: handles.get(&row.original_equipment)?.clone(),
                created_controller: row.created_controller,
                selected: role(row.selected),
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let attachment_menu = facts
        .attachment_menu
        .as_ref()
        .map(|menu| {
            Ok(HumanSeasonAttachmentMenuV48 {
                equipment: handles.get(&menu.equipment)?.clone(),
                chooser: menu.chooser,
                ordered_created: menu
                    .ordered_created
                    .iter()
                    .map(|token| handles.get(token).cloned())
                    .collect::<Result<Vec<_>, Error>>()?,
            })
        })
        .transpose()?;
    Ok(HumanSeasonFactsV48 {
        active_sources: sources(&facts.active_sources)?,
        creations,
        counter_placements,
        attachment_menu,
        attachments,
    })
}
