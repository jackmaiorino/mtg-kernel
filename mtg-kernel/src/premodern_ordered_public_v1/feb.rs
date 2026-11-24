//! In-process owner for the frozen Full English Breakfast mirror and the
//! widened registered Premodern ordered-graveyard profile.
#[cfg(test)]
use super::decision::bind_next;
use super::{
    decision::{bind_next_for_profile, BoundBoundaryV1, BoundDecisionV1, OrderedProfileV1},
    FullTextProgramV1, NumericDecisionViewV1, NumericMenuV1, NumericNativeActionV1,
    NumericRegisteredDecisionViewV1, ObservationV1,
};
use crate::{
    card_def::{card_id_by_name, CardCapability, CARD_DEFS, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::{visible_numeric_parts_v1, NumericTree},
    engine::UnsupportedMechanic,
    event::{propose_and_commit, ProposedEvent},
    full_text_v1,
    ids::{ObjectId, PlayerId},
    limited_session_v1::CustomCardCountV1,
    rl::{self, ActionSemanticV1, CardStableRefV1, PlayerSeatV1},
    state::{GameState, SplitMix64},
    surface_v2::{HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    sync::{Arc, OnceLock},
};

#[path = "feb_human.rs"]
mod human;
#[cfg(test)]
#[path = "feb_tests.rs"]
mod tests;
pub use human::{
    FebHumanActionV1, FebHumanOrderV1, FebHumanOrderedV1, FebHumanSceneV1, FebHumanViewV1,
};

pub const PROFILE_V1: &str = "mtg-kernel-premodern-feb-ordered/v1";
pub const REGISTERED_PROFILE_V1: &str = "mtg-kernel-premodern-ordered-registered/v1";
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
    #[serde(default)]
    sideboard: Vec<(u32, String)>,
    companion: Option<String>,
}

struct Mainboard {
    rows: Vec<CustomCardCountV1>,
    definitions: [u16; 60],
}

fn load_premodern_snapshot() -> Result<Snapshot, String> {
    if format!("{:x}", Sha256::digest(SNAPSHOT)) != SNAPSHOT_SHA256 {
        return Err("frozen FEB snapshot bytes changed".into());
    }
    serde_json::from_slice(SNAPSHOT).map_err(|e| e.to_string())
}

pub(crate) fn premodern_pool_names_v1() -> Result<&'static BTreeSet<String>, String> {
    static POOL: OnceLock<Result<BTreeSet<String>, String>> = OnceLock::new();
    POOL.get_or_init(|| {
        let snapshot = load_premodern_snapshot()?;
        let mut names = BTreeSet::new();
        for row in snapshot.formats.premodern {
            for (_, name) in row.mainboard.into_iter().chain(row.sideboard) {
                names.insert(name);
            }
            if let Some(companion) = row.companion {
                names.insert(companion);
            }
        }
        Ok(names)
    })
    .as_ref()
    .map_err(Clone::clone)
}

pub(crate) fn frozen_feb_75_ids_v1() -> Result<(Vec<u16>, Vec<u16>), String> {
    let mainboard = resolve_frozen_mainboard_v1()?.to_vec();
    let snapshot = load_premodern_snapshot()?;
    let mut matching = snapshot
        .formats
        .premodern
        .into_iter()
        .filter(|row| row.archetype == ARCHETYPE);
    let row = matching.next().ok_or("frozen FEB 75 is absent")?;
    if matching.next().is_some() || row.companion.is_some() {
        return Err("invalid frozen FEB 75 declaration".into());
    }
    let mut sideboard = Vec::with_capacity(15);
    for (count, name) in row.sideboard {
        let id = card_id_by_name(&name)
            .ok_or_else(|| format!("unknown FEB sideboard printing {name:?}"))?;
        let card = &CARD_DEFS[usize::from(id)];
        if card.name != name || card.is_token || count == 0 || count > 4 || !card.has_full_support()
        {
            return Err(format!(
                "unsupported frozen FEB sideboard printing {name:?}"
            ));
        }
        sideboard.extend(std::iter::repeat_n(id, count as usize));
    }
    if sideboard.len() != 15 {
        return Err("frozen FEB sideboard is not fifteen cards".into());
    }
    Ok((mainboard, sideboard))
}

fn mainboard() -> Result<&'static Mainboard, String> {
    static MAINBOARD: OnceLock<Result<Mainboard, String>> = OnceLock::new();
    MAINBOARD
        .get_or_init(|| {
            let snapshot = load_premodern_snapshot()?;
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

/// Owner of one exact FEB mirror or one registered Premodern ordered-graveyard
/// game. Its raw state and binding are private.
pub struct FebSessionV1 {
    inner: OwnedState,
    profile: OrderedProfileV1,
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
    RegisteredHaltV1 {
        mechanic: UnsupportedMechanic,
        source: Option<CardStableRefV1>,
    },
}

enum ProfileDecisionViewV1 {
    Frozen(Box<NumericDecisionViewV1>),
    Registered(Box<NumericRegisteredDecisionViewV1>),
}

impl ProfileDecisionViewV1 {
    fn ordered(&self) -> &ObservationV1 {
        match self {
            Self::Frozen(view) => &view.observation.ordered,
            Self::Registered(view) => &view.observation.ordered,
        }
    }

    fn menu(&self) -> Option<&NumericMenuV1> {
        match self {
            Self::Frozen(view) => view.menu.as_ref(),
            Self::Registered(view) => view.menu.as_ref(),
        }
    }

    fn menu_mut(&mut self) -> Option<&mut NumericMenuV1> {
        match self {
            Self::Frozen(view) => view.menu.as_mut(),
            Self::Registered(view) => view.menu.as_mut(),
        }
    }

    fn observation_value(&self) -> Result<Value, String> {
        match self {
            Self::Frozen(view) => serde_json::to_value(&view.observation),
            Self::Registered(view) => serde_json::to_value(&view.observation),
        }
        .map_err(|_| "FEB numeric serialization failed".into())
    }
}

fn public_halt_source(state: &GameState, id: ObjectId) -> Option<CardStableRefV1> {
    use crate::state::Zone;
    let object = state.objects.try_get(id)?;
    let is_public = match object.zone {
        Zone::Battlefield => state.players[object.controller.index()]
            .battlefield
            .contains(&id),
        Zone::Graveyard => state.players[object.owner.index()].graveyard.contains(&id),
        Zone::Exile => state.exile.contains(&id),
        Zone::Stack => state.stack.iter().any(|item| item.source == id),
        Zone::Command => state.command.contains(&id),
        Zone::Hand | Zone::Library => false,
    };
    is_public.then(|| rl::card_ref(state, id).ok()).flatten()
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
    view: &ProfileDecisionViewV1,
) -> Result<(Value, Vec<Value>), String> {
    let actions = match view.menu() {
        None => Vec::new(),
        Some(NumericMenuV1::Native { actions }) => actions.iter().map(|row| {
            json!({"ordinal":row.action.selected_index, "action":row.action.semantic,
                "full_text":full_text_action(row), "donated_mana":donated_mana(state, &row.action.semantic)})
        }).collect::<Vec<Value>>(),
        Some(NumericMenuV1::GraveyardOrder) => view.ordered().order_choice.as_ref()
            .ok_or("FEB native order choice is absent")?.native_candidates.iter().enumerate()
            .map(|(ordinal, card)| json!({"ordinal":ordinal, "order_card":card})).collect(),
    };
    let observed = view.observation_value()?;
    require_visible_actions(&observed, &actions)?;
    Ok((observed, actions))
}

impl FebSessionV1 {
    pub fn profile(&self) -> &'static str {
        match self.profile {
            OrderedProfileV1::Frozen => PROFILE_V1,
            OrderedProfileV1::Registered => REGISTERED_PROFILE_V1,
        }
    }

    /// Final state hash once the game reaches a natural `GameOver` terminal.
    /// Absent while the game is live so hidden information never crosses out.
    pub fn terminal_state_hash_v1(&self) -> Option<u64> {
        matches!(self.inner.current, Ok(BoundBoundaryV1::GameOver { .. }))
            .then(|| self.inner.game.state_hash())
    }

    pub(super) fn actor(&self) -> Option<PlayerSeatV1> {
        match &self.inner.current {
            Ok(BoundBoundaryV1::Decision(binding)) => Some(binding.actor.into()),
            _ => None,
        }
    }

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
        let profile = OrderedProfileV1::Frozen;
        let current = bind_next_for_profile(&mut game, &mut surface, 0, profile);
        Ok(Self {
            inner: OwnedState {
                game,
                surface,
                step: 0,
                current,
            },
            profile,
            owner: Arc::new(()),
            revision: 0,
        })
    }

    /// Construct and deal two validated Premodern configurations under the
    /// registered ordered-graveyard profile (Foundations combat, London
    /// mulligans, public shroud, and sideboards outside the game).
    pub(crate) fn reset_registered_v1(
        mainboards: [&[u16]; 2],
        outside_the_game: [Vec<u16>; 2],
        seed: u64,
        starting_player: PlayerSeatV1,
    ) -> Result<Self, String> {
        let mut shuffle = SplitMix64::seed(seed);
        let libraries = [
            rl::shuffled(mainboards[0], &mut shuffle),
            rl::shuffled(mainboards[1], &mut shuffle),
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
        crate::combat_damage_v1::enable_foundations_combat_v1(&mut game)?;
        crate::london_mulligan_v1::enable_london_mulligans_v1(&mut game)?;
        rl::enable_public_shroud_v1(&mut game);
        crate::eternal_outside_game_v1::set_outside_the_game(&mut game, outside_the_game);
        let mut surface = HarnessSurfaceV2::new_with_priority_mode_v1(
            PriorityModeV1::EngineWindowsV1,
            SuppressionAuditMode::Off,
        );
        let profile = OrderedProfileV1::Registered;
        let current = bind_next_for_profile(&mut game, &mut surface, 0, profile);
        Ok(Self {
            inner: OwnedState {
                game,
                surface,
                step: 0,
                current,
            },
            profile,
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
            Ok(BoundBoundaryV1::Halted) => {
                return Ok(match self.profile {
                    OrderedProfileV1::Frozen => FebCurrentV1::NativeHalt,
                    OrderedProfileV1::Registered => match self.inner.game.engine.halted {
                        Some((mechanic, source_id)) => FebCurrentV1::RegisteredHaltV1 {
                            mechanic,
                            source: public_halt_source(&self.inner.game, source_id),
                        },
                        None => FebCurrentV1::NativeHalt,
                    },
                });
            }
            Err(_) => return Ok(FebCurrentV1::UnsupportedDecision),
        };
        let (state, _) = self
            .profile_decision_view(binding, viewer)
            .map_err(|_| "FEB ordered observation is unsupported")?;
        let (observed, actions) = visible_numeric_input(&self.inner.game, &state)?;
        let (observation, actions) = visible_numeric_parts_v1(observed, &actions)
            .map_err(|error| format!("FEB numeric operand: {error}"))?;
        Ok(FebCurrentV1::Decision {
            numeric: Box::new(FebNumericViewV1 {
                profile: self.profile(),
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

    /// Typed registered view (V5 public projection and native action menu for
    /// the acting seat) for in-process registered ordered consumers.
    pub fn registered_decision_view_v1(
        &self,
        viewer: PlayerSeatV1,
    ) -> Result<Option<NumericRegisteredDecisionViewV1>, String> {
        if self.profile != OrderedProfileV1::Registered {
            return Err("FEB session does not use the registered ordered profile".into());
        }
        let Ok(BoundBoundaryV1::Decision(binding)) = &self.inner.current else {
            return Ok(None);
        };
        let (ProfileDecisionViewV1::Registered(view), _) =
            self.profile_decision_view(binding, native_seat(viewer))?
        else {
            unreachable!("registered profile builds registered decision views");
        };
        Ok(Some(*view))
    }

    #[cfg(test)]
    fn decision_view(
        &self,
        binding: &BoundDecisionV1,
        viewer: PlayerId,
    ) -> Result<(NumericDecisionViewV1, Option<human::NativeEffectOptionMenu>), String> {
        let (ProfileDecisionViewV1::Frozen(view), echo) =
            self.profile_decision_view(binding, viewer)?
        else {
            return Err("expected frozen FEB decision view".into());
        };
        Ok((*view, echo))
    }

    fn profile_decision_view(
        &self,
        binding: &BoundDecisionV1,
        viewer: PlayerId,
    ) -> Result<(ProfileDecisionViewV1, Option<human::NativeEffectOptionMenu>), String> {
        let state = &self.inner.game;
        let mut view = match self.profile {
            OrderedProfileV1::Frozen => ProfileDecisionViewV1::Frozen(Box::new(binding.view(
                state,
                &self.inner.surface,
                viewer,
                self.inner.step,
            )?)),
            OrderedProfileV1::Registered => ProfileDecisionViewV1::Registered(Box::new(
                binding.view_registered(state, &self.inner.surface, viewer, self.inner.step)?,
            )),
        };
        // The echo decision names its original public permanent. Only change
        // the presentation copy, keeping the bound native actions intact.
        let echo = match view.menu_mut() {
            Some(NumericMenuV1::Native { actions }) => human::guide_echo_menu(state, viewer)
                .inspect(|echo| echo.bind_guide_echo_source(actions)),
            _ => None,
        };
        Ok((view, echo))
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
        self.inner.current = bind_next_for_profile(
            &mut self.inner.game,
            &mut self.inner.surface,
            step,
            self.profile,
        );
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
