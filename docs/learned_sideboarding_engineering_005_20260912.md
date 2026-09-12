# BO3 pilot interruption and Ward observation prerequisite

September 12, 2026. Source b7ef18d4518943130f8494be0a7b678e1028dab6 in the assigned learned-sideboarding worktree. This is a coverage diagnosis and next integration design, not a playing-strength result or a completed repair.

## Observed result

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

`effect.rs::validate_counter_unless_pays_generic` rejects a payable Ward prompt when multiple live stack items target the same permanent. Here Fireblast and Lightning Bolt both target Tolarian Terror. The resolving Ward trigger retains the exact bound stack item internally, but `rl.rs::pending_effect_semantic` reduces its choice to generic `PayCost`. Neither V6 extensions nor flat V3 revision 2 expose the missing relation. A scorer therefore cannot distinguish which spell accepting payment would preserve. This is an intentional coverage guard, not evidence that the binding was corrupted.

Ward applies to the particular spell or ability that triggered it. See the [official Ward rules explanation](https://magic.wizards.com/en/news/feature/strixhaven-school-mages-and-commander-2021-edition-release-notes-2021-04-16). Removing the guard, always declining payment, suppressing the halt or replacing the seed would change the task without repairing the observation. Existing source references, target arrays and structural paths must retain their meanings.

## Smallest correct successor

1. Expose a typed public Ward-payment relation with the exact bound targeter represented by its public stack-item position/context plus source object row, payer and generic cost. A source row alone is insufficient: distinct activated abilities can share one source, and current V3 has no dedicated object row for every nonspell stack item. Carry source/resolving-trigger context without pretending a resolving trigger remains a live stack row. Validate internal stack-incarnation authority before projection; do not expose arena IDs or hidden state as model features.
2. Version the rich observation extension, flat scorer relation and numeric feature identity together. Keep legacy consumers rejecting states they cannot represent. Prefer preserving revision-2 canonical bytes and tensors when no Ward record exists, and verify this in both Python and Rust. Adding even a null extension can change full-observation hash features in every state; if the chosen successor remaps those inputs globally, disclose and qualify that broader transfer. Cover simultaneous targeters, payment accepted/declined, countered or departed targeters, and source incarnation changes. Targeter changes must produce distinguishable scorer inputs; hidden-state renumbering must not.
3. Review CPU/Python encoding parity, generated contracts and strict import compatibility. Shared Net8 dimensions do not establish semantic compatibility. Decide and record any explicit frozen-weight transfer before use. The sideboard head binds source weights/git identity and embeddings, not the complete V3 play-feature contract. Unchanged weights, embeddings and sideboard features can therefore retain the frozen head with a separately recorded play-observation transfer; that does not qualify the new inputs. Preserve the existing fresh head and refit only as a separately identified later fit if play/embedding binding changes.
4. Qualify the exact failed case and one end-to-end byte-identical repeat with the successor binary. Create a new full engineering pilot identity using the existing seeds, registrations, arms, limits and controls; never combine new-source outputs with the interrupted pilot. The pilot remains a coverage/cost check, without head selection from its outcomes.

The implementation is not yet changed. No replacement pilot, paid allocation or GPU run has started. Current scope is diagnosis and a reviewable observation prerequisite; full production integration still also needs successor-player loading, explicit registered 75s and the remaining CPU/GPU/store/search seams in the [brewing plan](learned_sideboard_production_brewing_plan_v1.md).

## Review and evidence

The diagnostic source, build/replay logs and trace are preserved in [engineering-005/engine-failure](E:/mtg-kernel-learned-sideboarding-evidence/engineering-005/engine-failure/pre-fix-trace-001.txt). Diagnostic state dumps are engineering evidence only and must not enter actor training inputs.

Root verified all 11 diagnostic manifest input/artifact hashes. Two failed-game traces are byte-identical, which reproduces the failure; it does not qualify a repaired game's determinism. Independent Codex critique confirmed the missing payment relation and accepted this plan with three material changes incorporated above: retain frozen sideboard heads only under their actual weight/embedding/sideboard-feature binding; account for global hash-feature changes from an added null observation field; and distinguish stack-item instances that share a source object. Root also corrects the diagnostic draft's request for a live resolving-trigger stack index: use pending-resolution context because the trigger has already left the live stack. These are accepted design dispositions; there is no production patch to endorse yet.

Fresh Fable review session 1ecd593c-24d1-4908-84e1-ee124b0585c5 failed with weekly HTTP429, zero source reads and zero substantive tokens. [Receipts](E:/mtg-kernel-learned-sideboarding-evidence/engineering-005/fable-review-001/result.json) establish an unavailable consultation, not feedback or endorsement. Root accepts the causal source/trace diagnosis and preserves the halt. The versioned observation design still needs source-level cross-examination before implementation is treated as qualified. No repeated quota attempt, reset or alternative purchase occurred.
