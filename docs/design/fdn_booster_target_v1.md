# Foundations Play Booster target

Jack selected the full Foundations booster set as issue #110's target on
October 9, 2026. `data/limited/fdn_v1/booster_pool_v1.json` freezes 286 unique
English names: 276 FDN names and ten Foundations Special Guests, SPG 74 through
83. The five basic land names are included. Art and foil variants share one
gameplay identity. Required token metadata is recorded separately, including
distinct Dragon and Cat variants and copy tokens. Separate canonical hashes
freeze both the name membership and the printing/token dependency metadata.

[Wizards' product guide](https://magic.wizards.com/en/news/feature/collecting-foundations)
defines the main-set, land and Special Guests slots. Scryfall supplies printing
identities, with response hashes retained in the manifest. The 276 FDN names
were independently compared with `Foundations.java` on XMage master, using its
explicit Play Booster printing range. All 286 names match the existing pinned
DraftZero reference exactly. That historical reference and its source hashes
remain unchanged; membership equality was verified instead of assumed.

Starter Collection, Beginner Box and Jumpstart exclusives do not enter this
target. Special Guests that can appear in Foundations Play Boosters do. This
target concerns supported gameplay from built decks; pack collation, drafting
and deck construction remain separate tasks.

Run `python python/tools/fdn_booster_pool_v1.py` to validate frozen membership
and report registry admission. The default `limited_decks_v1.py inventory`
uses this product manifest; `--card-names data/limited/fdn_v1/card_names.json`
still selects the historical reference. At main `faf1045a4`, coverage is 127
full, one partial and 158 missing names. Admission metadata cannot establish
rules parity or playing strength.

Completion requires all target cards and their token/copy dependencies,
focused interaction and pending-choice restore tests, representative
cross-color rules comparisons, fair Limited search, and the existing
DraftZero integration acceptance criteria. Catalog versions merge serially;
parallel mechanic work must reserve distinct primitives and append IDs only
at integration. This manifest does not authorize an experiment or paid run.
