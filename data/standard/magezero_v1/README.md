# MageZero Standard input fixtures

The 16 decks in `decks/` are MageZero's Standard opponent pool, copied unchanged from `xmage/decks/` in the
[MageZero `v0.2.0-alpha` release](https://github.com/WillWroble/MageZero/releases/tag/v0.2.0-alpha)
(`magezero-xmage-v0.2.0-alpha.zip`). Their names match the `opponents` list in MageZero's
`configs/run.yml` at commit `f7445e5ce9f42f4087bcf20c08e92ce1a71eec72`. MageZero is
MIT licensed, Copyright (c) 2025 Will Wroble.

| Deck | SHA-256 |
| --- | --- |
| `Standard-MonoB.dck` | `f0019124eef0f11a5458b7289344bd4dfbc99fc0fa1343c30ac93112594f17a6` |
| `Standard-MonoG.dck` | `093d5c57c20ccaf79c48b8707712a0e260eb578a148f31b864610e17e167f50b` |
| `Standard-MonoR.dck` | `68dac8d49eb93096eee27e5db2582f441334db0bc14345a69894ff089593e386` |
| `Standard-MonoU.dck` | `e2f14a69fa9a36a0fe7e57bc803ec0034efecfdcd6c6ea8736c8f4ceba35b02b` |
| `Standard-MonoW.dck` | `b8b416e6acd7e3004a364ace019a5d38d8bc7f0d9c23981efa0dd365e3213ed6` |
| `Standard16-5C.dck` | `95f90fd18ebac5d293166f4dddef71e93172ffc076f4e461d1b528011c66e6c8` |
| `Standard16-BW.dck` | `aa160d0ebd6c9aceb9463db60ce9c92713fe5d2254b4674c95ae060a180ec86b` |
| `Standard16-GB.dck` | `5005a49d5f33498a904c234fae3f18fa1e0117f406f3778cf487033514fe2f76` |
| `Standard16-GW.dck` | `d04f29eff80ecf1d03cc1382bc061f11ad1428685f2997db3c21cb56f6f215e6` |
| `Standard16-RB.dck` | `183d4f053aada5112ff42cbcf13e827fe37917d3cb82dd68ab6c8912ce852212` |
| `Standard16-RG.dck` | `5eeae760a2f7c85117717e8153dd54104a400e2537f18f25b343d1af6b174edd` |
| `Standard16-RW.dck` | `b32cd25712aa472e75851696e9d944a31184a5402485825923fda5c7a0677a13` |
| `Standard16-UB.dck` | `246accf8e03e0e1d07fbadef6631118d5b7b06f548216a8e90c07c6e3bb73c00` |
| `Standard16-UG.dck` | `2d604993eaecdd0a91550d1bdfdf54e65421877f315da751e6f7f2d66f83d444` |
| `Standard16-UR.dck` | `9fe5248d3c83db907094f0d729c66a32bc5be0aa746adc1c480e729654813fe9` |
| `Standard16-UW.dck` | `63a3e09868434432e532aeec8562c7d6ea49be2f377a00dd22518a70715b30c3` |

`card_names.json` lists every distinct nonbasic card in those decks. `cards_v1.json` holds the
Standard definitions appended after the unchanged 192-definition Pauper registry: Plains and
Burst Lightning, whose behavior is shared with the FDN build, then each Standard batch in merge
order. FDN definitions are not included, so FDN batches never move Standard card ids.

The completion candidate uses `kernel_carddb_standard/v7`: 253 appended definitions,
including 32 token or masked-face definitions, after the frozen 192-definition Pauper prefix.
All 225 distinct nonbasic cards have source implementations and Full admission flags.
Python fixture checks resolve all 16 decks, preserving the 62-card Mono-U fixture and empty
sideboards. These admission checks do not establish complete runtime support. Native
compilation, affected behavior tests, the observed v7 catalog hash, and deterministic terminal
public-session/replay validation remain pending in
[`standard_completion_v1.md`](../../../docs/reports/standard_completion_v1.md).

Build with the pinned toolchain and the host's supported guarded launcher:
`cargo +1.94.1 build --locked -p mtg-kernel --features standard-magezero-fixtures`.
The feature also enables `limited-fdn-fixtures` for shared rules behavior, but `build.rs`
appends this Standard file instead of the FDN registry. Run the Standard catalog, card,
choice and observation tests selected by CI; FDN-only catalog and card tests use a separate
build. The frozen Pauper and FDN registry files and IDs are unchanged.

Inspect fixture admission without a native build:

```
python -m unittest discover -s python/tests -p test_standard_decks_v1.py
python python/tools/limited_decks_v1.py inventory --registry-extension data/standard/magezero_v1/cards_v1.json \
  --card-names data/standard/magezero_v1/card_names.json --deck data/standard/magezero_v1/decks/Standard-MonoR.dck
```

[`standard_magezero_inventory_v1.md`](../../../docs/reports/standard_magezero_inventory_v1.md)
keeps the mechanic-family index. Generic public observations carry Standard state, including
poison, restricted floating mana, new choices and effective face/Room identity. Frozen Pauper
model encoders explicitly refuse unsupported Standard state; new Standard policy training
is outside this engineering task.
