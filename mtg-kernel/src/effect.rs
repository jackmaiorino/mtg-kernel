//! Interpreted effect programs.
//!
//! `EffectOp` is the only representation of card behavior: composition
//! primitives (`Sequence`, `Conditional`, `Choice`) plus a fixed
//! leaf-op vocabulary (`DealDamage`, `DrawCards`, `MoveObject`, library
//! partitioning, token creation, and other reusable state transitions).
//! There is no card-shaped op --
//! "Lightning Bolt" is not a variant, `DealDamage { amount: 3, .. }` is
//! (see `card_def.rs` / the generated `CARD_DEFS` table for how card
//! behavior handlers are wired up).
//!
//! `execute` and the resumable interpreter are the only paths that run an
//! `EffectOp`, and every leaf mutation goes through `event::propose_and_commit`
//! or `event::propose_and_commit_batch`, so nothing but the commit pipeline
//! (`event::commit`) mutates `GameState` in response to card behavior (see the
//! crate-level invariants in `lib.rs`).

use crate::card_def::{CardType, Subtype};
use crate::event;
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::state::{GameState, StackItem, Target, Zone};
use serde::{Deserialize, Serialize};

/// Which of a controller's creatures a team-wide pump/keyword effect
/// affects (`EffectOp::PumpControlled`). A closed, tiny enum rather than a
/// general subtype query: only the two shapes Rally's cards need exist
/// today (Goblin Bushwhacker's unfiltered "creatures you control", Rally at
/// the Hornburg's "Humans you control") -- a future card needing a
/// different subtype filter adds a `CreatureFilter` variant, reusing
/// `card_def::Subtype` (typed, not a string-contains check).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CreatureFilter {
    AnyControlled,
    ControlledWithSubtype(Subtype),
}

/// How long an impulse-drawn card (`EffectOp::ImpulseDraw`) stays playable
/// from exile. `EndOfTurn` is cleared unconditionally at the very next
/// `Step::Cleanup`, whoever's; `UntilOwnersNextTurn` survives through the
/// rest of this turn, the opponent's turn, and the owner's own next turn,
/// expiring at *that* turn's cleanup -- tracked via `engine::
/// PlayPermissionExpiry`, since a plain turn-number comparison can't tell
/// the owner's turn apart from the opponent's turn sharing the same kernel
/// round number (see that type's doc). Both durations are carried by an
/// `engine::PlayPermission`, not a pseudo-hand-zone membership list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImpulseDuration {
    EndOfTurn,
    UntilOwnersNextTurn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ObjectRef {
    /// The permanent/spell this effect program belongs to.
    ThisSource,
    /// A target resolved at cast/activation time, by index.
    Target(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlayerRef {
    Controller,
    Target(u8),
    /// The controller of a target object (e.g. "that creature's
    /// controller").
    ObjectController(ObjectRef),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TargetRef {
    ThisSource,
    Target(u8),
    /// The controller's one opponent. The kernel only ever simulates 1v1
    /// games (see `lib.rs`), so "deal N damage to each opponent"
    /// (Guttersnipe, Voldaren Epicure, Grab the Prize) never needs a
    /// chosen target -- it's always exactly `ctx.controller.opponent()`.
    Opponent,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EffectCond {
    Always,
    Never,
    /// True iff this cast's mandatory additional cost discarded a
    /// non-land card (Grab the Prize). Reads `ExecCtx::discarded`, which
    /// `engine::finalize_cast` populates from the additional-cost payment
    /// before pushing the spell onto the stack.
    DiscardedNonLandForCost,
    /// True iff `ctx.controller` had a land enter the battlefield under
    /// their control this turn (Searing Blaze's landfall clause). Reads
    /// `PlayerState::lands_played_this_turn`, which only tracks land
    /// *drops* -- an accurate proxy for this pool, since nothing in it puts
    /// a land onto the battlefield any other way.
    LandfallThisTurn,
    /// True iff `ctx.targets[idx]` is `Target::Object(id)` and that
    /// object is currently in `zone`. `false` for a `Target::Player` (no
    /// card in this pool needs that combination) or an out-of-range index.
    /// The general-purpose 608.2b "is this target still legal" fizzle
    /// check: a creature that died, a spell that already left the stack, or
    /// a permanent that's no longer on the battlefield all read `false`
    /// here, and the guarded effect they'd otherwise feed is skipped
    /// instead of misfiring against a stale `ObjectId`.
    TargetInZone(u8, Zone),
    /// True iff `ctx.targets[idx]` is a `Target::Object(id)` whose card
    /// definition's `colors` includes `color` (105.1/202.2). `false` for a
    /// `Target::Player`. Pyroblast's/Red Elemental Blast's "if it's
    /// blue"/"blue spell"/"blue permanent" checks.
    TargetIsColor(u8, crate::mana::ManaColor),
    /// Both sub-conditions must hold.
    And(Box<EffectCond>, Box<EffectCond>),
    /// Metalcraft (Galvanic Blast): true iff `ctx.controller` currently
    /// controls at least `n` permanents with `CardType::Artifact` (Great
    /// Furnace, Clockwork Percussionist, and Experimental Synthesizer are
    /// this pool's artifacts). A plain board-state count, not persisted
    /// anywhere -- recomputed fresh every time it's checked, same as
    /// `LandfallThisTurn`.
    ControlsArtifactCount(u8),
    /// True iff the cast this resolution's ETB trigger followed from was
    /// kicked (`card_def::CardDef::kicker_cost`, Goblin Bushwhacker's "if it
    /// was kicked" intervening-if). Reads `ExecCtx::kicked` -- cast-time
    /// metadata carried on the spell's `StackItem`, propagated to the ETB
    /// trigger's own `PendingTrigger`/`StackItem`/`ExecCtx` when it's
    /// queued, and gone once that trigger resolves (CR 702.33/601.2f: Kicker
    /// is a property of *this casting*, not a durable fact stored anywhere
    /// keyed by stable object id -- CR 400.7 zone changes create new
    /// objects, so a persistent id-keyed marker could falsely survive a
    /// later, unkicked cast of the same physical card if it were ever
    /// cleared incorrectly. Reading cast-scoped context instead of a lookup
    /// table makes that failure mode structurally impossible rather than
    /// merely avoided by careful clearing).
    WasKicked,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EffectOp {
    Sequence(Vec<EffectOp>),
    Conditional {
        cond: EffectCond,
        then: Box<EffectOp>,
        else_: Box<EffectOp>,
    },
    /// The selected player picks one of `options` during resolution. The
    /// generic resumable interpreter preserves printed option order and
    /// yields a policy-visible decision without opening a priority, SBA, or
    /// trigger window.
    Choice {
        controller: PlayerRef,
        options: Vec<EffectOp>,
    },
    DealDamage {
        target: TargetRef,
        amount: i32,
    },
    GainLife {
        player: PlayerRef,
        amount: i32,
    },
    LoseLife {
        player: PlayerRef,
        amount: i32,
    },
    DrawCards {
        player: PlayerRef,
        count: u32,
    },
    /// Publicly reveals a fixed snapshot of the top `count` cards of
    /// `player`'s library, then moves every card with `card_type` to
    /// `matching_to` and the remainder to `rest_to`. The matching group
    /// moves first and each group is one replacement-evaluated batch. A
    /// 2+ card graveyard group is explicitly ordered by its owner (the
    /// forced final card auto-completes); other groups retain snapshot order.
    /// This is not a draw: a short/empty library simply contributes fewer
    /// cards and never sets the draw-from-empty loss marker.
    RevealTopAndPartitionByType {
        player: PlayerRef,
        count: u8,
        card_type: CardType,
        matching_to: Zone,
        rest_to: Zone,
    },
    /// Discard `count` cards from `player`'s hand, chosen by that player.
    /// Unlike every other leaf, this one doesn't necessarily mutate state
    /// synchronously: `execute` stages `EngineState::pending_discard` and
    /// returns, and `engine::advance_until_decision` asks
    /// `Decision::Discard`. Because of that, **this must be the last leaf
    /// in any `Sequence` it appears in** (see `engine.rs`'s
    /// `pending_discard` doc for why: nothing after it in the same
    /// resolution would run before the decision is answered). The only
    /// user this increment, Faithless Looting ("draw two, then discard
    /// two"), satisfies this by construction.
    DiscardCards {
        player: PlayerRef,
        count: u32,
    },
    MoveObject {
        object: ObjectRef,
        to_zone: Zone,
    },
    TapObject {
        object: ObjectRef,
    },
    AddMana {
        player: PlayerRef,
        colors: Vec<ManaColor>,
    },
    /// Creates a fresh token permanent (e.g. Blood) directly on the
    /// battlefield under `controller`'s control. `token_def` indexes
    /// `card_def::CARD_DEFS` same as any other object -- tokens are real
    /// `GameObject`s, not a separate representation (see
    /// `event::ProposedEvent::create_token`).
    CreateToken {
        token_def: u16,
        controller: PlayerRef,
    },
    /// The controller may pay ONE of {discard `discard` cards, sacrifice
    /// `sacrifice_lands` lands} -- only whichever options are currently
    /// legal are offered, and declining is always legal too (Highway
    /// Robbery's `DoIfCostPaid(OrCost(DiscardCardCost, SacrificeTargetCost))`).
    /// If they do, `then` runs. Like `DiscardCards`, this is deferred:
    /// `execute` stages `EngineState::pending_optional_cost` and returns
    /// without knowing the outcome yet (`engine::Decision::ChooseOptionalCost`
    /// asks), so **this must be the last leaf in any `Sequence` it appears
    /// in**, same constraint and same reason as `DiscardCards`. A future
    /// card that needs both sub-costs simultaneously payable (not this
    /// pool) is out of scope: `discard`/`sacrifice_lands` are mutually
    /// exclusive choices, never both paid.
    MayPayCostThen {
        discard: u8,
        sacrifice_lands: u8,
        then: Box<EffectOp>,
    },
    /// "Deals `amount` damage to each opponent and each creature they
    /// control" (End the Festivities). The kernel only ever simulates 1v1
    /// games (see `lib.rs`), so "each opponent" is always exactly one
    /// player, same rationale as `TargetRef::Opponent`; no planeswalker
    /// card exists in the 132-card pool this increment adds to, so the
    /// "and each planeswalker they control" half of the real card's text
    /// is vacuously covered (there is never one to hit).
    DamageOpponentAndTheirCreatures {
        amount: i32,
    },
    /// A team-wide, until-end-of-turn pump/haste grant (Goblin Bushwhacker's
    /// kicked ETB, Rally at the Hornburg's token haste). Snapshots exactly
    /// which of `ctx.controller`'s current creatures match `filter` *at the
    /// moment this executes* (611.2c: the affected-objects set is locked in
    /// when the effect begins, not re-evaluated later) and stages an
    /// `engine::UntilEndOfTurnEffect::ResolvedSetEffect` naming those
    /// specific `ObjectId`s, cleared unconditionally at the next
    /// `Step::Cleanup` --
    /// see that variant's doc. Sequencing matters for Rally at the Hornburg:
    /// its own two `CreateToken` calls run *before* this in the same
    /// `Sequence`, so the freshly-created Human Soldier tokens are already
    /// on the battlefield (and therefore in the snapshot) by the time this
    /// leaf runs.
    PumpControlled {
        filter: CreatureFilter,
        power: i32,
        toughness: i32,
        grant_haste: bool,
    },
    /// Exiles the top `count` cards of `ctx.controller`'s library (silently
    /// stopping short if the library runs out first -- this is not a draw,
    /// so an empty library here is not a loss condition) and marks each one
    /// playable by its owner for `duration` -- Clockwork Percussionist's
    /// dies trigger, Experimental Synthesizer's enters-or-leaves trigger,
    /// and Reckless Impulse all reduce to this, differing only in `count`/
    /// `duration`. See `ImpulseDuration`'s doc for how each duration is
    /// tracked/expired (`engine::PlayPermission`), and `engine::
    /// castable_spells`/`engine::land_drop_candidates` for where the
    /// resulting exiled cards become legally castable/playable again,
    /// through the *ordinary* timing/cost/land-quota checks.
    ImpulseDraw {
        count: u8,
        duration: ImpulseDuration,
    },
    /// Chain Lightning's post-damage "that player or that permanent's
    /// controller may pay {R}{R}" branch. XMage asks this choice before it
    /// attempts payment, even when payment will fail, so this always
    /// suspends the current resolution in the engine's dedicated spell-copy
    /// state machine; no SBA, trigger placement, zone
    /// move, or priority window can occur until payment and optional
    /// retargeting finish. This leaf must remain last in Chain Lightning's
    /// generated sequence for the same reason `DiscardCards` must remain
    /// last in its sequence.
    OfferAffectedPlayerSpellCopy {
        affected: TargetRef,
    },
    /// Puts up to the top `count` cards of `player`'s library into their
    /// graveyard as one library-to-graveyard zone-change batch. This is not a draw: a
    /// short or empty library simply contributes fewer cards and never sets
    /// the draw-from-empty marker. If two or more cards would move together,
    /// their owner orders the batch through the resumable interpreter; the
    /// pending private-library identities are exposed only to that owner.
    /// This primitive does not yet emit a distinct pre-batch `MILL_CARDS`
    /// replacement event or a post-move mill-summary event; those hooks stay
    /// fail-closed until a supported pool card consumes them. Appended to
    /// preserve every existing variant's derived hash identity.
    MillCards {
        player: PlayerRef,
        count: u8,
    },
    /// Privately looks at up to the top `count` cards of `player`'s
    /// library and lets that same player put them back in any order. The
    /// interpreter binds the exact prefix/incarnations before yielding the
    /// ordered choice. AIRL/XMage presents each explicit pick as the next
    /// deepest card; the forced final card is therefore the new top card.
    /// Appended to preserve every existing variant's derived hash identity.
    LookAtLibraryTopAndReorder {
        player: PlayerRef,
        count: u8,
    },
    /// The selected player may shuffle their library. This is a real
    /// resolution-time Boolean choice even for a zero- or one-card library;
    /// accepting uses the state's deterministic shuffle stream and declining
    /// leaves both order and knowledge untouched. Appended for hash identity.
    MayShuffleLibrary {
        player: PlayerRef,
    },
    /// Repeatedly lets `player` choose one card from their current hand and
    /// puts that card on top of their library, stopping after `count` cards
    /// or when the hand is empty. Each card is a distinct private choice and
    /// zone change: the first chosen card is therefore deepest and the last
    /// chosen card is topmost. Appended to preserve existing hash identities.
    PutCardsFromHandOnLibraryTop {
        player: PlayerRef,
        count: u8,
    },
    /// Privately looks at the top `min(count, library.len())` cards of
    /// `player`'s library. The currently certified contract is only
    /// Preordain/Scry2 final-state semantics: `count > 2` fails before any
    /// library binding or reveal. The player chooses an unordered subset to
    /// put on the bottom, explicitly orders a 2-card bottom group
    /// shallow-to-deep, then explicitly orders a 2-card retained group
    /// deepest-to-topmost. The three private stages never open priority, SBA,
    /// or trigger windows; one atomic state transition applies the final
    /// top/tail/bottom order.
    ///
    /// No partial SCRY/SCRY_TO_BOTTOM/SCRIED event family is emitted yet.
    /// Arbitrary higher-count scry requires XMage-order bottom commitment plus
    /// typed hooks for those events, as does any supported replacement or
    /// trigger that observes them. Appended to preserve existing derived hash
    /// identities.
    Scry {
        player: PlayerRef,
        count: u8,
    },
}

/// One owned interpreter frame. `path` is the structural route through the
/// original effect program (sequence/branch/choice/group ordinals), making a
/// suspended continuation deterministic, hashable, and auditable without
/// storing closures or card-definition function pointers. Dynamic batch
/// frames are interpreter-owned: generated card programs only contain
/// `EffectOp`s.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EffectFrame {
    Program {
        op: EffectOp,
        path: Vec<u16>,
    },
    MoveObjectsBatch {
        objects: Vec<EffectObjectBinding>,
        to_zone: Zone,
        preserve_known_identity: bool,
        order_resolved: bool,
        path: Vec<u16>,
    },
    /// A bound library prefix awaiting its owner-selected graveyard order.
    /// Kept distinct from public reveal/partition batches so hidden milled
    /// identities stay chooser-private while the resolution is suspended.
    /// Appended to preserve existing continuation hashes.
    MillLibraryBatch {
        objects: Vec<EffectObjectBinding>,
        order_resolved: bool,
        path: Vec<u16>,
    },
    /// Commits one privately chosen ordering of an exact bound library
    /// prefix. `expected_prefix` preserves its pre-choice order separately
    /// from `ordered`, so a stale continuation cannot accept a same-set
    /// shuffle/reorder that incarnation-only validation would miss.
    ReorderLibraryTop {
        player: PlayerId,
        expected_prefix: Vec<EffectObjectBinding>,
        ordered: Vec<EffectObjectBinding>,
        path: Vec<u16>,
    },
    /// Executes an accepted optional shuffle on the next engine advance,
    /// rather than mutating during the action-answering call itself.
    ShuffleLibrary {
        player: PlayerId,
        path: Vec<u16>,
    },
    /// Coordinates one card at a time for a repeated private hand-to-library
    /// instruction. `chosen == None` stages the next exact-current-hand
    /// prompt; `Some` validates that prompt's hand snapshot and commits its
    /// single zone change before another prompt can be staged.
    PutCardsFromHandOnLibraryTop {
        player: PlayerId,
        /// Redundant copy of the originating op's requested count. Together
        /// with `remaining` and `prompt_index`, this makes trusted snapshot
        /// progress self-checking instead of trusting either counter alone.
        total: u8,
        remaining: u8,
        prompt_index: u16,
        expected_hand: Vec<EffectObjectBinding>,
        chosen: Option<EffectObjectBinding>,
        path: Vec<u16>,
        /// Redundant copy of the originating program path. Coordinator and
        /// prompt paths must remain mutually consistent with this copy.
        canonical_path: Vec<u16>,
    },
    /// Resumes one private scry after a completed policy stage. All original
    /// prefix bindings, requested-count metadata, and canonical structural
    /// path remain redundant in every progress state so a stale or malformed
    /// snapshot fails before the atomic library transition.
    ScryLibrary {
        player: PlayerId,
        requested_count: u8,
        original_library_len: u32,
        original_prefix: Vec<EffectObjectBinding>,
        progress: ScryProgress,
        /// Deterministic redundant commitment to `progress`. This is not an
        /// authentication boundary, but it makes any isolated progress-field
        /// corruption fail closed instead of silently selecting another valid
        /// subset/order.
        progress_fingerprint: u64,
        path: Vec<u16>,
        canonical_path: Vec<u16>,
    },
}

/// Completed private scry stages. A subset is canonicalized into original
/// prefix order before it enters this trusted frame, so the order in which
/// stage-one targets were selected can never leak into bottom ordering.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScryProgress {
    BottomSubsetChosen {
        bottom_subset: Vec<EffectObjectBinding>,
    },
    BottomOrderChosen {
        bottom_subset: Vec<EffectObjectBinding>,
        ordered_bottom: Vec<EffectObjectBinding>,
    },
    TopOrderChosen {
        bottom_subset: Vec<EffectObjectBinding>,
        ordered_bottom: Vec<EffectObjectBinding>,
        ordered_top: Vec<EffectObjectBinding>,
    },
}

/// Binds a physical arena id to the exact incarnation selected when an effect
/// snapshotted it. Visibility is governed separately: public reveals expose
/// these bindings to both players, while private library and hand choices
/// expose their otherwise-hidden bindings only to the chooser. A
/// restored/stale continuation must never move a later incarnation that
/// happens to reuse the same stable `ObjectId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EffectObjectBinding {
    pub object: ObjectId,
    pub expected_zone: Zone,
    pub expected_zone_change_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EffectTargetCandidate {
    pub target: Target,
    pub expected_object: Option<EffectObjectBinding>,
}

/// Internal reason/completion for a generic target-selection continuation.
/// Public schema-v4 projects these through already-reserved card-selection or
/// library-order purposes; no card-specific state or action identity is
/// introduced.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EffectTargetSelectionPurpose {
    OrderIntoGraveyard {
        preserve_known_identity: bool,
    },
    /// Orders a bound, otherwise-hidden library prefix for a mill batch.
    /// Only the milled cards' owner may inspect candidate identities while
    /// the choice is pending; the completed graveyard remains public.
    OrderMilledIntoGraveyard,
    /// Orders a privately looked-at library prefix. The original ordered
    /// bindings are retained independently from the mutable selected/legal
    /// partition so restore/tamper validation can require the exact prefix,
    /// not merely the same set of cards.
    OrderLookedLibraryTop {
        player: PlayerId,
        original_prefix: Vec<EffectObjectBinding>,
    },
    /// One of a repeated series of private, exact-one hand choices. The
    /// complete hand snapshot prevents a restored continuation from silently
    /// accepting a changed candidate pool. The next prompt is independent:
    /// after this choice commits, it snapshots the then-current hand anew.
    PutHandCardOnLibraryTop {
        player: PlayerId,
        original_hand: Vec<EffectObjectBinding>,
        total: u8,
        remaining: u8,
        prompt_index: u16,
        continuation_path: Vec<u16>,
        canonical_path: Vec<u16>,
    },
    /// One of the three private scry prompts. Stage one is an unordered,
    /// variable-size card selection; stages two and three are exact library
    /// orderings. Schema-v4 projects these through its existing
    /// CardSelection/LibraryOrder purposes and redacts identities from the
    /// non-chooser.
    ScryLibrary {
        player: PlayerId,
        requested_count: u8,
        original_library_len: u32,
        original_prefix: Vec<EffectObjectBinding>,
        stage: ScrySelectionStage,
        /// Redundant deterministic commitment to `stage`; see the frame's
        /// progress fingerprint for the trusted-snapshot threat boundary.
        stage_fingerprint: u64,
        canonical_path: Vec<u16>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScrySelectionStage {
    ChooseBottomSubset,
    OrderBottom {
        bottom_subset: Vec<EffectObjectBinding>,
    },
    OrderRetainedTop {
        bottom_subset: Vec<EffectObjectBinding>,
        ordered_bottom: Vec<EffectObjectBinding>,
    },
}

/// Internal completion semantics for a generic Boolean effect choice.
/// Public schema-v4 projects the shuffle use through its already-reserved
/// `BooleanChoicePurposeV4::Shuffle` variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EffectBooleanChoicePurpose {
    ShuffleLibrary { player: PlayerId },
}

/// A policy-visible choice yielded by the generic effect interpreter. This is
/// intentionally typed and extensible: later library ordering, subset, Ward,
/// and Escape choices add variants here instead of adding card-specific
/// `EngineState::pending_*` fields.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PendingEffectChoice {
    ChooseOption {
        player: PlayerId,
        path: Vec<u16>,
        options: Vec<EffectOp>,
    },
    SelectTargets {
        player: PlayerId,
        path: Vec<u16>,
        selected: Vec<EffectTargetCandidate>,
        legal: Vec<EffectTargetCandidate>,
        min_targets: u16,
        max_targets: u16,
        ordered: bool,
        purpose: EffectTargetSelectionPurpose,
    },
    ChooseBoolean {
        player: PlayerId,
        path: Vec<u16>,
        default: Option<bool>,
        purpose: EffectBooleanChoicePurpose,
    },
}

impl PendingEffectChoice {
    pub fn player(&self) -> PlayerId {
        match self {
            PendingEffectChoice::ChooseOption { player, .. } => *player,
            PendingEffectChoice::SelectTargets { player, .. } => *player,
            PendingEffectChoice::ChooseBoolean { player, .. } => *player,
        }
    }

    pub fn structural_path(&self) -> &[u16] {
        match self {
            PendingEffectChoice::ChooseOption { path, .. }
            | PendingEffectChoice::SelectTargets { path, .. }
            | PendingEffectChoice::ChooseBoolean { path, .. } => path,
        }
    }
}

/// Full in-state continuation for one resolving stack item. The complete
/// `StackItem`, execution context, remaining frames, and active typed choice
/// all participate in clone/equality/hash/serde, so snapshot/restore cannot
/// lose or alias a mid-resolution decision.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EffectContinuation {
    pub resolving_item: StackItem,
    pub ctx: ExecCtx,
    pub frames: Vec<EffectFrame>,
    pub choice: Option<PendingEffectChoice>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumableProgress {
    Complete(StackItem),
    Suspended,
}

/// Everything an effect program needs to resolve symbolic refs against a
/// concrete game: which object it's running for, who controls it, and the
/// targets chosen when it was cast/activated.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExecCtx {
    pub source: ObjectId,
    pub controller: PlayerId,
    pub targets: Vec<Target>,
    /// Cards discarded to pay this cast's mandatory additional cost (Grab
    /// the Prize), if any. Empty for everything else. Read by
    /// `EffectCond::DiscardedNonLandForCost`.
    pub discarded: Vec<ObjectId>,
    /// True iff the spell/ability this resolution belongs to was kicked
    /// (`card_def::CardDef::kicker_cost`) -- carried on `state::StackItem::
    /// kicked` and copied in here by `engine::resolve_top_of_stack`, and
    /// (for the ETB trigger this spell's own resolution queues) propagated
    /// onto that trigger's own `trigger::PendingTrigger`/`StackItem` in turn.
    /// Read by `EffectCond::WasKicked`. `false` for every card without
    /// Kicker (the overwhelming majority), and for any ability/trigger not
    /// downstream of a kicked cast.
    pub kicked: bool,
}

/// Whether this program can yield a policy-visible choice anywhere in its
/// tree. Existing Burn/Rally programs stay on their frozen synchronous/legacy
/// continuation paths; explicit `Choice`, public partition ordering,
/// private library/hand ordering, and multi-card mill ordering enter the v4
/// interpreter.
pub fn contains_player_choice(op: &EffectOp) -> bool {
    match op {
        EffectOp::Sequence(ops) => ops.iter().any(contains_player_choice),
        EffectOp::Conditional { then, else_, .. } => {
            contains_player_choice(then) || contains_player_choice(else_)
        }
        EffectOp::Choice { options, .. } => {
            options.len() > 1 || options.iter().any(contains_player_choice)
        }
        // Whether the decision is needed depends on the revealed cards, but
        // the program must enter the resumable interpreter so a 2+ card
        // graveyard batch can yield its owner's ordering choice.
        EffectOp::RevealTopAndPartitionByType { .. } => true,
        EffectOp::MillCards { count, .. } => *count > 1,
        EffectOp::LookAtLibraryTopAndReorder { .. }
        | EffectOp::MayShuffleLibrary { .. }
        | EffectOp::PutCardsFromHandOnLibraryTop { .. }
        | EffectOp::Scry { .. } => true,
        _ => false,
    }
}

/// Starts a choice-bearing stack resolution and runs synchronously until it
/// either completes or yields a real player choice. Legacy-suspending leaves
/// are rejected up front in this first v4 slice: mixing them with remaining
/// frames would change the already-certified Burn/Rally completion timing.
pub fn begin_resumable_resolution(
    op: &EffectOp,
    ctx: &ExecCtx,
    resolving_item: StackItem,
    state: &mut GameState,
) -> Result<ResumableProgress, String> {
    if state.engine.pending_effect.is_some() {
        return Err("cannot begin an effect while another continuation is pending".to_string());
    }
    validate_resumable_program(op)?;
    state.engine.pending_effect = Some(EffectContinuation {
        resolving_item,
        ctx: ctx.clone(),
        frames: vec![EffectFrame::Program {
            op: op.clone(),
            path: Vec::new(),
        }],
        choice: None,
    });
    drive_resumable(state)
}

/// Resumes after `choose_resumable_option` installed the selected branch.
pub fn resume_resumable_resolution(state: &mut GameState) -> Result<ResumableProgress, String> {
    if state.engine.pending_effect.is_none() {
        return Err("no effect continuation is pending".to_string());
    }
    drive_resumable(state)
}

/// Records one selected option without executing it. The next engine advance
/// owns resumption, preserving the usual step/advance separation and making a
/// snapshot taken immediately after the action deterministic too.
pub fn choose_resumable_option(state: &mut GameState, option_index: u16) -> Result<(), String> {
    let continuation = state
        .engine
        .pending_effect
        .as_mut()
        .ok_or("no effect continuation is pending")?;
    let choice = continuation
        .choice
        .take()
        .ok_or("the pending effect is not waiting for a choice")?;
    match choice {
        PendingEffectChoice::ChooseOption {
            player,
            mut path,
            options,
        } => {
            let index = option_index as usize;
            let Some(selected) = options.get(index).cloned() else {
                continuation.choice = Some(PendingEffectChoice::ChooseOption {
                    player,
                    path,
                    options,
                });
                return Err(format!(
                    "effect option {option_index} is outside the available range"
                ));
            };
            path.push(option_index);
            continuation
                .frames
                .push(EffectFrame::Program { op: selected, path });
            Ok(())
        }
        PendingEffectChoice::SelectTargets { .. } | PendingEffectChoice::ChooseBoolean { .. } => {
            continuation.choice = Some(choice);
            Err("the pending effect is not waiting for an option".to_string())
        }
    }
}

/// Records one target in an effect-owned selection. Exact-count selections
/// auto-append every forced remaining target, so an N-card ordering exposes
/// only N-1 picks. Validation precedes mutation, making stale, duplicate, or
/// wrong-shape actions byte-for-byte nonmutating.
pub fn choose_resumable_target(state: &mut GameState, target: Target) -> Result<(), String> {
    validate_pending_effect_choice(state)?;
    let choice = state
        .engine
        .pending_effect
        .as_ref()
        .and_then(|pending| pending.choice.as_ref())
        .ok_or("no effect continuation choice is pending")?;
    let PendingEffectChoice::SelectTargets {
        legal,
        selected,
        max_targets,
        ..
    } = choice
    else {
        return Err("the pending effect is not waiting for a target selection".to_string());
    };
    if selected.len() >= usize::from(*max_targets) {
        return Err("the pending effect target selection is already full".to_string());
    }
    let Some(position) = legal
        .iter()
        .position(|candidate| candidate.target == target)
    else {
        return Err(format!("{target:?} is not a legal remaining effect target"));
    };
    validate_effect_target_candidate(state, &legal[position])?;

    let continuation = state.engine.pending_effect.as_mut().unwrap();
    let PendingEffectChoice::SelectTargets {
        selected,
        legal,
        min_targets,
        max_targets,
        ordered,
        ..
    } = continuation.choice.as_mut().unwrap()
    else {
        unreachable!("validated target-selection choice above")
    };
    selected.push(legal.remove(position));

    let required = usize::from(*min_targets).saturating_sub(selected.len());
    if (*ordered && required == 1 && legal.len() == 1)
        || (!*ordered && required > 0 && required == legal.len())
    {
        selected.append(legal);
    }
    if selected.len() == usize::from(*max_targets) {
        complete_resumable_target_selection(continuation)?;
    }
    Ok(())
}

/// Finishes a generic variable-count selection once its minimum has been
/// met. Winding Way's graveyard ordering has `min == max`, so its forced
/// final card auto-completes and this action is never legal there.
pub fn finish_resumable_target_selection(state: &mut GameState) -> Result<(), String> {
    validate_pending_effect_choice(state)?;
    let choice = state
        .engine
        .pending_effect
        .as_ref()
        .and_then(|pending| pending.choice.as_ref())
        .ok_or("no effect continuation choice is pending")?;
    let PendingEffectChoice::SelectTargets {
        selected,
        min_targets,
        ..
    } = choice
    else {
        return Err("the pending effect is not waiting for a target selection".to_string());
    };
    if selected.len() < usize::from(*min_targets) {
        return Err("the pending effect target selection has not reached its minimum".to_string());
    }
    complete_resumable_target_selection(state.engine.pending_effect.as_mut().unwrap())
}

/// Records one generic Boolean answer without executing its consequence.
/// The next engine advance owns the accepted shuffle, keeping `step()` a
/// pure continuation transition and making post-action snapshots stable.
pub fn choose_resumable_boolean(state: &mut GameState, value: bool) -> Result<(), String> {
    validate_pending_effect_choice(state)?;
    let continuation = state
        .engine
        .pending_effect
        .as_mut()
        .ok_or("no effect continuation is pending")?;
    let choice = continuation
        .choice
        .take()
        .ok_or("the pending effect is not waiting for a choice")?;
    match choice {
        PendingEffectChoice::ChooseBoolean {
            player,
            mut path,
            default,
            purpose,
        } => {
            path.push(u16::from(value));
            match purpose {
                EffectBooleanChoicePurpose::ShuffleLibrary {
                    player: library_player,
                } => {
                    if player != library_player {
                        continuation.choice = Some(PendingEffectChoice::ChooseBoolean {
                            player,
                            path,
                            default,
                            purpose,
                        });
                        return Err(
                            "shuffle choice player does not own the selected library".to_string()
                        );
                    }
                    if value {
                        continuation.frames.push(EffectFrame::ShuffleLibrary {
                            player: library_player,
                            path,
                        });
                    }
                }
            }
            Ok(())
        }
        PendingEffectChoice::ChooseOption { .. } | PendingEffectChoice::SelectTargets { .. } => {
            continuation.choice = Some(choice);
            Err("the pending effect is not waiting for a Boolean choice".to_string())
        }
    }
}

fn complete_resumable_target_selection(
    continuation: &mut EffectContinuation,
) -> Result<(), String> {
    let choice = continuation
        .choice
        .take()
        .ok_or("no effect continuation choice is pending")?;
    let PendingEffectChoice::SelectTargets {
        path,
        selected,
        purpose,
        ..
    } = choice
    else {
        continuation.choice = Some(choice);
        return Err("the pending effect is not waiting for a target selection".to_string());
    };
    let mut objects = selected
        .into_iter()
        .map(|candidate| {
            let binding = candidate.expected_object.ok_or_else(|| {
                "zone-order selection target lacks an object-incarnation binding".to_string()
            })?;
            if candidate.target != Target::Object(binding.object) {
                return Err("zone-order selection target/binding mismatch".to_string());
            }
            Ok(binding)
        })
        .collect::<Result<Vec<_>, String>>()?;
    match purpose {
        EffectTargetSelectionPurpose::OrderIntoGraveyard {
            preserve_known_identity,
        } => continuation.frames.push(EffectFrame::MoveObjectsBatch {
            objects,
            to_zone: Zone::Graveyard,
            preserve_known_identity,
            order_resolved: true,
            path,
        }),
        EffectTargetSelectionPurpose::OrderMilledIntoGraveyard => {
            continuation.frames.push(EffectFrame::MillLibraryBatch {
                objects,
                order_resolved: true,
                path,
            });
        }
        EffectTargetSelectionPurpose::OrderLookedLibraryTop {
            player,
            original_prefix,
        } => {
            // AIRL's ordered-card chooser treats the first explicit pick as
            // deepest and the forced final card as topmost. `selected` is in
            // pick order, while the state library is top-to-bottom.
            objects.reverse();
            continuation.frames.push(EffectFrame::ReorderLibraryTop {
                player,
                expected_prefix: original_prefix,
                ordered: objects,
                path,
            });
        }
        EffectTargetSelectionPurpose::PutHandCardOnLibraryTop {
            player,
            original_hand,
            total,
            remaining,
            prompt_index,
            continuation_path,
            canonical_path,
        } => {
            if objects.len() != 1 {
                return Err("hand-to-library prompt did not select exactly one card".to_string());
            }
            validate_hand_to_library_progress(
                total,
                remaining,
                prompt_index,
                &continuation_path,
                &canonical_path,
            )?;
            if remaining == 0 {
                return Err("completed hand-to-library progress cannot own a prompt".to_string());
            }
            let mut expected_choice_path = canonical_path.clone();
            expected_choice_path.push(prompt_index);
            if path != expected_choice_path {
                return Err("hand-to-library prompt structural path changed".to_string());
            }
            continuation
                .frames
                .push(EffectFrame::PutCardsFromHandOnLibraryTop {
                    player,
                    total,
                    remaining,
                    prompt_index,
                    expected_hand: original_hand,
                    chosen: objects.pop(),
                    path: continuation_path,
                    canonical_path,
                });
        }
        EffectTargetSelectionPurpose::ScryLibrary {
            player,
            requested_count,
            original_library_len,
            original_prefix,
            stage,
            stage_fingerprint,
            canonical_path,
        } => {
            validate_scry_bound_metadata(requested_count, original_library_len, &original_prefix)?;
            if stage_fingerprint != scry_stage_fingerprint(&stage) {
                return Err("scry prompt stage fingerprint changed".to_string());
            }
            let mut expected_choice_path = canonical_path.clone();
            expected_choice_path.push(scry_stage_tag(&stage));
            if path != expected_choice_path {
                return Err("scry prompt structural path changed".to_string());
            }
            let progress = match stage {
                ScrySelectionStage::ChooseBottomSubset => {
                    let bottom_subset = canonicalize_scry_subset(&original_prefix, &objects)?;
                    ScryProgress::BottomSubsetChosen { bottom_subset }
                }
                ScrySelectionStage::OrderBottom { bottom_subset } => {
                    validate_exact_binding_permutation(
                        &bottom_subset,
                        &objects,
                        "scry bottom order",
                    )?;
                    ScryProgress::BottomOrderChosen {
                        bottom_subset,
                        ordered_bottom: objects,
                    }
                }
                ScrySelectionStage::OrderRetainedTop {
                    bottom_subset,
                    ordered_bottom,
                } => {
                    validate_exact_binding_permutation(
                        &bottom_subset,
                        &ordered_bottom,
                        "scry ordered bottom",
                    )?;
                    let retained = scry_retained_prefix(&original_prefix, &bottom_subset)?;
                    validate_exact_binding_permutation(
                        &retained,
                        &objects,
                        "scry retained-top order",
                    )?;
                    // Like Ponder, this prompt's first explicit selection is
                    // deepest and the forced final card is topmost.
                    objects.reverse();
                    ScryProgress::TopOrderChosen {
                        bottom_subset,
                        ordered_bottom,
                        ordered_top: objects,
                    }
                }
            };
            let progress_fingerprint = scry_progress_fingerprint(&progress);
            continuation.frames.push(EffectFrame::ScryLibrary {
                player,
                requested_count,
                original_library_len,
                original_prefix,
                progress,
                progress_fingerprint,
                path: canonical_path.clone(),
                canonical_path,
            });
        }
    }
    Ok(())
}

fn validate_effect_target_candidate(
    state: &GameState,
    candidate: &EffectTargetCandidate,
) -> Result<(), String> {
    let Some(binding) = candidate.expected_object else {
        return Ok(());
    };
    if candidate.target != Target::Object(binding.object) {
        return Err("effect target/binding mismatch".to_string());
    }
    let object = state
        .objects
        .try_get(binding.object)
        .ok_or_else(|| format!("effect target object {} no longer exists", binding.object.0))?;
    if object.zone != binding.expected_zone
        || object.zone_change_count != binding.expected_zone_change_count
    {
        return Err(format!(
            "effect target object {} changed incarnation: expected {:?}/{} but found {:?}/{}",
            binding.object.0,
            binding.expected_zone,
            binding.expected_zone_change_count,
            object.zone,
            object.zone_change_count
        ));
    }
    Ok(())
}

pub fn validate_pending_effect_choice(state: &GameState) -> Result<(), String> {
    let Some(choice) = state
        .engine
        .pending_effect
        .as_ref()
        .and_then(|pending| pending.choice.as_ref())
    else {
        return Ok(());
    };
    match choice {
        PendingEffectChoice::SelectTargets {
            player: chooser,
            path,
            selected,
            legal,
            min_targets,
            max_targets,
            ordered,
            purpose,
        } => {
            for candidate in selected.iter().chain(legal) {
                validate_effect_target_candidate(state, candidate)?;
            }
            match purpose {
                EffectTargetSelectionPurpose::OrderMilledIntoGraveyard => {
                    let bindings = selected
                        .iter()
                        .chain(legal)
                        .map(|candidate| {
                            candidate.expected_object.ok_or_else(|| {
                                "milled-card ordering target lacks an object-incarnation binding"
                                    .to_string()
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    validate_bound_library_prefix(state, &bindings)?;
                    let library_owner = bindings
                        .first()
                        .map(|binding| state.objects.get(binding.object).owner)
                        .ok_or_else(|| {
                            "milled-card ordering choice has no bound library objects".to_string()
                        })?;
                    if *chooser != library_owner {
                        return Err(
                            "milled-card ordering player does not own the selected library"
                                .to_string(),
                        );
                    }
                }
                EffectTargetSelectionPurpose::OrderLookedLibraryTop {
                    player: library_player,
                    original_prefix,
                } => {
                    if chooser != library_player {
                        return Err(
                            "library-order choice player does not own the selected library"
                                .to_string(),
                        );
                    }
                    validate_bound_library_prefix_exact(state, *library_player, original_prefix)?;
                    let mut partition = selected
                        .iter()
                        .chain(legal)
                        .map(|candidate| {
                            candidate.expected_object.ok_or_else(|| {
                                "library-order target lacks an object-incarnation binding"
                                    .to_string()
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    let mut original = original_prefix.clone();
                    partition.sort_by_key(|binding| binding.object);
                    original.sort_by_key(|binding| binding.object);
                    if partition != original {
                        return Err("library-order candidates do not partition the bound prefix"
                            .to_string());
                    }
                }
                EffectTargetSelectionPurpose::PutHandCardOnLibraryTop {
                    player: hand_player,
                    original_hand,
                    total,
                    remaining,
                    prompt_index,
                    continuation_path,
                    canonical_path,
                } => {
                    if chooser != hand_player {
                        return Err(
                            "hand-to-library choice player does not own the selected hand"
                                .to_string(),
                        );
                    }
                    validate_hand_to_library_progress(
                        *total,
                        *remaining,
                        *prompt_index,
                        continuation_path,
                        canonical_path,
                    )?;
                    if *remaining == 0 || original_hand.len() < 2 {
                        return Err(
                            "hand-to-library policy prompt has no genuine choice".to_string()
                        );
                    }
                    if *min_targets != 1 || *max_targets != 1 || !*ordered || !selected.is_empty() {
                        return Err(
                            "hand-to-library prompt is not an independent exact-one ordering choice"
                                .to_string(),
                        );
                    }
                    let mut expected_path = canonical_path.clone();
                    expected_path.push(*prompt_index);
                    if path != &expected_path {
                        return Err("hand-to-library prompt structural path changed".to_string());
                    }
                    validate_bound_hand_exact(state, *hand_player, original_hand)?;
                    let mut partition = selected
                        .iter()
                        .chain(legal)
                        .map(|candidate| {
                            candidate.expected_object.ok_or_else(|| {
                                "hand-to-library target lacks an object-incarnation binding"
                                    .to_string()
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    let mut original = original_hand.clone();
                    partition.sort_by_key(|binding| binding.object);
                    original.sort_by_key(|binding| binding.object);
                    if partition != original {
                        return Err("hand-to-library candidates do not partition the bound hand"
                            .to_string());
                    }
                }
                EffectTargetSelectionPurpose::ScryLibrary {
                    player: library_player,
                    requested_count,
                    original_library_len,
                    original_prefix,
                    stage,
                    stage_fingerprint,
                    canonical_path,
                } => {
                    if chooser != library_player {
                        return Err(
                            "scry choice player does not own the selected library".to_string()
                        );
                    }
                    validate_scry_live_metadata(
                        state,
                        *library_player,
                        *requested_count,
                        *original_library_len,
                        original_prefix,
                    )?;
                    if *stage_fingerprint != scry_stage_fingerprint(stage) {
                        return Err("scry prompt stage fingerprint changed".to_string());
                    }
                    let mut expected_path = canonical_path.clone();
                    expected_path.push(scry_stage_tag(stage));
                    if path != &expected_path {
                        return Err("scry prompt structural path changed".to_string());
                    }
                    let candidates = selected
                        .iter()
                        .chain(legal)
                        .map(|candidate| {
                            candidate.expected_object.ok_or_else(|| {
                                "scry target lacks an object-incarnation binding".to_string()
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    match stage {
                        ScrySelectionStage::ChooseBottomSubset => {
                            let count = u16::try_from(original_prefix.len())
                                .map_err(|_| "scry prefix exceeds u16".to_string())?;
                            if *min_targets != 0 || *max_targets != count || *ordered {
                                return Err("scry bottom-subset prompt has a noncanonical shape"
                                    .to_string());
                            }
                            validate_exact_binding_permutation(
                                original_prefix,
                                &candidates,
                                "scry bottom-subset candidates",
                            )?;
                        }
                        ScrySelectionStage::OrderBottom { bottom_subset } => {
                            validate_canonical_scry_subset(original_prefix, bottom_subset)?;
                            if bottom_subset.len() < 2 {
                                return Err(
                                    "scry bottom-order prompt has no genuine ordering choice"
                                        .to_string(),
                                );
                            }
                            let count = u16::try_from(bottom_subset.len())
                                .map_err(|_| "scry bottom group exceeds u16".to_string())?;
                            if *min_targets != count || *max_targets != count || !*ordered {
                                return Err(
                                    "scry bottom-order prompt has a noncanonical shape".to_string()
                                );
                            }
                            validate_exact_binding_permutation(
                                bottom_subset,
                                &candidates,
                                "scry bottom-order candidates",
                            )?;
                        }
                        ScrySelectionStage::OrderRetainedTop {
                            bottom_subset,
                            ordered_bottom,
                        } => {
                            validate_canonical_scry_subset(original_prefix, bottom_subset)?;
                            validate_exact_binding_permutation(
                                bottom_subset,
                                ordered_bottom,
                                "scry ordered bottom",
                            )?;
                            let retained = scry_retained_prefix(original_prefix, bottom_subset)?;
                            if retained.len() < 2 {
                                return Err(
                                    "scry retained-top prompt has no genuine ordering choice"
                                        .to_string(),
                                );
                            }
                            let count = u16::try_from(retained.len())
                                .map_err(|_| "scry retained group exceeds u16".to_string())?;
                            if *min_targets != count || *max_targets != count || !*ordered {
                                return Err(
                                    "scry retained-top prompt has a noncanonical shape".to_string()
                                );
                            }
                            validate_exact_binding_permutation(
                                &retained,
                                &candidates,
                                "scry retained-top candidates",
                            )?;
                        }
                    }
                }
                EffectTargetSelectionPurpose::OrderIntoGraveyard { .. } => {}
            }
        }
        PendingEffectChoice::ChooseBoolean {
            player,
            purpose:
                EffectBooleanChoicePurpose::ShuffleLibrary {
                    player: library_player,
                },
            ..
        } => {
            if player != library_player {
                return Err("shuffle choice player/library mismatch".to_string());
            }
        }
        PendingEffectChoice::ChooseOption { .. } => {}
    }
    Ok(())
}

fn validate_resumable_program(op: &EffectOp) -> Result<(), String> {
    match op {
        EffectOp::Sequence(ops) => {
            for inner in ops {
                validate_resumable_program(inner)?;
            }
        }
        EffectOp::Conditional { then, else_, .. } => {
            validate_resumable_program(then)?;
            validate_resumable_program(else_)?;
        }
        EffectOp::Choice { options, .. } => {
            for option in options {
                validate_resumable_program(option)?;
            }
        }
        EffectOp::DiscardCards { .. }
        | EffectOp::MayPayCostThen { .. }
        | EffectOp::OfferAffectedPlayerSpellCopy { .. } => {
            return Err(
                "choice-bearing programs cannot yet mix legacy-suspending effect leaves"
                    .to_string(),
            );
        }
        _ => {}
    }
    Ok(())
}

fn drive_resumable(state: &mut GameState) -> Result<ResumableProgress, String> {
    let mut continuation = state
        .engine
        .pending_effect
        .take()
        .ok_or("no effect continuation is pending")?;
    if continuation.choice.is_some() {
        state.engine.pending_effect = Some(continuation);
        return Ok(ResumableProgress::Suspended);
    }

    while let Some(frame) = continuation.frames.pop() {
        let EffectFrame::Program { op, path } = frame else {
            match frame {
                EffectFrame::MoveObjectsBatch {
                    objects,
                    to_zone,
                    preserve_known_identity,
                    order_resolved,
                    path,
                } => {
                    if to_zone == Zone::Graveyard && objects.len() >= 2 && !order_resolved {
                        for binding in &objects {
                            validate_effect_object_binding(state, *binding)?;
                        }
                        let player = state.objects.get(objects[0].object).owner;
                        assert!(
                            objects
                                .iter()
                                .all(|binding| state.objects.get(binding.object).owner == player),
                            "one graveyard-order batch must contain cards from one owner"
                        );
                        stage_graveyard_order_choice(
                            &mut continuation,
                            player,
                            path,
                            objects,
                            EffectTargetSelectionPurpose::OrderIntoGraveyard {
                                preserve_known_identity,
                            },
                        );
                        state.engine.pending_effect = Some(continuation);
                        return Ok(ResumableProgress::Suspended);
                    }
                    commit_zone_change_batch(state, &objects, to_zone, preserve_known_identity)?;
                }
                EffectFrame::MillLibraryBatch {
                    objects,
                    order_resolved,
                    path,
                } => {
                    validate_bound_library_prefix(state, &objects)?;
                    if objects.len() >= 2 && !order_resolved {
                        let player = state.objects.get(objects[0].object).owner;
                        // A mill instruction does not publicly reveal its
                        // library snapshot before the move. The owner must
                        // nevertheless see the cards to order them, so grant
                        // only that perspective exact temporary knowledge.
                        state.reveal_library_top(player, player, objects.len());
                        stage_graveyard_order_choice(
                            &mut continuation,
                            player,
                            path,
                            objects,
                            EffectTargetSelectionPurpose::OrderMilledIntoGraveyard,
                        );
                        state.engine.pending_effect = Some(continuation);
                        return Ok(ResumableProgress::Suspended);
                    }
                    commit_zone_change_batch(state, &objects, Zone::Graveyard, false)?;
                }
                EffectFrame::ReorderLibraryTop {
                    player,
                    expected_prefix,
                    ordered,
                    path: _,
                } => {
                    validate_bound_library_prefix_exact(state, player, &expected_prefix)?;
                    for &binding in &ordered {
                        validate_effect_object_binding(state, binding)?;
                    }
                    let mut expected_set = expected_prefix
                        .iter()
                        .map(|binding| binding.object)
                        .collect::<Vec<_>>();
                    let mut ordered_set = ordered
                        .iter()
                        .map(|binding| binding.object)
                        .collect::<Vec<_>>();
                    expected_set.sort_unstable();
                    ordered_set.sort_unstable();
                    if ordered_set != expected_set {
                        return Err(
                            "chosen library order is not the bound prefix permutation".to_string()
                        );
                    }
                    let ordered_ids = ordered
                        .iter()
                        .map(|binding| binding.object)
                        .collect::<Vec<_>>();
                    state.reorder_library_top(player, &ordered_ids, &[player])?;
                }
                EffectFrame::ShuffleLibrary { player, path: _ } => {
                    state.shuffle_library(player);
                }
                EffectFrame::PutCardsFromHandOnLibraryTop {
                    player,
                    total,
                    remaining,
                    prompt_index,
                    expected_hand,
                    chosen,
                    path,
                    canonical_path,
                } => {
                    validate_hand_to_library_progress(
                        total,
                        remaining,
                        prompt_index,
                        &path,
                        &canonical_path,
                    )?;
                    if remaining == 0 {
                        if prompt_index != u16::from(total)
                            || chosen.is_some()
                            || !expected_hand.is_empty()
                        {
                            return Err(
                                "completed hand-to-library frame has noncanonical progress"
                                    .to_string(),
                            );
                        }
                        continue;
                    }

                    if let Some(chosen) = chosen {
                        if expected_hand.is_empty() {
                            return Err(
                                "chosen hand-to-library frame lacks its bound hand snapshot"
                                    .to_string(),
                            );
                        }
                        validate_bound_hand_exact(state, player, &expected_hand)?;
                        if !expected_hand.contains(&chosen) {
                            return Err(
                                "chosen hand-to-library card is outside the bound hand".to_string()
                            );
                        }
                        let next_remaining = remaining
                            .checked_sub(1)
                            .ok_or("active hand-to-library frame has no remaining card count")?;
                        let next_prompt_index = prompt_index
                            .checked_add(1)
                            .ok_or("hand-to-library prompt index overflowed")?;
                        validate_hand_to_library_progress(
                            total,
                            next_remaining,
                            next_prompt_index,
                            &path,
                            &canonical_path,
                        )?;
                        let next_frame = EffectFrame::PutCardsFromHandOnLibraryTop {
                            player,
                            total,
                            remaining: next_remaining,
                            prompt_index: next_prompt_index,
                            expected_hand: Vec::new(),
                            chosen: None,
                            path,
                            canonical_path,
                        };
                        // The private subset choice invalidates every exact
                        // nonowner hand fact, not only the card that happened
                        // to be selected. Otherwise a previously known card
                        // left behind would reveal the hidden choice by
                        // elimination.
                        state.clear_nonowner_hand_knowledge(player);
                        event::propose_and_commit(
                            state,
                            event::ProposedEvent::private_top_library_insert(chosen.object),
                        );
                        if state.objects.get(chosen.object).zone != Zone::Library
                            || state.players[player.index()].library.first() != Some(&chosen.object)
                        {
                            return Err("private hand-to-library insertion did not commit on top"
                                .to_string());
                        }
                        continuation.frames.push(next_frame);
                        continue;
                    }

                    if !expected_hand.is_empty() {
                        return Err(
                            "hand-to-library coordinator carries an unchosen hand snapshot"
                                .to_string(),
                        );
                    }
                    let current_hand = bind_hand(state, player);
                    if current_hand.is_empty() {
                        continue;
                    }
                    state.clear_nonowner_hand_knowledge(player);
                    if current_hand.len() == 1 {
                        continuation
                            .frames
                            .push(EffectFrame::PutCardsFromHandOnLibraryTop {
                                player,
                                total,
                                remaining,
                                prompt_index,
                                chosen: current_hand.first().copied(),
                                expected_hand: current_hand,
                                path,
                                canonical_path,
                            });
                        continue;
                    }
                    stage_hand_to_library_choice(
                        &mut continuation,
                        player,
                        total,
                        remaining,
                        prompt_index,
                        canonical_path,
                        current_hand,
                    );
                    state.engine.pending_effect = Some(continuation);
                    return Ok(ResumableProgress::Suspended);
                }
                EffectFrame::ScryLibrary {
                    player,
                    requested_count,
                    original_library_len,
                    original_prefix,
                    progress,
                    progress_fingerprint,
                    path,
                    canonical_path,
                } => {
                    if path != canonical_path {
                        return Err(
                            "scry coordinator path changed from its canonical path".to_string()
                        );
                    }
                    if progress_fingerprint != scry_progress_fingerprint(&progress) {
                        return Err("scry coordinator progress fingerprint changed".to_string());
                    }
                    validate_scry_live_metadata(
                        state,
                        player,
                        requested_count,
                        original_library_len,
                        &original_prefix,
                    )?;
                    match progress {
                        ScryProgress::BottomSubsetChosen { bottom_subset } => {
                            validate_canonical_scry_subset(&original_prefix, &bottom_subset)?;
                            if bottom_subset.len() >= 2 {
                                stage_scry_choice(
                                    &mut continuation,
                                    player,
                                    requested_count,
                                    original_library_len,
                                    original_prefix,
                                    ScrySelectionStage::OrderBottom { bottom_subset },
                                    canonical_path,
                                )?;
                                state.engine.pending_effect = Some(continuation);
                                return Ok(ResumableProgress::Suspended);
                            }
                            let ordered_bottom = bottom_subset.clone();
                            let progress = ScryProgress::BottomOrderChosen {
                                bottom_subset,
                                ordered_bottom,
                            };
                            let progress_fingerprint = scry_progress_fingerprint(&progress);
                            continuation.frames.push(EffectFrame::ScryLibrary {
                                player,
                                requested_count,
                                original_library_len,
                                original_prefix,
                                progress,
                                progress_fingerprint,
                                path,
                                canonical_path,
                            });
                        }
                        ScryProgress::BottomOrderChosen {
                            bottom_subset,
                            ordered_bottom,
                        } => {
                            validate_canonical_scry_subset(&original_prefix, &bottom_subset)?;
                            validate_exact_binding_permutation(
                                &bottom_subset,
                                &ordered_bottom,
                                "scry ordered bottom",
                            )?;
                            let retained = scry_retained_prefix(&original_prefix, &bottom_subset)?;
                            if retained.len() >= 2 {
                                stage_scry_choice(
                                    &mut continuation,
                                    player,
                                    requested_count,
                                    original_library_len,
                                    original_prefix,
                                    ScrySelectionStage::OrderRetainedTop {
                                        bottom_subset,
                                        ordered_bottom,
                                    },
                                    canonical_path,
                                )?;
                                state.engine.pending_effect = Some(continuation);
                                return Ok(ResumableProgress::Suspended);
                            }
                            let progress = ScryProgress::TopOrderChosen {
                                bottom_subset,
                                ordered_bottom,
                                ordered_top: retained,
                            };
                            let progress_fingerprint = scry_progress_fingerprint(&progress);
                            continuation.frames.push(EffectFrame::ScryLibrary {
                                player,
                                requested_count,
                                original_library_len,
                                original_prefix,
                                progress,
                                progress_fingerprint,
                                path,
                                canonical_path,
                            });
                        }
                        ScryProgress::TopOrderChosen {
                            bottom_subset,
                            ordered_bottom,
                            ordered_top,
                        } => {
                            validate_canonical_scry_subset(&original_prefix, &bottom_subset)?;
                            validate_exact_binding_permutation(
                                &bottom_subset,
                                &ordered_bottom,
                                "scry ordered bottom",
                            )?;
                            let retained = scry_retained_prefix(&original_prefix, &bottom_subset)?;
                            validate_exact_binding_permutation(
                                &retained,
                                &ordered_top,
                                "scry ordered retained top",
                            )?;
                            let expected_prefix = original_prefix
                                .iter()
                                .map(|binding| crate::state::ObjectLinkV4 {
                                    object: binding.object,
                                    zone_change_count: binding.expected_zone_change_count,
                                })
                                .collect::<Vec<_>>();
                            let retained_top = ordered_top
                                .iter()
                                .map(|binding| binding.object)
                                .collect::<Vec<_>>();
                            let bottom = ordered_bottom
                                .iter()
                                .map(|binding| binding.object)
                                .collect::<Vec<_>>();
                            state.apply_scry_result(
                                player,
                                &expected_prefix,
                                &retained_top,
                                &bottom,
                            )?;
                        }
                    }
                }
                EffectFrame::Program { .. } => unreachable!(),
            }
            continue;
        };
        match op {
            EffectOp::Sequence(ops) => {
                for (index, inner) in ops.into_iter().enumerate().rev() {
                    let mut inner_path = path.clone();
                    inner_path.push(index as u16);
                    continuation.frames.push(EffectFrame::Program {
                        op: inner,
                        path: inner_path,
                    });
                }
            }
            EffectOp::Conditional { cond, then, else_ } => {
                let branch = if eval_cond(&cond, &continuation.ctx, state) {
                    0
                } else {
                    1
                };
                let mut branch_path = path;
                branch_path.push(branch);
                continuation.frames.push(EffectFrame::Program {
                    op: if branch == 0 { *then } else { *else_ },
                    path: branch_path,
                });
            }
            EffectOp::Choice {
                controller,
                mut options,
            } => match options.len() {
                0 => {}
                1 => {
                    let mut option_path = path;
                    option_path.push(0);
                    continuation.frames.push(EffectFrame::Program {
                        op: options.remove(0),
                        path: option_path,
                    });
                }
                _ => {
                    let player = continuation.ctx.resolve_player(controller, state);
                    continuation.choice = Some(PendingEffectChoice::ChooseOption {
                        player,
                        path,
                        options,
                    });
                    state.engine.pending_effect = Some(continuation);
                    return Ok(ResumableProgress::Suspended);
                }
            },
            EffectOp::RevealTopAndPartitionByType {
                player,
                count,
                card_type,
                matching_to,
                rest_to,
            } => {
                let player = continuation.ctx.resolve_player(player, state);
                let (matching, rest) =
                    reveal_top_and_partition(state, player, count, card_type, matching_to, rest_to);
                // Frames are LIFO: push the rest group first so the matching
                // group is committed/ordered first, mirroring XMage's two
                // separate moveCards calls.
                for (group_index, objects, to_zone) in
                    [(1_u16, rest, rest_to), (0_u16, matching, matching_to)]
                {
                    let mut group_path = path.clone();
                    group_path.push(group_index);
                    continuation.frames.push(EffectFrame::MoveObjectsBatch {
                        objects,
                        to_zone,
                        preserve_known_identity: true,
                        order_resolved: false,
                        path: group_path,
                    });
                }
            }
            EffectOp::MillCards { player, count } => {
                let player = continuation.ctx.resolve_player(player, state);
                continuation.frames.push(EffectFrame::MillLibraryBatch {
                    objects: bind_library_top(state, player, count),
                    order_resolved: false,
                    path,
                });
            }
            EffectOp::LookAtLibraryTopAndReorder { player, count } => {
                let player = continuation.ctx.resolve_player(player, state);
                let original_prefix = bind_library_top(state, player, count);
                // A private look changes only this observer's knowledge. A
                // 0/1-card prefix has no ordering choice and must not erase
                // another observer's still-valid prior fact.
                state.reveal_library_top(player, player, original_prefix.len());
                if original_prefix.len() >= 2 {
                    stage_library_order_choice(&mut continuation, player, path, original_prefix);
                    state.engine.pending_effect = Some(continuation);
                    return Ok(ResumableProgress::Suspended);
                }
            }
            EffectOp::MayShuffleLibrary { player } => {
                let player = continuation.ctx.resolve_player(player, state);
                continuation.choice = Some(PendingEffectChoice::ChooseBoolean {
                    player,
                    path,
                    default: Some(false),
                    purpose: EffectBooleanChoicePurpose::ShuffleLibrary { player },
                });
                state.engine.pending_effect = Some(continuation);
                return Ok(ResumableProgress::Suspended);
            }
            EffectOp::PutCardsFromHandOnLibraryTop { player, count } => {
                let player = continuation.ctx.resolve_player(player, state);
                let canonical_path = path.clone();
                continuation
                    .frames
                    .push(EffectFrame::PutCardsFromHandOnLibraryTop {
                        player,
                        total: count,
                        remaining: count,
                        prompt_index: 0,
                        expected_hand: Vec::new(),
                        chosen: None,
                        path,
                        canonical_path,
                    });
            }
            EffectOp::Scry { player, count } => {
                let player = continuation.ctx.resolve_player(player, state);
                let original_library_len = state.players[player.index()]
                    .library
                    .len()
                    .try_into()
                    .expect("a live library length fits the u32 state contract");
                // Reject outside the certified Scry2 envelope before even a
                // private prefix binding is materialized.
                validate_scry_static_metadata(count, original_library_len)?;
                let original_prefix = bind_library_top(state, player, count);
                validate_scry_bound_metadata(count, original_library_len, &original_prefix)?;
                // Looking is private. Candidate identities remain visible to
                // the owner alone through both state knowledge and RL
                // projection; another observer receives only the typed choice
                // envelope and its public cardinalities.
                state.reveal_library_top(player, player, original_prefix.len());
                if !original_prefix.is_empty() {
                    stage_scry_choice(
                        &mut continuation,
                        player,
                        count,
                        original_library_len,
                        original_prefix,
                        ScrySelectionStage::ChooseBottomSubset,
                        path,
                    )?;
                    state.engine.pending_effect = Some(continuation);
                    return Ok(ResumableProgress::Suspended);
                }
            }
            leaf => execute(&leaf, &continuation.ctx, state),
        }
    }

    Ok(ResumableProgress::Complete(continuation.resolving_item))
}

impl ExecCtx {
    pub fn no_targets(source: ObjectId, controller: PlayerId) -> ExecCtx {
        ExecCtx {
            source,
            controller,
            targets: Vec::new(),
            discarded: Vec::new(),
            kicked: false,
        }
    }

    fn resolve_object(&self, r: ObjectRef) -> ObjectId {
        match r {
            ObjectRef::ThisSource => self.source,
            ObjectRef::Target(i) => match self.targets[i as usize] {
                Target::Object(id) => id,
                Target::Player(_) => panic!("effect expected an object target at index {i}"),
            },
        }
    }

    fn resolve_target(&self, r: TargetRef) -> Target {
        match r {
            TargetRef::ThisSource => Target::Object(self.source),
            TargetRef::Target(i) => self.targets[i as usize],
            TargetRef::Opponent => Target::Player(self.controller.opponent()),
        }
    }

    fn resolve_player(&self, r: PlayerRef, state: &GameState) -> PlayerId {
        match r {
            PlayerRef::Controller => self.controller,
            PlayerRef::Target(i) => match self.targets[i as usize] {
                Target::Player(p) => p,
                Target::Object(_) => panic!("effect expected a player target at index {i}"),
            },
            PlayerRef::ObjectController(oref) => {
                state.objects.get(self.resolve_object(oref)).controller
            }
        }
    }
}

fn stage_graveyard_order_choice(
    continuation: &mut EffectContinuation,
    player: PlayerId,
    path: Vec<u16>,
    objects: Vec<EffectObjectBinding>,
    purpose: EffectTargetSelectionPurpose,
) {
    let count = objects
        .len()
        .try_into()
        .expect("effect target count fits the u16 public contract");
    continuation.choice = Some(PendingEffectChoice::SelectTargets {
        player,
        path,
        selected: Vec::new(),
        legal: objects
            .into_iter()
            .map(|binding| EffectTargetCandidate {
                target: Target::Object(binding.object),
                expected_object: Some(binding),
            })
            .collect(),
        min_targets: count,
        max_targets: count,
        ordered: true,
        purpose,
    });
}

fn stage_library_order_choice(
    continuation: &mut EffectContinuation,
    player: PlayerId,
    path: Vec<u16>,
    original_prefix: Vec<EffectObjectBinding>,
) {
    let count = original_prefix
        .len()
        .try_into()
        .expect("library-order target count fits the u16 public contract");
    continuation.choice = Some(PendingEffectChoice::SelectTargets {
        player,
        path,
        selected: Vec::new(),
        legal: original_prefix
            .iter()
            .copied()
            .map(|binding| EffectTargetCandidate {
                target: Target::Object(binding.object),
                expected_object: Some(binding),
            })
            .collect(),
        min_targets: count,
        max_targets: count,
        ordered: true,
        purpose: EffectTargetSelectionPurpose::OrderLookedLibraryTop {
            player,
            original_prefix,
        },
    });
}

fn stage_hand_to_library_choice(
    continuation: &mut EffectContinuation,
    player: PlayerId,
    total: u8,
    remaining: u8,
    prompt_index: u16,
    canonical_path: Vec<u16>,
    original_hand: Vec<EffectObjectBinding>,
) {
    let continuation_path = canonical_path.clone();
    debug_assert!(validate_hand_to_library_progress(
        total,
        remaining,
        prompt_index,
        &continuation_path,
        &canonical_path,
    )
    .is_ok());
    debug_assert!(remaining > 0);
    debug_assert!(original_hand.len() >= 2);
    let mut choice_path = canonical_path.clone();
    choice_path.push(prompt_index);
    continuation.choice = Some(PendingEffectChoice::SelectTargets {
        player,
        path: choice_path,
        selected: Vec::new(),
        legal: original_hand
            .iter()
            .copied()
            .map(|binding| EffectTargetCandidate {
                target: Target::Object(binding.object),
                expected_object: Some(binding),
            })
            .collect(),
        min_targets: 1,
        max_targets: 1,
        ordered: true,
        purpose: EffectTargetSelectionPurpose::PutHandCardOnLibraryTop {
            player,
            original_hand,
            total,
            remaining,
            prompt_index,
            continuation_path,
            canonical_path,
        },
    });
}

fn stage_scry_choice(
    continuation: &mut EffectContinuation,
    player: PlayerId,
    requested_count: u8,
    original_library_len: u32,
    original_prefix: Vec<EffectObjectBinding>,
    stage: ScrySelectionStage,
    canonical_path: Vec<u16>,
) -> Result<(), String> {
    validate_scry_bound_metadata(requested_count, original_library_len, &original_prefix)?;
    let (candidates, min_targets, max_targets, ordered) = match &stage {
        ScrySelectionStage::ChooseBottomSubset => (
            original_prefix.clone(),
            0,
            u16::try_from(original_prefix.len())
                .map_err(|_| "scry prefix exceeds u16".to_string())?,
            false,
        ),
        ScrySelectionStage::OrderBottom { bottom_subset } => {
            validate_canonical_scry_subset(&original_prefix, bottom_subset)?;
            if bottom_subset.len() < 2 {
                return Err("scry bottom-order prompt has no genuine choice".to_string());
            }
            let count = u16::try_from(bottom_subset.len())
                .map_err(|_| "scry bottom group exceeds u16".to_string())?;
            (bottom_subset.clone(), count, count, true)
        }
        ScrySelectionStage::OrderRetainedTop {
            bottom_subset,
            ordered_bottom,
        } => {
            validate_canonical_scry_subset(&original_prefix, bottom_subset)?;
            validate_exact_binding_permutation(
                bottom_subset,
                ordered_bottom,
                "scry ordered bottom",
            )?;
            let retained = scry_retained_prefix(&original_prefix, bottom_subset)?;
            if retained.len() < 2 {
                return Err("scry retained-top prompt has no genuine choice".to_string());
            }
            let count = u16::try_from(retained.len())
                .map_err(|_| "scry retained group exceeds u16".to_string())?;
            (retained, count, count, true)
        }
    };
    let mut choice_path = canonical_path.clone();
    choice_path.push(scry_stage_tag(&stage));
    let stage_fingerprint = scry_stage_fingerprint(&stage);
    continuation.choice = Some(PendingEffectChoice::SelectTargets {
        player,
        path: choice_path,
        selected: Vec::new(),
        legal: candidates
            .into_iter()
            .map(|binding| EffectTargetCandidate {
                target: Target::Object(binding.object),
                expected_object: Some(binding),
            })
            .collect(),
        min_targets,
        max_targets,
        ordered,
        purpose: EffectTargetSelectionPurpose::ScryLibrary {
            player,
            requested_count,
            original_library_len,
            original_prefix,
            stage,
            stage_fingerprint,
            canonical_path,
        },
    });
    Ok(())
}

fn scry_stage_tag(stage: &ScrySelectionStage) -> u16 {
    match stage {
        ScrySelectionStage::ChooseBottomSubset => 0,
        ScrySelectionStage::OrderBottom { .. } => 1,
        ScrySelectionStage::OrderRetainedTop { .. } => 2,
    }
}

fn scry_stage_fingerprint(stage: &ScrySelectionStage) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325;
    hash = fnv1a_u64(hash, u64::from(scry_stage_tag(stage)));
    match stage {
        ScrySelectionStage::ChooseBottomSubset => hash,
        ScrySelectionStage::OrderBottom { bottom_subset } => fnv1a_bindings(hash, bottom_subset),
        ScrySelectionStage::OrderRetainedTop {
            bottom_subset,
            ordered_bottom,
        } => {
            hash = fnv1a_bindings(hash, bottom_subset);
            fnv1a_bindings(hash, ordered_bottom)
        }
    }
}

fn scry_progress_fingerprint(progress: &ScryProgress) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325;
    match progress {
        ScryProgress::BottomSubsetChosen { bottom_subset } => {
            hash = fnv1a_u64(hash, 0);
            fnv1a_bindings(hash, bottom_subset)
        }
        ScryProgress::BottomOrderChosen {
            bottom_subset,
            ordered_bottom,
        } => {
            hash = fnv1a_u64(hash, 1);
            hash = fnv1a_bindings(hash, bottom_subset);
            fnv1a_bindings(hash, ordered_bottom)
        }
        ScryProgress::TopOrderChosen {
            bottom_subset,
            ordered_bottom,
            ordered_top,
        } => {
            hash = fnv1a_u64(hash, 2);
            hash = fnv1a_bindings(hash, bottom_subset);
            hash = fnv1a_bindings(hash, ordered_bottom);
            fnv1a_bindings(hash, ordered_top)
        }
    }
}

fn fnv1a_bindings(mut hash: u64, bindings: &[EffectObjectBinding]) -> u64 {
    hash = fnv1a_u64(hash, bindings.len() as u64);
    for binding in bindings {
        hash = fnv1a_u64(hash, u64::from(binding.object.0));
        hash = fnv1a_u64(hash, zone_fingerprint(binding.expected_zone));
        hash = fnv1a_u64(hash, u64::from(binding.expected_zone_change_count));
    }
    hash
}

fn fnv1a_u64(mut hash: u64, value: u64) -> u64 {
    for byte in value.to_le_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn zone_fingerprint(zone: Zone) -> u64 {
    match zone {
        Zone::Library => 0,
        Zone::Hand => 1,
        Zone::Battlefield => 2,
        Zone::Graveyard => 3,
        Zone::Stack => 4,
        Zone::Exile => 5,
        Zone::Command => 6,
    }
}

fn validate_scry_static_metadata(
    requested_count: u8,
    original_library_len: u32,
) -> Result<usize, String> {
    if requested_count > 2 {
        return Err(
            "scry counts above two are outside the certified Preordain contract".to_string(),
        );
    }
    let library_len = usize::try_from(original_library_len)
        .map_err(|_| "scry original library length does not fit usize".to_string())?;
    Ok(usize::from(requested_count).min(library_len))
}

fn validate_scry_bound_metadata(
    requested_count: u8,
    original_library_len: u32,
    original_prefix: &[EffectObjectBinding],
) -> Result<(), String> {
    let expected_prefix_len = validate_scry_static_metadata(requested_count, original_library_len)?;
    if original_prefix.len() != expected_prefix_len {
        return Err(
            "scry-bound prefix length disagrees with requested count and original library length"
                .to_string(),
        );
    }
    if original_prefix
        .iter()
        .any(|binding| binding.expected_zone != Zone::Library)
    {
        return Err("scry binding does not expect the library zone".to_string());
    }
    let mut ids = original_prefix
        .iter()
        .map(|binding| binding.object)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    if ids.len() != original_prefix.len() {
        return Err("scry-bound prefix contains a duplicate physical object".to_string());
    }
    Ok(())
}

fn validate_scry_live_metadata(
    state: &GameState,
    player: PlayerId,
    requested_count: u8,
    original_library_len: u32,
    original_prefix: &[EffectObjectBinding],
) -> Result<(), String> {
    validate_scry_bound_metadata(requested_count, original_library_len, original_prefix)?;
    if state.players[player.index()].library.len()
        != usize::try_from(original_library_len)
            .map_err(|_| "scry original library length does not fit usize".to_string())?
    {
        return Err("scry library length changed while its private choice was pending".to_string());
    }
    validate_bound_library_prefix_exact(state, player, original_prefix)
}

fn validate_exact_binding_permutation(
    expected: &[EffectObjectBinding],
    actual: &[EffectObjectBinding],
    label: &str,
) -> Result<(), String> {
    let mut expected = expected.to_vec();
    let mut actual = actual.to_vec();
    expected.sort_by_key(|binding| binding.object);
    actual.sort_by_key(|binding| binding.object);
    if actual != expected {
        return Err(format!("{label} is not an exact bound-object permutation"));
    }
    Ok(())
}

fn canonicalize_scry_subset(
    original_prefix: &[EffectObjectBinding],
    selected: &[EffectObjectBinding],
) -> Result<Vec<EffectObjectBinding>, String> {
    let mut selected_sorted = selected.to_vec();
    selected_sorted.sort_by_key(|binding| binding.object);
    selected_sorted.dedup();
    if selected_sorted.len() != selected.len()
        || selected_sorted
            .iter()
            .any(|binding| !original_prefix.contains(binding))
    {
        return Err("scry bottom selection is not a unique subset of the bound prefix".to_string());
    }
    Ok(original_prefix
        .iter()
        .copied()
        .filter(|binding| selected.contains(binding))
        .collect())
}

fn validate_canonical_scry_subset(
    original_prefix: &[EffectObjectBinding],
    bottom_subset: &[EffectObjectBinding],
) -> Result<(), String> {
    if canonicalize_scry_subset(original_prefix, bottom_subset)? != bottom_subset {
        return Err("scry bottom subset is not in canonical prefix order".to_string());
    }
    Ok(())
}

fn scry_retained_prefix(
    original_prefix: &[EffectObjectBinding],
    bottom_subset: &[EffectObjectBinding],
) -> Result<Vec<EffectObjectBinding>, String> {
    validate_canonical_scry_subset(original_prefix, bottom_subset)?;
    Ok(original_prefix
        .iter()
        .copied()
        .filter(|binding| !bottom_subset.contains(binding))
        .collect())
}

/// Validates the redundant progress/path metadata carried by Brainstorm-style
/// repeated hand-to-library coordinators. The copied total and structural path
/// catch stale or internally inconsistent trusted snapshots; they do not
/// authenticate a `GameState` whose related private fields were coherently
/// rewritten outside the opaque in-process `Snapshot` API.
fn validate_hand_to_library_progress(
    total: u8,
    remaining: u8,
    prompt_index: u16,
    path: &[u16],
    canonical_path: &[u16],
) -> Result<(), String> {
    if path != canonical_path {
        return Err("hand-to-library coordinator path changed from its canonical path".to_string());
    }
    if remaining > total {
        return Err("hand-to-library remaining count exceeds its canonical total".to_string());
    }
    if remaining > 0 && total == 0 {
        return Err("active hand-to-library progress has a zero canonical total".to_string());
    }
    let expected_prompt_index = u16::from(total - remaining);
    if prompt_index != expected_prompt_index {
        return Err("hand-to-library prompt index disagrees with canonical progress".to_string());
    }
    Ok(())
}

fn bind_hand(state: &GameState, player: PlayerId) -> Vec<EffectObjectBinding> {
    state.players[player.index()]
        .hand
        .iter()
        .copied()
        .map(|object| EffectObjectBinding {
            object,
            expected_zone: Zone::Hand,
            expected_zone_change_count: state.objects.get(object).zone_change_count,
        })
        .collect()
}

fn bind_library_top(state: &GameState, player: PlayerId, count: u8) -> Vec<EffectObjectBinding> {
    state.players[player.index()].library
        [..usize::from(count).min(state.players[player.index()].library.len())]
        .iter()
        .copied()
        .map(|object| EffectObjectBinding {
            object,
            expected_zone: Zone::Library,
            expected_zone_change_count: state.objects.get(object).zone_change_count,
        })
        .collect()
}

fn validate_bound_hand_exact(
    state: &GameState,
    player: PlayerId,
    objects: &[EffectObjectBinding],
) -> Result<(), String> {
    if objects
        .iter()
        .any(|binding| binding.expected_zone != Zone::Hand)
    {
        return Err("hand-to-library binding does not expect the hand zone".to_string());
    }
    for &binding in objects {
        validate_effect_object_binding(state, binding)?;
        if state.objects.get(binding.object).owner != player {
            return Err("hand-to-library binding has the wrong hand owner".to_string());
        }
    }
    let mut expected = objects
        .iter()
        .map(|binding| binding.object)
        .collect::<Vec<_>>();
    let mut current = state.players[player.index()].hand.clone();
    expected.sort_unstable();
    current.sort_unstable();
    if expected != current {
        return Err("bound hand changed identity or membership".to_string());
    }
    Ok(())
}

/// Validates both object incarnation and membership in the exact current
/// library prefix. Zone-change generations alone cannot detect a shuffle or
/// reorder, so a restored mill continuation must also prove that its bound
/// set is still precisely the top N cards it originally snapshotted.
fn validate_bound_library_prefix(
    state: &GameState,
    objects: &[EffectObjectBinding],
) -> Result<(), String> {
    let Some(first) = objects.first() else {
        return Ok(());
    };
    if objects
        .iter()
        .any(|binding| binding.expected_zone != Zone::Library)
    {
        return Err("milled-card binding does not expect the library zone".to_string());
    }
    for &binding in objects {
        validate_effect_object_binding(state, binding)?;
    }
    let owner = state.objects.get(first.object).owner;
    if objects
        .iter()
        .any(|binding| state.objects.get(binding.object).owner != owner)
    {
        return Err("milled-card bindings do not share one library owner".to_string());
    }
    let library = &state.players[owner.index()].library;
    if library.len() < objects.len() {
        return Err("milled-card binding is longer than the live library".to_string());
    }
    let mut expected = objects
        .iter()
        .map(|binding| binding.object)
        .collect::<Vec<_>>();
    let mut current = library[..objects.len()].to_vec();
    expected.sort_unstable();
    current.sort_unstable();
    if expected != current {
        return Err("milled-card bindings no longer match the live library prefix".to_string());
    }
    Ok(())
}

/// Exact-order counterpart to the mill batch's set validator. Private look
/// choices retain their originally observed prefix separately from the
/// mutable selected/legal partition, so even a same-card-set reorder in a
/// restored/tampered state fails closed before accepting another action.
fn validate_bound_library_prefix_exact(
    state: &GameState,
    player: PlayerId,
    objects: &[EffectObjectBinding],
) -> Result<(), String> {
    if objects
        .iter()
        .any(|binding| binding.expected_zone != Zone::Library)
    {
        return Err("library-order binding does not expect the library zone".to_string());
    }
    for &binding in objects {
        validate_effect_object_binding(state, binding)?;
        if state.objects.get(binding.object).owner != player {
            return Err("library-order binding has the wrong library owner".to_string());
        }
    }
    let library = &state.players[player.index()].library;
    if library.len() < objects.len() {
        return Err("library-order binding is longer than the live library".to_string());
    }
    let expected = objects
        .iter()
        .map(|binding| binding.object)
        .collect::<Vec<_>>();
    if library[..objects.len()] != expected {
        return Err("bound library prefix changed order or identity".to_string());
    }
    Ok(())
}

fn reveal_top_and_partition(
    state: &mut GameState,
    player: PlayerId,
    count: u8,
    card_type: CardType,
    matching_to: Zone,
    rest_to: Zone,
) -> (Vec<EffectObjectBinding>, Vec<EffectObjectBinding>) {
    assert!(
        !matches!(matching_to, Zone::Library | Zone::Stack)
            && !matches!(rest_to, Zone::Library | Zone::Stack),
        "library partition destinations must be ordinary nonlibrary card zones"
    );
    let revealed = state.players[player.index()].library
        [..usize::from(count).min(state.players[player.index()].library.len())]
        .to_vec();
    let mut matching = Vec::new();
    let mut rest = Vec::new();
    for object in revealed.iter().copied() {
        let live = state.objects.get(object);
        let binding = EffectObjectBinding {
            object,
            expected_zone: Zone::Library,
            expected_zone_change_count: live.zone_change_count,
        };
        let def = &crate::card_def::CARD_DEFS[live.card_def as usize];
        if def.has_type(card_type) {
            matching.push(binding);
        } else {
            rest.push(binding);
        }
    }

    // "Reveal" is public, unlike a private look. Record the exact prefix
    // for both perspectives before any member leaves and shifts the
    // remaining position facts.
    for observer in [PlayerId::P0, PlayerId::P1] {
        state.reveal_library_top(observer, player, revealed.len());
    }
    (matching, rest)
}

fn validate_effect_object_binding(
    state: &GameState,
    binding: EffectObjectBinding,
) -> Result<(), String> {
    let object = state
        .objects
        .try_get(binding.object)
        .ok_or_else(|| format!("effect object {} no longer exists", binding.object.0))?;
    if object.zone != binding.expected_zone
        || object.zone_change_count != binding.expected_zone_change_count
    {
        return Err(format!(
            "effect object {} changed incarnation: expected {:?}/{} but found {:?}/{}",
            binding.object.0,
            binding.expected_zone,
            binding.expected_zone_change_count,
            object.zone,
            object.zone_change_count
        ));
    }
    Ok(())
}

fn commit_zone_change_batch(
    state: &mut GameState,
    objects: &[EffectObjectBinding],
    to_zone: Zone,
    preserve_known_identity: bool,
) -> Result<(), String> {
    for &binding in objects {
        validate_effect_object_binding(state, binding)?;
    }
    let events = objects
        .iter()
        .map(|binding| {
            if preserve_known_identity {
                event::ProposedEvent::zone_change_preserving_known_identity(binding.object, to_zone)
            } else {
                event::ProposedEvent::zone_change(binding.object, to_zone)
            }
        })
        .collect();
    event::propose_and_commit_batch(state, events);
    Ok(())
}

pub fn execute(op: &EffectOp, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        EffectOp::Sequence(ops) => {
            for inner in ops {
                execute(inner, ctx, state);
            }
        }
        EffectOp::Conditional { cond, then, else_ } => {
            let taken = eval_cond(cond, ctx, state);
            execute(if taken { then } else { else_ }, ctx, state);
        }
        EffectOp::Choice { options, .. } => {
            if let Some(first) = options.first() {
                execute(first, ctx, state);
            }
        }
        EffectOp::DealDamage { target, amount } => {
            let target = ctx.resolve_target(*target);
            event::propose_and_commit(
                state,
                event::ProposedEvent::damage(ctx.source, target, *amount),
            );
        }
        EffectOp::GainLife { player, amount } => {
            let player = ctx.resolve_player(*player, state);
            event::propose_and_commit(state, event::ProposedEvent::life_gain(player, *amount));
        }
        EffectOp::LoseLife { player, amount } => {
            let player = ctx.resolve_player(*player, state);
            event::propose_and_commit(state, event::ProposedEvent::life_loss(player, *amount));
        }
        EffectOp::DrawCards { player, count } => {
            let player = ctx.resolve_player(*player, state);
            for _ in 0..*count {
                event::propose_and_commit(state, event::ProposedEvent::draw(player));
            }
        }
        EffectOp::RevealTopAndPartitionByType {
            player,
            count,
            card_type,
            matching_to,
            rest_to,
        } => {
            let player = ctx.resolve_player(*player, state);
            let (matching, rest) =
                reveal_top_and_partition(state, player, *count, *card_type, *matching_to, *rest_to);
            assert!(
                !(*matching_to == Zone::Graveyard && matching.len() >= 2
                    || *rest_to == Zone::Graveyard && rest.len() >= 2),
                "a multi-card graveyard partition must use the resumable interpreter"
            );
            for (objects, destination) in [(&matching, *matching_to), (&rest, *rest_to)] {
                commit_zone_change_batch(state, objects, destination, true)
                    .expect("freshly revealed batch bindings remain valid");
            }
        }
        EffectOp::MoveObject { object, to_zone } => {
            let object = ctx.resolve_object(*object);
            let live_stack_spell = state
                .stack
                .iter()
                .find(|item| {
                    item.source == object && item.kind == crate::state::StackItemKind::Spell
                })
                .cloned();
            if *to_zone != Zone::Stack {
                if let Some(item) = live_stack_spell {
                    if item.is_copy {
                        // A copied spell is not a card and never enters a
                        // destination zone when it leaves the stack (707.10a).
                        event::cease_to_exist(state, object);
                        return;
                    }
                    if item.is_flashback {
                        // Flashback's replacement applies to every attempted
                        // move away from the stack, including being countered.
                        event::propose_and_commit(
                            state,
                            event::ProposedEvent::zone_change(object, Zone::Exile),
                        );
                        return;
                    }
                }
            }
            event::propose_and_commit(state, event::ProposedEvent::zone_change(object, *to_zone));
        }
        EffectOp::TapObject { object } => {
            let object = ctx.resolve_object(*object);
            event::propose_and_commit(state, event::ProposedEvent::tap(object));
        }
        EffectOp::AddMana { player, colors } => {
            let player = ctx.resolve_player(*player, state);
            event::propose_and_commit(
                state,
                event::ProposedEvent::mana_add(player, colors.clone()),
            );
        }
        EffectOp::DiscardCards { player, count } => {
            let player = ctx.resolve_player(*player, state);
            state.engine.pending_discard = Some(crate::engine::PendingDiscard {
                player,
                count: *count,
                resume: crate::engine::DiscardResume::None,
            });
        }
        EffectOp::CreateToken {
            token_def,
            controller,
        } => {
            let token = crate::card_def::CARD_DEFS
                .get(*token_def as usize)
                .unwrap_or_else(|| panic!("CreateToken references unknown definition {token_def}"));
            assert!(
                token.is_token && token.is_executable() && token.has_full_support(),
                "CreateToken requires a fully supported executable token definition, got {} ({:?}, is_token={})",
                token.name,
                token.capability,
                token.is_token
            );
            let controller = ctx.resolve_player(*controller, state);
            event::propose_and_commit(
                state,
                event::ProposedEvent::create_token(*token_def, controller),
            );
        }
        EffectOp::MayPayCostThen {
            discard,
            sacrifice_lands,
            then,
        } => {
            let discard_payable = *discard > 0
                && state.players[ctx.controller.index()].hand.len() >= *discard as usize;
            let sacrifice_payable = *sacrifice_lands > 0
                && crate::engine::count_controlled_lands(ctx.controller, state)
                    >= *sacrifice_lands as u32;
            if !discard_payable && !sacrifice_payable {
                // Nothing payable: DoIfCostPaid's own `cost.canPay(...)`
                // gate is false too, so the reference never even offers the
                // "may pay?" prompt here -- matches, no-op.
                return;
            }
            state.engine.pending_optional_cost = Some(crate::engine::PendingOptionalCost {
                player: ctx.controller,
                source: ctx.source,
                discard: *discard,
                sacrifice_lands: *sacrifice_lands,
                discard_payable,
                sacrifice_payable,
                then: (**then).clone(),
                // `resolve_top_of_stack` fills this in right after this
                // call returns, if it's resolving this same spell -- see
                // `PendingOptionalCost::spell_resume`'s doc.
                spell_resume: None,
            });
        }
        EffectOp::DamageOpponentAndTheirCreatures { amount } => {
            // 611.2c-adjacent simultaneity: one instance of this effect
            // hits the opponent and every creature they control at once,
            // same pattern `engine::combat_damage_wave` already uses for
            // combat damage -- a single propose/replace/commit batch, not
            // sequential individual commits that could see each other's
            // side effects mid-resolution.
            let opponent = ctx.controller.opponent();
            let mut events = vec![event::ProposedEvent::damage(
                ctx.source,
                Target::Player(opponent),
                *amount,
            )];
            events.extend(
                state.players[opponent.index()]
                    .battlefield
                    .iter()
                    .copied()
                    .filter_map(|id| {
                        let def =
                            &crate::card_def::CARD_DEFS[state.objects.get(id).card_def as usize];
                        def.has_type(crate::card_def::CardType::Creature).then(|| {
                            event::ProposedEvent::damage(ctx.source, Target::Object(id), *amount)
                        })
                    }),
            );
            event::propose_and_commit_batch(state, events);
        }
        EffectOp::PumpControlled {
            filter,
            power,
            toughness,
            grant_haste,
        } => {
            let object_ids: Vec<ObjectId> = state.players[ctx.controller.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|&id| {
                    let def = &crate::card_def::CARD_DEFS[state.objects.get(id).card_def as usize];
                    if !def.has_type(crate::card_def::CardType::Creature) {
                        return false;
                    }
                    match filter {
                        CreatureFilter::AnyControlled => true,
                        CreatureFilter::ControlledWithSubtype(sub) => def.subtypes.contains(sub),
                    }
                })
                .collect();
            if !object_ids.is_empty() {
                let mut layer = crate::engine::Layers::NONE;
                if *power != 0 || *toughness != 0 {
                    layer = layer | crate::engine::Layers::POWER_TOUGHNESS;
                }
                if *grant_haste {
                    layer = layer | crate::engine::Layers::ABILITY_ADDING;
                }
                let timestamp = crate::engine::next_timestamp(state);
                state.engine.until_end_of_turn.push(
                    crate::engine::UntilEndOfTurnEffect::ResolvedSetEffect {
                        object_ids,
                        layer,
                        timestamp,
                        duration: crate::engine::EffectDuration::EndOfTurn,
                        power: *power,
                        toughness: *toughness,
                        grant_haste: *grant_haste,
                    },
                );
            }
        }
        EffectOp::ImpulseDraw { count, duration } => {
            for _ in 0..*count {
                let Some(&top) = state.players[ctx.controller.index()].library.first() else {
                    break; // library ran dry partway through -- not a draw, no loss condition
                };
                if std::env::var("REPLAY_DEBUG_IMPULSE").is_ok() {
                    eprintln!(
                        "IMPULSE_DRAW controller={:?} source={:?} exiling id={} name={:?} turn={} round_step={:?} lib_remaining_before={} hand_size={} priority_round={}",
                        ctx.controller,
                        state.objects.get(ctx.source).name,
                        top.0,
                        state.objects.get(top).name,
                        state.turn,
                        state.step,
                        state.players[ctx.controller.index()].library.len(),
                        state.players[ctx.controller.index()].hand.len(),
                        state.engine.priority_round,
                    );
                }
                event::propose_and_commit(
                    state,
                    event::ProposedEvent::zone_change(top, Zone::Exile),
                );
                let expiry = match duration {
                    ImpulseDuration::EndOfTurn => crate::engine::PlayPermissionExpiry::EndOfTurn,
                    ImpulseDuration::UntilOwnersNextTurn => {
                        crate::engine::PlayPermissionExpiry::UntilHoldersNextTurn {
                            holder_turn_started: false,
                        }
                    }
                };
                let def = &crate::card_def::CARD_DEFS[state.objects.get(top).card_def as usize];
                let play_or_cast = if def.is_playable_land() {
                    crate::engine::PlayOrCast::Play
                } else if def.is_castable() {
                    crate::engine::PlayOrCast::Cast
                } else {
                    // Exiling still happened, but an unsupported definition
                    // never receives an executable permission. This is the
                    // runtime half of the fail-closed deck preflight.
                    continue;
                };
                state
                    .engine
                    .exile_play_permissions
                    .push(crate::engine::PlayPermission {
                        object: top,
                        holder: ctx.controller,
                        // Snapshot *after* the exile move above, so this
                        // permission's own creating zone change isn't what
                        // immediately invalidates it -- see `PlayPermission::
                        // zone_change_generation`'s doc.
                        zone_change_generation: state.objects.get(top).zone_change_count,
                        play_or_cast,
                        expiry,
                    });
            }
        }
        EffectOp::OfferAffectedPlayerSpellCopy { affected } => {
            let target = ctx.resolve_target(*affected);
            let decider = match target {
                Target::Player(p) => p,
                Target::Object(id) => state.objects.get(id).controller,
            };
            state.engine.pending_spell_copy = Some(crate::engine::PendingSpellCopy {
                resolving_source: ctx.source,
                player: decider,
                inherited_target: target,
                stage: crate::engine::SpellCopyStage::Payment,
                copy_source: None,
            });
        }
        EffectOp::MillCards { player, count } => {
            let player = ctx.resolve_player(*player, state);
            let objects = bind_library_top(state, player, *count);
            assert!(
                objects.len() < 2,
                "a multi-card mill must use the resumable interpreter"
            );
            commit_zone_change_batch(state, &objects, Zone::Graveyard, false)
                .expect("fresh mill bindings remain valid");
        }
        EffectOp::LookAtLibraryTopAndReorder { .. }
        | EffectOp::MayShuffleLibrary { .. }
        | EffectOp::PutCardsFromHandOnLibraryTop { .. }
        | EffectOp::Scry { .. } => {
            panic!("private library choices must use the resumable interpreter")
        }
    }
}

fn eval_cond(cond: &EffectCond, ctx: &ExecCtx, state: &GameState) -> bool {
    match cond {
        EffectCond::Always => true,
        EffectCond::Never => false,
        EffectCond::DiscardedNonLandForCost => ctx.discarded.iter().any(|&id| {
            let def_idx = state.objects.get(id).card_def;
            !crate::card_def::CARD_DEFS[def_idx as usize].is_land
        }),
        EffectCond::LandfallThisTurn => {
            state.players[ctx.controller.index()].lands_played_this_turn > 0
        }
        EffectCond::TargetInZone(idx, zone) => match ctx.targets.get(*idx as usize) {
            Some(Target::Object(id)) if *zone == Zone::Stack => {
                state.stack.iter().any(|item| item.source == *id)
            }
            Some(Target::Object(id)) => state.objects.get(*id).zone == *zone,
            _ => false,
        },
        EffectCond::TargetIsColor(idx, color) => match ctx.targets.get(*idx as usize) {
            Some(Target::Object(id)) => {
                let def_idx = state.objects.get(*id).card_def;
                crate::card_def::CARD_DEFS[def_idx as usize]
                    .colors
                    .contains(color)
            }
            _ => false,
        },
        EffectCond::And(a, b) => eval_cond(a, ctx, state) && eval_cond(b, ctx, state),
        EffectCond::ControlsArtifactCount(n) => {
            let count = state.players[ctx.controller.index()]
                .battlefield
                .iter()
                .filter(|&&id| {
                    let def = &crate::card_def::CARD_DEFS[state.objects.get(id).card_def as usize];
                    def.has_type(crate::card_def::CardType::Artifact)
                })
                .count();
            count >= *n as usize
        }
        EffectCond::WasKicked => ctx.kicked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::CommittedEvent;
    use crate::ids::PlayerId;

    fn two_card_libraries() -> GameState {
        GameState::new_from_libraries(&[1, 2], &[3, 4], |c| format!("card-{c}"), 1)
    }

    #[test]
    fn sequence_runs_every_leaf_in_order() {
        let mut state = two_card_libraries();
        let ctx = ExecCtx::no_targets(ObjectId(0), PlayerId::P0);
        let op = EffectOp::Sequence(vec![
            EffectOp::LoseLife {
                player: PlayerRef::Controller,
                amount: 2,
            },
            EffectOp::GainLife {
                player: PlayerRef::Controller,
                amount: 5,
            },
        ]);
        execute(&op, &ctx, &mut state);
        assert_eq!(state.players[0].life, 20 - 2 + 5);
    }

    /// Proves the `Conditional` composition primitive works end-to-end even
    /// though no card in this increment's pool needs it.
    #[test]
    fn conditional_picks_then_or_else_branch() {
        let mut state = two_card_libraries();
        let ctx = ExecCtx::no_targets(ObjectId(0), PlayerId::P0);

        let taken = EffectOp::Conditional {
            cond: EffectCond::Always,
            then: Box::new(EffectOp::LoseLife {
                player: PlayerRef::Controller,
                amount: 3,
            }),
            else_: Box::new(EffectOp::Sequence(vec![])),
        };
        execute(&taken, &ctx, &mut state);
        assert_eq!(state.players[0].life, 17);

        let not_taken = EffectOp::Conditional {
            cond: EffectCond::Never,
            then: Box::new(EffectOp::LoseLife {
                player: PlayerRef::Controller,
                amount: 100,
            }),
            else_: Box::new(EffectOp::Sequence(vec![])),
        };
        execute(&not_taken, &ctx, &mut state);
        assert_eq!(state.players[0].life, 17, "else branch is a no-op here");
    }

    #[test]
    fn deal_damage_to_target_player_reduces_life() {
        let mut state = two_card_libraries();
        let ctx = ExecCtx {
            source: ObjectId(0),
            controller: PlayerId::P0,
            targets: vec![Target::Player(PlayerId::P1)],
            discarded: Vec::new(),
            kicked: false,
        };
        execute(
            &EffectOp::DealDamage {
                target: TargetRef::Target(0),
                amount: 3,
            },
            &ctx,
            &mut state,
        );
        assert_eq!(state.players[1].life, 17);
    }

    #[test]
    fn deal_damage_to_target_object_marks_damage() {
        let mut state = two_card_libraries();
        let creature = state.draw_card(PlayerId::P1).unwrap();
        state.move_hand_to_battlefield(PlayerId::P1, creature);
        let ctx = ExecCtx {
            source: ObjectId(0),
            controller: PlayerId::P0,
            targets: vec![Target::Object(creature)],
            discarded: Vec::new(),
            kicked: false,
        };
        execute(
            &EffectOp::DealDamage {
                target: TargetRef::Target(0),
                amount: 4,
            },
            &ctx,
            &mut state,
        );
        assert_eq!(state.objects.get(creature).damage, 4);
    }

    #[test]
    fn draw_cards_leaf_draws_the_requested_count() {
        let mut state = two_card_libraries();
        let ctx = ExecCtx::no_targets(ObjectId(0), PlayerId::P0);
        execute(
            &EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 2,
            },
            &ctx,
            &mut state,
        );
        assert_eq!(state.players[0].hand.len(), 2);
    }

    fn card_ids(names: &[&str]) -> Vec<u16> {
        names
            .iter()
            .map(|name| crate::card_def::card_id_by_name(name).unwrap())
            .collect()
    }

    #[test]
    fn reveal_top_partition_is_public_ordered_and_not_a_draw() {
        let definitions = card_ids(&[
            "Elvish Mystic",
            "Quirion Ranger",
            "Llanowar Elves",
            "Lightning Bolt",
            "Island",
        ]);
        let mut state = GameState::new_from_libraries(
            &definitions,
            &[],
            |card_def| {
                crate::card_def::CARD_DEFS[card_def as usize]
                    .name
                    .to_string()
            },
            9,
        );
        let original = state.players[0].library.clone();
        state.reveal_library_top(PlayerId::P1, PlayerId::P0, 5);
        let ctx = ExecCtx::no_targets(original[0], PlayerId::P0);

        execute(
            &EffectOp::RevealTopAndPartitionByType {
                player: PlayerRef::Controller,
                count: 4,
                card_type: CardType::Creature,
                matching_to: Zone::Hand,
                rest_to: Zone::Graveyard,
            },
            &ctx,
            &mut state,
        );

        assert_eq!(
            state.players[0].hand,
            vec![original[0], original[1], original[2]]
        );
        assert_eq!(state.players[0].graveyard, vec![original[3]]);
        assert_eq!(state.players[0].library, vec![original[4]]);
        assert_eq!(
            state
                .known_hand_cards(PlayerId::P1, PlayerId::P0)
                .iter()
                .map(|entry| entry.object)
                .collect::<Vec<_>>(),
            vec![original[0], original[1], original[2]]
        );
        assert!(state
            .known_hand_cards(PlayerId::P0, PlayerId::P0)
            .is_empty());
        assert_eq!(
            state
                .known_library_cards(PlayerId::P1, PlayerId::P0)
                .iter()
                .map(|entry| (entry.position, entry.object))
                .collect::<Vec<_>>(),
            vec![(0, original[4])]
        );
        assert_eq!(state.players[0].draws_this_turn, 0);
        assert!(!state.players[0].drew_from_empty);
        assert!(state
            .engine
            .event_history
            .iter()
            .all(|event| !matches!(event, CommittedEvent::Draw { .. })));
        assert_eq!(
            state
                .engine
                .event_history
                .iter()
                .filter_map(|event| match event {
                    CommittedEvent::ZoneChange { object, .. } => Some(*object),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec![original[0], original[1], original[2], original[3]]
        );
        for &object in &original[..4] {
            assert_eq!(state.objects.get(object).zone_change_count, 1);
        }
    }

    #[test]
    fn reveal_top_partition_handles_short_empty_and_zero_hit_libraries() {
        let definitions = card_ids(&["Lightning Bolt"]);
        let mut short = GameState::new_from_libraries(
            &definitions,
            &[],
            |card_def| {
                crate::card_def::CARD_DEFS[card_def as usize]
                    .name
                    .to_string()
            },
            1,
        );
        let original = short.players[0].library.clone();
        let ctx = ExecCtx::no_targets(original[0], PlayerId::P0);
        let op = EffectOp::RevealTopAndPartitionByType {
            player: PlayerRef::Controller,
            count: 4,
            card_type: CardType::Creature,
            matching_to: Zone::Hand,
            rest_to: Zone::Graveyard,
        };
        execute(&op, &ctx, &mut short);
        assert!(short.players[0].hand.is_empty());
        assert_eq!(short.players[0].graveyard, original);
        assert!(!short.players[0].drew_from_empty);

        let mut empty = GameState::new_from_libraries(&[], &[], |_| String::new(), 1);
        let before = empty.clone();
        execute(
            &op,
            &ExecCtx::no_targets(ObjectId(0), PlayerId::P0),
            &mut empty,
        );
        assert_eq!(empty, before);
    }

    #[test]
    fn tap_and_add_mana_leaves_compose_a_mana_ability() {
        let mut state = two_card_libraries();
        let land = state.draw_card(PlayerId::P0).unwrap();
        state.move_hand_to_battlefield(PlayerId::P0, land);
        let ctx = ExecCtx::no_targets(land, PlayerId::P0);
        let op = EffectOp::Sequence(vec![
            EffectOp::TapObject {
                object: ObjectRef::ThisSource,
            },
            EffectOp::AddMana {
                player: PlayerRef::Controller,
                colors: vec![ManaColor::R],
            },
        ]);
        execute(&op, &ctx, &mut state);
        assert!(state.objects.get(land).tapped);
        assert_eq!(state.players[0].mana_pool[ManaColor::R.pool_index()], 1);
    }

    #[test]
    fn create_token_requires_and_materializes_a_full_token_definition() {
        let mut state = two_card_libraries();
        let ctx = ExecCtx::no_targets(ObjectId(0), PlayerId::P0);
        let blood = crate::card_def::card_id_by_name("Blood Token").unwrap();
        execute(
            &EffectOp::CreateToken {
                token_def: blood,
                controller: PlayerRef::Controller,
            },
            &ctx,
            &mut state,
        );
        let created = *state.players[0].battlefield.last().unwrap();
        assert_eq!(state.objects.get(created).card_def, blood);
        assert!(crate::card_def::CARD_DEFS[blood as usize].has_full_support());
    }

    #[test]
    #[should_panic(expected = "CreateToken requires a fully supported executable token definition")]
    fn create_token_fails_loudly_for_a_nontoken_definition() {
        let mut state = two_card_libraries();
        let ctx = ExecCtx::no_targets(ObjectId(0), PlayerId::P0);
        execute(
            &EffectOp::CreateToken {
                token_def: crate::card_def::card_id_by_name("Island").unwrap(),
                controller: PlayerRef::Controller,
            },
            &ctx,
            &mut state,
        );
    }

    #[test]
    fn impulse_draw_exiles_but_does_not_authorize_an_unsupported_card() {
        let landscape = crate::card_def::card_id_by_name("Twisted Landscape").unwrap();
        let mut state = GameState::new_from_libraries(
            &[landscape],
            &[],
            |card_def| {
                crate::card_def::CARD_DEFS[card_def as usize]
                    .name
                    .to_string()
            },
            1,
        );
        let card = state.players[0].library[0];
        let ctx = ExecCtx::no_targets(card, PlayerId::P0);

        execute(
            &EffectOp::ImpulseDraw {
                count: 1,
                duration: ImpulseDuration::EndOfTurn,
            },
            &ctx,
            &mut state,
        );

        assert!(state.players[0].library.is_empty());
        assert_eq!(state.objects.get(card).zone, Zone::Exile);
        assert!(state.exile.contains(&card));
        assert!(state.engine.exile_play_permissions.is_empty());
    }
}
