# FDN draw creatures

Strix Lookout (ID 190), Mischievous Mystic (ID 191) and Faerie Token (ID 192)
append to the Limited feature. The original fixture files retain their exact
bytes. The catalog contains 193 definitions: 192 full and one partial.

| Card | Behavior |
| --- | --- |
| Strix Lookout | `{1}{U}`, 1/2 Bird, flying and vigilance. Its battlefield ability costs `{1}{U}` and tapping the source, works at instant speed, draws one card and then asks its controller to discard one. Summoning sickness, tapping and payment restrictions apply. The newly drawn card is a valid discard choice. |
| Mischievous Mystic | `{1}{U}`, 2/1 Human Wizard, flying. Its controller's second successful draw each turn creates one 1/1 blue Faerie with flying. Multiple-card draws remain separate events with their own historical ordinals. Opposing draws use the opposing player's count. Both players' counts reset at the actual next Untap boundary. |
| Faerie Token | Blue 1/1 Faerie creature with flying and no mana cost. It is a token, cannot be loaded into a mainboard, and its creation fires existing creature-entry triggers. |

The implementation reuses the existing typed `DrawThenDiscard` activation
recipe, discard continuation and `DrawNth(2)` matcher. Trigger collection
may queue Mystic's ability when the draw happens; the token is created only
after the resolving ability's discard choice finishes and the trigger
resolves. A queued trigger preserves its controller even if its source
leaves the battlefield.

The custom interface exposes the existing discard semantics. The real
40-card Lookout session restore check compares the complete response,
legal actions, environment binding and next transition at that choice.
No new serialized game-state fields or changes to frozen flat files are
required for these cards.

The live catalog is `kernel_carddb/v40`, hash `fbef8128c0ddad96`, read from
generated definitions. v39 remains `3f6b7e8df71f3195`; earlier profile
literals retain their original values and readability. Publisher, resume
and science-loop boundaries require the actual live identity.

The rules reference is [Comprehensive Rules](https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.pdf),
sections 121 (drawing cards), 602 (activated abilities) and 603 (triggers).
Matching XMage definitions at `a5c90fe180021e70e2a644ade00eeab07f857a40`
are `StrixLookout.java`, `MischievousMystic.java` and `FaerieToken.java`.

This batch raises declared fixture coverage to UG 34/40 and WG 34/40.
Nine distinct original-fixture names, mulligans, original-deck terminal
games and the remaining parity/CI checks still belong to the full goal.
See `docs/reports/fdn_draw_creatures_v1_validation.md` for executed checks
and pending verification.
