//! Source-only Niv appendix. No ordinary observation, executable owner or admission.
//! Raw native worlds retain their existing structural trust boundary; coordinated
//! edits to a world and every independent receipt are not authenticated snapshots.
use crate::{
    card_def::{self, CardType, Keywords},
    effect::{EffectOp, ExecCtx},
    event::CommittedEvent as E,
    ids::{ObjectId, PlayerId},
    optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    rules_meaning_niv_mizzet_v63 as meaning,
    state::{AbilitySourceContractV4, Counters, GameState, StackItemKind, Zone},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const SCHEMA: &str = "mtg-kernel/niv-mizzet-public/v63";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CountersV63 {
    pub plus_one: i32,
    pub minus_one: i16,
    pub minus_toughness: i16,
    pub stun: i16,
    pub lore: i16,
    pub oil: i16,
    pub charge: i32,
    pub net: i32,
    pub bait: i32,
    pub soul: i32,
    pub incubation: i32,
    pub revival: i32,
    pub fellowship: i32,
    pub lifelink: i16,
    pub time: u8,
    pub finality: u16,
    pub loyalty: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BattlefieldV63 {
    pub card: public::ReferenceV1,
    pub counters: CountersV63,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentV63 {
    pub card: public::ReferenceV1,
    pub is_token: bool,
    pub tapped: bool,
    pub summoning_sick: bool,
    pub marked_damage: u32,
    pub form: public::FormV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV63 {
    pub position: u32,
    pub source: public::ReferenceV1,
    pub original_controller: PlayerSeatV1,
    pub is_uncast_copy: bool,
    pub payment_pending: bool,
    pub genuine_cast_finalized: bool,
}
/// Damage made this object/incarnation/controller public, not necessarily its
/// hidden printed identity. The native receipt has no historical face/reveal
/// witness, so this reference deliberately contains no inferred card ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ControllerBasisV63 {
    AtDamageEvent,
    ExactDepartureLki,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageSourceV63 {
    pub object: u32,
    pub generation: u32,
    pub owner: PlayerSeatV1,
    pub damage_controller: PlayerSeatV1,
    pub original_zone: Zone,
    pub controller_basis: ControllerBasisV63,
    pub current_original_source: Option<public::ReferenceV1>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV63 {
    Uncollected,
    Held,
    Published,
    NoCurrentExecutableRow,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageDrawV63 {
    pub occurrence: u32,
    pub observer: public::RootV1,
    pub damage_source: DamageSourceV63,
    pub recipient: PlayerSeatV1,
    pub committed_damage: i32,
    pub draw_count: i32,
    pub stage: StageV63,
    pub native_position: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV63 {
    pub schema: String,
    pub producer_sha256: String,
    pub viewer: PlayerSeatV1,
    pub meaning: meaning::MeaningV63,
    pub authorized: Vec<public::ReferenceV1>,
    pub current: Vec<CurrentV63>,
    pub battlefield_counters: [Vec<BattlefieldV63>; 2],
    pub casts: Vec<CastV63>,
    pub damage_draws: Vec<DamageDrawV63>,
    pub no_maximum_hand_size: [bool; 2],
    pub complete_ordinary_observation: bool,
    pub owning_consumer_admitted: bool,
}
pub fn producer_sha256() -> String {
    meaning::producer_sha256()
}
fn ordinal(n: usize) -> Result<u32, String> {
    u32::try_from(n).map_err(|_| "V63 public ordinal overflow".into())
}
fn counter_values(state: &GameState, id: ObjectId) -> Result<CountersV63, String> {
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
        return Err("V63 negative native counter".into());
    }
    Ok(CountersV63 {
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
fn form(state: &GameState, id: ObjectId) -> Result<public::FormV1, String> {
    let o = state.objects.get(id);
    let base_power = crate::engine::effective_base_power(state, id);
    let base_toughness = crate::engine::effective_base_toughness(state, id);
    let pt = if base_power.is_some() || base_toughness.is_some() {
        Some(crate::engine::checked_effective_pt(state, id)?)
    } else {
        None
    };
    let active = crate::continuous_characteristics_v1::printed_abilities_active(state, id);
    let removed = crate::continuous_characteristics_v1::removal_timestamp(state, id).is_some();
    Ok(public::FormV1 {
        names: crate::engine::effective_names(state, id)
            .into_iter()
            .map(str::to_owned)
            .collect(),
        mana_value: crate::engine::object_mana_value(state, id),
        types: [
            CardType::Land,
            CardType::Creature,
            CardType::Instant,
            CardType::Sorcery,
            CardType::Artifact,
            CardType::Enchantment,
            CardType::Planeswalker,
        ]
        .into_iter()
        .filter(|kind| crate::engine::object_has_type(state, id, *kind))
        .collect(),
        supertypes: crate::engine::effective_supertypes(state, id)
            .iter()
            .map(|s| match s {
                card_def::Supertype::Basic => public::SupertypeV1::Basic,
                card_def::Supertype::Snow => public::SupertypeV1::Snow,
                card_def::Supertype::Legendary => public::SupertypeV1::Legendary,
            })
            .collect(),
        subtype_ids: crate::engine::effective_subtype_ids(state, id),
        color_mask: crate::engine::object_color_mask(state, id),
        base_power,
        base_toughness,
        power: pt.map(|p| p.0),
        toughness: pt.map(|p| p.1),
        keyword_bits: (0..u32::BITS).fold(0, |bits, bit| {
            let k = Keywords(1u32 << bit);
            if crate::engine::has_effective_keyword(state, id, k) {
                bits | k.0
            } else {
                bits
            }
        }),
        ward_generic: if o.v4.face_down_v1.is_some_and(|f| f.disguised) && !removed {
            2
        } else if active {
            o.v4.ward_generic
        } else {
            0
        },
        minimum_blockers: if crate::engine::object_has_type(state, id, CardType::Creature) {
            crate::engine::minimum_blockers_required(state, id)
        } else {
            0
        },
        printed_abilities_active: active,
        abilities_removed: removed,
    })
}
fn original_damage_source(
    state: &GameState,
    s: AbilitySourceContractV4,
    viewer: PlayerId,
    damage_index: u64,
) -> Result<DamageSourceV63, String> {
    if !matches!(s.zone, Zone::Battlefield | Zone::Stack) {
        return Err("V63 damage source was not public battlefield/stack incarnation".into());
    }
    let departed_before_damage = state.engine.event_history.iter().any(|event| {
        matches!(event, E::NivMizzetVisionaryDamageSourceDepartureV1 {
            source, departure_history_index,
        } if *source == s && *departure_history_index < damage_index)
    });
    Ok(DamageSourceV63 {
        object: s.source.0,
        generation: s.zone_change_count,
        owner: s.owner.into(),
        damage_controller: s.controller.into(),
        original_zone: s.zone,
        controller_basis: if departed_before_damage {
            ControllerBasisV63::ExactDepartureLki
        } else {
            ControllerBasisV63::AtDamageEvent
        },
        current_original_source: state
            .objects
            .try_get(s.source)
            .filter(|object| object.zone_change_count == s.zone_change_count)
            .and_then(|_| public::reference(state, s.source, Some(viewer))),
    })
}
pub fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV63, String> {
    let selected = meaning::selected_id()?;
    public::validate_state_checked(state)?;
    state
        .validate_attachment_relations()
        .map_err(|_| "V63 invalid native attachment relation")?;
    if let Some(p) = &state.engine.pending_cast {
        crate::engine::validate_pending_cast(state, p)?;
    }
    if let Some(p) = &state.engine.pending_activation {
        crate::engine::validate_pending_activation(state, p)?;
    }
    crate::doubling_season_v48::validate_token_journals(state)?;
    let mut authorized = vec![];
    let mut current = vec![];
    let mut battlefield_counters: [Vec<BattlefieldV63>; 2] = [vec![], vec![]];
    let mut seen = BTreeSet::new();
    for seat in [PlayerId::P0, PlayerId::P1] {
        for &id in &state.players[seat.index()].battlefield {
            let o = state
                .objects
                .try_get(id)
                .ok_or("V63 battlefield member absent")?;
            if o.zone != Zone::Battlefield || o.controller != seat || !seen.insert(id) {
                return Err("V63 battlefield membership changed".into());
            }
            battlefield_counters[seat.index()].push(BattlefieldV63 {
                card: public::reference(state, id, Some(viewer))
                    .ok_or("V63 public battlefield reference absent")?,
                counters: counter_values(state, id)?,
            });
        }
    }
    if state
        .objects
        .iter()
        .any(|(id, o)| o.zone == Zone::Battlefield && !seen.contains(&id))
    {
        return Err("V63 battlefield roster incomplete".into());
    }
    for (id, o) in state.objects.iter() {
        // Authorize visibility before inspecting the selected printing. Tombstones
        // and facedown identities are never recovered by scanning underlying IDs.
        let Some(reference) = public::reference(state, id, Some(viewer)) else {
            continue;
        };
        if !matches!(reference.identity,public::IdentityV1::Known{card_id}if card_id==selected)
            || o.v4.face_index != 0
            || (o.v4.is_token && o.zone != Zone::Battlefield)
            || (o.spell_copy_origin.is_some()
                && o.zone == Zone::Stack
                && !state.stack.iter().any(|r| r.source == id))
        {
            continue;
        }
        authorized.push(reference.clone());
        if o.zone == Zone::Battlefield {
            current.push(CurrentV63 {
                card: reference,
                is_token: o.v4.is_token,
                tapped: o.tapped,
                summoning_sick: o.summoning_sick,
                marked_damage: o.damage,
                form: form(state, id)?,
            });
        }
    }
    let mut casts = vec![];
    for (position, row) in state.stack.iter().enumerate() {
        if row.kind == StackItemKind::Spell && state.objects.get(row.source).card_def == selected {
            let source = public::reference(state, row.source, Some(viewer))
                .ok_or("V63 stack reference absent")?;
            if !matches!(source.identity,public::IdentityV1::Known{card_id}if card_id==selected) {
                continue;
            }
            casts.push(CastV63 {
                position: ordinal(position)?,
                source,
                original_controller: row.controller.into(),
                is_uncast_copy: row.is_copy,
                payment_pending: state
                    .engine
                    .pending_cast
                    .as_ref()
                    .is_some_and(|p| p.spell == row.source),
                genuine_cast_finalized: !row.is_copy
                    && row
                        .v4
                        .source_contract
                        .and_then(|s| s.spell_cast_origin)
                        .is_some_and(|o| o.finalized_method.is_some()),
            });
        }
    }
    let mut damage_draws = vec![];
    let held = public::held(state);
    let mut publications = BTreeMap::new();
    for (at, event) in state.engine.event_history.iter().enumerate() {
        if let E::NivMizzetVisionaryTriggerStackedV1 {
            stack_item_id,
            observer,
            damage_history_index,
        } = event
        {
            if stack_item_id.0 == 0
                || observer.card_def != selected
                || publications
                    .insert(*stack_item_id, (at, *observer, *damage_history_index))
                    .is_some()
            {
                return Err("V63 duplicate or malformed draw publication".into());
            }
        }
    }
    let mut used_publications = BTreeSet::new();
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let E::NivMizzetVisionaryDamageV1 {
            observer,
            damage_source,
            damage_history_index,
        } = event
        else {
            continue;
        };
        if observer.card_def != selected {
            return Err("V63 foreign observer in Niv receipt".into());
        }
        let effect = EffectOp::NivMizzetVisionaryDrawV1 {
            damage_history_index: *damage_history_index,
        };
        crate::event::validate_niv_mizzet_visionary_pending_v1(
            state,
            observer.source,
            observer.controller,
            &effect,
            Some(*observer),
        )?;
        let index =
            usize::try_from(*damage_history_index).map_err(|_| "V63 damage index overflow")?;
        let Some(E::Damage {
            target: crate::state::Target::Player(recipient),
            amount,
            ..
        }) = state.engine.event_history.get(index)
        else {
            return Err("V63 missing public player damage".into());
        };
        let logged = state
            .engine
            .event_log
            .iter()
            .filter(|e| *e == event)
            .count();
        let waiting = held
            .iter()
            .enumerate()
            .filter(|(_, p)| p.source_contract == Some(*observer) && p.effect == effect)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let published = publications
            .iter()
            .filter(|(_, (_, o, i))| *o == *observer && *i == *damage_history_index)
            .collect::<Vec<_>>();
        if logged > 1 || waiting.len() > 1 || published.len() > 1 {
            return Err("V63 duplicate native draw owners".into());
        }
        let (stage, position) = if let Some((&id, (publication, _, _))) = published.first().copied()
        {
            if *publication <= at || logged != 0 || !waiting.is_empty() {
                return Err("V63 publication reordered or overlaps held occurrence".into());
            }
            let mut ctx = ExecCtx::no_targets(observer.source, observer.controller);
            ctx.stack_item_id = Some(id);
            ctx.ability_source_contract = Some(*observer);
            if crate::event::resolve_niv_mizzet_visionary_draw_count_v1(
                state,
                &ctx,
                *damage_history_index,
            )? != *amount
            {
                return Err("V63 draw amount changed".into());
            }
            used_publications.insert(id);
            let rows = state
                .stack
                .iter()
                .enumerate()
                .filter(|(_, r)| r.v4.stack_item_id == id)
                .collect::<Vec<_>>();
            if rows.len() > 1 {
                return Err("V63 duplicate published row".into());
            }
            if let Some((position, row)) = rows.first() {
                if row.kind != StackItemKind::TriggeredAbility
                    || row.source != observer.source
                    || row.controller != observer.controller
                    || row.inline_effect.as_ref() != Some(&effect)
                    || row.v4.ability_source_contract != Some(*observer)
                {
                    return Err("V63 published native draw row changed".into());
                }
                (StageV63::Published, Some(ordinal(*position)?))
            } else {
                (StageV63::NoCurrentExecutableRow, None)
            }
        } else if logged == 1 && waiting.is_empty() {
            (StageV63::Uncollected, None)
        } else if logged == 0 && waiting.len() == 1 {
            (StageV63::Held, Some(ordinal(waiting[0])?))
        } else {
            return Err("V63 draw lost its sole native collection/publication owner".into());
        };
        damage_draws.push(DamageDrawV63 {
            occurrence: ordinal(damage_draws.len())?,
            observer: public::root(state, *observer, Some(viewer))?,
            damage_source: original_damage_source(
                state,
                *damage_source,
                viewer,
                *damage_history_index,
            )?,
            recipient: (*recipient).into(),
            committed_damage: *amount,
            draw_count: *amount,
            stage,
            native_position: position,
        });
    }
    if used_publications.len() != publications.len() {
        return Err("V63 orphaned draw publication".into());
    }
    if state.engine.pending_effect.as_ref().is_some_and(|p| {
        matches!(
            p.resolving_item.inline_effect,
            Some(EffectOp::NivMizzetVisionaryDrawV1 { .. })
        )
    }) {
        return Err("V63 atomic Niv draw has no externally resumable stage".into());
    }
    Ok(FactsV63 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        viewer: viewer.into(),
        meaning: meaning::printed_meaning()?,
        authorized,
        current,
        battlefield_counters,
        casts,
        damage_draws,
        no_maximum_hand_size: [
            crate::event::niv_mizzet_visionary_removes_hand_limit_v1(state, PlayerId::P0),
            crate::event::niv_mizzet_visionary_removes_hand_limit_v1(state, PlayerId::P1),
        ],
        complete_ordinary_observation: false,
        owning_consumer_admitted: false,
    })
}
pub fn validate(state: &GameState, viewer: PlayerId, value: &FactsV63) -> Result<(), String> {
    if *value == capture(state, viewer)? {
        Ok(())
    } else {
        Err("V63 facts differ from actual viewer-public state".into())
    }
}
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "niv_mizzet_public_v63/tests.rs"]
mod tests;
