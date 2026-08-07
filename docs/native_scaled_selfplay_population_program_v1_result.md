# Native scaled self-play population program v1 result

Status: base campaign complete. Candidate 02 closed
`INCONCLUSIVE-AT-MAX-N` without confirmation. The optional extension is held
until the frozen response-exploiter contract is realized and the split
external-native frontier is reviewed.

## Base-campaign completion

All three matched lineages completed the authorized 1,024 active updates from
global generation 512 through generation 1,536. Stores remained complete,
finite, and reproducible. The final 1,408-to-1,536 interval completed 24,576
terminal-blind episodes in 9,144.221 seconds at 2.68760 episodes/s. Its
execution manifest is
`D:\mtg-kernel-scaled-selfplay-population-v1\active\interval-1408-1536\attempt-001\interval-execution-manifest.json`,
SHA-256 `80f22fbfe2ab428d093120b1701b676b08f805e01183836c1515b063d5d7bdc1`.

The complete generation-1,536 payoff matrix and refresh also passed. Their
authorities are:

- matrix manifest SHA-256
  `522f5dbfd02d2841896e673b8ce998ecb00507533cd7e8f4be7a6878f0efc03d`;
- payoff panel SHA-256
  `327f682b1947b57f7f8a81972312bb68e6bd482ba2fdd6966092738e1c14fba1`;
- refresh-008 SHA-256
  `9c9490b205b7b5a933eae7ca86916e5ff5ff9307a150dc35487a8e1c28e73e22`;
- refresh-008 weight update SHA-256
  `321f15e6d7adfb84e47c356144089afd333dd97dbea939c49f801554c426badb`.

## Native strength and the external-native frontier

The scheduled 1,024-pair native reads against promoted(2), each with a shared
matched control, were:

| Lineage | Generation 1,024 direct W-L | Paired net vs control | Generation 1,536 direct W-L | Paired net vs control |
| --- | ---: | ---: | ---: | ---: |
| 970001 | 1,034-1,014 | +32 | 1,033-1,015 | +42 |
| 970002 | 1,022-1,026 | +8 | 1,045-1,003 | +66 |
| 970003 | 1,037-1,011 | +38 | 1,028-1,020 | +32 |
| Combined paired net | | +78 | | +140 |

The generation-1,024 native result was already retained before the extension
decision. Its manifest SHA-256 is
`b95f0add5987d7cabdad46dfce89e18eca6d0c4418ef067067b17367bf7cda61`
and its result SHA-256 is
`a4633ab4cf36e387dcf60ab12606c77eca90dc0f7d8e3916153990de8037db02`.
The generation-1,536 manifest SHA-256 is
`b1689ca47e909a0634f74e9bfd555afe011af1f60d711b57fdefc3958fc1c463`
and its result SHA-256 is
`8419f849108ce8a1d3b9e5e67689feba5477723e45180dc2bf6c774b9b7069fb`.

CP7 moved in the opposite direction:

| Lineage | Generation 1,024 CP7 W-L | Win rate | Generation 1,536 CP7 W-L | Win rate | Change |
| --- | ---: | ---: | ---: | ---: | ---: |
| 970001 | 118-138 | 46.09% | 106-150 | 41.41% | -4.69 pp |
| 970002 | 117-139 | 45.70% | 108-148 | 42.19% | -3.52 pp |
| 970003 | 119-137 | 46.48% | 104-152 | 40.63% | -5.86 pp |

The CP7 summary SHA-256 values are
`6b5dc575380aba781aac78a2685968e5a6429c2bb602ca1f5e8155655684ddb6`
at generation 1,024 and
`ccdf6fc7c88d3ddce2b579acc95fdb37ef6916aad9ae65f2e474908c48eaed28`
at generation 1,536. Each decline is individually noisy at 256 games, but all
three lineages moved down while aggregate matched native strength rose. This is
the population-overfitting signature that the response-exploiter roles were
intended to pressure.

The frontier is therefore split. Generation 1,536 is better in aggregate on
the native matched anchor, while generation 1,024 is better for every lineage
on CP7. Neither checkpoint dominates the other, and no endpoint is selected
post hoc from these revealed measurements.

## Response-exploiter limitation

The real Net8 response-exploiter build lane was unavailable throughout the
base campaign. Both designated slots used the frozen historical-fallback
route. Refresh-008's fallback record SHA-256 is
`48f669b7f5725a8271f86bb1a3b912154707f73bb9434b85643d996295df0d17`.
This preserves a population-pressure test but supports no
exploiter-robustness claim.

Before any extension or second population campaign, the program must build
and evaluate the two distinct response exploiters required at program update
1,024. They start from promoted(2) with fresh Adam, train for 256 updates of 64
natural episodes against the exact frozen six-slot generation-1,536 mixture,
and must pass the predeclared mixture and pure-promoted(2) terminal W/L/D
gates. A failed build follows the documented fallback rule but does not justify
more training that continues to rely only on fallbacks.

## Candidate-02 formal result

Lineage 970002 at generation 1,536 was the fixed development nominee. Its
candidate-02 initial gate completed all 131,072 clusters, or 524,288 natural
games, in 7,474.334 seconds at 70.1451 games/s. The final V3 effect estimate
was `0.0085296630859375`, with confidence-sequence interval
`[0.0033928632690480853, 0.011348276520729561]`. Because the interval did not
establish the required 0.01 effect, the disposition is
`INCONCLUSIVE-AT-MAX-N` and confirmation was not launched.

The execution manifest SHA-256 is
`7446c215961749c7d3d74f217f7baa553701b63046f480a05ee36a06c7c0df4d`;
the retained full analysis SHA-256 is
`4b8ecd305f7beb98b86aa2e250e5e301c4e745148167aa91006581c6a55fd6d3`.
Independent reconstruction from all 64 receipts and 128 raw outcomes produced
the same 45,187,757-byte analysis byte-for-byte. This is positive evidence
against the fixed control, but it is not a promotion.

## Disposition

The base population mechanism produced the first three-lineage native endpoint
above promoted(2), but it did not produce a formally promoted model and its
external anchor deteriorated. The next campaign action is the missing
response-exploiter implementation and bounded qualification, followed by a
fresh extension decision under the unchanged terminal-only reward contract.
The optional extension does not launch from this result alone.

This remains native Rally BO1 evidence. It does not establish human,
professional, multi-deck, BO3, sideboarding, or metagame-wide strength. XMage
CP7 remains an external development anchor, not the runtime or a substitute
for formal promotion.
