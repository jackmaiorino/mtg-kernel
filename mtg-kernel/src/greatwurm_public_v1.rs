//! Exact Quilled Greatwurm combat, casting and six-counter menu facts.
pub use crate::effect::greatwurm_v1::CounterKindV1;
use crate::{
    card_def::{self, CardType, Subtype},
    effect::{
        greatwurm_v1::{self as native, UnitV1},
        EffectOp,
    },
    engine::{self, greatwurm_cast_v1, Decision},
    event::CommittedEvent as E,
    event_history_v1::EventKindV1,
    greatwurm_receipts_v1::{self as receipts, EndV1, JournalV1},
    ids::{ObjectId, PlayerId, StackItemId},
    optional_trigger_public_v1::{self as public, IdentityV1, ReferenceV1, RootV1},
    policy_surface_v5::PolicyDecisionV5,
    rl::PlayerSeatV1,
    state::{AbilitySourceContractV4, GameState, SpellCastRouteV4, StackItem, StackItemKind, Zone},
    surface_v2::SurfaceDecision,
    tinybones_public_v72::IncarnationV72,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const PRINTING: u16 = 476;
pub const NAME: &str = "Quilled Greatwurm";
pub const SCHEMA: &str = "mtg-kernel/greatwurm-public/v1";
const MEANING_SCHEMA: &str = "rules-meaning/quilled-greatwurm/v1";

pub(crate) fn selected_id() -> Result<u16, String> {
    let triggers = crate::trigger::triggers_for(PRINTING);
    if card_def::card_id_by_name(NAME) != Some(PRINTING)
        || !native::definition(PRINTING)
        || triggers.len() != 1
        || triggers[0].condition
            != crate::trigger::TriggerCondition::ControlledCreatureCombatDamageDuringOwnTurnV1
        || triggers[0].home_zone != Zone::Battlefield
        || triggers[0].face_index != 0
        || triggers[0].intervening_if_kicked
        || triggers[0].intervening_if_controls_another_source_card
        || (triggers[0].effect)() != EffectOp::BindGreatwurmCountersV1
    {
        return Err("Greatwurm requires its exact pure Limited printing and combat trigger".into());
    }
    Ok(PRINTING)
}

pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            SCHEMA,
            MEANING_SCHEMA,
            include_str!("greatwurm_public_v1.rs"),
            include_str!("greatwurm_receipts_v1.rs"),
            include_str!("effect/greatwurm_v1.rs"),
            include_str!("engine/greatwurm_cast_v1.rs"),
            include_str!("event.rs"),
            include_str!("engine.rs"),
            include_str!("spell_costs_v1.rs"),
            include_str!("state.rs"),
            include_str!("../build.rs"),
        ] {
            hash.update((source.len() as u64).to_le_bytes());
            hash.update(source.as_bytes());
        }
        format!("{:x}", hash.finalize())
    })
    .clone()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub card_id: u16,
    pub card_type: CardType,
    pub subtype: Subtype,
    pub printed_generic: u8,
    pub printed_green: u8,
    pub mana_value: u16,
    pub power: i16,
    pub toughness: i16,
    pub trample: bool,
    pub additional_counters_from_controlled_creatures: u8,
    pub graveyard_route_uses_printed_mana_cost: bool,
    pub own_turn_controlled_creature_final_combat_damage: bool,
    pub counters_on_same_damage_dealing_creature: bool,
    pub counter_kinds_in_native_order: [CounterKindV1; 18],
}

pub fn printed_meaning() -> Result<MeaningV1, String> {
    Ok(MeaningV1 {
        schema: MEANING_SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id: selected_id()?,
        card_type: CardType::Creature,
        subtype: Subtype::Wurm,
        printed_generic: 4,
        printed_green: 2,
        mana_value: 6,
        power: 7,
        toughness: 7,
        trample: true,
        additional_counters_from_controlled_creatures: 6,
        graveyard_route_uses_printed_mana_cost: true,
        own_turn_controlled_creature_final_combat_damage: true,
        counters_on_same_damage_dealing_creature: true,
        counter_kinds_in_native_order: native::KINDS,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum GraveyardRouteV1 {
    GrantedPermission,
    PrintedSixCounterCost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AvailableV1 {
    pub source: ReferenceV1,
    pub printed_counter_cost_available: bool,
    pub castable_now: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitFactV1 {
    pub permanent: ReferenceV1,
    pub kind: CounterKindV1,
    pub before_payment: u32,
    pub available_after_selected: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ChoiceV1 {
    Route {
        source: ReferenceV1,
        routes: [GraveyardRouteV1; 2],
    },
    Unit {
        source: ReferenceV1,
        selected: Vec<UnitFactV1>,
        legal: Vec<UnitFactV1>,
        remaining: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebitV1 {
    pub original: IncarnationV72,
    pub identity: IdentityV1,
    pub kind: CounterKindV1,
    pub original_count: u32,
    pub remaining_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentV1 {
    pub original_spell: IncarnationV72,
    pub origin_graveyard_generation: u32,
    pub removals: Vec<DebitV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV1 {
    pub source: ReferenceV1,
    pub stack_position: u32,
    pub origin_zone: Option<Zone>,
    pub announcing: bool,
    pub uncast_copy: bool,
    pub finalized_graveyard_route: Option<GraveyardRouteV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CombatStageV1 {
    Captured,
    Held {
        position: u32,
    },
    Published {
        stack_position: u32,
    },
    Countered,
    OriginalCreatureGone,
    Resolved {
        recipient_controller: PlayerSeatV1,
        placed: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatV1 {
    pub observer: RootV1,
    pub original_creature: IncarnationV72,
    pub current_original_creature: Option<ReferenceV1>,
    pub final_combat_damage: u32,
    pub stage: CombatStageV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub schema: String,
    pub viewer: PlayerSeatV1,
    pub meaning: MeaningV1,
    pub graveyard: Vec<AvailableV1>,
    pub casts: Vec<CastV1>,
    pub chooser: Option<ChoiceV1>,
    pub paid: Vec<PaymentV1>,
    pub combat: Vec<CombatV1>,
}

fn visible(state: &GameState, viewer: PlayerId, object: ObjectId) -> Result<ReferenceV1, String> {
    public::reference(state, object, Some(viewer))
        .ok_or("Greatwurm original public object absent".into())
}

fn incarnation(source: AbilitySourceContractV4) -> IncarnationV72 {
    IncarnationV72 {
        object: source.source.0,
        generation: source.zone_change_count,
        owner: source.owner.into(),
        controller: source.controller.into(),
        zone: source.zone,
    }
}

fn identity(printing: u16) -> IdentityV1 {
    match card_def::CARD_DEFS[usize::from(printing)].name {
        "Face-down creature" => IdentityV1::FaceDown { disguised: true },
        "Face-down card" => IdentityV1::FaceDown { disguised: false },
        _ => IdentityV1::Known { card_id: printing },
    }
}

fn public_payments(state: &GameState) -> Result<Vec<PaymentV1>, String> {
    receipts::debits(state)?
        .into_iter()
        .map(|(paid, debits)| {
            let source = paid.source;
            let original_spell = IncarnationV72 {
                object: source.source.0,
                generation: source.zone_change_count,
                owner: source.owner.into(),
                controller: source.controller.into(),
                zone: source.zone,
            };
            let mut grouped = BTreeMap::<(ObjectId, usize), DebitV1>::new();
            for row in debits {
                let kind = native::KINDS
                    .iter()
                    .position(|&kind| kind == row.unit.kind)
                    .ok_or("Greatwurm native counter kind absent")?;
                let binding = row.unit.permanent;
                grouped
                    .entry((binding.object, kind))
                    .and_modify(|old| old.remaining_count = row.after)
                    .or_insert_with(|| DebitV1 {
                        original: IncarnationV72 {
                            object: binding.object.0,
                            generation: binding.expected_zone_change_count,
                            owner: row.owner.into(),
                            controller: row.controller.into(),
                            zone: binding.expected_zone,
                        },
                        identity: identity(row.public_printing),
                        kind: row.unit.kind,
                        original_count: row.before,
                        remaining_count: row.after,
                    });
            }
            Ok(PaymentV1 {
                original_spell,
                origin_graveyard_generation: source
                    .spell_cast_origin
                    .ok_or("Greatwurm origin absent")?
                    .origin_zone_change_count,
                removals: grouped.into_values().collect(),
            })
        })
        .collect()
}

type Key = (u64, u32);
fn key(op: &EffectOp) -> Option<Key> {
    match op {
        EffectOp::GreatwurmCountersV1 {
            capture_history_index,
            observer,
        } => Some((*capture_history_index, *observer)),
        _ => None,
    }
}

fn combat(state: &GameState, viewer: PlayerId) -> Result<Vec<CombatV1>, String> {
    use EventKindV1 as K;
    let history = state.engine.event_history.scan_for_v1(&[
        K::GreatwurmCombatCaptureV1,
        K::GreatwurmCombatBoundV1,
        K::GreatwurmCollectedV1,
        K::GreatwurmPublishedV1,
        K::GreatwurmFinishedV1,
        K::GreatwurmCommitReceiptV1,
    ]);
    let mut captures = Vec::new();
    let mut bound = BTreeSet::new();
    let mut collected = BTreeMap::new();
    let mut published = BTreeMap::<Key, (usize, &StackItem)>::new();
    let mut finished = BTreeMap::new();
    let mut endings = BTreeSet::new();
    let mut dispatches = BTreeSet::new();
    for (at, row) in history.iter().enumerate() {
        let duplicated = match row {
            E::GreatwurmCombatCaptureV1 { .. } => {
                captures.push(u64::try_from(at).map_err(|_| "Greatwurm capture history width")?);
                false
            }
            E::GreatwurmCombatBoundV1 {
                capture_history_index,
                ..
            } => !bound.insert(*capture_history_index),
            E::GreatwurmCollectedV1 {
                capture_history_index,
                observer,
            } => collected
                .insert((*capture_history_index, *observer), at)
                .is_some(),
            E::GreatwurmPublishedV1 {
                capture_history_index,
                observer,
                item,
            } => published
                .insert((*capture_history_index, *observer), (at, item))
                .is_some(),
            E::GreatwurmFinishedV1 { stack_item_id } => {
                finished.insert(*stack_item_id, at).is_some()
            }
            E::GreatwurmCommitReceiptV1 { journal } => match journal.as_ref() {
                JournalV1::Ended { id, .. } => !endings.insert(*id),
                JournalV1::Dispatched { id, .. } => !dispatches.insert(*id),
                _ => false,
            },
            _ => false,
        };
        if duplicated {
            return Err("Greatwurm lifecycle contains a duplicate native occurrence".into());
        }
    }
    let mut held = BTreeMap::new();
    for (position, pending) in public::held(state).into_iter().enumerate() {
        if let Some(id) = key(&pending.effect) {
            engine::validate_pending_trigger_identity(state, pending)?;
            if held.insert(id, position).is_some() {
                return Err("Greatwurm held occurrence duplicated".into());
            }
        }
    }
    let mut stacked = BTreeMap::<StackItemId, (usize, &StackItem)>::new();
    for (position, item) in state.stack.iter().enumerate() {
        if native::selected(state, item) {
            native::validate_published(state, item)?;
            if stacked
                .insert(item.v4.stack_item_id, (position, item))
                .is_some()
            {
                return Err("Greatwurm live occurrence duplicated".into());
            }
        }
    }

    let mut output = Vec::new();
    for index in captures {
        if !bound.remove(&index) {
            return Err("Greatwurm combat capture lost its native wave binding".into());
        }
        let capture = native::capture(state, index)?;
        for (position, source) in capture.observers.iter().enumerate() {
            let observer = u32::try_from(position).map_err(|_| "Greatwurm observer width")?;
            let key = (index, observer);
            let collection = collected.remove(&key);
            let pending = held.remove(&key);
            let stage = if let Some((published_at, row)) = published.remove(&key) {
                native::validate_archived(state, row)?;
                if row.inline_effect.as_ref().and_then(self::key) != Some(key)
                    || collection.is_none_or(|collected_at| {
                        collected_at >= published_at || collected_at as u64 <= index
                    })
                    || pending.is_some()
                {
                    return Err("Greatwurm published row lost original collection".into());
                }
                let id = row.v4.stack_item_id;
                if let Some(finished_at) = finished.remove(&id) {
                    if !endings.remove(&id) || !dispatches.remove(&id) || stacked.contains_key(&id)
                    {
                        return Err("Greatwurm terminal disagrees with live occurrence".into());
                    }
                    match receipts::end(state, id, published_at, finished_at, capture.creature)? {
                        EndV1::Countered => CombatStageV1::Countered,
                        EndV1::OriginalCreatureGone => CombatStageV1::OriginalCreatureGone,
                        EndV1::Resolved {
                            controller,
                            before,
                            after,
                            ..
                        } => CombatStageV1::Resolved {
                            recipient_controller: controller.into(),
                            placed: u32::try_from(
                                after
                                    .checked_sub(before)
                                    .ok_or("Greatwurm counter delta width")?,
                            )
                            .map_err(|_| "Greatwurm counter delta was negative")?,
                        },
                    }
                } else {
                    let (position, live) = stacked
                        .remove(&id)
                        .ok_or("Greatwurm publication lost native lifetime")?;
                    if live != row {
                        return Err("Greatwurm live trigger differs from publication".into());
                    }
                    CombatStageV1::Published {
                        stack_position: u32::try_from(position)
                            .map_err(|_| "Greatwurm stack width")?,
                    }
                }
            } else if let Some(position) = pending {
                if collection.is_none_or(|collected_at| collected_at as u64 <= index) {
                    return Err("Greatwurm held trigger lacks original capture".into());
                }
                CombatStageV1::Held {
                    position: u32::try_from(position).map_err(|_| "Greatwurm held width")?,
                }
            } else if collection.is_none() {
                CombatStageV1::Captured
            } else {
                return Err("Greatwurm collected trigger lost its native lifetime".into());
            };
            let creature = capture.creature;
            let current_original_creature = state
                .objects
                .try_get(creature.source)
                .filter(|live| {
                    live.zone == Zone::Battlefield
                        && live.zone_change_count == creature.zone_change_count
                })
                .and_then(|_| public::reference(state, creature.source, Some(viewer)));
            let mut observer = public::archived_root(state, index as usize, *source, Some(viewer))?;
            observer.original_source.identity = IdentityV1::Known { card_id: PRINTING };
            output.push(CombatV1 {
                observer,
                original_creature: incarnation(creature),
                current_original_creature,
                final_combat_damage: u32::try_from(capture.amount)
                    .map_err(|_| "Greatwurm damage width")?,
                stage,
            });
        }
    }
    if !(bound.is_empty()
        && collected.is_empty()
        && published.is_empty()
        && finished.is_empty()
        && endings.is_empty()
        && dispatches.is_empty()
        && held.is_empty()
        && stacked.is_empty())
    {
        return Err("Greatwurm lifecycle contains a detached native operation".into());
    }
    Ok(output)
}

fn unit(
    state: &GameState,
    viewer: PlayerId,
    unit: UnitV1,
    selected: &[UnitV1],
) -> Result<UnitFactV1, String> {
    let permanent = visible(state, viewer, unit.permanent.object)?;
    let before_payment = native::count(state, unit.permanent.object, unit.kind)?;
    let used = u32::try_from(selected.iter().filter(|&&row| row == unit).count())
        .map_err(|_| "Greatwurm selected width")?;
    if permanent.zone != Zone::Battlefield
        || permanent.zone != unit.permanent.expected_zone
        || permanent.generation != unit.permanent.expected_zone_change_count
        || permanent.controller != viewer.into()
    {
        return Err("Greatwurm counter menu changed original permanent or controller".into());
    }
    Ok(UnitFactV1 {
        permanent,
        kind: unit.kind,
        before_payment,
        available_after_selected: before_payment
            .checked_sub(used)
            .ok_or("Greatwurm original counter count overspent")?,
    })
}

pub(crate) fn capture(
    state: &GameState,
    viewer: PlayerId,
    origin: &PolicyDecisionV5,
) -> Result<FactsV1, String> {
    require_scope(state)?;
    let decision = match origin {
        PolicyDecisionV5::Surface(SurfaceDecision::Decision(decision)) => Some(decision),
        _ => None,
    };
    let castable = match decision {
        Some(Decision::CastSpellOrPass {
            player,
            castable_spells,
            ..
        }) if *player == viewer => castable_spells.as_slice(),
        _ => &[],
    };
    let graveyard = state.players[viewer.index()]
        .graveyard
        .iter()
        .copied()
        .filter(|&object| public::visible_known(state, object, Some(viewer), |id| id == PRINTING))
        .map(|object| {
            Ok(AvailableV1 {
                source: visible(state, viewer, object)?,
                printed_counter_cost_available: greatwurm_cast_v1::allowed(state, object, viewer),
                castable_now: castable.contains(&object),
            })
        })
        .collect::<Result<_, String>>()?;
    let pending = state.engine.pending_cast.as_ref();
    let mut casts = Vec::new();
    for (position, item) in state.stack.iter().enumerate() {
        if item.kind != StackItemKind::Spell
            || !public::visible_known(state, item.source, Some(viewer), |id| id == PRINTING)
        {
            continue;
        }
        engine::validated_stack_item_target_spec(item, state)?;
        let source = item
            .v4
            .source_contract
            .ok_or("Greatwurm original spell source absent")?;
        let announcing = pending.is_some_and(|row| row.spell == item.source);
        let finalized_graveyard_route =
            source
                .spell_cast_origin
                .filter(|_| !announcing)
                .and_then(|origin| match origin.route {
                    SpellCastRouteV4::GreatwurmGraveyardV1 { .. } => {
                        Some(GraveyardRouteV1::PrintedSixCounterCost)
                    }
                    SpellCastRouteV4::GraveyardPermissionV1 { .. } => {
                        Some(GraveyardRouteV1::GrantedPermission)
                    }
                    _ => None,
                });
        casts.push(CastV1 {
            source: visible(state, viewer, item.source)?,
            stack_position: u32::try_from(position)
                .map_err(|_| "Greatwurm stack position width")?,
            origin_zone: source.spell_cast_origin.map(|origin| origin.origin_zone),
            announcing,
            uncast_copy: item.is_copy,
            finalized_graveyard_route,
        });
    }
    let chooser = match (decision, pending) {
        (
            Some(Decision::ChooseEffectOption {
                player,
                source,
                option_count,
            }),
            Some(pending),
        ) if *player == viewer
            && pending.spell == *source
            && pending.controller == viewer
            && pending.greatwurm_v1.is_some() =>
        {
            engine::validate_pending_cast(state, pending)?;
            let native_choice = pending.greatwurm_v1.as_ref().unwrap();
            let source = visible(state, viewer, *source)?;
            if native_choice.printed_route.is_none() && *option_count == 2 {
                Some(ChoiceV1::Route {
                    source,
                    routes: [
                        GraveyardRouteV1::GrantedPermission,
                        GraveyardRouteV1::PrintedSixCounterCost,
                    ],
                })
            } else if native_choice.printed_route == Some(true) && native_choice.units.len() < 6 {
                let selected = &native_choice.units;
                let legal = native::units(state, viewer, selected)?;
                if legal.len() != usize::from(*option_count) {
                    return Err("Greatwurm native counter menu count changed".into());
                }
                Some(ChoiceV1::Unit {
                    source,
                    selected: selected
                        .iter()
                        .map(|&row| unit(state, viewer, row, selected))
                        .collect::<Result<_, _>>()?,
                    legal: legal
                        .into_iter()
                        .map(|row| unit(state, viewer, row, selected))
                        .collect::<Result<_, _>>()?,
                    remaining: u8::try_from(6 - selected.len()).expect("six-unit cost"),
                })
            } else {
                return Err("Greatwurm native menu belongs to a completed or foreign stage".into());
            }
        }
        _ => None,
    };
    Ok(FactsV1 {
        schema: SCHEMA.into(),
        viewer: viewer.into(),
        meaning: printed_meaning()?,
        graveyard,
        casts,
        chooser,
        paid: public_payments(state)?,
        combat: combat(state, viewer)?,
    })
}

fn require_scope(state: &GameState) -> Result<(), String> {
    if !state.greatwurm_commit_receipts_v1 {
        return Err("Greatwurm owner requires prospective native commit receipts".into());
    }
    Ok(())
}

pub(crate) fn validate_native(state: &GameState) -> Result<(), String> {
    require_scope(state)?;
    receipts::debits(state)?;
    combat(state, PlayerId::P0)?;
    Ok(())
}
