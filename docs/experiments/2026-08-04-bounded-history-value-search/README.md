# Candidate-turn bounded history-value search

Status: frozen before natural-terminal execution.

## Question

Does the confirmed bounded public-history value model improve the exact qualified structured policy when used for a narrow one-step information-set selector against the XMage CP7 anchor?

The treatment changes only action selection at eligible candidate-controlled roots. The qualified policy, opponent, environment, terminal reward, seeds, and paired seat schedule remain fixed. Natural terminal win, draw, or loss is the only playing-strength outcome.

## Fixed models

- Policy package: `D:\mtg-kernel-bounded-history-value-search-v1\policy`, candidate SHA-256 `204beb91c1a4b039e0c497f2b420e823b5cc9e2ceb8560f897d0b6251e916b72`, composite SHA-256 `47b10c1114efc01f9445c71c0c8c4d8cd4a4b89a2154ac68275f3b0c6ebb9ce3`.
- Bounded value package: `D:\mtg-kernel-bounded-history-value-search-v1\value`, candidate SHA-256 `83d6d2ddb97e96cf5ef4feda525b035bba079d6d1d2f4bc44f4affcf70fd6529`, weights SHA-256 `ac5cef36ba96af11acef1c01edc41cbeec792130d498277243d75f8801ff12bf`, composite SHA-256 `6329233bcc22f7941e8085ef0235107eb75293fe74c727434c0474da15354f22`.
- Combined manifest: SHA-256 `0d883d169fca504e4a413810454565d98cd0e8316cb76e7de4f538187b2865c9`.
- Python-to-Rust five-bucket raw-output parity fixture: SHA-256 `ea24f60614d923d47254d4baaa32b44f8395814a60cafb6b3ed76f7350ecc1a6`, with maximum accepted delta fixed by the native test at `2e-4`.
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

## Nonclaims

- This is a rapid mechanism screen, not evidence of pro-level play.
- The prior fresh MSE pass does not imply action-selection improvement.
- XMage is the external anchor and rules provenance, not the training runtime.
