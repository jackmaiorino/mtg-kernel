//! In-process owner for the exact frozen Full English Breakfast mainboard.
use super::{
    decision::{bind_next, BoundBoundaryV1},
    FullTextProgramV1, NumericDecisionViewV1, NumericMenuV1, NumericNativeActionV1,
};
use crate::{
    card_def::{card_id_by_name, CardCapability, CARD_DEFS, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::{visible_numeric_parts_v1, NumericTree},
    event::{propose_and_commit, ProposedEvent},
    full_text_v1,
    ids::{ObjectId, PlayerId},
    limited_session_v1::CustomCardCountV1,
    rl::{self, ActionSemanticV1, PlayerSeatV1},
    state::{GameState, SplitMix64},
    surface_v2::{HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::{Arc, OnceLock};

#[path = "feb_human.rs"]
mod human;
#[cfg(test)]
#[path = "feb_tests.rs"]
mod tests;
pub use human::{
    FebHumanActionV1, FebHumanOrderV1, FebHumanOrderedV1, FebHumanSceneV1, FebHumanViewV1,
};

pub const PROFILE_V1: &str = "mtg-kernel-premodern-feb-ordered/v1";
const ARCHETYPE: &str = "premodern-full-english-breakfast";
const SNAPSHOT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../data/eternal_v1/meta_snapshot_mtgtop8_premodern_20261101.json"
));
const SNAPSHOT_SHA256: &str = "9e9db030875447bf7874415d63d5bc686e86952abf5ab82f1c8c45ff54ab0fe3";

#[derive(Deserialize)]
struct Snapshot {
    formats: Formats,
}

#[derive(Deserialize)]
struct Formats {
    premodern: Vec<Archetype>,
}

#[derive(Deserialize)]
struct Archetype {
    archetype: String,
    mainboard: Vec<(u32, String)>,
    companion: Option<String>,
}

struct Mainboard {
    rows: Vec<CustomCardCountV1>,
    definitions: [u16; 60],
}

fn mainboard() -> Result<&'static Mainboard, String> {
    static MAINBOARD: OnceLock<Result<Mainboard, String>> = OnceLock::new();
    MAINBOARD
        .get_or_init(|| {
            if format!("{:x}", Sha256::digest(SNAPSHOT)) != SNAPSHOT_SHA256 {
                return Err("frozen FEB snapshot bytes changed".into());
            }
            let snapshot: Snapshot = serde_json::from_slice(SNAPSHOT).map_err(|e| e.to_string())?;
            let mut matching = snapshot
                .formats
                .premodern
                .into_iter()
                .filter(|row| row.archetype == ARCHETYPE);
            let row = matching.next().ok_or("frozen FEB mainboard is absent")?;
            if matching.next().is_some() || row.companion.is_some() || row.mainboard.len() != 27 {
                return Err("invalid frozen FEB mainboard declaration".into());
            }
            let mut definitions = Vec::with_capacity(60);
            let mut rows = Vec::with_capacity(row.mainboard.len());
            for (count, name) in row.mainboard {
                let id = card_id_by_name(&name)
                    .ok_or_else(|| format!("unknown FEB printing {name:?}"))?;
                let card = &CARD_DEFS[usize::from(id)];
                let volrath = name == "Volrath's Shapeshifter";
                if card.name != name
                    || card.is_token
                    || count == 0
                    || count > 4
                    || if volrath {
                        card.capability != CardCapability::Partial || count != 2
                    } else {
                        !card.has_full_support()
                    }
                {
                    return Err(format!("unsupported frozen FEB printing {name:?}"));
                }
                definitions.extend(std::iter::repeat_n(id, count as usize));
                rows.push(CustomCardCountV1 { name, count });
            }
            let definitions = definitions
                .try_into()
                .map_err(|_| "frozen FEB mainboard is not sixty cards")?;
            Ok(Mainboard { rows, definitions })
        })
        .as_ref()
        .map_err(Clone::clone)
}

pub fn frozen_mainboard_rows_v1() -> Result<&'static [CustomCardCountV1], String> {
    Ok(&mainboard()?.rows)
}

pub fn resolve_frozen_mainboard_v1() -> Result<&'static [u16; 60], String> {
    Ok(&mainboard()?.definitions)
}

fn native_seat(seat: PlayerSeatV1) -> PlayerId {
    match seat {
        PlayerSeatV1::P0 => PlayerId::P0,
        PlayerSeatV1::P1 => PlayerId::P1,
    }
}

#[derive(Clone)]
struct OwnedState {
    game: GameState,
    surface: HarnessSurfaceV2,
    step: u64,
    current: Result<BoundBoundaryV1, String>,
}

/// Owner of one exact FEB mirror. Its raw state and binding are private.
pub struct FebSessionV1 {
    inner: OwnedState,
    owner: Arc<()>,
    revision: u64,
}

/// In-process same-owner snapshot. It cannot be deserialized or inspect a world.
pub struct FebSnapshotV1 {
    inner: OwnedState,
    owner: Arc<()>,
}

/// Opaque authority for one actor at one revision, independent of the view.
pub struct FebDecisionV1 {
    owner: Arc<()>,
    revision: u64,
    actor: PlayerId,
}

pub struct FebNumericViewV1 {
    pub profile: &'static str,
    pub card_db_hash: u64,
    pub step: u64,
    pub actor: PlayerSeatV1,
    pub observation: NumericTree,
    /// In native ordinal order. Empty for a viewer who cannot act.
    pub actions: Vec<NumericTree>,
}

pub enum FebCurrentV1 {
    Decision {
        numeric: Box<FebNumericViewV1>,
        decision: Option<FebDecisionV1>,
    },
    GameOver {
        winner: Option<PlayerSeatV1>,
    },
    NativeHalt,
    UnsupportedDecision,
}

fn references<'a>(value: &'a Value, found: &mut Vec<&'a Value>) {
    match value {
        Value::Object(fields) if fields.contains_key("arena_id") => found.push(value),
        Value::Object(fields) => fields.values().for_each(|value| references(value, found)),
        Value::Array(rows) => rows.iter().for_each(|value| references(value, found)),
        _ => {}
    }
}

fn require_visible_actions(observed: &Value, actions: &[Value]) -> Result<(), String> {
    let mut visible = Vec::new();
    references(observed, &mut visible);
    let mut operands = Vec::new();
    for action in actions {
        references(action, &mut operands);
    }
    if operands.iter().any(|operand| !visible.contains(operand)) {
        return Err("FEB native action references a card outside this viewer's observation".into());
    }
    Ok(())
}

fn full_text_action(row: &NumericNativeActionV1) -> Option<FullTextProgramV1> {
    row.full_text.or_else(|| {
        let ActionSemanticV1::ActivateAbility {
            source,
            ability_index: 0,
            granted_ability,
            ..
        } = &row.action.semantic
        else {
            return None;
        };
        (source.zone == crate::state::Zone::Battlefield
            && granted_ability.as_ref().is_none()
            && full_text_v1::owns_definition(&CARD_DEFS[usize::from(source.card_db_id)]))
        .then_some(FullTextProgramV1::RetainedDiscard)
    })
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum DonatedManaV1 {
    BirdsOfParadise,
    WallOfRoots,
}

fn donated_mana(state: &GameState, action: &ActionSemanticV1) -> Option<DonatedManaV1> {
    let ActionSemanticV1::ActivateManaAbility { source, .. } = action else {
        return None;
    };
    let text = full_text_v1::mana_text(state, ObjectId(source.arena_id))?;
    Some(match text.definition().name {
        "Birds of Paradise" => DonatedManaV1::BirdsOfParadise,
        "Wall of Roots" => DonatedManaV1::WallOfRoots,
        _ => return None,
    })
}

fn visible_numeric_input(
    state: &GameState,
    view: &NumericDecisionViewV1,
) -> Result<(Value, Vec<Value>), String> {
    let actions = match &view.menu {
        None => Vec::new(),
        Some(NumericMenuV1::Native { actions }) => actions.iter().map(|row| {
            json!({"ordinal":row.action.selected_index, "action":row.action.semantic,
                "full_text":full_text_action(row), "donated_mana":donated_mana(state, &row.action.semantic)})
        }).collect::<Vec<Value>>(),
        Some(NumericMenuV1::GraveyardOrder) => view.observation.ordered.order_choice.as_ref()
            .ok_or("FEB native order choice is absent")?.native_candidates.iter().enumerate()
            .map(|(ordinal, card)| json!({"ordinal":ordinal, "order_card":card})).collect(),
    };
    let observed =
        serde_json::to_value(&view.observation).map_err(|_| "FEB numeric serialization failed")?;
    require_visible_actions(&observed, &actions)?;
    Ok((observed, actions))
}

impl FebSessionV1 {
    /// Construct and deal both exact 60-card mainboards, then issue the first
    /// London mulligan decision in native starting-seat order.
    pub fn reset(seed: u64, starting_player: PlayerSeatV1) -> Result<Self, String> {
        let deck = resolve_frozen_mainboard_v1()?;
        let mut shuffle = SplitMix64::seed(seed);
        let libraries = [
            rl::shuffled(deck, &mut shuffle),
            rl::shuffled(deck, &mut shuffle),
        ];
        let mut game = GameState::new_from_libraries_with_graveyard_order_v1(
            &libraries[0],
            &libraries[1],
            rl::card_name,
            seed,
            native_seat(starting_player),
        );
        for _ in 0..7 {
            for player in [PlayerId::P0, PlayerId::P1] {
                propose_and_commit(&mut game, ProposedEvent::draw(player));
            }
        }
        crate::london_mulligan_v1::enable_london_mulligans_v1(&mut game)?;
        let mut surface = HarnessSurfaceV2::new_with_priority_mode_v1(
            PriorityModeV1::EngineWindowsV1,
            SuppressionAuditMode::Off,
        );
        let current = bind_next(&mut game, &mut surface, 0);
        Ok(Self {
            inner: OwnedState {
                game,
                surface,
                step: 0,
                current,
            },
            owner: Arc::new(()),
            revision: 0,
        })
    }

    pub fn current(&self, viewer: PlayerSeatV1) -> Result<FebCurrentV1, String> {
        let viewer = native_seat(viewer);
        let binding = match &self.inner.current {
            Ok(BoundBoundaryV1::Decision(binding)) => binding,
            Ok(BoundBoundaryV1::GameOver { winner }) => {
                return Ok(FebCurrentV1::GameOver { winner: *winner })
            }
            Ok(BoundBoundaryV1::Halted) => return Ok(FebCurrentV1::NativeHalt),
            Err(_) => return Ok(FebCurrentV1::UnsupportedDecision),
        };
        let state = binding
            .view(
                &self.inner.game,
                &self.inner.surface,
                viewer,
                self.inner.step,
            )
            .map_err(|_| "FEB ordered observation is unsupported")?;
        let (observed, actions) = visible_numeric_input(&self.inner.game, &state)?;
        let (observation, actions) = visible_numeric_parts_v1(&observed, &actions)
            .map_err(|error| format!("FEB numeric operand: {error}"))?;
        Ok(FebCurrentV1::Decision {
            numeric: Box::new(FebNumericViewV1 {
                profile: PROFILE_V1,
                card_db_hash: KERNEL_CARDDB_HASH,
                step: self.inner.step,
                actor: binding.actor.into(),
                observation,
                actions,
            }),
            decision: (viewer == binding.actor).then(|| FebDecisionV1 {
                owner: self.owner.clone(),
                revision: self.revision,
                actor: viewer,
            }),
        })
    }

    pub fn step(&mut self, decision: &FebDecisionV1, native_ordinal: usize) -> Result<(), String> {
        let Ok(BoundBoundaryV1::Decision(binding)) = &self.inner.current else {
            return Err("FEB session has no current decision".into());
        };
        if !Arc::ptr_eq(&self.owner, &decision.owner)
            || self.revision != decision.revision
            || binding.actor != decision.actor
        {
            return Err("FEB decision is foreign or stale".into());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or("FEB revision exhausted")?;
        let step = self.inner.step.checked_add(1).ok_or("FEB step exhausted")?;
        binding
            .choose(
                &mut self.inner.game,
                &mut self.inner.surface,
                self.inner.step,
                native_ordinal,
            )
            .map_err(|_| "FEB native action was refused")?;
        self.inner.step = step;
        self.inner.current = bind_next(&mut self.inner.game, &mut self.inner.surface, step);
        self.revision = revision;
        Ok(())
    }

    pub fn snapshot(&self) -> FebSnapshotV1 {
        FebSnapshotV1 {
            inner: self.inner.clone(),
            owner: self.owner.clone(),
        }
    }

    pub fn restore(&mut self, snapshot: &FebSnapshotV1) -> Result<(), String> {
        if !Arc::ptr_eq(&self.owner, &snapshot.owner) {
            return Err("FEB snapshot belongs to a different owner".into());
        }
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or("FEB revision exhausted")?;
        self.inner = snapshot.inner.clone();
        Ok(())
    }
}
