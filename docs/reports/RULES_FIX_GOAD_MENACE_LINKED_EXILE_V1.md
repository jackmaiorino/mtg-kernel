# Rules fix: goad and menace scan answers, linked exile of a ceased token (v1)

Branch `claude/rules-fix-goad-menace-linked-exile` from `main` 54725398:
`c064e9c1` (linked exile) and `9171c9b5` (scan answers). Not merged; the
lane owner (Codex) and Jack decide. Source report: Spellbench launch
benchmark, task B, sections 2 and 3.

## Root causes (confirmed in source and by failing tests)

1. **Scan answers with no legal completion.** `PolicySurfaceV5` splits an
   attack or block declaration into include/exclude steps, and
   `core_policy_action_candidates_v5` offered both answers at every step. The
   engine refuses an attack that omits a goaded creature
   (`validate_declare_attackers`) and a block with `0 < blockers < minimum`
   (`validate_declare_blockers`); the v4 aggregate surface already filtered
   both. Declining a goaded attacker mid-scan was accepted and left no legal
   declaration; a menace step could offer an answer the commit then refused.
   The session reported `stale_environment_binding` with fixed text.
2. **Linked exile of a ceased token.** Journey to Nowhere records the exiled
   token; the 111.8/704.5d sweep (`event::cease_to_exist`) removes it from
   exile but kept the record. Every later observation failed
   ("linked-exile exact card incarnation is not uniquely in exile"), and
   Journey leaving returned the ceased token to the battlefield (a test
   reproduces this).

## Fix

- `policy_surface_v5.rs`: one rule, `CombatScanV5::answer_keeps_a_legal_completion`,
  probes the engine's own validators with the smallest and the largest
  completion (exact for the current requirement shapes; the brute-force oracle
  test guards it). Fast-actor prevalidation and `apply_in_place` refuse a failing
  answer before mutation, with the engine's text. `feasible_scan_answers`
  exposes it.
- `rl.rs`: `policy_legal_action_candidates_v5` drops infeasible scan answers; a
  forced step offers its one answer at index 0. `core_policy_action_candidates_v5`
  and `legal_action_candidates_v5` keep the original pair (the flat V1/V2 origin
  contract). The V5 episode validator accepts the pair or one answer bound to
  the current candidate.
- `rl_session.rs`: an in-process `RlEpisodeSessionV1` (Spellbench bridge, Rust
  episode recorders) offers the filtered list. Sessions the JSONL server resets
  keep the original pair, which Python's V5 encoder requires; the surface still
  refuses a stranding answer if a JSONL client picks it. A refused step keeps
  code `stale_environment_binding` and appends the refusal text.
- `event.rs`: `cease_to_exist` drops rows whose exiled object is the ceasing id.
  Rows naming it only as source are kept (a token source's pending return
  resolves from its row).

## What moves

| Surface | Change |
|---|---|
| In-process `RlEpisodeSessionV1` | At a forced scan step (goad, or a block that must end empty or reach the minimum) `legal_actions` has 1 entry, not 2; `selected_index`, environment and core hashes change there. Seeded runs with a sampling policy diverge from that step. |
| JSONL wire (V5 and V6) and Python clients | Menus unchanged, so `features.py` (V5, hash-pinned) keeps working. A stranding pick is now refused at that step, with the engine's text, instead of at the end of the scan; picks that keep a legal declaration behave as before. |
| Previously halted games | Elves goad dead ends and CawGates linked-exile halts now continue. |
| `FastActorSessionV1` flat V1/V2 | Candidate sets unchanged. An infeasible pick is refused at that step (retryable, same code) instead of stranding the scan. No completed trajectory changes. |
| Pinned goldens on `main` | None moved (full suite below). No pinned trajectory uses Elves or CawGates. |

## Fable cross-examination (2026-09-26)

Fresh read-only Fable reviewer, brief with the design (D1 to D7), source
paths, the Phase 1 precedent and project laws. Verdicts: D1 to D6 accept,
D7 accept with a disclosure requirement. Material points and dispositions:

1. D3 must use one predicate for prevalidation and apply; otherwise a
   fast-actor refusal becomes `InternalApplyFailure` and halts the episode.
   **Accepted**: single predicate; a fast-actor test pins the retryable
   rejection (removing the prevalidation check makes it fail).
2. Most plausible failure is drift between hand-written feasibility rules; the
   oracle should drive the session, not only the surface. **Accepted**:
   brute-force oracle walks both; mutation checks confirm both catch drift.
3. D7 (exiled side only) contradicts Codex's `f72b32bc`, which also drops
   source-side rows, and edits the same line. Fable verified the source-side
   drop is wrong in principle (it would strand a card exiled by a token copy of
   Journey or Mesmeric Fiend). **Accepted as a disclosure**: pin test
   `cease_to_exist_keeps_the_linked_exile_record_of_a_ceased_token_source`;
   Codex decides at merge.
4. Design A (filter inside `core_policy_action_candidates_v5`) would rewrite
   the frozen flat V1/V2 origin contract (`flat_validate_origin_decision_v1`
   maps index to include). **Accepted**: the session-layer boundary (D2).
5. Python V5 consumers (`client.py`, `trainer.py`, `rollout.py`,
   `sampled_evaluator.py`) would fail closed on forced Elves steps.
   **Superseded** by the channel split after the Codex review below: JSONL
   menus are unchanged; no `features.py` change.
6. With menace (minimum 2) every infeasible answer is a final answer the commit
   already refused, so the mid-scan refusal matters for goad and minimum 3+.
   Informational; the oracle covers minima 1 to 3.

## Codex diff review (2026-09-26)

One-shot `codex exec review --base main` (gpt-6-astra, xhigh effort). One P1:
single-answer menus on the existing JSONL protocol break Python's V5 encoder
(`features.py` rejects them and `client.py` raises `ProtocolError`), aborting
Python-driven Elves episodes, including ones that would have continued.
**Accepted**: menus are split by channel. JSONL-reset sessions keep the
original pair on both wire versions; in-process sessions filter.
`jsonl_sessions_keep_the_original_scan_pair_for_the_python_v5_encoder` covers
it. A second one-shot review of the result found no actionable regressions.

Fable's focused check of the split (same reviewer): **accept**. Both JSONL
reset paths are covered (the retry path constructs nothing; the server never
restores). For Python V5 the change is a strict improvement but not a fix: a
policy that declines a goaded creature still aborts, now at that step with
the engine's text, so Bug 1 is fixed for in-process consumers. Burn/Rally
menus, hashes and transcript goldens are unchanged. Named failure mode: two
menu behaviours share one type and wire version with no visible marker, so
comparing an in-process Elves or Spy episode (Troll of Khazad-dum needs three
blockers) with a JSONL replay would differ at forced steps. **Accepted**:
comments on `prove_fast_actor_parity` and on the Burn-only V5 recorder, whose
records Python consumes.

## Census (not a golden)

Uniform random choice over the offered actions (splitmix64, fixed seeds),
in-process `RlEpisodeSessionV1`, mirrors, 300 env seeds per deck,
600-decision cap.
Episodes by how they ended:

| Deck | Build | Game over | Decision cap | Refused scan answer | Linked-exile halt | Zero-legal-actions halt |
|---|---|---|---|---|---|---|
| Elves | main 54725398 | 203 | 30 | 67 (52 attack, 15 block) | 0 | 0 |
| Elves | branch 9171c9b5 | 248 | 52 | 0 | 0 | 0 |
| CawGates | main 54725398 | 103 | 112 | 0 | 82 | 3 |
| CawGates | branch 9171c9b5 | 130 | 166 | 0 | 0 | 4 |

Elves offered a single forced answer at 144 of 120,476 decisions. The
zero-legal-actions halt is a separate defect (a Journey trigger with no legal
target) fixed only on the Phase 1 branch (`008379d8`); it rises by one because
games that used to halt now play on.

## Merge notes

- Phase 1 branch (`codex/learned-sideboarding-integration-v1`): `event.rs`
  conflicts with `f72b32bc` on the same `retain`; its test's source half
  contradicts the pin test above. Its V3/V4 filters (`cbfa2435`, `cccf1011`)
  keep working (the core pair is unchanged).
- Spellbench bridge (`spellbench/agent-bridge-v1`, not edited):
  `agent_bridge_names_the_session_error_when_a_validated_step_is_refused`
  (Elves seed 11) and `agent_bridge_passes_a_kernel_halt_through_with_its_reason`
  (CawGates seed 22) pin the fixed defects and need repointing; session
  rejection reasons now carry the refusal text.

## Evidence

- Failing first on `main` (each for the stated reason), passing after:
  `event::tests::cease_to_exist_drops_the_linked_exile_record_of_the_ceased_exiled_object`;
  `caw_gates_completion_v1`: `journey_exiling_a_token_keeps_no_linked_record_once_the_token_ceases`,
  `journey_leaving_never_returns_a_ceased_token_it_exiled` (the token came back);
  `policy_surface_v5::tests`: `declining_a_goaded_attacker_is_refused_before_the_scan_is_stranded`,
  `a_partial_block_that_cannot_reach_the_minimum_is_refused_when_answered`,
  `scan_accepts_exactly_the_answers_that_keep_a_legal_completion`;
  `rl_session::tests`: `session_offers_only_the_inclusion_of_a_goaded_attacker`,
  `session_offers_only_blocker_answers_that_keep_a_legal_block`,
  `refused_apply_carries_the_surface_refusal_text`;
  `rl`: `a_single_forced_scan_answer_validates_only_when_bound_to_the_current_candidate`;
  `tests/rl_session.rs` mirror sweep (24 seeds per deck: 5 refused Elves answers, 6 CawGates linked-exile halts).
- Added after the review and checked by mutation:
  `session_offers_exactly_the_scan_answers_that_keep_a_legal_completion` (fails if the
  session offers the unfiltered pair) and `fast_actor_rejects_declining_a_goaded_attacker_before_mutation`
  (fails with an episode halt if prevalidation skips the rule).
  `cease_to_exist_keeps_the_linked_exile_record_of_a_ceased_token_source` pins existing behaviour.
- Full release suite, `cargo test --release --locked --workspace --all-targets` (Windows):
  `main` 54725398 2,278 passed, 0 failed, 50 ignored; branch 9171c9b5 2,292 passed
  (the 14 new tests), 0 failed, 50 ignored. No golden or pinned test changed.
- `cargo fmt --check` clean; clippy `-D warnings` clean for the workspace and both
  feature sets; the store-production and CUDA feature test steps pass. The Python
  suite was not run (CI runs it only for Python or data changes; none here).
