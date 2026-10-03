# Natural hand-burn resource audit

Completed 2026-09-22. Engineering inventory, no strength gate or model update. All five nominal hand-dependent records from the consumed g115 panel are retained; they contain four distinct counter-free visible positions. This is descriptive selection from an existing archive, not an independent prevalence sample.

The engine asks for spell targets before collecting the pending spell's costs (`mtg-kernel/src/engine.rs`, `drain_pending_cast_or_decide`, target choice around line 7580, cast mode and sacrifices thereafter). Therefore the root's lands and mana must fund BOTH the pending spell and any proposed follow-up. A sum of card damage is not a legal-win certificate.

| Record / decision | Public resources | Immediate resource assessment |
| --- | --- | --- |
| cell09-r1-s0 / 317 | Fireblast pending, Fireblast in hand; three untapped Mountains, empty mana pool; opponent 7 life | Nominal 8 damage cannot fund two two-land alternate costs from three lands. Three sources cannot fund the normal six-mana cast either. |
| cell25-r0-s0 / 227 | Lava Dart flashback pending, Fireblast in hand; five Mountains; opponent 3 life | Enough lands for the one-land flashback and two-land alternate costs. Policy already selected face. Retain as a potential positive control; actual stack responses remain unverified. |
| cell29-r0-s1 / 250 | Bolt pending, Galvanic Blast in hand; one untapped Mountain, four artifacts, no floating mana; opponent 7 life | Metalcraft is present, but the sole current red source is needed for Bolt. No immediate second red payment without generating new resources. |
| cell45-r0-s0 / 113 | Bolt pending, Galvanic Blast in hand; two untapped Great Furnaces and an untapped Mountain, four artifacts; opponent 5 life | Current resources can pay both red costs. Policy selected Burning-Tree Emissary. Candidate for engine verification. |
| cell45-r0-s1 / 146 | Same counter-free visible position as preceding row | Retained duplicate, not another independent witness. |

These are immediate resource constraints, not proof that no other same-turn or eventual winning line exists. Galvanic Blast deals four with metalcraft and two without; after a face Bolt against five life, even two would suffice if it resolves. A certified hand-using continuation would not by itself prove that hand use is necessary or that the policy's creature target loses.

## Evidence and next target

Read-only script: `python/tools/inspect_hand_burn_resources_v1.py`. Evidence: `E:/mtg-meta-recovery-20260921/hand-burn-resources-001.json`, SHA-256 `8efc9b66397425135b416e2c694e98a1165b3f4d7256da2eacfcab2f271cf0c7`. Archive hashes are checked against the prior candidate inventory; input and card registry hashes are recorded. No native engine, training, GPU or paid execution occurred.

Select the canonical cell45-r0-s0-g115, game array index 1 / native game 2, decision 113, actor p1, before branch outcomes. The existing Bolt/Island tree cannot certify a hand follow-up because it treats casts and subsequent Galvanic targets as unknown. A separate opt-in diagnostic must support that public continuation explicitly, retain every opponent alternative, and preserve information and budget boundaries. Do not silently broaden or rerun the completed cell25 oracle.

The monitor's E-20260922-07 remains binding. Previous immediate-Bolt negative controls passed, but Fable review is unavailable after HTTP 429 and that witness remains provisional. No training label or intervention is adopted here. g115 is unchanged; M1 is unmet.
