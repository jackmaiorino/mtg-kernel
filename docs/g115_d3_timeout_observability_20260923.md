# D3 timeout observability: diagnostic logging verified

September 23, 2026. An isolated evaluator now preserves each policy decision's begin/end binding on stderr, so an external timeout can leave the last active game, step, actor and physical decision id. Logging-on and logging-off executions produce byte-identical semantic stores on both fixed acceptance matches. The original timed-out match has not been replayed; its failing root remains unknown.

Diagnostic source `fe409f455276c0025372d7a5abf92df4191d7293`, at `E:/mtg-g115-d3-progress-diagnostic-20260923`, starts from formal source `cd41885e`. The only change is 23 added lines and one replaced line in `learned_bo3_v1/public_evaluation.rs`: `MTG_D3_PROGRESS_DIAGNOSTIC=1` writes begin/end JSON around the existing selection call. The end record carries elapsed time and the selected index or returned error. No reward, search budget, policy selection, engine rule or stored match schema changes. This diagnostic source is not admitted by the formal launch guard.

Build completed in186.944s with four BelowNormal compiler jobs and the reserved memory headroom checked. Executable SHA-256: `4c575904b197e54701ccda9399b6223ef000890dab1f3eb78068340ca37d118f`.

| Fixed consumed case | Decisions | Logging off | Logging on | Exact on/off store |
|---|---:|---:|---:|---|
| c00-r0-s0-baseline | 129 | 1.000s | 0.828s | Yes |
| c12-r3-s0-search | 110 | 9.703s | 9.813s | Yes |

The search case was selected as the shortest completed search by retained execution time, without inspecting outcomes. These are finite correctness controls, not a new strength panel or a representative scaling benchmark. Both conditions also reproduce the original formal stores after only the previously declared V3 build-envelope provenance normalization. All478 progress events pair correctly across239 decisions: matching game/step/actor/physical-id/substep, legal selected index and no returned error. Logging-off emits none of those events. Four recovered native stores and all events were independently rechecked locally. These repetitions do not create independent strength samples.

Initial remote admission refused competing native work before creating a run directory. That refusal remains at `E:/mtg-g115-lineage-20260923/d3-progress-haley-controller-001/`. Fresh three-host inventory003 then found Haley clear and Jack occupied, with corrected-User-Agent RunPod HTTP200/zeroPods. The unchanged four-execution check ran in fresh remote `progress-diagnostic-002`, CPU only, BelowNormal, eight-GiB reserve, 60-second per-execution bound, through `check-d3-progress-diagnostic.py`. No paid allocation or formal launch occurred.

Evidence under `E:/mtg-g115-lineage-20260923/`:

- `d3-progress-diagnostic-build-001/`: build/source/toolchain receipts.
- `d3-progress-diagnostic-inventory-003.json`: launch inventory.
- `d3-progress-haley-controller-002/recovery.zip`: SHA `d8d330ddb5b9eb63abf50720ffc787ce14229f8151f71c49e551cd7bc921f311`.
- `d3-progress-haley-controller-002/recovered/run/completion.json`: SHA `3776b45da6b6dc326eb84d92049d98ec6af7e131cea32b5755916140fb3abaee`.
- `d3-progress-diagnostic-verification-001.json`: independent verification of four stores and478 events.

Next required evidence is an instrumented diagnosis of the retained timeout case under an explicit guarded compute plan. No larger replay, changed1,800s limit, formal restart, ADVANCE/NO-ADVANCE or D4 selection is authorized by this correctness report. The existing D3 result review and any concrete design amendment still govern consequential decisions. The invalid attempt and all earlier failures remain preserved.
