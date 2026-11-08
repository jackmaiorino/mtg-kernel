//! Declared-deck owner for the exact Banner and Courtyard printings.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    banner_of_kinship_v1::entry,
    card_def::{CARD_DEFS, KERNEL_CARDDB_HASH},
    chosen_type_public_v66::{ChoiceV66, FactsV66},
    creature_types_v60::CreatureTypeV60,
    current_mixed_policy_v1::KaitoSession,
    engine::{Action, Decision},
    human_bo3_v1::HumanKaitoSessionV75,
    ids::PlayerId,
    limited_consumer_wire_v1::ProfileWireV1,
    linked_aura_operation_v1::{GeneralChoiceV36, OperationV1},
    policy_surface_v5::{PolicyActionV5, PolicyDecisionV5},
    rl::{ActionSemanticV1, CorePolicyActionCandidateV1, PlayerSeatV1, RlContractError},
    rl_session::FastActorSessionV1,
    rules_meaning_chosen_type_v66::{self as meaning, FamilyV66},
    state::{GameState, Zone},
    surface::{SurfaceAction, SurfaceDecision},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-chosen-type-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/chosen-type-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/chosen-type-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/chosen-type-candidate-meaning/v1";
const FAMILIES: [FamilyV66; 2] = [FamilyV66::BannerOfKinship, FamilyV66::SecludedCourtyard];

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("chosen_type_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("human_bo3_v1/chosen_type_v1.rs"),
            include_str!("bin/kernel_limited_chosen_type_env_v1.rs"),
        ] {
            hash.update([0]);
            hash.update(source.as_bytes());
        }
        hash.update(meaning::producer_sha256());
        hash.update(crate::limited_kaito_session_v16::producer_sha256_v16());
        hash.update(crate::current_mixed_policy_v1::feature_identity());
        format!("{:x}", hash.finalize())
    })
    .clone()
}

pub(crate) fn preflight(config: &ResetV1) -> Result<(), String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err("chosen-type profile requires the Foundations catalog".into());
    }
    if config.expected_card_db_hash != KERNEL_CARDDB_HASH
        || config.max_physical_decisions == 0
        || config.max_policy_steps == 0
        || config.mainboards.iter().any(|deck| {
            !(40..=crate::limited_session_v1::MAX_CUSTOM_DECK_CARDS_V1).contains(&deck.len())
        })
    {
        return Err("invalid chosen-type profile reset".into());
    }
    for family in FAMILIES {
        meaning::selected(family)?;
    }
    for &id in config.mainboards.iter().flatten() {
        let def = CARD_DEFS
            .get(usize::from(id))
            .ok_or("unknown declared card")?;
        if def.is_token || !(def.has_full_support() || entry::owns_entry_definition(id)) {
            return Err(format!(
                "card {id} is outside the chosen-type consumer profile"
            ));
        }
        if !entry::owns_entry_definition(id) {
            crate::rl::require_declared_cards_for_scope(
                crate::rl::CommonProjectionScopeV42::KaitoV75,
                std::slice::from_ref(&id),
            )?;
        }
    }
    Ok(())
}

fn native(config: &ResetV1) -> Result<FastActorSessionV1, String> {
    FastActorSessionV1::reset_limited_chosen_type_v1(config, false)
        .map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_chosen_type_v1(config, true)
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub native: FactsV66,
    /// Name of the card currently entering, authorized by the native checkpoint.
    /// This contains no handle into a pre-entry hand or other hidden zone.
    pub active_printing: Option<FamilyV66>,
}

fn active_choice(state: &GameState) -> Option<(&entry::ChoiceV1, &OperationV1)> {
    let checkpoint = state
        .engine
        .linked_aura_operation_v1
        .pending_kinship_v1
        .as_ref()?;
    match &checkpoint.choice {
        GeneralChoiceV36::KinshipV1 { choice, .. } => Some((choice, &checkpoint.operation)),
        _ => None,
    }
}

fn active_printing(state: &GameState, viewer: PlayerId) -> Result<Option<FamilyV66>, String> {
    let Some((choice, operation)) = active_choice(state) else {
        return Ok(None);
    };
    if choice.controller != viewer {
        return Ok(None);
    }
    let visible = choice.original.as_ref().is_some_and(|original| {
        original.v4.face_down_v1.is_none()
            && (matches!(
                original.zone,
                Zone::Stack | Zone::Battlefield | Zone::Graveyard | Zone::Exile | Zone::Command
            ) || (original.zone == Zone::Hand && original.owner == viewer))
    }) || matches!(operation, OperationV1::CopyTokenKinshipV1 { source, .. }
        if choice.original.is_none() && source.card_def == choice.card_def
            && source.zone == Zone::Battlefield
            && state.objects.try_get(source.source).is_some_and(|live| live.v4.face_down_v1.is_none()));
    if !visible {
        return Err("chosen-type entry source is not public to its controller".into());
    }
    FAMILIES
        .into_iter()
        .find(|family| family.card_id() == choice.card_def)
        .map(Some)
        .ok_or("chosen-type entry lost its exact printing".into())
}

pub(crate) fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV1, String> {
    let mut native = crate::chosen_type_public_v66::capture(state, viewer)?;
    native.visible_printings.retain(|reference| {
        !matches!(reference.zone, Zone::Hand | Zone::Library) || reference.owner == native.viewer
    });
    let active_printing = active_printing(state, viewer)?;
    if active_printing.is_none() {
        native.active_entry_choice = None;
    }
    let facts = FactsV1 {
        native,
        active_printing,
    };
    validate_facts(&facts, viewer.into())?;
    Ok(facts)
}

pub(crate) fn validate_facts(facts: &FactsV1, actor: PlayerSeatV1) -> Result<(), String> {
    let native = &facts.native;
    if native.schema != crate::chosen_type_public_v66::SCHEMA
        || native.producer_sha256 != meaning::producer_sha256()
        || native.viewer != actor
        || native.complete_ordinary_observation
        || native.owning_consumer_admitted
        || native.meanings.len() != FAMILIES.len()
        || native
            .meanings
            .iter()
            .zip(FAMILIES)
            .any(|(value, family)| value.family != family)
        || native
            .visible_printings
            .iter()
            .any(|r| matches!(r.zone, Zone::Hand | Zone::Library) && r.owner != actor)
    {
        return Err("invalid chosen-type facts".into());
    }
    for value in &native.meanings {
        meaning::validate(value)?;
    }
    match (&native.active_entry_choice, facts.active_printing) {
        (None, None) => Ok(()),
        (Some(choice), Some(_))
            if choice.controller == actor && choice.domain == CreatureTypeV60::ALL =>
        {
            Ok(())
        }
        _ => Err("invalid active chosen-type facts".into()),
    }
}

pub(crate) fn entry_candidates(
    state: &GameState,
    origin: &PolicyDecisionV5,
) -> Result<Option<Vec<CorePolicyActionCandidateV1>>, RlContractError> {
    let PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::ChooseEntryCreatureTypeV1 {
        player,
        entry_ordinal,
        types,
    })) = origin
    else {
        return Ok(None);
    };
    let (choice, _) = active_choice(state)
        .ok_or_else(|| RlContractError("chosen creature type lost its entry owner".into()))?;
    if choice.controller != *player
        || choice.ordinal != *entry_ordinal
        || !entry::owns_entry_definition(choice.card_def)
        || types.as_slice() != CreatureTypeV60::ALL
    {
        return Err(RlContractError(
            "chosen creature type entry menu changed".into(),
        ));
    }
    Ok(Some(
        types
            .iter()
            .copied()
            .map(|creature_type| CorePolicyActionCandidateV1 {
                semantic: ActionSemanticV1::ChooseEntryCreatureTypeV60 {
                    actor: (*player).into(),
                    entry_ordinal: *entry_ordinal,
                    creature_type,
                },
                policy_action: PolicyActionV5::Surface(SurfaceAction::Action(
                    Action::ChooseEffectOption(creature_type.choice_index()),
                )),
            })
            .collect(),
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerV1 {
    pub printing: FamilyV66,
    pub entry_ordinal: u32,
    pub creature_type: CreatureTypeV60,
}

pub(crate) fn answer(
    facts: Option<&FactsV1>,
    action: &ActionSemanticV1,
) -> Result<Option<AnswerV1>, String> {
    let active = facts.and_then(|facts| {
        facts
            .active_printing
            .zip(facts.native.active_entry_choice.as_ref())
    });
    match (active, action) {
        (
            Some((
                printing,
                ChoiceV66 {
                    controller,
                    entry_ordinal,
                    domain,
                },
            )),
            ActionSemanticV1::ChooseEntryCreatureTypeV60 {
                actor,
                entry_ordinal: ordinal,
                creature_type,
            },
        ) if actor == controller && ordinal == entry_ordinal && domain.contains(creature_type) => {
            Ok(Some(AnswerV1 {
                printing,
                entry_ordinal: *ordinal,
                creature_type: *creature_type,
            }))
        }
        (None, ActionSemanticV1::ChooseEntryCreatureTypeV60 { .. }) | (Some(_), _) => {
            Err("candidate differs from its active chosen-type entry".into())
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
