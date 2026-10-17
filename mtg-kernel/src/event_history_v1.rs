//! The engine's permanent event record, with a count of each event kind.
//!
//! [`EventHistoryV1`] holds exactly the `Vec<CommittedEvent>` that
//! `EngineState::event_history` always held, and serializes, deserializes,
//! hashes, compares and prints exactly as that `Vec` does, so no state byte,
//! digest or receipt changes. Beside it, it counts the events of each kind,
//! so a check that acts only on some kinds can skip its scan of the whole
//! history when none of them occurred ([`EventHistoryV1::has_any`]).
//!
//! The counting methods (`push`, `insert`, `remove`, `pop`, `retain`,
//! `truncate`, `clear`, `extend`) keep the counts equal to the events. Any
//! other mutable access goes through `DerefMut` (or `&mut` iteration),
//! which marks the counts stale: while they are stale `has_any` answers
//! true, so every scan runs as before, and the next counting method
//! recounts.

use crate::event::CommittedEvent;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};

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
    LilianaDreadhordeV1,
}

/// Number of `CommittedEvent` variants (and of `EventKindV1` values).
pub const EVENT_KIND_COUNT_V1: usize = 310;
const _: () = assert!(EventKindV1::LilianaDreadhordeV1 as usize + 1 == EVENT_KIND_COUNT_V1);

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
            Self::LilianaDreadhordeV1 { .. } => EventKindV1::LilianaDreadhordeV1,
        }
    }
}

/// `EngineState::event_history`: the events in commit order and the number
/// of events of each kind.
#[derive(Clone)]
pub struct EventHistoryV1 {
    events: Vec<CommittedEvent>,
    counts: [u32; EVENT_KIND_COUNT_V1],
    /// Set by a mutable access outside the counting methods.
    stale: bool,
}

impl EventHistoryV1 {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            counts: [0; EVENT_KIND_COUNT_V1],
            stale: false,
        }
    }

    /// Whether an event of one of `kinds` may be in the history: false only
    /// when none is. A check that acts only on events of `kinds` can skip its
    /// scan when this is false.
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
    pub fn scan_for_v1(&self, kinds: &[EventKindV1]) -> &[CommittedEvent] {
        let any = self.has_any(kinds);
        #[cfg(test)]
        guard_tests_v1::note_scan(std::panic::Location::caller(), !any);
        if any {
            &self.events
        } else {
            &[]
        }
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
        let kind = event.kind_v1();
        self.events.insert(index, event);
        self.counts[kind as usize] += 1;
    }

    pub fn remove(&mut self, index: usize) -> CommittedEvent {
        self.settle();
        let event = self.events.remove(index);
        self.counts[event.kind_v1() as usize] -= 1;
        event
    }

    pub fn pop(&mut self) -> Option<CommittedEvent> {
        self.settle();
        let event = self.events.pop()?;
        self.counts[event.kind_v1() as usize] -= 1;
        Some(event)
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&CommittedEvent) -> bool) {
        self.settle();
        let Self { events, counts, .. } = self;
        events.retain(|event| {
            let kept = keep(event);
            if !kept {
                counts[event.kind_v1() as usize] -= 1;
            }
            kept
        });
    }

    pub fn truncate(&mut self, len: usize) {
        self.settle();
        for event in self.events.get(len..).unwrap_or_default() {
            self.counts[event.kind_v1() as usize] -= 1;
        }
        self.events.truncate(len);
    }

    pub fn clear(&mut self) {
        self.events.clear();
        self.counts = [0; EVENT_KIND_COUNT_V1];
        self.stale = false;
    }

    fn settle(&mut self) {
        if self.stale {
            self.counts = [0; EVENT_KIND_COUNT_V1];
            for event in &self.events {
                self.counts[event.kind_v1() as usize] += 1;
            }
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
        let mut history = Self {
            events,
            counts: [0; EVENT_KIND_COUNT_V1],
            stale: true,
        };
        history.settle();
        history
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
        &self.events
    }
}

/// Any change outside the counting methods marks the counts stale.
impl DerefMut for EventHistoryV1 {
    fn deref_mut(&mut self) -> &mut Vec<CommittedEvent> {
        self.stale = true;
        &mut self.events
    }
}

impl<'a> IntoIterator for &'a EventHistoryV1 {
    type Item = &'a CommittedEvent;
    type IntoIter = std::slice::Iter<'a, CommittedEvent>;

    fn into_iter(self) -> Self::IntoIter {
        self.events.iter()
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
        self.events == other.events
    }
}

impl Eq for EventHistoryV1 {}

impl PartialEq<[CommittedEvent]> for EventHistoryV1 {
    fn eq(&self, other: &[CommittedEvent]) -> bool {
        self.events[..] == *other
    }
}

impl PartialEq<EventHistoryV1> for [CommittedEvent] {
    fn eq(&self, other: &EventHistoryV1) -> bool {
        *self == other.events[..]
    }
}

impl PartialEq<Vec<CommittedEvent>> for EventHistoryV1 {
    fn eq(&self, other: &Vec<CommittedEvent>) -> bool {
        self.events == *other
    }
}

impl PartialEq<EventHistoryV1> for Vec<CommittedEvent> {
    fn eq(&self, other: &EventHistoryV1) -> bool {
        *self == other.events
    }
}

impl Hash for EventHistoryV1 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.events.hash(state);
    }
}

impl fmt::Debug for EventHistoryV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.events, f)
    }
}

impl Serialize for EventHistoryV1 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.events.serialize(serializer)
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
