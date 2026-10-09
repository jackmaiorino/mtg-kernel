# Full-batch retained-imitation integration

The 256-group retention dataset now feeds the CPU learner through the same validated loader used by the read-only checker. Two-update checkpoints match byte-for-byte across one/four workers, fresh replay and fresh-process resume. The 100 reserved validation games remain unparsed and unscored. This is integration evidence, not improved playing strength.

Implementation `887fdd2f` extends `retained_engineering` to accept exactly one of the original trajectory fixture or a pinned retention dataset. The shared loader in `campaign/retention_data.rs` verifies all original-parent scores before an update. Checkpoints bind the retention data identity and ordered row selection; changing the data or switching from an old fixture checkpoint is rejected. The original fixture's checkpoint serialization and update result remain compatible.

The check used all 32 certified training teacher positions plus 256 retention physical groups containing 303 V4 rows. Twenty retention groups contain multiple substeps. Beta stays at the pre-existing diagnostic value 0.5, with learning rate 0.0001 and all original g115 Adam moments/age. Teacher CE and parent KL retain their separate physical-group denominators. No rewards, consumed evaluation positions or validation-game contents were substituted into training.

| Check | Result |
| --- | --- |
| Optimizer age | 32,400 to 32,402 |
| First update KL | Exactly zero; full state equals teacher-only update |
| Second update KL | 0.001510528498 |
| Second update teacher CE | 7.393366814 |
| One/four workers | Both checkpoint files identical |
| Fresh replay | Checkpoints and result identical |
| Save/resume | Final checkpoint identical to uninterrupted execution |
| Previous 16-row fixture resume | Checkpoint bytes match the preserved old result |
| Shared-loader read-only regression | Result bytes match the previous 303-row checker |

Five malformed requests are rejected before output publication: more than two updates, both retention input types, an old fixture checkpoint with the new data, corrupted parent logits and a changed retention-data pin. The last case retains identical parsed data but changes the pinned file, verifying that resume does not silently accept a new input identity.

Seven accepted full-batch engineering updates ran across four verification processes, plus one legacy-resume update. The native command is still capped at two updates and cannot serve as a substantive campaign launcher. It must not be repeatedly invoked to bypass throughput qualification. No retention weight, campaign schedule or statistical advancement rule was selected by this check.

The release build completed in 145.78 seconds using four BelowNormal Cargo jobs and E-drive target/temp storage. Two-update processes took 2.964 seconds with one worker and 2.237 seconds with four; a warm one-worker replay took 2.307 seconds. Update-and-publication sections alone took 1.906, 1.674 and 1.899 seconds respectively. These short correctness timings are not a full CPU/GPU/both-PC allocation qualification. Final inventories show no active owned native jobs on either PC; seven existing idle human-play sessions remain preserved.

Evidence: `E:/mtg-meta-recovery-20260921/retention-full-batch-engineering-001/completion.json`, with pinned `check-retention-full-batch.py`. Build: `retention-full-batch-tools-001`. Binary SHA-256 `fcab89a4af7ab5886bc27fbb67dec646481c60e07bbc8445209ed5fd316cea58`; result `e6394390244610747f143a10153c76fd89b4136ce874837d24cfd3d0ab3eae2c`; checkpoint file `d50eaa73da8e217c840629f2f4f379ecfd014cb2e7b020a6aa6bf93f3169ed3b`; final state `ce3664b4ab2a25e8440284dc909f4c333abc244f9d26e60c52bb39585cebe20f`.

Next is the causal retained-versus-unretained experiment, with the corrected semantic control, fresh tactical validation and predeclared analysis gates. Its question must separate learning the tactical labels from preserving the parent's broader policy; parent agreement alone is not optimality. The reserved 100-game panel can test policy preservation but cannot prove tactical transfer or match strength. A supported substantial launcher and measured completed-work allocation remain necessary. The old consumed 24-position evaluation and raw-index control must not select the successor.

Fable's independent objective/design review remains missing under the known zero-read HTTP429 until September 22 at 07:00 EDT. Reversible integration continued under the maintainer's execution assignment without retrying the quota error or claiming endorsement. No paid compute, CP7-based selection, candidate promotion or human-preview change occurred. CPU disposable-teacher-update overhead and the absence of CUDA retained imitation remain implementation limits.
