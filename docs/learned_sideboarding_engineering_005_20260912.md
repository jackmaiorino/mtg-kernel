# Ward successor and completed BO3 engineering pilot

September 12, 2026. Measured Ward successor source `0f479bad488628840b8ec0b1c0e626926d692318` in the assigned learned-sideboarding worktree. The earlier interruption at source `b7ef18d4518943130f8494be0a7b678e1028dab6` is preserved below as historical diagnosis. The repair and engineering pilot are complete; playing strength, production integration and the full Multi-Deck BO3 Campaign remain unqualified.

## Completed successor pilot

| Check | Actual outcome |
| --- | --- |
| Fresh engineering pilot | 400/400 matches, 927 physical games, zero unresolved matches |
| Coverage | All 25 ordered matchups in each of four arms, both candidate seats and both initial choosers |
| End-to-end replay | Extra first-match result byte-identical, outside the 400-match denominator |
| Runtime | 262.3207898 seconds including wrapper and replay |
| Evidence closure | 401 match-file hashes verified by root; all recorded pilot children reaped |
| Previously failing match | Completed two games in a separate verification outside the pilot denominator |

The immutable binary SHA is `6838d98a97949cecfc8cb9fae57e8af68adc6cf359ac0b8d2f5b985724c32ec5`. [Result](E:/mtg-kernel-learned-sideboarding-evidence/engineering-005/ward-successor-001/RESULT.md), [summary](E:/mtg-kernel-learned-sideboarding-evidence/engineering-005/ward-successor-001/pilot-summary.json), and [completion](E:/mtg-kernel-learned-sideboarding-evidence/engineering-005/ward-successor-001/bo3-pilot-001/completion.json) bind the new execution. Original registrations, seeds, arms, limits, frozen play parameters, sideboard heads and static opponent were retained under the explicit Ward feature transfer. No candidate head was selected and no win-rate or production-promotion claim follows. Original fit and native measurements remain unchanged.

## Ward successor implementation

The active Multi-Deck BO3 Campaign now implements V3 feature revision 3 with exact pending and queued Ward payment relations. The rules engine retains targeter/source/payer/payment authority checks, with a stronger exact pending-effect binding. Observation ambiguity rejection now belongs to legacy observation consumers. The successor exposes the actual public stack item, payer and cost through rich observations, flat relations and numeric features. Empty Ward fields are omitted, preserving unaffected inputs. The old measured executable and its interrupted results remain unchanged.

| Verification before release build | Result |
| --- | --- |
| Focused Rust observation/scorer/feature/import checks | 67 passed |
| Existing Tolarian Terror rules integration tests | 8 passed |
| Actual native fixture emitter | Passed; four new Ward cases included |
| Current Rust/Python parity | 28 fixtures, 38,643 values, all bit-identical |
| Historical non-Ward compatibility | 24 real fixtures, 32,845 values and canonical bytes identical to revision 2 |
| Python feature tests / V2 generator tests | 28 passed / 8 passed |
| Independent source and runner review | No blocking finding; read back compiled commit/feature receipt before dispatch |

The initial compile caught the expected shared V2 source-footprint pin change; the canonical generator refreshed only source/inventory digests, with unchanged V2 golden-case values and action contract. A too-broad test substring also selected an unrelated long Store test; root stopped that owned test process and reran explicitly scoped modules. A queued-state test fixture initially auto-advanced forced passes; it now includes real legal responses and verifies two queued payments before separately testing pending resolution. Original logs are retained. These preparation corrections did not alter frozen measurements or relax a failed rule.

Release build, exact formerly failing-match execution and the complete fresh engineering pilot subsequently passed under the separately recorded identity above. New successor-loader and explicit-registration code now in the dirty worktree was not compiled into that measured binary. Root's CLI integration, focused build, compatibility checks and new end-to-end registered-deck execution remain next.

## Historical original pilot interruption

| Check | Actual outcome |
| --- | --- |
| Approved engineering pilot | Stopped on the first no_change-seat0 batch after 30/400 matches and 67 physical games |
| Time | 18.964 seconds for the interrupted wrapper; insufficient for a full-pilot ETA |
| Failure | Terror versus Rally, first game, before sideboarding |
| Exact failed-game replay | Reproduced the halt at 34 physical decisions / 39 policy steps |
| Exact continuation diagnostic | Returned `the current public schema cannot identify the Ward-bound stack item unambiguously` |
| Other candidate arms and replay check | Did not run; no sideboard comparison or deterministic-replay qualification |

The original output is [engineering-004/bo3-pilot-001](E:/mtg-kernel-learned-sideboarding-evidence/engineering-004/bo3-pilot-001/completion.json). Preserve all completed records and source/binary/config identities. The 370 unresolved matches are not losses, draws or discardable cases. Match seed 4925086575387925151 and game environment seed 15386019971197935248 identify the failed case. The frozen native screen, confirmation reservation, heads and grouped-fit measurements remain unchanged.

## Cause and consequence

In the original measured source, `effect.rs::validate_counter_unless_pays_generic` rejected a payable Ward prompt when multiple live stack items targeted the same permanent. Here Fireblast and Lightning Bolt both targeted Tolarian Terror. The resolving Ward trigger retained the exact bound stack item internally, but `rl.rs::pending_effect_semantic` reduced its choice to generic `PayCost`. Neither the then-current V6 extensions nor flat V3 revision 2 exposed the missing relation. A scorer therefore could not distinguish which spell accepting payment would preserve. This was an intentional coverage guard, not evidence that the binding was corrupted. The successor moves this coverage restriction to legacy observation boundaries and supplies the missing relation.

Ward applies to the particular spell or ability that triggered it. See the [official Ward rules explanation](https://magic.wizards.com/en/news/feature/strixhaven-school-mages-and-commander-2021-edition-release-notes-2021-04-16). Removing the guard, always declining payment, suppressing the halt or replacing the seed would change the task without repairing the observation. Existing source references, target arrays and structural paths must retain their meanings.

## Smallest correct successor

1. Expose a typed public Ward-payment relation with the exact bound targeter represented by its public stack-item position/context plus source object row, payer and generic cost. A source row alone is insufficient: distinct activated abilities can share one source, and current V3 has no dedicated object row for every nonspell stack item. Carry pending-resolution context for the current payment and queued-trigger context before resolution. Current source inspection corrects the earlier draft: this engine retains the resolving item at the live stack top until completion. Avoid duplicating that item as both queued and pending payment. Validate internal stack-incarnation authority before projection; do not expose arena IDs or hidden state as model features.
2. Version the rich observation extension, flat scorer relation and numeric feature identity together. Keep legacy consumers rejecting states they cannot represent. Prefer preserving revision-2 canonical bytes and tensors when no Ward record exists, and verify this in both Python and Rust. Adding even a null extension can change full-observation hash features in every state; if the chosen successor remaps those inputs globally, disclose and qualify that broader transfer. Cover simultaneous targeters, payment accepted/declined, countered or departed targeters, and source incarnation changes. Targeter changes must produce distinguishable scorer inputs; hidden-state renumbering must not.
3. Review CPU/Python encoding parity, generated contracts and strict import compatibility. Shared Net8 dimensions do not establish semantic compatibility. Decide and record any explicit frozen-weight transfer before use. The sideboard head binds source weights/git identity and embeddings, not the complete V3 play-feature contract. Unchanged weights, embeddings and sideboard features can therefore retain the frozen head with a separately recorded play-observation transfer; that does not qualify the new inputs. Preserve the existing fresh head and refit only as a separately identified later fit if play/embedding binding changes.
4. Qualify the exact failed case and one end-to-end byte-identical repeat with the successor binary. Create a new full engineering pilot identity using the existing seeds, registrations, arms, limits and controls; never combine new-source outputs with the interrupted pilot. The pilot remains a coverage/cost check, without head selection from its outcomes.

The Ward implementation, focused verification and replacement engineering pilot are complete as described above. No paid allocation or GPU run was started. Strict successor-player loading and explicit registered-75 code are the next uncommitted, uncompiled phase; root's CLI connection and end-to-end qualification remain pending. Full production integration still needs these seams plus CPU/GPU/store/search work in the [brewing plan](learned_sideboard_production_brewing_plan_v1.md).

## Review and evidence

The diagnostic source, build/replay logs and trace are preserved in [engineering-005/engine-failure](E:/mtg-kernel-learned-sideboarding-evidence/engineering-005/engine-failure/pre-fix-trace-001.txt). Diagnostic state dumps are engineering evidence only and must not enter actor training inputs.

Root verified all 11 diagnostic manifest input/artifact hashes. Two failed-game traces are byte-identical, which reproduces the failure; it does not qualify a repaired game's determinism. Independent Codex critique confirmed the missing payment relation and accepted this plan with three material changes incorporated above: retain frozen sideboard heads only under their actual weight/embedding/sideboard-feature binding; account for global hash-feature changes from an added null observation field; and distinguish stack-item instances that share a source object. Root accepts pending-resolution context for the active payment. Implementation inspection subsequently corrected the earlier claim that the resolving trigger had left the stack: `validate_pending_effect_choice` authenticates it against the live top item. The projection keeps the semantic distinction without changing engine stack lifecycle. These are accepted design dispositions; implementation and independent Codex source/runner review completed for the measured Ward successor. The Multi-Deck BO3 Campaign remains active.

Fresh Fable Ward review session `1ecd593c-24d1-4908-84e1-ee124b0585c5` failed with weekly HTTP429, zero source reads and zero substantive tokens. [Receipts](E:/mtg-kernel-learned-sideboarding-evidence/engineering-005/fable-review-001/result.json) establish an unavailable consultation, not feedback or endorsement. Independent Codex source/runner review found no blocking Ward finding; root proceeded within the assigned engineering scope while recording the absent Fable consultation. The separate successor-integration review `6063b7ec-e34e-4728-b107-e93a4c71a25e` also failed weekly HTTP429 with zero source reads/tokens, as recorded in its [receipt](E:/mtg-kernel-learned-sideboarding-evidence/engineering-006/successor-bo3-001/fable-review-001/result.json). That failure supplies no endorsement for the dirty next-phase code. No quota reset or alternative purchase occurred.
