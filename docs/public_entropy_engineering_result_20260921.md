# Optional entropy implementation: engineering qualification passed

The opt-in public trainer loss now supports entropy regularization while the zero coefficient directly uses the prior loss. This changes the optimization objective only. It does not change rewards, GAE, legal choices, observation features, rollout sampling, optimizer settings or synchronous batch semantics. No playing-strength benefit has been measured.

Implementation is in 19657117, with the probe export correction and native qualification runner in 5c429a3f. Production trainer and CPU scorer are pinned in E:/mtg-meta-recovery-20260921/public-entropy-tools-002/build-completion.json. The permutation probe at fae930e9 changes the diagnostic only; its production loss prefix was checked against 5c429a3f.

| Check | Completed evidence |
| --- | --- |
| CUDA derivatives | Twelve ragged/uniform/saturated/shifted fixtures at beta 0, .05 and 1 agree with independent f64 loss/gradient calculations; maximum gradient error 1.20e-8 against 3e-6 tolerance |
| Masking and grouping | Forced choices contribute zero; padded actions, unused value rows, global physical-group denominator, split chunks and legal-action permutations pass |
| Zero compatibility | Old and new zero-loss gradients match bitwise; 36 scientific files match original late-update outputs exactly |
| Actual learning | Nonzero beta changes base parameters and Adam moments; disabled public projections and dummy embedding remain zero |
| Parallel collection and continuation | One versus ten collectors produce identical scientific outputs; a fresh process resumes the next update exactly |
| Inference and identity | Sixteen saved behavior scores reproduce; invalid coefficients and coefficient-changed resume are rejected |
| Config tests | Four focused release tests pass, zero failures, including old config serialization and masked parameter validation |

Native engineering root: E:/mtg-meta-recovery-20260921/public-entropy-engineering-001. All six jobs completed, totaling 80 natural games and 130.231 native seconds. These repeated engineering games are not pooled into outcome estimates. Headless GPU1 was verified by actual native PID and device UUID. No paid compute was used.

The initial probe build failed because its bin referenced a private module (163.888 seconds). The corrected four-tool build succeeded (188.718 seconds). The extra permutation probe build took 121.879 seconds and executed in 1.857 seconds. The focused release test build/run took 293.187 seconds. These costs are separate from the learning timing. Builds used four BelowNormal jobs and E-drive target/temp storage, Rust/Cargo 1.94.1; linker discovery and PE version indicate MSVC 14.50, without claiming process-level linker-path attestation.

Entropy is the differentiable float-softmax surrogate over legal choices at observed autoregressive prefixes. It is not the entropy of the exact quantized execution sampler or an enumeration of all macro branches. It is not PPO or MMD. The derivative and replay checks support implementation correctness on these cases, not improvement, global throughput qualification or human competence.

Next is one beta0 versus beta.05 comparison with two independent fresh training schedules and balanced matched BO3 panels. A fresh representative both-PC/storage qualification and supported launch guard are still required. Fable's independent review remains unavailable after the known zero-read HTTP429, reset September 22 at 07:00 EDT. No repeated retry or review endorsement is claimed. Proceeding with bounded preparation under the maintainer's research assignment leaves that review uncertainty explicit.
