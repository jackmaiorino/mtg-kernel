# Public feature successor: engineering qualification

The corrected runtime exposes active Strands prevention, but the existing model has no dedicated global prevention-color inputs. Visible card rows also lack explicit printed mana costs. This successor tests an additive representation without changing frozen V4/Python V7 identities or the reference checkpoint.

Implemented in this isolated worktree:

- Six public state fields: five active prevention colors and a separate damage-cannot-be-prevented flag.
- Thirty-two fields for each already-visible card token: known card, cost presence, front-face mana value, generic/X counts, colored/phyrexian pips and unordered hybrid pairs. These are printed costs, not actual payment or alternate costs. Hidden hands and libraries never supply tokens.
- Independent Rust compiled-cost and Python printed-cost encoders with a separate versioned contract.
- A Python research model that adds two zero-initialized input projections. Existing parameter names, shapes and accumulation order remain intact. Imported Adam ages remain 32400; the two new matrices start at age zero.

Evidence: `E:/mtg-postboard-campaign-20260920/public-feature-qualification-001`.

Native catalogue and actual V4 fixture tests passed. Twelve fixtures cover both seats, three shield configurations and two hidden-hand/library/allocation variants. The Python qualification in `python-002/qualification.json` passed all 192 catalogue rows, all thirteen base arrays and auxiliary rows. Imported parameters and both Adam moments are bit-identical to g115. Initial Python reference/successor logits and values are bit-identical. Both new matrices receive nonzero gradients; two synthetic updates reproduce parameters and Adam after save/resume. Qualification took 3.42 seconds on torch 2.13.0+cpu.

The earlier `python-001` qualification stopped before checkpoint transport because raw native action semantics lacked the Python legal-action envelope. The runner now constructs explicitly diagnostic operational wrappers; all native/Python tensor comparisons pass. No frozen encoder was modified and no measurement was rerun. Preserve both roots. A subsequent boundary check rejects i64::MIN tokens without arithmetic overflow.

No terminal-reward training, native successor scoring/update integration, CUDA optimizer parity, stronger candidate or human-strength result follows from these tests. The synthetic loss is only a gradient-connectivity probe. Existing human delivery and g115 checkpoint remain unchanged.

Next: implement explicit native successor scoring and checkpoint/optimizer support, qualify CPU/CUDA numerical behavior and actual rollout replay, then freeze a small matched learning comparison. Both arms must use the corrected observation runtime, matched seeds and terminal rewards. Keep canonical retention checks and independent opposition; do not select from CP7 outcomes.

Fable's September 19 consultation failed HTTP429 before source reads, with reset September 22 at 07:00 EDT. No retry or endorsement is implied. Bounded local work proceeds under Jack's explicit authority with this review gap unresolved. No paid compute or broad training campaign is launched.
