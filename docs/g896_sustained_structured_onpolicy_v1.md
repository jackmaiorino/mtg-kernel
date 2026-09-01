# g896 sustained structured on-policy A/B v1

Revision 2, 2026-08-31

## Question

Can a structured policy that reads the full native state, object, relation,
action, and action-reference tensors improve through sustained on-policy
terminal learning from the g896 anchor?

This is not another history, search, critic, GAE, or one-batch residual screen.
Earlier structured candidates were fit to fixed corpora or stopped after one
batch. This test gives the structured policy 256 consecutive updates from its
own changing behavior and compares it with a current Net8 continuation that
receives the same number of natural games.

## Fixed arms

Both arms start from exact g896 model parameter SHA-256
`97683d41dc35d1b0884c053b069d0b34ea5a4f600f1a1f5ea9bd9e72cf067578`.
They use Rally mirror, 256 updates, 64 complete games per update, the same
learner-seat schedule, the same environment seeds, and the same opponent-slot
schedule. The common opponent pool is the already validated stronger-pressure
pool from `g896_strong_neural_pressure_ab_v1`: every weight and slot is fixed,
with generations 768 and 640 occupying its two treatment slots for both arms.

- CONTROL continues the existing `kernel-policy-value-net-8` model and
  `terminal_reinforce_value/v3` trainer unchanged.
- STRUCTURED keeps the g896 policy/value network frozen as its base. It adds a
  trainable width-128 structured residual over the current native tensorizer's
  globals, objects, relations, legal actions, and action references. Public
  history is excluded because that representation has already been tested.
  The residual policy and value output layers initialize to zero, so update
  zero is exactly g896.

The structured policy adds its residual to the frozen g896 logits and value.
For every physical decision, deterministic bisection scales the policy residual
so every possible joint log-probability ratio relative to g896 is at most
`0.49`. The structured encoder, object and relation message layers, action
reference aggregation, policy head, and value head all remain trainable.

The exact architecture identity is
`g896-stateless-structured-residual-width128/v1`. It uses initializer seed
`20260831`, a 65,537 by 32 dense card-token embedding, a 20 by 16 object-group
embedding, a `220 -> 128 -> 128` state path, a `146 -> 128` object path, two
`169 -> 128 -> 128` directed relation-message rounds, 20-way group pooling, a
`196 -> 128` action path, a `153 -> 128` action-reference path, four-head
action-to-object attention, a `640 -> 256 -> 128` joint path, and zero-initialized
policy and value residual outputs. It has 2,662,882 parameters. The sorted
parameter-name-and-shape layout SHA-256 is
`6c921efc972b860a6a64589ec20537b3123ea55654326defc0373cd2d90d28cf`.

## Learning law

Natural terminal win, draw, or loss is the only reward and target. Both arms
use the existing `terminal_reinforce_value/v3` semantics: one policy/value term
per completed physical decision, joint physical-decision log probability,
return in `{-1, 0, +1}`, stopped value baseline, value coefficient `0.5`, and
one Adam update after each 64-game batch. No CP7 data, human actions, heuristic
targets, shaped rewards, search labels, intermediate rewards, imitation loss,
or checkpoint selection enters training.

STRUCTURED uses the same optimizer constants as the g896 run. Its optimizer
state starts at zero because its parameters are new. CONTROL retains the g896
optimizer state. The formal candidate is the fixed update-256 endpoint in each
arm. No intermediate strength result selects an endpoint.

## Engineering preflight

Preflight is freely retryable and may use GPU 1. It must establish:

1. clean source, pinned Rust, Cargo, MSVC linker, Python, PyTorch, CUDA, and
   physical GPU-1 identities;
2. exact g896, opponent, tensorizer, model-layout, seed, and optimizer bindings;
3. structured CPU reference versus selected GPU forward/loss/gradient agreement
   inside a declared numerical envelope;
4. one fixed 64-game structured update repeated from independent copies with
   identical outcomes, selected actions, model state, and optimizer state;
5. one fixed checkpoint replay producing a bit-identical terminal stream; and
6. a measured projection for the 16,384-game arm. If the conservative estimate
   exceeds 12 hours, classify `ENGINEERING_REDIRECT` and implement batched
   scoring before any formal training. This is not a model failure.

## Formal training and native gate

Run CONTROL and STRUCTURED sequentially on exclusive headless physical GPU 1.
Each run publishes one small manifest with commit, toolchain including linker,
seeds, input and output SHA-256s, and GPU ordinal. Stores publish checkpoints
crash-consistently every four updates.

After both update-256 endpoints exist, evaluate frozen STRUCTURED, CONTROL, and
g896 on the same fresh non-CP7 promoted(2) roots, seat-swapped, in 128-root
chunks to a maximum of 4,096 roots. Advance only if STRUCTURED:

- has at least 41 more paired terminal-order points than CONTROL;
- has at least 41 more paired terminal-order points than frozen g896;
- has an anytime 95 percent confidence-sequence lower bound above zero for
  both comparisons; and
- is no worse than `-18` paired terminal-order points at either learner seat
  in either comparison.

One disjoint confirmation uses the same gates. Only an initial and confirmation
pass authorizes the already-defined two-panel CP7 holdout. CP7 remains blocked
during design, implementation, preflight, training, native initial, and native
confirmation.

## Interpretation

A pass shows that sustained terminal-only learning can exploit the structured
representation on native Rally and creates a lawful promotion candidate. A
training or native failure closes this exact width, trust bound, budget, and
opponent schedule. It does not prove that all structured models fail. Neither
training diagnostics nor native win rate is a CP7 or professional-level claim.
