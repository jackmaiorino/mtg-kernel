# Live competitive pre-entry inspection, 2026-08-14

Scope: signed-in main account, MTGO 3.4.158.4689, visible Windows UI Automation only. No event-entry choice, Open Entry Review control, paid confirmation, purchase control, or event entry was invoked.

Observed League path:

1. Selecting the visible Modern format selected `Modern League` in the Event Browser.
2. The selected event visibly showed `Please Select a Deck`, `Choose Entry Option:`, `10 Event Tickets`, and `100 Play Points`.
3. The `SelectDeckButton` control was visible and enabled.
4. No `Open Entry Review` control was present. One displayed entry-option button was enabled, so generic enabled-button detection is unsafe.

Observed Challenge path:

1. The visible event list was scrolled through `EventBlocksListBox` until `Modern Challenge 64` was fully inside the client bounds, then selected.
2. The selected event visibly showed its start time/countdown, `Please Select a Deck`, `Choose Entry Option:`, `30 Event Tickets`, and `300 Play Points`.
3. The same deck-selection gate remained. No entry option was selected.

Deck chooser behavior:

- `SelectDeckButton` opened `Modern Decks`, with `Submit` disabled because no compatible deck was selected.
- `Import Deck` opened the standard `Select Deck(s)` file dialog and then an `Import Deck(s)` review dialog.
- The import dialog defaulted its format to Standard even though it was opened from the Modern chooser. The format had to be visibly changed to Modern.
- The imported 60-basic-land deck appeared under Modern in Collection after the import dialog closed. Returning to the Modern Challenge deck chooser showed the exact deck label. Selecting it visibly changed `Submit` from disabled to enabled.
- Invoking that visible `Submit` selected the deck for the event. `Please Select a Deck` disappeared and the exact deck label appeared on the event surface.
- Open Entry Review still did not appear after compatible deck selection. The ticket and play-point entry choices remained a separate gate. No entry option was selected, and the temporary import file was removed.

Safety finding:

MTGO's virtualized UI Automation tree reported some event-list descendants below the rendered client as `IsOffscreen = false`. A trusted visible-control producer must require positive-area bounds fully contained in the current client, not rely on `IsOffscreen` alone. The live actions above used that containment rule before selection.

Adapter consequence:

`MtgoVisibleCompetitiveDeckGateV1` now distinguishes:

- `awaiting_compatible_deck_selection`: visible selected event, enabled deck chooser, visible missing-deck prompt, and no Open Entry Review control.
- `compatible_deck_selected`: visible selected deck, missing-deck prompt absent, and Open Entry Review still unavailable.
- `open_entry_review_available`: visible selected deck and visible enabled Open Entry Review control, with the missing-deck prompt absent.

The contract is coordinate-private after validation and grants no input, entry, or spending authority. Production still needs a reviewed classifier corpus for all three states and a separately authorized exact-deck selection actuator with a visible postcondition before the entry-option and Open Entry Review stages can be calibrated.
