# First FDN combat-card slice

This slice completes Beast-Kin Ranger and Overrun from fixture batch C.
Dwynen, Gilt-Leaf Daen's Elf bonus and attack trigger are implemented and
tested, but the card remains `partial`: the engine still needs a real
legend-rule choice before arbitrary decks containing it can be admitted.
The fixture goal remains two complete original decks, not this slice.

Rules references are the corresponding XMage card implementations at
`a5c90fe180021e70e2a644ade00eeab07f857a40` and the
[FDN release notes](https://magic.wizards.com/en/news/feature/foundations-release-notes).
Source inspection is not executable XMage parity evidence.

Beast-Kin Ranger observes each other friendly creature entry, including
tokens and simultaneous entries. Collection binds its temporary +1/+0 to
the exact source incarnation. Returning that physical card cannot receive
an older trigger's bonus. Boosts expire at the actual cleanup checkpoint.

Overrun uses a reusable effect that snapshots current controlled creatures
at resolution and installs exact-incarnation power, toughness and keyword
effects. Later entrants are unaffected. An affected creature keeps the
bonus after changing controller, but loses it after leaving and returning.
Trample goes through schema 3's damage allocation and prevention pipeline.

Dwynen's continuously recomputed bonus applies to other friendly Elf
creatures. Removing or changing control of the source immediately changes
the bonus, including lethal marked-damage checks. A declaration marker
creates its attack trigger; simply entering attacking creates no marker.
The trigger counts current attacking Elves when it resolves, retaining its
original controller even if Dwynen leaves. The marker is emitted only for
definitions with attack triggers, preserving older event streams.

The next Dwynen requirement is a deterministic player choice under rule
704.5j: choose one controlled legendary permanent among those sharing a
name, put the rest into their owners' graveyards without destroying or
sacrificing them, finish other state-based actions, then expose triggers
and priority. The choice must be public, resumable and restored exactly.
It cannot be approximated by keeping the lowest object ID. Until verified,
the custom-deck importer refuses Dwynen as partial.

Card IDs append after batch B. The default 162-card registry and its v32
identity stay fixed; the opt-in registry receives a distinct v35 profile.
Earlier v33/v34 records remain readable but publication, resume and
science-loop mutation require their identities to match the current build.
No native scorer vocabulary or training/evaluation design changes here.
