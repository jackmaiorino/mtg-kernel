# Review record: London mulligans for the default Pauper game (v1)

Reviewers: Fable 5.1 at max effort (read-only adversarial subagent) and GPT-6
Astra at ultra effort (`codex exec -s read-only`). Each round read the design
and the code at main 007adfdfd.

| Round | Fable | Astra |
|---|---|---|
| 1 | Changes 1-6: session-owned completion boundary, second announcer's view of the first announcement, pregame row accounting, refusals for other episode fields, scope of the byte-identity claim, `event_history` | Changes 1-6: pregame facts visible to the value head, narrower column-15 claim, forced-keep completion, frozen-opponent feature generation, search path or explicit deferral, automatic-action accounting and telemetry |
| 2 | Proceed after changes 1-3: tested refusals and Serum Powder fail-closed, named production encoder paths, event-history outcome rule | Change 1: `has_lost` can be set at a scored gameplay decision (Highway Robbery draw continuation), so its weights may be trained |
| 3 | Proceed (confirmed the Astra counterexample in source) | Proceed |

Resolutions are in revision 3 of `pauper_london_mulligans_v1.md`. The
priority-after-loss behavior Astra found is an existing engine defect, left
unchanged here because fixing it would change frozen studies.
