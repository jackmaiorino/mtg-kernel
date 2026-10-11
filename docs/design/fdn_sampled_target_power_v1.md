# Sampled target power

Bulk Up at pinned Mage a5c90fe180021e70e2a644ade00eeab07f857a40 is
1R instant, mana value2, double target creature power until end of turn,
with flashback4RR. This source is unregistered and adds no accepted coverage.

The appended generic target-power effect authenticates the target incarnation,
requires a current battlefield creature and samples signed effective power
once at resolution. It installs that amount as an ordinary layer7c boost,
with no toughness change. Zero produces no effect. Later changes are added
normally; another doubler samples the updated power. Zone changes invalidate
the boost and old target contract. Cleanup removes the existing temporary
record. Normal targeting and flashback admission/payment/exile are reused.

No persistent field or public/model schema changes. Existing effect ordinals
remain unchanged. The build recipe records sampling, signed power, binding
and zero behavior; flashback cost participates in the catalog identity. Rules
features describe a dynamic characteristic read and temporary stat change.
A registered primitive covers negative/zero/positive power, later changes,
restore and incarnation refusal. Five unregistered actual groups cover metadata,
normal casting/resolution/cleanup, stacking, flashback pips/exile/refusal and
blinked targets. Native type checks, gameplay, catalog/profile admission,
review, CI and default acceptance remain pending.
