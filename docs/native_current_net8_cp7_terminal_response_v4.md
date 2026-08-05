# Current Net8 CP7 terminal response v4

Status: bounded throughput and identity screens complete. The formal v4
collection and both training arms have not run.

## Question

Can a four-times-larger on-policy CP7 terminal corpus, a fresh Adam optimizer,
and a decayed learning-rate schedule turn the retained GAE8 policy toward CP7
without recreating the broad movement tail that stopped v1 through v3?

V1 through v3 used 128 games, four full-batch updates, learning rate `0.001`,
and the inherited step-520 Adam state. V3 showed that the KL gradient is zero
at the source, begins acting on update two, and then oscillates under that fixed
optimizer and rate. This experiment changes the data volume and optimizer
schedule together. It is not a coefficient retry on the old corpus.

## Fixed development collection

- Behavior policy: exact retained GAE8 native state
  `ab7dd25ca6619a4a613ca089e1eb8e75981f8e5cfc0bae8535b78cddd7efa952`.
- Opponent: deterministic XMage CP7 skill 7 in the Rally mirror.
- Panel: 256 matched seat-swapped pairs, 512 natural games, base seed
  `1930001`, pair indices `0..255`.
- Export: every GAE8 decision tensor, fixed-source logits and value, selected
  action, and natural terminal win, draw, or loss under the existing outcome
  JSONL v2 contract.
- Reward: natural terminal win, draw, or loss only. Search scores, CP7 action
  labels, intermediate rewards, and reward shaping are absent.

The collection runner uses one GAE8 arm, multiple pairs per JVM, and eight
independent worker databases. Before this panel, run one bounded 32-pair,
64-game throughput and identity screen on already revealed base seed `1820001`.
Report wall time, games per second, CPU, memory, GPU 1 utilization, and exact
output identity. The prior eight-worker candidate-state collector sustained
`0.875` games/s with CPU near saturation while doing extra shadow queries, so
eight workers are the default. If the screen is below `0.60` games/s or shows
resource pressure, test at most one alternative topology before choosing.
The `0.60` rate is the trigger for exactly one alternative, not a campaign
stop. After that single alternative, choose the resource-safe,
identity-passing topology with the higher measured rate; an exact tie goes to
fewer workers. No third topology is authorized.
For the mechanical gate, resource pressure means minimum available system
memory below `16 GiB`, maximum system-memory use above `90%`, or GPU 1 memory
use above `512 MiB`. The formal collector accepts only the exact declared panel
and requires the selected screen, revealed-identity, and, when the trigger
fired, topology selection report hashes in its own manifest. It independently
revalidates the identity tool, baseline and candidate report hashes, complete
shard inventory, and every normalized per-pair digest.

The fixed collection starts only after a small manifest binds the git commit,
scorer and runner hashes, toolchain including linker, source package and card
database hashes, seed, worker topology, and output root. GPU 1 remains
exclusive and is expected to stay idle because the workload is CPU-bound.

## Fixed fresh-optimizer screen

Both arms start from the exact GAE8 model parameters, discard the inherited
first and second Adam moments, and reset Adam step to zero. The value gradient
is exactly zero. Each update uses the complete 512-game corpus, fixed-source
`pi_old`, seat-standardized terminal advantages with equal episode and physical
decision mass, PPO clip `0.10`, and full-network policy gradients.

The learning rates for updates one through eight are fixed to
`[0.0005, 0.0004, 0.0003, 0.0002, 0.00015, 0.0001, 0.000075, 0.00005]`.
The only arm difference is the full legal-action
`KL(pi_old || pi_current)` coefficient:

1. beta `0.3`
2. beta `1.0`

Both arms run all eight updates. Candidate checkpoints are measured after
updates `2`, `4`, `6`, and `8`. No terminal result, coefficient, learning rate,
update count, or checkpoint is changed after collection begins.

After collection and merge, the exact merged corpus SHA-256, decision count,
physical-group count, and terminal-return counts are pinned into the trainer
and screen manifest in one reviewed commit before either arm runs. A
caller-supplied hash is only a consistency check until that pin is present.

## Movement-only eligibility and selection

A checkpoint is eligible only if all quantities are finite, Adam step equals
its update index, parameter L2 from GAE8 is at most `0.75`, mean row action TV
is in `[0.010, 0.050]`, p90 row TV is at most `0.150`, p99 row TV is at most
`0.150`, p99 physical-group absolute selected joint log ratio is at most
`0.75`, maximum absolute selected joint log ratio is at most `1.0`, and zero
physical groups exceed absolute selected joint log ratio `1.0`.

Among eligible checkpoints, highest mean TV wins. An exact tie selects the
larger beta, then the earlier update. Corpus terminal counts and gameplay
outcomes are excluded from selection. If none is eligible, stop this exact
larger-corpus, fresh-optimizer recipe without another local schedule, beta, or
threshold retry.

Each arm may write only a non-runnable staging package for its own best
eligible checkpoint. One global selector consumes both complete arm reports,
re-applies every movement gate over the Rust-written metrics, validates the
staging payload hashes, and applies the cross-arm rule. Only that selector can
write a package with a terminal-gate authority, and the Rust loader must reload
that package against the selector-report hash before gameplay. The selector
does not independently derive movement metrics from model parameters; its
explicit trust boundary is the Rust arm report.

## Ordered terminal gates

Only the selected checkpoint may continue through the still-untouched gates:

1. One already-revealed-pair bridge repeat with exact identity and output
   transport checks.
2. Candidate versus retained GAE8 on 1,024 common-receipt native Pool3
   episodes at base seed `1830001`; require overall terminal-order net at least
   `-16` and each seat at least `-12`.
3. Candidate and fresh GAE8 arms versus XMage CP7 skill 7 on 128 fresh
   seat-swapped pairs, 256 natural games per arm, at base seed `1840001`;
   require paired terminal-order net at least `+4`, candidate wins at least
   GAE8 wins plus `4`, and each candidate-seat net at least `-2`.

No gate outcome is inspected before its complete panel finishes. CP7 skill 8
at base seed `1850001` remains untouched as a second external reference. A
skill-7 pass authorizes a separately frozen skill-8 test and larger
confirmation, not automatic promotion.

## Compute and nonclaims

The completed eight-worker screen measured `0.440188` games/s, while the one
authorized sixteen-worker alternative measured `0.323490` games/s. Both were
resource-safe and exactly identical to the revealed baseline after removal of
only the three shard-local ordinal fields. The selected eight-worker topology
projects the 512-game collection at about 19.4 minutes. Actual collection and
training timing will be reported from their run artifacts.

This is a versus-CP7 skill-7 response experiment. It does not establish broad,
human, professional, or metagame-wide strength. CP7 is part of the training
distribution. Terminal win, draw, or loss remains the only playing-strength
and promotion measure.
