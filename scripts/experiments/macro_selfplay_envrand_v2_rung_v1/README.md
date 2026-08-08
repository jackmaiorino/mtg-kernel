# Macro Self-Play Envrand-V2 Rung V1

Run from the repository root in this order:

```powershell
pwsh -File scripts/experiments/macro_selfplay_envrand_v2_rung_v1/preflight.ps1
pwsh -File scripts/experiments/macro_selfplay_envrand_v2_rung_v1/formal.ps1
pwsh -File scripts/experiments/macro_selfplay_envrand_v2_rung_v1/native-eval.ps1
```

The scripts refuse to overwrite evidence, require a clean pinned source for preflight and formal training, use physical GPU 1 only, run the three formal seeds sequentially to fit its 6 GiB memory, pin the Pool3 primary to true promoted(2) generation 384, and publish compact JSON manifests under `D:\mtg-kernel-macro-selfplay-envrand-v2-rung-v1`.

The external CP7 screen is intentionally separate. It runs only after the native summary selects the candidate seed.
