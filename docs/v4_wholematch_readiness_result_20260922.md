# V4 full-match readiness: NO-ADVANCE

2026-09-22. The frozen E controller is not ready for unrestricted whole-match use. It completed7of16 assigned consumed matches naturally and explicitly aborted9 at decision-local library contexts. The diagnostic execution is complete, not the playing-readiness objective. Preserve g115 as reference, M1 unmet. No strength gate or lineage promotion.

| Controller | Assigned | Natural BO3 | Wins | Losses | Typed aborts | Infrastructure failures |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Ordinary g115 | 16 | 16 | 10 | 6 | 0 | 0 |
| g115 E | 16 | 7 | 4 | 3 | 9 | 0 |

Wins and losses count only naturally completed matches. Among the seven jointly natural pairs, baseline5wins/2losses and E4wins/3losses: one win-to-loss, zero loss-to-win. The nine aborts are neither losses nor omitted samples. Conditioning on E completion selects a subset, so4/7 versus5/7 is not a full-population strength comparison. n16selected consumed identities,0trainingreplicas, one checkpoint; no identifiable effect against between-training SD0.76pp or sensitivity1.39pp, no nonregression inference. No future experiment or model may be selected using CP7.

| Frozen match | Candidate deck/seat | Baseline | E |
| --- | --- | --- | --- |
| cell07-r0-s0-g115 | Affinity P0 | Win | Loss |
| cell01-r1-s1-g115 | Affinity P1 | Loss | Loss |
| cell12-r1-s0-g115 | Burn P0 | Win | Win |
| cell11-r0-s1-g115 | Burn P1 | Win | Win |
| cell19-r1-s0-g115 | Elves P0 | Win | Abort game1 step79 |
| cell20-r1-s1-g115 | Elves P1 | Win | Abort game1 step59 |
| cell27-r0-s0-g115 | Faeries P0 | Win | Win |
| cell29-r1-s1-g115 | Faeries P1 | Loss | Loss |
| cell38-r1-s0-g115 | Gates P0 | Loss | Abort game1 step38 |
| cell38-r0-s1-g115 | Gates P1 | Loss | Abort game1 step38 |
| cell42-r1-s0-g115 | Rally P0 | Win | Abort game1 step136 |
| cell43-r0-s1-g115 | Rally P1 | Win | Win |
| cell52-r1-s0-g115 | Terror P0 | Win | Abort game1 step364 |
| cell51-r0-s1-g115 | Terror P1 | Loss | Abort game1 step53 |
| cell57-r0-s0-g115 | Wildfire P0 | Win | Abort game1 step123 |
| cell63-r0-s1-g115 | Wildfire P1 | Loss | Abort game1 step103 |

Every abort is State/Redeterminize/DecisionLocalLibrary, on the candidate actor. Source rl_session/flat_action_v4/search_state.rs:36 intentionally rejects this visible extension. The prior state-boundary design explains why: library-choice candidates can be visible without positional library-knowledge entries, and naive resampling could relabel them. This is a demonstrated operational coverage limitation, not proof the engine is wrong or that hidden-information search cannot work. No fallback, guard removal or parameter change occurred. Deck, opponent and seat are confounded within each selected stratum, so the table does not estimate archetype effects.

Evidence base B=E:/mtg-meta-recovery-20260921. B/v4-wholematch-summary-001/summary.json revalidates all16results against exact request/archive/package/option/receipt pins and controller semantics. Source inputs and output digests remain in the recorded chain. All16ordinary replays match archives exactly. B/v4-wholematch-run-001/completion.json is complete, handle73526 exited0; summary helper handle29331 exited0. The designated first serial qualification eight plus the remaining eight form the16observations. Parallel repetitions and the initial timing probe are not additional samples. B/summarize_v4_wholematch.py records its own pin in the summary. Frozen source ddd24baf and binary SHA b8bc325fbfc5c5e4b92f3c7c0dd2d11e5839bc17cbaed37faa56adc289c155fa; g115 checkpoint88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1. S128/T1024/depth8/search seed20260922, lowest-index E tie, ordinary opponent, native BO3 chooser coupling, zero retained full trees, GPU ordinal none.

Compute: qualification1/2/4/8workers took425.844/214.125/131.913/134.635seconds with exact semantic agreement;4selected. Total qualification919.225seconds, including repeated engineering work. Remaining-eight phase318.363seconds; run total348.277seconds including admission/full-output revalidation. Selected16 E native times sum1080.332seconds, longest300.400seconds, all below fixed600second ceiling. There were2725committed E search decisions. Remaining-phase median system CPU11.9%, max32.1%, minimum available RAM92,788,871,168bytes. The tail had two then one sequential match and no queued independent work. No extra workloads were invented for utilization. This small heterogeneous eight-job benchmark does not prove scaling for a larger formal campaign. Fresh placements preserved local reservations, Haley remained SSH-unreachable, RunPod read403/noallocation/no paid authority. No GPU, credential or guard changes.

Literature was posted and committed before launch in collab/lit/20260922-v4-wholematch-readiness.md. The design, exact frame and power nonclaims remain in v4_wholematch_readiness_design_20260922.md. Failed/aborted outputs and all qualification repetitions are preserved. This is the same-day committed NO-ADVANCE record.

Proposed next direction, not finalized: address the demonstrated library-choice information boundary before any broader strength spending. Audit how visible candidate cards and their identities relate to sampler constraints, consider an additive version that preserves the actual actor-visible choice contract, and reject designs that leak hidden order or silently fall back. No current measurement will be repaired in place or reclassified. Independent result/direction review pending; no implementation or new evaluation has begun. Formal measurement/seed and M1-scoping authority remain as stated by GUIDANCE and FOR-JACK.
