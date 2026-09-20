# Next bounded intervention: broaden card exposure

September 20, 2026. No training has been launched for this intervention. The research goal remains active; this is a bounded local experiment proposal, not a broad campaign or promotion.

## Evidence motivating the change

The completed reciprocal assignment check used 224 new BO3s and 224 preserved anchors with identical deck lists, seeds, starting roles and both physical seats. g115 scored 117/224 G1 wins while piloting the additional lists and 167/224 while piloting the canonical lists; A48 scored 57/224 and 107/224 respectively. Balanced g115 score is 63.39%, paired 95% interval 60.04-66.74%. Balanced additional-list score is 38.84%, interval 33.93-43.75%. All new matches completed naturally, with 506 games and no draws, in 109.57 summed worker-seconds including preflight/replay. Independent raw-file counts agree. g115 retains a relative advantage. The additional-list difficulty is shared, so the earlier cohort gap alone did not diagnose a g115-specific failure.

The read-only embedding audit found 90 of 192 card rows bit-identical to their own initialization with both retained Adam moments positive zero in both models. The qualified Gates-heavy list has 44/60 mainboard copies with such rows; the Spy list has 37/60. Across the eight additional lists the counts are 44, 37, 18, 7, 5, 0, 6 and 7. Control rows for Lightning Bolt, Counterspell and Mountain have changed. Token mapping is card ID plus one, and checkpoint values are verified as raw f32 bit patterns against the initial binary payload. These are exact retained-update facts, not proof of zero historical observation or proof that embeddings cause the losses. Shared network layers and semantic features can generalize; engine errors and strategy differences remain possible.

Evidence roots: E:/mtg-postboard-campaign-20260920/deck-assignment-crossover-001 and E:/mtg-postboard-campaign-20260920/embedding-exposure-audit-001. The former contains the completed analysis, per-list estimates, exact anchor mapping, report and checked figure. All original anchors and models remain unchanged.

## First engineering qualification

Start both branches from untouched g115's complete parameters and Adam state at step 32400. Use the actual qualified GAE producer from source 5239e656, not a trainer lacking GAE. Reuse the existing matched training preparation and runner only after inspecting their current schemas and assertions. Work in the owned E:/mtg-kernel-postboard-codex checkout, use E-drive evidence and local GPU1, BelowNormal priority, and preserve the user's resource reserve.

Prepare four updates of ten natural games per branch. Control uses the original seven registered mainboards. In the broader-exposure branch, replace exactly five learner registrations per update with balanced assignments from all eight qualified published lists, with selected mainboard equal to that registration. Keep the opposing registered list unchanged. Keep seeds, learner seats, initial play/draw roles, opponent policy assignment, terminal rewards, GAE gamma1/lambda0.9, learning rate0.0001, value coefficient0.5 and entropy0 matched. Use fresh episode IDs and seed domain; preserve the initial/current/A48 policy mixture. All eight additional lists, including the one with no unchanged mainboard rows, remain in scope. This is an exposure intervention, not a selection of only losing lists.

Verify exact 60/15 lists, actual native registration binding, every natural terminal, intended schedule differences, all four optimizer updates and unchanged control inputs. Inspect which previously unchanged card rows receive actual gradient/optimizer updates. Require at least one previously unchanged row from the substituted mainboards to change with nonzero optimizer state; do not demand every card in a short sample be drawn or receive a gradient. Check a one-update stop/resume against uninterrupted broader-exposure training, preserving full parameters, both moments and Adam steps; distinguish byte-identical trajectories from metadata-rebound equivalence. Use existing meaningful recovery checks, not tests of scaffolding. Initially cap each small native run at 120 seconds. No win-rate interpretation from this engineering prefix.

## Measurement after qualification

Only after qualification, prepare a separate matched 200-update control versus broader-exposure block from untouched g115, preserving the same intervention and optimizer. Freeze final-endpoint-only analysis, fresh paired evaluation seeds, an integer improvement gate on additional registrations, and a separate canonical-list retention gate before measurement. Include untouched g115 as a reference; neither prior trained endpoint is a matched substitute for the new control. Size and freeze the evaluation after timing the actual workload. No promotion follows from an engineering pass or one development block.

This first slice is explicitly preboard. Exact-registration sideboard plans and learned openings remain necessary for the BO3 goal; do not call Keep-sideboard games a sideboard-policy evaluation. No meta-weighted, held-out opponent or human-strength claim is justified. A48 has distinct fresh initialization but is familiar training opposition. No CP7 outcomes are used.

Fable review remains unavailable following the known zero-read HTTP429 until September 22 07:00 EDT. Jack authorized bounded local continuation and instructed no repeated quota retries. Record that gap and residual uncertainty, without implying endorsement or adding a user-approval step. No paid compute or broad training campaign is authorized here.

## Four-update qualification completed

E:/mtg-postboard-campaign-20260920/broader-exposure-qualification-001 passed the actual native collection/update/restart checks. The exact reused training binary is pinned in manifest.json; it was not rebuilt. All three configurations passed native validation. GPU1 was idle before launch and released afterward. The known Fable review gap remains; no native/build process remains.

| Branch | Natural games | Additional learner-list games | Physical decisions | Native wall seconds |
| --- | ---: | ---: | ---: | ---: |
| Existing-list control | 40 | 0 | 4,913 | 36.95 |
| Broader exposure | 40 | 20 | 5,793 | 38.72 |
| Broader exposure, stopped and resumed | 40 | 20 | 5,793 | 54.56 |

The 20 substituted slots are balanced ten per learner seat, with all eight lists occurring in both seats; four lists occur three times and four twice. The complete 40-game engineering schedule retains its original 24/16 learner-seat split and 19 initial/10 current/11 fixed-A48 opponent assignments. Twelve substituted games start with the learner on the play, eight on the draw. These small-sample imbalances are shared with the matched control and do not constitute a strength measurement. Both branches differ only in the five intended learner registrations per update and output paths. All games are explicitly preboard, with selected decks equal to registered decks.

Full audits checked all 120 trajectories, natural terminal classifications/rewards, source/opponent resolution, optimizer continuation, GAE scalar bits and four actual CUDA updates per branch. The resumed branch stopped after update one and completed in a fresh native process. Its parameters, both Adam moments, scorer anchor and Adam step equal the uninterrupted branch at every update. Ten trajectories are byte-identical; all 40 match after rebinding only checkpoint provenance already proven to identify equal full model/optimizer states. Do not call all 40 raw hashes identical.

The broader branch changed 35 formerly unchanged card embeddings from substituted mainboards, with nonzero retained optimizer moments. One additional row for the Sacred Cat Embalmed Token also changed. The control changed none of the 90 previously unchanged registry rows. The full resumed update list equals the uninterrupted list. Named changes include Basilisk Gate, Journey to Nowhere, Guardian of the Guildpact, Balustrade Spy, Land Grant and Lotleth Giant. This demonstrates actual retained learning on the new card identities, not improved decisions or wins.

Both endpoints finish at Adam32404. Broader full state: 3b7358201bbf1e9fb236287b015e9dac0eb00fabe9dcdae4d339c6e15ad41546. Control full state: 534bc18880a431cb81d058f01ad8321ed553de0baeb868a4be85092fd18f7134. These are engineering endpoints, not promoted candidates or starts for the next measurement. Use untouched g115 for both full branches.

Next prepare the separate bounded 200-update comparison described above, including fresh complete schedules, frozen additional-list improvement and canonical retention gates, untouched-g115 reference, and actual evaluator cost qualification. Do not rerun this successful qualification or interpret its training wins. Cold first-update times are substantial, so four-update wall time is not a reliable linear forecast for 200 updates. Current wall caps passed; no larger run has launched.
