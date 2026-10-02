# FDN Celestial Armor

Celestial Armor is a white artifact Equipment costing `{2}{W}`, with flash.
Its targeted entry trigger attaches it to a creature its controller controls,
then grants that creature hexproof and indestructible until end of turn.
The equipped creature gets +2/+0 and flying. Equip costs `{3}{W}` and follows
sorcery timing. Reference: XMage `CelestialArmor.java` and
`EntersBattlefieldAttachToTarget.java`.

Use the existing exact Equipment attachment operation, definition-owned
targeted trigger, temporary keyword grant and Equipment static profile.
The temporary protection belongs to the target incarnation independently of
the attachment. If Armor leaves before the trigger resolves, the creature
still gains protection. If the target leaves or becomes illegal, no part of
the trigger resolves. A returned Armor is a different source incarnation.

Acceptance cases: artifact admission and exact costs; flash and equip timing;
controlled targets and an empty battlefield; entry attachment and bonuses;
source removal or return before trigger/equip resolution; target removal or
return; protection surviving source removal and re-equip; temporary cleanup;
hexproof targeting and indestructible damage/destruction behavior; restore
during trigger targets and equip targets. Preserve default v32 and catalog
history, then check strict XMage cases, affected regressions, external natural
completion/replay and hosted CI.

Armor appends at ID 204 under FDN v48. The original fixture bytes remain
unchanged. A fully supported WG deck is the coverage target for this slice;
UG still needs Witness Protection. London mulligans remain required by the
overall fixture gameplay goal. This is bounded rules engineering.
