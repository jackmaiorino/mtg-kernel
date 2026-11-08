//! Declared-deck owner and wire for Quilled Greatwurm's combat and graveyard cost.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    effect::greatwurm_v1::CounterKindV1,
    greatwurm_public_v1::{self as public, ChoiceV1, FactsV1, GraveyardRouteV1, PRINTING},
    current_mixed_policy_v1::KaitoSession,
    human_bo3_v1::HumanKaitoSessionV75,
    limited_consumer_wire_v1::ProfileWireV1,
    optional_trigger_public_v1::{IdentityV1, ReferenceV1},
    rl::{ActionSemanticV1 as Action, CardStableRefV1, PlayerSeatV1},
    rl_session::FastActorSessionV1,
    state::Zone,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const PROFILE_V1: &str = "fdn-quilled-greatwurm-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/greatwurm-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/greatwurm-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/greatwurm-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1, include_str!("greatwurm_consumer_v1.rs"),
            include_str!("human_bo3_v1/greatwurm_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"), include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_greatwurm_env_v1.rs"),
        ] {
            hash.update((source.len() as u64).to_le_bytes());
            hash.update(source.as_bytes());
        }
        hash.update(public::producer_sha256());
        hash.update(crate::limited_kaito_session_v16::producer_sha256_v16());
        hash.update(crate::current_mixed_policy_v1::feature_identity());
        format!("{:x}", hash.finalize())
    }).clone()
}

pub(crate) fn preflight(config: &ResetV1) -> Result<(), String> {
    crate::limited_consumer_wire_v1::preflight_single_appendix(
        config, PRINTING, public::selected_id, public::NAME,
    )
}

fn native(config: &ResetV1) -> Result<FastActorSessionV1, String> {
    FastActorSessionV1::reset_limited_greatwurm_v1(config, false).map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_greatwurm_v1(config, true)
        .map_err(|error| format!("{error:?}"))?;
    KaitoSession::from_session(owner).map_err(|error| error.to_string())
}

pub fn reset_human_v1(config: &ResetV1, seat: PlayerSeatV1) -> Result<HumanKaitoSessionV75, String> {
    HumanKaitoSessionV75::from_session(native(config)?, seat).map_err(|error| error.to_string())
}

pub struct JsonlServerV1(ProfileWireV1);

impl Default for JsonlServerV1 {
    fn default() -> Self {
        Self(ProfileWireV1::new(PROFILE_V1, producer_sha256_v1, native))
    }
}

impl JsonlServerV1 {
    pub fn new() -> Self { Self::default() }

    pub fn handle_line(&mut self, line: &str) -> String { self.0.handle_line(line) }

    pub fn handle_request(&mut self, request: RequestV1) -> ReplyV1 { self.0.handle_request(request) }
}

fn is_greatwurm(source: &ReferenceV1, zone: Zone) -> bool {
    source.zone == zone && source.identity == (IdentityV1::Known { card_id: PRINTING })
}

pub(crate) fn validate_facts(facts: &FactsV1, actor: PlayerSeatV1) -> Result<(), String> {
    if facts.viewer != actor || facts.schema != public::SCHEMA || facts.meaning != public::printed_meaning()? {
        return Err("Greatwurm facts lost exact viewer or printing".into());
    }
    if facts.graveyard.iter().any(|row| !is_greatwurm(&row.source, Zone::Graveyard)
        || row.source.owner != actor)
        || facts.casts.iter().any(|row| !is_greatwurm(&row.source, Zone::Stack)
            || row.uncast_copy != row.origin_zone.is_none()
            || row.finalized_graveyard_route.is_some()
                && (row.announcing || row.origin_zone != Some(Zone::Graveyard)))
    {
        return Err("Greatwurm cast permission or source changed".into());
    }
    for paid in &facts.paid {
        let mut seen = BTreeSet::new();
        let mut total = 0_u32;
        for row in &paid.removals {
            let removed = row.original_count.checked_sub(row.remaining_count)
                .filter(|&n| n > 0).ok_or("Greatwurm actual counter counts did not decrease")?;
            let kind = facts.meaning.counter_kinds_in_native_order.iter().position(|&kind| kind == row.kind)
                .ok_or("Greatwurm counter kind absent")?;
            if row.original.zone != Zone::Battlefield
                || row.original.controller != paid.original_spell.controller
                || !seen.insert((row.original.object, kind))
            {
                return Err("Greatwurm paid roster changed original controlled creatures".into());
            }
            total = total.checked_add(removed).ok_or("Greatwurm actual debit width")?;
        }
        if paid.original_spell.zone != Zone::Stack
            || paid.original_spell.owner != paid.original_spell.controller
            || paid.origin_graveyard_generation.checked_add(1) != Some(paid.original_spell.generation)
            || total != 6
        {
            return Err("Greatwurm paid history changed original spell or cost".into());
        }
    }
    if facts.combat.iter().any(|row|
        !is_greatwurm(&row.observer.original_source, Zone::Battlefield)
            || row.original_creature.zone != Zone::Battlefield
            || row.observer.captured_controller != row.original_creature.controller
            || row.final_combat_damage == 0
            || row.current_original_creature.as_ref().is_some_and(|card| {
                card.object != row.original_creature.object || card.generation != row.original_creature.generation
                    || card.owner != row.original_creature.owner || card.zone != Zone::Battlefield
            }))
    {
        return Err("Greatwurm combat lost its original creature or final damage".into());
    }
    if let Some(choice) = &facts.chooser {
        let source = match choice {
            ChoiceV1::Route { source, routes } => {
                if *routes != [GraveyardRouteV1::GrantedPermission, GraveyardRouteV1::PrintedSixCounterCost] {
                    return Err("Greatwurm original native route order changed".into());
                }
                source
            }
            ChoiceV1::Unit { source, selected, legal, remaining } => {
                let mut seen = BTreeSet::new();
                let valid = |row: &public::UnitFactV1| {
                    row.permanent.zone == Zone::Battlefield && row.permanent.controller == actor
                        && row.before_payment.checked_sub(
                            selected.iter().filter(|candidate| candidate.permanent == row.permanent
                                && candidate.kind == row.kind).count() as u32,
                        ) == Some(row.available_after_selected)
                };
                if *remaining == 0 || selected.len() + usize::from(*remaining) != 6 || legal.is_empty()
                    || selected.iter().any(|row| !valid(row))
                    || legal.iter().any(|row| {
                        let kind = facts.meaning.counter_kinds_in_native_order.iter().position(|&kind| kind == row.kind);
                        !valid(row) || row.available_after_selected == 0 || !seen.insert((row.permanent.object, kind))
                    })
                {
                    return Err("Greatwurm chooser's original six-counter menu changed".into());
                }
                source
            }
        };
        if !is_greatwurm(source, Zone::Stack) || source.owner != actor || source.controller != actor
            || !facts.casts.iter().any(|row| row.announcing && row.source == *source
                && row.origin_zone == Some(Zone::Graveyard))
        {
            return Err("Greatwurm chooser lost its exact announcing graveyard source".into());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ActionV1 {
    ChooseGraveyardRoute { index: u16, route: GraveyardRouteV1 },
    SelectCounter { index: u16, kind: CounterKindV1, remaining: u8 },
}

fn binds(source: &ReferenceV1, stable: &CardStableRefV1) -> bool {
    is_greatwurm(source, Zone::Stack) && stable.card_db_id == PRINTING
        && source.object == stable.arena_id && source.generation == stable.zone_change_count
        && source.owner == stable.owner && source.controller == stable.controller && stable.zone == source.zone
}

pub(crate) fn action(facts: Option<&FactsV1>, action: &Action) -> Result<Option<ActionV1>, String> {
    let Some(facts) = facts else { return Ok(None) };
    let Some(choice) = &facts.chooser else { return Ok(None) };
    let Action::ChooseEffectOption { actor, source, option_index, option_count } = action else {
        return Err("Greatwurm native menu lost its original effect option".into());
    };
    if *actor != facts.viewer {
        return Err("Greatwurm native effect option lost original chooser".into());
    }
    let meaning = match choice {
        ChoiceV1::Route { source: exact, routes } if binds(exact, source) && *option_count == 2 => {
            let route = *routes.get(usize::from(*option_index)).ok_or("Greatwurm route index outside original menu")?;
            ActionV1::ChooseGraveyardRoute { index: *option_index, route }
        }
        ChoiceV1::Unit { source: exact, legal, remaining, .. }
            if binds(exact, source) && usize::from(*option_count) == legal.len() => {
            let unit = legal.get(usize::from(*option_index)).ok_or("Greatwurm unit index outside original menu")?;
            ActionV1::SelectCounter { index: *option_index, kind: unit.kind, remaining: *remaining }
        }
        _ => return Err("Greatwurm effect option changed source, native stage or width".into()),
    };
    Ok(Some(meaning))
}

pub(crate) fn validate_candidate_order<'a>(
    facts: &FactsV1,
    actions: impl ExactSizeIterator<Item = &'a Action>,
) -> Result<(), String> {
    let count = match &facts.chooser {
        Some(ChoiceV1::Route { routes, .. }) => routes.len(),
        Some(ChoiceV1::Unit { legal, .. }) => legal.len(),
        None => return Ok(()),
    };
    if actions.len() != count || !actions.enumerate().all(|(index, action)|
        matches!(action, Action::ChooseEffectOption { option_index, .. }
            if usize::from(*option_index) == index))
    {
        return Err("Greatwurm original effect option order changed".into());
    }
    Ok(())
}

#[cfg(all(test, feature = "limited-fdn-fixtures", not(feature = "standard-magezero-fixtures")))]
mod tests;
