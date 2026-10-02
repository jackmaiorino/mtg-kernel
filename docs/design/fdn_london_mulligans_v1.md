# London mulligans for the original FDN fixtures

Implement a separate Limited schema 4 mode combining engine priority,
Foundations combat and London mulligans. Keep existing schemas 1 through 3
and default Pauper resets unchanged. Mulligans are gameplay implementation;
no training or strategy-quality experiment is included.

Store the pregame state in the game snapshot: starting player, per-player
mulligan counts, kept status, current round/actor and exact current hand-card
bindings. Each selected bottom card is moved immediately, preserving its order
and new incarnation in the library suffix. Append engine decisions/actions for keep,
mulligan and one bottom-card selection at a time. Project only the acting
player's hand, plus public counts and kept status. Include these choices in
the normal decision/action binding and stale-action rejection contract.

Both players begin with seven cards. In starting-player order, each player who
has not kept chooses whether to keep or mulligan. Complete both announcements
before reshuffling/redrawing for that round. Each mulligan returns the whole
hand to its library, shuffles with the existing serialized deterministic RNG,
then draws seven and immediately chooses cards equal to their new mulligan
count to put on the bottom, in a declared selection order. Complete every
mulligan's bottoming before the next round of keep/mulligan announcements.
Keeping accepts the remaining hand without bottoming again. At seven mulligans
keeping is forced after bottoming all seven cards. Kept players skip later
rounds. Ordinary turn progression begins only after both have kept.
Use existing zone-change, shuffle and private-knowledge bookkeeping.

This follows Comprehensive Rules 103.5 and XMage's
`Mage/src/main/java/mage/game/mulligan/LondonMulligan.java`. The two-player
fixtures have no free first mulligan. Rules source:
https://media.wizards.com/2026/downloads/MagicCompRules%2020260925.txt

Existing XMage reference scenarios in
`Mage.Tests/src/test/java/org/mage/test/mulligan/LondonMulliganTest.java`:
`testLondonMulligan_NoMulligan` keeps7/33, `testLondonMulligan_OneMulligan`
checks7/33 then6/34 before keeping, and `testLondonMulligan_TwoMulligan`
checks6/34 before the second redraw and5/35 before the final keep. These map
to the no-mulligan, simultaneous-redraw and repeated-mulligan Rust cases.
All7 cases in that existing reference class passed in hosted Mage run37044173670
atd9536815d58, with zero failures, errors or skips. Source Git-blob hashes,
downloaded output hashes and XML counts were verified.

Prove these behaviors with named tests:

| Test | Requirement |
| --- | --- |
| keep_both_preserves_opening_hands_and_first_turn_draw_skip | Seven each; no redraw/shuffle when both keep; P0/P1 first-turn draw skip |
| announcements_complete_before_redraw_and_both_redraw_before_bottoming | Complete announcements, then redraw both hands, then bottom to six |
| repeated_mulligans_bottom_exact_count_before_next_announcement | Each round bottoms its current count; kept opponent skips subsequent rounds |
| zero_card_keep_is_forced_after_seven_mulligans | Bounded seven-mulligan limit, correct zero-card hand |
| bottom_selection_order_is_exact_and_private | Chosen order matches the library suffix; only owner knows identities |
| invalid_bottom_choice_is_atomic_for_foreign_duplicate_and_out_of_zone_cards | Reject invalid picks without mutation |
| stale_hand_incarnation_is_rejected_before_mutation | Exact hand bindings reject moved-and-returned cards |
| restore_each_pregame_phase_reproduces_choices_rng_and_zone_incarnations | Snapshot and JSON restore reproduce next choice and state |
| schema_four_exposes_private_hand_and_bound_mulligan_bottom_actions | Public counts, own hand, schema identity, retries and stale-action refusal |
| schemas_one_to_three_keep_historical_opening_deal_and_refuse_schema_four | Existing resets stay compatible and refuse the new mode |
| london_session_snapshot_restores_pending_bottom_menu_binding_and_transition | Pending session restore preserves menu, binding, response and hash |
| original_fixtures_play_with_london_in_both_seats_and_replay_exactly | Both unchanged decks, both seats, natural terminals and exact replay |

Add the external mode to `kernel_limited_env`, the Limited server constructor,
Python session client and their contract tests. Document the schema and action
semantics. Run fixed seeds with actual keep/mulligan/bottom decisions, repeat one
seed to prove deterministic replay, and restore a pending bottoming decision.
Record source/toolchain/seeds/input-output hashes in the existing small manifest.
