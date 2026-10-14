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
    pub recipient: String,
    pub actor: PlayerSeatV1,
    pub recipient_controller: PlayerSeatV1,
    pub original: i32,
    pub placed: i32,
    pub replacement_sources: Vec<HumanCardRefV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanSeasonFactsV48 {
    pub active_sources: Vec<HumanCardRefV1>,
    pub creations: Vec<HumanSeasonCreationV48>,
    pub counter_placements: Vec<HumanSeasonCounterV48>,
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
                recipient: role(row.recipient),
                actor: row.actor,
                recipient_controller: row.recipient_controller,
                original: row.original,
                placed: row.placed,
                replacement_sources: sources(&row.replacement_sources)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(HumanSeasonFactsV48 {
        active_sources: sources(&facts.active_sources)?,
        creations,
        counter_placements,
    })
}
