# Native matched terminal credit objective v1

## Question

When initialization, data, architecture, optimizer, and terminal reward are
fixed, does GAE produce a better held-out policy-head update than Monte Carlo?

This is an offline objective screen. Natural terminal win, draw, or loss is the
only reward, and every nonterminal reward is zero. Offline surrogate results are
not playing-strength evidence.

## Fixed experiment

- Corpus: separate Pool3 cache SHA-256
  `287d509794658bc167a7b61be450fa894d38ad837e7e6b212d49947629d542c6`,
  base seed `1910001`, 1,024 seat-swapped pairs and 2,048 natural games.
- Split: fit where `pair_index mod 4 != 3`; held out where it equals `3`.
- Initial policy and representation: policy-only structured successor state
  SHA-256
  `ff2abf50e8760780a9331e53aa7323cb96e3c64edb6e7d89062dbe38bf6a5cc0`.
- Frozen value estimator: bounded width-48 state SHA-256
  `cae8e19ef825325508de351b883b2df3863dc66f0288be06ad2ccf868e3d7d7c`.
- Trainable tensor: the same 48 policy-head weights in both arms. Every other
  tensor, including policy-head bias and value parameters, is frozen.
- Monte Carlo arm: `A_t = R_terminal - V(s_t)`.
- GAE arm: `gamma = 1`, `lambda = 0.95`, terminal value zero, and zero reward
  before the natural terminal.
- Both arms: seat-wise fit-split standardization, equal episode and physical-
  decision mass, joint-ratio PPO clip `0.10`, five epochs, batch size `64`,
  AdamW learning rate `3e-4`, weight decay `1e-4`, gradient cap `5`, and seed
  `20260812`.
- Common held-out score: Monte Carlo terminal advantage from the same frozen
  width-48 value model. This prevents each arm from grading its own estimator.

A 64-pair run is the bounded throughput preflight and cannot alter settings.

## Gate

Advance GAE to one full-corpus fit and fresh native win-count gate only if both
arms stay within mean TV `0.03`, p90 TV `0.10`, and maximum physical-decision
joint log ratio `0.50`; GAE's common held-out terminal surrogate is positive
overall and at both seats; GAE exceeds Monte Carlo overall and at both seats;
and the pair-bootstrap 5th percentile of GAE-minus-Monte-Carlo surrogate is
above zero using 4,096 fixed resamples.

A failure closes this exact GAE head-only objective. A pass still provides no
strength claim. The subsequent fresh live gate must use an integer terminal-win
criterion, with no hand-coded rewards or proxy promotion measure.
