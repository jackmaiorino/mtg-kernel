# Current Net8 XMage CP7 anchor v1

Status: complete. The bridge repeat and topology screen passed. The fresh
64-pair benchmark retained GAE8 after GAE8 and GAE16 each went 48-80 against
XMage CP7 skill 7, with paired gains 1-1 and 126 ties.

## Question

What is the external Rally strength of the current Net8 GAE8 state, and does
the fixed GAE16 extension improve it on identical XMage CP7 skill-7 games?

Natural terminal win, draw, or loss is the only outcome. This is an external
software anchor and current-model ranking screen, not professional-level
evidence.

## Fixed models

The GAE8 payload is
`D:\mtg-kernel-composed-factorial-v1\gae-16-update-development-v1\fresh-eval-v1\gae-8-update.state.f32le`,
SHA-256 `a0b7752181a562f8e5a0821a490ce20b777b509855d754283536e8242f489b98`,
native-state SHA-256
`ab7dd25ca6619a4a613ca089e1eb8e75981f8e5cfc0bae8535b78cddd7efa952`,
model-parameter SHA-256
`5efe2f167045bde379da3be8af6c480b6702f5d7a849ff8435d8ac6b1d91daa8`,
and Adam step 520.

The GAE16 payload is
`D:\mtg-kernel-composed-factorial-v1\gae-16-update-development-v1\fresh-eval-v1\gae-16-update.state.f32le`,
SHA-256 `2cb26bd9cd439090e923dc9e4df05d5d9a0793a1b26d4b632fe101c35ce84758`,
native-state SHA-256
`519ba6e6a9fec87d58fdf62b638eb78404319a4f2456a35575b8e55da4238415`,
model-parameter SHA-256
`015ef868210c09d2fdb0e50ad042d0ad136c55bf642962f9bfcfcdb282c44e26`,
and Adam step 528.

Each two-file deployment package contains one canonical JSON manifest and one
raw train-state payload. The loader requires exact inventory, canonical JSON,
payload length, all six raw and semantic SHA-256 values, Adam step, scorer-bias
anchor bits, architecture identity, and finite inference output. Package
identity is emitted in every scorer header.

## Bridge and throughput preflight

First run one already revealed pair twice for each model. Require exact package
identity, two natural terminals, matching candidate-seat schedule, and
bit-identical normalized candidate outcome streams. A failure may be fixed and
retried before formal roots are touched.

Then run the same four revealed pairs at one, two, and four concurrent JVM
workers. Select only the fastest topology with exact outcomes across
topologies, zero scorer or mapper failures, and acceptable host memory. Record
games per second, CPU, RAM, GPU 1 utilization, and projected formal wall time.
No model or gameplay setting may change.

## Fresh common-root benchmark

- Opponent: XMage CP7 skill 7.
- Base seed: `1820001`.
- Pair indices: `0..63`.
- Arms: exact GAE8 and exact GAE16.
- Games: 128 natural games per arm, with both candidate seats for every pair.
- Roots: both arms receive identical pair environment seeds and seat schedule.
- Maximum attempts: 72 ascending pair indices to acquire 64 mutually valid
  clusters. An index is excluded only if either arm fails before a natural
  terminal.
- Outcomes remain hidden until all 64 mutually valid clusters are complete.

Report each arm's win, draw, and loss count overall and by candidate seat.
Also report GAE16-better, GAE8-better, and tied terminal-order legs overall and
by seat. Name GAE16 the current external-anchor candidate only if its net is at
least `+4/128` and neither seat is below `-2`; otherwise retain GAE8. These are
rapid ranking margins, not confidence or promotion thresholds.

## Nonclaims

- CP7 skill 7 is not a professional player.
- This benchmark does not consume a V3 candidate slot or alpha.
- A raw CP7 win rate does not validate cross-deck play, hidden-information
  beliefs, sideboarding, or human strength.
- No search score, value prediction, or intermediate game feature is a reward.
