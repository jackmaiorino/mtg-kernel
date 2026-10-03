# Wide-decision repair and continuation004

The five-deck CPU training continuation is running from source `4289238463686a8c3707e4fcb9c311969f0c6555`. Before the main launch,125updates/1,250trained games were complete, including the repaired full-batch calibration. The full Multi-Deck BO3 Campaign remains active; this run is preboard player development, not terminal BO3 or sideboard-return training.

Continuation003 stopped at original batch124 because a real120-action decision exceeded the frozen64-action sampler. Full Adam127 and the124completed updates remained intact. The new V3 wrapper delegates widths1..64 to the unchanged sampler and supports wider menus through65,536 with the same exact integer sampling calculation. It uses bounded heap scratch, usize indices and deterministic remainder/index ordering. Wider trajectory rows carry a required runtime identity; imported ancestry, narrow trajectory bytes, RNG streams and legacy callers remain unchanged. The bound is not a claim that every conceivable engine menu is supported.

| Verification | Result |
| --- | --- |
| Targeted sampler, policy and training/replay checks |24passed; one existing artifact-dependent policy test ignored |
| Frozen sampler references, contract and allocation checks |9passed |
| Release build |200.250seconds, CPU, four Cargo jobs, BelowNormal |
| Failed124 game and exact replay |Both natural completions, identical trajectory SHA `457e02364ecda6cf7dab67b464b2cb0144aab7344675afe997b97188d939fb9c` |
| Previously repaired116,34 and original control |All original trajectory hashes preserved |
| Complete repaired training batch |10games/800learner physical decisions, Adam127to128,16.649seconds |

The repaired failed game contains725decisions, exactly one of width120 with the wide sampler identity. This is actual gameplay/replay evidence. Synthetic sampler tests separately cover65,120,256,257,5,040 and65,536, including index65,535, exact mass, ties, invalid inputs, scratch growth/shrink, seat RNG and receipt tampering. Debug unit wrappers record parent77d6c559 because the tested source was uncommitted at that point; reviewed file hashes match the bytes committed as42892384. Frozen integration checks and release use42892384.

Evidence root: `E:/mtg-kernel-learned-sideboarding-evidence/overnight-development-001`. Start with [repair result](E:/mtg-kernel-learned-sideboarding-evidence/overnight-development-001/failure-iteration124-001/RESULT.md), [independent review](E:/mtg-kernel-learned-sideboarding-evidence/overnight-development-001/failure-iteration124-001/REVIEW.md), and [continuation](E:/mtg-kernel-learned-sideboarding-evidence/overnight-development-001/continuation-004/README.md).

Continuation004 preserves original batches124..479 and all3,560episode payloads, including exact archived opponent mappings across all three previous segments. It resumes full Adam127, SHA `13bca62bbcfd33c73e48cc5f0f0c12ac0223e5fa26f774a07d581cbeba5f41af`. The successful calibration checkpoint at `run-004/iterations/000000/attempt-000000/update/checkpoint.json` is full Adam128, SHA `80c56ad746aea2d70ddf3cfaf40b8f15f61e61ddffc13c6b1df93deeddb1142e`. The main invocation resumes this same run for355additional updates. No partial batch was counted or used to reset the optimizer.

Main controller37208 and native child9156 launched at12:28:51UTC September13. These are historical launch identifiers; revalidate command/creation/parent identity before reporting live status or taking process action. Current paths are `continuation-004/controller-main.json`, `launch-main-001/progress.jsonl`, `launch-main-001/result.json` if terminal, and contiguous `run-004/iterations/*/complete.json`. Add124prior updates to the actual continuation count; calibration is already included. One persistent BelowNormal CPU child,8GiBmemory, shared original8h native-active/100GBoutput caps and16GiBreserves remain. Launch used1,615.595cumulative native seconds and10.035GBparent storage. No GPU, new paid allocation, automatic restart or promotion.

The two-hour read-only automation points to continuation004 and pauses on terminal completion/failure. Existing native-human001/002 packages and live sessions were not changed; their engines predate this sampler repair. A new human executable is sealed with the training build but has not been installed into a human package or substituted into a live game.

Fable session `feff262e-3f28-4ab9-9967-98021438617b` failed weeklyHTTP429 before any source read or feedback. Independent Codex source/mapping review and root execution checks completed within Jack's existing authorization. There is no Fable endorsement. No reset or purchase occurred.

Final model selection remains the checkpoint after exactly480updates, independent of training wins. The subsequent matched BO3 anchor comparison has an offline draft under `E:/mtg-kernel-learned-sideboarding-evidence/bo3-post480-preparation-001`; its numerical gates, cost limits, final model/binary pins and final design review remain before measurement. Jack's existing local campaign authorization continues to apply; this preparation does not create another user-approval step. Keep/Keep isolates player improvement; useful sideboarding requires a later fixed-player comparison. Broader opponents, forgetting, human meta alignment, actual Jack games, production/store/search reconciliation and brewing remain. CP7 outcomes remain excluded from selection.
