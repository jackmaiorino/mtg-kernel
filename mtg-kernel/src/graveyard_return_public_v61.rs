//! Viewer-public, source-only Alesha/Nine-Lives appendix. This is not a complete
//! ordinary observation or an owning/executable capability. Raw native worlds
//! retain the engine's existing mutable-state structural trust contract.
use crate::{
    card_def,
    effect::EffectOp,
    event::CommittedEvent as E,
    ids::{ObjectId, PlayerId},
    optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    rules_meaning_graveyard_return_v61::{self as meaning, FamilyV61},
    state::{
        AbilitySourceContractV4, Counters, GameState, ObjectLinkV4, StackItemKind,
        StackTargetContractV4, Target, Zone,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const SCHEMA: &str = "mtg-kernel/graveyard-return-public/v61";

/// Complete native-width counter roster. Every zero is explicit. Face-down
/// references remain anonymous; counters on a battlefield object are public.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CountersV61 {
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
    pub lifelink: i16,
    pub time: u8,
    pub finality: u16,
    pub loyalty: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BattlefieldCountersV61 {
    pub card: public::ReferenceV1,
    pub counters: CountersV61,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentV61 {
    pub family: FamilyV61,
    pub card: public::ReferenceV1,
    pub printed_abilities_active: bool,
    pub power: Option<i32>,
    pub toughness: Option<i32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizedV61 {
    pub family: FamilyV61,
    pub card: public::ReferenceV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerSourceV61 {
    CurrentOriginalIncarnation,
    ExactDepartureLki,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PowerV61 {
    pub amount: i32,
    pub source: PowerSourceV61,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV61 {
    Held,
    Published,
    Resolving,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AleshaChoiceV61 {
    BeforeResolution,
    AwaitingCostlessReturnChoice,
    Answered { accept: bool },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ProgramV61 {
    AleshaAttackCounter {
        plus_one: i32,
    },
    AleshaRaid {
        power: PowerV61,
        captured_controller_attacked_this_turn: bool,
        may_return_at_resolution: bool,
        optional_choice: AleshaChoiceV61,
    },
    NineLivesDeath {
        departure_revival: i32,
        return_base_before_replacements: i32,
    },
    NineLivesDelayedReturn {
        departure_revival: i32,
        return_base_before_replacements: i32,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetV61 {
    /// The originally public graveyard contract, never its newer hidden incarnation.
    pub original: public::ReferenceV1,
    pub current_exact_graveyard_card: Option<public::ReferenceV1>,
    pub legal_now: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerV61 {
    pub family: FamilyV61,
    pub stage: StageV61,
    /// Actual position within that native stage's complete row vector.
    pub position: u32,
    pub source: public::RootV1,
    pub program: ProgramV61,
    pub targets: Vec<TargetV61>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeathV61 {
    pub occurrence: u32,
    pub source: public::RootV1,
    pub departure_revival: i32,
    pub return_base_before_replacements: i32,
    pub collection_completed: bool,
    pub exact_graveyard_card_now: Option<public::ReferenceV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleV61 {
    pub occurrence: u32,
    pub source: public::RootV1,
    pub departure_revival: i32,
    pub return_base_before_replacements: i32,
    pub next_future_any_player_end_step: bool,
    pub exact_graveyard_card_now: Option<public::ReferenceV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV61 {
    pub family: FamilyV61,
    pub position: u32,
    pub source: public::ReferenceV1,
    pub original_controller: PlayerSeatV1,
    pub is_uncast_copy: bool,
    pub payment_pending: bool,
    pub genuine_cast_finalized: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AleshaDepartureV61 {
    pub occurrence: u32,
    pub source: public::RootV1,
    pub power: i32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV61 {
    pub schema: String,
    pub producer_sha256: String,
    pub viewer: PlayerSeatV1,
    pub meanings: Vec<meaning::MeaningV61>,
    pub authorized: Vec<AuthorizedV61>,
    pub current: Vec<CurrentV61>,
    pub battlefield_counters: [Vec<BattlefieldCountersV61>; 2],
    pub casts: Vec<CastV61>,
    pub triggers: Vec<TriggerV61>,
    pub alesha_departures: Vec<AleshaDepartureV61>,
    pub nine_lives_deaths: Vec<DeathV61>,
    pub nine_lives_schedules: Vec<ScheduleV61>,
    pub complete_ordinary_observation: bool,
    pub owning_consumer_admitted: bool,
}
pub fn producer_sha256() -> String {
    meaning::producer_sha256()
}
fn ordinal(value: usize) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| "V61 public ordinal overflow".into())
}
fn family(id: u16, ids: &[(FamilyV61, u16)]) -> Option<FamilyV61> {
    ids.iter()
        .find_map(|(family, selected)| (*selected == id).then_some(*family))
}
fn exact_graveyard(
    state: &GameState,
    source: AbilitySourceContractV4,
    link: ObjectLinkV4,
    viewer: PlayerId,
) -> Option<public::ReferenceV1> {
    state
        .objects
        .try_get(link.object)
        .filter(|live| {
            live.zone == Zone::Graveyard
                && state.players[live.owner.index()]
                    .graveyard
                    .contains(&link.object)
                && !live.v4.is_token
                && live.owner == source.owner
                && live.zone_change_count == link.zone_change_count
        })
        .and_then(|_| public::reference(state, link.object, Some(viewer)))
}
/// Older consumers select only authorized identities or originally public
/// native provenance. A concealed current printing never selects this guard.
pub(crate) fn require_legacy_visible(
    state: &GameState,
    viewer: Option<PlayerId>,
) -> Result<(), String> {
    // The registry is fixed at build time, so the family's ids are computed once.
    static IDS: std::sync::OnceLock<std::collections::BTreeSet<u16>> = std::sync::OnceLock::new();
    let ids = IDS.get_or_init(|| {
        FamilyV61::ALL
            .into_iter()
            .filter_map(|family| card_def::card_id_by_name(family.name()))
            .collect()
    });
    let selected = |id: u16| ids.contains(&id);
    let visible = state.objects.iter().any(|(id, live)| {
        public::visible_known(state, id, viewer, |card_id| selected(card_id))
            || viewer.is_some_and(|viewer| {
                live.v4
                    .face_down_v1
                    .is_some_and(|face| face.lookers & (1 << viewer.index()) != 0)
                    && selected(live.card_def)
            })
    });
    if visible {
        return Err("graveyard return rules require the distinct V61 consumer".into());
    }
    let item_selected = |item: &crate::state::StackItem| {
        item.v4
            .source_contract
            .is_some_and(|s| selected(s.card_def))
            || item
                .v4
                .ability_source_contract
                .is_some_and(|s| s.zone == Zone::Battlefield && selected(s.card_def))
    };
    let marker_selected = |event: &E| match event {
        E::AleshaPowerLkiV1 { .. } => true,
        E::ControlledCreatureDeathCaptureV1 { source, .. } => selected(source.card_def),
        _ => false,
    };
    let retained = state.stack.iter().any(item_selected)
        || state.engine.pending_effect.as_ref().is_some_and(|p| {
            item_selected(&p.resolving_item)
                || p.ctx
                    .ability_source_contract
                    .is_some_and(|s| s.zone == Zone::Battlefield && selected(s.card_def))
        })
        || public::held(state).iter().any(|p| {
            p.source_contract
                .is_some_and(|s| s.zone == Zone::Battlefield && selected(s.card_def))
        })
        || state.raid_waiting_v1.iter().flatten().any(|p| {
            p.source_contract
                .is_some_and(|s| s.zone == Zone::Battlefield && selected(s.card_def))
        })
        || state
            .nine_lives_delayed_v1
            .iter()
            .flatten()
            .any(|s| selected(s.source.card_def))
        || state.engine.event_history.iter().any(marker_selected)
        || state.engine.event_log.iter().any(marker_selected);
    if retained {
        // Authenticate the captured source/departure/death/schedule through the
        // existing read-only native appendix. No facts or authority are exported.
        // Malformed provenance also refuses, without consulting hidden identity
        // as a selector or modifying/collecting the original records.
        capture(state, viewer.unwrap_or(PlayerId::P0))?;
        return Err("graveyard return rules require the distinct V61 consumer".into());
    }
    Ok(())
}

pub(crate) fn require_call_legacy_visible_v62(
    state: &GameState,
    viewer: Option<PlayerId>,
) -> Result<(), String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Ok(());
    }
    if crate::card_def::card_id_by_name("Valkyrie's Call").is_none() {
        return Ok(());
    }
    let selected = 459;
    if crate::card_def::card_id_by_name("Valkyrie's Call") != Some(selected) {
        return Err("Call printing ID changed".into());
    }
    let visible = state.objects.iter().any(|(id, live)| {
        public::visible_known(state, id, viewer, |card_id| card_id == selected)
            || viewer.is_some_and(|viewer| {
                live.v4
                    .face_down_v1
                    .is_some_and(|face| face.lookers & (1 << viewer.index()) != 0)
                    && live.card_def == selected
            })
    });
    let source = |s: AbilitySourceContractV4| s.card_def == selected;
    let retained = state.stack.iter().any(|r| {
        r.v4.ability_source_contract.is_some_and(source)
            || r.v4.source_contract.is_some_and(|s| s.card_def == selected)
    }) || state.engine.pending_effect.as_ref().is_some_and(|p| {
        p.resolving_item
            .v4
            .ability_source_contract
            .is_some_and(source)
            || p.ctx.ability_source_contract.is_some_and(source)
    }) || public::held(state)
        .iter()
        .any(|p| p.source_contract.is_some_and(source))
        || state.engine.event_history.iter().chain(&state.engine.event_log).any(
            |e| matches!(e, E::ControlledCreatureDeathCaptureV1 { source: s, .. } if source(*s)),
        );
    if visible || retained {
        Err("Valkyrie's Call requires its distinct complete consumer".into())
    } else {
        Ok(())
    }
}

pub(crate) fn counter_values(state: &GameState, id: ObjectId) -> Result<CountersV61, String> {
    counter_values_scoped(state, id, public::CounterScopeV61::CompleteV61)
}
/// Private compatibility prefix: the distinct Call observation retains the
/// complete seventeen-field roster. No old V61 artifact admits fellowship.
pub(crate) fn counter_values_call_compat_v62(
    state: &GameState,
    id: ObjectId,
) -> Result<CountersV61, String> {
    if state.objects.get(id).counters.fellowship < 0 {
        return Err("Call negative native fellowship counter".into());
    }
    counter_values_scoped(state, id, public::CounterScopeV61::CallV62)
}
/// V72-only sixteen-field prefix. The complete eighteen counters remain mandatory
/// in the enclosing Tinybones observation; legacy V61/Call entrypoints still refuse stash.
pub(crate) fn counter_values_tinybones_compat_v72(
    state: &GameState,
    id: ObjectId,
) -> Result<CountersV61, String> {
    counter_values_scoped(state, id, public::CounterScopeV61::TinybonesV72)
}
fn counter_values_scoped(
    state: &GameState,
    id: ObjectId,
    scope: public::CounterScopeV61,
) -> Result<CountersV61, String> {
    let live = state.objects.get(id);
    let call = matches!(
        scope,
        public::CounterScopeV61::CallV62 | public::CounterScopeV61::TinybonesV72
    );
    if scope == public::CounterScopeV61::TinybonesV72 {
        crate::tinybones_public_v72::counters(state, id)?;
    }
    if call && live.counters.fellowship < 0 {
        return Err("Call negative native fellowship counter".into());
    }
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
    } = live.counters;
    if stash != 0 && scope != public::CounterScopeV61::TinybonesV72 {
        return Err("public projection does not expose stash counters".into());
    }
    if fellowship != 0 && !call {
        return Err("V61 does not expose fellowship counters".into());
    }
    let lifelink = live.v4.lifelink_keyword_counters;
    if [
        i64::from(plus1_plus1),
        i64::from(minus1_minus1),
        i64::from(minus0_minus1),
        i64::from(stun),
        i64::from(lore),
        i64::from(oil),
        i64::from(charge),
        i64::from(net),
        i64::from(bait),
        i64::from(soul),
        i64::from(incubation),
        i64::from(revival),
        i64::from(lifelink),
    ]
    .iter()
    .any(|n| *n < 0)
    {
        return Err("V61 negative native counter".into());
    }
    Ok(CountersV61 {
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
        lifelink,
        time: live.v4.time_counters_v1,
        finality: live.v4.creature_upgrade.as_ref().map_or(0, |u| u.finality),
        loyalty: crate::planeswalker_v1::loyalty(state, id).unwrap_or(0),
    })
}
fn departure_revival(state: &GameState, source: AbilitySourceContractV4) -> Result<i32, String> {
    let mut rows = state.counter_lki_v1.iter().flatten().filter(|row| {
        row.source.object == source.source
            && row.source.zone_change_count == source.zone_change_count
    });
    let count = rows
        .next()
        .ok_or("V61 exact departure counter LKI absent")?
        .counters
        .revival;
    if rows.next().is_some() || count <= 0 {
        return Err("V61 departure counter LKI duplicated or nonpositive".into());
    }
    Ok(count)
}
struct DeathBinding {
    source: AbilitySourceContractV4,
    graveyard: ObjectLinkV4,
    count: i32,
    capture_index: usize,
    collected: bool,
}
fn death_bindings(state: &GameState, nine: u16) -> Result<Vec<DeathBinding>, String> {
    let mut rows = vec![];
    let mut seen = BTreeSet::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let E::ControlledCreatureDeathCaptureV1 {
            source,
            ability_index,
            died,
            controller_before,
        } = event
        else {
            continue;
        };
        if source.card_def != nine {
            continue;
        }
        if *ability_index != 0
            || source.zone != Zone::Battlefield
            || source.source != died.object
            || source.zone_change_count != died.zone_change_count
            || source.controller != *controller_before
            || !seen.insert((source.source.0, source.zone_change_count))
        {
            return Err("V61 changed or duplicate Nine-Lives departure capture".into());
        }
        crate::engine::validate_historical_ability_source_contract(state, *source)?;
        let prior = Some(crate::creature_death_capture_v1::validated_capture_death(
            state, index,
        )?);
        if !matches!(prior,Some(E::ZoneChange {object,from:Zone::Battlefield,to:Zone::Graveyard,controller_before:actual})if *object==source.source && actual==controller_before)
        {
            return Err("V61 capture lost its adjacent actual death".into());
        }
        let collected=state.engine.event_history.iter().filter(|row|matches!(row,E::ControlledCreatureDeathCaptureCollectedV1{capture_history_index}if *capture_history_index==index as u64)).count();
        if collected > 1 {
            return Err("V61 death capture collected twice".into());
        }
        rows.push(DeathBinding {
            source: *source,
            graveyard: ObjectLinkV4 {
                object: died.object,
                zone_change_count: died
                    .zone_change_count
                    .checked_add(1)
                    .ok_or("V61 incarnation overflow")?,
            },
            count: departure_revival(state, *source)?,
            capture_index: index,
            collected: collected == 1,
        });
    }
    Ok(rows)
}
fn nine_binding(
    deaths: &[DeathBinding],
    source: AbilitySourceContractV4,
    graveyard: ObjectLinkV4,
    base: i32,
) -> Result<&DeathBinding, String> {
    deaths
        .iter()
        .find(|row| {
            row.source == source
                && row.graveyard == graveyard
                && row.count.checked_sub(1) == Some(base)
        })
        .ok_or_else(|| {
            "V61 Nine-Lives program changed its original source, death incarnation or counters"
                .into()
        })
}
fn targets(
    state: &GameState,
    source: AbilitySourceContractV4,
    contracts: &[StackTargetContractV4],
    power: i32,
    viewer: PlayerId,
) -> Result<Vec<TargetV61>, String> {
    contracts
        .iter()
        .map(|contract| {
            let StackTargetContractV4::Object {
                object,
                card_def,
                owner,
                controller,
                zone: Zone::Graveyard,
                zone_change_count,
                spell_copy_origin: None,
            } = *contract
            else {
                return Err("V61 Raid target lost its original public graveyard contract".into());
            };
            let original = public::ReferenceV1 {
                object: object.0,
                generation: zone_change_count,
                owner: owner.into(),
                controller: controller.into(),
                zone: Zone::Graveyard,
                identity: public::IdentityV1::Known { card_id: card_def },
            };
            let current = state
                .objects
                .try_get(object)
                .filter(|live| {
                    live.zone == Zone::Graveyard
                        && live.owner == owner
                        && live.card_def == card_def
                        && live.zone_change_count == zone_change_count
                })
                .and_then(|_| public::reference(state, object, Some(viewer)));
            let legal_now = current.is_some()
                && crate::engine::effect_target_is_legal_from_ability_source_with_power_lki(
                    state,
                    source,
                    source.controller,
                    card_def::TargetSpec::CreatureCardInOwnGraveyardManaValueAtMostSourcePower,
                    &[Target::Object(object)],
                    0,
                    Some(power),
                );
            Ok(TargetV61 {
                original,
                current_exact_graveyard_card: current,
                legal_now,
            })
        })
        .collect()
}
#[allow(clippy::too_many_arguments)]
fn trigger(
    state: &GameState,
    source: AbilitySourceContractV4,
    op: &EffectOp,
    contracts: &[StackTargetContractV4],
    stage: StageV61,
    position: usize,
    viewer: PlayerId,
    ids: &[(FamilyV61, u16)],
    deaths: &[DeathBinding],
) -> Result<Option<TriggerV61>, String> {
    let Some(family) = family(source.card_def, ids) else {
        return Ok(None);
    };
    let mut target_facts = vec![];
    let program = match (family, op) {
        (FamilyV61::Alesha, EffectOp::PutPlusOnePlusOneCounterOnBoundObject { object }) => {
            if object.object != source.source
                || object.expected_zone != source.zone
                || object.expected_zone_change_count != source.zone_change_count
            {
                return Err("V61 attack changed its original counter recipient".into());
            }
            ProgramV61::AleshaAttackCounter { plus_one: 1 }
        }
        (FamilyV61::Alesha, EffectOp::AleshaRaidReturn { .. }) => {
            let amount = crate::engine::alesha_raid_source_power(state, source)
                .ok_or("V61 Raid source power unavailable")?;
            let current = state.objects.try_get(source.source).is_some_and(|live| {
                live.zone == Zone::Battlefield && live.zone_change_count == source.zone_change_count
            });
            target_facts = targets(state, source, contracts, amount, viewer)?;
            ProgramV61::AleshaRaid {
                power: PowerV61 {
                    amount,
                    source: if current {
                        PowerSourceV61::CurrentOriginalIncarnation
                    } else {
                        PowerSourceV61::ExactDepartureLki
                    },
                },
                captured_controller_attacked_this_turn: state
                    .player_attacked_this_turn_v1(source.controller),
                may_return_at_resolution: true,
                optional_choice: if stage == StageV61::Resolving {
                    let pending = state
                        .engine
                        .pending_effect
                        .as_ref()
                        .ok_or("V61 Alesha resolving stage absent")?;
                    match (&pending.choice, pending.frames.as_slice()) {
                        (
                            Some(crate::effect::PendingEffectChoice::ChooseBoolean {
                                purpose:
                                    crate::effect::EffectBooleanChoicePurpose::AleshaRaidReturnV1,
                                ..
                            }),
                            [],
                        ) => AleshaChoiceV61::AwaitingCostlessReturnChoice,
                        (None, [crate::effect::EffectFrame::AleshaRaidAnswerV1 { accept }]) => {
                            AleshaChoiceV61::Answered { accept: *accept }
                        }
                        _ => AleshaChoiceV61::BeforeResolution,
                    }
                } else {
                    AleshaChoiceV61::BeforeResolution
                },
            }
        }
        (
            FamilyV61::NineLives,
            EffectOp::ScheduleNineLivesFamiliarReturnV1 {
                graveyard,
                revival_counters,
            },
        ) => {
            let row = nine_binding(deaths, source, *graveyard, *revival_counters)?;
            ProgramV61::NineLivesDeath {
                departure_revival: row.count,
                return_base_before_replacements: *revival_counters,
            }
        }
        (
            FamilyV61::NineLives,
            EffectOp::ReturnNineLivesFamiliarV1 {
                graveyard,
                revival_counters,
            },
        ) => {
            let row = nine_binding(deaths, source, *graveyard, *revival_counters)?;
            ProgramV61::NineLivesDelayedReturn {
                departure_revival: row.count,
                return_base_before_replacements: *revival_counters,
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(TriggerV61 {
        family,
        stage,
        position: ordinal(position)?,
        source: public::root(state, source, Some(viewer))?,
        program,
        targets: target_facts,
    }))
}
pub fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV61, String> {
    capture_scoped(state, viewer, public::CounterScopeV61::CompleteV61)
}
pub(crate) fn capture_complete_call_v62(
    state: &GameState,
    viewer: PlayerId,
) -> Result<FactsV61, String> {
    capture_scoped(state, viewer, public::CounterScopeV61::CallV62)
}
pub(crate) fn capture_tinybones_compat_v72(
    state: &GameState,
    viewer: PlayerId,
) -> Result<FactsV61, String> {
    capture_scoped(state, viewer, public::CounterScopeV61::TinybonesV72)
}
pub(crate) fn capture_kaito_compat_v75(
    state: &GameState,
    viewer: PlayerId,
) -> Result<FactsV61, String> {
    capture_for_consumer(
        state,
        viewer,
        public::CounterScopeV61::TinybonesV72,
        crate::optional_trigger_public_v1::ConsumerV1::KaitoV75,
    )
}
pub(crate) fn capture_chandra_compat_v74(
    state: &GameState,
    viewer: PlayerId,
) -> Result<FactsV61, String> {
    capture_for_consumer(
        state,
        viewer,
        public::CounterScopeV61::TinybonesV72,
        crate::optional_trigger_public_v1::ConsumerV1::ChandraV74,
    )
}
fn capture_scoped(
    state: &GameState,
    viewer: PlayerId,
    scope: public::CounterScopeV61,
) -> Result<FactsV61, String> {
    capture_for_consumer(
        state,
        viewer,
        scope,
        crate::optional_trigger_public_v1::ConsumerV1::Legacy,
    )
}
fn capture_for_consumer(
    state: &GameState,
    viewer: PlayerId,
    scope: public::CounterScopeV61,
    consumer: crate::optional_trigger_public_v1::ConsumerV1,
) -> Result<FactsV61, String> {
    let call = matches!(
        scope,
        public::CounterScopeV61::CallV62 | public::CounterScopeV61::TinybonesV72
    );
    let ids = FamilyV61::ALL
        .into_iter()
        .map(|f| Ok((f, meaning::selected_id(f)?)))
        .collect::<Result<Vec<_>, String>>()?;
    public::validate_state_for_consumer(state, true, consumer)?;
    if let Some(pending) = &state.engine.pending_cast {
        crate::engine::validate_pending_cast(state, pending)?;
    }
    if let Some(pending) = &state.engine.pending_activation {
        crate::engine::validate_pending_activation(state, pending)?;
    }
    crate::doubling_season_v48::validate_token_journals(state)?;
    let meanings = FamilyV61::ALL
        .into_iter()
        .map(meaning::printed_meaning)
        .collect::<Result<_, _>>()?;
    let mut authorized = vec![];
    let mut current = vec![];
    for (id, live) in state.objects.iter() {
        // Ceased tokens/copies retain arena tombstones, not current zone members.
        if (live.v4.is_token && live.zone != Zone::Battlefield)
            || (live.spell_copy_origin.is_some()
                && live.zone == Zone::Stack
                && !state.stack.iter().any(|item| item.source == id))
        {
            continue;
        }
        let Some(family) = family(live.card_def, &ids) else {
            continue;
        };
        let Some(card) = public::reference(state, id, Some(viewer)).filter(
            |r| matches!(r.identity,public::IdentityV1::Known{card_id}if card_id==live.card_def),
        ) else {
            continue;
        };
        authorized.push(AuthorizedV61 {
            family,
            card: card.clone(),
        });
        if live.zone == Zone::Battlefield {
            let has_pt = crate::engine::effective_base_power(state, id).is_some()
                || crate::engine::effective_base_toughness(state, id).is_some();
            let pt = if has_pt {
                Some(crate::engine::checked_effective_pt(state, id)?)
            } else {
                None
            };
            current.push(CurrentV61 {
                family,
                card,
                printed_abilities_active:
                    crate::continuous_characteristics_v1::printed_abilities_active(state, id),
                power: pt.map(|(power, _)| power),
                toughness: pt.map(|(_, toughness)| toughness),
            });
        }
    }
    let mut battlefield_counters: [Vec<BattlefieldCountersV61>; 2] = [vec![], vec![]];
    let mut seen = BTreeSet::new();
    for seat in [PlayerId::P0, PlayerId::P1] {
        for &id in &state.players[seat.index()].battlefield {
            let live = state
                .objects
                .try_get(id)
                .ok_or("V61 battlefield member absent")?;
            if live.zone != Zone::Battlefield || live.controller != seat || !seen.insert(id.0) {
                return Err("V61 battlefield counter roster membership changed".into());
            }
            battlefield_counters[seat.index()].push(BattlefieldCountersV61 {
                card: public::reference(state, id, Some(viewer))
                    .ok_or("V61 public battlefield reference absent")?,
                counters: counter_values_scoped(state, id, scope)?,
            });
        }
    }
    if state
        .objects
        .iter()
        .any(|(id, live)| live.zone == Zone::Battlefield && !seen.contains(&id.0))
    {
        return Err("V61 battlefield counter roster is incomplete".into());
    }
    let nine = ids
        .iter()
        .find(|(f, _)| *f == FamilyV61::NineLives)
        .ok_or("V61 Nine-Lives printing absent")?
        .1;
    let deaths = death_bindings(state, nine)?;
    let nine_lives_deaths = deaths
        .iter()
        .enumerate()
        .map(|(i, row)| {
            Ok(DeathV61 {
                occurrence: ordinal(i)?,
                source: public::root(state, row.source, Some(viewer))?,
                departure_revival: row.count,
                return_base_before_replacements: row.count - 1,
                collection_completed: row.collected,
                exact_graveyard_card_now: exact_graveyard(state, row.source, row.graveyard, viewer),
            })
        })
        .collect::<Result<_, String>>()?;
    let mut nine_lives_schedules = vec![];
    let mut scheduled = BTreeSet::new();
    for row in state.nine_lives_delayed_v1.iter().flatten() {
        let death = nine_binding(&deaths, row.source, row.graveyard, row.revival_counters)?;
        if !death.collected
            || row.created_after_history_len <= death.capture_index as u64
            || row.created_after_history_len > state.engine.event_history.len() as u64
            || !scheduled.insert((row.source.source.0, row.source.zone_change_count))
        {
            return Err("V61 delayed schedule changed its native creation/death relation".into());
        }
        nine_lives_schedules.push(ScheduleV61 {
            occurrence: ordinal(nine_lives_schedules.len())?,
            source: public::root(state, row.source, Some(viewer))?,
            departure_revival: death.count,
            return_base_before_replacements: row.revival_counters,
            next_future_any_player_end_step: true,
            exact_graveyard_card_now: exact_graveyard(state, row.source, row.graveyard, viewer),
        });
    }
    let mut triggers = vec![];
    for (position, pending) in public::held(state).into_iter().enumerate() {
        if let Some(source) = pending.source_contract {
            if let Some(row) = trigger(
                state,
                source,
                &pending.effect,
                &pending.target_contracts,
                StageV61::Held,
                position,
                viewer,
                &ids,
                &deaths,
            )? {
                triggers.push(row);
            }
        }
    }
    let mut casts = vec![];
    for (position, item) in state.stack.iter().enumerate() {
        // The retained physical row is represented below by its actual resolving
        // stage, not a second simultaneously published family occurrence.
        if state
            .engine
            .pending_effect
            .as_ref()
            .is_some_and(|p| p.resolving_item == *item)
        {
            continue;
        }
        if item.kind == StackItemKind::Spell {
            let Some(family) = family(state.objects.get(item.source).card_def, &ids) else {
                continue;
            };
            casts.push(CastV61 {
                family,
                position: ordinal(position)?,
                source: public::reference(state, item.source, Some(viewer))
                    .ok_or("V61 stack source hidden")?,
                original_controller: item.controller.into(),
                is_uncast_copy: item.is_copy,
                payment_pending: state
                    .engine
                    .pending_cast
                    .as_ref()
                    .is_some_and(|p| p.spell == item.source),
                genuine_cast_finalized: !item.is_copy
                    && item
                        .v4
                        .source_contract
                        .and_then(|s| s.spell_cast_origin)
                        .is_some_and(|o| o.finalized_method.is_some()),
            });
        } else if let (Some(source), Some(op)) =
            (item.v4.ability_source_contract, item.inline_effect.as_ref())
        {
            if let Some(row) = trigger(
                state,
                source,
                op,
                &item.v4.target_contracts,
                StageV61::Published,
                position,
                viewer,
                &ids,
                &deaths,
            )? {
                triggers.push(row);
            }
        }
    }
    if let Some(pending) = &state.engine.pending_effect {
        let item = &pending.resolving_item;
        if let (Some(source), Some(op)) =
            (item.v4.ability_source_contract, item.inline_effect.as_ref())
        {
            if let Some(row) = trigger(
                state,
                source,
                op,
                &item.v4.target_contracts,
                StageV61::Resolving,
                0,
                viewer,
                &ids,
                &deaths,
            )? {
                triggers.push(row);
            }
        }
    }
    let mut alesha_departures = vec![];
    for (receipt_index, event) in state.engine.event_history.iter().enumerate() {
        let E::AleshaPowerLkiV1 {
            source,
            card_def,
            zone_change_count,
            departure_history_index,
            power,
        } = event
        else {
            continue;
        };
        if family(*card_def, &ids) != Some(FamilyV61::Alesha) {
            continue;
        }
        let departure_index = usize::try_from(*departure_history_index)
            .map_err(|_| "V61 departure offset overflow")?;
        let E::ZoneChange {
            object,
            from: Zone::Battlefield,
            to,
            controller_before,
        } = state
            .engine
            .event_history
            .get(departure_index)
            .ok_or("V61 departure row absent")?
        else {
            return Err("V61 departure receipt lost its actual departure".into());
        };
        if object != source
            || *to == Zone::Battlefield
            || crate::engine::alesha_power_receipt_index_v1(state, departure_index)
                != Some(receipt_index)
            || state
                .engine
                .event_history
                .iter()
                .filter(|row| matches!(row, E::AleshaPowerLkiV1 {
                    source: row_source,
                    card_def: row_card_def,
                    zone_change_count: row_generation,
                    ..
                } if row_source == source && row_card_def == card_def && row_generation == zone_change_count))
                .count() != 1
        {
            return Err("V61 departure receipt points at another move".into());
        }
        let live = state
            .objects
            .try_get(*source)
            .ok_or("V61 departure physical source absent")?;
        if live.zone_change_count <= *zone_change_count {
            return Err("V61 departure receipt outlived no source incarnation".into());
        }
        let owner = live.owner;
        let source_root = if call {
            // This typed Call projection reads the adjacent native receipt as
            // the original public Alesha incarnation's LKI. The current
            // incarnation may occupy an unknown hidden slot, whose definition
            // cannot become a historical identity oracle.
            public::RootV1 {
                original_source: public::ReferenceV1 {
                    object: source.0,
                    generation: *zone_change_count,
                    owner: owner.into(),
                    controller: (*controller_before).into(),
                    zone: Zone::Battlefield,
                    identity: public::IdentityV1::Known { card_id: *card_def },
                },
                current_original_source: None,
                captured_controller: (*controller_before).into(),
            }
        } else {
            let contract = AbilitySourceContractV4 {
                source: *source,
                card_def: *card_def,
                owner,
                controller: *controller_before,
                zone: Zone::Battlefield,
                zone_change_count: *zone_change_count,
                attached_to: None,
            };
            if crate::engine::alesha_raid_source_power(state, contract) != Some(*power) {
                return Err("V61 departure receipt changed its exact power binding".into());
            }
            public::root(state, contract, Some(viewer))?
        };
        alesha_departures.push(AleshaDepartureV61 {
            occurrence: ordinal(alesha_departures.len())?,
            source: source_root,
            power: *power,
        });
    }
    Ok(FactsV61 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        viewer: viewer.into(),
        meanings,
        authorized,
        current,
        battlefield_counters,
        casts,
        triggers,
        alesha_departures,
        nine_lives_deaths,
        nine_lives_schedules,
        complete_ordinary_observation: false,
        owning_consumer_admitted: false,
    })
}
pub fn validate(state: &GameState, viewer: PlayerId, value: &FactsV61) -> Result<(), String> {
    if *value == capture(state, viewer)? {
        Ok(())
    } else {
        Err("V61 facts changed from the actual viewer-public state".into())
    }
}
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "graveyard_return_public_v61/tests.rs"]
pub(crate) mod tests;
