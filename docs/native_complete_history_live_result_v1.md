# Native complete-history live result v1

## Result

The fixed complete-history candidate failed its live mechanism gate and is
retired.

Both formal arms completed all 32 legs against XMage CP7 with identical
environment seeds and candidate seats. There were no draws, scorer fallbacks,
selected-action projections, identity failures, protocol failures, or missing
legs.

| Arm | Wins | Losses | P0 wins | P1 wins |
| --- | ---: | ---: | ---: | ---: |
| Complete-history candidate | 14 | 18 | 6 | 8 |
| Retained parent | 15 | 17 | 6 | 9 |

The paired comparison contained 2 gains, 3 losses, 12 shared wins, and 15
shared losses. The required gain condition was `G >= L + 2`; observed was
`2 >= 5`, so it failed. Candidate-minus-parent wins were 0 at P0 and -1 at
P1, so both per-seat non-regression checks passed.

The predeclared 32-pair extension at base seed `1190001` is not authorized and
was not run.

## Interpretation

The representation learned the held-out corpus very strongly, including large
seat-balanced policy-agreement gains and improved terminal-value error, but
those offline improvements did not transfer to a measurable live Rally gain
under direct policy-logit action selection. This rejects the fixed candidate,
not public action history as a general modeling idea.

The next experiment should not repeat this fit or spend more games on this
package. The useful remaining asset is its parity-checked value head and
structured state, which can support a bounded selective-search mechanism
screen while preserving terminal win/loss as the sole reward and promotion
measure.

## Primary artifacts

- Manifest: `D:\mtg-kernel-complete-history-live-v1\live-gate\manifest.json`
- Candidate outcomes SHA-256:
  `8bbedccedadd166894a74262e5f68766ae745ef75aade2a41c122188f8dba206`
- Retained-parent outcomes SHA-256:
  `d5bb6b2559b9a5f4920f816cdc05464f799659654e1259b2a8840adaa1f44b17`
- Candidate log SHA-256:
  `7fc7b53c8fc9333a599b56106af6d2f6fc396042fa09f30403dd070e03153264`
- Retained-parent log SHA-256:
  `0f78d817cc3f32928e220d489dfbf55417bdcf7a85963f1f11a86de074644cea`

## Non-claims

This result does not establish strength outside Rally mirrors, measure
professional-level play, reject structured models generally, reject recurrent
history generally, or test value-guided search. It tests only the fixed package
and direct policy-logit selector declared in
`native_complete_history_live_mechanism_v1.md`.
