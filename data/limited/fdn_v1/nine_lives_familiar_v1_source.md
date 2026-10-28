# Nine-Lives Familiar (kernel ID 458; FDN collector number 66)

Source: pinned Mage commit `a5c90fe180021e70e2a644ade00eeab07f857a40`, `Mage.Sets/src/mage/cards/n/NineLivesFamiliar.java` (constructor, lines 32-49; death trigger and delayed return, lines 66-104). The printing is `FDN:66`, Scryfall ID `988c23f6-59fe-49f9-a9ce-9881dccb7033`, Oracle ID `5049da27-f38b-4fe6-a387-51510bd1f44e`.

The card is 1BB for a 1/1 Cat. If cast, it enters with eight revival counters. When it dies with at least one revival counter, its death trigger creates a one-shot delayed trigger for the next beginning of an end step, for either player. That trigger returns the exact card incarnation from its graveyard if it remains there, under the captured ability controller, with one fewer revival counter. Counter-entry replacements apply to that reduced amount once. If the death trigger resolves after the current end-step beginning, the return waits for the next one.

This registration is explicitly Partial. The reference deck is a source fixture, not a claim that the card passes full-deck admission. Rust source fixtures are not executed by this checkpoint; no runtime, profile, or model admission is claimed.
