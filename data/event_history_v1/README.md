# Every-kind event samples

`every_kind_samples.json` holds one `CommittedEvent` of each kind, one JSON event
per line, in kind order: the order of the variants in `CommittedEvent`, which
`EventKindV1` follows. The event-history tests read it at run time:

- `event_history_v1::tests` check that `EventHistoryV1` serializes, digests,
  compares and prints exactly as the plain `Vec<CommittedEvent>` over histories
  holding every kind.
- `regret_census_v1::stage4a::history_guard_tests::one_event_of_each_kind_changes_no_guarded_result`
  adds one event of each kind to base sessions and checks that every guarded
  history scan gives the same result as with every guard open.

## Where the samples came from

216 are the first event of their kind that a test process committed, collected
by `event_history_v1::sample_harvest_v1` and taken from the first run, in this
order, that had one: `limited-fdn-fixtures` library tests (187), its integration
tests (12), the rest of its library tests (10) and `standard-magezero-fixtures`
integration tests (7).

None of those runs committed the other 18, so they were built by hand from
their fields, reusing a harvested sibling's payload where the types match:
`BaitTransferAppliedV48`, `BaitTransferInputV48`, `BanishingRecipientCeasedV1`,
`ConquerorCapturedV49`, `ConquerorCollectedV49`, `ConquerorJournalV49`,
`OptionalAdditionalCostPaid`, `PlusCounterInputV48`, `PlusCounterReplacementV48`,
`PrivateKeepOneRaidSkippedV1`, `RaidEntryDroppedV1`, `RoomDoorUnlockedV1`,
`StromkirkPlacementRemovedV2`, `StrongboxRaidSkippedV1`,
`TokenAttachmentJournalV48`, `TokenCreationInputV48`,
`TokenCreationReplacementV48` and `WildwoodTriggerCounteredV1`.

The three appended samples for `DrakeJournalV1`, `DrakeHistoryRefV1` and
`ConquerorGainReplayBorrowV49` are also hand-built from their declared fields.
They exercise kind indexing and value serialization, not valid gameplay or
custody. The original 234 samples retain their order and payloads.

## Adding a kind

A new `CommittedEvent` variant fails
`the_samples_hold_one_event_of_every_kind_in_order` until its sample is in the
file. Take one from a test that commits an event of the new kind:

```text
MTG_EVENT_SAMPLES_V1=/tmp/samples cargo test --release -p mtg-kernel --lib [--features ...] -- FILTER
```

Library tests always carry the hook; integration tests need
`--features event-sample-harvest-v1`. The first event of each kind a test
process commits is written to `/tmp/samples/<Kind>.json`, and existing files
are never overwritten. Leave out the tests that read this file
(`event_history_v1` and `history_guard_tests`): they commit its events, so a
harvest that runs them copies the file back, hand-built events included.
Insert that line where the variant sits in `CommittedEvent`. When no test
commits the kind, build the event by hand from its fields, reusing a harvested
sibling's payload where the types match.

## Unqualified future source bridge

The future native source classifies 310 event kinds. The 237 current samples
above remain byte-for-byte unchanged. The following 73 future kinds do not yet
have sample rows. The existing complete-kind assertions are retained and will
remain unqualified until genuine or honestly hand-built typed samples are
appended in this order. The initial future export builds and lists its binary;
it does not execute these or any other native tests.

This is an explicit remaining runtime prerequisite, not a passed check or a
consumer/Full qualification. Internal kind indices use u16; events, event
history serialization/Hash and all earlier kind indices retain their shape.

```text
AleshaPowerLkiV1
NivMizzetVisionaryDamageV1
NivMizzetVisionaryTriggerStackedV1
NivMizzetVisionaryDamageSourceDepartureV1
KinshipEntryV1
SphinxsTutelageDrawV1
SphinxsTutelageCollectedV1
SphinxsTutelagePublishedV1
SphinxsTutelageMillStartedV1
SphinxsTutelageMillBatchV1
LathrilCombatDamageV1
LathrilCombatCollectedV1
LathrilCombatPublishedV1
LathrilCombatResolvedV1
LathrilCombatCounteredV1
LathrilCombatObservedV1
CopiedTokenAbilitiesEnteredV1
CopiedTokenEndStepV1
CopiedTokenEndStepCollectedV1
CopiedTokenSacrificePublishedV1
CopiedTokenSacrificeFinishedV1
CuratorEnteredV1
CuratorCollectedV1
CuratorPublishedV1
CuratorFinishedV1
CuratorProgressV1
KykarExiledV1
KykarDelayedCollectedV1
GreatwurmCounterPaymentV1
GreatwurmCombatCaptureV1
GreatwurmCombatBoundV1
GreatwurmCollectedV1
GreatwurmPublishedV1
GreatwurmFinishedV1
GreatwurmCounterInputV1
SourceCopyDepartureV1
SphinxLoreV1
SphinxLoreCastOptionsV1
SphinxLoreCastSelectedV1
OmniscienceCastOptionsV1
OmniscienceCastSelectedV1
NightmareCopyEnteredV1
NightmareCopyLayerV1
HarvesterGraveyardEntryV1
HarvesterResolvedV1
HarvesterCastAnnouncedV1
HarvesterCastReversedV1
ProgenitusInputV1
ProgenitusShuffledV1
LeylineOpeningV1
FiendArtisanInputV1
FiendArtisanPaidV1
FiendArtisanSearchStartedV1
FiendArtisanSearchFinishedV1
LifeLossWideV59
LifeGainWideV59
ConquerorObserverInputV59
ConquerorLossBatchV59
ConquerorCapturedV59
ConquerorCollectedV59
ConquerorJournalV59
FiendishPandaWideGainCaptureV59
ThousandYearStormV1
TimeStopJournalV1
OrdinaryCleanupStageV1
GenesisWaveProgressV1
GenesisWaveFinishedV1
PublishedCastIdentityV1
VivienReidV1
MuldrothaOptionsV1
MuldrothaSelectedV1
MuldrothaPlayedV1
LilianaDreadhordeV1
```
