# FDN Brass's Bounty preparation

Brass's Bounty is a sorcery for {6}{R}: for each land you control, create a
Treasure token. The pinned
[XMage implementation](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/b/BrasssBounty.java)
samples controlled lands when its effect resolves. Tentative serial admission
is v70, ID344, after v63-v69.

Append `ControlledPermanentsWithType` to the existing dynamic value enum.
It scans live battlefield objects for current control and effective type,
including tokens and copies regardless of ownership or stored seat bucket.
Existing `CreateTokensDynamic` samples that value once before creating the
untapped, fully supported Treasure tokens. Its fixed rules features can
represent the controller's typed land count exactly. Historical enum
discriminants, state fields, profile pins and admitted registries stay intact.

A primitive regression checks both controllers, borrowed control, zones,
artifact lands, tokens and JSON restore. Three prepared card tests cover
metadata/payment, zero and 257-land counts, opponent/nonland exclusion, and
resolution-time board/control changes with pending-stack restore. These
checks have not executed. Registry, fixture, generated catalog/profile and
native gameplay qualification remain pending serial admission. Read-only
review of source2135882f found no actionable defects; its restore-coverage
note is addressed by requiring a finalized spell and a priority decision
before board changes and serialization.
