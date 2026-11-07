# Loss-terminal rules profile (v1)

Issue #433. Opt-in; default off.

## Defect

`advance_until_decision` checks for game over only at the top of its loop. A
stage can run state-based actions that set `has_lost` and then fall through
to a later decision in the same pass. Highway Robbery's sacrifice branch
draws two from an empty library, the state-based actions set `has_lost`, and
the loser then receives priority. The RL session publishes that decision, so
such states can reach training rows. The next advance still ends the game.

## Rule

`RuntimeRulesV1::loss_ends_game`: when on, `advance_until_decision` returns
`GameOver` in place of any other decision once a player has lost; `GameOver`
and `Halted` pass through. Before the loss, transitions, decisions and
`GameState::state_hash` match the default rules. With it on, no published
in-game decision has `has_lost` set.

## Opting in

| Consumer | Setting |
|---|---|
| Episode or run config | `"runtime_rules": "loss-terminal-v1"`, or `"resolution-boundary-loss-terminal-v1"` for a study already on `resolution-boundary-v1` (keeps its Delve refusal) |
| Rust session | `set_runtime_rules_v1` with the profile's `EpisodeRuntimeRulesV1::rules()`, before the first decision |

A study adopts it at its next design gate, never mid-study. It is a new
runtime identity: decision counts, RNG use and trajectories differ wherever
a loss happens inside a continuation.

## Unchanged

- Default rules keep the historical transition; a test pins it.
- The flag is omitted from snapshots while off and `RuntimeRulesV1` is not
  hashed, so existing snapshots, goldens, replays and state hashes keep their
  bytes; `resolution-boundary-v1` serializes as before.
- Frozen studies and the tools that pin `resolution-boundary-v1` are
  untouched.
