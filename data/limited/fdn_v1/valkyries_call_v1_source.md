# Valkyrie's Call Partial source

Kernel ID 459 corresponds to the FDN printing `Valkyrie's Call` (collector 27, Scryfall `0e1f1ff2-fa8f-4d38-b631-2d6e08e614c8`). The pinned Mage source is commit `a5c90fe180021e70e2a644ade00eeab07f857a40`, `Mage.Sets/src/mage/cards/v/ValkyriesCall.java`.

The enchantment costs `{3}{W}{W}`. Its trigger sees a nontoken, non-Angel creature controlled by the Call's controller die. It returns that physical card under its owner's control, puts a +1/+1 counter on that entering incarnation, and continuously gives that incarnation flying and Angel in addition to its other types. The counter is installed as part of entry and uses replacement effects for the actual entering controller. The type and keyword grants end when that incarnation changes zones.

This registration is Partial. Runtime/profile admission and model support remain unclaimed.
