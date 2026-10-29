//! Validated factual source components for the distinct FDN V76 consumer.
//! These records cannot create a native owner or admit an ordinary observation.
use crate::{
    card_def::{self, CardCapability, CARD_DEFS},
    drakuseth_damage_source_v76::{self as native, JournalV76},
    engine,
    event::CommittedEvent as E,
    ids::{PlayerId, StackItemId},
    optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    state::{
        AbilitySourceContractV4, GameState, StackItemKind, StackTargetContractV4, Target, Zone,
    },
    tinybones_public_v72::CountersV72,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA: &str = "mtg-kernel/fdn-family-source/v76";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum LaterFamilyV76 {
    Drakuseth(DrakusethSourceV76),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV76 {
    pub schema: String,
    pub producer_sha256: String,
    pub viewer: PlayerSeatV1,
    pub families: Vec<LaterFamilyV76>,
    pub complete_ordinary_observation: bool,
    pub owning_consumer_admitted: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentV76 {
    pub card: public::ReferenceV1,
    pub is_token: bool,
    pub tapped: bool,
    pub summoning_sick: bool,
    pub marked_damage: u32,
    pub counters: CountersV72,
    pub form: public::FormV1,
    pub currently_attacking: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum TargetIdentityV76 {
    Player {
        player: PlayerSeatV1,
    },
    Object {
        object: u32,
        generation: u32,
        owner: PlayerSeatV1,
        controller_when_selected: PlayerSeatV1,
        original_zone: Zone,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalSlotV76 {
    pub slot: u32,
    pub printed_damage: i32,
    pub original_target: TargetIdentityV76,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentSlotV76 {
    pub original: OriginalSlotV76,
    pub current_original_target: Option<public::ReferenceV1>,
    pub currently_legal: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV76 {
    Held,
    ChoosingTargets,
    Published,
    Resolving,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageSourceV76 {
    pub uses_original_departure_values: bool,
    pub controller_of_current_or_departing_source: PlayerSeatV1,
    pub color_mask: u8,
    pub lifelink: bool,
    pub deathtouch: bool,
    pub werewolf: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerV76 {
    pub stage: StageV76,
    /// Position in the actual held/stack vector, never a filtered ordinal.
    pub native_position: Option<u32>,
    /// Dense public publication occurrence, not a native execution identifier.
    pub publication: Option<u32>,
    pub source: public::RootV1,
    pub current_or_original_departure_characteristics: DamageSourceV76,
    pub original_target_slots: Vec<CurrentSlotV76>,
    pub mandatory_target_selected: bool,
    pub placement_complete: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemovalV76 {
    Resolved,
    Countered,
    NoLegalTarget,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum PublicationStateV76 {
    Published,
    NativeRowRemoved { outcome: RemovalV76 },
    CrystalResolutionRecorded,
    DisposedByTimeStop,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationV76 {
    pub occurrence: u32,
    pub source: public::RootV1,
    pub original_target_slots: Vec<OriginalSlotV76>,
    /// A native row-removal result is not an inferred per-target damage receipt.
    pub state: PublicationStateV76,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrakusethSourceV76 {
    pub card_id: u16,
    pub catalog_capability: CardCapability,
    pub authorized_printings: Vec<public::ReferenceV1>,
    pub battlefield: [Vec<CurrentV76>; 2],
    pub casts: Vec<crate::drakuseth_public_v67::CastV67>,
    pub triggers: Vec<TriggerV76>,
    pub publications: Vec<PublicationV76>,
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut h = Sha256::new();
        for part in [
            SCHEMA,
            include_str!("fdn_family_source_v76.rs"),
            include_str!("drakuseth_damage_source_v76.rs"),
            include_str!("rules_meaning_drakuseth_v67.rs"),
            include_str!("tinybones_public_v72.rs"),
            include_str!("optional_trigger_public_v1.rs"),
            include_str!("engine.rs"),
            include_str!("event.rs"),
            include_str!("state.rs"),
            include_str!("effect/crystal_prevention_v1.rs"),
        ] {
            h.update(part.as_bytes());
            h.update([0]);
        }
        h.update(card_def::KERNEL_CARDDB_HASH.to_le_bytes());
        format!("{:x}", h.finalize())
    })
    .clone()
}
fn ord(n: usize) -> Result<u32, String> {
    u32::try_from(n).map_err(|_| "V76 source ordinal overflow".into())
}
fn root(s: &GameState, c: AbilitySourceContractV4, v: PlayerId) -> Result<public::RootV1, String> {
    engine::validate_historical_ability_source_contract(s, c)?;
    let mut r = public::root(s, c, Some(v))?;
    // The printed attack was public even if its current form is neutral.
    r.original_source.identity = public::IdentityV1::Known {
        card_id: c.card_def,
    };
    Ok(r)
}
fn slots(
    s: &GameState,
    targets: &[Target],
    contracts: &[StackTargetContractV4],
) -> Result<Vec<OriginalSlotV76>, String> {
    if targets.len() != contracts.len() || targets.len() > 3 {
        return Err("V76 positional target cardinality changed".into());
    }
    targets
        .iter()
        .zip(contracts)
        .enumerate()
        .map(|(i, (&t, &c))| {
            if targets[..i].contains(&t)
                || !crate::state::stack_target_contract_is_structurally_valid(
                    s,
                    crate::rules_meaning_drakuseth_v67::target_spec(),
                    i,
                    t,
                    c,
                )
            {
                return Err("V76 distinct target contract changed".into());
            }
            let original_target = match c {
                StackTargetContractV4::Player(p) => TargetIdentityV76::Player { player: p.into() },
                StackTargetContractV4::Object {
                    object,
                    owner,
                    controller,
                    zone,
                    zone_change_count,
                    ..
                } => TargetIdentityV76::Object {
                    object: object.0,
                    generation: zone_change_count,
                    owner: owner.into(),
                    controller_when_selected: controller.into(),
                    original_zone: zone,
                },
                StackTargetContractV4::StackItem { .. } => {
                    return Err("V76 Drakuseth cannot target a stack row".into())
                }
            };
            Ok(OriginalSlotV76 {
                slot: ord(i)?,
                printed_damage: if i == 0 { 4 } else { 3 },
                original_target,
            })
        })
        .collect()
}
fn trigger(
    s: &GameState,
    v: PlayerId,
    c: AbilitySourceContractV4,
    location: (StageV76, Option<usize>, Option<u32>),
    targets: &[Target],
    contracts: &[StackTargetContractV4],
) -> Result<TriggerV76, String> {
    let (stage, position, publication) = location;
    if c.zone != Zone::Battlefield
        || !native::owns_definition(c.card_def)
        || stage == StageV76::Held && !targets.is_empty()
        || matches!(stage, StageV76::Published | StageV76::Resolving) && targets.is_empty()
    {
        return Err("V76 source target stage changed".into());
    }
    if !s.engine.event_history.iter().any(|e| matches!(e,
        E::DeclaredAttacker{source,source_zone_change_count,controller}
        if *source==c.source&&*source_zone_change_count==c.zone_change_count&&*controller==c.controller)) {
        return Err("V76 source has no matching actual declaration".into());
    }
    let facts = native::source_facts(s, c)?;
    let legal = native::legal_targets(
        s,
        c.source,
        c.card_def,
        c.zone_change_count,
        c.controller,
        &[],
    )?;
    let originals = slots(s, targets, contracts)?;
    let mut projected = Vec::new();
    for ((original, &t), &contract) in originals.into_iter().zip(targets).zip(contracts) {
        let current_original_target = match contract {
            StackTargetContractV4::Object {
                object,
                zone_change_count,
                ..
            } => s
                .objects
                .try_get(object)
                .filter(|o| o.zone_change_count == zone_change_count)
                .and_then(|_| public::reference(s, object, Some(v))),
            _ => None,
        };
        projected.push(CurrentSlotV76 {
            original,
            current_original_target,
            currently_legal: engine::target_contract_matches_live(s, t, contract)
                && legal.contains(&t),
        });
    }
    Ok(TriggerV76 {
        stage,
        native_position: position.map(ord).transpose()?,
        publication,
        source: root(s, c, v)?,
        current_or_original_departure_characteristics: DamageSourceV76 {
            uses_original_departure_values: !s.objects.try_get(c.source).is_some_and(|o| {
                o.zone == Zone::Battlefield
                    && o.zone_change_count == c.zone_change_count
                    && o.owner == c.owner
            }),
            controller_of_current_or_departing_source: facts.source.controller.into(),
            color_mask: facts.color_mask,
            lifelink: facts.lifelink,
            deathtouch: facts.deathtouch,
            werewolf: facts.werewolf,
        },
        mandatory_target_selected: !targets.is_empty(),
        placement_complete: matches!(stage, StageV76::Published | StageV76::Resolving),
        original_target_slots: projected,
    })
}
pub fn capture(s: &GameState, v: PlayerId) -> Result<FactsV76, String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) || !matches!(v, PlayerId::P0 | PlayerId::P1)
        || s.engine.halted.is_some()
    {
        return Err("V76 unsupported source world or viewer".into());
    }
    let id = card_def::card_id_by_name(crate::rules_meaning_drakuseth_v67::NAME)
        .ok_or("V76 Drakuseth absent")?;
    crate::rules_meaning_drakuseth_v67::exact_native_definition_v76(
        id,
        &CARD_DEFS[usize::from(id)],
    )?;
    public::validate_state_checked(s)?;
    native::validate_state(s)?;
    crate::effect::validate_pending_effect_choice(s)?;
    if let Some(p) = &s.engine.pending_cast {
        engine::validate_pending_cast(s, p)?;
    }
    if let Some(p) = &s.engine.pending_activation {
        engine::validate_pending_activation(s, p)?;
    }
    let mut out = DrakusethSourceV76 {
        card_id: id,
        catalog_capability: CARD_DEFS[usize::from(id)].capability,
        authorized_printings: Vec::new(),
        battlefield: [vec![], vec![]],
        casts: vec![],
        triggers: vec![],
        publications: vec![],
    };
    for (object, o) in s.objects.iter() {
        if o.v4.is_token && o.zone != Zone::Battlefield
            || o.spell_copy_origin.is_some()
                && o.zone == Zone::Stack
                && !s.stack.iter().any(|r| r.source == object)
        {
            continue;
        }
        if let Some(r) = public::reference(s, object, Some(v))
            .filter(|r| matches!(r.identity,public::IdentityV1::Known{card_id}if card_id==id))
        {
            out.authorized_printings.push(r);
        }
    }
    let mut seen = BTreeSet::new();
    for p in [PlayerId::P0, PlayerId::P1] {
        for &object in &s.players[p.index()].battlefield {
            let o = s
                .objects
                .try_get(object)
                .ok_or("V76 battlefield member absent")?;
            if o.zone != Zone::Battlefield || o.controller != p || !seen.insert(object) {
                return Err("V76 battlefield roster changed".into());
            }
            out.battlefield[p.index()].push(CurrentV76 {
                card: public::reference(s, object, Some(v))
                    .ok_or("V76 public battlefield member absent")?,
                is_token: o.v4.is_token,
                tapped: o.tapped,
                summoning_sick: o.summoning_sick,
                marked_damage: o.damage,
                counters: crate::tinybones_public_v72::counters(s, object)?,
                form: public::checked_form(s, object)?,
                currently_attacking: s.engine.combat.attackers.contains(&object),
            });
        }
    }
    if s.objects
        .iter()
        .any(|(object, o)| o.zone == Zone::Battlefield && !seen.contains(&object))
    {
        return Err("V76 battlefield roster incomplete".into());
    }
    let mut publications = BTreeMap::new();
    for (at, e) in s.engine.event_history.iter().enumerate() {
        if let E::DrakusethDamageSourceV76 { journal } = e {
            if let JournalV76::Published { item } = journal.as_ref() {
                let occurrence = ord(out.publications.len())?;
                if publications.insert(ord(at)?, occurrence).is_some() {
                    return Err("V76 duplicate publication occurrence".into());
                }
                let source = item
                    .v4
                    .ability_source_contract
                    .ok_or("V76 publication source absent")?;
                let disposed = crate::engine::time_stop_v1::disposed(s, item)?;
                out.publications.push(PublicationV76 {
                    occurrence,
                    source: root(s, source, v)?,
                    original_target_slots: slots(s, &item.targets, &item.v4.target_contracts)?,
                    state: if disposed {
                        PublicationStateV76::DisposedByTimeStop
                    } else {
                        PublicationStateV76::Published
                    },
                });
            }
        }
    }
    for e in s.engine.event_history.iter() {
        if let E::DrakusethDamageSourceV76 { journal } = e {
            let (at, state) = match journal.as_ref() {
                JournalV76::Removed {
                    publication,
                    outcome,
                    ..
                } => (
                    *publication,
                    PublicationStateV76::NativeRowRemoved {
                        outcome: match outcome {
                            native::OutcomeV76::Resolved => RemovalV76::Resolved,
                            native::OutcomeV76::Countered => RemovalV76::Countered,
                            native::OutcomeV76::NoLegalTarget => RemovalV76::NoLegalTarget,
                        },
                    },
                ),
                JournalV76::CrystalResolved { publication, .. } => {
                    (*publication, PublicationStateV76::CrystalResolutionRecorded)
                }
                _ => continue,
            };
            let occurrence = *publications
                .get(&at)
                .ok_or("V76 closure publication absent")?;
            out.publications[occurrence as usize].state = state;
        }
    }
    for (position, p) in public::held(s).into_iter().enumerate() {
        if let Some(c) = p.source_contract.filter(|c| {
            c.card_def == id
                && p.granted_by.is_none()
                && p.effect == crate::standard_cards_v1::drakuseth_attack_v1()
        }) {
            if p.source != c.source
                || p.controller != c.controller
                || p.target_spec != crate::rules_meaning_drakuseth_v67::target_spec()
            {
                return Err("V76 held printed trigger changed".into());
            }
            if p.placement_ordered {
                engine::validate_pending_trigger(s, p)?;
            }
            out.triggers.push(trigger(
                s,
                v,
                c,
                (
                    if p.placement_ordered {
                        StageV76::ChoosingTargets
                    } else {
                        StageV76::Held
                    },
                    Some(position),
                    None,
                ),
                &p.targets,
                &p.target_contracts,
            )?);
        }
    }
    let pending = s
        .engine
        .pending_effect
        .as_ref()
        .filter(|p| native::selected(&p.resolving_item));
    let mut tracked = BTreeSet::<StackItemId>::new();
    for (position, item) in s.stack.iter().enumerate() {
        if item.kind == StackItemKind::Spell {
            if let Some(source) = public::reference(s, item.source, Some(v))
                .filter(|r| matches!(r.identity,public::IdentityV1::Known{card_id}if card_id==id))
            {
                out.casts.push(crate::drakuseth_public_v67::CastV67 {
                    native_stack_position: ord(position)?,
                    source,
                    original_controller: item.controller.into(),
                    payment_pending: s
                        .engine
                        .pending_cast
                        .as_ref()
                        .is_some_and(|p| p.spell == item.source),
                    is_uncast_copy: item.is_copy,
                    genuine_cast_finalized: !item.is_copy
                        && item
                            .v4
                            .source_contract
                            .and_then(|c| c.spell_cast_origin)
                            .is_some_and(|o| o.finalized_method.is_some()),
                });
            }
        } else if native::selected(item) {
            tracked.insert(item.v4.stack_item_id);
            let publication = item
                .v4
                .drakuseth_published_v76
                .0
                .and_then(|at| publications.get(&at).copied())
                .ok_or("V76 active source publication absent")?;
            out.triggers.push(trigger(
                s,
                v,
                item.v4
                    .ability_source_contract
                    .ok_or("V76 active source absent")?,
                (
                    if pending
                        .is_some_and(|p| p.resolving_item.v4.stack_item_id == item.v4.stack_item_id)
                    {
                        StageV76::Resolving
                    } else {
                        StageV76::Published
                    },
                    Some(position),
                    Some(publication),
                ),
                &item.targets,
                &item.v4.target_contracts,
            )?);
        }
    }
    if let Some(p) = pending {
        if !tracked.contains(&p.resolving_item.v4.stack_item_id) {
            let item = &p.resolving_item;
            let publication = item
                .v4
                .drakuseth_published_v76
                .0
                .and_then(|at| publications.get(&at).copied())
                .ok_or("V76 resolving publication absent")?;
            out.triggers.push(trigger(
                s,
                v,
                item.v4
                    .ability_source_contract
                    .ok_or("V76 resolving source absent")?,
                (StageV76::Resolving, None, Some(publication)),
                &item.targets,
                &item.v4.target_contracts,
            )?);
        }
    }
    Ok(FactsV76 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        viewer: v.into(),
        families: vec![LaterFamilyV76::Drakuseth(out)],
        complete_ordinary_observation: false,
        owning_consumer_admitted: false,
    })
}
pub fn validate(s: &GameState, v: PlayerId, facts: &FactsV76) -> Result<(), String> {
    if *facts != capture(s, v)? {
        return Err("V76 source facts differ from exact native recapture".into());
    }
    Ok(())
}
