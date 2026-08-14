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

- `SelectDeckButton` opened `Modern Decks`. The chooser retained the currently selected deck when reopened.
- `Import Deck` opened the standard `Select Deck(s)` file dialog and then an `Import Deck(s)` review dialog.
- The import dialog defaulted its format to Standard even though it was opened from the Modern chooser. The format had to be visibly changed to Modern.
- The imported 60-basic-land deck appeared under Modern in Collection after the import dialog closed. Returning to the Modern Challenge deck chooser showed the exact deck label. Selecting it visibly changed the deck tile and populated the right-side `MAIN DECK (60)` detail pane.
- Invoking that visible `Submit` selected the deck for the event. `Please Select a Deck` disappeared and the exact deck label appeared on the event surface.
- Open Entry Review still did not appear after compatible deck selection. The ticket and play-point entry choices remained a separate gate. No entry option was selected, and the temporary import file was removed.

Safety finding:

MTGO's virtualized UI Automation tree reported some event-list descendants below the rendered client as `IsOffscreen = false`. A trusted visible-control producer must require positive-area bounds fully contained in the current client, not rely on `IsOffscreen` alone. The live actions above used that containment rule before selection.

Adapter consequence:

`MtgoVisibleCompetitiveDeckGateV1` now distinguishes:

- `awaiting_compatible_deck_selection`: visible selected event, enabled deck chooser, visible missing-deck prompt, and no Open Entry Review control.
- `compatible_deck_selected`: visible selected deck, missing-deck prompt absent, and Open Entry Review still unavailable.
- `open_entry_review_available`: visible selected deck and visible enabled Open Entry Review control, with the missing-deck prompt absent.

The contract is coordinate-private after validation and grants no input, entry, or spending authority. The event target binds the exact expected visible deck-label hash as well as the deck list, manifest, and format; a different displayed deck rejects. An `open_entry_review_available` value can be consumed directly into the existing selected-listing evaluation chain; neither earlier state can take that path. The dormant exact-deck actuator is now wired, but production still needs a reviewed classifier corpus for all three states and an exact correspondence-pinned authority commitment before it can interact with the client. Entry-option and Open Entry Review calibration remain separate.

## Selected-deck calibration candidate

A later signed-in read-only check found exactly one visible `mtgo-kernel-modern-basics-v1` label, zero `Please Select a Deck` labels, and zero `Open Entry Review` labels. A 1550 by 925 composed-desktop calibration preview retained that exact middle state with PNG SHA-256 `9edf61e2df4ccfbe1855159950a0555cb99d29a55310ee7138f45208cc9f8313`. Desktop Duplication timed out on the static Event Browser frame, so this fallback preview remains `pending_visual_review` and explicitly unsafe for semantic evidence, OCR, policy scoring, and input. It is not a production classifier case.

The classifier binary and runtime now implement an exact three-state deck-gate protocol. A target is structurally eligible only when its asset manifest carries one reviewed profile for each state. State profiles bind event OCR, exact expected deck-name OCR or exact missing-deck prompt OCR, exact enabled Select Deck or Change Deck pixels, and exact Open Entry Review available or unavailable pixels. Ambiguous, incomplete, substituted, or region-drifted profiles reject. The reviewed League and Challenge three-state corpus is still missing, so production classifier readiness remains false.

The same pinned binary and runtime now implement a distinct exact two-state deck-chooser protocol for the dialog reached from that missing-deck gate. A fresh Modern League transaction on MTGO 3.4.158.4691 established the natural state rule. The fresh `Modern Decks` dialog had an unselected exact deck tile, empty detail pane, and visibly disabled Submit. Selecting the exact deck changed the tile, populated `MAIN DECK (60)`, and visibly enabled Submit. The fixed-label offline OCR probe found the exact title but could not reliably recognize the wrapped two-line deck name. The protocol therefore binds the original gate, account, event, expected deck hash, complete newer frame, process/window continuity, exact title OCR, a reviewed exact deck-label pixel region inside the row, exact selected or unselected row pixels, the corresponding populated or empty detail pane, and enabled or disabled Submit pixels. An artificial UI Automation removal of a retained Challenge selection left stale enabled-looking Submit pixels; that state is not reachable through the normal three-control transaction and is excluded from calibration. Both reviewed natural state profiles are required and exactly one must match. The result is move-only and non-authorizing. The dormant actuator consumes that ownership in three stages: `Select Deck`, exact deck row, and `Submit`. It withholds all further MTGO input after each click until the exact newer visible state is confirmed. No reviewed League or Challenge chooser corpus exists, and the compile-pinned actuator authority root is empty.

The external League pending-review capture SHA-256s are `1865154A3B84EC0DBB97720D0133DE6C21FA110D4BA48076562EA467B1D9EBC0` for the missing-deck main page, `B8109095B9239AA949E7E8FA2CF5A0A4E2D701BEB85DDF347E312B33F828C3C8` for the fresh unselected chooser, `B339E16B46B47B56ED1060C7F7975CE720FCEB55527697EC054FA3155D0C5C41` for the selected chooser, and `156B0687EA48A749AFDD748C9307B029ED38942B5AAF73680F1DF45850B09CE6` for the compatible-deck main page after Submit. They remain outside the repository and are not admitted classifier assets. An ignored opt-in test over the exact unselected and selected PNGs passed on 2026-08-14: the real Windows OCR path found the bounded `Modern Decks` title and the corrected exact-region classifier matched exactly one of the two natural states for each frame. This confirms the wrapped-label correction against the retained frames but does not promote the frames or enable the actuator. The ticket and Play Point options remained untouched, and no fee, Open Entry Review, purchase, event entry, or match was invoked.
