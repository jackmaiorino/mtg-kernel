# Current-main post-attack replay compatibility

The identical actual attack fixture was observed on accepted main2f55986 and
PR224 headad53, under Default and Standard, in both seats with a retained
priority decision and snapshot restore. Full serialized session-state and
hash artifact bytes match between those sources for each catalog. The small
control manifest is fdn110-current-base-hash-control-manifest.json; immutable
outputs and failed Native27 are retained in E:/storage-records/fdn110.

Default keeps both accepted a957 diagnostic/core pins and its original artifact
schema. Standard on accepted main has new hashes after Standard completion:
P0 diagnostic0b888429a46bd910/core4c26f4054b11987a;
P1 diagnosticdfb303462e23d089/core1c6f5f450ebca677.
An explicit current-main Standard regression uses that accepted base and its
own artifact schema. Historical a957 Standard source/receipts remain preserved;
they are not rewritten or claimed to match the new Standard catalog.

The failure is an incompatible historical test baseline after a base change,
not a Raid-byte difference in the matched fixtures. Current-head CI and guarded
requalification of the repaired regression remain required for delivery.
This replay check establishes no fresh-policy determinism or playing strength.
