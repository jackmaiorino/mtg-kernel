//! Read-only Drakuseth facts. Native history is not an execution capability.
use crate::{
    engine,
    event::CommittedEvent,
    ids::{ObjectId, PlayerId},
    niv_mizzet_public_v63::CountersV63 as CountersV67,
    optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    rules_meaning_drakuseth_v67 as meaning, standard_cards_v1,
    state::{
        AbilitySourceContractV4, Counters, GameState, StackItemKind, StackTargetContractV4, Target,
        Zone,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const SCHEMA: &str = "mtg-kernel/drakuseth-public/v67";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentV67 {
    pub card: public::ReferenceV1,
    pub is_token: bool,
    pub tapped: bool,
    pub summoning_sick: bool,
    pub marked_damage: u32,
    pub counters: CountersV67,
    pub form: public::FormV1,
    pub currently_attacking: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV67 {
    pub native_stack_position: u32,
    pub source: public::ReferenceV1,
    pub original_controller: PlayerSeatV1,
    pub payment_pending: bool,
    pub is_uncast_copy: bool,
    pub genuine_cast_finalized: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV67 {
    Held,
    ChoosingTargets,
    Published,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum TargetV67 {
    Player {
        slot: u32,
        player: PlayerSeatV1,
        damage: i32,
        currently_legal: bool,
    },
    Object {
        slot: u32,
        object: u32,
        generation: u32,
        owner: PlayerSeatV1,
        controller_when_selected: PlayerSeatV1,
        original_zone: Zone,
        damage: i32,
        current_original_target: Option<public::ReferenceV1>,
        currently_legal: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerV67 {
    pub stage: StageV67,
    /// Position in the complete native held or stack vector, not a filtered ordinal.
    pub native_position: u32,
    pub source: public::RootV1,
    /// Descriptive history count only. Native rows contain no unique attack occurrence join.
    pub matching_native_declarations: u32,
    pub original_target_slots: Vec<TargetV67>,
    pub mandatory_target_selected: bool,
    pub placement_complete: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV67 {
    pub schema: String,
    pub producer_sha256: String,
    pub viewer: PlayerSeatV1,
    pub meaning: meaning::MeaningV67,
    pub authorized_printings: Vec<public::ReferenceV1>,
    pub battlefield: [Vec<CurrentV67>; 2],
    pub casts: Vec<CastV67>,
    pub triggers: Vec<TriggerV67>,
    pub complete_ordinary_observation: bool,
    pub owning_consumer_admitted: bool,
}
pub fn producer_sha256() -> String {
    meaning::producer_sha256()
}
fn ordinal(n: usize) -> Result<u32, String> {
    u32::try_from(n).map_err(|_| "V67 public ordinal overflow".into())
}
fn counter_values(state: &GameState, id: ObjectId) -> Result<CountersV67, String> {
    let o = state.objects.get(id);
    let Counters {
        plus1_plus1,
        minus1_minus1,
        minus0_minus1,
        stun,
        lore,
        oil,
        charge,
        net,
        bait,
        soul,
        incubation,
        revival,
        fellowship,
        stash,
    } = o.counters;
    if stash != 0 {
        return Err("public projection does not expose stash counters".into());
    }
    let lifelink = o.v4.lifelink_keyword_counters;
    if [
        plus1_plus1,
        i32::from(minus1_minus1),
        i32::from(minus0_minus1),
        i32::from(stun),
        i32::from(lore),
        i32::from(oil),
        charge,
        net,
        bait,
        soul,
        incubation,
        revival,
        fellowship,
        i32::from(lifelink),
    ]
    .iter()
    .any(|n| *n < 0)
    {
        return Err("V67 negative native counter".into());
    }
    Ok(CountersV67 {
        plus_one: plus1_plus1,
        minus_one: minus1_minus1,
        minus_toughness: minus0_minus1,
        stun,
        lore,
        oil,
        charge,
        net,
        bait,
        soul,
        incubation,
        revival,
        fellowship,
        lifelink,
        time: o.v4.time_counters_v1,
        finality: o.v4.creature_upgrade.as_ref().map_or(0, |u| u.finality),
        loyalty: crate::planeswalker_v1::loyalty(state, id).unwrap_or(0),
    })
}

fn trigger(
    state: &GameState,
    viewer: PlayerId,
    source: AbilitySourceContractV4,
    stage: StageV67,
    position: usize,
    targets: &[Target],
    contracts: &[StackTargetContractV4],
) -> Result<TriggerV67, String> {
    if source.card_def != meaning::selected_id()?
        || source.zone != Zone::Battlefield
        || targets.len() != contracts.len()
        || targets.len() > 3
        || stage == StageV67::Published && targets.is_empty()
        || stage == StageV67::Held && !targets.is_empty()
    {
        return Err("V67 source/target stage changed".into());
    }
    engine::validate_historical_ability_source_contract(state, source)?;
    let declarations=state.engine.event_history.iter().filter(|e|matches!(e,CommittedEvent::DeclaredAttacker {source:id,source_zone_change_count,controller}
        if *id==source.source&&*source_zone_change_count==source.zone_change_count&&*controller==source.controller)).count();
    if declarations == 0 {
        return Err("V67 source has no matching actual attack declaration".into());
    }
    let mut seen = Vec::new();
    let mut original_target_slots = vec![];
    for (index, (&target, &contract)) in targets.iter().zip(contracts).enumerate() {
        if seen.contains(&target)
            || !crate::state::stack_target_contract_is_structurally_valid(
                state,
                meaning::target_spec(),
                index,
                target,
                contract,
            )
        {
            return Err("V67 distinct original target contract changed".into());
        }
        seen.push(target);
        let legal = engine::target_contract_matches_live(state, target, contract)
            && engine::effect_target_is_legal_from_ability_source(
                state,
                source,
                source.controller,
                meaning::target_spec(),
                targets,
                index,
            );
        let slot = ordinal(index)?;
        let damage = if index == 0 { 4 } else { 3 };
        original_target_slots.push(match contract {
            StackTargetContractV4::Player(player) => TargetV67::Player {
                slot,
                player: player.into(),
                damage,
                currently_legal: legal,
            },
            StackTargetContractV4::Object {
                object,
                owner,
                controller,
                zone,
                zone_change_count,
                ..
            } => TargetV67::Object {
                slot,
                object: object.0,
                generation: zone_change_count,
                owner: owner.into(),
                controller_when_selected: controller.into(),
                original_zone: zone,
                damage,
                current_original_target: state
                    .objects
                    .try_get(object)
                    .filter(|o| o.zone_change_count == zone_change_count)
                    .and_then(|_| public::reference(state, object, Some(viewer))),
                currently_legal: legal,
            },
            StackTargetContractV4::StackItem { .. } => {
                return Err("V67 any-target cannot select a stack row".into())
            }
        });
    }
    let mut root = public::root(state, source, Some(viewer))?;
    // The captured printed attack was public. A later neutral current form does not rewrite it.
    root.original_source.identity = public::IdentityV1::Known {
        card_id: source.card_def,
    };
    Ok(TriggerV67 {
        stage,
        native_position: ordinal(position)?,
        source: root,
        matching_native_declarations: ordinal(declarations)?,
        mandatory_target_selected: !targets.is_empty(),
        placement_complete: stage == StageV67::Published,
        original_target_slots,
    })
}
pub fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV67, String> {
    let selected = meaning::selected_id()?;
    if !matches!(viewer, PlayerId::P0 | PlayerId::P1) || state.engine.halted.is_some() {
        return Err("V67 unsupported viewer or halted native world".into());
    }
    public::validate_state_checked(state)?;
    if let Some(p) = &state.engine.pending_cast {
        engine::validate_pending_cast(state, p)?;
    }
    if let Some(p) = &state.engine.pending_activation {
        engine::validate_pending_activation(state, p)?;
    }
    crate::doubling_season_v48::validate_token_journals(state)?;
    let mut authorized_printings = vec![];
    for (id, o) in state.objects.iter() {
        if o.v4.is_token && o.zone != Zone::Battlefield
            || o.spell_copy_origin.is_some()
                && o.zone == Zone::Stack
                && !state.stack.iter().any(|r| r.source == id)
        {
            continue;
        }
        if let Some(card) = public::reference(state, id, Some(viewer)) {
            if matches!(card.identity,public::IdentityV1::Known{card_id}if card_id==selected) {
                authorized_printings.push(card);
            }
        }
    }
    let mut battlefield = [vec![], vec![]];
    let mut seen = BTreeSet::new();
    for player in [PlayerId::P0, PlayerId::P1] {
        for &id in &state.players[player.index()].battlefield {
            let o = state
                .objects
                .try_get(id)
                .ok_or("V67 battlefield member absent")?;
            if o.zone != Zone::Battlefield || o.controller != player || !seen.insert(id) {
                return Err("V67 battlefield roster changed".into());
            }
            battlefield[player.index()].push(CurrentV67 {
                card: public::reference(state, id, Some(viewer))
                    .ok_or("V67 public member absent")?,
                is_token: o.v4.is_token,
                tapped: o.tapped,
                summoning_sick: o.summoning_sick,
                marked_damage: o.damage,
                counters: counter_values(state, id)?,
                form: public::checked_form(state, id)?,
                currently_attacking: state.engine.combat.attackers.contains(&id),
            });
        }
    }
    if state
        .objects
        .iter()
        .any(|(id, o)| o.zone == Zone::Battlefield && !seen.contains(&id))
    {
        return Err("V67 battlefield roster incomplete".into());
    }
    let mut triggers = vec![];
    for (position, p) in public::held(state).into_iter().enumerate() {
        if let Some(source) = p.source_contract.filter(|s| s.card_def == selected) {
            if p.source != source.source
                || p.controller != source.controller
                || p.effect != standard_cards_v1::drakuseth_attack_v1()
                || p.target_spec != meaning::target_spec()
                || p.granted_by.is_some()
            {
                return Err("V67 distinct unsupported held source ability".into());
            }
            if p.placement_ordered {
                engine::validate_pending_trigger(state, p)?;
            }
            triggers.push(trigger(
                state,
                viewer,
                source,
                if p.placement_ordered {
                    StageV67::ChoosingTargets
                } else {
                    StageV67::Held
                },
                position,
                &p.targets,
                &p.target_contracts,
            )?);
        }
    }
    let mut casts = vec![];
    for (position, row) in state.stack.iter().enumerate() {
        if row.kind == StackItemKind::Spell {
            if let Some(source) = public::reference(state, row.source, Some(viewer)).filter(
                |r| matches!(r.identity,public::IdentityV1::Known{card_id}if card_id==selected),
            ) {
                casts.push(CastV67 {
                    native_stack_position: ordinal(position)?,
                    source,
                    original_controller: row.controller.into(),
                    payment_pending: state
                        .engine
                        .pending_cast
                        .as_ref()
                        .is_some_and(|p| p.spell == row.source),
                    is_uncast_copy: row.is_copy,
                    genuine_cast_finalized: !row.is_copy
                        && row
                            .v4
                            .source_contract
                            .and_then(|s| s.spell_cast_origin)
                            .is_some_and(|o| o.finalized_method.is_some()),
                });
            }
        } else if let Some(source) = row
            .v4
            .ability_source_contract
            .filter(|s| s.card_def == selected)
        {
            if row.kind != StackItemKind::TriggeredAbility
                || row.source != source.source
                || row.controller != source.controller
                || row.inline_effect != Some(standard_cards_v1::drakuseth_attack_v1())
                || row.v4.granted_by.is_some()
            {
                return Err("V67 distinct unsupported published source ability".into());
            }
            triggers.push(trigger(
                state,
                viewer,
                source,
                StageV67::Published,
                position,
                &row.targets,
                &row.v4.target_contracts,
            )?);
        }
    }
    // The native printed program executes atomically. No synthetic resolving/completed row.
    if state.engine.pending_effect.as_ref().is_some_and(|p| {
        p.ctx
            .ability_source_contract
            .is_some_and(|s| s.card_def == selected)
    }) {
        return Err("V67 atomic printed program has no supported external continuation".into());
    }
    Ok(FactsV67 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        viewer: viewer.into(),
        meaning: meaning::printed_meaning()?,
        authorized_printings,
        battlefield,
        casts,
        triggers,
        complete_ordinary_observation: false,
        owning_consumer_admitted: false,
    })
}
pub fn validate(state: &GameState, viewer: PlayerId, value: &FactsV67) -> Result<(), String> {
    if *value == capture(state, viewer)? {
        Ok(())
    } else {
        Err("V67 complete viewer facts changed".into())
    }
}
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
