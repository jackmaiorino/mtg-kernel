//! The engine's permanent event record, with a count of each event kind.
//!
//! [`EventHistoryV1`] keeps events in a persistent chunk tree, and serializes,
//! deserializes, hashes, compares and prints exactly as the original
//! `Vec<CommittedEvent>` did, so no state byte, digest or receipt changes.
//! Clones and prefix cuts share immutable nodes; appending copies only a
//! bounded leaf and its spine. Explicit Vec/slice compatibility and arbitrary
//! mutation still materialize a contiguous copy. Hot readers use logical
//! iteration and [`EventHistoryV1::range_v2`]. It counts events of each kind,
//! so a check that acts only on some kinds can skip its scan of the whole
//! history when none of them occurred ([`EventHistoryV1::has_any`]).
//!
//! The counting methods (`push`, `insert`, `remove`, `pop`, `retain`,
//! `truncate`, `clear`, `extend`) keep the counts equal to the events. Any
//! other mutable access goes through `DerefMut` (or `&mut` iteration),
//! which marks the counts stale: while they are stale `has_any` answers
//! true, so every scan runs as before, and the next counting method
//! recounts.
//!
//! It also keeps the hash chain behind the linked Aura origin digest v2
//! ([`EventHistoryV1::chain_digest_v2`]) for its longest unchanged prefix,
//! so a digest hashes only the events appended since the previous one, and
//! a validator's memo of one unchanged prefix
//! ([`EventHistoryV1::prefix_memo_v1`]), so a validation rechecks only the
//! events appended since an earlier complete one. Some validators keep
//! proofs the same way ([`EventHistoryV1::proof_v1`]): the lengths at which
//! their checks that read only the history passed.

use crate::event::CommittedEvent;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::any::Any;
use std::cell::Cell;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::{Bound, Deref, DerefMut, Index, IndexMut, RangeBounds};
use std::sync::{Arc, Mutex};

mod shared_sequence_v2;
pub(crate) use shared_sequence_v2::SharedSequence;

/// One value per `CommittedEvent` variant, in declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum EventKindV1 {
    Damage,
    ZoneChange,
    LifeLoss,
    LifeGain,
    Draw,
    Tap,
    ManaAdded,
    CreateToken,
    SpellCast,
    Targeted,
    Sacrificed,
    CombatDamageToPlayer,
    SagaChapter,
    UpkeepBegan,
    Transformed,
    OptionalAdditionalCostPaid,
    InitiativeTrigger,
    MonarchTrigger,
    DeclaredAttacker,
    PlusOneCountersAdded,
    BeginningEndStep,
    PrintedAbilitiesRemovedBeforeZoneChange,
    ControllerAttacked,
    CrimeCommitted,
    PowerBeforeLeavingBattlefield,
    BeginningOfCombat,
    WasCreatureBeforeLeavingBattlefield,
    LifeGainTurnBeganV1,
    LeftBattlefieldFaceV1,
    RoomDoorUnlockedV1,
    ClassLevelGainedV1,
    TurnedFaceUp,
    BeginningPrecombatMainV1,
    LeftBattlefieldCopyV1,
    ControlledCreatureDeathCaptureV1,
    ControlledCreatureDeathCaptureCollectedV1,
    DifferentControllerTargetDepartureV1,
    SingleGraveyardEntryV1,
    SourcePowerActivationCommittedV1,
    SourceDamageBeforeLeavingV1,
    SourcePowerActivationFinishedV1,
    SquadRallierResolutionBoundaryV1,
    SowerRestrictionInstalledV1,
    SowerRestrictionExpiredV1,
    SowerRestrictionResolutionV1,
    AuthorityEntryCaptureV1,
    AuthorityEntryCollectedV1,
    AuthorityEntryObserversV1,
    AuthorityEntryCompletedV1,
    AuthorityEntryOperationV1,
    AuthorityEntryPublishedV1,
    WildwoodCounterCaptureV1,
    WildwoodTriggerPublishedV1,
    WildwoodTriggerResolvingV1,
    WildwoodTriggerOperationV1,
    WildwoodTriggerClosedV1,
    WildwoodTriggerCounteredV1,
    TemporaryControlResolutionV1,
    TemporaryControlInstalledV1,
    TemporaryControlCleanupV1,
    TemporaryControlExpiredV1,
    FrenziedGoblinPublishedV1,
    FrenziedGoblinResolutionBoundaryV1,
    FrenziedGoblinPaymentV1,
    FrenziedGoblinRemovalV1,
    FrenziedGoblinCompletedV1,
    FrenziedGoblinInstalledV1,
    FrenziedGoblinExpiredV1,
    FrenziedGoblinCleanupV1,
    AuthorityEntryPublicFactsV1,
    CerberusObserversV1,
    CerberusCaptureV1,
    CerberusCollectedV1,
    CerberusPublishedV1,
    CerberusOperationV1,
    CerberusCompletedV1,
    LibrarianActivationCommittedV1,
    LibrarianActivationFinishedV1,
    LibrarianActivationOperationV1,
    LibrarianActivationCounterRemovalV1,
    PrivateKeepOneLibraryCapturedV1,
    PrivateKeepOneLibraryConsumedV1,
    RaidEntrySourceV1,
    PrivateKeepOneRaidSkippedV1,
    ExtraTurnStartedV1,
    ExtraTurnScheduledV1,
    ExtraTurnBeganV1,
    ExtraTurnOwnerPublishedV1,
    ExtraTurnOwnerRemovedV1,
    ExtraTurnVirtualCeasedV1,
    AshrootPowerBeforeDepartureV1,
    AshrootDispatchV2,
    GorehornDepartureV1,
    GorehornPublishedV1,
    GorehornPreparedV1,
    GorehornDamageOperationV1,
    GorehornOperationV1,
    RaidEntryCaptureV1,
    RaidEntryPublishedV1,
    RaidEntryDroppedV1,
    RaidEntryCounteredV1,
    RaidEntryStartedV1,
    BanishingEntryV1,
    BanishingCollectedV1,
    BanishingNoTargetsV1,
    BanishingPublishedV1,
    BanishingRemovedV1,
    BanishingFinishedV1,
    BanishingLinkInstalledV1,
    BanishingLinkExpiredV1,
    BanishingRecipientCeasedV1,
    BanishingAuraHostsCapturedV1,
    BanishingAuraHostChosenV1,
    BanishingAuraOperationStartedV1,
    LifeLossTurnBeganV1,
    StromkirkEndStepObserversV1,
    StromkirkTriggerCaptureV2,
    StromkirkCollectedV2,
    StromkirkPublishedV2,
    StromkirkPlacementRemovedV2,
    StromkirkResolutionRemovedV2,
    StromkirkCounterRemovedV2,
    StromkirkOperationV2,
    StromkirkCompletedV2,
    VampireGourmandResolutionBoundaryV1,
    VampireGourmandCostOperationV2,
    VampireGourmandCleanupV2,
    VampireGourmandExpiredV2,
    VampireGourmandAttackCaptureV2,
    VampireGourmandCollectedV2,
    VampireGourmandPublishedV2,
    VampireGourmandCounterRemovedV2,
    VampireGourmandCounterClosedV2,
    GarrukEntryObservationV1,
    GarrukDrawCaptureV1,
    GarrukDrawCollectedV1,
    GarrukDrawPublishedV1,
    GarrukDrawRemovedV1,
    GarrukDrawCompletedV1,
    CrystalPreventionBoundaryV1,
    CrystalShieldOperationV1,
    CrystalExecutionV1,
    CrystalDamageOperationV3,
    CrystalBatchPreparedV3,
    CrystalProgramChoiceV3,
    FixedDamageDepartureV3,
    FixedDamagePaidV3,
    FixedDamageRemovedV3,
    GorehornCrystalBeginV1,
    GorehornCrystalCompletionV1,
    ArahboEntryObservationV1,
    ArahboTokenCaptureV1,
    ArahboTokenCollectedV1,
    ArahboTokenPublishedV1,
    ArahboTokenRemovedV1,
    ArahboTokenCompletedV1,
    ArahboCounterInputV1,
    ExilePlaySelectionCapturedV1,
    ExilePlaySelectionConsumedV1,
    StrongboxRaidSkippedV1,
    StrongboxPermissionGrantedV1,
    StrongboxPermissionTurnStartedV1,
    StrongboxPermissionCleanupV1,
    StrongboxPermissionExpiredV1,
    ExileIfDiesReplacementInstalledV1,
    ExileIfDiesReplacementExpiredV1,
    ExileIfDiesReplacementCleanupV1,
    EquipmentDamagePublishedV4,
    EquipmentDamageDepartureV4,
    EquipmentDamagePreparedV4,
    EquipmentDamageClosedV4,
    EquipmentDamageCounterRemovalV4,
    KrenkoPaidV1,
    KrenkoRemovedV1,
    KrenkoSampleV1,
    KrenkoCompletedV1,
    KrenkoRemovalInputV1,
    KrenkoPaymentInputV1,
    ZulAshurPublishedV1,
    ZulAshurRemovedV1,
    ZulAshurInstalledV1,
    ZulAshurCleanupEnteredV1,
    ZulAshurCleanupV1,
    ZulAshurExpiredV1,
    ZulAshurRemovalInputV1,
    BannerEntryOperationStartedV1,
    BannerEntryColorCapturedV1,
    BannerEntryColorChosenV1,
    BannerEntryColorCommittedV1,
    BannerEntryOperationCompletedV1,
    InspirationResolutionBoundaryV1,
    InspirationResolutionInputV1,
    InspirationResolutionOperationV1,
    InspirationEngineDepartureV1,
    ProtectionSourceColorDepartureV1,
    FishingPoleJournalV39,
    TwinflameReplacementInputV38,
    TwinflameReplacementOperationV38,
    TwinflameDamagePreparationV38,
    TwinflameCombatPublishedV38,
    TwinflameCombatInputV38,
    TwinflameCombatOperationV38,
    TwinflameDynamicDamageSampleV38,
    TwinflameFieryCaptureV38,
    TwinflameFieryFinishedV38,
    AjaniJournalV42,
    FaebloomJournalV43,
    HighSocietyHunterAttackCaptureV1,
    HighSocietyHunterAttackCollectedV1,
    MandatoryCastPaymentV1,
    MandatoryCastPaymentInputV1,
    HighSocietyHunterJournalV45,
    FiendishPandaLifeGainCaptureV1,
    FiendishPandaDeathCaptureV1,
    FiendishPandaCaptureCollectedV1,
    FiendishPandaJournalV1,
    TokenCreationReplacementV48,
    PlusCounterReplacementV48,
    TokenCreationInputV48,
    TokenAttachmentJournalV48,
    BaitTransferInputV48,
    BaitTransferAppliedV48,
    PlusCounterInputV48,
    ConquerorLossBatchV49,
    ConquerorCapturedV49,
    ConquerorCollectedV49,
    ConquerorJournalV49,
    ConquerorObserverInputV49,
    FakeJournalV50,
    SoulCounterAddedV1,
    SoulCounterInputV1,
    RavenousPaymentV1,
    RavenousRemovedV1,
    RavenousCompletedV1,
    DrakeJournalV1,
    DrakeHistoryRefV1,
    ConquerorGainReplayBorrowV49,
    AleshaPowerLkiV1,
    NivMizzetVisionaryDamageV1,
    NivMizzetVisionaryTriggerStackedV1,
    NivMizzetVisionaryDamageSourceDepartureV1,
    KinshipEntryV1,
    SphinxsTutelageDrawV1,
    SphinxsTutelageCollectedV1,
    SphinxsTutelagePublishedV1,
    SphinxsTutelageMillStartedV1,
    SphinxsTutelageMillBatchV1,
    LathrilCombatDamageV1,
    LathrilCombatCollectedV1,
    LathrilCombatPublishedV1,
    LathrilCombatResolvedV1,
    LathrilCombatCounteredV1,
    LathrilCombatObservedV1,
    CopiedTokenAbilitiesEnteredV1,
    CopiedTokenEndStepV1,
    CopiedTokenEndStepCollectedV1,
    CopiedTokenSacrificePublishedV1,
    CopiedTokenSacrificeFinishedV1,
    CuratorEnteredV1,
    CuratorCollectedV1,
    CuratorPublishedV1,
    CuratorFinishedV1,
    CuratorProgressV1,
    KykarExiledV1,
    KykarDelayedCollectedV1,
    GreatwurmCounterPaymentV1,
    GreatwurmCombatCaptureV1,
    GreatwurmCombatBoundV1,
    GreatwurmCollectedV1,
    GreatwurmPublishedV1,
    GreatwurmFinishedV1,
    GreatwurmCounterInputV1,
    SourceCopyDepartureV1,
    SphinxLoreV1,
    SphinxLoreCastOptionsV1,
    SphinxLoreCastSelectedV1,
    OmniscienceCastOptionsV1,
    OmniscienceCastSelectedV1,
    NightmareCopyEnteredV1,
    NightmareCopyLayerV1,
    HarvesterGraveyardEntryV1,
    HarvesterResolvedV1,
    HarvesterCastAnnouncedV1,
    HarvesterCastReversedV1,
    ProgenitusInputV1,
    ProgenitusShuffledV1,
    LeylineOpeningV1,
    FiendArtisanInputV1,
    FiendArtisanPaidV1,
    FiendArtisanSearchStartedV1,
    FiendArtisanSearchFinishedV1,
    LifeLossWideV59,
    LifeGainWideV59,
    ConquerorObserverInputV59,
    ConquerorLossBatchV59,
    ConquerorCapturedV59,
    ConquerorCollectedV59,
    ConquerorJournalV59,
    FiendishPandaWideGainCaptureV59,
    ThousandYearStormV1,
    TimeStopJournalV1,
    OrdinaryCleanupStageV1,
    GenesisWaveProgressV1,
    GenesisWaveFinishedV1,
    PublishedCastIdentityV1,
    VivienReidV1,
    MuldrothaOptionsV1,
    MuldrothaSelectedV1,
    MuldrothaPlayedV1,
    TinybonesV72,
    LilianaDreadhordeV1,
    ChandraFlameshaperV74,
    KaitoCunningV75,
    DrakusethDamageSourceV76,
    NishobaDamageSourceV1,
    FullTextTriggerCapturedV1,
    FullTextBatchEntryV1,
    FullTextRestrictedUseV1,
    OrderedGraveyardControlGainedV1,
    MartyrRevealPaidV1,
    MeddlingMageEntryV1,
    EternalOpeningV1,
    EternalOwnedDrawV1,
}

/// Number of `CommittedEvent` variants (and of `EventKindV1` values).
pub const EVENT_KIND_COUNT_V1: usize = 323;
const _: () = assert!(EventKindV1::EternalOwnedDrawV1 as usize + 1 == EVENT_KIND_COUNT_V1);

impl CommittedEvent {
    /// This event's kind (its variant).
    pub fn kind_v1(&self) -> EventKindV1 {
        match self {
            Self::Damage { .. } => EventKindV1::Damage,
            Self::ZoneChange { .. } => EventKindV1::ZoneChange,
            Self::LifeLoss { .. } => EventKindV1::LifeLoss,
            Self::LifeGain { .. } => EventKindV1::LifeGain,
            Self::Draw { .. } => EventKindV1::Draw,
            Self::Tap { .. } => EventKindV1::Tap,
            Self::ManaAdded { .. } => EventKindV1::ManaAdded,
            Self::CreateToken { .. } => EventKindV1::CreateToken,
            Self::SpellCast { .. } => EventKindV1::SpellCast,
            Self::Targeted { .. } => EventKindV1::Targeted,
            Self::Sacrificed { .. } => EventKindV1::Sacrificed,
            Self::CombatDamageToPlayer { .. } => EventKindV1::CombatDamageToPlayer,
            Self::SagaChapter { .. } => EventKindV1::SagaChapter,
            Self::UpkeepBegan { .. } => EventKindV1::UpkeepBegan,
            Self::Transformed { .. } => EventKindV1::Transformed,
            Self::OptionalAdditionalCostPaid { .. } => EventKindV1::OptionalAdditionalCostPaid,
            Self::InitiativeTrigger { .. } => EventKindV1::InitiativeTrigger,
            Self::MonarchTrigger { .. } => EventKindV1::MonarchTrigger,
            Self::DeclaredAttacker { .. } => EventKindV1::DeclaredAttacker,
            Self::PlusOneCountersAdded { .. } => EventKindV1::PlusOneCountersAdded,
            Self::BeginningEndStep { .. } => EventKindV1::BeginningEndStep,
            Self::PrintedAbilitiesRemovedBeforeZoneChange { .. } => {
                EventKindV1::PrintedAbilitiesRemovedBeforeZoneChange
            }
            Self::ControllerAttacked { .. } => EventKindV1::ControllerAttacked,
            Self::CrimeCommitted { .. } => EventKindV1::CrimeCommitted,
            Self::PowerBeforeLeavingBattlefield { .. } => {
                EventKindV1::PowerBeforeLeavingBattlefield
            }
            Self::BeginningOfCombat { .. } => EventKindV1::BeginningOfCombat,
            Self::WasCreatureBeforeLeavingBattlefield { .. } => {
                EventKindV1::WasCreatureBeforeLeavingBattlefield
            }
            Self::LifeGainTurnBeganV1 { .. } => EventKindV1::LifeGainTurnBeganV1,
            Self::LeftBattlefieldFaceV1 { .. } => EventKindV1::LeftBattlefieldFaceV1,
            Self::RoomDoorUnlockedV1 { .. } => EventKindV1::RoomDoorUnlockedV1,
            Self::ClassLevelGainedV1 { .. } => EventKindV1::ClassLevelGainedV1,
            Self::TurnedFaceUp { .. } => EventKindV1::TurnedFaceUp,
            Self::BeginningPrecombatMainV1 { .. } => EventKindV1::BeginningPrecombatMainV1,
            Self::LeftBattlefieldCopyV1 { .. } => EventKindV1::LeftBattlefieldCopyV1,
            Self::ControlledCreatureDeathCaptureV1 { .. } => {
                EventKindV1::ControlledCreatureDeathCaptureV1
            }
            Self::ControlledCreatureDeathCaptureCollectedV1 { .. } => {
                EventKindV1::ControlledCreatureDeathCaptureCollectedV1
            }
            Self::DifferentControllerTargetDepartureV1 { .. } => {
                EventKindV1::DifferentControllerTargetDepartureV1
            }
            Self::SingleGraveyardEntryV1 { .. } => EventKindV1::SingleGraveyardEntryV1,
            Self::SourcePowerActivationCommittedV1 { .. } => {
                EventKindV1::SourcePowerActivationCommittedV1
            }
            Self::SourceDamageBeforeLeavingV1 { .. } => EventKindV1::SourceDamageBeforeLeavingV1,
            Self::SourcePowerActivationFinishedV1 { .. } => {
                EventKindV1::SourcePowerActivationFinishedV1
            }
            Self::SquadRallierResolutionBoundaryV1 { .. } => {
                EventKindV1::SquadRallierResolutionBoundaryV1
            }
            Self::SowerRestrictionInstalledV1 { .. } => EventKindV1::SowerRestrictionInstalledV1,
            Self::SowerRestrictionExpiredV1 { .. } => EventKindV1::SowerRestrictionExpiredV1,
            Self::SowerRestrictionResolutionV1 { .. } => EventKindV1::SowerRestrictionResolutionV1,
            Self::AuthorityEntryCaptureV1 { .. } => EventKindV1::AuthorityEntryCaptureV1,
            Self::AuthorityEntryCollectedV1 { .. } => EventKindV1::AuthorityEntryCollectedV1,
            Self::AuthorityEntryObserversV1 { .. } => EventKindV1::AuthorityEntryObserversV1,
            Self::AuthorityEntryCompletedV1 { .. } => EventKindV1::AuthorityEntryCompletedV1,
            Self::AuthorityEntryOperationV1 { .. } => EventKindV1::AuthorityEntryOperationV1,
            Self::AuthorityEntryPublishedV1 { .. } => EventKindV1::AuthorityEntryPublishedV1,
            Self::WildwoodCounterCaptureV1 { .. } => EventKindV1::WildwoodCounterCaptureV1,
            Self::WildwoodTriggerPublishedV1 { .. } => EventKindV1::WildwoodTriggerPublishedV1,
            Self::WildwoodTriggerResolvingV1 { .. } => EventKindV1::WildwoodTriggerResolvingV1,
            Self::WildwoodTriggerOperationV1 { .. } => EventKindV1::WildwoodTriggerOperationV1,
            Self::WildwoodTriggerClosedV1 { .. } => EventKindV1::WildwoodTriggerClosedV1,
            Self::WildwoodTriggerCounteredV1 { .. } => EventKindV1::WildwoodTriggerCounteredV1,
            Self::TemporaryControlResolutionV1 { .. } => EventKindV1::TemporaryControlResolutionV1,
            Self::TemporaryControlInstalledV1 { .. } => EventKindV1::TemporaryControlInstalledV1,
            Self::TemporaryControlCleanupV1 { .. } => EventKindV1::TemporaryControlCleanupV1,
            Self::TemporaryControlExpiredV1 { .. } => EventKindV1::TemporaryControlExpiredV1,
            Self::FrenziedGoblinPublishedV1 { .. } => EventKindV1::FrenziedGoblinPublishedV1,
            Self::FrenziedGoblinResolutionBoundaryV1 { .. } => {
                EventKindV1::FrenziedGoblinResolutionBoundaryV1
            }
            Self::FrenziedGoblinPaymentV1 { .. } => EventKindV1::FrenziedGoblinPaymentV1,
            Self::FrenziedGoblinRemovalV1 { .. } => EventKindV1::FrenziedGoblinRemovalV1,
            Self::FrenziedGoblinCompletedV1 { .. } => EventKindV1::FrenziedGoblinCompletedV1,
            Self::FrenziedGoblinInstalledV1 { .. } => EventKindV1::FrenziedGoblinInstalledV1,
            Self::FrenziedGoblinExpiredV1 { .. } => EventKindV1::FrenziedGoblinExpiredV1,
            Self::FrenziedGoblinCleanupV1 { .. } => EventKindV1::FrenziedGoblinCleanupV1,
            Self::AuthorityEntryPublicFactsV1 { .. } => EventKindV1::AuthorityEntryPublicFactsV1,
            Self::CerberusObserversV1 { .. } => EventKindV1::CerberusObserversV1,
            Self::CerberusCaptureV1 { .. } => EventKindV1::CerberusCaptureV1,
            Self::CerberusCollectedV1 { .. } => EventKindV1::CerberusCollectedV1,
            Self::CerberusPublishedV1 { .. } => EventKindV1::CerberusPublishedV1,
            Self::CerberusOperationV1 { .. } => EventKindV1::CerberusOperationV1,
            Self::CerberusCompletedV1 { .. } => EventKindV1::CerberusCompletedV1,
            Self::LibrarianActivationCommittedV1 { .. } => {
                EventKindV1::LibrarianActivationCommittedV1
            }
            Self::LibrarianActivationFinishedV1 { .. } => {
                EventKindV1::LibrarianActivationFinishedV1
            }
            Self::LibrarianActivationOperationV1 { .. } => {
                EventKindV1::LibrarianActivationOperationV1
            }
            Self::LibrarianActivationCounterRemovalV1 { .. } => {
                EventKindV1::LibrarianActivationCounterRemovalV1
            }
            Self::PrivateKeepOneLibraryCapturedV1 { .. } => {
                EventKindV1::PrivateKeepOneLibraryCapturedV1
            }
            Self::PrivateKeepOneLibraryConsumedV1 { .. } => {
                EventKindV1::PrivateKeepOneLibraryConsumedV1
            }
            Self::RaidEntrySourceV1 { .. } => EventKindV1::RaidEntrySourceV1,
            Self::PrivateKeepOneRaidSkippedV1 { .. } => EventKindV1::PrivateKeepOneRaidSkippedV1,
            Self::ExtraTurnStartedV1 { .. } => EventKindV1::ExtraTurnStartedV1,
            Self::ExtraTurnScheduledV1 { .. } => EventKindV1::ExtraTurnScheduledV1,
            Self::ExtraTurnBeganV1 { .. } => EventKindV1::ExtraTurnBeganV1,
            Self::ExtraTurnOwnerPublishedV1 { .. } => EventKindV1::ExtraTurnOwnerPublishedV1,
            Self::ExtraTurnOwnerRemovedV1 { .. } => EventKindV1::ExtraTurnOwnerRemovedV1,
            Self::ExtraTurnVirtualCeasedV1 { .. } => EventKindV1::ExtraTurnVirtualCeasedV1,
            Self::AshrootPowerBeforeDepartureV1 { .. } => {
                EventKindV1::AshrootPowerBeforeDepartureV1
            }
            Self::AshrootDispatchV2 { .. } => EventKindV1::AshrootDispatchV2,
            Self::GorehornDepartureV1 { .. } => EventKindV1::GorehornDepartureV1,
            Self::GorehornPublishedV1 { .. } => EventKindV1::GorehornPublishedV1,
            Self::GorehornPreparedV1 { .. } => EventKindV1::GorehornPreparedV1,
            Self::GorehornDamageOperationV1 { .. } => EventKindV1::GorehornDamageOperationV1,
            Self::GorehornOperationV1 { .. } => EventKindV1::GorehornOperationV1,
            Self::RaidEntryCaptureV1 { .. } => EventKindV1::RaidEntryCaptureV1,
            Self::RaidEntryPublishedV1 { .. } => EventKindV1::RaidEntryPublishedV1,
            Self::RaidEntryDroppedV1 { .. } => EventKindV1::RaidEntryDroppedV1,
            Self::RaidEntryCounteredV1 { .. } => EventKindV1::RaidEntryCounteredV1,
            Self::RaidEntryStartedV1 { .. } => EventKindV1::RaidEntryStartedV1,
            Self::BanishingEntryV1 { .. } => EventKindV1::BanishingEntryV1,
            Self::BanishingCollectedV1 { .. } => EventKindV1::BanishingCollectedV1,
            Self::BanishingNoTargetsV1 { .. } => EventKindV1::BanishingNoTargetsV1,
            Self::BanishingPublishedV1 { .. } => EventKindV1::BanishingPublishedV1,
            Self::BanishingRemovedV1 { .. } => EventKindV1::BanishingRemovedV1,
            Self::BanishingFinishedV1 { .. } => EventKindV1::BanishingFinishedV1,
            Self::BanishingLinkInstalledV1 { .. } => EventKindV1::BanishingLinkInstalledV1,
            Self::BanishingLinkExpiredV1 { .. } => EventKindV1::BanishingLinkExpiredV1,
            Self::BanishingRecipientCeasedV1 { .. } => EventKindV1::BanishingRecipientCeasedV1,
            Self::BanishingAuraHostsCapturedV1 { .. } => EventKindV1::BanishingAuraHostsCapturedV1,
            Self::BanishingAuraHostChosenV1 { .. } => EventKindV1::BanishingAuraHostChosenV1,
            Self::BanishingAuraOperationStartedV1 { .. } => {
                EventKindV1::BanishingAuraOperationStartedV1
            }
            Self::LifeLossTurnBeganV1 { .. } => EventKindV1::LifeLossTurnBeganV1,
            Self::StromkirkEndStepObserversV1 { .. } => EventKindV1::StromkirkEndStepObserversV1,
            Self::StromkirkTriggerCaptureV2 { .. } => EventKindV1::StromkirkTriggerCaptureV2,
            Self::StromkirkCollectedV2 { .. } => EventKindV1::StromkirkCollectedV2,
            Self::StromkirkPublishedV2 { .. } => EventKindV1::StromkirkPublishedV2,
            Self::StromkirkPlacementRemovedV2 { .. } => EventKindV1::StromkirkPlacementRemovedV2,
            Self::StromkirkResolutionRemovedV2 { .. } => EventKindV1::StromkirkResolutionRemovedV2,
            Self::StromkirkCounterRemovedV2 { .. } => EventKindV1::StromkirkCounterRemovedV2,
            Self::StromkirkOperationV2 { .. } => EventKindV1::StromkirkOperationV2,
            Self::StromkirkCompletedV2 { .. } => EventKindV1::StromkirkCompletedV2,
            Self::VampireGourmandResolutionBoundaryV1 { .. } => {
                EventKindV1::VampireGourmandResolutionBoundaryV1
            }
            Self::VampireGourmandCostOperationV2 { .. } => {
                EventKindV1::VampireGourmandCostOperationV2
            }
            Self::VampireGourmandCleanupV2 { .. } => EventKindV1::VampireGourmandCleanupV2,
            Self::VampireGourmandExpiredV2 { .. } => EventKindV1::VampireGourmandExpiredV2,
            Self::VampireGourmandAttackCaptureV2 { .. } => {
                EventKindV1::VampireGourmandAttackCaptureV2
            }
            Self::VampireGourmandCollectedV2 { .. } => EventKindV1::VampireGourmandCollectedV2,
            Self::VampireGourmandPublishedV2 { .. } => EventKindV1::VampireGourmandPublishedV2,
            Self::VampireGourmandCounterRemovedV2 { .. } => {
                EventKindV1::VampireGourmandCounterRemovedV2
            }
            Self::VampireGourmandCounterClosedV2 { .. } => {
                EventKindV1::VampireGourmandCounterClosedV2
            }
            Self::GarrukEntryObservationV1 { .. } => EventKindV1::GarrukEntryObservationV1,
            Self::GarrukDrawCaptureV1 { .. } => EventKindV1::GarrukDrawCaptureV1,
            Self::GarrukDrawCollectedV1 { .. } => EventKindV1::GarrukDrawCollectedV1,
            Self::GarrukDrawPublishedV1 { .. } => EventKindV1::GarrukDrawPublishedV1,
            Self::GarrukDrawRemovedV1 { .. } => EventKindV1::GarrukDrawRemovedV1,
            Self::GarrukDrawCompletedV1 { .. } => EventKindV1::GarrukDrawCompletedV1,
            Self::CrystalPreventionBoundaryV1 { .. } => EventKindV1::CrystalPreventionBoundaryV1,
            Self::CrystalShieldOperationV1 { .. } => EventKindV1::CrystalShieldOperationV1,
            Self::CrystalExecutionV1 { .. } => EventKindV1::CrystalExecutionV1,
            Self::CrystalDamageOperationV3 { .. } => EventKindV1::CrystalDamageOperationV3,
            Self::CrystalBatchPreparedV3 { .. } => EventKindV1::CrystalBatchPreparedV3,
            Self::CrystalProgramChoiceV3 { .. } => EventKindV1::CrystalProgramChoiceV3,
            Self::FixedDamageDepartureV3 { .. } => EventKindV1::FixedDamageDepartureV3,
            Self::FixedDamagePaidV3 { .. } => EventKindV1::FixedDamagePaidV3,
            Self::FixedDamageRemovedV3 { .. } => EventKindV1::FixedDamageRemovedV3,
            Self::GorehornCrystalBeginV1 { .. } => EventKindV1::GorehornCrystalBeginV1,
            Self::GorehornCrystalCompletionV1 { .. } => EventKindV1::GorehornCrystalCompletionV1,
            Self::ArahboEntryObservationV1 { .. } => EventKindV1::ArahboEntryObservationV1,
            Self::ArahboTokenCaptureV1 { .. } => EventKindV1::ArahboTokenCaptureV1,
            Self::ArahboTokenCollectedV1 { .. } => EventKindV1::ArahboTokenCollectedV1,
            Self::ArahboTokenPublishedV1 { .. } => EventKindV1::ArahboTokenPublishedV1,
            Self::ArahboTokenRemovedV1 { .. } => EventKindV1::ArahboTokenRemovedV1,
            Self::ArahboTokenCompletedV1 { .. } => EventKindV1::ArahboTokenCompletedV1,
            Self::ArahboCounterInputV1 { .. } => EventKindV1::ArahboCounterInputV1,
            Self::ExilePlaySelectionCapturedV1 { .. } => EventKindV1::ExilePlaySelectionCapturedV1,
            Self::ExilePlaySelectionConsumedV1 { .. } => EventKindV1::ExilePlaySelectionConsumedV1,
            Self::StrongboxRaidSkippedV1 { .. } => EventKindV1::StrongboxRaidSkippedV1,
            Self::StrongboxPermissionGrantedV1 { .. } => EventKindV1::StrongboxPermissionGrantedV1,
            Self::StrongboxPermissionTurnStartedV1 { .. } => {
                EventKindV1::StrongboxPermissionTurnStartedV1
            }
            Self::StrongboxPermissionCleanupV1 { .. } => EventKindV1::StrongboxPermissionCleanupV1,
            Self::StrongboxPermissionExpiredV1 { .. } => EventKindV1::StrongboxPermissionExpiredV1,
            Self::ExileIfDiesReplacementInstalledV1 { .. } => {
                EventKindV1::ExileIfDiesReplacementInstalledV1
            }
            Self::ExileIfDiesReplacementExpiredV1 { .. } => {
                EventKindV1::ExileIfDiesReplacementExpiredV1
            }
            Self::ExileIfDiesReplacementCleanupV1 { .. } => {
                EventKindV1::ExileIfDiesReplacementCleanupV1
            }
            Self::EquipmentDamagePublishedV4 { .. } => EventKindV1::EquipmentDamagePublishedV4,
            Self::EquipmentDamageDepartureV4 { .. } => EventKindV1::EquipmentDamageDepartureV4,
            Self::EquipmentDamagePreparedV4 { .. } => EventKindV1::EquipmentDamagePreparedV4,
            Self::EquipmentDamageClosedV4 { .. } => EventKindV1::EquipmentDamageClosedV4,
            Self::EquipmentDamageCounterRemovalV4 { .. } => {
                EventKindV1::EquipmentDamageCounterRemovalV4
            }
            Self::KrenkoPaidV1 { .. } => EventKindV1::KrenkoPaidV1,
            Self::KrenkoRemovedV1 { .. } => EventKindV1::KrenkoRemovedV1,
            Self::KrenkoSampleV1 { .. } => EventKindV1::KrenkoSampleV1,
            Self::KrenkoCompletedV1 { .. } => EventKindV1::KrenkoCompletedV1,
            Self::KrenkoRemovalInputV1 { .. } => EventKindV1::KrenkoRemovalInputV1,
            Self::KrenkoPaymentInputV1 { .. } => EventKindV1::KrenkoPaymentInputV1,
            Self::ZulAshurPublishedV1 { .. } => EventKindV1::ZulAshurPublishedV1,
            Self::ZulAshurRemovedV1 { .. } => EventKindV1::ZulAshurRemovedV1,
            Self::ZulAshurInstalledV1 { .. } => EventKindV1::ZulAshurInstalledV1,
            Self::ZulAshurCleanupEnteredV1 { .. } => EventKindV1::ZulAshurCleanupEnteredV1,
            Self::ZulAshurCleanupV1 { .. } => EventKindV1::ZulAshurCleanupV1,
            Self::ZulAshurExpiredV1 { .. } => EventKindV1::ZulAshurExpiredV1,
            Self::ZulAshurRemovalInputV1 { .. } => EventKindV1::ZulAshurRemovalInputV1,
            Self::BannerEntryOperationStartedV1 { .. } => {
                EventKindV1::BannerEntryOperationStartedV1
            }
            Self::BannerEntryColorCapturedV1 { .. } => EventKindV1::BannerEntryColorCapturedV1,
            Self::BannerEntryColorChosenV1 { .. } => EventKindV1::BannerEntryColorChosenV1,
            Self::BannerEntryColorCommittedV1 { .. } => EventKindV1::BannerEntryColorCommittedV1,
            Self::BannerEntryOperationCompletedV1 { .. } => {
                EventKindV1::BannerEntryOperationCompletedV1
            }
            Self::InspirationResolutionBoundaryV1 { .. } => {
                EventKindV1::InspirationResolutionBoundaryV1
            }
            Self::InspirationResolutionInputV1 { .. } => EventKindV1::InspirationResolutionInputV1,
            Self::InspirationResolutionOperationV1 { .. } => {
                EventKindV1::InspirationResolutionOperationV1
            }
            Self::InspirationEngineDepartureV1 { .. } => EventKindV1::InspirationEngineDepartureV1,
            Self::ProtectionSourceColorDepartureV1 { .. } => {
                EventKindV1::ProtectionSourceColorDepartureV1
            }
            Self::FishingPoleJournalV39 { .. } => EventKindV1::FishingPoleJournalV39,
            Self::TwinflameReplacementInputV38 { .. } => EventKindV1::TwinflameReplacementInputV38,
            Self::TwinflameReplacementOperationV38 { .. } => {
                EventKindV1::TwinflameReplacementOperationV38
            }
            Self::TwinflameDamagePreparationV38 { .. } => {
                EventKindV1::TwinflameDamagePreparationV38
            }
            Self::TwinflameCombatPublishedV38 { .. } => EventKindV1::TwinflameCombatPublishedV38,
            Self::TwinflameCombatInputV38 { .. } => EventKindV1::TwinflameCombatInputV38,
            Self::TwinflameCombatOperationV38 { .. } => EventKindV1::TwinflameCombatOperationV38,
            Self::TwinflameDynamicDamageSampleV38 { .. } => {
                EventKindV1::TwinflameDynamicDamageSampleV38
            }
            Self::TwinflameFieryCaptureV38 { .. } => EventKindV1::TwinflameFieryCaptureV38,
            Self::TwinflameFieryFinishedV38 { .. } => EventKindV1::TwinflameFieryFinishedV38,
            Self::AjaniJournalV42 { .. } => EventKindV1::AjaniJournalV42,
            Self::FaebloomJournalV43 { .. } => EventKindV1::FaebloomJournalV43,
            Self::HighSocietyHunterAttackCaptureV1 { .. } => {
                EventKindV1::HighSocietyHunterAttackCaptureV1
            }
            Self::HighSocietyHunterAttackCollectedV1 { .. } => {
                EventKindV1::HighSocietyHunterAttackCollectedV1
            }
            Self::MandatoryCastPaymentV1 { .. } => EventKindV1::MandatoryCastPaymentV1,
            Self::MandatoryCastPaymentInputV1 { .. } => EventKindV1::MandatoryCastPaymentInputV1,
            Self::HighSocietyHunterJournalV45 { .. } => EventKindV1::HighSocietyHunterJournalV45,
            Self::FiendishPandaLifeGainCaptureV1 { .. } => {
                EventKindV1::FiendishPandaLifeGainCaptureV1
            }
            Self::FiendishPandaDeathCaptureV1 { .. } => EventKindV1::FiendishPandaDeathCaptureV1,
            Self::FiendishPandaCaptureCollectedV1 { .. } => {
                EventKindV1::FiendishPandaCaptureCollectedV1
            }
            Self::FiendishPandaJournalV1 { .. } => EventKindV1::FiendishPandaJournalV1,
            Self::TokenCreationReplacementV48 { .. } => EventKindV1::TokenCreationReplacementV48,
            Self::PlusCounterReplacementV48 { .. } => EventKindV1::PlusCounterReplacementV48,
            Self::TokenCreationInputV48 { .. } => EventKindV1::TokenCreationInputV48,
            Self::TokenAttachmentJournalV48 { .. } => EventKindV1::TokenAttachmentJournalV48,
            Self::BaitTransferInputV48 { .. } => EventKindV1::BaitTransferInputV48,
            Self::BaitTransferAppliedV48 { .. } => EventKindV1::BaitTransferAppliedV48,
            Self::PlusCounterInputV48 { .. } => EventKindV1::PlusCounterInputV48,
            Self::ConquerorLossBatchV49 { .. } => EventKindV1::ConquerorLossBatchV49,
            Self::ConquerorCapturedV49 { .. } => EventKindV1::ConquerorCapturedV49,
            Self::ConquerorCollectedV49 { .. } => EventKindV1::ConquerorCollectedV49,
            Self::ConquerorJournalV49 { .. } => EventKindV1::ConquerorJournalV49,
            Self::ConquerorObserverInputV49 { .. } => EventKindV1::ConquerorObserverInputV49,
            Self::FakeJournalV50 { .. } => EventKindV1::FakeJournalV50,
            Self::SoulCounterAddedV1 { .. } => EventKindV1::SoulCounterAddedV1,
            Self::SoulCounterInputV1 { .. } => EventKindV1::SoulCounterInputV1,
            Self::RavenousPaymentV1 { .. } => EventKindV1::RavenousPaymentV1,
            Self::RavenousRemovedV1 { .. } => EventKindV1::RavenousRemovedV1,
            Self::RavenousCompletedV1 { .. } => EventKindV1::RavenousCompletedV1,
            Self::DrakeJournalV1 { .. } => EventKindV1::DrakeJournalV1,
            Self::DrakeHistoryRefV1 { .. } => EventKindV1::DrakeHistoryRefV1,
            Self::ConquerorGainReplayBorrowV49 { .. } => EventKindV1::ConquerorGainReplayBorrowV49,
            Self::AleshaPowerLkiV1 { .. } => EventKindV1::AleshaPowerLkiV1,
            Self::NivMizzetVisionaryDamageV1 { .. } => EventKindV1::NivMizzetVisionaryDamageV1,
            Self::NivMizzetVisionaryTriggerStackedV1 { .. } => {
                EventKindV1::NivMizzetVisionaryTriggerStackedV1
            }
            Self::NivMizzetVisionaryDamageSourceDepartureV1 { .. } => {
                EventKindV1::NivMizzetVisionaryDamageSourceDepartureV1
            }
            Self::KinshipEntryV1 { .. } => EventKindV1::KinshipEntryV1,
            Self::SphinxsTutelageDrawV1 { .. } => EventKindV1::SphinxsTutelageDrawV1,
            Self::SphinxsTutelageCollectedV1 { .. } => EventKindV1::SphinxsTutelageCollectedV1,
            Self::SphinxsTutelagePublishedV1 { .. } => EventKindV1::SphinxsTutelagePublishedV1,
            Self::SphinxsTutelageMillStartedV1 { .. } => EventKindV1::SphinxsTutelageMillStartedV1,
            Self::SphinxsTutelageMillBatchV1 { .. } => EventKindV1::SphinxsTutelageMillBatchV1,
            Self::LathrilCombatDamageV1 { .. } => EventKindV1::LathrilCombatDamageV1,
            Self::LathrilCombatCollectedV1 { .. } => EventKindV1::LathrilCombatCollectedV1,
            Self::LathrilCombatPublishedV1 { .. } => EventKindV1::LathrilCombatPublishedV1,
            Self::LathrilCombatResolvedV1 { .. } => EventKindV1::LathrilCombatResolvedV1,
            Self::LathrilCombatCounteredV1 { .. } => EventKindV1::LathrilCombatCounteredV1,
            Self::LathrilCombatObservedV1 { .. } => EventKindV1::LathrilCombatObservedV1,
            Self::CopiedTokenAbilitiesEnteredV1 { .. } => {
                EventKindV1::CopiedTokenAbilitiesEnteredV1
            }
            Self::CopiedTokenEndStepV1 { .. } => EventKindV1::CopiedTokenEndStepV1,
            Self::CopiedTokenEndStepCollectedV1 { .. } => {
                EventKindV1::CopiedTokenEndStepCollectedV1
            }
            Self::CopiedTokenSacrificePublishedV1 { .. } => {
                EventKindV1::CopiedTokenSacrificePublishedV1
            }
            Self::CopiedTokenSacrificeFinishedV1 { .. } => {
                EventKindV1::CopiedTokenSacrificeFinishedV1
            }
            Self::CuratorEnteredV1 { .. } => EventKindV1::CuratorEnteredV1,
            Self::CuratorCollectedV1 { .. } => EventKindV1::CuratorCollectedV1,
            Self::CuratorPublishedV1 { .. } => EventKindV1::CuratorPublishedV1,
            Self::CuratorFinishedV1 { .. } => EventKindV1::CuratorFinishedV1,
            Self::CuratorProgressV1 { .. } => EventKindV1::CuratorProgressV1,
            Self::KykarExiledV1 { .. } => EventKindV1::KykarExiledV1,
            Self::KykarDelayedCollectedV1 { .. } => EventKindV1::KykarDelayedCollectedV1,
            Self::GreatwurmCounterPaymentV1 { .. } => EventKindV1::GreatwurmCounterPaymentV1,
            Self::GreatwurmCombatCaptureV1 { .. } => EventKindV1::GreatwurmCombatCaptureV1,
            Self::GreatwurmCombatBoundV1 { .. } => EventKindV1::GreatwurmCombatBoundV1,
            Self::GreatwurmCollectedV1 { .. } => EventKindV1::GreatwurmCollectedV1,
            Self::GreatwurmPublishedV1 { .. } => EventKindV1::GreatwurmPublishedV1,
            Self::GreatwurmFinishedV1 { .. } => EventKindV1::GreatwurmFinishedV1,
            Self::GreatwurmCounterInputV1 { .. } => EventKindV1::GreatwurmCounterInputV1,
            Self::SourceCopyDepartureV1 { .. } => EventKindV1::SourceCopyDepartureV1,
            Self::SphinxLoreV1 { .. } => EventKindV1::SphinxLoreV1,
            Self::SphinxLoreCastOptionsV1 { .. } => EventKindV1::SphinxLoreCastOptionsV1,
            Self::SphinxLoreCastSelectedV1 { .. } => EventKindV1::SphinxLoreCastSelectedV1,
            Self::OmniscienceCastOptionsV1 { .. } => EventKindV1::OmniscienceCastOptionsV1,
            Self::OmniscienceCastSelectedV1 { .. } => EventKindV1::OmniscienceCastSelectedV1,
            Self::NightmareCopyEnteredV1 { .. } => EventKindV1::NightmareCopyEnteredV1,
            Self::NightmareCopyLayerV1 { .. } => EventKindV1::NightmareCopyLayerV1,
            Self::HarvesterGraveyardEntryV1 { .. } => EventKindV1::HarvesterGraveyardEntryV1,
            Self::HarvesterResolvedV1 { .. } => EventKindV1::HarvesterResolvedV1,
            Self::HarvesterCastAnnouncedV1 { .. } => EventKindV1::HarvesterCastAnnouncedV1,
            Self::HarvesterCastReversedV1 { .. } => EventKindV1::HarvesterCastReversedV1,
            Self::ProgenitusInputV1 { .. } => EventKindV1::ProgenitusInputV1,
            Self::ProgenitusShuffledV1 { .. } => EventKindV1::ProgenitusShuffledV1,
            Self::LeylineOpeningV1 { .. } => EventKindV1::LeylineOpeningV1,
            Self::FiendArtisanInputV1 { .. } => EventKindV1::FiendArtisanInputV1,
            Self::FiendArtisanPaidV1 { .. } => EventKindV1::FiendArtisanPaidV1,
            Self::FiendArtisanSearchStartedV1 { .. } => EventKindV1::FiendArtisanSearchStartedV1,
            Self::FiendArtisanSearchFinishedV1 { .. } => EventKindV1::FiendArtisanSearchFinishedV1,
            Self::LifeLossWideV59 { .. } => EventKindV1::LifeLossWideV59,
            Self::LifeGainWideV59 { .. } => EventKindV1::LifeGainWideV59,
            Self::ConquerorObserverInputV59 { .. } => EventKindV1::ConquerorObserverInputV59,
            Self::ConquerorLossBatchV59 { .. } => EventKindV1::ConquerorLossBatchV59,
            Self::ConquerorCapturedV59 { .. } => EventKindV1::ConquerorCapturedV59,
            Self::ConquerorCollectedV59 { .. } => EventKindV1::ConquerorCollectedV59,
            Self::ConquerorJournalV59 { .. } => EventKindV1::ConquerorJournalV59,
            Self::FiendishPandaWideGainCaptureV59 { .. } => {
                EventKindV1::FiendishPandaWideGainCaptureV59
            }
            Self::ThousandYearStormV1 { .. } => EventKindV1::ThousandYearStormV1,
            Self::TimeStopJournalV1 { .. } => EventKindV1::TimeStopJournalV1,
            Self::OrdinaryCleanupStageV1 { .. } => EventKindV1::OrdinaryCleanupStageV1,
            Self::GenesisWaveProgressV1 { .. } => EventKindV1::GenesisWaveProgressV1,
            Self::GenesisWaveFinishedV1 { .. } => EventKindV1::GenesisWaveFinishedV1,
            Self::PublishedCastIdentityV1 { .. } => EventKindV1::PublishedCastIdentityV1,
            Self::VivienReidV1 { .. } => EventKindV1::VivienReidV1,
            Self::MuldrothaOptionsV1 { .. } => EventKindV1::MuldrothaOptionsV1,
            Self::MuldrothaSelectedV1 { .. } => EventKindV1::MuldrothaSelectedV1,
            Self::MuldrothaPlayedV1 { .. } => EventKindV1::MuldrothaPlayedV1,
            Self::TinybonesV72 { .. } => EventKindV1::TinybonesV72,
            Self::LilianaDreadhordeV1 { .. } => EventKindV1::LilianaDreadhordeV1,
            Self::ChandraFlameshaperV74 { .. } => EventKindV1::ChandraFlameshaperV74,
            Self::KaitoCunningV75 { .. } => EventKindV1::KaitoCunningV75,
            Self::DrakusethDamageSourceV76 { .. } => EventKindV1::DrakusethDamageSourceV76,
            Self::NishobaDamageSourceV1 { .. } => EventKindV1::NishobaDamageSourceV1,
            Self::FullTextTriggerCapturedV1 { .. } => EventKindV1::FullTextTriggerCapturedV1,
            Self::FullTextBatchEntryV1 { .. } => EventKindV1::FullTextBatchEntryV1,
            Self::FullTextRestrictedUseV1 { .. } => EventKindV1::FullTextRestrictedUseV1,
            Self::OrderedGraveyardControlGainedV1 { .. } => {
                EventKindV1::OrderedGraveyardControlGainedV1
            }
            Self::MartyrRevealPaidV1 { .. } => EventKindV1::MartyrRevealPaidV1,
            Self::MeddlingMageEntryV1 { .. } => EventKindV1::MeddlingMageEntryV1,
            Self::EternalOpeningV1 { .. } => EventKindV1::EternalOpeningV1,
            Self::EternalOwnedDrawV1 { .. } => EventKindV1::EternalOwnedDrawV1,
        }
    }
}

/// The v2 hash chain over prefixes of the events: each prefix length and the
/// SHA-256 state after it, shortest first. Only an acceleration: the chain is
/// a function of the events, so it never enters a comparison, hash, print or
/// record. Earlier prefixes let a copy of a prefix (a Drake enclosing replay
/// input) resume its chain instead of rehashing every event.
#[derive(Default)]
struct ChainV2(Mutex<ChainPrefixesV2>);

#[derive(Clone)]
struct ChainPrefixesV2 {
    strides: SharedSequence<(usize, Sha256), 0>,
    recent: Vec<(usize, Sha256)>,
}

impl Default for ChainPrefixesV2 {
    fn default() -> Self {
        Self {
            strides: SharedSequence::new(|_| 0),
            recent: Vec::new(),
        }
    }
}

/// The most recent prefix states one history keeps; the oldest are dropped
/// first.
const CHAIN_V2_PREFIXES: usize = 16;
/// Every prefix whose length is a multiple of this is kept as well, so a copy
/// of any earlier prefix rehashes fewer than this many events.
const CHAIN_V2_STRIDE: usize = 64;

impl ChainV2 {
    fn get(&self) -> ChainPrefixesV2 {
        self.0.lock().map(|chain| chain.clone()).unwrap_or_default()
    }

    /// The longest chained prefix of at most `len` events.
    fn longest(&self, len: usize) -> Option<(usize, Sha256)> {
        let prefixes = self.get();
        let end = prefixes
            .strides
            .partition_point(|(covered, _)| *covered <= len);
        let stride = end
            .checked_sub(1)
            .and_then(|i| prefixes.strides.get(i))
            .cloned();
        let recent = prefixes
            .recent
            .iter()
            .rev()
            .find(|(covered, _)| *covered <= len)
            .cloned();
        match (stride, recent) {
            (Some(a), Some(b)) => Some(if a.0 >= b.0 { a } else { b }),
            (a, b) => a.or(b),
        }
    }

    /// Records `chains`, shortest first, past every kept prefix they extend.
    fn set(&self, chains: Vec<(usize, Sha256)>) {
        let Some(&(shortest, _)) = chains.first() else {
            return;
        };
        if let Ok(mut slot) = self.0.lock() {
            let before = slot
                .strides
                .partition_point(|(covered, _)| *covered < shortest);
            slot.strides.truncate(before);
            slot.recent.retain(|(covered, _)| *covered < shortest);
            for checkpoint in chains {
                if checkpoint.0.is_multiple_of(CHAIN_V2_STRIDE) {
                    slot.strides.push(checkpoint);
                } else {
                    slot.recent.push(checkpoint);
                }
            }
            let excess = slot.recent.len().saturating_sub(CHAIN_V2_PREFIXES);
            slot.recent.drain(..excess);
        }
    }

    /// Drops every chained prefix longer than the first `len` events.
    fn keep(&mut self, len: usize) {
        let slot = self
            .0
            .get_mut()
            .unwrap_or_else(|poison| poison.into_inner());
        let keep = slot.strides.partition_point(|(covered, _)| *covered <= len);
        slot.strides.truncate(keep);
        slot.recent.retain(|(covered, _)| *covered <= len);
    }

    fn forget(&mut self) {
        *self
            .0
            .get_mut()
            .unwrap_or_else(|poison| poison.into_inner()) = ChainPrefixesV2::default();
    }
}

impl Clone for ChainV2 {
    fn clone(&self) -> Self {
        Self(Mutex::new(self.get()))
    }
}

/// A shared memo about the first `covered` events.
pub(crate) type PrefixMemoV1 = (usize, Arc<dyn Any + Send + Sync>);

/// The validators that keep proofs with the history
/// ([`EventHistoryV1::proof_v1`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProvedV1 {
    /// Arahbo's entry, observation and capture-count checks.
    ArahboObservations,
    /// Garruk's Uprising's entry, observation and capture-count checks.
    GarrukObservations,
    /// The color departure checks that read only the history and the
    /// snapshots.
    ColorDepartures,
}

const PROVED_V1: usize = 3;

/// Each validator keeps its latest proofs...
const PROOFS_RECENT_V1: usize = 8;
/// ...and the first one recorded in each block of this many events, so a
/// copy of an earlier prefix (a Drake verification replay) keeps one near
/// its end.
const PROOFS_STRIDE_V1: usize = 64;

/// For each validator, the history lengths at which its history-only checks
/// passed, each with its data, shortest first.
#[derive(Clone, Default)]
struct ProofsV1([Vec<(usize, u64)>; PROVED_V1]);

impl ProofsV1 {
    fn longest(&self) -> usize {
        self.0
            .iter()
            .filter_map(|proofs| proofs.last().map(|(covered, _)| *covered))
            .max()
            .unwrap_or(0)
    }

    fn keep(&mut self, len: usize) {
        for proofs in &mut self.0 {
            let kept = proofs.partition_point(|(covered, _)| *covered <= len);
            proofs.truncate(kept);
        }
    }

    /// Whether a proof of `validator` at least `covered` long is kept.
    fn covers(&self, validator: ProvedV1, covered: usize) -> bool {
        self.0[validator as usize]
            .last()
            .is_some_and(|(last, _)| *last >= covered)
    }

    fn record(&mut self, validator: ProvedV1, covered: usize, data: u64) {
        let proofs = &mut self.0[validator as usize];
        proofs.push((covered, data));
        let recent = proofs.len().saturating_sub(PROOFS_RECENT_V1);
        let mut index = 0;
        let mut block = None;
        proofs.retain(|(covered, _)| {
            let first_in_block = block != Some(covered / PROOFS_STRIDE_V1);
            block = Some(covered / PROOFS_STRIDE_V1);
            index += 1;
            index > recent || first_in_block
        });
    }
}

/// What validators established about the first `covered` events: one
/// validator's memo and the validators' proofs. Each is kept only while the
/// events it covers stay unchanged: `push` keeps them, the other counting
/// methods keep those that cover only events they leave alone, and
/// `retain`, `clear` and any `DerefMut` access drop them all. Clones carry
/// them. Only an acceleration, like the chain: never compared, hashed,
/// printed or recorded.
#[derive(Clone, Default)]
struct PrefixMemosV1 {
    memo: Option<PrefixMemoV1>,
    /// Shared by clones until one of them records.
    proofs: Option<Arc<ProofsV1>>,
}

#[derive(Default)]
struct PrefixSlotV1(Mutex<PrefixMemosV1>);

impl PrefixSlotV1 {
    fn get(&self) -> PrefixMemosV1 {
        self.0.lock().map(|slot| slot.clone()).unwrap_or_default()
    }

    fn memo(&self) -> Option<PrefixMemoV1> {
        self.0.lock().ok().and_then(|slot| slot.memo.clone())
    }

    fn proof(&self, validator: ProvedV1, len: usize) -> Option<(usize, u64)> {
        let slot = self.0.lock().ok()?;
        let proofs = &slot.proofs.as_ref()?.0[validator as usize];
        let end = proofs.partition_point(|(covered, _)| *covered <= len);
        end.checked_sub(1).map(|at| proofs[at])
    }

    /// Records a proof unless one at least as long is kept. Copies the
    /// proofs only when a clone shares them.
    fn record(&self, validator: ProvedV1, covered: usize, data: u64) {
        if let Ok(mut slot) = self.0.lock() {
            let proofs = slot.proofs.get_or_insert_with(Default::default);
            if !proofs.covers(validator, covered) {
                Arc::make_mut(proofs).record(validator, covered, data);
            }
        }
    }

    /// Drops what covers more than the first `len` events.
    fn keep(&mut self, len: usize) {
        let slot = self
            .0
            .get_mut()
            .unwrap_or_else(|poison| poison.into_inner());
        if slot
            .memo
            .as_ref()
            .is_some_and(|(covered, _)| *covered > len)
        {
            slot.memo = None;
        }
        if let Some(proofs) = slot.proofs.as_mut().filter(|proofs| proofs.longest() > len) {
            Arc::make_mut(proofs).keep(len);
        }
    }

    fn forget(&mut self) {
        *self
            .0
            .get_mut()
            .unwrap_or_else(|poison| poison.into_inner()) = PrefixMemosV1::default();
    }
}

impl Clone for PrefixSlotV1 {
    fn clone(&self) -> Self {
        Self(Mutex::new(self.get()))
    }
}

thread_local! {
    /// The address of the one history that serializes as `[]`, while the v2
    /// origin digest writes the rest of its state.
    static OMITTED_V2: Cell<usize> = const { Cell::new(0) };
}

/// `EngineState::event_history`: the events in commit order and the number
/// of events of each kind.
#[derive(Clone)]
pub struct EventHistoryV1 {
    events: SharedSequence<CommittedEvent, EVENT_KIND_COUNT_V1>,
    counts: [u32; EVENT_KIND_COUNT_V1],
    /// Set by a mutable access outside the counting methods.
    stale: bool,
    chain: ChainV2,
    prefix: PrefixSlotV1,
}

fn event_kind_index_v2(event: &CommittedEvent) -> usize {
    event.kind_v1() as usize
}

/// Logical iteration without creating a contiguous copy of the history.
#[derive(Clone)]
pub struct EventHistoryIterV2<'a> {
    inner: shared_sequence_v2::Iter<'a, CommittedEvent, EVENT_KIND_COUNT_V1>,
}

impl<'a> Iterator for EventHistoryIterV2<'a> {
    type Item = &'a CommittedEvent;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.inner.nth(n)
    }
    #[inline]
    fn fold<B, F>(self, init: B, f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        self.inner.fold(init, f)
    }
}

impl DoubleEndedIterator for EventHistoryIterV2<'_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back()
    }
    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.inner.nth_back(n)
    }
    #[inline]
    fn rfold<B, F>(self, init: B, f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        self.inner.rfold(init, f)
    }
}

impl ExactSizeIterator for EventHistoryIterV2<'_> {}
impl std::iter::FusedIterator for EventHistoryIterV2<'_> {}

/// A borrowed logical range. Only explicitly requesting Vec/slice compatibility
/// materializes storage; iterating this range never copies its prefix.
#[derive(Clone, Copy)]
pub struct EventHistoryRangeV2<'a> {
    history: &'a EventHistoryV1,
    start: usize,
    end: usize,
}

impl<'a> EventHistoryRangeV2<'a> {
    pub fn len(&self) -> usize {
        self.end - self.start
    }
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
    #[inline]
    pub fn iter(&self) -> EventHistoryIterV2<'a> {
        EventHistoryIterV2 {
            inner: self.history.events.range(self.start..self.end),
        }
    }
    pub fn to_vec(&self) -> Vec<CommittedEvent> {
        self.iter().cloned().collect()
    }
}

impl<'a> IntoIterator for EventHistoryRangeV2<'a> {
    type Item = &'a CommittedEvent;
    type IntoIter = EventHistoryIterV2<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &EventHistoryRangeV2<'a> {
    type Item = &'a CommittedEvent;
    type IntoIter = EventHistoryIterV2<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl PartialEq for EventHistoryRangeV2<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl Eq for EventHistoryRangeV2<'_> {}

impl fmt::Debug for EventHistoryRangeV2<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl Serialize for EventHistoryRangeV2<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter())
    }
}

/// Concrete usize and standard range indices retain their original results.
/// Generic indices now require this trait rather than only `SliceIndex`.
/// Single-event reads use the shared tree; contiguous slice requests and
/// arbitrary mutation are cold compatibility paths. Hot range readers use
/// `range_v2` instead.
pub trait EventHistoryIndexV2 {
    type Output: ?Sized;
    fn get(self, history: &EventHistoryV1) -> Option<&Self::Output>;
    fn get_mut(self, history: &mut EventHistoryV1) -> Option<&mut Self::Output>;
}

impl EventHistoryIndexV2 for usize {
    type Output = CommittedEvent;
    fn get(self, history: &EventHistoryV1) -> Option<&Self::Output> {
        history.events.get(self)
    }
    fn get_mut(self, history: &mut EventHistoryV1) -> Option<&mut Self::Output> {
        history.deref_mut().get_mut(self)
    }
}

macro_rules! contiguous_history_index_v2 {
    ($($index:ty),* $(,)?) => {$(
        impl EventHistoryIndexV2 for $index {
            type Output = [CommittedEvent];
            fn get(self, history: &EventHistoryV1) -> Option<&Self::Output> {
                history.events.as_vec().get(self)
            }
            fn get_mut(self, history: &mut EventHistoryV1) -> Option<&mut Self::Output> {
                history.deref_mut().get_mut(self)
            }
        }
    )*};
}

contiguous_history_index_v2!(
    std::ops::Range<usize>,
    std::ops::RangeFrom<usize>,
    std::ops::RangeTo<usize>,
    std::ops::RangeFull,
    std::ops::RangeInclusive<usize>,
    std::ops::RangeToInclusive<usize>,
    (Bound<usize>, Bound<usize>),
);

impl<I: EventHistoryIndexV2> Index<I> for EventHistoryV1 {
    type Output = I::Output;
    fn index(&self, index: I) -> &Self::Output {
        index.get(self).expect("history index out of bounds")
    }
}

impl<I: EventHistoryIndexV2> IndexMut<I> for EventHistoryV1 {
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        index.get_mut(self).expect("history index out of bounds")
    }
}

impl EventHistoryV1 {
    pub fn new() -> Self {
        Self {
            events: SharedSequence::new(event_kind_index_v2),
            counts: [0; EVENT_KIND_COUNT_V1],
            stale: false,
            chain: ChainV2::default(),
            prefix: PrefixSlotV1::default(),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.events.len()
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
    /// Exact prefix equality, skipping shared immutable history subtrees.
    pub fn common_prefix_len_v2(&self, other: &Self) -> usize {
        self.events.common_prefix_len(&other.events)
    }
    #[inline]
    pub fn iter(&self) -> EventHistoryIterV2<'_> {
        EventHistoryIterV2 {
            inner: self.events.iter(),
        }
    }
    #[inline]
    pub fn get<I: EventHistoryIndexV2>(&self, index: I) -> Option<&I::Output> {
        index.get(self)
    }
    pub fn get_mut<I: EventHistoryIndexV2>(&mut self, index: I) -> Option<&mut I::Output> {
        index.get_mut(self)
    }
    pub fn first(&self) -> Option<&CommittedEvent> {
        self.events.get(0)
    }
    pub fn last(&self) -> Option<&CommittedEvent> {
        self.len()
            .checked_sub(1)
            .and_then(|index| self.events.get(index))
    }
    #[inline]
    pub fn get_range_v2(&self, range: impl RangeBounds<usize>) -> Option<EventHistoryRangeV2<'_>> {
        let start = match range.start_bound() {
            Bound::Unbounded => 0,
            Bound::Included(&start) => start,
            Bound::Excluded(&start) => start.checked_add(1)?,
        };
        let end = match range.end_bound() {
            Bound::Unbounded => self.len(),
            Bound::Excluded(&end) => end,
            Bound::Included(&end) => end.checked_add(1)?,
        };
        (start <= end && end <= self.len()).then_some(EventHistoryRangeV2 {
            history: self,
            start,
            end,
        })
    }
    #[inline]
    pub fn range_v2(&self, range: impl RangeBounds<usize>) -> EventHistoryRangeV2<'_> {
        self.get_range_v2(range)
            .expect("history range out of bounds")
    }

    /// Whether an event of one of `kinds` may be in the history: false only
    /// when none is. A check that acts only on events of `kinds` can skip its
    /// scan when this is false.
    #[inline]
    pub fn has_any(&self, kinds: &[EventKindV1]) -> bool {
        #[cfg(test)]
        if guard_tests_v1::scans_forced() {
            return true;
        }
        self.stale || kinds.iter().any(|&kind| self.counts[kind as usize] != 0)
    }

    /// The events a scan for `kinds` has to visit: all of them when an event
    /// of one of `kinds` may occur, none when none does. A scan that acts
    /// only on events of `kinds` gives the same result over either.
    #[cfg_attr(test, track_caller)]
    pub fn scan_for_v1(&self, kinds: &[EventKindV1]) -> EventHistoryRangeV2<'_> {
        let any = self.has_any(kinds);
        #[cfg(test)]
        guard_tests_v1::note_scan(std::panic::Location::caller(), !any);
        if any {
            self.range_v2(..)
        } else {
            self.range_v2(..0)
        }
    }

    /// The events of `kinds` with their positions, in history order: what
    /// filtering `iter().enumerate()` to those kinds yields. Stored chunks
    /// that count none of `kinds` are skipped whole, so a check of rare events
    /// reads only the chunks that hold one, not the whole history.
    ///
    /// As with `scan_for_v1`, a check must act only on events of `kinds`:
    /// with every guard forced open (tests) it is given every event, so a
    /// check that needs a kind it does not list gives a different result.
    #[cfg_attr(test, track_caller)]
    pub(crate) fn of_kinds_v2(
        &self,
        kinds: &[EventKindV1],
    ) -> impl Iterator<Item = (usize, &CommittedEvent)> + '_ {
        #[cfg(test)]
        {
            if guard_tests_v1::scans_forced() {
                return self.events.every_element_with_position();
            }
            guard_tests_v1::note_scan(std::panic::Location::caller(), !self.has_any(kinds));
        }
        // The usual answer is none: the history's own counts give it without
        // building a kind set or reading the tree.
        if !self.has_any(kinds) {
            return self.events.of_no_kinds();
        }
        self.events
            .of_kinds(kinds.iter().map(|&kind| kind as usize))
    }

    pub fn push(&mut self, event: CommittedEvent) {
        #[cfg(any(test, feature = "event-sample-harvest-v1"))]
        sample_harvest_v1::note(&event);
        self.settle();
        self.counts[event.kind_v1() as usize] += 1;
        self.events.push(event);
    }

    pub fn insert(&mut self, index: usize, event: CommittedEvent) {
        #[cfg(any(test, feature = "event-sample-harvest-v1"))]
        sample_harvest_v1::note(&event);
        self.settle();
        self.chain.keep(index);
        self.prefix.keep(index);
        let kind = event.kind_v1();
        self.events.as_mut_vec().insert(index, event);
        self.events.repack();
        self.counts[kind as usize] += 1;
    }

    pub fn remove(&mut self, index: usize) -> CommittedEvent {
        self.settle();
        self.chain.keep(index);
        self.prefix.keep(index);
        let event = self.events.as_mut_vec().remove(index);
        self.events.repack();
        self.counts[event.kind_v1() as usize] -= 1;
        event
    }

    pub fn pop(&mut self) -> Option<CommittedEvent> {
        self.settle();
        let len = self.events.len().checked_sub(1)?;
        let event = self.events.get(len)?.clone();
        self.events.truncate(len);
        self.chain.keep(self.events.len());
        self.prefix.keep(self.events.len());
        self.counts[event.kind_v1() as usize] -= 1;
        Some(event)
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&CommittedEvent) -> bool) {
        self.settle();
        self.chain.forget();
        self.prefix.forget();
        let Self { events, counts, .. } = self;
        events.as_mut_vec().retain(|event| {
            let kept = keep(event);
            if !kept {
                counts[event.kind_v1() as usize] -= 1;
            }
            kept
        });
        events.repack();
    }

    pub fn truncate(&mut self, len: usize) {
        self.settle();
        self.chain.keep(len);
        self.prefix.keep(len);
        self.events.truncate(len);
        self.counts = self.events.counts();
    }

    /// A copy of the first `len` events (all of them if `len` is longer),
    /// keeping the chained prefixes and the prefix memo it contains.
    pub(crate) fn prefix_v1(&self, len: usize) -> Self {
        let len = len.min(self.events.len());
        let events = self.events.prefix(len);
        let mut prefix = Self {
            counts: events.counts(),
            events,
            stale: false,
            chain: self.chain.clone(),
            prefix: self.prefix.clone(),
        };
        prefix.chain.keep(len);
        prefix.prefix.keep(len);
        prefix
    }

    /// Whether unrestricted mutable access left the history in editable form.
    pub(crate) fn is_editable_v2(&self) -> bool {
        self.stale
    }

    /// The memo a validator left about a prefix of these events, if those
    /// events are unchanged since: `(covered, memo)` with `covered` at most
    /// the current length.
    pub(crate) fn prefix_memo_v1(&self) -> Option<PrefixMemoV1> {
        self.prefix
            .memo()
            .filter(|(covered, _)| *covered <= self.events.len())
    }

    /// Leaves `memo` about the first `covered` events (ignored past the end).
    /// The caller must have derived it from exactly these events.
    /// Drake memos also preserve the ownership invariant on `CheckedPrefixV1`.
    pub(crate) fn set_prefix_memo_v1(&self, covered: usize, memo: Arc<dyn Any + Send + Sync>) {
        if covered <= self.events.len() {
            if let Ok(mut slot) = self.prefix.0.lock() {
                slot.memo = Some((covered, memo));
            }
        }
    }

    /// The longest proof `validator` recorded about a prefix of these events
    /// that is unchanged since: `(covered, data)`, meaning its checks that
    /// read only the history passed on the first `covered` events.
    pub(crate) fn proof_v1(&self, validator: ProvedV1) -> Option<(usize, u64)> {
        #[cfg(test)]
        if proof_tests_v1::off() {
            return None;
        }
        self.prefix.proof(validator, self.events.len())
    }

    /// Records that `validator`'s checks that read only the history passed on
    /// the first `covered` events, with `data` (ignored past the end, or when
    /// a proof at least as long is kept). The caller must have checked
    /// exactly these events.
    pub(crate) fn set_proof_v1(&self, validator: ProvedV1, covered: usize, data: u64) {
        #[cfg(test)]
        if proof_tests_v1::off() {
            return;
        }
        if covered <= self.events.len() {
            self.prefix.record(validator, covered, data);
        }
    }

    pub fn clear(&mut self) {
        self.events = SharedSequence::new(event_kind_index_v2);
        self.counts = [0; EVENT_KIND_COUNT_V1];
        self.stale = false;
        self.chain.forget();
        self.prefix.forget();
    }

    /// The v2 hash chain over every event: SHA-256 of a domain tag, then of
    /// each event's canonical JSON length (u64, little-endian) and bytes, in
    /// order. Hashes only the events past the longest prefix chained before.
    pub(crate) fn chain_digest_v2(&self) -> [u8; 32] {
        let (covered, mut chain) = self.chain.longest(self.events.len()).unwrap_or_else(|| {
            let mut chain = Sha256::new();
            chain.update(b"mtg-kernel/event-history-chain/v2");
            (0, chain)
        });
        let mut chains = Vec::new();
        for (position, event) in self.events.iter().enumerate().skip(covered) {
            if position > covered && position.is_multiple_of(CHAIN_V2_STRIDE) {
                chains.push((position, chain.clone()));
            }
            let bytes = serde_json::to_vec(event).expect("committed events serialize");
            chain.update(
                u64::try_from(bytes.len())
                    .expect("event length")
                    .to_le_bytes(),
            );
            chain.update(&bytes);
        }
        chains.push((self.events.len(), chain.clone()));
        self.chain.set(chains);
        chain.finalize().into()
    }

    /// Runs `write` with this history (and no other) serializing as `[]`.
    pub(crate) fn omitted_v2<T>(&self, write: impl FnOnce() -> T) -> T {
        struct Restore(usize);
        impl Drop for Restore {
            fn drop(&mut self) {
                OMITTED_V2.with(|omitted| omitted.set(self.0));
            }
        }
        let _restore =
            Restore(OMITTED_V2.with(|omitted| omitted.replace(self as *const Self as usize)));
        write()
    }

    /// Clone the persistent prefix for the private Drake checkpoint writer.
    /// Editable histories and callers already inside an omitted write fall back
    /// to the ordinary serializer.
    pub(crate) fn drake_compaction_prefix_v2(&self) -> Option<Self> {
        (!self.stale
            && !Self::serde_omission_active_v2()
            && self.has_any(&[EventKindV1::DrakeJournalV1]))
        .then(|| self.clone())
    }

    pub(crate) fn serde_omission_active_v2() -> bool {
        OMITTED_V2.with(Cell::get) != 0
    }

    fn settle(&mut self) {
        if self.stale {
            self.events.repack();
            self.counts = self.events.counts();
            self.stale = false;
        }
    }
}

impl Default for EventHistoryV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Vec<CommittedEvent>> for EventHistoryV1 {
    fn from(events: Vec<CommittedEvent>) -> Self {
        #[cfg(any(test, feature = "event-sample-harvest-v1"))]
        events.iter().for_each(sample_harvest_v1::note);
        let events = SharedSequence::from_vec(events, event_kind_index_v2);
        Self {
            counts: events.counts(),
            events,
            stale: false,
            chain: ChainV2::default(),
            prefix: PrefixSlotV1::default(),
        }
    }
}

impl Extend<CommittedEvent> for EventHistoryV1 {
    fn extend<I: IntoIterator<Item = CommittedEvent>>(&mut self, events: I) {
        for event in events {
            self.push(event);
        }
    }
}

impl Deref for EventHistoryV1 {
    type Target = Vec<CommittedEvent>;

    fn deref(&self) -> &Vec<CommittedEvent> {
        self.events.as_vec()
    }
}

/// Any change outside the counting methods marks the counts stale and drops
/// the v2 chain and the prefix memo.
impl DerefMut for EventHistoryV1 {
    fn deref_mut(&mut self) -> &mut Vec<CommittedEvent> {
        self.stale = true;
        self.chain.forget();
        self.prefix.forget();
        self.events.as_mut_vec()
    }
}

impl<'a> IntoIterator for &'a EventHistoryV1 {
    type Item = &'a CommittedEvent;
    type IntoIter = EventHistoryIterV2<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut EventHistoryV1 {
    type Item = &'a mut CommittedEvent;
    type IntoIter = std::slice::IterMut<'a, CommittedEvent>;

    fn into_iter(self) -> Self::IntoIter {
        self.deref_mut().iter_mut()
    }
}

// Exactly the `Vec<CommittedEvent>` behavior: the counts are derived from
// the events, so they never enter a comparison, hash, print or record.
impl PartialEq for EventHistoryV1 {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.common_prefix_len_v2(other) == self.len()
    }
}

impl Eq for EventHistoryV1 {}

impl PartialEq<[CommittedEvent]> for EventHistoryV1 {
    fn eq(&self, other: &[CommittedEvent]) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl PartialEq<EventHistoryV1> for [CommittedEvent] {
    fn eq(&self, other: &EventHistoryV1) -> bool {
        other == self
    }
}

impl PartialEq<Vec<CommittedEvent>> for EventHistoryV1 {
    fn eq(&self, other: &Vec<CommittedEvent>) -> bool {
        self == other.as_slice()
    }
}

impl PartialEq<EventHistoryV1> for Vec<CommittedEvent> {
    fn eq(&self, other: &EventHistoryV1) -> bool {
        other == self.as_slice()
    }
}

impl Hash for EventHistoryV1 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.len().hash(state);
        for event in self.iter() {
            event.hash(state);
        }
    }
}

impl fmt::Debug for EventHistoryV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl Serialize for EventHistoryV1 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if OMITTED_V2.with(Cell::get) == self as *const Self as usize {
            return serializer.collect_seq(std::iter::empty::<&CommittedEvent>());
        }
        crate::drake_hatcher_v1::custody::serialize_history(self, serializer)
    }
}

impl<'de> Deserialize<'de> for EventHistoryV1 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Vec::<CommittedEvent>::deserialize(deserializer).map(Self::from)
    }
}

/// Test-only controls of the guards: a forced mode in which no scan is
/// skipped, so a computation can be compared with and without its guards,
/// and a record of which guards were reached and which skipped.
#[cfg(test)]
pub(crate) mod guard_tests_v1 {
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;
    use std::panic::Location;

    /// Times a guard was reached and times it skipped its scan, by the
    /// file and line of the guard.
    pub(crate) type SitesV1 = BTreeMap<(&'static str, u32), [u64; 2]>;

    thread_local! {
        static FORCED: Cell<bool> = const { Cell::new(false) };
        static SITES: RefCell<Option<SitesV1>> = const { RefCell::new(None) };
    }

    pub(crate) fn scans_forced() -> bool {
        FORCED.with(Cell::get)
    }

    pub(crate) fn note_scan(site: &'static Location<'static>, skipped: bool) {
        SITES.with(|sites| {
            if let Some(sites) = sites.borrow_mut().as_mut() {
                let counts = sites.entry((site.file(), site.line())).or_default();
                counts[0] += 1;
                counts[1] += u64::from(skipped);
            }
        });
    }

    /// Runs `f` with every guard open, as if the history had no index.
    pub(crate) fn with_scans_forced<T>(f: impl FnOnce() -> T) -> T {
        struct Restore(bool);
        impl Drop for Restore {
            fn drop(&mut self) {
                FORCED.with(|forced| forced.set(self.0));
            }
        }
        let _restore = Restore(FORCED.with(|forced| forced.replace(true)));
        f()
    }

    /// One event of every kind, in kind order: events the crate's library
    /// and integration tests committed (collected by `sample_harvest_v1`),
    /// and hand-built ones for the kinds no test commits.
    /// `data/event_history_v1/README.md` says how to add a new kind's.
    pub(crate) fn every_kind_v1() -> Vec<super::CommittedEvent> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../data/event_history_v1/every_kind_samples.json");
        let samples = std::fs::read_to_string(path).expect("every-kind samples read");
        serde_json::from_str(&samples).expect("every-kind samples parse")
    }

    /// Runs `f` and returns the guards it reached on this thread.
    pub(crate) fn recording_sites<T>(f: impl FnOnce() -> T) -> (T, SitesV1) {
        struct Restore(Option<SitesV1>);
        impl Drop for Restore {
            fn drop(&mut self) {
                SITES.with(|sites| *sites.borrow_mut() = self.0.take());
            }
        }
        let outer = Restore(SITES.with(|sites| sites.borrow_mut().replace(SitesV1::new())));
        let result = f();
        let sites = SITES
            .with(|sites| sites.borrow_mut().take())
            .unwrap_or_default();
        drop(outer);
        (result, sites)
    }
}

/// Test-only controls of the validators' proofs: a mode that uses and
/// records none, a count of the validations that used one, and the check
/// that each such validation gives the full check's result.
#[cfg(test)]
pub(crate) mod proof_tests_v1 {
    use std::cell::Cell;

    thread_local! {
        static OFF: Cell<bool> = const { Cell::new(false) };
        static USES: Cell<u64> = const { Cell::new(0) };
    }

    pub(crate) fn off() -> bool {
        OFF.with(Cell::get)
    }

    /// Runs `f` with no proof used or recorded, as before proofs.
    pub(crate) fn without_proofs<T>(f: impl FnOnce() -> T) -> T {
        struct Restore(bool);
        impl Drop for Restore {
            fn drop(&mut self) {
                OFF.with(|off| off.set(self.0));
            }
        }
        let _restore = Restore(OFF.with(|off| off.replace(true)));
        f()
    }

    /// Validations on this thread that used a proof.
    pub(crate) fn uses() -> u64 {
        USES.with(Cell::get)
    }

    /// Counts a validation that used a proof, and asserts it gave what the
    /// full check gives.
    #[track_caller]
    pub(crate) fn assert_same(
        validator: super::ProvedV1,
        proved: &Result<(), String>,
        full: &Result<(), String>,
    ) {
        USES.with(|uses| uses.set(uses.get() + 1));
        assert_eq!(proved, full, "{validator:?} proof changed the verdict");
    }
}

/// Test-only sample source: when `MTG_EVENT_SAMPLES_V1` names a directory,
/// the first event of each kind a test process commits is written there as
/// `<Kind>.json`. It is on in the crate's unit tests, and in integration
/// tests built with the `event-sample-harvest-v1` feature. Most every-kind
/// fixtures were collected this way.
#[cfg(any(test, feature = "event-sample-harvest-v1"))]
pub(crate) mod sample_harvest_v1 {
    use super::{CommittedEvent, EVENT_KIND_COUNT_V1};
    use std::sync::atomic::{AtomicBool, Ordering};

    static SEEN: [AtomicBool; EVENT_KIND_COUNT_V1] =
        [const { AtomicBool::new(false) }; EVENT_KIND_COUNT_V1];

    pub(crate) fn note(event: &CommittedEvent) {
        let kind = event.kind_v1();
        if SEEN[kind as usize].swap(true, Ordering::Relaxed) {
            return;
        }
        let Some(directory) = std::env::var_os("MTG_EVENT_SAMPLES_V1") else {
            return;
        };
        let path = std::path::Path::new(&directory).join(format!("{kind:?}.json"));
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            use std::io::Write;
            file.write_all(&serde_json::to_vec(event).expect("event serializes"))
                .expect("sample written");
        }
    }
}

#[cfg(test)]
#[path = "event_history_v1_tests.rs"]
mod tests;
