# Localized teacher intervention: feasibility and limits

The full retained128 candidate lost 20 net BO3 wins against its matched parent. The first-step control found mean retention-input policy movement about ten times larger for the actual teaching update than for momentum alone. Those results motivate testing whether the learned change can be confined to its intended action domain. They do not establish that routing would improve play, and neither result justifies resetting the original optimizer.

Training coverage is narrower than a general targeting skill: fixtures/teacher_data.rs produces 32 training roots using Lightning Bolt and Lava Dart, two tactical families, both actors and four counter settings. All 128 legal actions in train.json are ChooseTarget. These are synthetic DeclareBlockers positions with carefully arranged life, damage and combat facts; the gate below also admits ordinary positions outside that distribution. In-domain spell identity does not imply a correct target decision, and retained128's tactical fitting result does not certify its decisions in those ordinary positions.

Before deriving opportunity counts, target-route-census-plan-20260921.md fixed a structural gate: candidate-actor gameplay, at least two legal actions, every action ChooseTarget with remaining 1 and the same Stack source, controlled and owned by the actor, and card Lightning Bolt or Lava Dart. IDs 66/63 are tied to the named training specifications and every corresponding legal source reference, not guessed from registry array order. It uses no terminal outcomes, confidence thresholds, life cutoffs, deck or hidden state. This is a proposed restriction for diagnosis; no routed agent has been implemented.

Four implementation constraints are already established from source:

1. sideboard_play_policy_v1.rs::sample_scores consumes one seat RNG draw on every selection, including single-action menus. Each FrozenPlayPolicy owns its own RNGs. Routing between their ordinary select methods would skip draws on the baseline stream and change later off-gate sampled actions. A composite must score without sampling, then use one shared stream and the existing production sampler exactly once.
2. CompleteAgentPackageV1 and load_expanded_inference_v1 currently identify one gameplay model, optimizer ancestry and embedding table. A composite needs an explicit versioned identity that binds both models and the routing rule. It must not claim the baseline package identity while invoking teacher weights. Keep the old single-model schemas and frozen evidence unchanged.
3. The BO3 collector accepts concrete FrozenPlayPolicy instances and captures their last scored tensor together with actual sampled logits. A composite recorder must identify the active branch and capture that branch's bound input and distribution. Logging an inactive model's cached tensor or re-scoring a different route would invalidate the likelihood record.
4. Opening, sideboarding, value-dependent behavior and embedding provenance need explicit baseline semantics. Preserving off-gate logits alone does not promise a complete-agent intervention. In this search-free comparison the policy chooses from logits, but future search or training could use recorded values. A composite requires an explicit value rule and must not silently acquire continuation authority as one ordinary checkpoint.

The intended invariants are same-input equality outside the gate, one shared RNG draw schedule, exact model/source identities and actual live hidden-state invariance. They do not promise identical trajectories after a changed gated action. Model routing cannot be added to the existing evaluation by relabeling a single-model descriptor.

The first feasibility check is a read-only census over all 256 complete parent BO3 paths in the sealed512-match panel. Count all games and candidate decisions, then join the completed parent outcomes. With the invariants above, a baseline match with no gated decision follows exactly the same path by induction. Therefore baseline losses containing at least one gated decision give a finite-panel upper bound on additional wins for this gate. Eligible losses might be unrecoverable and eligible wins might regress. This is a necessary opportunity check, not an expected gain, generalization estimate, statistical strength gate or model selection result.

Pure-menu checks accept all 32 teaching menus, reject eight malformed/unsupported variants, and preserve admission under actor relabeling and consistent arena renumbering. They do not yet prove a live composite's hidden-state invariance or sampler parity. The new census is guarded by representative serial/parallel timing, exact reader output agreement, fresh owner checks and pinned inputs; staged immutable copies can be reused after per-read hash verification. No model, reward, rollout or frozen measurement changes are involved.

## Completed opportunity census

All 256 complete parent BO3 paths were verified, covering 600 games, 53,714 candidate gameplay rows and 53,300 candidate choice rows. The gate appears in 60 matches, 113 games and 287 decisions (128 Bolt, 159 Dart). It occurs in 20 of the parent's 133 losses and 40 of its 123 wins.

| Candidate deck | Matches | Matches with gate | Losses with gate | Losses without gate |
| --- | ---: | ---: | ---: | ---: |
| Affinity | 32 | 0 | 0 | 17 |
| Burn | 32 | 32 | 19 | 0 |
| Elves | 32 | 0 | 0 | 13 |
| Faeries | 32 | 0 | 0 | 20 |
| Gates | 32 | 0 | 0 | 30 |
| Rally | 32 | 28 | 1 | 1 |
| Terror | 32 | 0 | 0 | 9 |
| Wildfire | 32 | 0 | 0 | 23 |

With exact baseline behavior outside the gate and the shared RNG and auxiliary-policy conditions above, at most 20 additional wins are possible on this finite panel: a ceiling of 143/256, or +7.8125 percentage points. This is an opportunity bound, not expected improvement. All 40 eligible wins could regress; an eligible loss need not be repairable. Physical seats have 12 and 8 eligible losses respectively. The fixed eight-deck mixture, two seeds per ordered matchup/seat and candidate-first game-one opening limit generalization. No new model, match or training update was produced.

Park the router as the primary research direction. Its taught domain leaves six decks and 113 baseline losses untouched on these paths. A narrow later mechanism test remains possible, but it cannot carry the across-meta objective. Before extending imitation, investigate whether ordinary positions admit terminal-grounded labels across more action families. Do not use a value drop or model ranking as an expert label.

Evidence: E:/mtg-meta-recovery-20260921/target-route-dispatch-001/{completion.json,analysis.json,matches.csv}. The completion receipt binds the manifest, throughput choice and full 256-row result; the analysis binds the complete original 512-match outcome report. All eight timing placements produced identical outputs on 16 fixed representative pairs. The maintainer E: with eight workers was fastest among those measured: 2.3588 seconds for the timing workload, versus 4.0233 serial, 6.3266 on the compute host with four workers, and 2.6461 combined before setup. The full read took 4.3027 seconds, with 31.7723 seconds of qualification and 2.2860 seconds of new remote staging reported separately. Previously staged immutable inputs were hash-checked and reused; their old transfer cost was not charged again. Fresh RunPod inventory returned HTTP403, with no allocation or verified availability.

The guarded target-route-dispatch.py and worker require matching source/input pins, exact qualification output agreement, selected host/worker assignments and fresh owner checks. Summed child CPU time was 30.4531 seconds and process read counters totaled 681,469,103 bytes. The maximum start/end per-process RSS sample was 57,823,232 bytes, not peak or aggregate memory. The 37.7403-second forecast linearly scaled the small qualification workload, including startup, and is not a confidence bound or proof of a global allocation optimum. Old evidence and source were unchanged.

Fable's review remains unavailable under the known zero-read quota failure until September 22 at 07:00 EDT. This source audit and bounded read-only census proceed under the maintainer's authority, with no completed independent endorsement. A consequential routed-agent or learning experiment still needs a concrete design, mechanical verification, matched controls, fresh frozen outcomes and independent review when available. Broader teaching coverage and on-policy distribution shift remain alternative research directions. Human and league competitiveness are still unproven; CP7 remains excluded.
