# FDN private surveil windows

Lightshell Duo is a blue 3/4 Rat Otter for `{3}{U}` with prowess and an
enter-the-battlefield surveil 2 trigger. Cephalid Inkmage is a blue 2/2
Octopus Wizard for `{2}{U}` with an entry surveil 3 trigger and threshold:
it cannot be blocked while its controller has seven or more graveyard
cards. Token and spell-copy objects do not count toward threshold. Ability
removal suppresses its printed static ability and Duo's printed triggers.

Primary reference: pinned XMage card sources `LightshellDuo.java` and
`CephalidInkmage.java`, plus `SurveilEffect.java`. This batch reserves IDs
330-331 after the v61 mana/combat batch. Its catalog identity is pending
the preceding batch's integration and generated v62 hash.

The new continuation binds the complete library and definition-owned
standalone Surveil root. It privately looks at the top N cards once,
selects a graveyard subset in the owner's chosen order, then orders the
kept complement. No card moves until both orders are accepted. The kept
cards replace only the bound prefix; the unlooked tail remains unchanged.
This bounded batch supports the two standalone ETB roots. Nested Surveil
programs with structural paths or remaining frames require a later
continuation extension. Existing count-one prompts retain their contract.

Source provenance, count, full library order, candidate partition, kept
permutation, and answered-frame guards are checked before an accepted
move. Pending and answered stages serialize for restore. All stages use
one nonchooser public envelope, including before engine advance commits
accepted answers. The actor retains the real candidates and stage purpose.
Public and typed observation tests compare across hidden card changes,
every graveyard subset, both stages, and accepted answers.

Focused gameplay tests cover all three-card partitions and reversed kept
order, surveil two, empty/short libraries, private projection invariance,
restore at both pending and answered boundaries, source/count/program and
library tampering, unlooked-card rejection, departed source resolution,
threshold changes and ability removal, actual prowess, and count-one
compatibility. Source formatting and diff checks pass. Native and hosted
execution remain pending; no gameplay acceptance or hash is claimed yet.
