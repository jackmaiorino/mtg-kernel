//! An immediate linked-Aura return borrows the genuine operation which caused
//! departure. Only the checkpoint at the owner choice is published. Resuming
//! deterministically reruns that operation with the already confirmed answers;
//! speculative tails are never installed in the game or its committed journal.
use crate::effect::EffectObjectBinding;
use crate::engine::{Action, Decision};
use crate::eternal_sideboard_s1b_v1::meddling_mage_entry::{self as mage_entry, show_and_tell};
use crate::event::{CommittedEvent, ProposedEvent};
use crate::linked_aura_return_v1::HostChoiceV1;
use crate::state::{GameState, Target, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationV1 {
    Step(Action),
    Advance,
    Event(ProposedEvent),
    Batch(Vec<ProposedEvent>),
    Commit(ProposedEvent),
    Effect {
        op: crate::effect::EffectOp,
        ctx: Box<crate::effect::ExecCtx>,
    },
    CopyTokenV36 {
        source: crate::state::AbilitySourceContractV4,
        controller: crate::ids::PlayerId,
    },
    DrakeCombatBatchV1 {
        events: Vec<ProposedEvent>,
        legacy_normal_after: bool,
    },
    DrakeCounterStackV1 {
        stack_item_id: crate::ids::StackItemId,
    },
    CopyTokenKinshipV1 {
        source: crate::state::AbilitySourceContractV4,
        controller: crate::ids::PlayerId,
    },
}

impl Hash for OperationV1 {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        serde_json::to_vec(self)
            .expect("typed linked operation serializes")
            .hash(hasher);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerV1 {
    pub choice: HostChoiceV1,
    pub host: EffectObjectBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointV1 {
    pub original: Box<GameState>,
    pub operation: OperationV1,
    pub answers: Vec<AnswerV1>,
    pub choice: HostChoiceV1,
    pub capture_history_index: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GeneralAnswerV36 {
    Host(AnswerV1),
    Color {
        choice: Box<crate::heraldic_banner_v1::ChoiceV1>,
        color: crate::mana::ManaColor,
    },
    TokenEquipmentV48 {
        choice: Box<crate::season_token_attachment_v48::ChoiceV48>,
        selected: EffectObjectBinding,
    },
    FakeAuraV50 {
        choice: Box<crate::fake_your_own_death_v50::return_aura::ChoiceV50>,
        selected: EffectObjectBinding,
    },
    KinshipV1 {
        choice: Box<crate::banner_of_kinship_v1::entry::ChoiceV1>,
        chosen: crate::creature_types_v60::CreatureTypeV60,
    },
    MeddlingMageV1 {
        choice: Box<mage_entry::ChoiceV1>,
        group: Option<u16>,
        index: u16,
    },
    ReanimateMetamorphV1 {
        choice: Box<mage_entry::NoncastCopyChoiceV1>,
        copied: Option<crate::state::AbilitySourceContractV4>,
    },
    VialMetamorphV1 {
        choice: Box<mage_entry::NoncastCopyChoiceV1>,
        copied: Option<crate::state::AbilitySourceContractV4>,
    },
    ShowAndTellV1 {
        choice: Box<show_and_tell::ChoiceV1>,
        selected: Option<crate::state::AbilitySourceContractV4>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GeneralChoiceV36 {
    Host {
        choice: HostChoiceV1,
        capture: u64,
    },
    Color {
        choice: Box<crate::heraldic_banner_v1::ChoiceV1>,
        capture: u32,
    },
    TokenEquipmentV48 {
        choice: Box<crate::season_token_attachment_v48::ChoiceV48>,
        input: u64,
    },
    FakeAuraV50 {
        choice: Box<crate::fake_your_own_death_v50::return_aura::ChoiceV50>,
        input: u64,
    },
    KinshipV1 {
        choice: Box<crate::banner_of_kinship_v1::entry::ChoiceV1>,
        capture: u32,
    },
    MeddlingMageV1 {
        choice: Box<mage_entry::ChoiceV1>,
        group: Option<u16>,
    },
    ReanimateMetamorphV1 {
        choice: Box<mage_entry::NoncastCopyChoiceV1>,
        original_sha256: String,
    },
    VialMetamorphV1 {
        choice: Box<mage_entry::NoncastCopyChoiceV1>,
        original_sha256: String,
    },
    ShowAndTellV1 {
        choice: Box<show_and_tell::ChoiceV1>,
        original_sha256: String,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralCheckpointV36 {
    pub original: Box<GameState>,
    pub operation: OperationV1,
    pub answers: Vec<GeneralAnswerV36>,
    pub choice: GeneralChoiceV36,
    pub operation_history_index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) genesis_current_slots_v69: Option<genesis_current_slots_v69::InputV69>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ColorPlanV36 {
    choice: crate::heraldic_banner_v1::ChoiceV1,
    capture: u32,
    color: Option<crate::mana::ManaColor>,
    answer: Option<u32>,
    committed: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct KinshipPlanV1 {
    choice: crate::banner_of_kinship_v1::entry::ChoiceV1,
    capture: u32,
    chosen: Option<crate::creature_types_v60::CreatureTypeV60>,
    count: Option<i32>,
    answer: Option<u32>,
    entered: Option<crate::state::ObjectLinkV4>,
    committed: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct MagePlanV1 {
    choice: mage_entry::ChoiceV1,
    named: Option<(u32, mage_entry::NameV1)>,
    committed: bool,
}

/// GameState never restores runtime ownership, and other modules cannot
/// construct it. Opaque input borrowing is confined to local family verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RunnerV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kinship_start: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    kinship_plans: Vec<KinshipPlanV1>,
    original_stack: Vec<crate::state::StackItem>,
    original_history_len: usize,
    batch_depth: usize,
    deferred_departures: Vec<(crate::ids::ObjectId, u32)>,
    answers: Vec<GeneralAnswerV36>,
    cursor: usize,
    checkpoint: Option<(Box<GameState>, GeneralChoiceV36)>,
    operation: OperationV1,
    original_sha256: String,
    banner_start: Option<u32>,
    color_plans: Vec<ColorPlanV36>,
    used_banner: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    original_world_v48: Option<Box<GameState>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attachment_input_v48: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    original_world_v50: Option<Box<GameState>>,
    #[serde(skip)]
    active_fake_aura_entry_v50: Option<crate::fake_your_own_death_v50::return_aura::EntryLoanV50>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mage_start: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    mage_plans: Vec<MagePlanV1>,
    #[serde(skip)]
    mage_names_v2: bool,
    #[serde(skip)]
    metamorph_mage_copies: Vec<mage_entry::MetamorphCopyV1>,
    #[serde(skip)]
    show_and_tell_entries: Vec<(
        crate::state::AbilitySourceContractV4,
        crate::event::ZoneChangeProposed,
    )>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateV1 {
    pub pending: Option<Box<CheckpointV1>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_kinship_v1: Option<Box<crate::banner_of_kinship_v1::entry::CheckpointV1>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_general_v36: Option<Box<GeneralCheckpointV36>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_token_attachment_v48:
        Option<Box<crate::season_token_attachment_v48::CheckpointV48>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_fake_aura_v50:
        Option<Box<crate::fake_your_own_death_v50::return_aura::CheckpointV50>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pending_meddling_mage_v1: Option<Box<mage_entry::CheckpointV1>>,
    #[serde(skip)]
    runtime: Option<Box<RunnerV1>>,
}
impl StateV1 {
    pub fn is_empty(&self) -> bool {
        self.pending_kinship_v1.is_none()
            && self.pending.is_none()
            && self.pending_general_v36.is_none()
            && self.pending_token_attachment_v48.is_none()
            && self.pending_fake_aura_v50.is_none()
            && self.pending_meddling_mage_v1.is_none()
    }
}
impl Hash for StateV1 {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        if let Some(pending) = &self.pending_fake_aura_v50 {
            "fake-aura-return/v50".hash(hasher);
            serde_json::to_vec(pending)
                .expect("owned Fake Aura checkpoint serializes")
                .hash(hasher);
        }
        if let Some(pending) = &self.pending_token_attachment_v48 {
            "season-token-attachment/v48".hash(hasher);
            serde_json::to_vec(pending)
                .expect("owned token checkpoint serializes")
                .hash(hasher);
        }
        if let Some(pending) = &self.pending {
            "linked-aura-operation/v1".hash(hasher);
            serde_json::to_vec(pending)
                .expect("owned checkpoint serializes")
                .hash(hasher);
        }
        if let Some(pending) = &self.pending_general_v36 {
            "banner-entry-operation/v36".hash(hasher);
            serde_json::to_vec(pending)
                .expect("owned Banner checkpoint serializes")
                .hash(hasher);
        }
        if let Some(pending) = &self.pending_kinship_v1 {
            "kinship-entry-operation/v1".hash(hasher);
            serde_json::to_vec(pending)
                .expect("owned Kinship checkpoint serializes")
                .hash(hasher);
        }
        if let Some(pending) = &self.pending_meddling_mage_v1 {
            "meddling-mage-entry/v1".hash(hasher);
            serde_json::to_vec(pending)
                .expect("owned Mage checkpoint serializes")
                .hash(hasher);
        }
    }
}

pub(crate) fn locally_owned(state: &GameState) -> bool {
    state.engine.linked_aura_operation_v1.runtime.is_some()
}

/// Borrow the existing enclosing operation of a published entry checkpoint.
pub(crate) fn pending_original_operation(state: &GameState) -> Option<(&GameState, &OperationV1)> {
    if let Some(pending) = &state
        .engine
        .linked_aura_operation_v1
        .pending_meddling_mage_v1
    {
        return Some((&pending.original, &pending.operation));
    }
    if let Some(p) = &state.engine.linked_aura_operation_v1.pending_kinship_v1 {
        return Some((&p.original, &p.operation));
    }
    if let Some(pending) = crate::fake_your_own_death_v50::return_aura::pending(state) {
        return Some((&pending.original, &pending.operation));
    }
    if let Some(pending) = crate::season_token_attachment_v48::pending(state) {
        return Some((&pending.original, &pending.operation));
    }
    if let Some(pending) = &state.engine.linked_aura_operation_v1.pending_general_v36 {
        return Some((&pending.original, &pending.operation));
    }
    state
        .engine
        .linked_aura_operation_v1
        .pending
        .as_ref()
        .map(|pending| (pending.original.as_ref(), &pending.operation))
}

/// Only the new end-turn borrow compares the same original-world framing.
pub(crate) fn origin_digest_matches_for_end_turn_v1(recorded: &str, state: &GameState) -> bool {
    origin_digest_matches(recorded, state)
}

/// An opaque input for speculative Panda verification. It cannot install a
/// runtime in a restored game; the only installer below borrows a local replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PandaReplayBorrowV1 {
    runner: Option<Box<RunnerV1>>,
}
impl Hash for PandaReplayBorrowV1 {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        serde_json::to_vec(self)
            .expect("Panda replay input serializes")
            .hash(hasher);
    }
}
pub(crate) fn capture_panda_replay_borrow_v1(state: &GameState) -> PandaReplayBorrowV1 {
    PandaReplayBorrowV1 {
        runner: state.engine.linked_aura_operation_v1.runtime.clone(),
    }
}

/// Opaque data from the actual enclosing operation, usable only in a
/// disposable end-turn journal replay. It never restores a world's runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndTurnReplayBorrowV1 {
    runner: Option<Box<RunnerV1>>,
}
impl Hash for EndTurnReplayBorrowV1 {
    fn hash<H: Hasher>(&self, h: &mut H) {
        serde_json::to_vec(self)
            .expect("native end-turn replay input serializes")
            .hash(h);
    }
}
pub(crate) fn capture_end_turn_replay_borrow_v1(state: &GameState) -> EndTurnReplayBorrowV1 {
    EndTurnReplayBorrowV1 {
        runner: state.engine.linked_aura_operation_v1.runtime.clone(),
    }
}
pub(crate) fn with_end_turn_replay_borrow_v1<T>(
    replay: &mut GameState,
    input: &EndTurnReplayBorrowV1,
    expected_operation: &OperationV1,
    run: impl FnOnce(&mut GameState) -> Result<T, String>,
) -> Result<T, String> {
    if locally_owned(replay) || has_pending(replay) {
        return Err("end-turn verification cannot acquire an actual public checkpoint".into());
    }
    if let Some(runner) = &input.runner {
        if &runner.operation != expected_operation
            || runner.checkpoint.is_some()
            || runner.batch_depth != 0
            || !runner.deferred_departures.is_empty()
            || runner.active_fake_aura_entry_v50.is_some()
        {
            return Err("end-turn replay changed its enclosing native operation".into());
        }
        crate::engine::time_stop_v1::validate_enclosing_input(
            replay,
            &runner.original_stack,
            runner.original_history_len,
            &runner.original_sha256,
        )?;
    }
    replay.engine.linked_aura_operation_v1.runtime = input.runner.clone();
    let result = run(replay);
    // Even an error cannot export the replay's synchronous owner.
    replay.engine.linked_aura_operation_v1.runtime = None;
    result
}
/// Ordinary V49 keeps its frozen world payload and old non-Drake bytes.
/// Only an actual synchronous Drake-owning operation needs this sidecar data.
/// The opaque value can be borrowed only by the existing local replay helper.
pub(crate) fn capture_conqueror_drake_replay_borrow_v49(
    state: &GameState,
) -> Option<PandaReplayBorrowV1> {
    (crate::drake_hatcher_v1::custody::relevant(state) && locally_owned(state))
        .then(|| capture_panda_replay_borrow_v1(state))
}

pub(crate) fn with_panda_replay_borrow_v1(
    replay: &mut GameState,
    original_item: &crate::state::StackItem,
    input: &PandaReplayBorrowV1,
    run: impl FnOnce(&mut GameState) -> Result<(), String>,
) -> Result<(), String> {
    if locally_owned(replay) || has_pending(replay) {
        return Err("Panda verification cannot acquire a public operation checkpoint".into());
    }
    if let Some(runner) = input.runner.as_ref() {
        if runner.operation != OperationV1::Advance
            || runner.checkpoint.is_some()
            || runner.original_history_len > replay.engine.event_history.len()
            || runner
                .original_stack
                .iter()
                .filter(|item| item.v4.stack_item_id == original_item.v4.stack_item_id)
                .count()
                != 1
            || !runner
                .original_stack
                .iter()
                .any(|item| item == original_item)
        {
            return Err("Panda replay lost its genuine enclosing Advance/stack input".into());
        }
    }
    replay.engine.linked_aura_operation_v1.runtime = input.runner.clone();
    let result = run(replay);
    // The borrowed runner never escapes into an actual/restored game.
    replay.engine.linked_aura_operation_v1.runtime = None;
    result
}
/// Captured scope for the two exact atomic Amulet programs. No original worlds,
/// answers or executable entry loans are nested in this input.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RavenousReplayBorrowV51 {
    enclosing: Option<RavenousEnclosingV51>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RavenousEnclosingV51 {
    original_stack: Vec<crate::state::StackItem>,
    original_history_len: usize,
    operation: OperationV1,
    original_sha256: String,
    had_checkpoint: bool,
}
pub(crate) fn capture_ravenous_replay_borrow_v51(state: &GameState) -> RavenousReplayBorrowV51 {
    RavenousReplayBorrowV51 {
        enclosing: state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_ref()
            .map(|runner| RavenousEnclosingV51 {
                original_stack: runner.original_stack.clone(),
                original_history_len: runner.original_history_len,
                operation: runner.operation.clone(),
                original_sha256: runner.original_sha256.clone(),
                had_checkpoint: runner.checkpoint.is_some(),
            }),
    }
}
pub(crate) fn with_ravenous_replay_borrow_v51(
    replay: &mut GameState,
    original_item: &crate::state::StackItem,
    input: &RavenousReplayBorrowV51,
    run: impl FnOnce(&mut GameState) -> Result<(), String>,
) -> Result<(), String> {
    if locally_owned(replay) || has_pending(replay) {
        return Err("Amulet verification cannot acquire a public checkpoint".into());
    }
    let expected = match original_item.v4.activated_ability_index {
        Some(0) => crate::ravenous_amulet_v1::draw_program(),
        Some(1) => crate::ravenous_amulet_v1::loss_program(),
        _ => return Err("Amulet verification lost its exact atomic program".into()),
    };
    let Some(CommittedEvent::RavenousRemovedV1 { removed }) = replay.engine.event_history.last()
    else {
        return Err("Amulet verification lost its original removal".into());
    };
    if original_item.inline_effect.as_ref() != Some(&expected)
        || !original_item
            .v4
            .ability_source_contract
            .is_some_and(|source| crate::ravenous_amulet_v1::definition(source.card_def))
        || removed.item.as_ref() != original_item
        || removed.kind != crate::ravenous_amulet_v1::RemovalKindV1::Resolving
        || removed.replay_borrow.as_ref() != Some(input)
    {
        return Err("Amulet verification lacks its original removed atomic input".into());
    }
    let Some(enclosing) = &input.enclosing else {
        return run(replay);
    };
    if enclosing.operation != OperationV1::Advance
        || enclosing.had_checkpoint
        || enclosing.original_history_len > removed.index as usize
        || !origin_digest_well_formed(&enclosing.original_sha256)
        || enclosing
            .original_stack
            .iter()
            .filter(|item| item.v4.stack_item_id == original_item.v4.stack_item_id)
            .collect::<Vec<_>>()
            .as_slice()
            != [original_item]
    {
        return Err("Amulet replay changed its enclosing Advance/stack input".into());
    }
    // The installer is limited to this disposable atomic replay. It never
    // returns a Runner or installs one in an actual/restored game.
    replay.engine.linked_aura_operation_v1.runtime = Some(Box::new(RunnerV1 {
        kinship_start: None,
        kinship_plans: Vec::new(),
        mage_start: None,
        mage_plans: Vec::new(),
        mage_names_v2: true,
        metamorph_mage_copies: Vec::new(),
        show_and_tell_entries: Vec::new(),
        original_stack: enclosing.original_stack.clone(),
        original_history_len: enclosing.original_history_len,
        batch_depth: 0,
        deferred_departures: Vec::new(),
        answers: Vec::new(),
        cursor: 0,
        checkpoint: None,
        operation: enclosing.operation.clone(),
        original_sha256: enclosing.original_sha256.clone(),
        banner_start: None,
        color_plans: Vec::new(),
        used_banner: false,
        original_world_v48: None,
        attachment_input_v48: None,
        original_world_v50: None,
        active_fake_aura_entry_v50: None,
    }));
    let result = run(replay);
    let runner = replay
        .engine
        .linked_aura_operation_v1
        .runtime
        .take()
        .ok_or("Amulet verification lost its local borrowed scope")?;
    if runner.checkpoint.is_some()
        || runner.cursor != 0
        || runner.batch_depth != 0
        || !runner.answers.is_empty()
        || !runner.deferred_departures.is_empty()
        || runner.kinship_start.is_some()
        || !runner.kinship_plans.is_empty()
        || runner.mage_start.is_some()
        || !runner.mage_plans.is_empty()
        || !runner.metamorph_mage_copies.is_empty()
        || !runner.show_and_tell_entries.is_empty()
        || runner.banner_start.is_some()
        || !runner.color_plans.is_empty()
        || runner.attachment_input_v48.is_some()
        || runner.original_world_v48.is_some()
        || runner.original_world_v50.is_some()
        || runner.active_fake_aura_entry_v50.is_some()
    {
        return Err("Amulet atomic replay attempted a choice or departure".into());
    }
    result
}

/// A selected spell's erased local pending row is valid only inside its own
/// exact enclosing entry operation. This query grants no execution authority;
/// the existing complete checkpoint replay/validation remains mandatory.
pub(crate) fn owns_entry_row_checkpoint_v1(
    state: &GameState,
    item: &crate::state::StackItem,
) -> bool {
    let scope = &state.engine.linked_aura_operation_v1;
    let original = |world: &GameState, operation: &OperationV1| {
        matches!(operation, OperationV1::Advance | OperationV1::Step(_))
            && world
                .engine
                .pending_effect
                .as_ref()
                .is_some_and(|p| p.resolving_item == *item)
            && world
                .stack
                .iter()
                .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
                .eq(std::iter::once(item))
    };
    scope.runtime.as_ref().is_some_and(|r| {
        r.checkpoint.is_some()
            && matches!(r.operation, OperationV1::Advance | OperationV1::Step(_))
            && r.original_stack
                .iter()
                .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
                .eq(std::iter::once(item))
    }) || scope
        .pending_general_v36
        .as_ref()
        .is_some_and(|p| original(&p.original, &p.operation))
        || scope
            .pending_kinship_v1
            .as_ref()
            .is_some_and(|p| original(&p.original, &p.operation))
}
pub(crate) fn entry_checkpoint_in_flight_v1(state: &GameState) -> bool {
    state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .is_some_and(|r| r.checkpoint.is_some())
}
pub fn has_pending(state: &GameState) -> bool {
    state
        .engine
        .linked_aura_operation_v1
        .pending_kinship_v1
        .is_some()
        || state.engine.linked_aura_operation_v1.pending.is_some()
        || has_banner_pending(state)
        || crate::season_token_attachment_v48::pending(state).is_some()
        || crate::fake_your_own_death_v50::return_aura::pending(state).is_some()
        || state
            .engine
            .linked_aura_operation_v1
            .pending_meddling_mage_v1
            .is_some()
}
pub(crate) fn has_banner_pending(state: &GameState) -> bool {
    state
        .engine
        .linked_aura_operation_v1
        .pending_general_v36
        .is_some()
}
fn potentially_returns_aura(state: &GameState) -> bool {
    state
        .engine
        .linked_exile_records
        .iter()
        .any(|link| crate::banishing_light_v1::original_token(state, *link).is_ok())
}
/// Separate simultaneous departures complete before their immediate linked
/// returns ask for attachment legality on the resulting battlefield.
pub(crate) fn begin_batch(state: &mut GameState) {
    if let Some(runtime) = state.engine.linked_aura_operation_v1.runtime.as_mut() {
        runtime.batch_depth += 1;
    }
}
pub(crate) fn defer_departure(
    state: &mut GameState,
    source: crate::ids::ObjectId,
    generation: u32,
) -> bool {
    let scoped = state.engine.linked_exile_records.iter().any(|link| {
        link.source.source == source
            && link.source.zone_change_count == generation
            && crate::banishing_light_v1::owns_definition(link.source.card_def)
    });
    let Some(runtime) = state.engine.linked_aura_operation_v1.runtime.as_mut() else {
        return false;
    };
    if !scoped || runtime.batch_depth == 0 {
        return false;
    }
    if !runtime.deferred_departures.contains(&(source, generation)) {
        runtime.deferred_departures.push((source, generation));
    }
    true
}
pub(crate) fn end_batch(state: &mut GameState) {
    let Some(runtime) = state.engine.linked_aura_operation_v1.runtime.as_mut() else {
        return;
    };
    runtime.batch_depth = runtime
        .batch_depth
        .checked_sub(1)
        .expect("paired actual batch owner");
    if runtime.batch_depth != 0 {
        return;
    }
    let departures = std::mem::take(&mut runtime.deferred_departures);
    return_departures(state, &departures);
}

fn invoke(state: &mut GameState, operation: &OperationV1) -> Result<Option<Decision>, String> {
    match operation {
        OperationV1::DrakeCombatBatchV1 {
            events,
            legacy_normal_after,
        } => {
            crate::engine::commit_combat_damage_events_inner(
                state,
                events.clone(),
                *legacy_normal_after,
            );
            if state.engine.halted.is_some() {
                return Err("Drake combat operation halted".into());
            }
            Ok(None)
        }
        OperationV1::DrakeCounterStackV1 { stack_item_id } => {
            crate::engine::counter_stack_item_by_id(state, *stack_item_id)?;
            Ok(None)
        }
        OperationV1::Step(action) => {
            if crate::drake_hatcher_v1::custody::relevant(state)
                && crate::twinflame_tyrant_v1::combat::has_pending(state)
            {
                let Action::ChooseEffectOption(index) = action else {
                    return Err("only the actual combat menu may be answered".into());
                };
                crate::twinflame_tyrant_v1::combat::answer(state, *index)?;
                return Ok(None);
            }
            crate::engine::step_without_linked_aura_v1(state, action.clone())?;
            Ok(None)
        }
        OperationV1::Advance => Ok(Some(crate::engine::advance_without_linked_aura_v1(state))),
        OperationV1::Event(event) => {
            crate::event::propose_without_linked_aura_v1(state, event.clone());
            Ok(None)
        }
        OperationV1::Batch(events) => {
            crate::event::batch_without_linked_aura_v1(state, events.clone());
            Ok(None)
        }
        OperationV1::Effect { op, ctx } => {
            crate::effect::execute_without_linked_aura_v1(op, ctx, state);
            Ok(None)
        }
        OperationV1::CopyTokenV36 { source, controller } => {
            let live = state
                .objects
                .try_get(source.source)
                .ok_or("Banner copy template missing")?;
            if live.zone != crate::state::Zone::Battlefield
                || live.zone_change_count != source.zone_change_count
                || live.owner != source.owner
                || live.card_def != source.card_def
                || live.v4.face_down_v1.is_some()
                || !crate::heraldic_banner_v1::owns_definition(source.card_def)
            {
                return Err("Banner copy lost actual original template incarnation".into());
            }
            crate::standard_cards_v1::create_token_copy_without_banner_owner(
                state,
                source.source,
                *controller,
            );
            Ok(None)
        }
        OperationV1::CopyTokenKinshipV1 { source, controller } => {
            let live = state
                .objects
                .try_get(source.source)
                .ok_or("Banner copy template missing")?;
            if live.zone != crate::state::Zone::Battlefield
                || live.zone_change_count != source.zone_change_count
                || live.owner != source.owner
                || live.card_def != source.card_def
                || live.v4.face_down_v1.is_some()
                || !crate::banner_of_kinship_v1::entry::owns_entry_definition(source.card_def)
            {
                return Err("Banner copy lost actual original template incarnation".into());
            }
            crate::standard_cards_v1::create_token_copy_without_banner_owner(
                state,
                source.source,
                *controller,
            );
            Ok(None)
        }
        OperationV1::Commit(event) => {
            crate::event::commit_without_linked_aura_v1(state, event.clone());
            Ok(None)
        }
    }
}
fn potential_operation(state: &GameState) -> bool {
    if state
        .stack
        .iter()
        .any(|row| crate::effect::genesis_wave_v1::selected(state, row))
    {
        return true;
    }
    if state
        .stack
        .iter()
        .any(|row| crate::effect::curator_v1::selected(state, row))
    {
        return true;
    }
    mage_entry::vial::waiting(state)
        || crate::effect::needs_electroduplicate_entry_owner_v1(state)
        || state.stack.last().is_some_and(|item| {
            show_and_tell::printed(item) || mage_entry::reanimate_metamorph_waiting(state, item)
        })
        || ((!state.stack.is_empty() || state.engine.pending_effect.is_some())
            && mage_entry::potentially_enters(state))
        || crate::drake_hatcher_v1::custody::relevant(state)
        || potentially_returns_aura(state)
        || crate::fake_your_own_death_v50::death_custody::has_unowned_roots(state)
        || state
            .objects
            .iter()
            .any(|(_, o)| crate::banner_of_kinship_v1::entry::owns_entry_definition(o.card_def))
        || crate::heraldic_banner_v1::potentially_enters(state)
        || !crate::doubling_season_v48::sources(state, state.active_player).is_empty()
        || !crate::doubling_season_v48::sources(state, state.active_player.opponent()).is_empty()
}

fn operation_has_color_entry(state: &GameState, operation: &OperationV1) -> bool {
    let selected = |event: &ProposedEvent| {
        crate::heraldic_banner_v1::choice(state, event, 0).is_ok_and(|choice| choice.is_some())
            || crate::banner_of_kinship_v1::entry::choice(state, event, 0)
                .is_ok_and(|choice| choice.is_some())
            || mage_entry::choice(state, event, 0, true).is_some()
    };
    match operation {
        OperationV1::Event(event) | OperationV1::Commit(event) => selected(event),
        OperationV1::Batch(events) => events.iter().any(selected),
        _ => false,
    }
}

thread_local! {
    /// Set while an operation recorded before origin digest v2 is replayed.
    static ORIGIN_V1: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether new origin digests are v1 here ([`as_recorded`]).
pub(crate) fn origin_v1_active() -> bool {
    ORIGIN_V1.with(std::cell::Cell::get)
}

/// The digest an operation records for its original world: v2, or v1 while
/// replaying an operation recorded before v2 ([`as_recorded`]).
fn original_sha256(state: &GameState) -> String {
    if ORIGIN_V1.with(std::cell::Cell::get) {
        origin_digest_v1(state)
    } else {
        origin_digest_v2(state)
    }
}

/// Streams canonical JSON into a digest instead of materializing it: a
/// history holding saved worlds can serialize to tens of megabytes.
fn stream_json(digest: &mut Sha256, value: &impl Serialize) {
    struct DigestWriter<'a>(&'a mut Sha256);
    impl std::io::Write for DigestWriter<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = std::io::BufWriter::with_capacity(1 << 16, DigestWriter(digest));
    serde_json::to_writer(&mut writer, value).expect("genuine origin serializes");
    std::io::Write::flush(&mut writer).expect("digest writes never fail");
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// v1: SHA-256 of the whole state's canonical JSON, history included, so
/// every operation rehashed the whole history.
fn origin_digest_v1(state: &GameState) -> String {
    let mut digest = Sha256::new();
    stream_json(&mut digest, state);
    hex(&digest.finalize())
}

/// v2: SHA-256 of a domain tag, the history length (u64, little-endian), the
/// history's v2 chain and the canonical JSON of the state with its own
/// history written as `[]`. The chain hashes each event once per history
/// lineage, so an operation's digest no longer grows with the game.
fn origin_digest_v2(state: &GameState) -> String {
    let history = &state.engine.event_history;
    let mut digest = Sha256::new();
    digest.update(b"mtg-kernel/linked-aura-origin/v2");
    digest.update(
        u64::try_from(history.len())
            .expect("history length")
            .to_le_bytes(),
    );
    digest.update(history.chain_digest_v2());
    history.omitted_v2(|| stream_json(&mut digest, state));
    format!("v2:{}", hex(&digest.finalize()))
}

/// A recorded origin digest of either version: 64 hex digits (v1), or `v2:`
/// and 64 hex digits.
pub(crate) fn origin_digest_well_formed(digest: &str) -> bool {
    let hex = digest.strip_prefix("v2:").unwrap_or(digest);
    hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Whether `state` is the original world of a recorded digest, checked in
/// the recorded digest's own version.
pub(crate) fn origin_digest_matches(recorded: &str, state: &GameState) -> bool {
    if recorded.starts_with("v2:") {
        recorded == origin_digest_v2(state)
    } else {
        recorded == origin_digest_v1(state)
    }
}

/// Runs `run` as code before origin digest v2 did: every new digest is v1.
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) fn with_origin_v1_for_test<T>(run: impl FnOnce() -> T) -> T {
    let previous = ORIGIN_V1.with(|v1| v1.replace(true));
    let result = run();
    ORIGIN_V1.with(|v1| v1.set(previous));
    result
}

/// The v1 and v2 origin digests of `state`.
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) fn origin_digests_for_test(state: &GameState) -> (String, String) {
    (origin_digest_v1(state), origin_digest_v2(state))
}

/// Checks a replay of a recorded operation as it was recorded: under v2, and
/// only when that fails, once more with v1 digests, so an operation recorded
/// before v2 still replays to its recorded bytes. A v1 failure reports the
/// v2 error.
pub(crate) fn as_recorded<T>(check: impl Fn() -> Result<T, String>) -> Result<T, String> {
    let current = check();
    if current.is_ok() || ORIGIN_V1.with(std::cell::Cell::get) {
        return current;
    }
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            ORIGIN_V1.with(|v1| v1.set(false));
        }
    }
    ORIGIN_V1.with(|v1| v1.set(true));
    let restore = Restore;
    let v1 = check();
    drop(restore);
    v1.or(current)
}

fn replay(
    original: &GameState,
    operation: &OperationV1,
    answers: &[AnswerV1],
) -> Result<(GameState, Option<Decision>), String> {
    let answers = answers
        .iter()
        .cloned()
        .map(GeneralAnswerV36::Host)
        .collect::<Vec<_>>();
    replay_general(original, operation, &answers, false)
}
fn replay_general(
    original: &GameState,
    operation: &OperationV1,
    answers: &[GeneralAnswerV36],
    used_banner: bool,
) -> Result<(GameState, Option<Decision>), String> {
    replay_general_current_slots_v69(original, operation, answers, used_banner, None, None)
}
fn replay_general_current_slots_v69(
    original: &GameState,
    operation: &OperationV1,
    answers: &[GeneralAnswerV36],
    used_banner: bool,
    current_slots: Option<&genesis_current_slots_v69::InputV69>,
    mage_names_v2: Option<bool>,
) -> Result<(GameState, Option<Decision>), String> {
    if has_pending(original) || locally_owned(original) {
        return Err("entry operation cannot recursively own another public checkpoint".into());
    }
    crate::effect::validate_pending_effect_choice(original)?;
    // Before any clone, so the staged world inherits the history's chain.
    let origin = original_sha256(original);
    let mut staged = match current_slots {
        Some(input) => genesis_current_slots_v69::materialize(original, operation, input)?,
        None => original.clone(),
    };
    staged.engine.linked_aura_operation_v1.runtime = Some(Box::new(RunnerV1 {
        kinship_start: None,
        kinship_plans: Vec::new(),
        original_stack: original.stack.clone(),
        original_history_len: original.engine.event_history.len(),
        batch_depth: 0,
        deferred_departures: Vec::new(),
        answers: answers.to_vec(),
        cursor: 0,
        checkpoint: None,
        operation: operation.clone(),
        original_sha256: origin.clone(),
        banner_start: None,
        color_plans: Vec::new(),
        used_banner,
        original_world_v48: crate::doubling_season_v48::requires_successor(original)
            .then(|| Box::new(original.clone())),
        attachment_input_v48: None,
        original_world_v50: crate::fake_your_own_death_v50::death_custody::has_unowned_roots(
            original,
        )
        .then(|| Box::new(original.clone())),
        active_fake_aura_entry_v50: None,
        mage_start: None,
        mage_plans: Vec::new(),
        metamorph_mage_copies: Vec::new(),
        show_and_tell_entries: Vec::new(),
        mage_names_v2: mage_names_v2
            .or_else(|| {
                answers.iter().find_map(|answer| match answer {
                    GeneralAnswerV36::MeddlingMageV1 { choice, .. } => Some(choice.names_v2),
                    _ => None,
                })
            })
            .unwrap_or(true),
    }));
    if potentially_returns_aura(original) {
        staged
            .engine
            .event_history
            .push(CommittedEvent::BanishingAuraOperationStartedV1 {
                operation: Box::new(operation.clone()),
                original_sha256: origin,
            });
    }
    let result = invoke(&mut staged, operation);
    let runner = staged
        .engine
        .linked_aura_operation_v1
        .runtime
        .take()
        .ok_or("entry operation lost synchronous owner")?;
    if runner.cursor != answers.len() {
        return Err("entry replay did not consume every confirmed answer".into());
    }
    if let Some((mut checkpoint, choice)) = runner.checkpoint {
        if current_slots.is_some()
            && (!runner.used_banner
                || !matches!(choice, GeneralChoiceV36::Color { .. })
                || runner.kinship_start.is_some()
                || runner.attachment_input_v48.is_some())
        {
            return Err("Genesis current-slot replay changed its exact color-entry owner".into());
        }
        if let GeneralChoiceV36::FakeAuraV50 { input, .. } = &choice {
            checkpoint
                .engine
                .linked_aura_operation_v1
                .pending_fake_aura_v50 = Some(Box::new(
                crate::fake_your_own_death_v50::return_aura::CheckpointV50 {
                    original: Box::new(original.clone()),
                    operation: operation.clone(),
                    answers: answers.to_vec(),
                    input: *input,
                    choice,
                    used_banner: runner.used_banner,
                },
            ));
        } else if let Some(input) = runner.attachment_input_v48 {
            checkpoint
                .engine
                .linked_aura_operation_v1
                .pending_token_attachment_v48 = Some(Box::new(
                crate::season_token_attachment_v48::CheckpointV48 {
                    original: Box::new(original.clone()),
                    operation: operation.clone(),
                    answers: answers.to_vec(),
                    choice,
                    input,
                    used_banner: runner.used_banner,
                },
            ));
        } else if let Some(operation_history_index) = runner.kinship_start {
            checkpoint
                .engine
                .linked_aura_operation_v1
                .pending_kinship_v1 =
                Some(Box::new(crate::banner_of_kinship_v1::entry::CheckpointV1 {
                    original: Box::new(original.clone()),
                    operation: operation.clone(),
                    answers: answers.to_vec(),
                    choice,
                    operation_history_index,
                    used_banner: runner.used_banner,
                }));
        } else if runner.mage_start.is_some()
            || matches!(
                choice,
                GeneralChoiceV36::ReanimateMetamorphV1 { .. }
                    | GeneralChoiceV36::VialMetamorphV1 { .. }
                    | GeneralChoiceV36::ShowAndTellV1 { .. }
            )
        {
            let confirmed_show_answers_sha256 = (!answers.is_empty()
                && matches!(&choice, GeneralChoiceV36::ShowAndTellV1 { choice, .. } if choice.physical.is_none()))
                .then(|| {
                    let mut digest = Sha256::new();
                    digest.update(b"mtg-kernel/show-and-tell-private-answers/v1");
                    digest.update(runner.original_sha256.as_bytes());
                    stream_json(&mut digest, &answers);
                    hex(&digest.finalize())
                });
            checkpoint
                .engine
                .linked_aura_operation_v1
                .pending_meddling_mage_v1 = Some(Box::new(mage_entry::CheckpointV1 {
                original: Box::new(original.clone()),
                operation: operation.clone(),
                answers: answers.to_vec(),
                confirmed_show_answers_sha256,
                choice,
                operation_history_index: runner.mage_start,
                used_banner: runner.used_banner,
            }));
        } else if runner.used_banner {
            checkpoint
                .engine
                .linked_aura_operation_v1
                .pending_general_v36 = Some(Box::new(GeneralCheckpointV36 {
                original: Box::new(original.clone()),
                operation: operation.clone(),
                answers: answers.to_vec(),
                choice,
                operation_history_index: runner
                    .banner_start
                    .ok_or("Banner checkpoint lost actual operation")?,
                genesis_current_slots_v69: current_slots.cloned(),
            }));
        } else {
            let GeneralChoiceV36::Host { choice, capture } = choice else {
                return Err("legacy host checkpoint changed kind".into());
            };
            let answers = answers
                .iter()
                .map(|answer| match answer {
                    GeneralAnswerV36::Host(host) => Ok(host.clone()),
                    _ => Err("legacy host checkpoint contains color answer"),
                })
                .collect::<Result<Vec<_>, _>>()?;
            checkpoint.engine.linked_aura_operation_v1.pending = Some(Box::new(CheckpointV1 {
                original: Box::new(original.clone()),
                operation: operation.clone(),
                answers,
                choice,
                capture_history_index: capture,
            }));
        }
        crate::drake_hatcher_v1::custody::enclose(
            original,
            operation,
            answers,
            runner.used_banner,
            &mut checkpoint,
        )?;
        return Ok((*checkpoint, None));
    }
    let result = result?;
    if runner.mage_plans.iter().any(|plan| !plan.committed) {
        return Err("Meddling Mage operation ended before its actual entry".into());
    }
    if let Some(operation) = runner.mage_start {
        mage_entry::record(&mut staged, mage_entry::JournalV1::Completed { operation });
    }
    if let Some(operation) = runner.kinship_start {
        if runner.kinship_plans.is_empty()
            || runner.kinship_plans.iter().any(|p| p.committed.is_none())
        {
            return Err("Kinship operation lost actual entries".into());
        }
        let mut entries: Vec<u32> = runner
            .kinship_plans
            .iter()
            .map(|p| p.committed.unwrap())
            .collect();
        entries.sort_unstable();
        staged
            .engine
            .event_history
            .push(CommittedEvent::KinshipEntryV1 {
                journal: Box::new(crate::banner_of_kinship_v1::entry::JournalV1::Completed {
                    operation,
                    entries,
                }),
            });
    }
    if let Some(operation) = runner.banner_start {
        if runner.color_plans.is_empty()
            || runner
                .color_plans
                .iter()
                .any(|plan| plan.committed.is_none())
        {
            return Err("Banner operation ended without all actual entries".into());
        }
        let mut entries: Vec<u32> = runner
            .color_plans
            .iter()
            .map(|plan| plan.committed.unwrap())
            .collect();
        entries.sort_unstable();
        staged
            .engine
            .event_history
            .push(CommittedEvent::BannerEntryOperationCompletedV1 { operation, entries });
    }
    crate::drake_hatcher_v1::custody::enclose(
        original,
        operation,
        answers,
        runner.used_banner,
        &mut staged,
    )?;
    Ok((staged, result))
}

/// The first unanswered return snapshots the exact departure point. Later
/// work may run only in the disposable candidate, never the published state.
pub(crate) fn request(state: &mut GameState, choice: HostChoiceV1) -> Result<bool, String> {
    let at = crate::linked_aura_return_v1::record_capture(state, choice.clone())?;
    let Some(host) = choose(state, choice.clone(), at)? else {
        return Ok(false);
    };
    let start = state.engine.event_history.len();
    crate::linked_aura_return_v1::answer(state, &choice, host)?;
    record_answer(state, &choice, at, host, start)?;
    Ok(true)
}

fn record_answer(
    state: &mut GameState,
    choice: &HostChoiceV1,
    at: u64,
    host: EffectObjectBinding,
    start: usize,
) -> Result<(), String> {
    let returned = state.engine.event_history[start..]
        .iter()
        .position(|event| {
            matches!(event,
        CommittedEvent::ZoneChange { object, from: crate::state::Zone::Exile,
            to: crate::state::Zone::Battlefield, .. } if *object == choice.aura.object)
        })
        .map(|offset| start + offset)
        .ok_or("linked Aura selected return did not actually occur")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::BanishingAuraHostChosenV1 {
            capture_history_index: at,
            host,
            return_history_index: u64::try_from(returned).map_err(|_| "Aura return exceeds u64")?,
        });
    Ok(())
}

/// Choosing does not move any returning member. In a simultaneous return all
/// menus are captured first, then owners answer in APNAP order (CR 610.3d).
fn choose(
    state: &mut GameState,
    choice: HostChoiceV1,
    at: u64,
) -> Result<Option<EffectObjectBinding>, String> {
    crate::linked_aura_return_v1::validate(state, &choice)?;
    if choice.original_candidates.is_empty() {
        return Err("empty host menu must use the no-return path".into());
    }
    let runner = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .ok_or("nonempty linked Aura return has no genuine operation owner")?;
    if runner.checkpoint.is_some() {
        return Ok(None);
    }
    if let Some(answer) = runner.answers.get(runner.cursor).cloned() {
        let GeneralAnswerV36::Host(answer) = answer else {
            return Err("entry replay changed its confirmed choice kind".into());
        };
        if answer.choice != choice || !choice.original_candidates.contains(&answer.host) {
            return Err("linked Aura replay changed an already committed owner answer".into());
        }
        state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_mut()
            .unwrap()
            .cursor += 1;
        Ok(Some(answer.host))
    } else {
        let mut snapshot = state.clone();
        // The genuine resolving row was removed by the ordinary engine before
        // its effects. Borrow that original row for visible stack membership
        // during the owner choice; never reconstruct a fabricated definition.
        let runner = state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_ref()
            .unwrap();
        if let Some(original) = runner.original_stack.last() {
            let absent = !snapshot
                .stack
                .iter()
                .any(|item| item.v4.stack_item_id == original.v4.stack_item_id);
            let departed=snapshot.engine.event_history[runner.original_history_len..].iter().any(|event|
                matches!(event,CommittedEvent::ZoneChange {object,from:crate::state::Zone::Stack,..} if *object==original.source));
            if absent && !departed {
                snapshot.stack.insert(
                    runner
                        .original_stack
                        .len()
                        .saturating_sub(1)
                        .min(snapshot.stack.len()),
                    original.clone(),
                );
            }
        }
        snapshot.engine.linked_aura_operation_v1 = StateV1::default();
        state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_mut()
            .unwrap()
            .checkpoint = Some((
            Box::new(snapshot),
            GeneralChoiceV36::Host {
                choice,
                capture: at,
            },
        ));
        Ok(None)
    }
}

/// All selected until-event effects triggered by the same event return in one
/// entry batch. A creature entering in that batch cannot be a chosen Aura host
/// because host legality is determined before any member enters (303.4f).
pub(crate) fn return_departures(state: &mut GameState, departures: &[(crate::ids::ObjectId, u32)]) {
    use crate::banishing_light_v1::NoReturnV1;
    use crate::state::Zone;
    let mut records = Vec::new();
    state.engine.linked_exile_records.retain(|record| {
        let selected = crate::banishing_light_v1::owns_definition(record.source.card_def)
            && departures.contains(&(record.source.source, record.source.zone_change_count));
        if selected {
            records.push(*record);
        }
        !selected
    });
    if records.is_empty() {
        return;
    }
    let result = (|| -> Result<(), String> {
        let mut reasons = vec![None; records.len()];
        let mut choices = Vec::new();
        for (index, record) in records.iter().copied().enumerate() {
            if crate::banishing_light_v1::original_token(state, record)? {
                reasons[index] = Some(NoReturnV1::TokenCannotReturn);
            } else if !state.objects.try_get(record.exiled).is_some_and(|live| {
                live.zone == Zone::Exile
                    && live.zone_change_count == record.exiled_zone_change_count
                    && state.exile.contains(&record.exiled)
            }) {
                reasons[index] = Some(NoReturnV1::RecipientDeparted);
            } else if crate::card_def::CARD_DEFS[state.objects.get(record.exiled).card_def as usize]
                .attachment
                .is_some()
            {
                let choice = crate::linked_aura_return_v1::prepare(state, record)?;
                choices.push((index, choice));
            }
        }
        // Preserve same-owner source order; opposite owners follow APNAP,
        // independently of the order in which source departures were listed.
        choices.sort_by_key(|(_, choice)| choice.owner != state.active_player);
        let mut captured = Vec::new();
        for (index, choice) in choices {
            let at = crate::linked_aura_return_v1::record_capture(state, choice.clone())?;
            if choice.original_candidates.is_empty() {
                reasons[index] = Some(NoReturnV1::AuraNoLegalHost {
                    capture_history_index: at,
                });
            } else {
                captured.push((index, choice, at));
            }
        }
        let mut selected = Vec::new();
        for (index, choice, at) in captured {
            let Some(host) = choose(state, choice.clone(), at)? else {
                return Ok(());
            };
            selected.push((index, choice, at, host));
        }
        let start = state.engine.event_history.len();
        let events = records
            .iter()
            .enumerate()
            .filter(|(index, _)| reasons[*index].is_none())
            .map(|(_, record)| {
                ProposedEvent::zone_change_preserving_known_identity(
                    record.exiled,
                    Zone::Battlefield,
                )
            })
            .collect();
        let attachments = selected
            .iter()
            .map(|(_, choice, _, host)| (choice.aura, *host))
            .collect();
        crate::event::batch_with_linked_aura_attachments_v1(state, events, attachments);
        for (_, choice, at, host) in selected {
            record_answer(state, &choice, at, host, start)?;
        }
        for (record, reason) in records.into_iter().zip(reasons) {
            crate::banishing_light_v1::record_expiry(state, record, start, reason);
        }
        Ok(())
    })();
    if result.is_err() {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            departures[0].0,
        ));
    }
}

/// Complete checkpoint reconstruction authenticates source incarnation,
/// owner, menu, already committed choices, actual journal and operation tail.
pub fn validate(state: &GameState) -> Result<(), String> {
    let owners = usize::from(
        state
            .engine
            .linked_aura_operation_v1
            .pending_kinship_v1
            .is_some(),
    ) + usize::from(state.engine.linked_aura_operation_v1.pending.is_some())
        + usize::from(
            state
                .engine
                .linked_aura_operation_v1
                .pending_general_v36
                .is_some(),
        )
        + usize::from(crate::season_token_attachment_v48::pending(state).is_some())
        + usize::from(crate::fake_your_own_death_v50::return_aura::pending(state).is_some())
        + usize::from(
            state
                .engine
                .linked_aura_operation_v1
                .pending_meddling_mage_v1
                .is_some(),
        );
    if owners > 1 || (owners != 0 && locally_owned(state)) {
        return Err("entry checkpoint has multiple or synchronous owners".into());
    }
    if let Some(pending) = &state
        .engine
        .linked_aura_operation_v1
        .pending_meddling_mage_v1
    {
        let names_v2 = if let GeneralChoiceV36::ReanimateMetamorphV1 {
            original_sha256, ..
        }
        | GeneralChoiceV36::VialMetamorphV1 {
            original_sha256, ..
        }
        | GeneralChoiceV36::ShowAndTellV1 {
            original_sha256, ..
        } = &pending.choice
        {
            if pending.operation_history_index.is_some()
                || !origin_digest_matches(original_sha256, &pending.original)
            {
                return Err("Metamorph copy checkpoint changed its original operation".into());
            }
            true
        } else {
            let started = pending
                .operation_history_index
                .and_then(|at| mage_entry::journal(state, at));
            let names_v2 = matches!(
                started,
                Some(mage_entry::JournalV1::StartedWithNamesV2 { .. })
            );
            match started {
                Some(
                    mage_entry::JournalV1::Started {
                        operation,
                        original_sha256,
                    }
                    | mage_entry::JournalV1::StartedWithNamesV2 {
                        operation,
                        original_sha256,
                    },
                ) if **operation == pending.operation
                    && origin_digest_matches(original_sha256, &pending.original) => {}
                _ => return Err("Meddling Mage checkpoint lost its original operation".into()),
            }
            names_v2
        };
        return as_recorded(|| {
            let (expected, _) = replay_general_current_slots_v69(
                &pending.original,
                &pending.operation,
                &pending.answers,
                pending.used_banner,
                None,
                Some(names_v2),
            )?;
            if expected != *state {
                return Err("Meddling Mage checkpoint, entrant or original choice changed".into());
            }
            Ok(())
        });
    }
    if let Some(p) = &state.engine.linked_aura_operation_v1.pending_kinship_v1 {
        if !matches!(crate::banner_of_kinship_v1::entry::journal(state,p.operation_history_index),Some(crate::banner_of_kinship_v1::entry::JournalV1::Started{operation,original_sha256:digest}) if **operation==p.operation && origin_digest_matches(digest, &p.original))
        {
            return Err("Kinship checkpoint lost independently committed operation".into());
        }
        return as_recorded(|| {
            let (expected, _) =
                replay_general(&p.original, &p.operation, &p.answers, p.used_banner)?;
            if expected != *state
                || expected
                    .engine
                    .linked_aura_operation_v1
                    .pending_kinship_v1
                    .is_none()
            {
                return Err(
                    "Kinship checkpoint/source/proposal/scene/stack/answers changed".into(),
                );
            }
            Ok(())
        });
    }
    if let Some(pending) = crate::fake_your_own_death_v50::return_aura::pending(state) {
        if !matches!(state.engine.event_history.get(pending.input as usize),
            Some(CommittedEvent::FakeJournalV50 { journal }) if matches!(journal.as_ref(),
                crate::fake_your_own_death_v50::spell_custody::JournalV50::AuraReturnInput { input }
                    if input.original == pending.original && input.operation == pending.operation
                        && input.prior_answers == pending.answers
                        && pending.choice == (GeneralChoiceV36::FakeAuraV50 {
                            choice: Box::new(input.choice.clone()), input: pending.input })))
        {
            return Err("Fake Aura checkpoint lost independently committed original input".into());
        }
        return as_recorded(|| {
            let (expected, _) = replay_general(
                &pending.original,
                &pending.operation,
                &pending.answers,
                pending.used_banner,
            )?;
            if expected != *state
                || crate::fake_your_own_death_v50::return_aura::pending(&expected).is_none()
            {
                return Err(
                    "Fake Aura checkpoint changed original operation, source, menu or prior answers"
                        .into(),
                );
            }
            Ok(())
        });
    }
    if let Some(pending) = crate::season_token_attachment_v48::pending(state) {
        if locally_owned(state)
            || state.engine.linked_aura_operation_v1.pending.is_some()
            || state
                .engine
                .linked_aura_operation_v1
                .pending_general_v36
                .is_some()
        {
            return Err("token attachment checkpoint has another owner".into());
        }
        if !matches!(state.engine.event_history.get(pending.input as usize), Some(CommittedEvent::TokenAttachmentJournalV48 { journal }) if matches!(journal.as_ref(), crate::season_token_attachment_v48::JournalV48::Input { input } if input.original==pending.original && input.operation==pending.operation))
        {
            return Err("token attachment checkpoint lost independent original input".into());
        }
        as_recorded(|| {
            let (expected, _) = replay_general(
                &pending.original,
                &pending.operation,
                &pending.answers,
                pending.used_banner,
            )?;
            if expected != *state
                || crate::season_token_attachment_v48::pending(&expected).is_none()
            {
                return Err(
                    "token attachment checkpoint changed original operation or confirmed answers"
                        .into(),
                );
            }
            Ok(())
        })?;
        return Ok(());
    }
    if let Some(pending) = state
        .engine
        .linked_aura_operation_v1
        .pending_general_v36
        .as_ref()
    {
        if locally_owned(state) || state.engine.linked_aura_operation_v1.pending.is_some() {
            return Err("Banner checkpoint has foreign runtime/second owner".into());
        }
        if !matches!(state.engine.event_history.get(pending.operation_history_index as usize), Some(CommittedEvent::BannerEntryOperationStartedV1 { operation, original_sha256: digest }) if **operation == pending.operation && origin_digest_matches(digest, &pending.original))
        {
            return Err("Banner checkpoint lost independently committed operation".into());
        }
        return as_recorded(|| {
            let (expected, _) = replay_general_current_slots_v69(
                &pending.original,
                &pending.operation,
                &pending.answers,
                true,
                pending.genesis_current_slots_v69.as_ref(),
                None,
            )?;
            if expected != *state || !has_banner_pending(&expected) {
                return Err("Banner checkpoint/original operation changed".into());
            }
            Ok(())
        });
    }
    let Some(pending) = state.engine.linked_aura_operation_v1.pending.as_ref() else {
        return Ok(());
    };
    if locally_owned(state) || pending.choice.original_candidates.is_empty() {
        return Err("linked Aura public checkpoint has invalid ownership".into());
    }
    let start = pending.original.engine.event_history.len();
    if !matches!(state.engine.event_history.get(start),Some(CommittedEvent::BanishingAuraOperationStartedV1 {operation,original_sha256:digest})
        if **operation==pending.operation && origin_digest_matches(digest, &pending.original))
    {
        return Err(
            "linked Aura checkpoint lost its independently committed original operation".into(),
        );
    }
    as_recorded(|| {
        let (expected, _) = replay(&pending.original, &pending.operation, &pending.answers)?;
        if expected != *state || !has_pending(&expected) {
            return Err("linked Aura checkpoint or genuine operation binding changed".into());
        }
        Ok(())
    })
}
fn decision(state: &GameState) -> Result<Decision, String> {
    validate(state)?;
    if let Some(pending) = &state
        .engine
        .linked_aura_operation_v1
        .pending_meddling_mage_v1
    {
        return equipment_decision_v48(&pending.choice);
    }
    if let Some(p) = &state.engine.linked_aura_operation_v1.pending_kinship_v1 {
        return equipment_decision_v48(&p.choice);
    }
    if let Some(pending) = crate::fake_your_own_death_v50::return_aura::pending(state) {
        let GeneralChoiceV36::FakeAuraV50 { choice, .. } = &pending.choice else {
            return Err("Fake Aura checkpoint changed choice kind".into());
        };
        return Ok(Decision::ChooseEffectTargets {
            player: choice.owner,
            source: choice.aura.object,
            selected_count: 0,
            min_targets: 1,
            max_targets: 1,
            can_finish: false,
            legal_targets: choice
                .candidates
                .iter()
                .map(|host| Target::Object(host.object))
                .collect(),
        });
    }
    if let Some(pending) = crate::season_token_attachment_v48::pending(state) {
        return equipment_decision_v48(&pending.choice);
    }
    if let Some(pending) = state
        .engine
        .linked_aura_operation_v1
        .pending_general_v36
        .as_ref()
    {
        return Ok(match &pending.choice {
            GeneralChoiceV36::TokenEquipmentV48 { .. }
            | GeneralChoiceV36::FakeAuraV50 { .. }
            | GeneralChoiceV36::KinshipV1 { .. }
            | GeneralChoiceV36::MeddlingMageV1 { .. }
            | GeneralChoiceV36::ReanimateMetamorphV1 { .. }
            | GeneralChoiceV36::VialMetamorphV1 { .. }
            | GeneralChoiceV36::ShowAndTellV1 { .. } => {
                return Err("successor choice cannot use frozen Banner checkpoint".into())
            }
            GeneralChoiceV36::Color { choice, .. } => Decision::ChooseEntryColorV36 {
                player: choice.controller,
                entry_ordinal: choice.ordinal,
                colors: crate::heraldic_banner_v1::COLORS,
            },
            GeneralChoiceV36::Host { choice, .. } => Decision::ChooseEffectTargets {
                player: choice.owner,
                source: choice.aura.object,
                selected_count: 0,
                min_targets: 1,
                max_targets: 1,
                can_finish: false,
                legal_targets: choice
                    .original_candidates
                    .iter()
                    .map(|binding| Target::Object(binding.object))
                    .collect(),
            },
        });
    }

    let pending = state
        .engine
        .linked_aura_operation_v1
        .pending
        .as_ref()
        .ok_or("linked Aura choice is absent")?;
    Ok(Decision::ChooseEffectTargets {
        player: pending.choice.owner,
        source: pending.choice.aura.object,
        selected_count: 0,
        min_targets: 1,
        max_targets: 1,
        can_finish: false,
        legal_targets: pending
            .choice
            .original_candidates
            .iter()
            .map(|binding| Target::Object(binding.object))
            .collect(),
    })
}
pub(crate) fn step(state: &mut GameState, action: Action) -> Result<(), String> {
    crate::event::validate_copied_token_abilities_v1(state)?;
    mage_entry::validate_retained(state)?;
    if let Some(pending) = &state
        .engine
        .linked_aura_operation_v1
        .pending_meddling_mage_v1
    {
        validate(state)?;
        let answer = answer_equipment_checkpoint_v48(&pending.choice, action)?;
        let mut answers = pending.answers.clone();
        answers.push(answer);
        let (next, _) = replay_general(
            &pending.original,
            &pending.operation,
            &answers,
            pending.used_banner,
        )?;
        *state = next;
        return Ok(());
    }
    if let Some(p) = &state.engine.linked_aura_operation_v1.pending_kinship_v1 {
        validate(state)?;
        let answer = answer_equipment_checkpoint_v48(&p.choice, action)?;
        let mut answers = p.answers.clone();
        answers.push(answer);
        let (next, _) = replay_general(&p.original, &p.operation, &answers, p.used_banner)?;
        *state = next;
        return Ok(());
    }
    if let Some(pending) = crate::fake_your_own_death_v50::return_aura::pending(state) {
        validate(state)?;
        let answer = answer_equipment_checkpoint_v48(&pending.choice, action)?;
        let mut answers = pending.answers.clone();
        answers.push(answer);
        let (next, _) = replay_general(
            &pending.original,
            &pending.operation,
            &answers,
            pending.used_banner,
        )?;
        *state = next;
        return Ok(());
    }
    if let Some(pending) = crate::season_token_attachment_v48::pending(state) {
        validate(state)?;
        let answer = answer_equipment_checkpoint_v48(&pending.choice, action)?;
        let mut answers = pending.answers.clone();
        answers.push(answer);
        let (next, _) = replay_general(
            &pending.original,
            &pending.operation,
            &answers,
            pending.used_banner,
        )?;
        *state = next;
        return Ok(());
    }
    if let Some(pending) = state
        .engine
        .linked_aura_operation_v1
        .pending_general_v36
        .as_ref()
    {
        validate(state)?;
        let answer = match (&pending.choice, action) {
            (GeneralChoiceV36::Color { choice, .. }, Action::ChooseEffectOption(index)) => {
                GeneralAnswerV36::Color {
                    choice: choice.clone(),
                    color: *crate::heraldic_banner_v1::COLORS
                        .get(index as usize)
                        .ok_or("color index outside WUBRG")?,
                }
            }
            (
                GeneralChoiceV36::Host { choice, .. },
                Action::ChooseEffectTarget(Target::Object(id)),
            ) => GeneralAnswerV36::Host(AnswerV1 {
                choice: choice.clone(),
                host: *choice
                    .original_candidates
                    .iter()
                    .find(|binding| binding.object == id)
                    .ok_or("host outside original entry menu")?,
            }),
            _ => {
                return Err(
                    "only the exact pre-entry owner choice may advance this operation".into(),
                )
            }
        };
        let mut answers = pending.answers.clone();
        answers.push(answer);
        let (next, _) = replay_general_current_slots_v69(
            &pending.original,
            &pending.operation,
            &answers,
            true,
            pending.genesis_current_slots_v69.as_ref(),
            None,
        )?;
        *state = next;
        return Ok(());
    }
    crate::heraldic_banner_v1::validate_state(state)?;
    crate::banner_of_kinship_v1::entry::validate_state(state)?;
    if locally_owned(state) || (!has_pending(state) && !potential_operation(state)) {
        return crate::engine::step_without_linked_aura_v1(state, action);
    }
    if let Some(pending) = state.engine.linked_aura_operation_v1.pending.as_ref() {
        validate(state)?;
        let Action::ChooseEffectTarget(Target::Object(host)) = action else {
            return Err("only the Aura owner may choose its untargeted legal attachment".into());
        };
        let binding = pending
            .choice
            .original_candidates
            .iter()
            .find(|binding| binding.object == host)
            .copied()
            .ok_or("linked Aura attachment is outside the current exact menu")?;
        let mut answers = pending.answers.clone();
        answers.push(AnswerV1 {
            choice: pending.choice.clone(),
            host: binding,
        });
        let (next, _) = replay(&pending.original, &pending.operation, &answers)?;
        *state = next;
        return Ok(());
    }
    let (next, _) = replay(state, &OperationV1::Step(action), &[])?;
    *state = next;
    Ok(())
}
pub(crate) fn advance(state: &mut GameState) -> Decision {
    if crate::event::validate_copied_token_abilities_v1(state).is_err()
        || mage_entry::validate_retained(state).is_err()
    {
        return Decision::Halted {
            mechanic: crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            source: crate::ids::ObjectId(0),
        };
    }
    if has_pending(state) {
        return decision(state).unwrap_or(Decision::Halted {
            mechanic: crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            source: state
                .engine
                .linked_aura_operation_v1
                .pending
                .as_ref()
                .map(|pending| pending.choice.aura.object)
                .unwrap_or(crate::ids::ObjectId(0)),
        });
    }
    if crate::heraldic_banner_v1::validate_state(state).is_err()
        || crate::banner_of_kinship_v1::entry::validate_state(state).is_err()
    {
        return Decision::Halted {
            mechanic: crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            source: crate::ids::ObjectId(0),
        };
    }
    if locally_owned(state) || !potential_operation(state) {
        return crate::engine::advance_without_linked_aura_v1(state);
    }
    match replay(state, &OperationV1::Advance, &[]) {
        Ok((next, result)) => {
            *state = next;
            result.unwrap_or_else(|| decision(state).expect("new exact owner checkpoint"))
        }
        Err(_error) => {
            #[cfg(test)]
            eprintln!("Linked operation advance rejected: {_error}");
            Decision::Halted {
                mechanic: crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                source: crate::ids::ObjectId(0),
            }
        }
    }
}
pub(crate) fn needs_event_owner(state: &GameState) -> bool {
    !locally_owned(state)
        && (has_pending(state)
            || potential_operation(state)
            || crate::card_def::CARD_DEFS
                .iter()
                .enumerate()
                .any(|(id, _)| {
                    crate::heraldic_banner_v1::owns_definition(id as u16)
                        || crate::banner_of_kinship_v1::entry::owns_entry_definition(id as u16)
                        || mage_entry::owns_definition(id as u16)
                }))
}

/// Whole-resolution ownership follows the same actual selection as outer
/// Advance/Step. Catalog-aware event dispatch additionally probes new entries,
/// but an unrelated catalog definition does not create a surrounding Runner.
pub(crate) fn needs_surrounding_operation_owner(state: &GameState) -> bool {
    !locally_owned(state) && (has_pending(state) || potential_operation(state))
}

pub(crate) fn event(state: &mut GameState, operation: OperationV1) -> bool {
    if crate::event::validate_copied_token_abilities_v1(state).is_err() {
        return true;
    }
    if locally_owned(state) {
        return false;
    }
    if crate::heraldic_banner_v1::validate_state(state).is_err()
        || crate::banner_of_kinship_v1::entry::validate_state(state).is_err()
    {
        return true;
    }
    if !has_pending(state)
        && !potential_operation(state)
        && !operation_has_color_entry(state, &operation)
    {
        return false;
    }
    if has_pending(state) {
        return true; // No event may mutate an externally suspended owner.
    }
    // A draw moves no permanent, so it starts no linked operation. Its replay
    // would validate a resolving ability without the removed row and drop it.
    if matches!(
        &operation,
        OperationV1::Event(ProposedEvent::Draw(_)) | OperationV1::Commit(ProposedEvent::Draw(_))
    ) {
        return false;
    }
    if let Ok((next, _)) = replay(state, &operation, &[]) {
        *state = next;
    }
    true
}

/// Capture the complete actual entry roster before any member is committed.
/// Answers run in APNAP order; proposal ordinals retain the original roster.
fn capture_color_plans(
    state: &mut GameState,
    events: &[ProposedEvent],
) -> Result<Vec<usize>, String> {
    let mut choices = Vec::new();
    for event in events {
        if let Some(choice) = crate::heraldic_banner_v1::choice(state, event, 0)? {
            choices.push(choice);
        }
    }
    if choices.is_empty() {
        return Ok(Vec::new());
    }
    let runner = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .ok_or("Banner as-enters choice has no actual operation owner")?;
    if runner.checkpoint.is_some() {
        return Err("entry capture after suspended operation".into());
    }
    // A batch pre-captures these proposals. Its per-member commit consumes
    // an existing plan, never asks a second choice for the same occurrence.
    if choices.len() == 1
        && runner.color_plans.iter().any(|p| {
            p.committed.is_none() && p.color.is_some() && p.choice.proposal == choices[0].proposal
        })
    {
        return Ok(Vec::new());
    }
    let first = runner.color_plans.len();
    let mut additions = Vec::new();
    for (offset, mut choice) in choices.into_iter().enumerate() {
        choice.ordinal =
            u32::try_from(first + offset).map_err(|_| "Banner entry ordinal overflow")?;
        additions.push(choice);
    }
    if state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .unwrap()
        .banner_start
        .is_none()
    {
        let index = u32::try_from(state.engine.event_history.len())
            .map_err(|_| "Banner operation index overflow")?;
        let runtime = state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_mut()
            .unwrap();
        runtime.banner_start = Some(index);
        runtime.used_banner = true;
        state
            .engine
            .event_history
            .push(CommittedEvent::BannerEntryOperationStartedV1 {
                operation: Box::new(runtime.operation.clone()),
                original_sha256: runtime.original_sha256.clone(),
            });
    }
    let operation = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .unwrap()
        .banner_start
        .unwrap();
    for choice in additions {
        let capture = u32::try_from(state.engine.event_history.len())
            .map_err(|_| "Banner capture index overflow")?;
        state
            .engine
            .event_history
            .push(CommittedEvent::BannerEntryColorCapturedV1 {
                operation,
                choice: Box::new(choice.clone()),
            });
        state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_mut()
            .unwrap()
            .color_plans
            .push(ColorPlanV36 {
                choice,
                capture,
                color: None,
                answer: None,
                committed: None,
            });
    }
    Ok((first
        ..state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_ref()
            .unwrap()
            .color_plans
            .len())
        .collect())
}

fn answer_color_plan(state: &mut GameState, index: usize) -> Result<bool, String> {
    let runtime = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .unwrap();
    let plan = runtime.color_plans[index].clone();
    if let Some(answer) = runtime.answers.get(runtime.cursor).cloned() {
        let GeneralAnswerV36::Color { choice, color } = answer else {
            return Err("Banner replay changed its confirmed choice kind".into());
        };
        if choice.as_ref() != &plan.choice || !crate::heraldic_banner_v1::COLORS.contains(&color) {
            return Err("Banner replay changed an actual entry/color answer".into());
        }
        let at = u32::try_from(state.engine.event_history.len())
            .map_err(|_| "Banner answer index overflow")?;
        state
            .engine
            .event_history
            .push(CommittedEvent::BannerEntryColorChosenV1 {
                capture: plan.capture,
                color,
            });
        let runtime = state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_mut()
            .unwrap();
        runtime.cursor += 1;
        runtime.color_plans[index].color = Some(color);
        runtime.color_plans[index].answer = Some(at);
    } else {
        let mut snapshot = state.clone();
        let runner = state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_ref()
            .unwrap();
        if let Some(original) = runner.original_stack.last() {
            let absent = !snapshot
                .stack
                .iter()
                .any(|i| i.v4.stack_item_id == original.v4.stack_item_id);
            let departed = snapshot.engine.event_history[runner.original_history_len..].iter().any(|event|
                matches!(event, CommittedEvent::ZoneChange { object, from: crate::state::Zone::Stack, .. } if *object == original.source));
            if absent && !departed {
                snapshot.stack.insert(
                    runner
                        .original_stack
                        .len()
                        .saturating_sub(1)
                        .min(snapshot.stack.len()),
                    original.clone(),
                );
            }
        }
        snapshot.engine.linked_aura_operation_v1 = StateV1::default();
        state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_mut()
            .unwrap()
            .checkpoint = Some((
            Box::new(snapshot),
            GeneralChoiceV36::Color {
                choice: Box::new(plan.choice),
                capture: plan.capture,
            },
        ));
        return Ok(false);
    }
    Ok(true)
}

fn capture_mage_plans(
    state: &mut GameState,
    events: &[ProposedEvent],
) -> Result<Vec<usize>, String> {
    let names_v2 = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .is_none_or(|runner| runner.mage_names_v2);
    let mut plans = Vec::new();
    for (ordinal, event) in events.iter().enumerate() {
        let ordinal = u32::try_from(ordinal).map_err(|_| "entry ordinal overflow")?;
        if let Some(choice) = mage_entry::choice(state, event, ordinal, names_v2) {
            plans.push(MagePlanV1 {
                choice,
                named: None,
                committed: false,
            });
        }
    }
    if plans.is_empty() {
        return Ok(Vec::new());
    }
    let runner = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_mut()
        .ok_or("Meddling Mage entry lost its actual operation owner")?;
    let start = if runner.mage_start.is_none() && plans.iter().any(|plan| !plan.choice.suppressed) {
        let at = u32::try_from(state.engine.event_history.len())
            .map_err(|_| "Meddling Mage operation index overflow")?;
        runner.mage_start = Some(at);
        let operation = Box::new(runner.operation.clone());
        let original_sha256 = runner.original_sha256.clone();
        Some(if names_v2 {
            mage_entry::JournalV1::StartedWithNamesV2 {
                operation,
                original_sha256,
            }
        } else {
            mage_entry::JournalV1::Started {
                operation,
                original_sha256,
            }
        })
    } else {
        None
    };
    let first = runner.mage_plans.len();
    runner.mage_plans.extend(plans);
    let range = first..runner.mage_plans.len();
    if let Some(start) = start {
        mage_entry::record(state, start);
    }
    Ok(range.collect())
}

pub(crate) fn stage_metamorph_mage_copy(
    state: &mut GameState,
    copy: mage_entry::MetamorphCopyV1,
) -> Result<(), String> {
    let runner = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_mut()
        .ok_or("Metamorph copy has no native entry replay owner")?;
    if runner
        .metamorph_mage_copies
        .iter()
        .any(|owned| owned.physical.source == copy.physical.source)
    {
        return Err("Metamorph copy already owns a prospective entry".into());
    }
    runner.metamorph_mage_copies.push(copy);
    Ok(())
}

pub(crate) fn metamorph_mage_copy(
    state: &GameState,
    object: crate::ids::ObjectId,
) -> Option<mage_entry::MetamorphCopyV1> {
    state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()?
        .metamorph_mage_copies
        .iter()
        .copied()
        .find(|copy| {
            copy.physical.source == object
                && copy.physical == crate::state::AbilitySourceContractV4::capture(state, object)
        })
}

fn mage_checkpoint_world(state: &GameState, runner: &RunnerV1) -> GameState {
    let mut snapshot = state.clone();
    if let Some(original) = runner.original_stack.last() {
        let absent = !snapshot
            .stack
            .iter()
            .any(|item| item.v4.stack_item_id == original.v4.stack_item_id);
        let departed = snapshot.engine.event_history[runner.original_history_len..].iter().any(|event|
            matches!(event, CommittedEvent::ZoneChange { object, from: Zone::Stack, .. } if *object == original.source));
        if absent && !departed {
            snapshot.stack.insert(
                runner
                    .original_stack
                    .len()
                    .saturating_sub(1)
                    .min(snapshot.stack.len()),
                original.clone(),
            );
        }
    }
    snapshot.engine.linked_aura_operation_v1 = StateV1::default();
    snapshot
}

pub(crate) fn resolve_show_and_tell(
    state: &mut GameState,
    pending: &crate::effect::EffectContinuation,
) -> Result<bool, String> {
    use crate::state::AbilitySourceContractV4;

    let runner = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .ok_or("Show and Tell has no native resolution owner")?;
    if !matches!(
        runner.operation,
        OperationV1::Advance | OperationV1::Step(_)
    ) {
        return Err("Show and Tell requires an actual resolution operation".into());
    }
    let original = runner
        .original_stack
        .last()
        .ok_or("Show and Tell lost its original stack row")?;
    let (producer, cast_at) = show_and_tell::capture_paid(state, pending, original)?;
    let item = pending.resolving_item.v4.stack_item_id;
    let mut entrants = Vec::new();
    for player in [state.active_player, state.active_player.opponent()] {
        let candidates = crate::eternal_cards_v1::e3::show_and_tell_cards(state, player)
            .iter()
            .map(|binding| AbilitySourceContractV4::capture(state, binding.object))
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            continue;
        }
        let Some(answer) = show_and_tell_answer(
            state,
            show_and_tell::ChoiceV1 {
                producer,
                player,
                physical: None,
                candidates,
            },
        )?
        else {
            return Ok(true);
        };
        entrants.extend(answer);
    }
    for entrant in &entrants {
        // spellbench-public-reveal: reveal selected cards only after both hand answers.
        state.reveal_hand_card(entrant.owner.opponent(), entrant.owner, entrant.source)?;
    }
    let mut proposals = Vec::new();
    for physical in entrants {
        let copy = if mage_entry::vial::is_physical_metamorph_in_hand(state, physical.source) {
            if let Some(choice) = show_and_tell::copy_choice(state, producer, physical)? {
                let Some(answer) = show_and_tell_answer(state, choice)? else {
                    return Ok(true);
                };
                answer.map(|copied| {
                    show_and_tell::prospective_copy(
                        state, producer, cast_at, item, physical, copied,
                    )
                })
            } else {
                None
            }
        } else {
            None
        };
        if let Some(copy) = copy {
            stage_metamorph_mage_copy(state, copy)?;
        }
        let ProposedEvent::ZoneChange(mut proposal) =
            ProposedEvent::zone_change(physical.source, Zone::Battlefield)
        else {
            unreachable!("Show and Tell moves selected hand cards");
        };
        crate::eternal_sideboard_s1a_v1::statics::replace_zone_change(state, &mut proposal);
        if let Some(copy) = copy.filter(|_| proposal.to_zone == Zone::Battlefield) {
            mage_entry::record(
                state,
                mage_entry::JournalV1::ShowAndTellMetamorphChosen { copy },
            );
        }
        proposals.push((physical, proposal));
    }
    if !proposals.is_empty() {
        let events = proposals
            .iter()
            .map(|(_, proposal)| ProposedEvent::ZoneChange(proposal.clone()))
            .collect();
        state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_mut()
            .unwrap()
            .show_and_tell_entries = proposals;
        crate::event::propose_and_commit_batch(state, events);
        state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_mut()
            .unwrap()
            .show_and_tell_entries
            .clear();
    }
    Ok(entry_checkpoint_in_flight_v1(state))
}

/// A Priest in the same batch cannot change the destination already
/// determined from the common pre-entry battlefield.
pub(crate) fn show_and_tell_entry_prepared(
    state: &GameState,
    change: &crate::event::ZoneChangeProposed,
) -> bool {
    state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .is_some_and(|runner| {
            runner
                .show_and_tell_entries
                .iter()
                .any(|(physical, prepared)| {
                    prepared == change
                        && *physical
                            == crate::state::AbilitySourceContractV4::capture(state, change.object)
                })
        })
}

fn show_and_tell_answer(
    state: &mut GameState,
    choice: show_and_tell::ChoiceV1,
) -> Result<Option<Option<crate::state::AbilitySourceContractV4>>, String> {
    let runner = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .ok_or("Show and Tell answer lost its native owner")?;
    if let Some(answer) = runner.answers.get(runner.cursor) {
        let GeneralAnswerV36::ShowAndTellV1 {
            choice: captured,
            selected,
        } = answer
        else {
            return Err("Show and Tell replay changed its answer kind".into());
        };
        if **captured != choice
            || selected.is_some_and(|source| !choice.candidates.contains(&source))
        {
            return Err("Show and Tell replay changed its actual answer".into());
        }
        let selected = *selected;
        state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_mut()
            .unwrap()
            .cursor += 1;
        return Ok(Some(selected));
    }
    let snapshot = mage_checkpoint_world(state, runner);
    let original_sha256 = runner.original_sha256.clone();
    state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_mut()
        .unwrap()
        .checkpoint = Some((
        Box::new(snapshot),
        GeneralChoiceV36::ShowAndTellV1 {
            choice: Box::new(choice),
            original_sha256,
        },
    ));
    Ok(None)
}

pub(crate) fn prepare_reanimated_metamorph(
    state: &mut GameState,
    ctx: &crate::effect::ExecCtx,
    object: crate::ids::ObjectId,
) -> Result<bool, String> {
    if !mage_entry::physical_graveyard_metamorph(state, object) || ctx.stack_item_id.is_none() {
        return Ok(true);
    }
    let ProposedEvent::ZoneChange(proposal) =
        ProposedEvent::zone_change_to_battlefield_under_controller(object, ctx.controller)
    else {
        unreachable!("Reanimate proposes a zone change");
    };
    if crate::eternal_legacy_e7a_v1::prevents_zone_change(state, &proposal) {
        return Ok(true);
    }
    let runner = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .ok_or("noncast Metamorph has no actual Reanimate replay owner")?;
    if !matches!(
        runner.operation,
        OperationV1::Advance | OperationV1::Step(_)
    ) || state
        .stack
        .iter()
        .any(|row| Some(row.v4.stack_item_id) == ctx.stack_item_id)
    {
        return Err("noncast Metamorph requires the actual resolving Reanimate".into());
    }
    let parent = runner
        .original_stack
        .last()
        .ok_or("noncast Metamorph lost its original Reanimate stack row")?;
    let Some(choice) = mage_entry::NoncastCopyChoiceV1::capture(state, ctx, object, parent)? else {
        return Ok(true);
    };
    prepare_noncast_metamorph(state, choice, false)
}

pub(crate) fn prepare_vial_metamorph(
    state: &mut GameState,
    pending: &crate::effect::EffectContinuation,
    paid_at: usize,
    object: crate::ids::ObjectId,
) -> Result<bool, String> {
    if !mage_entry::vial::is_physical_metamorph_in_hand(state, object) {
        return Ok(true);
    }
    let Some(choice) = mage_entry::vial::capture(state, pending, paid_at, object)? else {
        return Ok(true);
    };
    let runner = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .ok_or("Vial Metamorph has no native hand-answer owner")?;
    // The hand answer advances through a native answered-choice frame.
    if !matches!(&runner.operation, OperationV1::Advance) {
        return Err("Vial Metamorph lost its actual private hand answer".into());
    }
    prepare_noncast_metamorph(state, choice, true)
}

fn prepare_noncast_metamorph(
    state: &mut GameState,
    choice: mage_entry::NoncastCopyChoiceV1,
    vial: bool,
) -> Result<bool, String> {
    let runner = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .ok_or("noncast Metamorph has no entry owner")?;
    if let Some(answer) = runner.answers.get(runner.cursor) {
        let (expected, copied) = match (vial, answer) {
            (false, GeneralAnswerV36::ReanimateMetamorphV1 { choice, copied })
            | (true, GeneralAnswerV36::VialMetamorphV1 { choice, copied }) => (choice, copied),
            _ => return Err("Metamorph replay changed its confirmed copy choice kind".into()),
        };
        if **expected != choice || copied.is_some_and(|source| !choice.candidates.contains(&source))
        {
            return Err("Metamorph replay changed its confirmed copy candidate".into());
        }
        let copy = copied.map(|copied| choice.copy(copied));
        state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_mut()
            .unwrap()
            .cursor += 1;
        if let Some(copy) = copy {
            if vial {
                mage_entry::vial::record_hand_answer(state, copy)?;
            }
            stage_metamorph_mage_copy(state, copy)?;
        }
        return Ok(true);
    }
    let snapshot = mage_checkpoint_world(state, runner);
    let original_sha256 = runner.original_sha256.clone();
    let choice = Box::new(choice);
    let checkpoint = if vial {
        GeneralChoiceV36::VialMetamorphV1 {
            choice,
            original_sha256,
        }
    } else {
        GeneralChoiceV36::ReanimateMetamorphV1 {
            choice,
            original_sha256,
        }
    };
    state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_mut()
        .unwrap()
        .checkpoint = Some((Box::new(snapshot), checkpoint));
    Ok(false)
}

fn mage_answer(
    state: &mut GameState,
    choice: &mage_entry::ChoiceV1,
    group: Option<u16>,
) -> Result<Option<u16>, String> {
    let runner = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .ok_or("Meddling Mage answer lost its actual operation owner")?;
    if let Some(answer) = runner.answers.get(runner.cursor) {
        let GeneralAnswerV36::MeddlingMageV1 {
            choice: captured,
            group: prior_group,
            index,
        } = answer
        else {
            return Err("Meddling Mage replay changed its confirmed choice kind".into());
        };
        if captured.as_ref() != choice
            || *prior_group != group
            || !mage_entry::valid_index(choice, group, *index)
        {
            return Err("Meddling Mage replay changed its confirmed name choice".into());
        }
        let index = *index;
        state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_mut()
            .unwrap()
            .cursor += 1;
        return Ok(Some(index));
    }
    let snapshot = mage_checkpoint_world(state, runner);
    state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_mut()
        .unwrap()
        .checkpoint = Some((
        Box::new(snapshot),
        GeneralChoiceV36::MeddlingMageV1 {
            choice: Box::new(choice.clone()),
            group,
        },
    ));
    Ok(None)
}

fn answer_mage_plan(state: &mut GameState, index: usize) -> Result<bool, String> {
    let choice = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .unwrap()
        .mage_plans[index]
        .choice
        .clone();
    if choice.suppressed {
        return Ok(true);
    }
    let Some(group) = mage_answer(state, &choice, None)? else {
        return Ok(false);
    };
    let Some(name_index) = mage_answer(state, &choice, Some(group))? else {
        return Ok(false);
    };
    let named =
        mage_entry::name(&choice, group, name_index).ok_or("Meddling Mage name index absent")?;
    let operation = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .and_then(|runner| runner.mage_start)
        .ok_or("Meddling Mage choice operation absent")?;
    let chosen = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "Meddling Mage choice index overflow")?;
    mage_entry::record(
        state,
        mage_entry::JournalV1::Chosen {
            operation,
            choice: Box::new(choice),
            group,
            index: name_index,
        },
    );
    state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_mut()
        .unwrap()
        .mage_plans[index]
        .named = Some((chosen, named));
    Ok(true)
}

/// Capture the complete simultaneous roster before asking any owner.
/// All choices share APNAP ordering and no entry commits mid-menu.
pub(crate) fn prepare_entry_colors(
    state: &mut GameState,
    events: &[ProposedEvent],
) -> Result<bool, String> {
    if let Some(r) = state.engine.linked_aura_operation_v1.runtime.as_ref() {
        if r.checkpoint.is_some() {
            return Ok(false);
        }
        if let [event] = events {
            if r.color_plans
                .iter()
                .any(|p| p.committed.is_none() && p.color.is_some() && p.choice.proposal == *event)
                || r.kinship_plans.iter().any(|p| {
                    p.committed.is_none() && p.chosen.is_some() && p.choice.proposal == *event
                })
                || r.mage_plans.iter().any(|p| {
                    !p.committed
                        && (p.choice.suppressed || p.named.is_some())
                        && p.choice.proposal == *event
                })
            {
                return Ok(true);
            }
        }
    }
    let colors = capture_color_plans(state, events)?;
    let kinship = capture_kinship_plans(state, events)?;
    let mages = capture_mage_plans(state, events)?;
    if colors.is_empty() && kinship.is_empty() && mages.is_empty() {
        return Ok(true);
    }
    let r = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .ok_or("entry roster lost actual operation owner")?;
    let mut color_cursor = 0;
    let mut kinship_cursor = 0;
    let mut order = Vec::new();
    for (ordinal, event) in events.iter().enumerate() {
        if let Some(&index) = colors.get(color_cursor) {
            if r.color_plans[index].choice.proposal == *event {
                order.push((
                    r.color_plans[index].choice.controller != state.active_player,
                    ordinal,
                    0,
                    index,
                ));
                color_cursor += 1;
                continue;
            }
        }
        if let Some(&index) = kinship.get(kinship_cursor) {
            if r.kinship_plans[index].choice.proposal == *event {
                order.push((
                    r.kinship_plans[index].choice.controller != state.active_player,
                    ordinal,
                    1,
                    index,
                ));
                kinship_cursor += 1;
                continue;
            }
        }
    }
    for index in mages {
        let choice = &r.mage_plans[index].choice;
        order.push((
            choice.controller != state.active_player,
            choice.ordinal as usize,
            2,
            index,
        ));
    }
    if color_cursor != colors.len() || kinship_cursor != kinship.len() {
        return Err("entry roster lost original proposal ordering".into());
    }
    order.sort_unstable();
    for (_, _, kind, index) in order {
        let complete = match kind {
            0 => answer_color_plan(state, index)?,
            1 => answer_kinship_plan(state, index)?,
            _ => answer_mage_plan(state, index)?,
        };
        if !complete {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(crate) fn commit_mage_entry(
    state: &mut GameState,
    proposal: &ProposedEvent,
    object: crate::ids::ObjectId,
    entry: usize,
) -> Result<(), String> {
    let live = state.objects.get(object);
    if !mage_entry::owns_definition(live.card_def)
        || live.v4.face_down_v1.is_some()
        || live.v4.face_index != 0
    {
        return Ok(());
    }
    let runner = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .ok_or("Meddling Mage entry has no genuine replay owner")?;
    let index = runner
        .mage_plans
        .iter()
        .position(|p| !p.committed && p.choice.proposal == *proposal)
        .ok_or("Meddling Mage committed before capturing its actual entry")?;
    let plan = runner.mage_plans[index].clone();
    if !plan.choice.suppressed {
        let (chosen, named) = plan.named.ok_or("Meddling Mage entry was not named")?;
        mage_entry::commit(
            state,
            &plan.choice,
            runner
                .mage_start
                .ok_or("Meddling Mage operation marker absent")?,
            chosen,
            entry,
            object,
            named,
        )?;
    }
    state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_mut()
        .unwrap()
        .mage_plans[index]
        .committed = true;
    Ok(())
}

fn capture_kinship_plans(
    state: &mut GameState,
    events: &[ProposedEvent],
) -> Result<Vec<usize>, String> {
    use crate::banner_of_kinship_v1::entry::{self, JournalV1};
    let mut choices = Vec::new();
    for event in events {
        if let Some(c) = entry::choice(state, event, 0)? {
            choices.push(c);
        }
    }
    if choices.is_empty() {
        return Ok(Vec::new());
    }
    let r = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .ok_or("Kinship entry has no actual operation owner")?;
    if r.checkpoint.is_some() {
        return Err("Kinship capture after suspended operation".into());
    }
    let first = r.kinship_plans.len();
    if r.kinship_start.is_none() {
        let at = u32::try_from(state.engine.event_history.len())
            .map_err(|_| "Kinship operation index overflow")?;
        let operation = r.operation.clone();
        let digest = r.original_sha256.clone();
        state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_mut()
            .unwrap()
            .kinship_start = Some(at);
        state
            .engine
            .event_history
            .push(CommittedEvent::KinshipEntryV1 {
                journal: Box::new(JournalV1::Started {
                    operation: Box::new(operation),
                    original_sha256: digest,
                }),
            });
    }
    let operation = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .unwrap()
        .kinship_start
        .unwrap();
    for (offset, mut choice) in choices.into_iter().enumerate() {
        choice.ordinal =
            u32::try_from(first + offset).map_err(|_| "Kinship entry ordinal overflow")?;
        let capture = u32::try_from(state.engine.event_history.len())
            .map_err(|_| "Kinship capture index overflow")?;
        state
            .engine
            .event_history
            .push(CommittedEvent::KinshipEntryV1 {
                journal: Box::new(JournalV1::Captured {
                    operation,
                    choice: Box::new(choice.clone()),
                }),
            });
        state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_mut()
            .unwrap()
            .kinship_plans
            .push(KinshipPlanV1 {
                choice,
                capture,
                chosen: None,
                count: None,
                answer: None,
                entered: None,
                committed: None,
            });
    }
    Ok((first
        ..state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_ref()
            .unwrap()
            .kinship_plans
            .len())
        .collect())
}

fn answer_kinship_plan(state: &mut GameState, index: usize) -> Result<bool, String> {
    use crate::banner_of_kinship_v1::entry::{self, JournalV1};
    let r = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .unwrap();
    let plan = r.kinship_plans[index].clone();
    if let Some(answer) = r.answers.get(r.cursor).cloned() {
        let GeneralAnswerV36::KinshipV1 { choice, chosen } = answer else {
            return Err("Kinship replay changed actual choice kind".into());
        };
        if *choice != plan.choice {
            return Err("Kinship replay changed actual proposal/controller/source/scene".into());
        }
        let count = entry::amount(&choice, chosen)?;
        let at = u32::try_from(state.engine.event_history.len())
            .map_err(|_| "Kinship answer index overflow")?;
        state
            .engine
            .event_history
            .push(CommittedEvent::KinshipEntryV1 {
                journal: Box::new(JournalV1::Chosen {
                    capture: plan.capture,
                    chosen,
                    count,
                }),
            });
        let r = state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_mut()
            .unwrap();
        r.cursor += 1;
        r.kinship_plans[index].chosen = Some(chosen);
        r.kinship_plans[index].count = Some(count);
        r.kinship_plans[index].answer = Some(at);
        Ok(true)
    } else {
        let mut snapshot = state.clone();
        if let Some(original) = r.original_stack.last() {
            let absent = !snapshot
                .stack
                .iter()
                .any(|i| i.v4.stack_item_id == original.v4.stack_item_id);
            let departed=snapshot.engine.event_history[r.original_history_len..].iter().any(|e|matches!(e,CommittedEvent::ZoneChange{object,from:Zone::Stack,..}if *object==original.source));
            if absent && !departed {
                snapshot.stack.insert(
                    r.original_stack
                        .len()
                        .saturating_sub(1)
                        .min(snapshot.stack.len()),
                    original.clone(),
                );
            }
        }
        snapshot.engine.linked_aura_operation_v1 = StateV1::default();
        state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_mut()
            .unwrap()
            .checkpoint = Some((
            Box::new(snapshot),
            GeneralChoiceV36::KinshipV1 {
                choice: Box::new(plan.choice),
                capture: plan.capture,
            },
        ));
        Ok(false)
    }
}

/// Actual allocator/move already reset this incarnation. Install the chosen
/// type and all fellowship counters before any entry observer reads the scene.
pub(crate) fn initialize_kinship_entry(
    state: &mut GameState,
    proposal: &ProposedEvent,
    object: crate::ids::ObjectId,
) -> Result<(), String> {
    if !crate::banner_of_kinship_v1::entry::owns_entry_definition(
        state.objects.get(object).card_def,
    ) {
        return Ok(());
    }
    let r = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .ok_or("Kinship entry lacks native operation owner")?;
    let index = r
        .kinship_plans
        .iter()
        .position(|p| p.entered.is_none() && p.choice.proposal == *proposal)
        .ok_or("Kinship entry absent from original roster")?;
    let p = &r.kinship_plans[index];
    let chosen = p.chosen.ok_or("Kinship entry has no type answer")?;
    let count = p.count.ok_or("Kinship entry has no checked amount")?;
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield
        || live.controller != p.choice.controller
        || live.card_def != p.choice.card_def
        || live.v4.kinship_type_v1.is_some()
        || live.counters.fellowship != 0
    {
        return Err("Kinship actual allocated entry changed".into());
    }
    let link = crate::state::ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    state.objects.get_mut(object).v4.kinship_type_v1 = Some(chosen);
    state.objects.get_mut(object).counters.fellowship = count;
    state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_mut()
        .unwrap()
        .kinship_plans[index]
        .entered = Some(link);
    Ok(())
}

pub(crate) fn local_kinship_type(
    state: &GameState,
    object: crate::ids::ObjectId,
) -> Option<crate::creature_types_v60::CreatureTypeV60> {
    let r = state.engine.linked_aura_operation_v1.runtime.as_ref()?;
    let live = state.objects.try_get(object)?;
    let link = crate::state::ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    let mut plans = r.kinship_plans.iter().filter(|p| p.entered == Some(link));
    let p = plans.next()?;
    if plans.next().is_some()
        || live.v4.kinship_type_v1 != p.chosen
        || live.owner
            != p.choice
                .original
                .as_ref()
                .map_or(p.choice.controller, |o| o.owner)
    {
        return None;
    }
    p.chosen
}

pub(crate) fn commit_kinship_entry(
    state: &mut GameState,
    proposal: &ProposedEvent,
    object: crate::ids::ObjectId,
    entry: usize,
) -> Result<(), String> {
    use crate::banner_of_kinship_v1::entry::{EnteredV1, JournalV1};
    if !crate::banner_of_kinship_v1::entry::owns_entry_definition(
        state.objects.get(object).card_def,
    ) {
        return Ok(());
    }
    let r = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .ok_or("Kinship entry lost native owner")?;
    let index = r
        .kinship_plans
        .iter()
        .position(|p| {
            p.committed.is_none()
                && p.choice.proposal == *proposal
                && p.entered.is_some_and(|l| l.object == object)
        })
        .ok_or("Kinship entry lost exact allocated roster member")?;
    let p = &r.kinship_plans[index];
    let entered = EnteredV1 {
        operation: r.kinship_start.ok_or("Kinship operation absent")?,
        capture: p.capture,
        answer: p.answer.ok_or("Kinship chosen receipt absent")?,
        entry: u32::try_from(entry).map_err(|_| "Kinship physical entry index overflow")?,
        source: crate::state::AbilitySourceContractV4::capture(state, object),
        chosen: p.chosen.ok_or("Kinship type absent")?,
        count: p.count.ok_or("Kinship amount absent")?,
    };
    let marker = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "Kinship committed index overflow")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::KinshipEntryV1 {
            journal: Box::new(JournalV1::Committed { entered }),
        });
    state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_mut()
        .unwrap()
        .kinship_plans[index]
        .committed = Some(marker);
    Ok(())
}

/// Called after the actual allocator/move and before entry observers.
pub(crate) fn commit_entry_color(
    state: &mut GameState,
    proposal: &ProposedEvent,
    object: crate::ids::ObjectId,
    entry: usize,
) -> Result<(), String> {
    if !crate::heraldic_banner_v1::owns_definition(state.objects.get(object).card_def) {
        return Ok(());
    }
    let runner = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .ok_or("Banner committed entry lost its operation owner")?;
    let index = runner
        .color_plans
        .iter()
        .position(|p| p.committed.is_none() && p.choice.proposal == *proposal)
        .ok_or("Banner committed entry was not in its actual pre-entry roster")?;
    let plan = runner.color_plans[index].clone();
    let color = plan
        .color
        .ok_or("Banner entry committed before its choice")?;
    let entered = crate::heraldic_banner_v1::EnteredV1 {
        operation: runner
            .banner_start
            .ok_or("Banner entry lost operation occurrence")?,
        capture: plan.capture,
        answer: plan.answer.ok_or("Banner entry lost chosen occurrence")?,
        entry: u32::try_from(entry).map_err(|_| "Banner entry index overflow")?,
        object: crate::state::AbilitySourceContractV4::capture(state, object),
        color,
    };
    state.objects.get_mut(object).v4.chosen_color = Some(color);
    let marker =
        u32::try_from(state.engine.event_history.len()).map_err(|_| "Banner marker overflow")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::BannerEntryColorCommittedV1 {
            entered: Box::new(entered),
        });
    state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_mut()
        .unwrap()
        .color_plans[index]
        .committed = Some(marker);
    Ok(())
}

fn equipment_decision_v48(choice: &GeneralChoiceV36) -> Result<Decision, String> {
    match choice {
        GeneralChoiceV36::MeddlingMageV1 { choice, group } => mage_entry::decision(choice, *group),
        GeneralChoiceV36::ShowAndTellV1 { choice, .. } => Ok(Decision::ChooseEffectTargets {
            player: choice.player,
            source: choice.physical.unwrap_or(choice.producer).source,
            selected_count: 0,
            min_targets: 0,
            max_targets: 1,
            can_finish: true,
            legal_targets: choice
                .candidates
                .iter()
                .map(|source| Target::Object(source.source))
                .collect(),
        }),
        GeneralChoiceV36::ReanimateMetamorphV1 { choice, .. }
        | GeneralChoiceV36::VialMetamorphV1 { choice, .. } => Ok(Decision::ChooseEffectTargets {
            player: choice.producer.controller,
            source: choice.decision_source(),
            selected_count: 0,
            min_targets: 0,
            max_targets: 1,
            can_finish: true,
            legal_targets: choice
                .candidates
                .iter()
                .map(|source| Target::Object(source.source))
                .collect(),
        }),
        GeneralChoiceV36::KinshipV1 { choice, .. } => Ok(Decision::ChooseEntryCreatureTypeV1 {
            player: choice.controller,
            entry_ordinal: choice.ordinal,
            types: crate::creature_types_v60::CreatureTypeV60::ALL.to_vec(),
        }),
        GeneralChoiceV36::FakeAuraV50 { choice, .. } => Ok(Decision::ChooseEffectTargets {
            player: choice.owner,
            source: choice.aura.object,
            selected_count: 0,
            min_targets: 1,
            max_targets: 1,
            can_finish: false,
            legal_targets: choice
                .candidates
                .iter()
                .map(|host| Target::Object(host.object))
                .collect(),
        }),
        GeneralChoiceV36::TokenEquipmentV48 { choice, .. } => Ok(Decision::ChooseEffectTargets {
            player: choice.controller,
            source: choice.source.source,
            selected_count: 0,
            min_targets: 1,
            max_targets: 1,
            can_finish: false,
            legal_targets: choice
                .candidates
                .iter()
                .map(|c| Target::Object(c.object))
                .collect(),
        }),
        GeneralChoiceV36::Host { choice, .. } => Ok(Decision::ChooseEffectTargets {
            player: choice.owner,
            source: choice.aura.object,
            selected_count: 0,
            min_targets: 1,
            max_targets: 1,
            can_finish: false,
            legal_targets: choice
                .original_candidates
                .iter()
                .map(|c| Target::Object(c.object))
                .collect(),
        }),
        GeneralChoiceV36::Color { choice, .. } => Ok(Decision::ChooseEntryColorV36 {
            player: choice.controller,
            entry_ordinal: choice.ordinal,
            colors: crate::heraldic_banner_v1::COLORS,
        }),
    }
}
fn answer_equipment_checkpoint_v48(
    choice: &GeneralChoiceV36,
    action: Action,
) -> Result<GeneralAnswerV36, String> {
    match (choice, action) {
        (GeneralChoiceV36::ShowAndTellV1 { choice, .. }, action) => {
            let selected = match action {
                Action::FinishEffectSelection => None,
                Action::ChooseEffectTarget(Target::Object(object)) => Some(
                    choice
                        .candidates
                        .iter()
                        .find(|source| source.source == object)
                        .copied()
                        .ok_or("Show and Tell answer is outside its actual menu")?,
                ),
                _ => return Err("Show and Tell needs its actual choice answer".into()),
            };
            Ok(GeneralAnswerV36::ShowAndTellV1 {
                choice: choice.clone(),
                selected,
            })
        }
        (
            GeneralChoiceV36::ReanimateMetamorphV1 { choice: copy, .. }
            | GeneralChoiceV36::VialMetamorphV1 { choice: copy, .. },
            action,
        ) => {
            let copied = match action {
                Action::FinishEffectSelection => None,
                Action::ChooseEffectTarget(Target::Object(object)) => Some(
                    copy.candidates
                        .iter()
                        .find(|source| source.source == object)
                        .copied()
                        .ok_or("Metamorph copy answer is outside its original menu")?,
                ),
                _ => return Err("Metamorph copy needs a native choice answer".into()),
            };
            Ok(
                if matches!(choice, GeneralChoiceV36::VialMetamorphV1 { .. }) {
                    GeneralAnswerV36::VialMetamorphV1 {
                        choice: copy.clone(),
                        copied,
                    }
                } else {
                    GeneralAnswerV36::ReanimateMetamorphV1 {
                        choice: copy.clone(),
                        copied,
                    }
                },
            )
        }
        (GeneralChoiceV36::MeddlingMageV1 { choice, group }, Action::ChooseEffectOption(index))
            if mage_entry::valid_index(choice, *group, index) =>
        {
            Ok(GeneralAnswerV36::MeddlingMageV1 {
                choice: choice.clone(),
                group: *group,
                index,
            })
        }
        (GeneralChoiceV36::KinshipV1 { choice, .. }, Action::ChooseEffectOption(index)) => {
            Ok(GeneralAnswerV36::KinshipV1 {
                choice: choice.clone(),
                chosen: crate::creature_types_v60::CreatureTypeV60::from_choice_index(index)
                    .ok_or("Kinship type index outside full legal domain")?,
            })
        }
        (
            GeneralChoiceV36::FakeAuraV50 { choice, .. },
            Action::ChooseEffectTarget(Target::Object(id)),
        ) => Ok(GeneralAnswerV36::FakeAuraV50 {
            choice: choice.clone(),
            selected: *choice
                .candidates
                .iter()
                .find(|host| host.object == id)
                .ok_or("host outside original untargeted Fake Aura menu")?,
        }),
        (
            GeneralChoiceV36::TokenEquipmentV48 { choice, .. },
            Action::ChooseEffectTarget(Target::Object(id)),
        ) => Ok(GeneralAnswerV36::TokenEquipmentV48 {
            choice: choice.clone(),
            selected: *choice
                .candidates
                .iter()
                .find(|c| c.object == id)
                .ok_or("selected token outside complete original roster")?,
        }),
        (GeneralChoiceV36::Host { choice, .. }, Action::ChooseEffectTarget(Target::Object(id))) => {
            Ok(GeneralAnswerV36::Host(AnswerV1 {
                choice: choice.clone(),
                host: *choice
                    .original_candidates
                    .iter()
                    .find(|c| c.object == id)
                    .ok_or("host outside original menu")?,
            }))
        }
        (GeneralChoiceV36::Color { choice, .. }, Action::ChooseEffectOption(index)) => {
            Ok(GeneralAnswerV36::Color {
                choice: choice.clone(),
                color: *crate::heraldic_banner_v1::COLORS
                    .get(index as usize)
                    .ok_or("color outside WUBRG")?,
            })
        }
        _ => Err("only the exact current operation owner may answer".into()),
    }
}

pub(crate) fn choose_created_token_v48(
    state: &mut GameState,
    choice: crate::season_token_attachment_v48::ChoiceV48,
) -> Result<Option<EffectObjectBinding>, String> {
    crate::season_token_attachment_v48::validate_choice(state, &choice)?;
    let runner = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .ok_or("expanded token attachment has no genuine enclosing operation")?;
    if runner.checkpoint.is_some() {
        return Ok(None);
    }
    let input = crate::season_token_attachment_v48::InputV48 {
        original: runner
            .original_world_v48
            .clone()
            .ok_or("token attachment lost actual preoperation Season input")?,
        operation: runner.operation.clone(),
        prior_answers: runner.answers[..runner.cursor].to_vec(),
        choice: choice.clone(),
    };
    let answer = runner.answers.get(runner.cursor).cloned();
    let at = crate::season_token_attachment_v48::record(
        state,
        crate::season_token_attachment_v48::JournalV48::Input {
            input: Box::new(input),
        },
    );
    state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_mut()
        .unwrap()
        .attachment_input_v48 = Some(at);
    match answer {
        Some(GeneralAnswerV36::TokenEquipmentV48 {
            choice: original,
            selected,
        }) => {
            if *original != choice || !choice.candidates.contains(&selected) {
                return Err("confirmed token attachment changed original roster".into());
            }
            state
                .engine
                .linked_aura_operation_v1
                .runtime
                .as_mut()
                .unwrap()
                .cursor += 1;
            Ok(Some(selected))
        }
        Some(_) => Err("token attachment replay changed confirmed answer kind".into()),
        None => {
            let mut snapshot = state.clone();
            let runner = state
                .engine
                .linked_aura_operation_v1
                .runtime
                .as_ref()
                .unwrap();
            if let Some(original) = runner.original_stack.last() {
                let absent = !snapshot
                    .stack
                    .iter()
                    .any(|row| row.v4.stack_item_id == original.v4.stack_item_id);
                let departed = snapshot.engine.event_history[runner.original_history_len..].iter().any(|event|matches!(event,CommittedEvent::ZoneChange {object,from:Zone::Stack,..} if *object==original.source));
                if absent && !departed {
                    snapshot.stack.insert(
                        runner
                            .original_stack
                            .len()
                            .saturating_sub(1)
                            .min(snapshot.stack.len()),
                        original.clone(),
                    );
                }
            }
            snapshot.engine.linked_aura_operation_v1 = StateV1::default();
            state
                .engine
                .linked_aura_operation_v1
                .runtime
                .as_mut()
                .unwrap()
                .checkpoint = Some((
                Box::new(snapshot),
                GeneralChoiceV36::TokenEquipmentV48 {
                    choice: Box::new(choice),
                    input: at,
                },
            ));
            Ok(None)
        }
    }
}

/// The opaque loan is created only by the authenticated gained-death return.
pub(crate) fn install_fake_aura_entry_v50(
    state: &mut GameState,
    loan: crate::fake_your_own_death_v50::return_aura::EntryLoanV50,
) -> Result<(), String> {
    let runner = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_mut()
        .ok_or("Fake Aura entry lacks actual enclosing operation")?;
    if runner.active_fake_aura_entry_v50.is_some() {
        return Err("Fake Aura entry already has a private loan".into());
    }
    runner.active_fake_aura_entry_v50 = Some(loan);
    Ok(())
}
pub(crate) fn clear_fake_aura_entry_v50(state: &mut GameState) {
    if let Some(runner) = state.engine.linked_aura_operation_v1.runtime.as_mut() {
        runner.active_fake_aura_entry_v50 = None;
    }
}
pub(crate) fn fake_aura_entry_v50(
    state: &GameState,
) -> Option<&crate::fake_your_own_death_v50::return_aura::EntryLoanV50> {
    state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()?
        .active_fake_aura_entry_v50
        .as_ref()
}

/// Keep suspension distinct from the independently captured no-host result.
pub(crate) fn choose_fake_aura_v50(
    state: &mut GameState,
    choice: crate::fake_your_own_death_v50::return_aura::ChoiceV50,
) -> Result<
    (
        u64,
        crate::fake_your_own_death_v50::return_aura::SelectionV50,
    ),
    String,
> {
    use crate::fake_your_own_death_v50::return_aura::SelectionV50;
    crate::fake_your_own_death_v50::return_aura::validate_choice(state, &choice)?;
    let runner = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .ok_or("Fake Aura return has no genuine enclosing operation")?;
    if runner.checkpoint.is_some() {
        return Ok((0, SelectionV50::Suspended));
    }
    let input = crate::fake_your_own_death_v50::return_aura::InputV50 {
        original: runner
            .original_world_v50
            .clone()
            .ok_or("Fake Aura lost actual original operation input")?,
        operation: runner.operation.clone(),
        prior_answers: runner.answers[..runner.cursor].to_vec(),
        choice: choice.clone(),
    };
    let answer = runner.answers.get(runner.cursor).cloned();
    let at = crate::fake_your_own_death_v50::spell_custody::emit(
        state,
        crate::fake_your_own_death_v50::spell_custody::JournalV50::AuraReturnInput {
            input: Box::new(input),
        },
    );
    match choice.candidates.as_slice() {
        [] => return Ok((at, SelectionV50::NoHost)),
        [host] => return Ok((at, SelectionV50::Host(*host))),
        _ => {}
    }
    match answer {
        Some(GeneralAnswerV36::FakeAuraV50 {
            choice: original,
            selected,
        }) => {
            if *original != choice || !choice.candidates.contains(&selected) {
                return Err(
                    "Fake Aura replay changed its original complete menu or owner answer".into(),
                );
            }
            state
                .engine
                .linked_aura_operation_v1
                .runtime
                .as_mut()
                .unwrap()
                .cursor += 1;
            Ok((at, SelectionV50::Host(selected)))
        }
        Some(_) => Err("Fake Aura replay changed its confirmed answer kind".into()),
        None => {
            let mut snapshot = state.clone();
            let runner = state
                .engine
                .linked_aura_operation_v1
                .runtime
                .as_ref()
                .unwrap();
            let original = runner
                .original_stack
                .last()
                .ok_or("Fake Aura choice lost original stack top")?;
            if snapshot
                .stack
                .iter()
                .any(|row| row.v4.stack_item_id == original.v4.stack_item_id)
            {
                return Err(
                    "Fake Aura choice did not borrow the actually removed original row".into(),
                );
            }
            snapshot.stack.insert(
                runner
                    .original_stack
                    .len()
                    .saturating_sub(1)
                    .min(snapshot.stack.len()),
                original.clone(),
            );
            snapshot.engine.linked_aura_operation_v1 = StateV1::default();
            state
                .engine
                .linked_aura_operation_v1
                .runtime
                .as_mut()
                .unwrap()
                .checkpoint = Some((
                Box::new(snapshot),
                GeneralChoiceV36::FakeAuraV50 {
                    choice: Box::new(choice),
                    input: at,
                },
            ));
            Ok((at, SelectionV50::Suspended))
        }
    }
}

/// Genuine direct combat/counter operations use the same private whole-input owner.
pub(crate) fn run_drake_operation_v1(
    state: &mut GameState,
    operation: &OperationV1,
) -> Result<(), String> {
    crate::drake_hatcher_v1::custody::validate(state)?;
    let (next, _) = replay_general(state, operation, &[], false)?;
    *state = next;
    Ok(())
}
pub(crate) fn replay_drake_enclosing_v1(
    original: &GameState,
    operation: &OperationV1,
    answers: &[GeneralAnswerV36],
    used_banner: bool,
) -> Result<GameState, String> {
    replay_general(original, operation, answers, used_banner).map(|(world, _)| world)
}

/// Deliberately invalid saved-output mutation for the Drake memoization test.
/// This is not an operation owner or a production constructor. The runtime is
/// absent from serde but present in typed equality, so eligibility must fail.
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) fn tamper_drake_saved_output_runtime_for_test(state: &mut GameState) {
    assert!(!locally_owned(state));
    let runner = RunnerV1 {
        kinship_start: None,
        kinship_plans: Vec::new(),
        mage_start: None,
        mage_plans: Vec::new(),
        mage_names_v2: true,
        original_stack: state.stack.clone(),
        original_history_len: state.engine.event_history.len(),
        batch_depth: 0,
        deferred_departures: Vec::new(),
        answers: Vec::new(),
        cursor: 0,
        checkpoint: None,
        operation: OperationV1::Advance,
        original_sha256: original_sha256(state),
        banner_start: None,
        color_plans: Vec::new(),
        used_banner: false,
        original_world_v48: None,
        attachment_input_v48: None,
        original_world_v50: None,
        active_fake_aura_entry_v50: None,
        metamorph_mage_copies: Vec::new(),
        show_and_tell_entries: Vec::new(),
    };
    state.engine.linked_aura_operation_v1.runtime = Some(Box::new(runner));
}

mod genesis_current_slots_v69;
pub(crate) use genesis_current_slots_v69::sample_genesis_checkpoint_v69;
