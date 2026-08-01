# Native rollout-teacher ceiling v1

## Question

Can full-terminal counterfactual continuations identify stable, useful action corrections for the retained Rally policy before we invest in an information-set-safe teacher or training path?

This is a perfect-information ceiling diagnostic. Each branch holds the actual opponent hand and future library order fixed. Its labels are not admissible for training.

## Fixed design

- Source: retained outcome checkpoint manifest `706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb`, Adam step 1.
- Roots: 32 deterministic Rally mirror states, at most one per episode.
- Eligibility: surface decision, one substep, physical decision at least 10, and 2 through 8 legal actions.
- Ranking: four common-random-number retained-policy continuations per legal action.
- Confirmation: 32 fresh paired continuations for the teacher choice and retained-policy argmax.
- Horizon: 512 policy steps including the forced root action.
- Continuation policy: retained checkpoint for both seats, sampled with `f32-q8-expq63-hamilton-splitmix64-v1`.
- Gate: byte-identical rerun, zero failures or incomplete pairs, all ranking continuations natural, at least 99% natural overall, runtime below ten minutes, at least six changed roots, positive changed roots outnumber negative roots, and mean confirmed reward delta at least `+0.05`.

## Reproducible result

Both formal reports are 160,074 bytes and byte-identical:

- `D:\mtg-kernel-rollout-teacher-ceiling-v1\formal1.json`
- `D:\mtg-kernel-rollout-teacher-ceiling-v1\formal2.json`
- SHA-256: `b86e04b68894c23563e2d2cff31ad8149656ccb784f6a45d9fe8f804dd3eeaf6`
- Runtime: 103.947 seconds and 103.633 seconds.
- Release executable SHA-256: `37e0bd5a92d8c8a3de19dab9f8d4834290ef0a8eee2bb26dcb157ad8d2f0a49f`.
- Source parent commit: `63517571834b0c88f14e74b61cd341f6b54cb565`.
- Toolchain: `rustc 1.94.1 (e408947bf 2026-03-25)`, LLVM 21.1.8, Cargo 1.94.1, and `cc (Ubuntu 11.4.0-1ubuntu1~22.04) 11.4.0`.

The exact retained source identities were:

| Field | SHA-256 |
| --- | --- |
| Manifest | `706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb` |
| Payload | `eb83be33bcb7418b6f85ec9687da4b7ca5620a1df64721a1942d2793588bbd3c` |
| Native state | `2c55a13abb3157f3f4ba012af663ffa56599c5d6cb90743c1ba6e024ca47a9c8` |
| Model parameters | `883e4882d01d9cb55ecd7a4ae00e3c95793b6147baf3df08650ef1fa7f8e9546` |

Results:

| Metric | Result |
| --- | ---: |
| Roots | 32 |
| Teacher choices different from parent argmax | 8 |
| Positive changed roots | 7 |
| Negative changed roots | 0 |
| Zero-delta changed roots | 1 |
| Confirmed teacher-minus-parent reward sum | 204 / 1,024 |
| Mean confirmed reward delta | +0.19921875 |
| Natural completion | 100% |
| Incomplete ranking actions | 0 |
| Incomplete confirmation pairs | 0 |
| Branch failures | 0 |
| Same-action paired mismatches | 0 |

The diagnostic passed every declared gate.

## Decision

Proceed to a Rally-only acting-player information-set redeterminizer, then rerun the teacher over multiple hidden-state samples. Do not train from the v1 report. A safe redeterminizer must preserve the acting player's observation, legal action semantics, flat binding, known hand cards, known library positions, public counts, per-owner card multisets, and environment RNG while resampling only actor-unknown hidden slots.

This result demonstrates a promising decision-level supervision signal inside one native Rally mirror. It does not measure XMage or CP7 strength, cross-deck generality, human strength, or pro-level play.
