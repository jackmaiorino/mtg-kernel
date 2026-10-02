# Control endpoint vectors exported for power preparation

Validated the original pinned reader, plan, endpoint requests, binaries, match hashes, 11,264 match records and published endpoint totals, then exported only the existing outcome vectors. Shape64matchups x8environment-seed replicates x2seats x11endpoints x2metrics (BO3 win and game-one win). All11endpoints share each of512distinct seed pairs. Reproduced between-run SD0.762650227296156pp for the10same-config endpoints. New training n=0, new games0. Export took7.765s.

Evidence E:/mtg-meta-recovery-20260921/control-endpoint-vectors-001/{values.npy,seeds.npy,completion.json}, with axis/endpoint/deck-pair identities and source hashes. Reader python/tools/export_control_endpoint_vectors_v1.py reuses only pinned validation/assembly functions, not the original normal-power or disputed covariance-subtraction functions. It rejects optimized Python because the historical validator uses assertions. Original artifacts are read-only.

This is power-analysis input, not a power result or launch permission. A future analysis must specify treatment assumptions and the complete gate, preserve paired seats and shared endpoint covariance, and assess treatment variance/sensitivity. Observed control vectors do not identify the search controller's effect or response heterogeneity.
