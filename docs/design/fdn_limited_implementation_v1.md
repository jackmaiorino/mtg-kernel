# Issue 110: FDN Limited implementation

Assigned by Jack on 2026-09-30. Implementation branch:
`codex/fdn-limited-deck-loading-v1`, based on public main `d9e0910a`.
This is engineering work; no training, evaluation campaign or playing-strength
claim is authorized by the work plan. Preserve the lead's current PC reservation.

Current fixture milestone: the owned implementation stack through London
mulligans supports all 39 unique names in the two unchanged decks, plus their
seven token definitions. Both decks resolve 40/40. Schema-4 external fixture
games with actual mulligans and pending-choice restore passed on Ubuntu.
The final XMage suite passed all 146 cases in 16 classes, including combat
allocation. Windows integration log audits and complete regression/CI
verification remain pending. See
[the gameplay validation matrix](../reports/fdn_fixture_gameplay_v1_validation.md).
Milestones 5 through 7 remain separate work.

| Milestone | Concrete work | Acceptance |
| --- | --- | --- |
| 1. Inputs and coverage | Import `.dck` files; separate mainboard and sideboard; inspect capabilities; resolve fully supported mainboards; pin two DraftZero fixtures and its reference names. | Reject unknown/partial/no-effect cards and tokens; preserve row/copy order; accept 40+ cards and duplicate counts; deterministic output. Implemented in this PR. |
| 2. Custom-deck game interface | Add a separate opt-in session entry accepting deck contents, using the existing card-array state builder. Bind its deck identity to the registry and contents. Add a minimal external reset/step adapter. | Implemented on `codex/fdn-custom-session-v1`; a supported 40-card smoke reaches a natural terminal and replays byte-identically. Invalid second-seat decks leave the session unchanged. Existing V5/V6 catalog protocols retain their meanings. See `limited_custom_session_v1.md`. |
| 3. Limited rules foundation | London mulligans, full priority windows, current combat damage choices with trample, and planeswalker state/actions/targeting. Add each independently with focused interaction tests. | Compare targeted positions against current Magic rules and XMage; test snapshot/restore while each choice is pending. Merely having keyword bits is insufficient. |
| 4. First FDN games | Inventory the two fixtures by required primitive. Add basic lands and simple creatures/spells first; then their triggers, continuous effects, counters, equipment/auras and unusual cards. Reuse generic engine operations. | Every reachable branch and token dependency implemented; both real fixtures play complete games through milestone 2; targeted XMage comparisons before claiming parity. |
| 5. Complete agreed pool | Freeze the intended FDN draft-card names, basic lands, tokens and applicable Special Guests. Expand in mechanic batches, covering rares as well as commons. | Every target name resolves; edge-case rules tests and representative cross-color deck comparisons. Never promote a reduced pool as full FDN. |
| 6. Fair Limited search | Replace true unseen opponent-card composition with a publicly conditioned deck/pool belief sampler. Adapt evaluator and policy features to new rules and varied decks. | Changing hidden opponent composition with identical public evidence cannot change the searcher's belief inputs. Validate known-card conditioning and deterministic sampling. |
| 7. DraftZero integration | Map its observations/actions and deck inputs into the new interface; validate complete games, then measure representative search and training throughput. | Rules parity and hidden-information checks first. Substantial runs use the supported throughput guard and current compute policy. Any design/result gate follows the single-lead review process. |

Start with gameplay from already-built decks. Draft picks, sealed-pool deck
construction and BO3 Limited sideboarding are separate extensions. Define the
target card pool by names, not collector-number cutoffs or every FDN product.
The source reference has 286 names, including all five basics. Before card
implementation it had seven registry matches and 279 missing names. The two
starting fixtures contain 39 unique names, originally with 36 missing. Their
baseline support was UG 17/40 copies and WG 8/40. Batch A added six names:
Plains, Healer's Hawk, Fleeting Distraction, Cathar Commando, Spectral Sailor
and Treetop Snarespinner. That first batch reached 13/286 reference names and
19/40 copies in each fixture. The fixture stack reached **43/286
reference names** and **40/40 copies in each fixture**, with no unsupported
fixture name. Milestone 5's first batch adds seven keyword-only creatures
(`fdn_keyword_creatures_v1.md`), reaching 50/286, and the second adds
gainlands and life-gain creatures (`fdn_gainlands_lifegain_v1.md`), reaching
61/286, and the third adds trigger creatures and three instants
(`fdn_triggers_tricks_v1.md`), reaching **73/286**. One reference
planeswalker remains partial and 212 names are
missing from the wider reference; `../reports/fdn_remaining_pool_inventory_v1.md` sizes them.

Milestone 3's opt-in engine priority presentation is implemented on
`codex/fdn-priority-windows-v1`; see `limited_priority_windows_v1.md`.
The originally missing fixture names have seven mechanic batches in
`fdn_fixture_mechanics_v1.md`. Batch A implements complete card behaviors
through generic engine operations, with casting, target loss, payment,
restoration and combat tests. Batches B through G and London mulligans are
implemented in the owned PR stack. The remaining fixture work is validation
and repair of any observed failures, including complete CI and audits of the
Windows test summaries. The executed combat references passed.
The Forest/Island
smoke cannot establish Limited playing ability or FDN parity.
Milestone 4 is complete only after the actual card behavior, required rules,
original external games and reference/regression checks are verified together.
The partial planeswalker reference is not admitted as a playable fixture card.
