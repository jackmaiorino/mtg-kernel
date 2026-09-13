# Phase 1: Pauper meta competence with RunPod training bursts

Accepted by Jack for implementation on September 13, 2026. Infrastructure implementation and engineering tests are active. The plan explicitly leaves the campaign paused, so campaign training, formal measurements and paid allocation await clarification of launch scope. Work is incomplete; progress is tracked in the external Phase 1 status note.

## Objective and delivery boundaries

Build a complete BO3 agent covering at least 90% of a pinned MTGO competitive field. Deliver an internally qualified agent plus 24 BO3 feedback matches against Jack first. Phase 1 remains open until broader human or qualified MTGO evidence supports competitive regular-human play. Expert mastery, unrestricted brewing and unfamiliar-archetype generalization are later phases.

| Milestone | Deliverable |
| --- | --- |
| A | Close the frozen five-deck comparison and conditional confirmation; fit a compatible final-player sideboard head and separately measure its BO3 benefit. |
| B | Qualify current Linux training/evaluation, parallel collection, recovery, useful throughput and RunPod lifecycle accounting. |
| C | Freeze eight complete weeks of MTGO competitive registrations, preserving missing rows and grouping exact-list train/development/confirmation splits. Select actual supported registrations covering at least 90%. |
| D | Implement required cards, observations, interactions and registration sizes; preserve learned parameters and optimizer state through explicit registry transfers. |
| E | Train broad preboard/postboard competence with historical, independent and specialist opponents. |
| F | Learn opening and sideboard decisions on match outcomes; combine useful sampling/search changes through controlled comparisons. |
| G | Pass independent internal confirmation; deliver the same agent in the human interface and play 24 feedback BO3s against Jack. |
| H | Qualify the approved MTGO adapter, rehearse complete matches, run a separately funded 40-match pilot, then prepare the broader frozen external confirmation. |

Archetype names or selected winning lists alone do not establish field coverage. Current engineering contains five original lists, not the selected current MTGO field. Keep the original five-deck source, model, seed maps, numeric gates and analysis sealed.

## Training and interfaces

Extend V3/Net8 with a pinned meta snapshot, compatible complete-agent package, actor-visible BO3 training trajectory, explicit full-state registry transfer and local/cloud supervisor. Preserve existing serialized defaults and old loaders. New learning semantics use a versioned path.

Breadth warmup starts with ten fresh games per update and the existing optimizer settings. Learner-deck mixture is half field-weighted and half uniform, with balanced seats/initial roles. Progress from preboard to half preboard/half legal postboard play. Add two independently initialized training lineages; snapshots from the current lineage are not independent. Planned blocks contain max(200, 20 * archetype_count) updates. Three blocks without meaningful development improvement require diagnosis before further scaling.

New match-return learning weights complete matches equally and alternates frozen gameplay/auxiliary-policy blocks. Preserve the existing game-terminal objective for warmup. Opening and sideboard policies receive the player's own deck information and only legitimate opponent evidence. Parallel collection preserves batch membership/order and finishes before one update; no stale next-batch trajectories.

## Acceptance and budgets

Verify actual rules/visible observations, both-seat openings, sideboarding, natural three-game credit assignment, incomplete-match rejection, no gradient to frozen components, full-state transfer, replay/recovery, worker-count invariance where promised, and actual GPU numerical/continuation behavior before GPU training. Existing CPU reference remains authoritative until a new backend is qualified.

New internal formal comparisons default to 4,096 paired cases per candidate archetype, at least +2 percentage points overall with a paired 95% interval above zero, and simultaneous per-archetype bounds above -5 points versus the incumbent. Reserve opponent policies, variants and seeds for confirmation. These do not replace the older frozen five-deck gates.

Jack's 24 matches are development feedback. The later external default is 400 frozen BO3s, one-sided 95% lower confidence bound above 40% match score with repeated-opponent/session dependence addressed. Keep League/Challenge results separate and all starts/failures visible. Inadequate evidence leaves Phase 1 open.

RunPod's first qualification burst costs at most $10 including storage and recovery, inside the existing $200 scope. Subsequent stages require a measured cost/funding notice. Funding forecasts include full work, export/shutdown and 20% contingency. Refresh balances during runs and notify before remaining credit falls below six hours at current burn or the remaining stage if shorter. Apply the mandatory compute policy in AGENTS.md; count actual completed work, not artificial utilization.

## Current evidence and review

Starting state: 480 updates/4,800 preboard games complete at Adam483; 200 calibration BO3s plus byte-identical replay complete; full comparison and confirmation unrun. Root verified the assigned worktree was clean at 96af84ec4700899d1b06508f0da211706b6bd3cd before implementation.

Fresh Fable plan review 2d5aa122-1f76-494d-93b9-a245d26c5150 failed weekly HTTP429 with zero source reads and no substantive feedback. Independent source audits identified serial collection/update and the older MTGO adapter seams. Accepted dispositions: preserve fresh ordered data; qualify cloud training rather than just evaluation; retain broad human proof as pending; reuse existing Daybreak authority without weakening exact visible-information boundaries. This is not Fable endorsement.
