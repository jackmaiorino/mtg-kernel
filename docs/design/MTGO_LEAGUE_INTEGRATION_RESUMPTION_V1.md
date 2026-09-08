# MTGO League integration: resumption design v1 (revision 5)

Date: 2026-09-08. Author: LEAD (Fable). Status: revision 4 after Codex review rounds 1 (gpt-5.6-sol xhigh, CHANGES REQUIRED), 2 (gpt-6-astra xhigh, CHANGES REQUIRED with replacement clauses, adopted), 3 (gpt-6-astra high, CHANGES REQUIRED precision-only, adopted verbatim; "no further architectural redesign is requested"), and 4 (gpt-6-astra low, COUNTERSIGN "for Jack's design ratification. This closes the design review, not live-use authorization"). Revision 5 (2026-09-08, after the Flat V2 overlay-digest finding and its xhigh Codex consult) makes the integration branch a deployment branch and defers main-line landing to a ratified change at a campaign boundary; sections 6.1, 7, and 9 changed. For Jack's rulings in section 9. Nothing in this document authorizes live client input, event entry, or spending.

## 1. Goal and claim boundary

Goal: a composite agent plays Magic Online (MTGO) Pauper League matches on Jack's main account, receiving only information visible to the seated player. The composite agent is: the fixed mtg-kernel checkpoint inside the ratified bounded test-time search for duel decisions, plus a deterministic pre-registered controller for mulligan, London bottoming, and sideboarding, plus a human operator for login, navigation, event entry, deck selection, spending, and the kill switch.

Claim boundary: every result names the actual deployed configuration, including whether bounded search was enabled, never "the model plays" unqualified. League results are a public-play diagnostic and a demonstration; the 60 percent claim stays defined against CP7. No League outcome tunes any pre-registered constant, selects a checkpoint, or feeds training. Every started match is reported with its final result, its automation-completion status, and the takeover reason if any; fully autonomous win-loss is reported as a conditional subset alongside the takeover rate, never as the whole record.

## 2. Authorization: an external prerequisite, not an owner decision

Record so far (Jack's words in Codex session transcripts rollout-2026-08-03T20-49-11 and rollout-2026-08-08T10-23-46 under `~/.codex/sessions`, plus the sent email in Gmail thread 19fca59907c641d0 of 2026-08-04 to accounts@daybreakgames.com):

- 2026-08-08: permission on the main account, sole condition "we don't reverse engineer the stack to the point where we see information we aren't supposed to".
- 2026-08-10: "equivalent approval for leagues and challenges".
- 2026-08-12: "free reign as long as we aren't feeding data to the model/us that wouldn't be visible in the UI".
- 2026-08-14: the second local account is "a second account you control, we got approved for it under the same guidance".

The 2026-08-04 email described a system that "would not ... inject code". The shipped transport loads a bootstrap DLL into MTGO.exe with CreateRemoteThread, hosts the producer in the client's CLR, reads allowlisted WPF view-model getters, and dispatches through the client's own `IGame.ExecuteAction`. Daybreak's terms require express permission for client modification and gameplay-affecting programs. Jack's recollection cannot settle whether the reply covers that mechanism.

Rule adopted: no further injected live use (observe-only included) until the actual Daybreak reply bytes are retrieved, filed under the lane artifact home, and read by the lead and Codex. If the reply does not explicitly supersede the no-injection description, Jack sends a disclosure naming DLL injection, CLR hosting, allowlisted WPF inspection, automated dispatch through the client's own action interface, two-account Freeform tests, public open-play rooms, and paid League play, and waits for written acceptance. Offline work continues meanwhile. If Daybreak declines injection, the program reassesses option C, noting that UI Automation and synthetic input are not automatically authorized either.

Information boundary kept from the branch: any value that leaves the sealed producer must have a demonstrated on-screen counterpart for the seated player; hidden cards, library order, opponent private state, RNG state, and internal identifiers never leave it, and the same restriction binds the operator and the logs. Chat is never read by the model and never logged.

## 3. Where Codex stopped

- Branch `codex/mtgo-black-box-adapter-v1`: 408 commits from 2026-08-08 to 2026-08-14 15:48 EDT, fork point e930890b, never merged. The lead recovered and committed Codex's last uncommitted work (two-local-client observe-only broker) as c661ed3e on 2026-09-08; all three live broker hashes reproduce from source with VS 18 BuildTools.
- Last live state (2026-08-14 23:20Z): two local clients (main plus approved second account) seated in a no-cost Freeform Buddy Challenge with 60-card basic-land decks; the two-local observe-only broker reached the live primary duel; the producer returned the fixed abstention `projection_incomplete` at the opponent's beginning-of-combat pass window.
- Cause, verified in source on 2026-09-08: `IsSupportedNoncombatPriorityPhaseV1` in `SanitizedVisibleDecisionV1.cs` admits ordinary priority only in upkeep, draw, main 1, main 2, and end; `BeginCombat` maps to `begin_combat` and every combat phase is excluded by design. Phase exclusion is a sufficient explanation of the historical abstention, which was correct fail-closed behavior; without the historical diagnostic it does not prove which guard returned first.
- Readiness (non-actuating `check_mtgo_competitive_wiring_readiness_v1`): `blocked_missing_ratifications_and_model_interfaces`; every live authority flag false. 33 compile-pinned ratification roots across the two Rust crates are `None`; one offline Solitaire calibration root is populated.
- Verified on 2026-09-08: both Rust crates compile and pass 539 offline tests; the producer builds with dotnet; the merge onto the main tree conflicts only in `data/flat_policy_v2/goldens_v2.json`, which regenerates cleanly; the merged kernel compiles; the full lib suite passes 1709 of 1710 tests, the one failure being the frozen contract-digest golden explained in 6.1.
- Client pins: 3.4.158.4691 (installed 2026-08-11) is still the newest build on disk; the next login will auto-update and break every pin.
- Card coverage: the kernel registry has 162 definitions and the Pauper pool report lists all 150 pool cards as fully supported for the nine runtime decks. Codex's August figure of 49 of 136 is stale. Opponent cards outside the pool remain unrepresentable (section 6.9).
- Two sentences in Codex's status documents that call a two-player corpus unauthorized predate Jack's 2026-08-14 approval of the second account and are stale.

## 4. Existing pipeline and per-component status

| Stage | Component | Status |
|---|---|---|
| Client identity | broker `ExactLiveMtgoIdentityV1`: process count, x64, Authenticode, exact version, DLL hashes, start time | proven live (2026-08-13, 2026-08-14) |
| Transport | broker injects bootstrap DLL, CLR-hosts producer, shared-memory channel, strict validator child | proven live to the seated duel (2026-08-14) |
| Visible projection | producer walks WPF `DuelScene`, reads allowlisted getters, emits `MtgoPlayerVisibleDuelDecisionInputV1` | offline synthetic only; live abstained by phase exclusion |
| Decision scoring | `score_external_observation_v1` (kernel seam, bit-exact with the session path) | offline; consumes full `ObservationV5`, not the visible schema |
| Player-visible scorer | `PLAYER_VISIBLE_NATIVE_SCORER_V1.md` | designed, not built |
| Search wrapper | `model_guided_search_core_v1` takes `&FastActorSessionV1`; redeterminizer shuffles cards already present in hidden zones | cannot consume a visible projection; no visible root constructor exists |
| Dispatch | sealed `IGame.ExecuteAction` through the dispatch-admitted broker (single-process build only); replay key is hash-only; Rust ratification roots `None` | offline synthetic only; never sent live |
| Postcondition | before and after composed-frame bracket plus Game Log corroboration (direct-source contract) | DXGI capture proven live; not yet joined to dispatch |
| Combat | attacker toggle and blocker step transactions | offline synthetic only |
| Pregame | non-model mulligan heuristic; card-aware scoring over DXGI | Solitaire calibration only; no visible pregame producer |
| Sideboard | sequential sideboard interface | contract only; no unchanged-sideboard submission surface |
| Pre-entry | event browser, deck chooser, deck gate over pixels plus UI Automation | League and Challenge listing traced live; third state never captured; not on the path under B+ |
| Public history | Game Log parser | offline; validation-only import required by the scorer contract before dispatch |

## 5. Options

A. Continue Codex's full program: automate navigation, entry, pregame, gameplay, sideboarding, and lifecycle with hash-pinned reviewed corpora for every screen.

B+ (recommended, conditional on section 2): human operator owns login, navigation, event entry, deck selection, spending, and the kill switch; the in-process producer owns primary gameplay perception and sealed dispatch; a minimal independent capture bracket is retained for postcondition confirmation, lifecycle detection, and sampled consistency audits until a direct-visible successor with a private generation token and bounded action-specific postcondition polling is qualified; pregame and sideboard decisions come from deterministic pre-registered rules executed by versioned visible producers.

C. Rebuild on UI Automation and pixels only. Held in reserve for the case that Daybreak declines injection; not automatically authorized either.

Why B+: everything it keeps has reached a live duel or is required by the adopted contracts; everything it removes from the critical path (navigation automation, entry, deck-gate corpora) was never proven live and involves money or credentials, which stay human acts.

## 6. Design of the recommended path

### 6.1 Repository integration

- `lead/mtgo-integration-v1` remains a deployment branch, integrating explicitly selected main-tree commits. It was created on 2026-09-08 as merge 324b1325 of `codex/mtgo-black-box-adapter-v1` at c661ed3e onto the main tree head 75406ffe, with goldens regenerated through `uvx uv@0.11.29 run --no-sync python python/tools/generate_flat_policy_v2_goldens.py` (the auto-merged feature inventory passes the generator's `--check`), plus 855e41a8, which makes `validate_external_fixed_action_relation_v1` require a present `ActivateManaAbility.cost_target` to be a battlefield permanent controlled by the actor. Current science campaigns and S1 use their recorded worktrees and binaries unchanged.
- Why it does not land on the main tree now: the Codex branch adds code to `mtg-kernel/src/flat_policy_v2.rs`, and that file's byte digest is the Flat V2 overlay digest, which feeds the composite and canonical inventory digests and `FLAT_POLICY_CONTRACT_DIGESTS_V2`. The encoding is unchanged (goldens fixture content byte-identical; the external path is bit-exact with the session path), but the provenance digests move. Consequences on the main tree would be: the frozen joined-frame serializer golden in the action-block gradient diagnostic fails (re-baselined once under CLAUDE #236, a once-only ruling and not standing permission); old Flat packets would fail the enclosing scorer-contract equality checks in async rollout packet validation, checkpoint batch scoring, and trainer batch scoring; the terminal-blind coefficient screen's corpus commitment would change with identical inputs; and search authorities and S1 shard merging bind the compiled engine commit in any case. Relocating the code without touching the file is impossible because the external path calls private encoder methods. This is legacy encoding preservation with provenance changes, not serialized-byte equality across the revision.
- Deployment builds carry their own source, executable, checkpoint, and contract identities. The deployment record binds the exact integration commit and its science base, the executable SHA-256, the toolchain including the linker, the checkpoint, run, and parameter hashes, the full Flat contract digest set, the external and visible scorer identity, the applicable wrapper configuration, and the qualification evidence; the adapter's existing authority roots remain required. The frozen serializer golden requires a separately approved deployment re-baseline before qualification closes; the test is neither suppressed nor made to derive its expected value from the implementation under test.
- Main-line integration is deferred to a separately ratified, narrow-interface change at a campaign boundary: after the current campaign and the pending S1 evidence are sealed and before a subsequent campaign pins its binaries. It consists of one provenance-changing Flat V2 contract revision with a narrow validated encoding interface (not blanket exposure of the five private encoder methods), all external-scoring code moved out of the hashed file, and the rule that ordinary external-scoring development must not edit `flat_policy_v2.rs` while legitimate core-contract changes require separate review. Historical evidence is neither rewritten nor relabeled. The proposed ruling text is section 9 item 11.
- The integration crates stay outside the workspace `Cargo.toml` (own lockfiles). Add explicit CI commands for both crates and both PowerShell policy suites so they cannot rot silently.
- Codex reviews: the diff review of 855e41a8 (gpt-6-astra, medium) found no defects; the overlay-digest consult (gpt-6-astra, xhigh) is filed as LEAD_MTGO_FLAT_V2_OVERLAY_DIGEST_codex-opinion.txt under the lane home.

### 6.2 Live projection recovery, dispatch path, and rehearsal ladder

Offline first (no client):
1. Add a closed abstention-reason enum to the producer (for example `ordinary_priority_phase_not_supported`, `chrome_projection_incomplete`, `action_binding_incomplete`) exporting no phase value, getter result, exception, or object. Detailed refusal codes identify only public-surface failures; private guard failures retain their generic refusal code.
2. Extend the ordinary-priority slice to combat-phase pass windows (begin combat, declare attackers with no attackers, declare blockers with no blockers, combat damage, end of combat) under the same fail-closed rules, with synthetic fixtures. Combat-phase admission requires actual seated-player priority and an enabled bound Pass action; declaration and mandatory-choice prompts never become Pass decisions merely because their phase is admitted.
3. Build and qualify a two-local, dispatch-admitted broker variant with unambiguous target-process and account binding (the current dispatch build requires exactly one MTGO process; the recovered two-local build is observe-only).
4. Replace the producer's hash-only replay key with a private observation-generation token that never reaches the model or operator; single-use request semantics; fresh re-observation must reproduce identical decision bytes and action ordering before any dispatch.
5. Derive the exact reachable-action inventory for the deployed 75 cards plus adversarial opponent actions: mana choices, targets, modes, ordering, optional triggers, sacrifice and discard prompts, stack responses, multiple combats, concession, terminal states, mulligan, bottoming, unchanged sideboard submission. Each family gets a producer mapping, a binder, a readiness flag, and fixtures; unmapped families abstain.

Live, gated on section 2 and on a re-pinned client (6.8):
6. Reproduce the 2026-08-14 two-local Freeform state and obtain the first live data-bearing decision on an ordinary priority window; file it as the first real duel corpus case; manual review.
7. Full basic-land and then real-deck games with observe-only brokers until every reachable family in step 5 has live coverage or a documented abstention. This is reconnaissance; a documented abstention does not qualify a family for autonomous operation, and unsupported families stay disabled.
8. First live dispatch: `Pass` (see section 7 checklist), then land, cast, activate, attack, block, each with the confirmed postcondition and the global next-input hold.
9. Two-local best-of-three Buddy Challenge: sideboard screen, between-games flow, match terminal states.
10. Lifecycle qualification (6.7) in the two-local setting.
11. Public open-play rooms against humans with Jack present (shadow first, then dispatch), before any League entry.

### 6.3 Kernel-owned player-visible scorer and history importer

Implement `PLAYER_VISIBLE_NATIVE_SCORER_V1.md` as written: kernel module `external_player_visible_scoring_v1` that encodes the Flat V2 packet directly from `ExternalPlayerVisibleDecisionV1`, with one documented constant neutral encoding per simulator-only feature, exact-name card resolution against the frozen catalog, fail-closed abstention on any hidden value, and a parity test enumerating every non-neutral delta against the simulator packet on a shared fixture.

The public-history importer is implemented now as validation-only: it validates and commits the ordered public streams before any dispatch, feeds composite combat history, replay prevention, recovery, and search-root reconstruction, and the score must report `public_history_used_by_model_v1 == false` with a test proving zero tensor effect. Flat V2 bookkeeping features correlated with prior actions are neutralized in the visible path.

Compatibility gate (frozen before any result is read): a fixed corpus of kernel self-play decisions; exact action bijection and coverage; finite outputs; a quantized policy-distribution distance; value error; greedy agreement stratified by seat, phase, and action family; and a CP7-free paired terminal win-loss noninferiority screen in the simulator where one seat uses the complete visible path. Before qualification results are read, a gate specification fixes all corpus sizes, metrics, thresholds, paired sample sizes, seat allocations, confidence procedures, and failure handling. Raw-policy qualification and search-successor qualification are separate gates; search-action agreement belongs to the successor's gate, not to the raw gate. One-shot accept or reject. Revising neutral constants against the same corpus afterwards is tuning and is not allowed. The first sole-action Pass requires only conformance of the admitted projection, scoring, binding, and confirmation path; full strength qualification is required before any public automated play.

### 6.4 Player-visible search root

The ratified search wrapper needs a full `FastActorSessionV1`, and its redeterminizer only permutes cards already present in hidden zones. A League opponent's unseen cards do not exist in the visible projection and may not be copied from client state. Required successor, to be designed as its own sketch for Jack's ratification:
- a constructor from visible facts, the seated player's own known deck, validated public history, and an explicit opponent-card prior;
- the opponent-card prior drawn only from the kernel's own nine-deck pool, so no human game data enters; any prior derived from the human metagame is a separate ruling under the no-human-game-data law;
- before observation, pinned deck weights and a pinned conditioning rule; sampling from all registered lists consistent with validated public evidence, preserving uncertainty, known cards, multiplicities, and shuffle effects; the mixture over compatible decks is an approximate prior, never an exact posterior; card accounting distinguishes repeated appearances of one card from additional copies, and deck cards from tokens, copies, and control changes;
- zero compatible support disables search for that decision, permitting the raw policy only while its complete visible input remains supported;
- inferred deck identity and sampled cards remain search hypotheses and never become observed actor inputs;
- a deterministic League search seed derivation;
- executable rules coverage for every sampled card;
- its own ratified identity, CP7-free strength qualification, and mechanical tier selector (section 6.6);
- until this successor is ratified and qualified, live play uses the raw policy through the visible scorer and is labeled as such; raw-only League entry needs its own owner ruling.

### 6.5 Pregame and sideboard controllers

Deterministic, disclosed, pre-registered rules executed by versioned visible pregame and sideboard producers with their own binders and readiness flags. A provisional sketch of the rules, which approval of this design does not ratify: keep any seven with two to five lands; otherwise mulligan once, redraw seven cards, and bottom exactly one, applying the land-count and mana-value rule to that new seven-card hand (bottom the highest mana-value non-land, or the excess land when lands exceed five); never below six; play first when offered the choice; submit the sideboard unchanged before every game after the first. The sketch is a simple disclosed controller for integration work and is no evidence of good mulligan strategy across the nine decks: forced keeps can retain zero-land redraws and five-land redraws can become five-land sixes.

Before controller qualification, Jack ratifies a total rule table covering the play or draw choice, every keep, mulligan, and London-bottoming case, the land and mana-value definitions, deterministic tie-breaking, and the unchanged submission between games; the successor explicitly replaces the existing model-only pregame and sideboard authorization requirements in the branch. These rules do not violate the terminal-reward law if frozen before outcomes, but the search ruling does not automatically cover them; they need their own ruling (section 9).

### 6.6 Clock and tier selection

MTGO competitive 1v1 gives each player 25 minutes per match and clock expiry loses the match. The rule must be executable: measure, on the exact MTGO host, projection through confirmed postcondition per decision family using two-local timing traces (full best-of-three traces from the two-local best-of-three stage); a qualified numeric clock parser reads the visible clock. S1 and exact-host traces establish timing feasibility only. The search-enabled League successor has its own ratified identity, CP7-free strength qualification (the ratified S2 search-gain screen), and mechanical tier selector; this document does not replace the existing S2 selector with a largest-feasible tier. Timing feasibility can only exclude tiers the selector would otherwise pick. Derive the raw-policy fallback threshold from a high-quantile full best-of-three raw-path clock requirement plus a fixed emergency reserve; cache the raw choice before search starts and revalidate the decision before dispatch. Non-timing failures disable the model for the match; the agent never blind-passes.

### 6.7 Operator loop and lifecycle

Human: launch clients, log in, open the event, select the deck, choose the entry option, pay, hold the kill switch. Adapter: per decision, verify client identity, obtain the projection, validate history, score, dispatch one sealed action, confirm the postcondition, log the decision with commitments. Fixed and hashed client settings before any live step: priority stops, auto-yield, auto-pass, auto-tap, trigger ordering, interface options. Lifecycle tests: stale decision, duplicated request, postcondition timeout, broker death, client crash, reconnect, opponent concession, both clocks expiring, sideboard timeout, game and match transitions, occluding dialogs, notifications, chat. Kill switch semantics: a disable acknowledgment prevents new dispatch and cancels pending work; an action already submitted to the client may complete; human takeover waits for dispatch quiescence and a fresh visible state. A human intervention disables the model for the rest of that match, and the match is reported with its takeover reason (section 1).

### 6.8 Client drift procedure

After any client update: launch the client without injection; regenerate every pin (executable, DLLs, bootstrap, producer, validator, brokers) by script, never by hand; rebuild the brokers; start fresh client processes (the managed producer cannot unload); requalify observe-only through a complete two-local duel, not just the lobby; rerun both offline suites and both policy suites.

### 6.9 Unknown opponent cards

League opponents may play cards outside the 150-card kernel pool, and the pool report's scope is canonical mainboard best-of-one, so it does not establish arbitrary-opponent or best-of-three coverage. No qualified unknown-card encoding exists for this checkpoint: the tensorizer treats zero-token objects as synthetic and excludes them from the object projection, an unused embedding row has no established meaning, the visible schema carries no general mana-value or card-type fields for battlefield cards, and an unknown static ability can change decisions about other objects.

Rule for v1: automated gameplay is disabled for the match when a required visible card cannot be resolved or its required semantics cannot be represented; the agent submits no default action and requests human takeover. Unknown-card encoding requires a separately versioned and qualified observation contract, which is future work, as is expanding the catalog toward the current metagame (engine work outside this lane). The takeover rate this rule produces is reported per section 1 and is expected to be high against the open field until coverage grows.

## 7. Before the first live Pass (checklist)

- Daybreak reply retrieved, filed, and read; adequate for injection and attended dispatch.
- Jack's authority for the step limited to the two named accounts, Freeform, Pass only, no spending, exact binary hashes, a bounded session.
- Client re-pinned per 6.8; fresh processes with the intended producer version loaded.
- Two-local dispatch broker built and the target client and account bound unambiguously.
- `cost_target` validation fixed in the pinned deployment commit.
- Non-abstaining ordinary-priority corpus manually reviewed.
- Visible scorer and validation-only history importer conformance-tested on the admitted projection, scoring, binding, and confirmation path (full strength qualification is required before public automated play, not before this step).
- Retained postcondition and lifecycle bracket implemented and joined to dispatch.
- Private generation token and single-use request semantics implemented; re-observation proves identical decision bytes and action ordering.
- Pass is the model-selected action, preferably in a sole-action state; no index override.
- Confirmed postcondition and global next-input hold operational.
- All exact authority roots populated for the binaries, corpus, checkpoint, and account scope.
- Kill switch tested: disable acknowledgment prevents new dispatch and cancels pending work; takeover waits for dispatch quiescence and a fresh visible state.
- Auto-yields and automatic client choices cleared; chat ignored.
- The step is labeled actuator qualification, not League readiness.

## 8. Work breakdown

| # | Item | Needs Jack present | Depends on |
|---|---|---|---|
| 0 | Integration branch, goldens, cost_target fix, CI commands, Codex review | no | none |
| 1 | Producer abstention-reason enum and combat-phase pass windows | no | 0 |
| 2 | Two-local dispatch broker; generation token; single-use semantics | no | 0 |
| 3 | Player-visible scorer, validation-only importer, raw gate specification and measurement | no | 0 |
| 4 | Reachable-action inventory for the deployed 75 cards; producer mappings and fixtures | no | 0; finalized only after the deck ruling |
| 5 | Pregame and sideboard controllers as versioned visible producers | no | finalized only after the rule-table ruling |
| 6a | Player-visible search-root sketch for ratification | no | 3 |
| 6b | Search-root successor implementation, its CP7-free qualification, and its S2-style selector | no | 6a ratified |
| 7 | Retained postcondition and lifecycle bracket joined to dispatch; numeric clock parser | no | 0 |
| 8 | Daybreak reply filed and reviewed; disclosure if needed | Jack only | none |
| 9 | Live projection recovery and first real duel corpus | yes | 1, 8, re-pin |
| 10 | Live dispatch ladder in two-local Freeform | yes | 2, 3 (conformance), 7, 9, and the section 7 checklist; dispatch beyond the admitted Pass slice additionally requires item 4's qualified mapping and binder for each enabled family |
| 11a | Two-local best-of-three and lifecycle qualification; full best-of-three timing traces on the intended supported deck and action configuration | yes | 10, 4, 5 |
| 11b | Clock rule frozen from 11a traces | no | 11a |
| 11c | Public open-play shadow (observe and score, no dispatch) | yes | 11a |
| 11d | Public open-play dispatch | yes | 11b, 11c, 4, 5, raw gate passed (3) |
| 12 | First League entry using the explicitly approved raw or search configuration | yes (spending) | 11d; separate League authority; completed qualification, exact-host timing, and rehearsal for that same configuration; search additionally requires 6b, and prior raw-policy rehearsal does not qualify the search deployment |

Offline preparation may proceed within existing lane authorization. Item 6b waits for 6a ratification; items 4 and 5 are finalized only after their respective rulings. Measurement and machine use remain subject to existing compute authorization.

## 9. Owner rulings required

1. Adopt B+ conditionally, with the section 2 rule that no injected live use happens before the Daybreak reply is filed and read.
2. Composite-agent claim wording (section 1).
3. The total pregame and sideboard rule table (6.5) as pre-registered constants.
4. Approval to deploy the mechanically selected checkpoint hash (the cycle-4 routing outcome), separate from the exact 75-card deck list, separate from a capped card-purchase authorization.
5. Split live authorities: observe-only two-local; two-local Pass; expanded two-local actions; public shadow; public dispatch; paid League. Each is granted separately.
6. Search-root prior source (kernel pool only, or a metagame prior under the no-human-game-data law), League seed derivation, clock tier and fallback identity, fixed MTGO settings, human-intervention reporting.
7. League approval bound to one entry, exact account, artifact, deck, and checkpoint hashes, a payment cap, kill conditions, and reporting of every started match, including human-completed matches; fully autonomous win-loss is a separately labeled conditional subset.
8. Data retention and no-chat-ingestion rules.
9. Confirmation of the fail-closed unknown-card rule (6.9) and its takeover reporting.
10. Whether a raw-policy-only League entry is acceptable before the search successor (6.4) is qualified, or whether League entry waits for it.
11. Main-line integration ruling, to be put with concrete hashes after the structural patch: approve one provenance-changing Flat V2 contract revision at the declared integration boundary; the existing session encoder, action ordering, tensor mapping, checkpoint parameters, and trainer semantics remain unchanged; evidence comprises the reviewed source diff, unchanged golden fixture content, external and session parity tests, and one matched-seed end-to-end legacy-path comparison with only enumerated provenance differences permitted; old and new overlay, composite, inventory, goldens-payload, and joined-frame golden hashes are recorded mechanically; historical artifacts remain untouched and retain their original execution identities; this is not a claim of cross-revision serialized-byte equality.
12. Approval of the deployment-only re-baseline of the frozen joined-frame serializer golden on `lead/mtgo-integration-v1`, with the old and new hashes recorded, before deployment qualification closes.

## 10. Non-claims

This document does not attest a live projection, live input, League readiness, or perception accuracy. It does not change any training, evaluation, or pre-registered constant.
