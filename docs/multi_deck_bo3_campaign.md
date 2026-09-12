# Multi-Deck BO3 Campaign

Jack's active goal is to complete the Multi-Deck BO3 Campaign. Its scope is broader deck competence plus learned sideboarding, evaluated through self-play, human matchup calibration and games against Jack. Brewing remains the longer-term end goal this campaign supports. Completion of a repair or an engineering pilot does not complete this campaign.

| Requirement | Current evidence | Remaining deliverable |
| --- | --- | --- |
| Reliable supported BO3 play | The first 400-match pilot stopped after 30 matches on a missing Ward payment observation | Versioned Ward binding, meaningful rules/projection/feature checks, failed-case replay and complete fresh pilot |
| Broader trained player in BO3 | CPU training/update/reload exists separately; BO3 uses the old frozen player | Strict successor checkpoint loader and actual current parameter/embedding identity in BO3 |
| Explicit deck registrations | CPU collection accepts supported 60/15 registrations; BO3 constructs checked-in lists | Validated explicit registered 75s through the full match, preserving registration versus postboard configuration identity |
| Learned sideboarding evaluated on outcomes | Fresh imitation fit is legal but matches 0/10 held-out movement plans | Terminal BO3 comparisons, an evidence-driven response to the demonstrated learning limitations, and a bounded report of benefit or lack of benefit; no strength claim from imitation or engineering checks |
| Meaningful recursive improvement | League/regression/confirmation roles are proposed | Broader-deck training with a diverse policy archive, matched candidate/incumbent evaluation, forgetting checks and independent selected-candidate confirmation |
| Human matchup calibration | Goal recorded; no human dataset selected | Aligned human/agent/difference matchup matrices with counts, uncertainty and the same opposition weights |
| Games against Jack | Proposed external evaluation | Verified human-facing interface, frozen package and fixed decklists, balanced play/draw, recorded games; separate feedback games from later evaluation |
| Complete-agent integration toward brewing | CPU successor, production and search seams are documented separately | Reconcile player/head compatibility and the production CPU/GPU/store/search paths under explicit identities, then evaluate the resulting package |

Current execution starts with the Ward implementation in the assigned worktree `E:/mtg-kernel-learned-sideboarding-codex`. Preserve existing fits, original interrupted pilot and the completed native screen. No native confirmation or new paid/GPU allocation is launched by this goal continuation. CPU work remains BelowNormal with at most four build jobs, one cargo writer, and target/temp/output storage on E.

Numerical playing-strength gates, training budgets and a human data cohort must be specified for the associated future measurements. Do not silently redefine those decisions from engineering-pilot outcomes. CP7 outcome information remains excluded from experiment/model selection. Human rates are an external diagnostic rather than a policy-loss target; stronger play and brewing may justify departures from the baseline. Human participation cannot be inferred from a prepared interface or simulated opponent.

See [Ward prerequisite](learned_sideboarding_engineering_005_20260912.md), [production and brewing sequence](learned_sideboard_production_brewing_plan_v1.md), and [evaluation design discussion](E:/mtg-kernel-learned-sideboarding-evidence/evaluation-breadth-20260912/NOTE.md). These documents describe remaining work, not campaign completion.
