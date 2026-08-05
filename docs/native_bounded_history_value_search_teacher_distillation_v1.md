# Bounded history-value search-teacher distillation v1

Status: draft for Fable countersign. No compute launch is authorized by this
draft.

## Question

Can information-set-safe depth-8 lookahead supply action targets that a
history-aware policy can distill into real CP7 playing strength, even though
deploying earlier fixed search selectors at inference did not pass?

Search is used only to label states visited by the exact qualified structured
policy. The trained learner acts without search. Natural terminal win, draw, or
loss remains the only reward and the only playing-strength measure. Search
scores and supervised targets are never rewards.

## Fixed teacher and source

- Acting policy: qualified structured policy candidate SHA-256
  `204beb91c1a4b039e0c497f2b420e823b5cc9e2ceb8560f897d0b6251e916b72`,
  composite SHA-256
  `47b10c1114efc01f9445c71c0c8c4d8cd4a4b89a2154ac68275f3b0c6ebb9ce3`.
- Leaf critic: confirmed bounded complete-public-history value candidate
  SHA-256
  `83d6d2ddb97e96cf5ef4feda525b035bba079d6d1d2f4bc44f4affcf70fd6529`,
  composite SHA-256
  `6329233bcc22f7941e8085ef0235107eb75293fe74c727434c0474da15354f22`.
  Its fresh confirmation report SHA-256 is
  `716189e49c635eebdf5647e17ef4e3b3ab684c68addbc6b3c94fc3bed46f7539`.
- Combined package manifest SHA-256:
  `0d883d169fca504e4a413810454565d98cd0e8316cb76e7de4f538187b2865c9`.
- Source roots: reuse the already revealed 256 candidate-state CP7 pair roots
  at base seed `1400001`, pair indices `0..255`. For every task, first replay
  the exact qualified policy with search disabled and retain its candidate
  outcome and opponent-teacher JSONL. Then replay the same roots with search
  enabled only on cloned native information-set snapshots. The two candidate
  outcome streams and the two opponent-teacher streams must each be
  byte-identical, including every selected action, model input, action
  semantic, and natural terminal. Any mismatch stops the lane. Search never
  controls or mutates the authoritative replay session.

At each candidate-controlled surface decision with one substep, physical
decision ordinal at least 20, and 2 through 8 legal actions, draw four
acting-player information-set redeterminizations shared across root actions.
For each legal action, apply it and run at most eight additional policy
decisions. Both seats use deterministic qualified-policy argmax with lower-index
ties. Natural terminals receive exact candidate-seat terminal value. A branch
still live after eight decisions receives the confirmed bounded critic value,
signed to the candidate seat. Actual hidden cards and library order never enter
the ranking.

The teacher target is the highest mean-value action only when it beats the live
fallback action by at least `0.25`; otherwise it is the fallback action. The
live action is always retained. Every label records the join key, fallback and
teacher indices, legal action semantics, four redeterminizations, per-action
values, branch completion counts, public-history hash, and model-input SHA-256.

The maximum budget for a root with `A` legal actions is `4 + 36A` policy/value
evaluations, at most 292 when `A=8`, with early terminals reducing it. The prior
weak-critic depth-8 gate completed eight matched pairs, 16 games total, in
144.13 seconds at four-worker concurrency. This is about 36.0 worker-seconds
per game and projects 512 search games at four workers to about 77 minutes.
The mandatory policy-only reference replay adds its measured cost. Before
collection, measure 1, 2, and 4 concurrent JVM tasks on 16 already revealed
roots, require byte-identical reference and shadow trajectories, exact repeated
labels, and four distinct samples per root, then select only the fastest valid
topology. Record achieved games per second, CPU and GPU utilization, memory,
and the revised wall-time projection.

The critic's independently held-out episode-balanced MSE was `0.442846`, so the
`0.25` action-margin threshold is not claimed to exceed per-example critic
error. Its justification is selectivity from an independently confirmed
outcome predictor, not a confidence interval on any root. The matched
fallback-control and fresh terminal selector determine whether that imperfect
target signal is useful.

## Fixed learner comparison

Train two width-128 two-layer recurrent structured residuals from the same
zero-residual initialization over the qualified policy logits:

1. `search-target`: selected-index cross entropy to the search-teacher target.
2. `fallback-control`: the identical objective with the target replaced by the
   live fallback action at the same rows.

Both arms add `6 * KL(parent || learner)` on every labeled row, use the hard
`0.49` legal-action log-probability budget, equal label-bearing-episode mass,
AdamW learning rate `3e-4`, weight decay `1e-4`, gradient cap `5`, batch 256,
eight epochs, and seed `20260811`. No coefficient, epoch, width, margin, or
checkpoint is selected. The value head is frozen and ignored.

Before any learner fit, compute override coverage directly from the completed
label corpus. Search targets must differ from fallback on at least 5 percent of
eligible label mass separately at P0 and P1, weighting each label-bearing
episode to total mass one and each eligible label equally within its episode.
Failure stops before training. This is a target-content gate only and is not
playing-strength evidence.

Pair residues 1 and 2 modulo 4 train both arms. Residue 3 is read once, followed
only on pass by residue 0. On each held-out residue, require the search-target
arm to improve search-target NLL at least 5 percent relative to the
fallback-control, improve top-1 at least 3 percentage points, and not regress
search-target NLL at either candidate seat. Both learners must be finite, have
maximum pre-clip gradient norm at most 5, mean TV at most 0.03,
p90 TV at most 0.10, and maximum legal-action log-probability change at most
0.49. A pass refits each fixed arm once on all 256 pairs.

## Single-shot terminal selector

Run the exact qualified parent, fallback-control refit, and search-target refit
against XMage CP7 skill 7 on base seed `1810001`. Acquire 256 mutually
successful seat-swapped clusters, 512 natural games per arm, from at most 288
ascending pair indices. A cluster is excluded only when any arm fails before a
natural terminal. All three arms retain the same environment root and seat
schedule. No outcome is inspected until the fixed panel is complete.

Advance to a separately frozen candidate-02 V3 sheet only if:

1. search-target terminal-order better minus worse is at least `+8/512` versus
   the qualified parent;
2. search-target terminal-order better minus worse is at least `+8/512` versus
   fallback-control;
3. search-target minus parent is at least `-4` separately at P0 and P1; and
4. all identity, natural-terminal, pairing, sample-distinctness, movement, and
   numerical checks pass.

Terminal order is win greater than draw greater than loss. The panel is a
development selector, not a confidence claim. The observed native planning
heuristic spans 1.46 to 2.91 times mean TV across four PPO or structured
residual panels, none in this recurrent distillation class. The low-end ratio
and TV 0.03 suggest about 4.5 percent discordance, or `D` near 23 in 512 games.
Under a null direction split, net SD is then about `sqrt(23)=4.8`; `+8` is about
1.7 SD with roughly 5 percent one-sided false-advance probability. TV 0.03 is
the movement ceiling, not a projection. Under the closest prior cap regime,
CP7 candidate-state DAgger reached mean TV only 0.0103 before its per-decision
cap bound; at the low-end 1.46 ratio that implies `D` near 8, at the arithmetic
floor for a `+8` net. If discordance is 10 percent, roughly bracketing the high
end of the observed ratio range, `D` is about 51, SD about 7.1, and the same
margin is only 1.1 SD.
Requiring both parent and matched-control margins lowers false advance, but the
two comparisons are correlated, so no independence claim is made. At least
eight discordances are arithmetically necessary. Low discordance can therefore
produce a valid false stop, which a later V3 panel, not threshold tuning here,
is designed to address.

Failure closes this exact source panel, depth, four-sample teacher, margin,
learner, and seed domain without panel reuse or threshold changes. A pass only
authorizes a fresh V3 design, assigned-alpha sizing with at least a 50 percent
haircut to this revealed effect, per-sheet countersign, implementation review,
and a new 1/2/4-worker formal throughput screen.

## Nonclaims

- The confirmed critic predicts outcomes; it is not itself playing strength.
- The teacher's self-policy continuation is not an exact model of CP7.
- A development pass does not consume V3 alpha or promote a policy.
- This lane does not test an independent exploiter, human strength, cross-deck
  play, or professional-level play.
- A pass on this structured recurrent lineage proves the search-as-teacher
  mechanism, not that it benefits the Net8 or GAE8 lineage. A pass obligates a
  bridge step, either adapter-based distillation into the Net8 or GAE8
  representation or a formal structured-versus-GAE8 strength gate, before
  further structured-lineage investment is treated as strategically
  load-bearing.
