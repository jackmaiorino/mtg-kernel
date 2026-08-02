# Native structured policy residual live v1

## Question

Can the structured object, relation, action, and reference representation that
strongly improved held-out CP7 policy prediction produce a small live strength
gain when its rejected value residual is removed?

## Fixed candidate

- Parent: exact retained outcome manifest
  `706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb`
  at Adam step 1.
- Development corpus: all 4,952 policy examples from the base-seed `970001`
  CP7 teacher export, JSONL SHA-256
  `24211ca83cc56d40fd2b574bbb120345aa602d9bec66e0e1b938ff9cb91bf6b0`.
  The corpus is reused development data and is not strength evidence.
- Parent outputs: recompute every row under the retained parent and bind each
  row to the exact exported generation-384 logits before replacing them.
- Architecture: the raw structured screen's 48-wide state, object, relation,
  group-pooling, action, reference, and object-attention path. Train only its
  policy residual. The live value is the retained parent's exact value.
- Fit: seed `20260802`, 20 epochs, batch size 32, AdamW learning rate `3e-4`,
  weight decay `1e-4`, gradient norm cap 5, and 12 CPU threads.
- Calibration: multiply only the final policy head by a deterministic scalar
  no greater than 1 so weighted mean policy total variation is at most `0.02`.
  No fresh game result may change the scale or fit.

Offline policy agreement and movement checks qualify the package for live
evaluation only. They are not play-strength claims. This candidate differs
from the rejected simple behavior clone because it learns from typed objects,
relations, action references, and action-conditioned attention. Better CP7
agreement can still reduce strength.

## Live protocol

After strict native loading and one protocol smoke, run the candidate and
retained parent sequentially against XMage CP7 on the same 16 seat-swapped
pairs at fresh base seed `1160001`, episodes `0..31`.

A gain is an episode the candidate wins and the retained parent loses. A loss
is the reverse. A tie has the same win/loss result in both runs.

The candidate qualifies only if all conditions hold:

1. Paired gains satisfy `G >= L + 2` over all 32 episodes.
2. Candidate-minus-parent paired net is at least `-1` separately when the
   candidate is P0 and P1.
3. Both runs complete all 32 legs with matched seats and environment seeds.
4. There is no candidate projection, scorer fallback, alignment mismatch,
   protocol failure, or identity failure.

A pass authorizes 32 additional fresh seat-swapped pairs at base seed
`1170001` with the frozen package. A fail retires this fixed policy-only
candidate. Do not tune width, epochs, scale, or architecture against either
fresh block.

## Non-claims

This test does not validate the policy corpus as a held-out dataset, establish
generalization beyond this matchup, authorize promotion, or establish
pro-level play.

## Candidate package

Commits `4504c86` and `9278c18` implement the fixed fit, deterministic
publication, strict native loader, structured inference, and checkpoint-shadow
integration. The final package is
`D:\mtg-kernel-structured-policy-residual-v1\candidate-final`:

- `structured_candidate.json` SHA-256:
  `3918ebc432aa65216898707ef1cc63d49f4251a0968ab0200ecceb222fb93aee`.
- `report.json` SHA-256:
  `164853713285ffdaac6aa1ceb393bd6fd20386b1f081cdd55eac30a7454820a6`.
- `weights.f32le` SHA-256:
  `fc159303af67f888e92e50d85b43899cac7bf373e0aed77a7ddd86a5ede0c406`.
- Composite model-parameter SHA-256:
  `3ec3b507ec6475f0208b195a00d68ff075f7c914c43df6310b56ba75e82a4445`.
- Windows scorer SHA-256:
  `3cfa92c7b96ab984600555ee91192aab0eada633fc69c27204ca7eb07457ddbe`.

Two independent fits produced identical final package bytes across all five
files after wall-clock runtime was removed from the published report. The
strict loader accepted the final package and exact retained parent.

The final scale was `0.0092964877`. Weighted mean total variation was exactly
`0.0200000`, p90 total variation was `0.0504729`, and mean parent-to-candidate
KL was `0.00208830`. Development policy NLL improved by `2.98%` overall,
`3.37%` for acting seat P0, and `2.66%` for acting seat P1. The calibrated
candidate changed only 3 of 4,952 development-corpus top actions.

## Fresh CP7 result

The old-seed one-pair protocol smoke passed both legs with zero projections and
clean alignment. The formal candidate and retained parent then each completed
all 32 fresh legs at base seed `1160001`:

| Model | Overall | P0 | P1 |
| --- | ---: | ---: | ---: |
| Structured policy residual | 15-17 | 5-11 | 10-6 |
| Retained parent | 15-17 | 5-11 | 10-6 |

Joining by episode produced `G=0`, `L=0`, and `T=32`. The paired net was zero
for P0 and P1. Seats and environment seeds matched for every episode, and both
runs reported zero projections with `no_selected_action_projection`. No
fallback, protocol, alignment, or identity failure occurred.

Ten episodes had different Rust-step or physical-decision counts, so the
residual did affect live trajectories. None of those trajectory changes
changed a terminal outcome.

The candidate log SHA-256 is
`eefdfd054a08c23f86eb7ce9842383e641aa7235f10dcc28791a820ab1ee6e6f`.
The retained-control log SHA-256 is
`d091214f260bcd23f295963b88adeec3469719805b322bd2ab1ad9c1c7354622`.

## Result disposition

Reject the fixed candidate because `0 >= 0 + 2` is false. Do not run the
base-seed `1170001` confirmation block and do not tune this package against the
revealed block.

This result closes only the calibrated policy-only candidate, not structured
representations generally. It shows that the strong offline representation
signal survived native integration and altered some trajectories, but this
small residual did not produce a measurable outcome improvement.
