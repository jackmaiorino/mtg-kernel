# Life-payment Ward

Sire of Seven Deaths costs {7}, is a 7/7 Eldrazi and has First Strike,
Vigilance, Menace, Trample, Reach, Lifelink and Ward, pay 7 life. The primary
reference is its [pinned Mage constructor](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/s/SireOfSevenDeaths.java).

Front-face life Ward uses an explicit Boolean payment choice bound to the
trigger's original permanent contract and exact targeting stack incarnation.
An affordable payment uses the existing life-payment event; payment equal to
the payer's life is legal and leaves state-based actions to determine the loss.
Declining or inability to pay counters only that bound targeter. Departure of
the Ward source does not remove an already triggered payment. Root, choice,
answered frame and guard validation preserve the bindings across restoration.
Existing generic-mana Ward and back-face automatic life Ward retain their
costs and behavior. New enum variants are appended to preserve old identities.

This is unregistered source preparation. Actual Sire fixtures remain
unexecuted until admission. Legacy and V6 observations explicitly refuse this
new payment until a typed public life-payment projection and its consumers
are qualified. Registration, profile identity, native qualification, review,
CI and default acceptance remain pending. No accepted coverage is added.
