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

## Appended FDN event samples (October 21, 2026)

The original 237 rows retain their exact byte prefix. The next 76 rows follow `EventKindV1` order and were harvested from native tests that completed successfully. These are serialization fixtures, not complete replay worlds or gameplay/profile acceptance. Labelled native setup and counter primitives retain that limited provenance. Snapshot-bearing events retain the actual emitted payload, including private synthetic fixture state.

Original failed and timed-out attempts remain in the cold evidence records. The source-of-record receipt is `E:/storage-records/fdn110/fdn110-all76-event-samples-applied-receipt.json`, SHA-256 `2fc3eaf7be0e93bc6fdff0bfb40cc120da65229a7bbf44e1a915a5a9c0a8b610`. It binds original payload hashes, producer results and retained sources.

| Event kind | Producer source | Native producer | Original payload SHA-256 |
| --- | --- | --- | --- |
| `AleshaPowerLkiV1` | `b6a32a534c5e` | `alesha_v1::tests::paid_raid_uses_original_source_lki_after_its_own_death_return_both_seats` | `b8543880161bcbb19af11e60065c676b3cc42905c9a1f11044f0afb70aa91f8d` |
| `NivMizzetVisionaryDamageV1` | `5e52982e89b0` | `niv_mizzet_visionary_v1::tests::paid_archer_damage_after_removal_uses_departure_controller_both_seats` | `4d4bf871b22d2e1276f98067569da6bbcf2e430ca1f4a2f10cf24ca22cd8319f` |
| `NivMizzetVisionaryTriggerStackedV1` | `5e52982e89b0` | `niv_mizzet_visionary_v1::tests::paid_archer_damage_after_removal_uses_departure_controller_both_seats` | `d30be34be7b57e30e63a591fd6676ba56d1152bd8016c74bab660d7906be9b5b` |
| `NivMizzetVisionaryDamageSourceDepartureV1` | `1dee95151f6b` | `snack_wide_life_v59::tests::paid_snack_commits_one_exact_wide_loss_and_conqueror_panda_gain_with_restore_both_seats` | `4ace334ed8428d1163eaa242a00d6ff5d80643d8061558b37972d678a407e50f` |
| `KinshipEntryV1` | `1dee95151f6b` | `banner_of_kinship_v1::tests::printed::paid_banner_complete_domain_original_payment_checkpoint_restore_and_zero_counts_both_seats` | `398c04dceac6f86f841be339167354a5b103bd502f8cc12c4489013ad365ebe0` |
| `SphinxsTutelageDrawV1` | `1dee95151f6b` | `effect::sphinxs_tutelage_v1_tests::paid_repeat_original_pair_order_restore_and_readonly_forgery_both_seats` | `87141ceafe14a69f44ceb907cbc43c3775367b498c66182e04716693e7a5e497` |
| `SphinxsTutelageCollectedV1` | `1dee95151f6b` | `effect::sphinxs_tutelage_v1_tests::paid_repeat_original_pair_order_restore_and_readonly_forgery_both_seats` | `0b49f8cf62ca23a023d15621db37747c55749a749884b38bd7af0865db37131c` |
| `SphinxsTutelagePublishedV1` | `1dee95151f6b` | `effect::sphinxs_tutelage_v1_tests::paid_repeat_original_pair_order_restore_and_readonly_forgery_both_seats` | `db29724e609efa20620ae73a49c7a441d326a09e3b367a65994e3ebebba498bc` |
| `SphinxsTutelageMillStartedV1` | `1dee95151f6b` | `effect::sphinxs_tutelage_v1_tests::paid_repeat_original_pair_order_restore_and_readonly_forgery_both_seats` | `c6a4ff788bf46c55ff500d35a7961704beaaa8d231cbdb4f59c74d84bf340b76` |
| `SphinxsTutelageMillBatchV1` | `1dee95151f6b` | `effect::sphinxs_tutelage_v1_tests::paid_repeat_original_pair_order_restore_and_readonly_forgery_both_seats` | `7e1b449cab21972caee2d35077eab444337ecdf36919a931ed575c43d5122b7a` |
| `LathrilCombatDamageV1` | `1dee95151f6b` | `effect::lathril_v1_tests::paid_combat_capture_publication_restore_departure_and_single_consumption_both_seats` | `7015576f248b537b084d257f3bed77612f9e75d5a9026298a3f8738cbce0e2c1` |
| `LathrilCombatCollectedV1` | `1dee95151f6b` | `effect::lathril_v1_tests::paid_combat_capture_publication_restore_departure_and_single_consumption_both_seats` | `79d8a07369d73b2a655ceeba3410b624cfe7319b8075186baf00d39eb0e80c79` |
| `LathrilCombatPublishedV1` | `1dee95151f6b` | `effect::lathril_v1_tests::paid_combat_capture_publication_restore_departure_and_single_consumption_both_seats` | `fec0de47b5c826e0d8dd988eb493934c9c48c4d806a147ab69c1e5fccf969d39` |
| `LathrilCombatResolvedV1` | `1dee95151f6b` | `effect::lathril_v1_tests::paid_combat_capture_publication_restore_departure_and_single_consumption_both_seats` | `ac3a554a09ba54174655208ed541ee2f7f29f4404f5af8812dcd3337514532d5` |
| `LathrilCombatCounteredV1` | `5e52982e89b0` | `effect::lathril_v1_tests::paid_capture_direct_api_and_readonly_history_row_forgeries_both_seats` | `ec41c90ecc0f677cfb28cd58b38bd816f181dc516d84f3822652c39d507ca082` |
| `LathrilCombatObservedV1` | `1dee95151f6b` | `effect::lathril_v1_tests::paid_combat_capture_publication_restore_departure_and_single_consumption_both_seats` | `368a3b5e8607fe12534af7e72c7b6570635552f72fdc94bd80987686f5a29d2a` |
| `CopiedTokenAbilitiesEnteredV1` | `1dee95151f6b` | `effect::electroduplicate_v1_tests::genuine_paid_copy_of_copy_and_self_reflection_inherit_instances_and_countered_recurrence_both_seats` | `fbed8e9492e25fb810c48d585bb4fed72f96bf24f229ea59ba1734f611686425` |
| `CopiedTokenEndStepV1` | `1dee95151f6b` | `effect::electroduplicate_v1_tests::genuine_paid_copy_of_copy_and_self_reflection_inherit_instances_and_countered_recurrence_both_seats` | `37253ebde49d1e08cf8edea6bd6f4586002fc946d96b6be03777cda9fcf0c6c0` |
| `CopiedTokenEndStepCollectedV1` | `1dee95151f6b` | `effect::electroduplicate_v1_tests::genuine_paid_copy_of_copy_and_self_reflection_inherit_instances_and_countered_recurrence_both_seats` | `0cccd3f0d50bd7fd28ef73ca3fd323a69334c1b7a2c538479d3fd7ca552ef4a5` |
| `CopiedTokenSacrificePublishedV1` | `1dee95151f6b` | `effect::electroduplicate_v1_tests::genuine_paid_copy_of_copy_and_self_reflection_inherit_instances_and_countered_recurrence_both_seats` | `cb5b9b52fa806a74795600c9ef544e3f6a2995f505b6a96f0320c148c6036d7c` |
| `CopiedTokenSacrificeFinishedV1` | `1dee95151f6b` | `effect::electroduplicate_v1_tests::genuine_paid_copy_of_copy_and_self_reflection_inherit_instances_and_countered_recurrence_both_seats` | `aba2c178f693cdaec23ed18c90b71adc3c59609e451cabc297e50dad631df3f5` |
| `CuratorEnteredV1` | `b6a32a534c5e` | `effect::curator_v1::tests::genuine_paid_complete_printing_uncounterable_and_zero_to_five_piles_restore_both_seats` | `c93c7d60607d34e81de9840da6fb25ff999cfb9f00a01ae9988a1f66175d5d8c` |
| `CuratorCollectedV1` | `b6a32a534c5e` | `effect::curator_v1::tests::genuine_paid_complete_printing_uncounterable_and_zero_to_five_piles_restore_both_seats` | `e7b4cf2c3177db2b23245a2215d387411ec1ffa90ab7aa4a080c1a0073569e8a` |
| `CuratorPublishedV1` | `b6a32a534c5e` | `effect::curator_v1::tests::genuine_paid_complete_printing_uncounterable_and_zero_to_five_piles_restore_both_seats` | `87dfcfd28e1876a49fea2537bad93c520b3cf5a3a72cc82e768220be2c7800d8` |
| `CuratorFinishedV1` | `b6a32a534c5e` | `effect::curator_v1::tests::genuine_paid_complete_printing_uncounterable_and_zero_to_five_piles_restore_both_seats` | `0928ff8d89d968d65c1c482f948b2cc06cdb7040662ad3dc07fc350fcdfd6b9a` |
| `CuratorProgressV1` | `b6a32a534c5e` | `effect::curator_v1::tests::genuine_paid_complete_printing_uncounterable_and_zero_to_five_piles_restore_both_seats` | `02558af4988d0497c0cf9099ee69bbe2aea4ffff810283fa35343c8d9134f3bc` |
| `KykarExiledV1` | `b6a32a534c5e` | `effect::kykar_v1::tests::paid_exile_response_source_departure_owner_return_and_delayed_restore_both_seats` | `1090c12432339c55dd35bec99a4514728c8dda655b8035a81cc58a6691f3272b` |
| `KykarDelayedCollectedV1` | `b6a32a534c5e` | `effect::kykar_v1::tests::paid_exile_response_source_departure_owner_return_and_delayed_restore_both_seats` | `c1011e717afbf11a69fe3f7e6d73638d56b22245a20d6ab7b3eed40dbca1794c` |
| `GreatwurmCounterPaymentV1` | `b3b1e28b69b0` | `effect::greatwurm_v1::tests::genuine_paid_cast_accepts_every_native_counter_kind_without_narrowing_both_seats` | `a6d9dc2e0816a9c22f2d08f00fc050872e5f789e06597280250fee1fec799df2` |
| `GreatwurmCombatCaptureV1` | `b6a32a534c5e` | `effect::greatwurm_v1::tests::genuine_paid_printing_real_declared_trample_combat_counts_one_entire_wave_both_seats` | `9c36165610d4339c2fe44076836d10be828a9cdda952b57a1c405ff9e8be8358` |
| `GreatwurmCombatBoundV1` | `b6a32a534c5e` | `effect::greatwurm_v1::tests::genuine_paid_printing_real_declared_trample_combat_counts_one_entire_wave_both_seats` | `837b96f21bac0766007f68cca8f48a73982cff4683cd737ab725a370dae862fc` |
| `GreatwurmCollectedV1` | `b6a32a534c5e` | `effect::greatwurm_v1::tests::genuine_paid_printing_real_declared_trample_combat_counts_one_entire_wave_both_seats` | `4484c07c9f4eaf07b5a940c83706f98f3bb738c56facdea724ff3792fd624109` |
| `GreatwurmPublishedV1` | `b6a32a534c5e` | `effect::greatwurm_v1::tests::genuine_paid_printing_real_declared_trample_combat_counts_one_entire_wave_both_seats` | `92f1bf02a6e772a4433389bb596f10337be2997ff9d63cbf7d8fdbc36d44dd36` |
| `GreatwurmFinishedV1` | `b6a32a534c5e` | `effect::greatwurm_v1::tests::genuine_paid_printing_real_declared_trample_combat_counts_one_entire_wave_both_seats` | `8a30dfc6e027c69b6b3aee881b19ae99dbec21db7a242b830df748b25a5cd359` |
| `GreatwurmCounterInputV1` | `b3b1e28b69b0` | `effect::greatwurm_v1::tests::genuine_paid_cast_accepts_every_native_counter_kind_without_narrowing_both_seats` | `9c8fb92c8aa525781fb467fc8f13be9f4aa97174601e42673b7580afc59ea36a` |
| `SourceCopyDepartureV1` | `b6a32a534c5e` | `effect::electroduplicate_v1_tests::genuine_paid_horde_departed_copy_uses_original_lki_and_refuses_corruption_both_seats` | `54b2ee685c29c852003662c801e985be3d8157eddd4130c6e096490e2e3b9998` |
| `SphinxLoreV1` | `b6a32a534c5e` | `effect::sphinx_lore_v1::tests::actual_deep_analysis_printed_and_granted_costs_remain_distinct_with_restore_both_seats` | `ebac26218b6a4f66e3ff7f97c22c052ebfd0d4b5dac96ec5d3e02bcf6814c379` |
| `SphinxLoreCastOptionsV1` | `b6a32a534c5e` | `effect::sphinx_lore_v1::tests::actual_deep_analysis_printed_and_granted_costs_remain_distinct_with_restore_both_seats` | `a4699f5239eacb321afafe67f9f65980a7149540412caf3a4fe88356cd8d2b9b` |
| `SphinxLoreCastSelectedV1` | `b6a32a534c5e` | `effect::sphinx_lore_v1::tests::actual_deep_analysis_printed_and_granted_costs_remain_distinct_with_restore_both_seats` | `e1054ac5fa0d49a32d16efc306f308c1180d9ee4291f6a12b37424ef24127c61` |
| `OmniscienceCastOptionsV1` | `1dee95151f6b` | `engine::omniscience_cast_v1::tests::optional_paid_and_free_bolt_keep_actual_source_target_cost_and_response_restore_both_seats` | `f213f9be5018bf14f46cc61af4b441e77fc95a701bb7a9e4317e8afca7cdb7d4` |
| `OmniscienceCastSelectedV1` | `1dee95151f6b` | `engine::omniscience_cast_v1::tests::optional_paid_and_free_bolt_keep_actual_source_target_cost_and_response_restore_both_seats` | `9f4c40099159f66dd61fba6ce1e658eaa4806cb1c483c8951404136f2af31ca3` |
| `NightmareCopyEnteredV1` | `b3b1e28b69b0` | `effect::electroduplicate_v1_tests::genuine_paid_nightmare_native_aegis_layer_save_restore_and_paid_copy_values_both_seats` | `6eb5d5388e2e769cab74e7df1e6de558659980481b0d582a6d3a1f4c8c863ecd` |
| `NightmareCopyLayerV1` | `b3b1e28b69b0` | `effect::electroduplicate_v1_tests::genuine_paid_nightmare_native_aegis_layer_save_restore_and_paid_copy_values_both_seats` | `d02c0be49cda845f5267114369605572c2f758e217aecf1e05b93b9ec9523e13` |
| `HarvesterGraveyardEntryV1` | `1dee95151f6b` | `engine::harvester_cast_reversal_v1_tests::genuine_paid_harvester_native_cast_abort_rebinds_original_hand_and_graveyard_with_restore_both_seats` | `f927870ac04b81f94411d430fa7468e63cb7e800570b2ba3dcd537da545aba7f` |
| `HarvesterResolvedV1` | `b3b1e28b69b0` | `effect::electroduplicate_v1_tests::genuine_paid_nightmare_native_aegis_layer_save_restore_and_paid_copy_values_both_seats` | `1356cc8c03cb7d359b9a90e5d0f0c114258371dfc157a53f034c0f9134d247be` |
| `HarvesterCastAnnouncedV1` | `1dee95151f6b` | `snack_wide_life_v59::tests::paid_snack_commits_one_exact_wide_loss_and_conqueror_panda_gain_with_restore_both_seats` | `2dc48274673db7d269de2ca99aff6b580689a6a0570dc57063fd3bd89ece3f75` |
| `HarvesterCastReversedV1` | `1dee95151f6b` | `engine::harvester_cast_reversal_v1_tests::genuine_paid_harvester_native_cast_abort_rebinds_original_hand_and_graveyard_with_restore_both_seats` | `4925454f8d44c820fdb55248de7e428082553ab3fa56bdf1457411e776f76715` |
| `ProgenitusInputV1` | `1dee95151f6b` | `engine::progenitus_v1_tests::exact_ten_colored_paid_spell_counter_response_and_restore_both_seats` | `776e9d986f0bd4c3dacf9c2cc35deb809980ee35eaa0cd567864098c6373b76d` |
| `ProgenitusShuffledV1` | `1dee95151f6b` | `engine::progenitus_v1_tests::exact_ten_colored_paid_spell_counter_response_and_restore_both_seats` | `8cadfbe871d0e0f05410ae559e2a31646a582044f6a0da49db72fa850c5d8810` |
| `LeylineOpeningV1` | `1dee95151f6b` | `leyline_axe_v1::tests::real_opening_draws_optional_each_copy_starting_seat_order_and_restored_answers_both_seats` | `d126b3eda86d7a618f2858a1de6fbe651141940e8c8241c6cbce1f85fb03659e` |
| `FiendArtisanInputV1` | `b6a32a534c5e` | `engine::fiend_artisan_v1_tests::paid_hybrid_printing_sorcery_mana_x_tap_sacrifice_search_and_restore_both_seats` | `2150f9ffbb81c6173205d6d26abe817c33e1c304ccec556032a83bd39f394d52` |
| `FiendArtisanPaidV1` | `b6a32a534c5e` | `engine::fiend_artisan_v1_tests::paid_hybrid_printing_sorcery_mana_x_tap_sacrifice_search_and_restore_both_seats` | `b589a42870677fec654d6d90d50533d5dfa5259e43aebb2e3be5dffdfecab9c8` |
| `FiendArtisanSearchStartedV1` | `b6a32a534c5e` | `engine::fiend_artisan_v1_tests::paid_hybrid_printing_sorcery_mana_x_tap_sacrifice_search_and_restore_both_seats` | `0acb9807e03aa82afd0faba092d1a20cf9490533a31a95ab7fe45b7c48024981` |
| `FiendArtisanSearchFinishedV1` | `b6a32a534c5e` | `engine::fiend_artisan_v1_tests::paid_hybrid_printing_sorcery_mana_x_tap_sacrifice_search_and_restore_both_seats` | `74d20985b74a53380c888cbebe97b7014896cc45636f310c31baeba10331badd` |
| `LifeLossWideV59` | `1dee95151f6b` | `snack_wide_life_v59::tests::paid_snack_commits_one_exact_wide_loss_and_conqueror_panda_gain_with_restore_both_seats` | `51435e7917ee7303a800753634b1e443f3097ef496d0c822203c9a09551b329a` |
| `LifeGainWideV59` | `1dee95151f6b` | `snack_wide_life_v59::tests::paid_snack_commits_one_exact_wide_loss_and_conqueror_panda_gain_with_restore_both_seats` | `eee1786cbef44d6608d88dab3548816979a2f684c85662001ef40c210230c020` |
| `ConquerorObserverInputV59` | `1dee95151f6b` | `snack_wide_life_v59::tests::paid_snack_commits_one_exact_wide_loss_and_conqueror_panda_gain_with_restore_both_seats` | `3d6609c4244ac3bbfecc165ded42fdb409a1c5453f8674cbad616fe7bd9ba211` |
| `ConquerorLossBatchV59` | `1dee95151f6b` | `snack_wide_life_v59::tests::paid_snack_commits_one_exact_wide_loss_and_conqueror_panda_gain_with_restore_both_seats` | `b19749430355e9493e88acf1eaf0ec2a851401409903d7cc834d8f738d5f16b2` |
| `ConquerorCapturedV59` | `1dee95151f6b` | `snack_wide_life_v59::tests::paid_snack_commits_one_exact_wide_loss_and_conqueror_panda_gain_with_restore_both_seats` | `24e462adb144f55e2bdd2bb96660944ca9a3fdc8d7a70087de5d781370dc47b6` |
| `ConquerorCollectedV59` | `1dee95151f6b` | `snack_wide_life_v59::tests::paid_snack_commits_one_exact_wide_loss_and_conqueror_panda_gain_with_restore_both_seats` | `d5c4cfcf92e9591af88d41aef48cae0aacc61cdb166fbb33e9818faaefa35a67` |
| `ConquerorJournalV59` | `1dee95151f6b` | `snack_wide_life_v59::tests::paid_snack_commits_one_exact_wide_loss_and_conqueror_panda_gain_with_restore_both_seats` | `7a168c63658fed7dd7b8c4f89107e8df45123a62778272e438835d6ebc497686` |
| `FiendishPandaWideGainCaptureV59` | `1dee95151f6b` | `snack_wide_life_v59::tests::paid_snack_commits_one_exact_wide_loss_and_conqueror_panda_gain_with_restore_both_seats` | `6d985ca99aa17af6c90b3aea377d34c8c5313256e3539a26aa08aa3fbe23eb41` |
| `ThousandYearStormV1` | `1dee95151f6b` | `effect::thousand_year_storm_v1_tests::paid_countered_original_and_destroyed_observer_keep_frozen_copies_and_controller_both_seats` | `49567ca7fb39622af76de98466da1706077cc2e149b320ada195c8609c6e4298` |
| `TimeStopJournalV1` | `b6a32a534c5e` | `engine::time_stop_v1::tests::paid_cleanup_discard_precedes_damage_and_duration_expiration_both_seats` | `8b907bae60b96cd74d1ad73781b23cf572ebda1abf8fddfb4f55099a3a1f2bc2` |
| `OrdinaryCleanupStageV1` | `b3b1e28b69b0` | `effect::electroduplicate_v1_tests::genuine_paid_nightmare_native_aegis_layer_save_restore_and_paid_copy_values_both_seats` | `94896070c676eae74409e5eb1e3e8a7ac1b4b9ad49231dfb25c5b66a826f93d1` |
| `GenesisWaveProgressV1` | `b6a32a534c5e` | `effect::genesis_wave_v1::tests::genuine_paid_wave_subset_and_remaining_public_graveyard_order_restore_both_seats` | `139f4c6e40fc1a0decfaf2e310005270ad4d0a1c9ce69ef463449472f6527767` |
| `GenesisWaveFinishedV1` | `b6a32a534c5e` | `effect::genesis_wave_v1::tests::genuine_paid_wave_subset_and_remaining_public_graveyard_order_restore_both_seats` | `3bff5628bab8375419d8b64c383db264a1aaa2b2a2e9340fed9fd0d2c510740a` |
| `PublishedCastIdentityV1` | `b3b1e28b69b0` | `goblin_negotiation_v1::tests::actual_numeric_prevention_and_doubling_orders_have_different_excess_both_seats_restore_every_choice` | `4a3f779139c94348624b91da71f6d3b876140e0becf40ca1895381ff8be79931` |
| `VivienReidV1` | `b6a32a534c5e` | `current_mixed_policy_v1::vivien::tests::genuine_paid_vivien_numeric_full_packet_index_parity_restore_and_foreign_refusal_both_seats` | `495f025e222610231f550d03ce6514373fcfcb5de93f44a797ca12a40c206043` |
| `MuldrothaOptionsV1` | `b6a32a534c5e` | `current_mixed_policy_v1::muldrotha::tests::paid_numeric_permission_cost_native_parity_fresh_packet_and_snapshot_both_seats` | `fdba343ffa844f7d6975b305af0f115330a773fb0571eb30025ef8bdcf7d8eb5` |
| `MuldrothaSelectedV1` | `b6a32a534c5e` | `current_mixed_policy_v1::muldrotha::tests::paid_numeric_permission_cost_native_parity_fresh_packet_and_snapshot_both_seats` | `1c2935cc58d7881ccaa2a0a456650aaa865d4fd2fc11d29f3ba7632df54086b7` |
| `MuldrothaPlayedV1` | `b6a32a534c5e` | `current_mixed_policy_v1::muldrotha::tests::paid_numeric_permission_cost_native_parity_fresh_packet_and_snapshot_both_seats` | `4dacf6209d030791eb3e3dfaa2d054c832ba51613e277aed6c9c131913c9a90a` |
| `TinybonesV72` | `b6a32a534c5e` | `current_mixed_policy_v1::tinybones::tests::genuine_paid_numeric_activation_discard_stash_payment_snapshot_and_native_parity_both_seats` | `f9b21644477b0cfe972ffa1a4a0f00baf9b4805015ea848d266c0261137d3d2a` |
| `LilianaDreadhordeV1` | `5e52982e89b0` | `effect::liliana_dreadhorde_v1::tests::paid_zombie_exact_token_season_cost_and_original_controller_after_departure_both_seats` | `fd6f78af0e89d3e9e31165a119d7f9abed101288f654882b73d72bad7821c719` |
| `ChandraFlameshaperV74` | `65e23248d2c2` | `rl::complete_copy_prefix_scope_tests::paid_complete_walkers_keep_private_mode_through_shared_prefix_and_legacy_refusal_both_seats` | `f252a9121c17f26cb619d7470859cb259abaee793c72678e3ecfc11d999edd8a` |
| `KaitoCunningV75` | `5e52982e89b0` | `effect::kaito_cunning_v75::tests::genuine_paid_zero_or_one_target_draw_discard_and_waiting_restore_both_seats` | `bdc39fa270e814b332e23a18b8b1a67dbb3132fdab8e6a72134bcaed6c50ff07` |

## Appended Drakuseth event sample (October 26, 2026)

Row 314, `DrakusethDamageSourceV76`, is the actual first event emitted by
`standard_cards_v1::drakuseth_tests::paid_attack_mandatory_first_zero_one_two_other_targets_and_every_target_checkpoint_restore_both_seats`
at source `088cc890e94987c42ad5159d4e809af98f4af530`. That complete native case
passed in 0.063444293 seconds in [CI 40272003668, job 121410706264](https://github.com/jackmaiorino/mtg-kernel/actions/runs/40272003668/job/121410706264);
the native job passed all 457 cases. Only that named producer had the harvest
environment enabled. The original 313 event payloads retain their bytes and order.

The original 798-byte payload has SHA-256
`a0ffe9181e70ebd433ef56c99ff3e39e856efcbed965d2dd0a1e1944893d7a7e`.
It came from artifact `12630466327`, whose original 591-byte ZIP has SHA-256
`35f6f1706dd0b9f6a5920ac10657c35a3a9c5897856e5209c380a05f41d8a056`.
Original ZIP, payload, producer log and receipts are byte-verified on D: and E:
under `fdn110-ward-20261025/ci40272003668`; the E: root is
`E:/fdn110-ward-20261025/ci40272003668`. The receipt
`every-kind-314-append-receipt.json` records the preserved prefix and complete
kind order. This standalone event fragment tests serialization; it does not
make the artificial mixed fixture a valid native replay or qualify a gameplay
profile or Drakuseth consumers.
