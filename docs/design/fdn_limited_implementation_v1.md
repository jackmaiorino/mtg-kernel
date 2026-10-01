# Issue 110: FDN Limited implementation

Assigned by Jack on 2026-09-30. Implementation branch:
`codex/fdn-limited-deck-loading-v1`, based on public main `d9e0910a`.
This is engineering work; no training, evaluation campaign or playing-strength
claim is authorized by the work plan. Preserve the lead's current PC reservation.

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
The source reference has 286 names, including all five basics, with seven
current registry matches and 279 missing names before further pool auditing.
The two starting fixtures contain 39 unique names, of which 36 are missing.
The UG fixture has 17/40 supported copies; the WG fixture has 8/40. This
36-name union is the first card batch after the custom-deck interface.

Next bounded implementation: milestone 3's priority and combat foundation,
with a mechanic inventory of the 36 missing fixture names. The Forest/Island
smoke cannot establish Limited playing ability or FDN parity.
Milestone 4's real fixtures remain visibly unsupported until their
actual card behavior and milestone 3's dependencies land. No delivery date is
promised before the mechanic inventory and first complete-game comparison.
