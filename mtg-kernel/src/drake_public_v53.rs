//! Standalone V53 public facts. Private operation worlds are validated first,
//! then projected into public occurrence ordinals, numeric outcomes and safe refs.
use crate::card_def::{CardType, Keywords};
use crate::drake_hatcher_v1::custody::{self, JournalV1, RecordV1, RemovalV1};
use crate::event::CommittedEvent;
use crate::ids::ObjectId;
use crate::rl::{PlayerSeatV1, RlContractError};
use crate::rules_meaning_drake_hatcher_v53::{self as meaning, RootV53};
use crate::state::{AbilitySourceContractV4, Counters, GameState, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/drake-hatcher/public/v53";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncarnationV53 {
    pub object: u32,
    pub generation: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum IdentityV53 {
    Known { card_id: u16 },
    FaceDown { disguised: bool },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceV53 {
    pub incarnation: IncarnationV53,
    pub owner: PlayerSeatV1,
    pub controller: PlayerSeatV1,
    pub zone: Zone,
    pub identity: IdentityV53,
}
/// Mandatory native-width values. No omitted zeros or legacy placeholder slots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CountersV53 {
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
    pub lifelink: i16,
    pub time: u8,
    pub finality: u16,
    pub loyalty: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupertypeV53 {
    Basic,
    Snow,
    Legendary,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormV53 {
    pub names: Vec<String>,
    pub mana_value: u16,
    pub types: Vec<CardType>,
    pub supertypes: Vec<SupertypeV53>,
    pub subtype_ids: Vec<u16>,
    pub color_mask: u8,
    pub base_power: Option<i32>,
    pub base_toughness: Option<i32>,
    pub power: Option<i32>,
    pub toughness: Option<i32>,
    pub keyword_bits: u32,
    pub ward_generic: u16,
    pub minimum_blockers: usize,
    pub printed_abilities_active: bool,
    pub abilities_removed: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemporaryBindingV53 {
    OriginalIncarnation,
    LockedObjectSet,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemporaryBoostV53 {
    pub binding: TemporaryBindingV53,
    pub power: i32,
    pub toughness: i32,
    pub grant_haste: bool,
    pub until_end_of_turn: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardV53 {
    pub reference: ReferenceV53,
    pub face_index: Option<u8>,
    pub is_token: bool,
    pub tapped: bool,
    pub summoning_sick: bool,
    pub marked_damage: u32,
    pub counters: CountersV53,
    pub form: FormV53,
    /// Current generic resolved effects, not invented Prowess occurrences.
    pub temporary_boosts: Vec<TemporaryBoostV53>,
    pub attachments: Vec<ReferenceV53>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementV53 {
    pub before: i32,
    pub added: i32,
    pub after: i32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CombatOutcomeV53 {
    Held,
    Published,
    Countered,
    Resolved {
        placement_on_original_incarnation: Option<PlacementV53>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatV53 {
    /// Public capture order, not an executable history index.
    pub occurrence: usize,
    pub original_source: ReferenceV53,
    pub current_original_source: Option<ReferenceV53>,
    pub original_controller: PlayerSeatV1,
    pub damaged_player: PlayerSeatV1,
    pub final_positive_damage: i32,
    pub outcome: CombatOutcomeV53,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentV53 {
    pub kind: meaning::CounterV53,
    pub removed: u16,
    pub before: i32,
    pub after: i32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenV53 {
    pub created: ReferenceV53,
    pub current_original_token: Option<ReferenceV53>,
    pub original_controller: PlayerSeatV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ActivationOutcomeV53 {
    Published,
    Countered,
    Resolved { tokens: Vec<TokenV53> },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationV53 {
    /// Public payment order, not an executable history index or stack id.
    pub occurrence: usize,
    pub original_source: ReferenceV53,
    pub current_original_source: Option<ReferenceV53>,
    pub original_controller: PlayerSeatV1,
    pub payment: PaymentV53,
    pub outcome: ActivationOutcomeV53,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV53 {
    pub schema: String,
    pub producer_sha256: String,
    pub meanings: Vec<meaning::MeaningV53>,
    pub battlefield: [Vec<CardV53>; 2],
    pub combat: Vec<CombatV53>,
    pub activations: Vec<ActivationV53>,
}

pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut h = Sha256::new();
        for source in [
            SCHEMA,
            include_str!("drake_public_v53.rs"),
            include_str!("rl.rs"),
            include_str!("planeswalker_v1.rs"),
            include_str!("standard_creatures_v1.rs"),
        ] {
            h.update((source.len() as u64).to_le_bytes());
            h.update(source.as_bytes());
        }
        h.update(meaning::producer_sha256().as_bytes());
        h.update(b"legacy-graveyard-return-guard/v61\0");
        h.update(crate::graveyard_return_public_v61::producer_sha256().as_bytes());
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
}
fn error(message: impl Into<String>) -> RlContractError {
    RlContractError(message.into())
}
fn reference(state: &GameState, original: AbilitySourceContractV4) -> ReferenceV53 {
    let face = state.objects.try_get(original.source).and_then(|live| {
        let face = live.v4.face_down_v1?;
        let same = live.zone_change_count == original.zone_change_count;
        let disguised_resolution = face.disguised
            && original.zone == Zone::Stack
            && live.zone == Zone::Battlefield
            && original.zone_change_count.checked_add(1) == Some(live.zone_change_count);
        (same || disguised_resolution).then_some(face)
    });
    ReferenceV53 {
        incarnation: IncarnationV53 {
            object: original.source.0,
            generation: original.zone_change_count,
        },
        owner: original.owner.into(),
        controller: original.controller.into(),
        zone: original.zone,
        identity: face.map_or(
            IdentityV53::Known {
                card_id: original.card_def,
            },
            |face| IdentityV53::FaceDown {
                disguised: face.disguised,
            },
        ),
    }
}
fn current_original(state: &GameState, source: AbilitySourceContractV4) -> Option<ReferenceV53> {
    state
        .objects
        .try_get(source.source)
        .filter(|live| {
            live.zone == Zone::Battlefield && live.zone_change_count == source.zone_change_count
        })
        .map(|_| {
            reference(
                state,
                AbilitySourceContractV4::capture(state, source.source),
            )
        })
}
fn counters(
    state: &GameState,
    id: ObjectId,
    scope: crate::optional_trigger_public_v1::CounterScopeV61,
) -> Result<CountersV53, RlContractError> {
    let live = state.objects.get(id);
    // Exhaustive destructuring requires a deliberate V53 decision for any
    // newly added storage family, rather than silently losing its counters.
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
    } = live.counters;
    if fellowship != 0 {
        return Err(error("V53 does not expose fellowship counters"));
    }
    if scope == crate::optional_trigger_public_v1::CounterScopeV61::CompleteV61 {
        crate::graveyard_return_public_v61::counter_values(state, id).map_err(error)?;
    }
    if revival != 0 && scope == crate::optional_trigger_public_v1::CounterScopeV61::Legacy {
        return Err(error("V53 does not expose Nine-Lives revival counters"));
    }
    let lifelink = live.v4.lifelink_keyword_counters;
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
        i32::from(lifelink),
    ]
    .iter()
    .any(|n| *n < 0)
    {
        return Err(error("V53 counter underflow in public battlefield"));
    }
    Ok(CountersV53 {
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
        lifelink,
        time: live.v4.time_counters_v1,
        finality: live.v4.creature_upgrade.as_ref().map_or(0, |u| u.finality),
        loyalty: crate::planeswalker_v1::loyalty(state, id).unwrap_or(0),
    })
}
fn form(state: &GameState, id: ObjectId) -> Result<FormV53, RlContractError> {
    let live = state.objects.get(id);
    let base_power = crate::engine::effective_base_power(state, id);
    let base_toughness = crate::engine::effective_base_toughness(state, id);
    let has_pt = base_power.is_some() || base_toughness.is_some();
    let pt = has_pt
        .then(|| crate::engine::checked_effective_pt(state, id))
        .transpose()
        .map_err(error)?;
    let printed = crate::continuous_characteristics_v1::printed_abilities_active(state, id);
    let removed = crate::continuous_characteristics_v1::removal_timestamp(state, id).is_some();
    let keyword_bits = (0..u32::BITS).fold(0, |bits, bit| {
        let keyword = Keywords(1_u32 << bit);
        if crate::engine::has_effective_keyword(state, id, keyword) {
            bits | keyword.0
        } else {
            bits
        }
    });
    Ok(FormV53 {
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
            .map(|kind| match kind {
                crate::card_def::Supertype::Basic => SupertypeV53::Basic,
                crate::card_def::Supertype::Snow => SupertypeV53::Snow,
                crate::card_def::Supertype::Legendary => SupertypeV53::Legendary,
            })
            .collect(),
        subtype_ids: crate::engine::effective_subtype_ids(state, id),
        color_mask: crate::engine::object_color_mask(state, id),
        base_power,
        base_toughness,
        power: pt.map(|value| value.0),
        toughness: pt.map(|value| value.1),
        keyword_bits,
        ward_generic: if live.v4.face_down_v1.is_some_and(|face| face.disguised) && !removed {
            2
        } else if printed {
            live.v4.ward_generic
        } else {
            0
        },
        minimum_blockers: if crate::engine::object_has_type(state, id, CardType::Creature) {
            crate::engine::minimum_blockers_required(state, id)
        } else {
            0
        },
        printed_abilities_active: printed,
        abilities_removed: removed,
    })
}
fn temporary_boosts(state: &GameState, id: ObjectId) -> Vec<TemporaryBoostV53> {
    use crate::engine::UntilEndOfTurnEffect as E;
    let generation = state.objects.get(id).zone_change_count;
    state
        .engine
        .until_end_of_turn
        .iter()
        .filter_map(|effect| match effect {
            E::ResolvedObjectEffect {
                object_id,
                object_zone_change_count,
                power,
                toughness,
                grant_haste,
                ..
            } if *object_id == id && *object_zone_change_count == generation => {
                Some(TemporaryBoostV53 {
                    binding: TemporaryBindingV53::OriginalIncarnation,
                    power: *power,
                    toughness: *toughness,
                    grant_haste: *grant_haste,
                    until_end_of_turn: true,
                })
            }
            E::ResolvedSetEffect {
                object_ids,
                power,
                toughness,
                grant_haste,
                ..
            } if object_ids.contains(&id) => Some(TemporaryBoostV53 {
                binding: TemporaryBindingV53::LockedObjectSet,
                power: *power,
                toughness: *toughness,
                grant_haste: *grant_haste,
                until_end_of_turn: true,
            }),
            _ => None,
        })
        .collect()
}
fn battlefield(
    state: &GameState,
    scope: crate::optional_trigger_public_v1::CounterScopeV61,
) -> Result<[Vec<CardV53>; 2], RlContractError> {
    let mut result: [Vec<CardV53>; 2] = [Vec::new(), Vec::new()];
    let mut seen = Vec::new();
    for (seat, player) in state.players.iter().enumerate() {
        for id in &player.battlefield {
            let live = state
                .objects
                .try_get(*id)
                .ok_or_else(|| error("V53 public battlefield object absent"))?;
            if seen.contains(id)
                || live.zone != Zone::Battlefield
                || live.controller.index() != seat
            {
                return Err(error("V53 public battlefield roster duplicated or changed"));
            }
            seen.push(*id);
            let mut attachments = Vec::new();
            for attachment in &live.attachments {
                let attached = state
                    .objects
                    .try_get(*attachment)
                    .ok_or_else(|| error("V53 public attachment absent"))?;
                if attached.zone != Zone::Battlefield
                    || attached.v4.attached_to
                        != Some(crate::state::ObjectLinkV4 {
                            object: *id,
                            zone_change_count: live.zone_change_count,
                        })
                {
                    return Err(error("V53 public attachment relation changed"));
                }
                attachments.push(reference(
                    state,
                    AbilitySourceContractV4::capture(state, *attachment),
                ));
            }
            result[seat].push(CardV53 {
                reference: reference(state, AbilitySourceContractV4::capture(state, *id)),
                face_index: live.v4.face_down_v1.is_none().then_some(live.v4.face_index),
                is_token: live.v4.is_token,
                tapped: live.tapped,
                summoning_sick: live.summoning_sick,
                marked_damage: live.damage,
                counters: counters(state, *id, scope)?,
                form: form(state, *id)?,
                temporary_boosts: temporary_boosts(state, *id),
                attachments,
            });
        }
    }
    if state
        .objects
        .iter()
        .any(|(id, live)| live.zone == Zone::Battlefield && !seen.contains(&id))
    {
        return Err(error(
            "V53 public battlefield incarnation lacks its controller roster",
        ));
    }
    Ok(result)
}
fn removed<'a>(rows: &'a [&RecordV1], publication: u64) -> Option<&'a RecordV1> {
    rows.iter()
        .copied()
        .find(|row| matches!(row.value,JournalV1::Removed{publication:p,..} if p==publication))
}
fn completed<'a>(
    rows: &'a [&RecordV1],
    removal: u64,
) -> Result<(&'a RecordV1, &'a GameState), RlContractError> {
    rows.iter()
        .find_map(|row| match &row.value {
            JournalV1::Completed { removed, output } if *removed == removal => {
                Some((*row, output.as_ref()))
            }
            _ => None,
        })
        .ok_or_else(|| error("V53 public original removal lacks completion"))
}
fn combat_outcome(
    rows: &[&RecordV1],
    capture: u64,
    source: AbilitySourceContractV4,
) -> Result<CombatOutcomeV53, RlContractError> {
    let Some(publication) = rows
        .iter()
        .find(|row| matches!(row.value,JournalV1::Published{capture:c,..}if c==capture))
    else {
        return Ok(CombatOutcomeV53::Held);
    };
    let Some(removal) = removed(rows, publication.index) else {
        return Ok(CombatOutcomeV53::Published);
    };
    let JournalV1::Removed { kind, original, .. } = &removal.value else {
        unreachable!()
    };
    let (_, output) = completed(rows, removal.index)?;
    if *kind == RemovalV1::Countered {
        return Ok(CombatOutcomeV53::Countered);
    }
    let live = original
        .objects
        .try_get(source.source)
        .ok_or_else(|| error("V53 original placement source absent"))?;
    let placement = if live.zone == Zone::Battlefield
        && live.zone_change_count == source.zone_change_count
    {
        let after = output
            .objects
            .try_get(source.source)
            .ok_or_else(|| error("V53 completed placement source absent"))?;
        if after.zone != Zone::Battlefield || after.zone_change_count != source.zone_change_count {
            return Err(error("V53 placement changed source incarnation"));
        }
        let before = live.counters.incubation;
        let added = after
            .counters
            .incubation
            .checked_sub(before)
            .filter(|n| *n >= 0)
            .ok_or_else(|| error("V53 placement count arithmetic changed"))?;
        Some(PlacementV53 {
            before,
            added,
            after: after.counters.incubation,
        })
    } else {
        None
    };
    Ok(CombatOutcomeV53::Resolved {
        placement_on_original_incarnation: placement,
    })
}
fn activation_outcome(
    state: &GameState,
    rows: &[&RecordV1],
    publication: &RecordV1,
    actor: crate::ids::PlayerId,
    token_id: u16,
) -> Result<ActivationOutcomeV53, RlContractError> {
    let Some(removal) = removed(rows, publication.index) else {
        return Ok(ActivationOutcomeV53::Published);
    };
    let JournalV1::Removed { kind, .. } = &removal.value else {
        unreachable!()
    };
    let (complete, output) = completed(rows, removal.index)?;
    if *kind == RemovalV1::Countered {
        return Ok(ActivationOutcomeV53::Countered);
    }
    let start = usize::try_from(removal.index)
        .ok()
        .and_then(|at| at.checked_add(1))
        .ok_or_else(|| error("V53 removal boundary overflow"))?;
    let end =
        usize::try_from(complete.index).map_err(|_| error("V53 completion boundary overflow"))?;
    let events = state
        .engine
        .event_history
        .get(start..end)
        .ok_or_else(|| error("V53 actual token completion suffix absent"))?;
    let mut tokens = Vec::new();
    for event in events {
        let CommittedEvent::CreateToken {
            object,
            token_def,
            controller,
            ..
        } = event
        else {
            continue;
        };
        if *token_def != token_id || *controller != actor {
            return Err(error(
                "V53 original paid token blueprint/controller changed",
            ));
        }
        let live = output
            .objects
            .try_get(*object)
            .ok_or_else(|| error("V53 actual created token incarnation absent"))?;
        if live.zone != Zone::Battlefield
            || live.card_def != token_id
            || live.controller != actor
            || !live.v4.is_token
        {
            return Err(error("V53 completed token incarnation changed"));
        }
        let original = AbilitySourceContractV4::capture(output, *object);
        tokens.push(TokenV53 {
            created: reference(state, original),
            current_original_token: current_original(state, original),
            original_controller: actor.into(),
        });
    }
    Ok(ActivationOutcomeV53::Resolved { tokens })
}
/// Validate the genuine private inputs and complete operation replay first.
/// Hashes and the current board never stand in for historical authority.
pub fn capture(state: &GameState) -> Result<FactsV53, RlContractError> {
    capture_scoped(
        state,
        crate::optional_trigger_public_v1::CounterScopeV61::Legacy,
    )
}
pub(crate) fn capture_complete_v61(state: &GameState) -> Result<FactsV53, RlContractError> {
    capture_scoped(
        state,
        crate::optional_trigger_public_v1::CounterScopeV61::CompleteV61,
    )
}
fn capture_scoped(
    state: &GameState,
    scope: crate::optional_trigger_public_v1::CounterScopeV61,
) -> Result<FactsV53, RlContractError> {
    crate::effect::require_no_copied_token_abilities_v1(state).map_err(error)?;
    crate::engine::validate_battlefield_pt(state)
        .map_err(|id| error(format!("native PT envelope exceeded for {id:?}")))?;
    if crate::linked_aura_operation_v1::locally_owned(state) {
        return Err(error(
            "V53 cannot publish a private synchronous operation tail",
        ));
    }
    // Fellowship zero still leaves an incarnation-bound chosen type and a
    // current-counter anthem. Neither that entry checkpoint nor its live
    // metadata has a representation in this frozen V53 appendix. Inspect
    // public battlefield rows only, never hidden hand/library printings.
    if state
        .engine
        .linked_aura_operation_v1
        .pending_kinship_v1
        .is_some()
        || state
            .objects
            .iter()
            .any(|(_, live)| live.zone == Zone::Battlefield && live.v4.kinship_type_v1.is_some())
    {
        return Err(error(
            "V53 does not expose native creature-type entry mechanics",
        ));
    }
    custody::validate(state).map_err(error)?;
    // Reuse existing generic authority for Prowess/current effect state. This
    // does not manufacture a new historical Prowess occurrence receipt.
    crate::effect::validate_pending_effect_choice(state).map_err(error)?;
    for item in &state.stack {
        crate::engine::validated_stack_item_target_spec(item, state).map_err(error)?;
    }
    for pending in state
        .engine
        .pending_triggers
        .iter()
        .chain(state.legend_pending_v1.iter().flatten())
        .chain(
            state
                .pending_legend_rule_v1
                .iter()
                .flat_map(|p| p.waiting_triggers()),
        )
        .chain(
            state
                .standard_v1
                .iter()
                .filter_map(|s| s.resolution_play.as_ref())
                .flat_map(|p| p.deferred_triggers.iter()),
        )
    {
        crate::engine::validate_pending_trigger_identity(state, pending).map_err(error)?;
    }
    let id = crate::card_def::card_id_by_name("Drake Hatcher")
        .ok_or_else(|| error("V53 genuine printing absent"))?;
    let meanings = [
        RootV53::Prowess,
        RootV53::CombatIncubation,
        RootV53::RemoveIncubationForDrake,
    ]
    .into_iter()
    .map(|root| meaning::printed_meaning(id, root).map_err(error))
    .collect::<Result<Vec<_>, _>>()?;
    let token_id = crate::card_def::card_id_by_name("Drake Token")
        .ok_or_else(|| error("V53 genuine Drake token absent"))?;
    let rows = state
        .engine
        .event_history
        .iter()
        .filter_map(custody::journal)
        .collect::<Vec<_>>();
    let mut combat = Vec::new();
    let mut activations = Vec::new();
    for row in &rows {
        match &row.value {
            JournalV1::Captured {
                source,
                player,
                amount,
                ..
            } => combat.push(CombatV53 {
                occurrence: combat.len(),
                original_source: reference(state, *source),
                current_original_source: current_original(state, *source),
                original_controller: source.controller.into(),
                damaged_player: (*player).into(),
                final_positive_damage: *amount,
                outcome: combat_outcome(&rows, row.index, *source)?,
            }),
            JournalV1::Paid { debit, item, .. } => {
                let debit = rows
                    .iter()
                    .find(|row| row.index == *debit)
                    .ok_or_else(|| error("V53 original typed payment absent"))?;
                let JournalV1::Debit {
                    source,
                    count,
                    before,
                    after,
                    ..
                } = &debit.value
                else {
                    return Err(error("V53 payment lost actual typed debit"));
                };
                activations.push(ActivationV53 {
                    occurrence: activations.len(),
                    original_source: reference(state, *source),
                    current_original_source: current_original(state, *source),
                    original_controller: item.controller.into(),
                    payment: PaymentV53 {
                        kind: meaning::CounterV53::Incubation,
                        removed: *count,
                        before: *before,
                        after: *after,
                    },
                    outcome: activation_outcome(state, &rows, row, item.controller, token_id)?,
                });
            }
            _ => {}
        }
    }
    Ok(FactsV53 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        meanings,
        battlefield: battlefield(state, scope)?,
        combat,
        activations,
    })
}
pub fn validate(state: &GameState, value: &FactsV53) -> Result<(), RlContractError> {
    if capture(state)? == *value {
        Ok(())
    } else {
        Err(error(
            "V53 public Drake facts differ from authenticated recapture",
        ))
    }
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
