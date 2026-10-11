# Controller spell timing permission

High Fae Trickster is a 4/2 Faerie Wizard for {3}{U}, with Flying, Flash and
permission for its controller to cast spells as though they had Flash.
The primary reference is the pinned
[Mage constructor](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/h/HighFaeTrickster.java).

The reusable timing predicate checks a live executable front-face battlefield
source, its current controller and active printed abilities. It applies to
normal and alternate spell forms, including pending form selection. It grants
no keyword and changes neither land play nor sorcery-only activated abilities.
Witness Protection suppresses the static permission; departure and control
change update it immediately. The extractor records the permission and marks
the currently unrepresented timing relation opaque rather than calling it a
keyword grant.

This is unregistered source preparation after candidate v67. Registration,
generated identity, actual casting/restore fixtures, native checks, review,
CI and default acceptance remain pending. It adds no accepted card coverage.
