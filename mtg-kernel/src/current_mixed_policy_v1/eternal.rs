//! Numeric transport from the separate sealed Eternal owner. No public JSON
//! importer, world constructor or executable authority is exposed here.
use super::{normalize, Packet, Result};
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct EternalPublicV1 {
    pub observation: crate::rl::ObservationV5,
    pub dynamic: crate::eternal_actor_public_v1::PublicChoicesV1,
    pub selection: Option<crate::eternal_actor_choice_v1::SelectionV1>,
    pub stack: Vec<crate::eternal_actor_stack_v1::StackProgramV1>,
    pub pending: Vec<crate::eternal_actor_stack_v1::PendingProgramV1>,
    pub resolving: Option<crate::eternal_actor_stack_v1::PendingProgramV1>,
}

pub(crate) fn eternal_packet_v1(
    public: &EternalPublicV1,
    actions: &[(
        crate::rl::ActionSemanticV1,
        Option<crate::eternal_actor_public_v1::EntryOptionV1>,
    )],
) -> Result<Packet> {
    normalize::encode_eternal(public, actions)
}
