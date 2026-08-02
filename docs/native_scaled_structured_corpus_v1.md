# Native scaled structured corpus v1

## Question

Are the structured model's held-out value failures primarily a small-data
problem?

The 256-pair value-only screen overfit despite using the structured object,
action, and reference path correctly. The next test increases complete-pair
data by eight times and exports both players' observed decisions from each
game. It does not change model width or tune against the revealed folds.

## Fixed collection

- Behavior policy: exact retained parent manifest
  `706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb`,
  Adam step 1.
- Opponent: deterministic XMage CP7 skill 7 in the Rally mirror.
- Base seed: `1400001`.
- Primary block: 2,048 seat-swapped pairs, pair indices `0..2047`, 4,096
  games.
- Exports: matched CP7 teacher and candidate outcome JSONL from the same game
  trajectories.
- Throughput: eight isolated XMage workers, 32 pairs per initial task.
- Every worker has a private copy of the pinned card database and the four
  deterministic CP7 environment flags.

If a task hits a mapper or engine failure, preserve it and bisect the pair
range until the failing pair is isolated. Exclude only isolated failing pairs.
After the primary block finishes, replace exclusions in sorted order using the
smallest unused pair index at or above 2,048 with the same `pair_index mod 4`.
If a replacement fails, advance by four. This preserves exactly 512 complete
pairs per fold without choosing replacements from game outcomes.

## Data gate

Publish a corpus only if all conditions hold:

1. Exactly 2,048 complete pairs and 4,096 natural terminals are present in
   both matched exports.
2. Every pair has both candidate seats, identical environment identity across
   exports, complete physical substeps, and exact parent identity.
3. There are no candidate projections, scorer fallbacks, alignment failures,
   duplicate episodes, or pair overlap.
4. A repeated task contains at least one byte-identical complete seat-swapped
   pair. Across the full task, model inputs, parent outputs, terminal outcomes,
   and decision counts must be exact. The only permitted remaining difference
   is which arena ID represents interchangeable duplicate copies of the same
   card. This amendment follows two repeats that each changed only duplicate
   discard identities while preserving every terminal result and count; it
   does not permit a different card definition, tensor, parent output, game
   result, or trajectory length.

## Downstream screen

After the data gate, freeze one structured policy-plus-value screen before
training. Keep width 48 and the existing digest channel. Use complete
actor-visible public history from the matched dual exports and whole-pair
held-out folds. Policy and value metrics must pass overall and by seat before
any native integration or GPU campaign.

The collection is development data, not strength evidence. It cannot promote
a model or establish professional-level play.

## Preflight

One pair completed through the exact dual-export command in 4.93 seconds. It
produced two natural terminals in each export, 107 teacher decisions, and 102
outcome decisions. The preflight teacher SHA-256 is
`a52376ae74cfc36ce6be6ef3ddab6110ab3557eec26c02a3bfc9f3e7bbbbbff8`;
the outcome SHA-256 is
`4ada2a0e7011f00f033f57892d2c3e6fc6c1824ec78b90841bcf91675e4eedff`.
