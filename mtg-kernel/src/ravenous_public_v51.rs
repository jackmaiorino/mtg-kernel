//! Public soul counts and paid Amulet outcomes. Private worlds and executable
//! history references are consumed internally, never serialized in these DTOs.
use crate::event::CommittedEvent;
use crate::event_history_v1::EventKindV1;
use crate::ravenous_amulet_v1::{OperationV1, PaidV1};
use crate::rl::{CardStableRefV1, PlayerSeatV1, RlContractError};
use crate::rules_meaning_ravenous_amulet_v51::{self as meaning, RootV51};
use crate::state::{AbilitySourceContractV4, GameState, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/ravenous-amulet/public/v51";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncarnationV51 {
    pub object: u32,
    pub generation: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoulCountV51 {
    pub permanent: IncarnationV51,
    pub count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementV51 {
    pub before: i32,
    pub added: i32,
    pub after: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum OutcomeV51 {
    Published,
    Countered,
    Draw {
        /// A draw attempt can fail on an empty library. The card identity is
        /// private hand information and is deliberately absent here.
        drew_card: bool,
        placement: Option<PlacementV51>,
    },
    Loss {
        opponent: PlayerSeatV1,
        amount: i32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationV51 {
    /// Public ordinal in payment order, not an executable history index.
    pub occurrence: usize,
    pub source: CardStableRefV1,
    pub current_original_source: Option<CardStableRefV1>,
    pub original_controller: PlayerSeatV1,
    pub root: RootV51,
    /// The cost object was public. Its private printing identity is not
    /// required here, including when a face-down creature was sacrificed.
    pub sacrificed: IncarnationV51,
    pub self_sacrifice_soul_count: Option<i32>,
    pub outcome: OutcomeV51,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV51 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub meanings: Vec<meaning::MeaningV51>,
    /// Nonzero counts on the public battlefield, including a face-down
    /// permanent. Neither hidden zones nor underlying card names are read.
    pub soul_counts: Vec<SoulCountV51>,
    pub activations: Vec<ActivationV51>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardV51 {
    pub card: crate::rl::CardPublicV2,
    /// Exact values supersede the base schema's narrow placeholder slots.
    /// These fields are mandatory even at zero.
    pub marked_damage: u32,
    pub plus_one_counters: i32,
    pub soul: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BattlefieldV51 {
    pub facts: FactsV51,
    pub cards: [Vec<CardV51>; 2],
}

pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            SCHEMA,
            include_str!("ravenous_public_v51.rs"),
            include_str!("rl.rs"),
            include_str!("effect/thousand_year_storm_v1.rs"),
            include_str!("effect/standard_copy.rs"),
            include_str!("standard_spell_copy.rs"),
            include_str!("event_history_v1.rs"),
        ] {
            hash.update((source.len() as u64).to_le_bytes());
            hash.update(source.as_bytes());
        }
        hash.update(meaning::producer_sha256().as_bytes());
        hash.update(b"legacy-graveyard-return-guard/v61\0");
        hash.update(crate::graveyard_return_public_v61::producer_sha256().as_bytes());
        hash.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    })
    .clone()
}

fn source_ref(state: &GameState, source: AbilitySourceContractV4) -> CardStableRefV1 {
    CardStableRefV1 {
        arena_id: source.source.0,
        card_db_id: crate::rl::projected_captured_card_def(
            state,
            source.source,
            source.zone_change_count,
            source.zone,
            source.card_def,
        ),
        owner: source.owner.into(),
        controller: source.controller.into(),
        zone: source.zone,
        zone_change_count: source.zone_change_count,
    }
}

fn outcome(state: &GameState, paid: &PaidV1) -> Result<OutcomeV51, RlContractError> {
    let removed = state
        .engine
        .event_history
        .scan_for_v1(&[EventKindV1::RavenousRemovedV1])
        .iter()
        .find_map(|event| match event {
            CommittedEvent::RavenousRemovedV1 { removed } if removed.paid == paid.index => {
                Some(removed.as_ref())
            }
            _ => None,
        });
    let Some(removed) = removed else {
        return Ok(OutcomeV51::Published);
    };
    let completed = state
        .engine
        .event_history
        .scan_for_v1(&[EventKindV1::RavenousCompletedV1])
        .iter()
        .find_map(|event| match event {
            CommittedEvent::RavenousCompletedV1 { completed }
                if completed.removed == removed.index =>
            {
                Some(completed.as_ref())
            }
            _ => None,
        })
        .ok_or_else(|| RlContractError("Amulet public completion absent".into()))?;
    let at = |index| {
        usize::try_from(index)
            .ok()
            .and_then(|i| state.engine.event_history.get(i))
    };
    match &completed.operation {
        OperationV1::Countered => Ok(OutcomeV51::Countered),
        OperationV1::Draw { draw, soul } => {
            let Some(CommittedEvent::Draw { object, .. }) = at(*draw) else {
                return Err(RlContractError("Amulet public draw attempt absent".into()));
            };
            let placement = match soul {
                None => None,
                Some((_, added_at)) => {
                    let Some(CommittedEvent::SoulCounterAddedV1 {
                        before,
                        count,
                        after,
                        ..
                    }) = at(*added_at)
                    else {
                        return Err(RlContractError(
                            "Amulet public soul placement absent".into(),
                        ));
                    };
                    Some(PlacementV51 {
                        before: *before,
                        added: *count,
                        after: *after,
                    })
                }
            };
            Ok(OutcomeV51::Draw {
                drew_card: object.is_some(),
                placement,
            })
        }
        OperationV1::Loss { event } => {
            let Some(CommittedEvent::LifeLoss { player, amount }) = at(*event) else {
                return Err(RlContractError("Amulet public loss absent".into()));
            };
            Ok(OutcomeV51::Loss {
                opponent: (*player).into(),
                amount: *amount,
            })
        }
    }
}

pub fn capture(state: &GameState) -> Result<FactsV51, RlContractError> {
    let id = crate::card_def::card_id_by_name("Ravenous Amulet")
        .ok_or_else(|| RlContractError("Amulet public view requires the actual printing".into()))?;
    let meanings = [RootV51::DrawAndSoul, RootV51::SoulLoss]
        .into_iter()
        .map(|root| meaning::printed_meaning(id, root).map_err(RlContractError))
        .collect::<Result<Vec<_>, _>>()?;
    crate::soul_counters_v1::validate(state).map_err(RlContractError)?;
    let mut soul_counts = Vec::new();
    for player in &state.players {
        for object in &player.battlefield {
            let live = state
                .objects
                .try_get(*object)
                .ok_or_else(|| RlContractError("Amulet public battlefield object absent".into()))?;
            if live.zone != Zone::Battlefield || live.counters.soul < 0 {
                return Err(RlContractError(
                    "Amulet public counter roster invalid".into(),
                ));
            }
            if live.counters.soul != 0 {
                soul_counts.push(SoulCountV51 {
                    permanent: IncarnationV51 {
                        object: object.0,
                        generation: live.zone_change_count,
                    },
                    count: live.counters.soul,
                });
            }
        }
    }
    soul_counts.sort_by_key(|entry| (entry.permanent.object, entry.permanent.generation));
    if soul_counts
        .windows(2)
        .any(|pair| pair[0].permanent == pair[1].permanent)
    {
        return Err(RlContractError(
            "Amulet public counter roster duplicated".into(),
        ));
    }
    let mut activations = Vec::new();
    for event in state
        .engine
        .event_history
        .scan_for_v1(&[EventKindV1::RavenousPaymentV1])
    {
        let CommittedEvent::RavenousPaymentV1 { paid } = event else {
            continue;
        };
        let original = paid.original_source;
        let current_original_source = state
            .objects
            .try_get(original.source)
            .filter(|live| {
                live.zone == Zone::Battlefield
                    && live.zone_change_count == original.zone_change_count
            })
            .map(|_| {
                source_ref(
                    state,
                    AbilitySourceContractV4::capture(state, original.source),
                )
            });
        let root = match paid.item.v4.activated_ability_index {
            Some(0) => RootV51::DrawAndSoul,
            Some(1) => RootV51::SoulLoss,
            _ => return Err(RlContractError("Amulet public paid root invalid".into())),
        };
        activations.push(ActivationV51 {
            occurrence: activations.len(),
            source: source_ref(state, original),
            current_original_source,
            original_controller: paid.item.controller.into(),
            root,
            sacrificed: IncarnationV51 {
                object: paid.original_sacrifice.source.0,
                generation: paid.original_sacrifice.zone_change_count,
            },
            self_sacrifice_soul_count: paid.original_soul_count,
            outcome: outcome(state, paid)?,
        });
    }
    Ok(FactsV51 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        meanings,
        soul_counts,
        activations,
    })
}

pub fn validate(state: &GameState, value: &FactsV51) -> Result<(), RlContractError> {
    if capture(state)? == *value {
        Ok(())
    } else {
        Err(RlContractError(
            "Amulet public facts differ from current state".into(),
        ))
    }
}

pub fn battlefield(state: &GameState) -> Result<BattlefieldV51, RlContractError> {
    let facts = capture(state)?;
    let mut cards: [Vec<CardV51>; 2] = [Vec::new(), Vec::new()];
    let mut seen = Vec::new();
    for (seat, player) in state.players.iter().enumerate() {
        for id in &player.battlefield {
            let live = state
                .objects
                .try_get(*id)
                .ok_or_else(|| RlContractError("Amulet public battlefield object absent".into()))?;
            if seen.contains(id)
                || live.zone != Zone::Battlefield
                || live.controller.index() != seat
                || live.counters.plus1_plus1 < 0
                || live.counters.soul < 0
            {
                return Err(RlContractError(
                    "Amulet public battlefield roster/count invalid".into(),
                ));
            }
            seen.push(*id);
            cards[seat].push(CardV51 {
                card: crate::rl::public_card_v51_base(state, *id)?,
                marked_damage: live.damage,
                plus_one_counters: live.counters.plus1_plus1,
                soul: live.counters.soul,
            });
        }
    }
    Ok(BattlefieldV51 { facts, cards })
}

pub(crate) fn battlefield_prepared_v57(
    state: &GameState,
) -> Result<BattlefieldV51, RlContractError> {
    battlefield_complete_scope(state, crate::rl::CommonProjectionScopeV42::PreparedV57)
}
pub(crate) fn battlefield_complete_scope(
    state: &GameState,
    scope: crate::rl::CommonProjectionScopeV42,
) -> Result<BattlefieldV51, RlContractError> {
    if !matches!(
        scope,
        crate::rl::CommonProjectionScopeV42::PreparedV57
            | crate::rl::CommonProjectionScopeV42::NextFiveV59
            | crate::rl::CommonProjectionScopeV42::GraveyardReturnV61
            | crate::rl::CommonProjectionScopeV42::CallV62
            | crate::rl::CommonProjectionScopeV42::EndTurnV68
            | crate::rl::CommonProjectionScopeV42::GenesisWaveV69
            | crate::rl::CommonProjectionScopeV42::VivienV70
            | crate::rl::CommonProjectionScopeV42::MuldrothaV71
    ) {
        return Err(RlContractError(
            "wrong complete counter compatibility scope".into(),
        ));
    }
    let facts = capture(state)?;
    let mut cards: [Vec<CardV51>; 2] = [Vec::new(), Vec::new()];
    let mut seen = Vec::new();
    for (seat, player) in state.players.iter().enumerate() {
        for id in &player.battlefield {
            let live = state
                .objects
                .try_get(*id)
                .ok_or_else(|| RlContractError("Amulet public battlefield object absent".into()))?;
            if seen.contains(id)
                || live.zone != Zone::Battlefield
                || live.controller.index() != seat
                || live.counters.plus1_plus1 < 0
                || live.counters.soul < 0
            {
                return Err(RlContractError(
                    "Amulet public battlefield roster/count invalid".into(),
                ));
            }
            seen.push(*id);
            cards[seat].push(CardV51 {
                card: if matches!(
                    scope,
                    crate::rl::CommonProjectionScopeV42::CallV62
                        | crate::rl::CommonProjectionScopeV42::EndTurnV68
                        | crate::rl::CommonProjectionScopeV42::GenesisWaveV69
                        | crate::rl::CommonProjectionScopeV42::VivienV70
                        | crate::rl::CommonProjectionScopeV42::MuldrothaV71
                ) {
                    crate::rl::public_card_call_compat_v62(state, *id)?
                } else if scope == crate::rl::CommonProjectionScopeV42::GraveyardReturnV61 {
                    crate::rl::public_card_graveyard_return_compat_v61(state, *id)?
                } else {
                    crate::rl::public_card_prepared_compat_v57(state, *id)?
                },
                marked_damage: live.damage,
                plus_one_counters: live.counters.plus1_plus1,
                soul: live.counters.soul,
            });
        }
    }
    Ok(BattlefieldV51 { facts, cards })
}

pub fn validate_battlefield(
    state: &GameState,
    value: &BattlefieldV51,
) -> Result<(), RlContractError> {
    if battlefield(state)? == *value {
        Ok(())
    } else {
        Err(RlContractError(
            "Amulet public battlefield differs from current state".into(),
        ))
    }
}
