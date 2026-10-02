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
additional fixture cards. After fixture batches A and B and the combat and
legend-rule, targeted-spell, counter-creature, life-gain, draw, Horde, Koma, Kiora, Prowler, Rebuke, Voyage, Scavenging and Armor slices,
42 reference names have full registry support, one reference planeswalker
is partial and 243 remain missing. UG resolves 38/40 mainboard copies and
WG resolves 40/40. The original fixtures still
need Witness Protection. Dwynen now includes its Elf bonuses, attack
trigger and a real resumable legend-rule choice.
Printed collector numbers are not a safe
filter for draft availability: these real decks also use alternate printings.
The full target pool, including any Special Guests, needs a separate manifest.

FDN definitions live in `cards_v1.json` in this directory and append to the
unchanged 162-definition Pauper registry. The importer combines both files in
that order and reports their separate SHA-256s. Build the gameplay binary with
`cargo build --locked -p mtg-kernel --features limited-fdn-fixtures --bin kernel_limited_env`.
Default builds retain the original Pauper catalog and its v32 identity; the
Limited feature selects the appended definitions and their v48 identity.
The older v33 batch A, v34 batch B, v35 combat, v36 legend, v37 targeted-spell,
v38 counter, v39 life-gain, v40 draw, v41 Horde, v42 Koma, v43 Kiora, v44 Prowler v45 Rebuke, v46 Voyage and v47 Scavenging profiles remain readable and are
rejected for mutation
when they do not match the actual build.

`kernel_limited_env --foundations-combat-v1` selects custom-game schema 3,
with full priority windows, damage assignment choices and trample. The
Python client/tool accepts the matching `--foundations-combat-v1` option.
Simultaneous groups above seven triggers use `choose_trigger_order_next`
actions in bottom-to-top stack order. Each action includes the selected
prefix; placement occurs once after the complete order is selected.
See `docs/design/fdn_combat_damage_v1.md` for the rules and compatibility
boundary. Mulligans and the remaining fixture cards still need implementation.

From the repository root, no dependencies or engine build are needed for inspection:

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

`FDN_reference_planeswalker.dck` is a synthetic correctness fixture for
Bite Down's planeswalker recipient. Ajani has entry loyalty and damage
handling but no loyalty abilities or combat defender support. It is marked
partial and refused by deck admission. It is excluded from original-deck
coverage counts.
