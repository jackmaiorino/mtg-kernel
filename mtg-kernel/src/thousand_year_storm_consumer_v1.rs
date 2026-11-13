//! Declared-deck owner for Thousand-Year Storm cast counts and independent copy targets.
pub use crate::limited_consumer_wire_v1::{CommandV1, ReplyBodyV1, ReplyV1, RequestV1, ResetV1};
use crate::{
    current_mixed_policy_v1::KaitoSession,
    human_bo3_v1::HumanKaitoSessionV75,
    limited_consumer_wire_v1::ProfileWireV1,
    optional_trigger_public_v1::{IdentityV1, ReferenceV1},
    rl::{ActionSemanticV1 as Action, CardStableRefV1, PlayerSeatV1},
    rl_session::FastActorSessionV1,
    state::Zone,
    thousand_year_storm_public_v1::{self as public, FactsV1, PRINTING},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-thousand-year-storm-consumer/v1";
pub const OBSERVATION_SCHEMA: &str = "mtg-kernel/thousand-year-storm-observation/v1";
pub const CANDIDATES_SCHEMA: &str = "mtg-kernel/thousand-year-storm-candidates/v1";
pub const ACTION_SCHEMA: &str = "mtg-kernel/thousand-year-storm-candidate-meaning/v1";

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROFILE_V1,
            include_str!("thousand_year_storm_consumer_v1.rs"),
            include_str!("human_bo3_v1/thousand_year_storm_consumer_v1.rs"),
            include_str!("limited_consumer_wire_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"),
            include_str!("reveal_meaning_v3.rs"),
            include_str!("reveal_meaning_v3/reducer.rs"),
            include_str!("bin/kernel_limited_thousand_year_storm_env_v1.rs"),
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
    FastActorSessionV1::reset_limited_thousand_year_storm_v1(config, false)
        .map_err(|error| format!("{error:?}"))
}

pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

pub fn reset_numeric_reveal_facts_v3(config: &ResetV1) -> Result<KaitoSession, String> {
    let owner = FastActorSessionV1::reset_limited_thousand_year_storm_v1(config, true)
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

fn snapshot(value: &public::SnapshotV1) -> Result<(), String> {
    let IdentityV1::Known { card_id } = value.original_spell.identity else {
        return Err("Storm original spell is not public".into());
    };
    let def = crate::card_def::CARD_DEFS
        .get(card_id as usize)
        .ok_or("Storm spell printing absent")?;
    let instant_sorcery = if value.cast_method == crate::state::CastMethodV4::Omen {
        def.omen_spell_form().is_some_and(|(_, types)| {
            types.iter().any(|t| {
                matches!(
                    t,
                    crate::card_def::CardType::Instant | crate::card_def::CardType::Sorcery
                )
            })
        })
    } else {
        def.types.iter().any(|t| {
            matches!(
                t,
                crate::card_def::CardType::Instant | crate::card_def::CardType::Sorcery
            )
        })
    };
    if value.original_spell.zone != Zone::Stack || !instant_sorcery {
        return Err("Storm original spell kind changed".into());
    }
    for target in &value.targets {
        validate_target(target)?;
    }
    Ok(())
}
fn validate_target(value: &public::TargetV1) -> Result<(), String> {
    if let public::TargetV1::Object {
        original,
        current_original: Some(current),
    } = value
    {
        if current.object != original.object
            || current.generation != original.generation
            || current.zone != original.zone
            || current.owner != original.owner
        {
            return Err("Storm target follows a later incarnation".into());
        }
    }
    Ok(())
}
pub(crate) fn validate_facts(facts: &FactsV1, actor: PlayerSeatV1) -> Result<(), String> {
    if facts.schema != public::SCHEMA
        || facts.viewer != actor
        || facts.meaning != public::printed_meaning()?
    {
        return Err("invalid Storm actor facts".into());
    }
    let printing = IdentityV1::Known { card_id: PRINTING };
    let mut source_ids = std::collections::BTreeSet::new();
    for source in &facts.sources {
        if source.source.identity != printing
            || source.source.zone != Zone::Battlefield
            || !source_ids.insert(source.source.object)
        {
            return Err("Storm current source roster changed".into());
        }
    }
    let mut counts = [0u16; 2];
    let mut current_started = false;
    for cast in &facts.casts {
        snapshot(&cast.spell)?;
        if cast.spell.is_copy {
            return Err("Storm copy was counted as a cast".into());
        }
        if current_started && !cast.current_turn {
            return Err("Storm turn cast order changed".into());
        }
        if cast.current_turn {
            current_started = true;
            let slot = match cast.spell.original_spell.controller {
                PlayerSeatV1::P0 => 0,
                PlayerSeatV1::P1 => 1,
            };
            if cast.prior_own_casts != counts[slot] {
                return Err("Storm current turn count changed".into());
            }
            counts[slot] = counts[slot]
                .checked_add(1)
                .ok_or("Storm cast count overflow")?;
        }
        let mut seen = std::collections::BTreeSet::new();
        for source in &cast.observers {
            if source.original_source.identity != printing
                || source.original_source.zone != Zone::Battlefield
                || source.original_source.controller != cast.spell.original_spell.controller
                || source.captured_controller != source.original_source.controller
                || !seen.insert((
                    source.original_source.object,
                    source.original_source.generation,
                ))
                || source.current_original_source.as_ref().is_some_and(|c| {
                    c.object != source.original_source.object
                        || c.generation != source.original_source.generation
                })
            {
                return Err("Storm original observer changed".into());
            }
        }
    }
    if counts != facts.current_turn_casts {
        return Err("Storm per-seat cast count changed".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut choices = 0;
    for trigger in &facts.triggers {
        let cast = facts
            .casts
            .get(trigger.cast as usize)
            .ok_or("Storm trigger cast absent")?;
        if !cast.observers.contains(&trigger.source)
            || trigger.expected_copies != cast.prior_own_casts
            || trigger.created_copies.len() > usize::from(trigger.expected_copies)
            || !seen.insert((
                trigger.cast,
                trigger.source.original_source.object,
                trigger.source.original_source.generation,
            ))
            || matches!(
                trigger.stage,
                Some(public::StageV1::Held | public::StageV1::Ordered | public::StageV1::Published)
            ) != trigger.position.is_some()
            || matches!(
                trigger.stage,
                Some(public::StageV1::Held | public::StageV1::Ordered)
            ) && !trigger.created_copies.is_empty()
        {
            return Err("Storm trigger source, count or stage changed".into());
        }
        for (i, copy) in trigger.created_copies.iter().enumerate() {
            snapshot(&copy.spell)?;
            let a = &cast.spell;
            let b = &copy.spell;
            if usize::from(copy.ordinal) != i + 1
                || !b.is_copy
                || a.original_spell.identity != b.original_spell.identity
                || b.original_spell.controller != trigger.source.captured_controller
                || b.mode != a.mode
                || b.x_value != a.x_value
                || b.kicked != a.kicked
                || b.cast_method != crate::source_type_hexproof_v1::copy_method(a.cast_method)
                || b.optional_additional_cost_paid.is_some()
                || b.authorized_paid_costs != a.authorized_paid_costs
                || b.mana_spent != a.mana_spent
                || b.targets.len() != a.targets.len()
            {
                return Err("Storm copy changed original casting choices".into());
            }
        }
        if let Some(choice) = &trigger.choice {
            choices += 1;
            if choices > 1
                || trigger.stage != Some(public::StageV1::Resolving)
                || choice.chooser != actor
                || choice.chooser != trigger.source.captured_controller
                || usize::from(choice.copy_ordinal) != trigger.created_copies.len() + 1
                || choice.copy_ordinal > trigger.expected_copies
                || usize::from(choice.target_slot) != choice.selected_prefix.len()
                || cast.spell.targets.get(choice.target_slot as usize)
                    != Some(&choice.original_target)
                || choice.legal_new_targets.is_empty() && !choice.can_keep_original
            {
                return Err("Storm private copy target slot changed".into());
            }
            for target in &choice.selected_prefix {
                validate_target(target)?;
            }
        }
    }
    Ok(())
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ActionV1 {
    ChooseNewTarget { copy: u16, slot: u16 },
    KeepOriginalTarget { copy: u16, slot: u16 },
}
fn binds(card: &ReferenceV1, stable: &CardStableRefV1) -> bool {
    card.identity
        == (IdentityV1::Known {
            card_id: stable.card_db_id,
        })
        && card.object == stable.arena_id
        && card.generation == stable.zone_change_count
        && card.owner == stable.owner
        && card.controller == stable.controller
        && card.zone == stable.zone
}
pub(crate) fn action(facts: Option<&FactsV1>, action: &Action) -> Result<Option<ActionV1>, String> {
    let Some(facts) = facts else { return Ok(None) };
    let (actor, source, selected_count) = match action {
        Action::ChooseEffectTarget {
            actor,
            source,
            selected_count,
            ..
        }
        | Action::FinishEffectSelection {
            actor,
            source,
            selected_count,
        } => (actor, source, selected_count),
        _ => return Ok(None),
    };
    let row = facts.triggers.iter().find(|r| r.choice.is_some());
    let Some(row) = row else {
        if source.card_db_id == PRINTING {
            return Err("Storm target action lost original trigger".into());
        }
        return Ok(None);
    };
    let choice = row.choice.as_ref().unwrap();
    if *actor != facts.viewer
        || *actor != choice.chooser
        || *selected_count != 0
        || !binds(&row.source.original_source, source)
    {
        return Err("Storm action changed original source or chooser".into());
    }
    match action {
        Action::ChooseEffectTarget {
            target,
            min_targets,
            max_targets: 1,
            ..
        } if *min_targets == u16::from(!choice.can_keep_original)
            && choice.legal_new_targets.contains(target) =>
        {
            Ok(Some(ActionV1::ChooseNewTarget {
                copy: choice.copy_ordinal,
                slot: choice.target_slot,
            }))
        }
        Action::FinishEffectSelection { .. } if choice.can_keep_original => {
            Ok(Some(ActionV1::KeepOriginalTarget {
                copy: choice.copy_ordinal,
                slot: choice.target_slot,
            }))
        }
        _ => Err("Storm action changed its independent native target slot".into()),
    }
}
pub(crate) fn validate_candidate_order<'a>(
    facts: &FactsV1,
    actions: impl Iterator<Item = &'a Action>,
) -> Result<(), String> {
    let Some(row) = facts.triggers.iter().find(|r| r.choice.is_some()) else {
        return Ok(());
    };
    let choice = row.choice.as_ref().unwrap();
    let mut actual = vec![];
    let mut finished = false;
    for candidate in actions {
        match (action(Some(facts), candidate)?, candidate) {
            (Some(ActionV1::ChooseNewTarget { .. }), Action::ChooseEffectTarget { target, .. })
                if !finished =>
            {
                actual.push(target.clone())
            }
            (Some(ActionV1::KeepOriginalTarget { .. }), _) if !finished => finished = true,
            _ => return Err("Storm candidate list contains a foreign choice".into()),
        }
    }
    if actual != choice.legal_new_targets || finished != choice.can_keep_original {
        return Err("Storm original native target order changed".into());
    }
    Ok(())
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
