# Live counter threshold for Skyknight Squire

Source preparation only. Skyknight Squire is unregistered and its actual
gameplay tests are unrun. Pinned Mage `a5c90fe180021e70e2a644ade00eeab07f857a40`,
`SkyknightSquire.java`, defines 1W, 1/1 Cat Scout. Another controlled creature
entering gives the source one +1/+1 counter. At three or more such counters,
the source has flying and adds Knight to its creature subtypes.

The entry trigger reuses the existing other-controlled-creature predicate and
the exact-incarnation source-counter effect. Tokens count, the source's own
entry does not, and the queued ability does not target anything. A source
leaving and returning cannot receive its old trigger's counter.

One common live descriptor supplies the threshold, keyword and subtype to
both the effective-keyword query and the two effective-subtype queries. It
requires the executable front-face source on the battlefield with printed
abilities active. Only the +1/+1 counter count satisfies the threshold.
The added subtype is recomputed; Cat and Scout persist and Knight disappears
as soon as the count drops. Ability-removing Auras suppress the static
ability and future trigger capture, while already queued counters still
resolve against their original source incarnation.

The generated recipe binds both abilities. RulesMeaningH records conditional
flying and a characteristic change, explicitly disclosing its counter/subtype
vocabulary gaps. Public card characteristics already use these common
keyword/subtype queries; no observation or tensor shape changes. Existing
subtype IDs, enum variants, state fields and historical definitions stay fixed.

Qualification requires both seats, actual cast/cost boundaries, creature and
token entries, opposing entries and noncreatures, effective subtype and
keyword observation, true flying blocker legality, source departure/blink,
controller changes, actual Witness Protection and saved-game restoration.
Catalog registration, compiled qualification, CI and default acceptance are
pending. No coverage, parity run or strength result is claimed here.
