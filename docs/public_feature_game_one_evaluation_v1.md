# Public evaluator game-one mode

`public_feature_evaluation_v1` accepts optional `"game_one_only": true` on its
command. Omission or false preserves the existing BO3 execution and serialized
match results. Keep the ordinary bounded match config, including
`max_physical_games >= 3`; setting that limit to one is not a game-one mode.

The opt-in path uses the same admitted models, adapters, opening protocol,
first environment seed, policy reset and natural-terminal runner as BO3. It
stops after the first physical game, including a natural draw, before another
game or sideboard callback. A non-natural ending still fails the case.

The existing `match-NNNNNN.json` filename holds schema
`public-input-evaluation-game-one/v1`, `game_one_only: true`, exactly one
`games` entry and no `outcome` field. There is no completed-match reward.
The completion receipt also marks `game_one_only: true`; its `natural_games`
count equals the number of completed cases.

`nine_deck_public_panel_v1.py build --game-one-only ...` opts in. The mode is
bound into the schedule digest, requests and qualification receipt, so BO3
and game-one throughput evidence are not interchangeable. The existing
guarded `launch qualify` and `launch run` paths apply. Rows score a natural
draw as 0.5 and reject game-one files containing extra games or a match outcome.
Completed and failed outputs must carry the matching native `start.json`
command. Recovery also verifies its original request and segment pins, so a
BO3 failure or an old continuation cannot be reused as a game-one case.

The unit checks cover default wire compatibility, first-game/seed/decision
prefix parity for V3/V4 policy routes, no second reset or sideboarding, draw
serialization/scoring and qualification mode mismatch. Actual T1/H admission
and runtime parity remain part of workload qualification. These checks are
engineering evidence, not a training or playing-strength result.
