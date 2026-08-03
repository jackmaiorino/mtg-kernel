# Native CP7-opponent history-value search v1

Status: development screen, not run

## Question

Did depth-8 search fail because it simulated XMage CP7 turns with the candidate's own policy?

## Fixed components

- Candidate, parent, eligibility, depth, four information-set samples, 0.25 override margin, and candidate-seat terminal-or-value bootstrap are identical to `native_history_value_depth8_search_v1`.
- The candidate model still selects candidate continuation actions by deterministic argmax and supplies every nonterminal horizon value.
- Terminal win or loss remains the only reward and promotion measure.

## Changed mechanism

On opponent-controlled continuation decisions only, load the verified derivative at `D:\mtg-kernel-cp7-bc-train-base970001-grid-strict-v1` and sample from its CP7 behavior-clone logits. The clone was selected on held-out CP7 imitation NLL, not play strength. Its value head is never used.

The fixed clone has manifest SHA-256 `6ba733fead0d36c26cd24630245fa6f2a1216ae60c73f46d45e83b4cc714676c`, payload SHA-256 `de1132f6b8b55975154133b91a2f2ea90bc1159676a041057fd827e728eca4e1`, and model-parameter SHA-256 `3f4da9d761771cf0d7cfe2da19b52dd93dd0bc59466d92318cc11fc850d8c4dc`. Its selected held-out mean NLL is 1.5390 and substep top-1 accuracy is 70.21 percent.

Opponent samples use deterministic common-random-number seeds shared across counterfactual root actions for the same episode, root, hidden-state sample, and continuation index. This changes the opponent continuation model rather than search depth or override threshold.

## Development screen

Replay the already revealed base-1400001 eight-pair block. This is implementation and mechanism evidence only, not fresh strength evidence. Require:

- all pairs complete with no sample or continuation-contract violations;
- at least one override from each seat;
- at least 10 percent of roots shared with the self-model depth-8 run choose a different final action;
- exploratory paired gains are not below losses.

If these checks fail, retire this exact opponent model without spending fresh seeds.

## Fresh gate if development passes

- Base seed: 1420001.
- Target: 8 mutually successful matched pairs, 16 games.
- Maximum: 32 attempted pairs.
- Batch size: 4 pairs.

Pass only if gains are at least losses plus 2, both seat nets are at least -1, both seats contain an override, and all information-set and opponent-policy diagnostics are valid. A pass authorizes a 16-pair extension at base seed 1430001. A fail retires this exact CP7-clone opponent continuation mechanism.
