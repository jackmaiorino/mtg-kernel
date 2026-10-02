# Natural-root sensitivity: response without argmax changes

Frozen g115 reproduced every original archived logit and value bit exactly at eight selected natural roots. All32 ablation outputs then reproduced byte-for-byte in a fresh process. No new games, training, or candidate selection. These are eight correlated hand configurations from one first pre-update Gates episode, all replacing token18 with Mountain77 at the lowest eligible own-hand node absent from action references.

The table reports max(delta logits)-min(delta logits), the largest change to any pairwise logit difference. It removes common logit shifts. Digest ablation zeroes96 state features; token ablation replaces one card, so their magnitudes are not comparable estimates of channel importance.

| Archived step | Menu | Original top probability | Token-only span | Digest-only span |
| --- | ---: | ---: | ---: | ---: |
| 1 | 3 | .528463 | .014238 | .699795 |
| 5 | 4 | ~1 | .023500 | .060530 |
| 7 | 3 | .999729 | .002609 | .088494 |
| 29 | 8 | .995170 | .035441 | .347858 |
| 42 | 2 | .503166 | .002374 | .007043 |
| 43 | 8 | .879403 | .037042 | .462515 |
| 46 | 3 | .504061 | .022764 | .087430 |
| 49 | 2 | .999765 | .010819 | .340609 |

Argmax changes: token0/8, digest0/8, combined0/8. There is a nonzero hand-token response on every root. No tactical labels exist for these roots, so unchanged argmax is not a failure and a changed argmax would not be success. The artificial inputs retain original menus; they are not legal counterfactual game states. One card identity and one episode cannot establish broad hand sensitivity, superiority of a representation change, or natural missed-lethal prevalence. The earlier Burn-slice negative and constructed two-Bolt miss remain separate evidence.

Build332cd419 completed196.999s; run/replay6.367s. Input SHA1118359076d4bb090ac56d3369fe634edbb0c500574572f5c9ecdc9a826571bc; result/replay SHAd74ceb45459b58220bf701a4570779d30e9b87d5b9ac75e3395bd3c7d17ed64e. Evidence E:/mtg-meta-recovery-20260921/natural-hand-channel-001/completion.json and analysis.json. Archived member SHA was verified before extracting roots; whole training zip was not rehashed. Analyzer python/tools/analyze_natural_hand_channels_v1.py verifies both output hashes. CPU only, BelowNormal, fresh owner checks, no paid allocation or production guard changes.

Disposition: this supplies a bounded natural-root channel read requested by E-12/E-13, with limited coverage; it does not resolve the missing tactical counterpart or choose a repair. New independent training n=0, no effect-size/power or ADVANCE claim. M1 unmet. Next substantive work should formulate competing, testable explanations and a conditional repair proposal using these limits, rather than interpreting more tiny ablations as playing-strength progress. Independent Fable review remains pending availability before a critical decision.
