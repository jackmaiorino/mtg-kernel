# EX1 engine qualification (design v3.3 section 7 item 7)

Qualification of the EX1 engine (search targets in RT1 collection). Numbers and evidence paths are in `QUALIFICATION.json`; this note says what each check is.

- **Binary:** `native_expanded_training_run_v1` and `regret_census_v1` built at ffa9903e4, default features. Later branch commits are formatting, lints, tests and tooling only.
- **Host:** Jack's PC under `host_slots` claims. HaleysPC was held by another session's reservation for the whole window. RunPod was not used.
- **Parents:** r2 is OH2 r2-s1 at dose 512. r1 is OH1 r1-s1 at dose 256, the nearest stored dose below 512 (design section 5). Hashes, Adam steps and states are in the manifest.

| Check | How |
|---|---|
| Resume determinism, search on | `det-a` runs 2 updates straight. `det-b` stops after update 1 and resumes. States, trajectories (without run-local file identities), EX1 records and step telemetry must be equal. |
| Hook on against off | `off-o` is the ordinary update from the same parent and episodes. Its trajectories must be byte-identical to `det-a`'s. |
| Target bit equality | The engine's fixtures equal the Python mirror (`tools/ex1/target_ref.py`). Every recorded row is recomputed by the mirror (`qual.py mirror`), and by the engine at the update from the trajectory's own logits. |
| CPU and wall per update, scaling | The same update at 1, 2, 5 and 10 collection workers, each on its own core claim. CPU per update is taken from the calibration chains. |
| Counts per stratum | Decisions and triggers per stratum over the calibration episodes, against n(S) and decisions x p(S). |
| #393 | The Lembas sampler tests pass on this build. Boundary-encoding rejections are counted in every search. |
| Substep menus | Every root and burst lookup checks that the node's edges biject onto the live substep menu, or the episode fails. |
| Labels | Under `resolution-boundary-v1` and this census binary: a recorded negative (a step 1 Spy game) and a scripted known-positive. No recorded evaluator game completes the line (0 of 2,592 archive-reset panel games), so the positive is built from that game's certified oracle line at decision 30 (`replay_labels.rs`, `ex1_scripted`). |
| Dose calibration | Per lineage, K1 is 16 updates at c0 = 1 with the cap off, giving c* = 0.3 / median raw ratio (3 significant figures). K2 is 16 updates at c* with kappa = 1, and its median applied ratio must lie in [0.1, 0.9]. |
