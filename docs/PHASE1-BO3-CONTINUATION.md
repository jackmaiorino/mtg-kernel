# Explicit BO3 gameplay continuation V1

`phase1_bo3_learning_v1::update_bo3_gameplay_v1` performs one explicitly requested CPU gameplay operation. It consumes the actual private groups produced by BO3 preparation and calls the separate weighted primitive once for an eligible batch. It adds no training loop, CLI update command, GPU path, registry expansion or auxiliary-policy learning. Legacy checkpoint DTOs, old loss arithmetic, and ordinary update dispatch remain unchanged.

The scope is the existing registry, ordinary-checkpoint transition or this BO3 successor, KeepSevenV2 opening, fixed Play/Draw, Keep sideboarding at both seats, and disabled search. Existing V1 captures lacking original native witnesses remain ineligible. No CP7 outcomes enter the operation.

## Caller interface and exact state

`Bo3GameplayUpdateRequestV1` uses schema `mtg-kernel-bo3-gameplay-update-request/v1` and these fields:

| Field | Contract |
| --- | --- |
| `input` | Tagged `ordinary_checkpoint_transition` or `bo3_checkpoint`, containing the exact `learner` behavior |
| `preparation_request` | SHA-pinned absolute path to the ordered, original attempted-input manifest |
| `learning_rate_bits`, `value_coefficient_bits` | Positive finite binary32 values, bit-identical to the actual parent checkpoint |
| `previous_progress` | Latest immutable progress pin, including after `NoUpdate`; `null` starts a deliberate ordinary-origin chain |
| `output_directory` | Absolute fresh operation directory, or that same operation's directory for recovery; its parent already exists |

The updater loads full named parameters, both Adam moment arrays, Adam step and gauge anchor through a narrow strict ordinary accessor or the new BO3 reader. It never creates fresh Adam from inference weights. The ordinary accessor requires an existing checkpoint and rejects registry-transfer/BO3 descriptors. First transition preserves the ordinary checkpoint's exact scalar bits and learned state. The old game-terminal loss remains the parent's objective until an eligible BO3 update succeeds.

The first successful update publishes a fixed `source.json` descriptor with schema `mtg-kernel-bo3-gameplay-source/v1`, ordinary origin, feature identity, scalar bits, objective `bo3_match_return_weighted_gameplay/v1`, and normalization `equal_eligible_match_groups_f64_div_cast_f32/v1`. Later BO3 updates retain that descriptor. Each `checkpoint.json` uses the distinct `mtg-kernel-bo3-gameplay-checkpoint/v1` schema and pins the exact operation, parent and descriptor. It preserves all state bits, preparation report/weights and attempted progress. `adam_step = ordinary_origin_step + completed_bo3_updates` is checked with bounded native state validation.

Only `load_expanded_inference_v1` dispatches this descriptor. Ordinary initialization/update readers remain strict. The new reader validates the fixed ordinary origin/template, objective, scalars, parent/count metadata and full state before `from_snapshot_v1` and policy installation. It inspects immediate progress metadata without recursively replaying training ancestry. Complete-package wire shapes stay unchanged; the descriptor/checkpoint pins bind the new objective.

## Attempts, weighting and recovery

For `N` eligible matches and `G` learner physical groups in a match, each group retains `(1.0_f64 / N / G) as f32`. The weighted CPU primitive's detached first-value baseline, summed policy substeps, first-only value term, reverse order, gauge and Adam are unchanged. Frozen opponents and fixed auxiliaries never enter learner gradient groups. The scalar/state identity is distinct from the objective identity.

Every validated attempt, including incomplete matches, contributes a compact triple of request SHA, result SHA and semantic physical-match SHA to the immutable progress ledger. Exact artifact reuse and physical duplicates are checked separately. `NoUpdate` appends attempt progress while preserving the exact source, parameters, moments, gauge and optimizer count. It publishes neither a new source descriptor nor a checkpoint. A subsequent ordinary transition after ordinary `NoUpdate` remains valid with that prior progress pin; BO3 `NoUpdate` retains its BO3 descriptor/count.

The caller must pass the latest progress tip and exclusively own the operation directory. There is no global compare-and-swap or campaign-wide fork detector. Opening another ordinary-origin chain is a deliberate independent fork. The exact ledger supports 65,536 attempts, including the planned three-block diagnostic window. Exhaustion rejects; longer campaigns need planned compaction or extension that preserves deduplication, never a silent reset.

Publication uses the existing synced, checked, no-overwrite primitive. `request.json` is committed first. A matching committed `progress.json` returns its original result after source/state checks. A matching committed checkpoint with missing progress is fully restored and completes only the missing progress publication, without preparation or Adam. `optimizer_updated` describes whether that recorded operation updated the optimizer, not whether recovery called Adam. Mismatched existing artifacts reject. A partial orphan `.stage` with no committed final is preserved; use a fresh operation directory with the same parent, prior tip and preserved attempts after inspecting that interruption. No attempted progress advances without its committed receipt.

Requests/source descriptors are bounded to 1 MiB for publication, progress to 32 MiB and checkpoints to 512 MiB. The maximum compact ledger is approximately 13.3 MiB before the bounded envelope. Serialization retains one bounded JSON buffer; typed reads, model copies and native snapshots consume additional memory. These are payload limits, not total RSS guarantees. Existing Windows directory-entry power-loss limitations remain.

## Qualification status

Six regular source tests cover actual native state/moment reload and a freshly rescored numerical next update, corrupt state/layout/gauge rejection, both input kinds' structural incomplete-attempt accounting, full ledger capacity, exact report weights and strict request parsing. Those numerical/accounting fixtures do not claim end-to-end fresh engine recollection.

Two ignored tests require root-supplied real pinned manifests through `MTG_BO3_CONTINUATION_REQUEST`: an update from either explicit input kind against a direct weighted oracle with injected checkpoint-only recovery, and all-incomplete `NoUpdate` preservation/recovery for either input kind. The update test derives expected update, batch and attempt counts from the validated parent/prior progress, so a fresh BO3-source second update receives the same checks. Test-only counters distinguish repeated preparation/Adam from merely equal final states. Root ran all six regular checks successfully in 5.37 test seconds after fixing a missing test-only trait import. Public continuation, fresh next-checkpoint recollection, and real `NoUpdate` qualification remain pending at this handoff.

The preceding clean public capture/preparation check did produce natural two-game BO3 data with real multi-substep physical groups; it established capture/preparation engineering, not this continuation or playing strength. After a successful update, rebuild the exact learner package from readback and recollect fresh matches under its new behavior before another update.

Fresh Fable session `8432de0c-1fa8-4e7a-97bf-1bffdf98cb31` failed the weekly limit on 2026-09-13 before source reads. That is unavailability, not endorsement. Root accepted bounded engineering with independent source review and tests; no campaign unpause, candidate promotion or spending follows from this interface.
