# Morbid controlled-creature counters

Needletooth Pack is prepared without registration. Pinned Mage
`a5c90fe180021e70e2a644ade00eeab07f857a40` defines a 3GG 4/5 Dinosaur:
at the beginning of its controller's end step, if a creature died this turn,
put two +1/+1 counters on target creature that player controls.

The definition reuses the existing morbid capture condition, resolution-time
conditional, controlled-creature target and counter effect. Generic trigger
authentication, captured controller and target-incarnation checks apply.
No effect enum, observation, state field or catalog membership changes.

Five unregistered gameplay groups cover metadata and actual casting; real
own/opposing/token deaths and false positives; controlled targeting and
restore; source departure, controller changes and target invalidation; and
ability removal plus forged pending roots. Both seats are covered. Formatting
and diff checks are preparation. Compilation, gameplay and admission remain
pending; no evaluation, training or strength result is claimed.
