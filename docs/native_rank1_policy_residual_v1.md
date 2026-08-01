# Native rank-one policy residual v1

## Question

Can the strongest low-capacity projection of the rejected full bilinear policy gradient improve the retained model on fresh XMage CP7 games?

## Fixed design

- Parent: exact retained outcome manifest `706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb` at Adam step 1.
- Corpus: all 32 previously collected on-policy pairs, JSONL SHA-256 `b75677397c8461a702bdb5d0f7dfc47fe651e2cd1d4f048cc218001055a828cd`. The old holdout was already revealed and was not reused as a holdout.
- Architecture: `parent_logit + state_hidden^T W action_hidden`, with the frozen parent and value output. `W` is the deterministic leading rank-one projection of the zero-point analytic policy-gradient matrix.
- Capacity: 127 effective degrees of freedom, expanded to a row-major `64 x 64` runtime matrix.
- Scale: selected without fresh outcomes to produce mean policy total variation `0.01`, subject only to movement caps.
- Strength decision: one fresh matched CP7 block at base seed `1140001`, episodes `0..31`, with the candidate and retained control run sequentially on the same 16 seat-swapped pairs.
- Frozen qualification rule: paired gains `G >= L + 2`, net `G - L >= -1` separately for P0 and P1, and no candidate projection, fallback, alignment, or identity failure.
- Failure behavior: retire the frozen-latent bilinear residual family without changing rank or scale against the revealed fresh block.

Here, a gain is an episode the rank-one candidate wins and the retained control loses. A loss is the reverse, and a tie is an episode with the same model win/loss result in both runs.

## Candidate package

Commits `0eebe18`, `dd37378`, and `e4d1477` implemented and hardened the generator, strict package loader, and rank-one projection. Commit `801d1d2` routed a valid package through the existing XMage checkpoint-shadow scorer while preserving the Java authority and identity checks.

The formal package is `/mnt/d/mtg-kernel-rank1-policy-residual-v1`:

- `candidate.json` SHA-256: `6a4f6d7f9cdd397c888cbcd6bd79f3849ab8b59992e3d80fe9b0372c7dd9b606`.
- `report.json` SHA-256: `55271d5418d016953af2b08e54e601f77adc762a90f46315a8e363bdaf35148b`.
- `weights.f32le` SHA-256: `7065c332c4654b2f0bffb885160a988f36f0828b0744403cc01da77af125098a`.
- Windows scorer SHA-256: `1845c69c8f709ee8450603c324590ed4029f11cba5ba8001f466168eee1a69b0`.

Offline movement checks all passed. The projection captured `70.5157%` of gradient energy. Mean total variation was `0.0100000`, p90 total variation was `0.0322877`, mean parent-to-candidate KL was `0.000709855`, and the all-data training surrogate was `+0.00125287`.

These checks qualified the package for fresh evaluation only. They were not a play-strength claim.

## Fresh CP7 result

The candidate log is `/mnt/d/mtg-kernel-rank1-cp7-base1140001-candidate.log`, SHA-256 `b6bd95d8012563c3cfb6426d6f138165399c62c025d945c029b9ff612e478b72`. The retained-control log is `/mnt/d/mtg-kernel-rank1-cp7-base1140001-control.log`, SHA-256 `d043eebd970edb095d580b3a078e6ba85e005b74343ea9e20a31238e26a0fb52`.

The rank-one candidate finished 15-17 against CP7 for score `0.46875`: 8-8 as P0 and 7-9 as P1. The retained control finished 14-18 for score `0.43750`: 7-9 as P0 and 7-9 as P1.

Joining the runs by episode produced:

| Stratum | G | L | T | Net |
| --- | ---: | ---: | ---: | ---: |
| All 32 episodes | 1 | 0 | 31 | +1 |
| Even episodes, candidate P0 | 1 | 0 | 15 | +1 |
| Odd episodes, candidate P1 | 0 | 0 | 16 | 0 |

The only changed outcome was episode 20, environment seed `1488c08b710292af`. The rank-one model as P0 won; the retained control as P0 lost.

Both runs completed all 32 legs with matching seats and environment seeds. Both reported zero candidate priority projections, every leg reported `alignment=no_selected_action_projection`, and no scorer fallback, alignment-mismatch, protocol, or identity record occurred. The CP7 telemetry field `forced_priority_pass_menu_mismatch` is an opponent forced-event counter and is not a candidate scorer fallback.

The seat floors and cleanliness conditions passed. The primary margin condition failed because `1 >= 0 + 2` is false.

## Disposition

Reject the rank-one candidate and do not spend a confirmation block. Do not sweep scale, rank, or another projection on base seed `1140001`.

Together with the full-matrix held-out rejection, this closes the frozen-latent bilinear residual family. The result is directionally positive but too small to distinguish from a one-game perturbation under the precommitted gate. The next strength candidate should change the learned representation or training signal rather than further tune a residual on the same 32-pair corpus.
