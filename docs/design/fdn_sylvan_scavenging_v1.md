# FDN Sylvan Scavenging

Sylvan Scavenging is a green enchantment for `{1}{G}{G}`, mana value
three. At the beginning of its controller's end step, its triggered
ability offers two printed modes: put a +1/+1 counter on target creature
that player controls; or create a 3/3 green Raccoon if that player controls
a creature with power at least four.

Reference: XMage `Mage.Sets/src/mage/cards/s/SylvanScavenging.java` and
`Mage/src/main/java/mage/game/permanent/token/RaccoonToken.java`.
The Raccoon is green, a creature with subtype Raccoon, 3/3, nonlegendary,
and has no printed abilities. Its token definition cannot be admitted
as a physical deck card.

Mode selection occurs while putting the trigger on the stack, before
choosing its mode's targets. Selecting a generic effect branch during
resolution would change priority and target-loss behavior. Extend the
existing ordered pending-trigger placement state machine with a public,
resumable mode decision. Preserve APNAP and each controller's ordering
before presenting the first trigger's mode and targets.

Authenticate mode effects and target specifications against the immutable
source definition. The counter mode requires a legal controlled creature.
The token mode remains selectable without a creature of power four,
including an empty battlefield: its power condition is checked only on
resolution, not as an intervening-if trigger condition. Source removal
after triggering must not erase the independently resolving ability.

Prefer a definition-owned selected branch carried by the existing pending
trigger effect over adding unconditional hashed state to every older
trigger. If a new decision/action is needed, append its enum variants and
provide its legal actions and public custom-session projection together.
Preserve every existing default hash and serialized decision meaning.

Required cases: exact three-mana payment and both green pips; trigger only
on its controller's end step; both modes with no, small and large creatures;
illegal counter targets excluded; mode selected before targets and priority;
power gained or lost in response; correct token and entry-trigger effects;
target loss and reentry; source removal; simultaneous trigger ordering;
invalid actions without mutation; restore before mode, during target
selection and after either answer. Compare the rules cases against strict
XMage tests, then verify catalog history, affected regressions, default
compatibility, lint, natural external completion/replay and CI.

The enchantment and token append at IDs 202/203 under FDN v47,
`874200c08d207e29`. Both original deck hashes are unchanged. Registry
coverage is now UG 38/40 and WG 39/40. Celestial Armor, Witness Protection
and London mulligans remain required by the full fixture goal.

Implemented placement-time modes, exact selected-branch target contracts
and a resolution-time current-power predicate. All twenty-one focused rules
and pending-decision restoration cases pass. All 46 definition and 141 catalog-history checks pass, with three existing
ignores. Twelve strict XMage comparisons and twenty Python checks pass.
Limited all-target Clippy passes with warnings denied. Broader regressions and default compatibility pass. Two external smoke
games complete naturally with matching replay hashes and both trigger modes.
Release boundaries and remaining hosted CI jobs are pending.
The existing flat mode row represents both printed spell and trigger modes;
engine decisions and executable actions distinguish ChooseTriggerMode.
No new unconditional pending-trigger field changes older state hashes.
Routine rules engineering only, with no training or playing-strength claim.
