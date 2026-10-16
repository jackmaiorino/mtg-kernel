# Fiend Artisan native prerequisites

Source-only work for issue110. Fiend Artisan is not registered here, and this
change grants no activation, deck, consumer, or Full capability.

The pinned Mage source at `a5c90fe180021e70e2a644ade00eeab07f857a40`,
`Mage.Sets/src/mage/cards/f/FiendArtisan.java`, and the
[official FDN release notes](https://magic.wizards.com/en/news/feature/foundations-release-notes)
give the cost as `{B/G}{B/G}`, printed 1/1 Nightmare. Its graveyard count is a
battlefield-only +N/+N modifier, not an all-zone characteristic-defining
ability. Its sorcery-speed activation costs `{X}{B/G}`, tap, and sacrifice
another controlled creature. It searches for a creature card with mana value
at most X, puts it onto the battlefield, then shuffles.

Implemented prerequisite: append-only `LibraryCardFilter::CreatureManaValueAtMost`
uses the original exact library/incarnation validation, native u16 mana-value
ceiling, existing name-order candidates, and fingerprint tag11. The old meaning
vocabulary marks the unrepresented ceiling Opaque. One explicitly labelled
native primitive fixture checks zero/one/two/full-width ceilings, both seats,
serialization, stale incarnation, and alternate-face refusal. It does not
construct a printed Fiend activation or claim payment.

Remaining implementation seams:

- Freely chosen activated mana X needs its own pending choice and independently
  recorded paid-X binding. Existing activation X is either target-derived
  artifact mana value or loyalty X. Neither authenticates this activation.
- Existing payment X is u8. Supporting a larger selection requires an explicit
  payment-width extension; silently truncating or saturating X is incorrect.
- The checked layer7c modifier needs current controller's actual creature cards
  in graveyard, excluding tokens and virtual spell copies. Zone-batch final-PT
  preflight must include this dependency before mutations and SBA.
- A runtime-X search leaf must derive this exact filter from the authenticated
  paid activation, not a caller-supplied fixed ceiling.
- Genuine copied Fiend tokens need closed native copy permission through the
  actual Self-Reflection, Electroduplicate, and Replication paths. Generic raw
  Partial-token creation must remain refused.

Registration and genuine paid printing fixtures wait for real481 and482 before
the contiguous483 append. Existing registry and frozen pool are unchanged.
All Rust remains uncompiled and unrun; pinned formatting and static checks are
the only local verification.

Rules references: [CR 20260619](https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.pdf)
613.4c (layer7c), 602.2b (announcement/payment), 602.5a (tap-symbol sickness),
701.23b (qualified hidden-zone search may find nothing), and 701.23e (reveal).
