C6b call 4 reserves Doctor subtype ID 710. No token IDs are needed.

Night Nurse uses exact graveyard incarnations tagged with TurnV1.
Six extends the existing retrace permission and land-discard payment path.
Compilation and test execution are reserved for the coordinator.

Both cards are registered Full using the guide's existing-Full convention
exception. Six inherits Wrenn and Six's retrace restrictions: another
graveyard permission takes precedence, and a quote needing two independent
object-cost selections is not offered (for example land discard plus
Solitude's pitch-evoke or Abhorrent Oculus's graveyard-exile cost).
Wrenn's emblem shares this limitation for Force of Will. Native
flashback/escape routes retain the same precedence as with Wrenn.

The announced Six cast stores its exact stack incarnation, following
Underworld Breach's announced-escape record, so source removal cannot
invalidate an already cast spell. The attack uses native private mill
ordering followed by an optional public exact-incarnation land choice.

Call 5 adds Ursine Monstrosity as Full. No new token or subtype IDs.
It reuses native mill, Barrowgoyf's card-type counter, the temporary-boost
installer and Rabblemaster's required-attacker handling. A module-local
record binds the requirement to the source incarnation, TurnV1 and the
beginning-of-combat event index. Attack-target choices enforce the selected
player, including when that player controls a planeswalker. In the two-seat
engine there is exactly one opponent, so no random selection is necessary.

Pyrogoyf remains no_effect, with its printed base toughness corrected to 1.
Barrowgoyf supplies the CDA analog, OtherControlledCreatureEnters supplies
the other-entrant condition, and Bowmasters supplies the any-target shape.
The missing support is a damage-source binding to a creature other than
the ability source, with power and damage characteristics refreshed at that
creature's departure (including simultaneous departures), preserved after
blink or control change, and accepted by public resolution. The existing
EternalDamageLkiV1 loans are specific to Bombardment and Minsc & Boo; the
paid-cost power snapshot does not supply this entrant's damage identity.
No approximate Pyrogoyf program is installed.

The sub-wave public-session test uses all seven Full cards in both seats,
Ranked and Random policies, 12 original games and 12 exact replays. It
counts each card's observed casts and requires natural game completion.
Rules and session tests are written but unrun. No compilation, test run,
network command or commit was performed in this call.
