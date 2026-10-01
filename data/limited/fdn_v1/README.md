# FDN Limited input fixtures

These small fixtures come from [DraftZero](https://github.com/danieljbrooks/draft-zero)
at commit `b1e0ba6863182a612a1a06fbac9754cbc979e018`.
Decks and 17lands-derived card names retain DraftZero's
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) attribution to 17lands
and its contributing players. `card_names.json` is an adaptation containing
only the alphabetically sorted names and source provenance, with the source
statistics omitted. The two deck files are unchanged copies.

| Local file | Source path | Source SHA-256 |
| --- | --- | --- |
| `card_names.json` | `assets/reference/FDN_gih.json` | `ec6d09222c8a8ad14bf76edd91d38c8481e8931eb1aaf64db1901343279ea97f` |
| `FDN_top_04956_UG.dck` | `assets/sample/decks/FDN_top_04956_UG.dck` | `bb618d6eaddf04b0a9e51e9a88cd512a04635e99ebca91b11ace4c305d634c86` |
| `FDN_top_20626_WG.dck` | `assets/sample/decks/FDN_top_20626_WG.dck` | `be026f1c86e3aabcb294517188d0c5f4f0cdfa3c5e95ee9dedfc51d3cdf814f7` |

The 286 reference names are an observed gameplay dataset, not a complete
booster manifest. The inventory tool adds all five basic lands and any
additional fixture cards. After fixture batch A, 13 reference names are
supported and 273 remain missing. Both fixtures resolve 19 of their 40
mainboard copies; 30 distinct fixture names still need implementation.
Printed collector numbers are not a safe
filter for draft availability: these real decks also use alternate printings.
The full target pool, including any Special Guests, needs a separate manifest.

From the repository root, no dependencies or engine build are needed:

```powershell
py -3.11 python/tools/limited_decks_v1.py inventory --deck data/limited/fdn_v1/FDN_top_04956_UG.dck --deck data/limited/fdn_v1/FDN_top_20626_WG.dck
py -3.11 python/tools/limited_decks_v1.py inspect --deck data/limited/fdn_v1/FDN_top_04956_UG.dck
py -3.11 python/tools/limited_decks_v1.py resolve --deck data/limited/fdn_v1/FDN_top_04956_UG.dck
```

`inspect` and `inventory` succeed when they can report unsupported cards.
`resolve` exits 2 and emits no card ids if any mainboard card is missing,
partial, a token or `no_effect`, or the deck has fewer than 40 cards. It resolves
supported BO1 mainboards into stable registry ids in row/copy order. Sideboards
are reported separately and are never included in the BO1 mainboard. More than
40 cards and more than four copies are accepted. A registry-ready deck is not
a rules-parity verdict or a Limited-ready policy session.

Reports are generated on demand rather than committed. Their raw registry,
deck and names-file hashes identify the exact inputs. No engine registry,
frozen training manifest or existing session schema is changed by this tool.
