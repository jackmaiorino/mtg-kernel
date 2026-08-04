# Candidate-turn bounded history-value search

Status: rapid mechanism gate complete, failed. One independent extension is frozen below.

## Question

Does the confirmed bounded public-history value model improve the exact qualified structured policy when used for a narrow one-step information-set selector against the XMage CP7 anchor?

The treatment changes only action selection at eligible candidate-controlled roots. The qualified policy, opponent, environment, terminal reward, seeds, and paired seat schedule remain fixed. Natural terminal win, draw, or loss is the only playing-strength outcome.

## Fixed models

- Policy package: `D:\mtg-kernel-bounded-history-value-search-v1\policy`, candidate SHA-256 `204beb91c1a4b039e0c497f2b420e823b5cc9e2ceb8560f897d0b6251e916b72`, composite SHA-256 `47b10c1114efc01f9445c71c0c8c4d8cd4a4b89a2154ac68275f3b0c6ebb9ce3`.
- Bounded value package: `D:\mtg-kernel-bounded-history-value-search-v1\value`, candidate SHA-256 `83d6d2ddb97e96cf5ef4feda525b035bba079d6d1d2f4bc44f4affcf70fd6529`, weights SHA-256 `ac5cef36ba96af11acef1c01edc41cbeec792130d498277243d75f8801ff12bf`, composite SHA-256 `6329233bcc22f7941e8085ef0235107eb75293fe74c727434c0474da15354f22`.
- Combined manifest: SHA-256 `0d883d169fca504e4a413810454565d98cd0e8316cb76e7de4f538187b2865c9`.
- Python-to-Rust five-bucket raw-output parity fixture: SHA-256 `ea24f60614d923d47254d4baaa32b44f8395814a60cafb6b3ed76f7350ecc1a6`, with maximum accepted delta fixed by the native test at `2e-4`.
- Frozen card database source: SHA-256 `b833d6a7b44ad1f7bd6aef9a21d1f2498136ef61e44db0e48e60e5ec471ce09d`. This is a dedicated copy of the current XMage H2 database and differs bytewise from the historical `e7c982...` snapshot used by earlier search screens. Treatment and control use copies of this same source.
- The value package is forbidden from supplying policy logits. The separate qualified policy package always supplies fallback and action logits.

## Fixed selector

At a candidate-controlled surface decision, search is eligible only when substep index is zero, substep count is one, the actor has reached physical decision ordinal 20, and there are 2 through 8 legal actions.

For each eligible root:

1. Draw four shared hidden-information redeterminizations from the fixed native seed schedule.
2. Apply each legal action once in every redeterminization.
3. Use exact candidate-relative terminal reward for terminal successors.
4. Use the bounded value model only when every nonterminal successor remains candidate-controlled. If any action in any redeterminization yields an opponent-controlled successor, retain the qualified-policy fallback for the whole root and record the exclusion.
5. Average the four successor values per action. Override the qualified-policy fallback only when the best action exceeds its value by at least `0.25`.

This candidate-turn restriction keeps the deployed value queries inside the candidate-decision distribution established by the fresh confirmation. It makes no claim about opponent-turn value accuracy.

## Throughput and determinism preflight

Before the fresh gate, run one nonfresh matched seat-swapped pair twice with identical inputs. Both arms must complete, the search and control logs must be bit-identical between repeats, all four redeterminizations at each reported root must be distinct, diagnostic contracts must pass, and the achieved wall time and task concurrency must be recorded. Preflight may change execution topology only.

The selected topology is at most four pair indices per batch, with treatment and control for every index run concurrently. Each JVM remains single-threaded for deterministic simulation.

## Fresh mechanism gate

- Opponent: XMage CP7 skill 7.
- Base seed: `1700001`.
- Target: 8 mutually successful seat-swapped pairs, 16 natural games.
- Maximum attempts: 32 pair indices, in batches of at most four.
- Control: exact qualified structured policy package with ordinary policy sampling.
- Treatment: the same qualified policy plus the fixed bounded selector above.

All gates must pass:

1. Treatment gains are at least treatment losses plus two games.
2. Treatment paired net is at least `-1` separately at P0 and P1.
3. At least one eligible override occurs at each candidate seat.
4. Every reported four-sample set contains four distinct privileged-state hashes.
5. Every candidate-turn diagnostic is internally consistent: eligible roots have zero opponent successors, and excluded roots have at least one opponent successor.
6. All accepted games are natural terminals with exact matched seeds and seat swaps. Harness or scorer errors do not count as outcomes.

If the gate passes, extend to a larger fresh paired panel before making a strength claim. If it fails because the mechanism is active but loses, close this one-step selector. If it is too sparse to override on both seats, use the diagnostics to decide whether to relax only the candidate-turn deployment boundary or move directly to a larger recurrent end-to-end learner. No threshold is tuned on these eight pairs.

## Rapid gate result

The repeated one-pair preflight completed in `35.42` and `34.22` seconds. Treatment and control trajectory-bearing log content was bit-identical between repeats after replacing only the explicitly nondeterministic `elapsed_ms` fields. Canonical treatment SHA-256 was `489dd57472f634a5330536abeeb0fe1f4c777e8efac17daaa3c3eb0b445fb8fc`; canonical control SHA-256 was `f19e81cd07de5b868f0a3af3205b06d949c4dbe8554ef5cfb8b6cd0b51657b75`.

The fresh eight-pair gate completed all 16 treatment games and 16 matched control games in `95.31` seconds with no task, sample-distinctness, or diagnostic failures. Treatment and control each won 8 of 16 candidate games. Every paired outcome tied, so gains, losses, and net were all zero. The paired-gain gate failed.

The mechanism was active but sparse. It found 22 eligible P0 roots and 13 eligible P1 roots, excluded 107 P0 roots and 70 P1 roots because some immediate successor passed control to the opponent, and overrode the policy twice at P0 and once at P1. Two overrides changed downstream trajectory-length summaries but no override changed a winner. This is a valid negative rapid screen, but three interventions are too few to distinguish a genuinely neutral selector from a low-frequency useful one.

Report: `D:\mtg-kernel-bounded-history-value-search-v1-formal\report.json`, SHA-256 `690cce58e219700d34c7cbfaa23259827568bf0128bb2eb8075f800770fbb42f`.

## Independent extension

Run exactly one independent 64-pair, 128-game treatment panel and matched 128-game control panel at base seed `1710001`. Keep every model and selector setting above unchanged. This panel is not pooled with the observed rapid gate.

Before it, run a 12-pair nonfresh topology screen at base seed `950001` with 24 concurrent single-threaded JVM tasks. Use batch size 12 only if all tasks succeed, peak Java working set remains below 60 GB, and total simulated throughput is at least `0.32` games per second across both arms. Otherwise retain batch size 4. Topology is the only selectable setting.

The extension passes only if all conditions hold:

1. Treatment gains are at least treatment losses plus four games.
2. Treatment paired net is at least `-2` separately at P0 and P1.
3. At least eight eligible overrides occur at each candidate seat.
4. Sample distinctness and candidate-turn diagnostic contracts have zero violations.
5. All 64 matched pairs complete with natural terminal outcomes and no substituted pair.

An extension pass authorizes a larger strength validation. Any extension failure closes this one-step selector and moves the project to the larger recurrent end-to-end learner. These thresholds were fixed after observing only the rapid panel and before touching seed `1710001`.

## Nonclaims

- This is a rapid mechanism screen, not evidence of pro-level play.
- The prior fresh MSE pass does not imply action-selection improvement.
- XMage is the external anchor and rules provenance, not the training runtime.
