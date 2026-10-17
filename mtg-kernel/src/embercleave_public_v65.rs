//! Viewer-public Embercleave appendix. No complete observation or execution authority.
use crate::{
    card_def::{self, CardType, DynamicCountDef, TargetSpec},
    effect::{EffectOp, ObjectRef},
    engine,
    ids::{ObjectId, PlayerId},
    niv_mizzet_public_v63::CountersV63 as CountersV65,
    optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    rules_meaning_embercleave_v65 as meaning,
    state::{
        AbilitySourceContractV4, Counters, GameState, StackItemKind, StackTargetContractV4, Target,
        Zone,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const SCHEMA: &str = "mtg-kernel/embercleave-public/v65";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentV65 {
    pub card: public::ReferenceV1,
    pub is_token: bool,
    pub tapped: bool,
    pub summoning_sick: bool,
    pub marked_damage: u32,
    pub counters: CountersV65,
    pub form: public::FormV1,
    pub attached_to: Option<public::ReferenceV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReductionV65 {
    pub controller: PlayerSeatV1,
    pub current_attackers: Vec<public::ReferenceV1>,
    pub count: u32,
    /// Current quote only, never a reconstruction of historical mana paid.
    pub generic_due_now: u8,
    pub retained_red_pips: u8,
    pub printed_mana_value: u16,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV65 {
    pub position: u32,
    pub source: public::ReferenceV1,
    pub original_controller: PlayerSeatV1,
    pub is_uncast_copy: bool,
    pub payment_pending: bool,
    pub genuine_cast_finalized: bool,
}
/// Original public relation without a guessed historical face identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetV65 {
    pub object: u32,
    pub generation: u32,
    pub owner: PlayerSeatV1,
    pub controller_when_selected: PlayerSeatV1,
    pub original_zone: Zone,
    pub current_original_target: Option<public::ReferenceV1>,
    pub currently_legal: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AbilityKindV65 {
    EntryAttach,
    Equip,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV65 {
    HeldEntry,
    ChoosingEntryTarget,
    PayingEquip,
    Published,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityV65 {
    pub kind: AbilityKindV65,
    pub stage: StageV65,
    /// Position in the entire native stage vector, never a filtered ordinal.
    pub position: u32,
    pub source: public::RootV1,
    pub original_source_incarnation_present: bool,
    pub original_targets: Vec<TargetV65>,
    /// Current pool under the captured source/controller. Published targets
    /// stay fixed; this field grants no retarget permission.
    pub current_candidate_pool: Vec<public::ReferenceV1>,
    pub selection_complete: bool,
    pub payment_complete: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV65 {
    pub schema: String,
    pub producer_sha256: String,
    pub viewer: PlayerSeatV1,
    pub meaning: meaning::MeaningV65,
    pub authorized: Vec<public::ReferenceV1>,
    pub battlefield: [Vec<CurrentV65>; 2],
    pub current_cast_reduction: [ReductionV65; 2],
    pub casts: Vec<CastV65>,
    pub abilities: Vec<AbilityV65>,
    pub complete_ordinary_observation: bool,
    pub owning_consumer_admitted: bool,
}
pub fn producer_sha256() -> String {
    meaning::producer_sha256()
}
fn ordinal(n: usize) -> Result<u32, String> {
    u32::try_from(n).map_err(|_| "V65 public ordinal overflow".into())
}
fn counter_values(state: &GameState, id: ObjectId) -> Result<CountersV65, String> {
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
    } = o.counters;
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
        return Err("V65 negative native counter".into());
    }
    Ok(CountersV65 {
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
fn reduction(state: &GameState, controller: PlayerId) -> Result<ReductionV65, String> {
    let selected = meaning::selected_id()?;
    let d = &card_def::CARD_DEFS[usize::from(selected)];
    let mut current_attackers = vec![];
    let mut seen = BTreeSet::new();
    for &id in &state.engine.combat.attackers {
        if !seen.insert(id) {
            return Err("V65 duplicate native attacker".into());
        }
        if state.objects.try_get(id).is_some_and(|o| {
            o.zone == Zone::Battlefield
                && o.controller == controller
                && engine::object_has_type(state, id, CardType::Creature)
        }) {
            current_attackers.push(
                public::reference(state, id, Some(controller))
                    .ok_or("V65 public current attacker absent")?,
            );
        }
    }
    let count = engine::normal_cast_reduction_count(
        DynamicCountDef::ControllerAttackingCreatures,
        controller,
        &[],
        state,
    );
    if ordinal(current_attackers.len())? != count {
        return Err("V65 native attacker count differs".into());
    }
    let cost = engine::printed_normal_cast_cost_with_targets(d, controller, &[], state);
    if cost.pips != d.cost.pips || cost.x_count != 0 {
        return Err("V65 native red/X quote changed".into());
    }
    Ok(ReductionV65 {
        controller: controller.into(),
        current_attackers,
        count,
        generic_due_now: cost.generic,
        retained_red_pips: 2,
        printed_mana_value: d.mana_value,
    })
}
fn source_current(state: &GameState, source: AbilitySourceContractV4) -> bool {
    state.objects.try_get(source.source).is_some_and(|o| {
        o.zone == Zone::Battlefield
            && o.zone_change_count == source.zone_change_count
            && o.card_def == source.card_def
            && o.owner == source.owner
    })
}
// These are independent fields of the existing published ability contract.
#[allow(clippy::too_many_arguments)]
fn ability(
    state: &GameState,
    viewer: PlayerId,
    source: AbilitySourceContractV4,
    kind: AbilityKindV65,
    stage: StageV65,
    position: usize,
    targets: &[Target],
    contracts: &[StackTargetContractV4],
    payment_complete: bool,
) -> Result<AbilityV65, String> {
    if source.card_def != meaning::selected_id()?
        || source.zone != Zone::Battlefield
        || targets.len() != contracts.len()
        || targets.len() > 1
    {
        return Err("V65 attachment source or target shape changed".into());
    }
    engine::validate_historical_ability_source_contract(state, source)?;
    let mut original_targets = vec![];
    for (index, (&target, &contract)) in targets.iter().zip(contracts).enumerate() {
        if !crate::state::stack_target_contract_is_structurally_valid(
            state,
            TargetSpec::ControlledCreature,
            index,
            target,
            contract,
        ) {
            return Err("V65 original target contract changed".into());
        }
        let StackTargetContractV4::Object {
            object,
            owner,
            controller,
            zone,
            zone_change_count,
            ..
        } = contract
        else {
            return Err("V65 nonpermanent attachment target".into());
        };
        original_targets.push(TargetV65 {
            object: object.0,
            generation: zone_change_count,
            owner: owner.into(),
            controller_when_selected: controller.into(),
            original_zone: zone,
            current_original_target: state
                .objects
                .try_get(object)
                .filter(|o| o.zone_change_count == zone_change_count)
                .and_then(|_| public::reference(state, object, Some(viewer))),
            currently_legal: engine::target_contract_matches_live(state, target, contract)
                && engine::effect_target_is_legal_from_ability_source(
                    state,
                    source,
                    source.controller,
                    TargetSpec::ControlledCreature,
                    targets,
                    index,
                ),
        });
    }
    let mut current_candidate_pool = vec![];
    for player in [PlayerId::P0, PlayerId::P1] {
        for &id in &state.players[player.index()].battlefield {
            if engine::effect_target_is_legal_from_ability_source(
                state,
                source,
                source.controller,
                TargetSpec::ControlledCreature,
                &[Target::Object(id)],
                0,
            ) {
                current_candidate_pool.push(
                    public::reference(state, id, Some(viewer))
                        .ok_or("V65 public target candidate absent")?,
                );
            }
        }
    }
    Ok(AbilityV65 {
        kind,
        stage,
        position: ordinal(position)?,
        source: public::root(state, source, Some(viewer))?,
        original_source_incarnation_present: source_current(state, source),
        original_targets,
        current_candidate_pool,
        selection_complete: targets.len() == 1,
        payment_complete,
    })
}
pub fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV65, String> {
    let selected = meaning::selected_id()?;
    if state.engine.halted.is_some() {
        return Err("V65 native state halted".into());
    }
    public::validate_state_checked(state)?;
    state
        .validate_attachment_relations()
        .map_err(|_| "V65 invalid native attachment relation")?;
    if let Some(p) = &state.engine.pending_cast {
        engine::validate_pending_cast(state, p)?;
    }
    if let Some(p) = &state.engine.pending_activation {
        engine::validate_pending_activation(state, p)?;
    }
    crate::doubling_season_v48::validate_token_journals(state)?;
    let mut authorized = vec![];
    for (id, o) in state.objects.iter() {
        if (o.v4.is_token && o.zone != Zone::Battlefield)
            || (o.spell_copy_origin.is_some()
                && o.zone == Zone::Stack
                && !state.stack.iter().any(|row| row.source == id))
        {
            continue;
        }
        if let Some(card) = public::reference(state, id, Some(viewer)) {
            if matches!(card.identity, public::IdentityV1::Known { card_id } if card_id == selected)
            {
                authorized.push(card);
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
                .ok_or("V65 battlefield member absent")?;
            if o.zone != Zone::Battlefield || o.controller != player || !seen.insert(id) {
                return Err("V65 battlefield roster changed".into());
            }
            let attached_to =
                o.v4.attached_to
                    .map(|link| {
                        let host = state
                            .objects
                            .try_get(link.object)
                            .ok_or("V65 host absent")?;
                        if host.zone != Zone::Battlefield
                            || host.zone_change_count != link.zone_change_count
                        {
                            return Err("V65 host incarnation changed");
                        }
                        public::reference(state, link.object, Some(viewer))
                            .ok_or("V65 public host absent")
                    })
                    .transpose()?;
            battlefield[player.index()].push(CurrentV65 {
                card: public::reference(state, id, Some(viewer))
                    .ok_or("V65 public battlefield member absent")?,
                is_token: o.v4.is_token,
                tapped: o.tapped,
                summoning_sick: o.summoning_sick,
                marked_damage: o.damage,
                counters: counter_values(state, id)?,
                form: public::checked_form(state, id)?,
                attached_to,
            });
        }
    }
    if state
        .objects
        .iter()
        .any(|(id, o)| o.zone == Zone::Battlefield && !seen.contains(&id))
    {
        return Err("V65 battlefield roster incomplete".into());
    }
    let current_cast_reduction = [
        reduction(state, PlayerId::P0)?,
        reduction(state, PlayerId::P1)?,
    ];
    let mut abilities = vec![];
    for (position, p) in public::held(state).into_iter().enumerate() {
        if let Some(source) = p.source_contract.filter(|s| s.card_def == selected) {
            if p.effect
                != (EffectOp::AttachSourceToTarget {
                    object: ObjectRef::Target(0),
                })
                || p.source != source.source
                || p.controller != source.controller
                || p.target_spec != TargetSpec::ControlledCreature
                || p.granted_by.is_some()
            {
                return Err("V65 distinct unsupported held source ability".into());
            }
            if p.placement_ordered {
                engine::validate_pending_trigger(state, p)?;
            }
            abilities.push(ability(
                state,
                viewer,
                source,
                AbilityKindV65::EntryAttach,
                if p.placement_ordered {
                    StageV65::ChoosingEntryTarget
                } else {
                    StageV65::HeldEntry
                },
                position,
                &p.targets,
                &p.target_contracts,
                true,
            )?);
        }
    }
    if let Some(p) = &state.engine.pending_activation {
        if state.objects.get(p.source).card_def == selected {
            if p.ability_index != 0 {
                return Err("V65 distinct unsupported activation".into());
            }
            abilities.push(ability(
                state,
                viewer,
                AbilitySourceContractV4::capture(state, p.source),
                AbilityKindV65::Equip,
                StageV65::PayingEquip,
                0,
                &p.targets_chosen,
                &p.target_contracts,
                false,
            )?);
        }
    }
    let mut casts = vec![];
    for (position, item) in state.stack.iter().enumerate() {
        if item.kind == StackItemKind::Spell {
            if let Some(source) = public::reference(state, item.source, Some(viewer)).filter(|r|
                matches!(r.identity, public::IdentityV1::Known { card_id } if card_id == selected)) {
                casts.push(CastV65 { position: ordinal(position)?, source, original_controller: item.controller.into(),
                    is_uncast_copy: item.is_copy,
                    payment_pending: state.engine.pending_cast.as_ref().is_some_and(|p| p.spell == item.source),
                    genuine_cast_finalized: !item.is_copy && item.v4.source_contract.and_then(|s| s.spell_cast_origin)
                        .is_some_and(|origin| origin.finalized_method.is_some()) });
            }
        } else if let Some(source) = item
            .v4
            .ability_source_contract
            .filter(|s| s.card_def == selected)
        {
            if item.source != source.source
                || item.controller != source.controller
                || item.inline_effect
                    != Some(EffectOp::AttachSourceToTarget {
                        object: ObjectRef::Target(0),
                    })
                || item.v4.granted_by.is_some()
            {
                return Err("V65 distinct unsupported published source ability".into());
            }
            let kind = match item.kind {
                StackItemKind::TriggeredAbility => AbilityKindV65::EntryAttach,
                StackItemKind::ActivatedAbility if item.v4.activated_ability_index == Some(0) => {
                    AbilityKindV65::Equip
                }
                _ => return Err("V65 attachment ability kind changed".into()),
            };
            abilities.push(ability(
                state,
                viewer,
                source,
                kind,
                StageV65::Published,
                position,
                &item.targets,
                &item.v4.target_contracts,
                true,
            )?);
        }
    }
    if state.engine.pending_effect.as_ref().is_some_and(|p| {
        p.ctx
            .ability_source_contract
            .is_some_and(|source| source.card_def == selected)
    }) {
        return Err("V65 atomic attachment cannot be externally resumed".into());
    }
    Ok(FactsV65 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        viewer: viewer.into(),
        meaning: meaning::printed_meaning()?,
        authorized,
        battlefield,
        current_cast_reduction,
        casts,
        abilities,
        complete_ordinary_observation: false,
        owning_consumer_admitted: false,
    })
}
pub fn validate(state: &GameState, viewer: PlayerId, value: &FactsV65) -> Result<(), String> {
    if *value == capture(state, viewer)? {
        Ok(())
    } else {
        Err("V65 facts differ from actual viewer-public state".into())
    }
}
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
