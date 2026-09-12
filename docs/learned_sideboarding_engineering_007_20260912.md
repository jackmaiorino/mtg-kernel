# Population training and per-seat BO3

The Multi-Deck BO3 Campaign remains active. This continuation supplies distinct learner/opponent routing and resumable CPU iteration execution, preserving the measured engineering-006 player/head and all earlier native/sideboard evidence. Brewing remains a primary end goal through explicit registered and selected lists.

## Implementation and review

`ExpandedEpisodeV1.opponent` optionally names a separately pinned model source. Explicit opponents produce trajectory schema v2 with ordered per-seat sources and actual model/state identities. Each physical actor consumes its own RNG stream. Update replays both players against their actual models, but only learner decisions enter the unchanged terminal-return policy/value update. Omitting the opponent retains the historical self-play trajectory shape.

`native_expanded_training_run_v1` binds a complete iteration schedule and named opponent roster. Assignments may select the current learner, initial learner, fixed archive member, or an already completed iteration. Every update requires fresh trajectories for the exact learner state. The CPU coordinator publishes immutable collection, update, checkpoint and iteration records, validates a contiguous completed prefix on resume, and uses an OS file lock for single-writer ownership. A bounded invocation can stop after a chosen number of new completed iterations without changing the schedule.

A checkpoint without its completed update receipt is never adopted as the next learner. Recovery reuses a completed collection and retries from the same predecessor in a fresh attempt. Completed updates require exact source, trajectory, full-state and one-step Adam continuity. Orphan staging files are retained while receipt publication uses fresh staging names. Independent Codex review identified the staging restart failure and a public BO3 V3-identity gap; both were corrected. The [source review](E:/mtg-kernel-learned-sideboarding-evidence/engineering-007/SOURCE-REVIEW.md) records dispositions.

`run_population_batch` loads two strict V3 model sources and binds each sideboard head to its own physical seat's actual weights and embedding table. The router passes only the acting seat's public input. Population results record both models explicitly; no synthetic pair digest is labeled as a single model's weight hash. Existing CLI modes and legacy match serialization remain unchanged.

## Verification

Forty focused tests passed: 19 library tests, 14 CLI/evaluator tests, and seven live-registration configuration tests. These cover actor/RNG routing, wrong model/head bindings, replayed opponent outputs, learner-only gradients, stale learner rejection, schedule references, exact legacy serialization, OS lock lifetime and stale staging recovery. Some tensor fixtures use synthetic terminal containers to test grouping and gradients; these are not played-game results.

Live verification is prepared under [engineering-007](E:/mtg-kernel-learned-sideboarding-evidence/engineering-007). The bounded plan uses two iterations and four episodes across Affinity/Elves and Terror/Rally, with both learner seats. Iteration zero uses the original warm-start player as a fixed opponent; iteration one uses the initial successor and completed iteration zero. In a two-iteration check, the latter is also the current learner, so it exercises history resolution rather than an older distinct opponent. Planned checks include a pause/resume boundary, an orphan checkpoint with no update receipt, exact reference state comparison, idempotent completed-run resume, distinct-player BO3 and one exact replay.

No large training campaign, new GPU or paid allocation, win-rate gate, model promotion or strength claim follows from these checks. Future breadth/learning comparisons need a separate fixed design with matched roots and independent confirmation. Human meta rates remain an external diagnostic, not a training target; CP7 outcomes remain excluded from selection.

Fresh Fable review `9583d514-4f80-4ddd-9619-815c934b3831` failed weekly HTTP429 with zero source reads or substantive tokens. Root continued already assigned CPU engineering work with independent Codex review and focused tests, recording the unavailable Fable consultation rather than claiming endorsement. No quota reset or purchase occurred.
