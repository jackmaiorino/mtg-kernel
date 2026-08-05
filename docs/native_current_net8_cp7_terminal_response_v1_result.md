# Current Net8 CP7 terminal response v1 result

Status: complete, stopped by the frozen movement gate.

## Decision

The unique result is `STOP_CURRENT_NET8_CP7_TERMINAL_RESPONSE_V1`. The fixed
four-update candidate passed finiteness, Adam-step, L2, mean-TV, and p90-TV
requirements, but exceeded the maximum selected physical-decision log-ratio
cap. No candidate package was published and no fresh gameplay was run.

| movement metric | observed | required | result |
| --- | ---: | ---: | --- |
| Parameter L2 from retained GAE8 | 0.357244 | at most 0.75 | pass |
| Mean action TV | 0.0113081 | 0.010 to 0.050 | pass |
| p90 action TV | 0.0259013 | at most 0.150 | pass |
| Maximum absolute joint log ratio | 1.640879 | at most 1.0 | fail |

This closes only the exact `0.001` learning rate, `0.5` value coefficient,
four-update, fixed-parent PPO recipe on the revealed CP7 skill-7 corpus. It
does not show that terminal-only CP7 response training is ineffective.

## Training result

The run started from exact retained GAE8 Adam step `520` and ended at step
`524`. It used all 128 natural games and 4,769 decisions. P0 and P1 advantages
were centered and standardized independently with equal episode mass.
Terminal win, draw, or loss was the only reward and the value target.

The source transport audit was bit-exact on all 4,769 decisions. Final mean
TV cleared the activity floor, but the worst selected joint probability ratio
was outside the declared envelope. The report was written before the failed
publication decision and the candidate-package path does not exist.

Training diagnostics suggest the active value loss was not helping the policy
response. Value MSE improved from `0.668835` to `0.555405`, while selected
action NLL changed from `0.282648` to `0.286190`, physical top-1 changed from
`0.894800` to `0.893602`, and the rare selected-probability excursion grew
beyond the safety cap. These are development diagnostics, not playing-strength
evidence.

## Throughput and evidence

The optimized CPU run completed in `30.786` seconds. GPU 1 was idle at launch
and remained reserved. No additional throughput optimization was warranted
for a sub-minute update.

- Source commit: `56d5cdb33fddcc43e2531664c210941233889b27`.
- Attempt manifest SHA-256:
  `846873831578d4d6bb2f0cd4fada39a18dc37abd1741a2b4465abcf666fafb9f`.
- Training report SHA-256:
  `c0a9159c9109f293857d78983e892eef78313eed6a9c2d6faa3030fa4305ba57`.
- Evidence root:
  `D:\mtg-kernel-current-net8-cp7-terminal-response-v1\development\training-attempt-02`.
- Native Pool3 seed `1830001`, fresh CP7 seed `1840001`, and reserved CP7
  skill-8 seed `1850001` remain untouched.

## Next lane

Run a bounded movement-only v2 screen from the same source and revealed
corpus. Compare exact pure-policy and low-value-coefficient terminal updates,
selecting only by the unchanged movement window before any fresh gameplay.
This directly tests whether the auxiliary value fit caused the rare unsafe
policy excursion while preserving terminal-only credit assignment.

## Nonclaims

- Training movement and fit are not playing strength.
- CP7 skill 7 is now a training distribution for this lane.
- No native, fresh CP7, V3, human, or professional-strength result was
  produced.
