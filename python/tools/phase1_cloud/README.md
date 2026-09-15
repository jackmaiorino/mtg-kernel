# Phase 1 cloud execution tools

Canonical source for offline payload packaging, exact artifact comparison, CPU workers, export/recovery and bounded leases. `baseline-provenance.json` records the byte-identical starting copy from the preserved experiment tools. Those original tools and all historical payloads/results remain unchanged.

Run these files as scripts, or add this directory to `PYTHONPATH`. The tools have no provider credentials embedded in their source. Packaging is offline and does not allocate a Pod. Campaign execution and paid allocation remain paused under the project instructions.

`pack.py` prepares ordinary training and frozen evaluation payloads. `pack_bo3.py` prepares a BO3 stream beginning with an ordinary checkpoint for every actor. Runtime build commit/archive pins identify the native binaries; payload helper pins independently identify these execution tools. Fresh initialization provenance retains the original manifest and parameter bytes even when descriptor paths move to Linux.

The lease, funding, numerical compatibility, throughput and release gates still apply. A passing offline test or local Linux replay does not qualify a paid RunPod burst. See [the fresh-origin design](../../../docs/PHASE1-FRESH-CLOUD.md) and [Phase 1 plan](../../../docs/phase1_pauper_meta_competence.md).

Focused suite, with temporary storage on a permitted volume:

```text
python -B -m unittest discover -s python/tools/phase1_cloud -p "test_*.py"
```

Tests marked for actual retained artifacts require the associated local evidence. Preserve their explicit skips when those files are unavailable. This suite does not execute the MTG native binaries or allocate cloud resources.
