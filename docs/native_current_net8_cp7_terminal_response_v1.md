# Current Net8 CP7 terminal response v1

Status: Fable countersigned for implementation preflight; amendments folded
before the fixed update.

## Question

Can one fixed terminal-only update of retained GAE8 on its own XMage CP7
skill-7 trajectories produce a fresh versus-CP7 gain without a material loss
on the native Pool3 distribution?

Training against CP7 makes CP7 skill 7 a training distribution. A pass is
therefore versus-CP7 skill evidence only, not general strength or professional
evidence. CP7 skill 8 at fresh base seed `1850001` is reserved as an external
reference and is not touched in this experiment.

## Fixed update

- Initial state: exact retained GAE8 native state
  `ab7dd25ca6619a4a613ca089e1eb8e75981f8e5cfc0bae8535b78cddd7efa952`,
  payload
  `a0b7752181a562f8e5a0821a490ce20b777b509855d754283536e8242f489b98`,
  Adam step `520`.
- Corpus: all 64 matched pairs and 128 natural games from the completed GAE8
  CP7 skill-7 anchor at base seed `1820001`. The merged 4,769-decision JSONL
  SHA-256 is
  `fe95949e852227259efda060889c2ea707033f77b919f6100f42f5feeef754b4`.
  The observed 48 wins and 80 losses are already revealed development data.
- Reward: natural terminal win, draw, or loss only. Search scores, labels, and
  intermediate rewards are absent.
- Advantage: terminal return minus the frozen source value, standardized by
  candidate seat over the complete corpus, with equal episode mass and equal
  physical-decision mass within each episode.
- Objective: physical-decision joint-ratio PPO, clip `0.10`, four full-batch
  epochs, learning rate `0.001`, value coefficient `0.5`, unchanged Adam
  state, and full-network updates. Every epoch computes the ratio and clip
  against the fixed step-520 GAE8 behavior policy; `pi_old` is not rolled
  forward between epochs. The active value loss targets the same natural
  terminal result. No epoch, coefficient, checkpoint, or hyperparameter is
  selected from this corpus.

The candidate is publishable only if every tensor is finite, Adam ends at
step `524`, parameter L2 movement from GAE8 is at most `0.75`, mean policy TV
on the training corpus is in `[0.010, 0.050]`, p90 TV is at most `0.150`, and
maximum absolute selected physical-decision log ratio is at most `1.0`.
Failure stops this exact update before any fresh gameplay.

The `1.0` final selected-log-ratio cap is intentionally looser than the
`0.49` to `0.50` caps in the DAgger and search-teacher sheets. This update also
has a fixed-parent PPO clip of `0.10` at every epoch, while those supervised
trainers lacked that complementary clipped objective. The final cap still
stops cumulative drift that the per-update objective does not itself bound.

The `0.010` activity floor addresses the repeated absorption failure under
tighter envelopes. The historical Pool3 planning range of 1.46 to 2.91 times
mean TV suggests roughly 1.5 to 2.9 percent discordance at that floor. On the
256-game fresh CP7 panel below, that is about 4 to 7 discordant outcomes, so a
`+4` gate is arithmetically feasible but remains a noisy rapid selector. The
range is a cross-mechanism planning heuristic, not a bound. At exactly `D=4`,
the one-sided null probability of `+4` is `1/16 = 6.25%`; at `D=6`, it is
`7/64 = 10.94%`. Parity makes the exact small-`D` probability non-monotonic.
This is feasibility arithmetic at the movement floor, not an expected
discordance forecast. At 10 percent discordance, `D` is about 26, null SD is
about `sqrt(26) = 5.1`, and `+4` is only about 0.8 SD. For context, the
depth-8 live v3 screen saw roughly 31 percent discordance between genuinely
diverged policies.

## Ordered development gates

1. Run a bit-identity bridge repeat on one already-revealed pair. The model
   architecture is unchanged, so reuse the anchor's measured four-worker
   topology: `0.2178` games/s in the bounded screen and `0.2588` games/s in the
   completed 256-game anchor. The fresh CP7 comparison below projects to about
   33 minutes at the conservative screen rate. The workload is CPU dominated;
   GPU 1 remains exclusively reserved and is expected to remain near idle.
2. Before touching the fresh CP7 seed, compare the fixed candidate and GAE8 on
   1,024 common-receipt native Pool3 episodes at base seed `1830001`.
   Candidate terminal-order net must be at least `-16` overall and at least
   `-12` at each seat. A DAgger-scale 10 percentage-point regression would be
   about `-102/1024`, far beyond these floors. This is a hard transport check,
   not a strength claim. No outcome is inspected before the complete gate-2
   panel finishes.
3. Only after steps 1 and 2 pass, compare candidate and GAE8 against XMage CP7
   skill 7 on 128 fresh seat-swapped pairs, 256 natural games per arm, at base
   seed `1840001`. Require candidate-better minus GAE8-better terminal outcomes
   at least `+4`, candidate total wins at least GAE8 wins plus `4`, and each
   candidate-seat net at least `-2`. No outcome is inspected before the full
   panel completes.

All comparisons use common environment roots, exact seat swaps, natural
terminals, and terminal win/draw/loss only. A failure closes this exact corpus,
four-epoch update, and seed domain without threshold changes or panel reuse.
A pass authorizes a separately frozen larger confirmation and the untouched
CP7 skill-8 reference. It does not promote the model or establish broad,
human, or professional-level strength.
