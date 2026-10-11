# Filtered library search

Micromancer's pinned Mage constructor at
a5c90fe180021e70e2a644ade00eeab07f857a40 is3U,3/3 Human Wizard.
Its ETB optionally searches its controller's library for an instant or sorcery
card with mana value1, reveals it, puts it into hand and shuffles. Declining the
search does not shuffle. Searching permits failing to find even when matches
exist. This source remains unregistered and adds no accepted coverage.

The new generic exact-mana-value instant/sorcery filter appends to the existing
library filter enum. It uses front-face printed types and mana value, with an
independent stable fingerprint tag7. Existing tags and reveal rules retain
their identities. The existing optional effect choice surrounds the existing
resumable, incarnation-bound, owner-only library search and reveal-to-hand path.
The definition trigger key records the complete search behavior. Rule features
mark the unsupported type disjunction/exact mana value as opaque. No old model
schema is changed and no new persistent state field is added.

A registered-card primitive covers spell type/mana value, owned candidates,
restoration and stale zone/face refusal. Five unregistered card groups cover
metadata and actual cast/search/reveal, decline and failure to find, nonacting
privacy across different hidden match presence, captured controller after source
departure/control change, and forged filter/fingerprint/candidate refusal.
A sixth group coherently changes the filter, fingerprint and legal candidates,
or the optional/answered program and remaining frames. Every restorable phase
must match the definition-owned root and captured controller before mutation.
Formatting and diff checks pass. Native type checking, all actual card groups,
catalog/profile admission, review, CI and default acceptance remain pending.
