# Net8 Observation Diagnostics Classification Retry Manifest v1

Status: PREDECLARED offline-classification packaging retry; diagnostic only.

This manifest authorizes one classification-only retry over the immutable
execution completed under `OBSERVATION_DIAGNOSTICS_EXECUTION_V2.md`, whose
SHA-256 is
`28cdbd8e582c49d5a414c61cf8dfda974467f36093783e246ee9a26d1b79e4d1`.
The scientific protocol and interpretation in Sections 2 through 6 of
`OBSERVATION_DIAGNOSTICS_EXECUTION_V1.md` remain unchanged.

## 1. Immutable execution input

Execution v2 completed all six fixed Store pairs sequentially from producer
commit `c1cf5f1de05b64a4cae35c61862adc725df46837`. The immutable authorities are:

- artifact root:
  `D:\mtg-kernel-observation-diagnostics-v2-20260727`;
- build receipt SHA-256:
  `c15e70719aa52bd31d55389edc57d0ca76665b0f924e96e4e51a90ed35173f49`;
- completion receipt SHA-256:
  `f1be312d65e28e1c803c69fafc65cbe509d4ae4ba2828c0f8b8aa38595c55eb1`;
- completion payload SHA-256:
  `d174a5c55a4dcac2ed397941f2f7626855cd78a9c8924fdd64db6a38553e9a25`;
- bound 30-file inventory aggregate SHA-256:
  `5017527337cd0c29ed781a1aebffa421cc3528cc6f53f7d6f09248f35a57913e`;
- executable SHA-256:
  `b4a6c5d0713f5ba562212aa6411b4938b9448086d2e26c6a5248ec67ab9ed533`.

All six invocations exited zero without timeout, produced empty stderr, passed
the exact libtest/output/hash/replay admission contract, and are `VALID`.
Those build, completion, run, envelope, payload, stdout, stderr, and
invocation-receipt files are immutable inputs. No Store probe or build may be
rerun for this retry.

## 2. Why classification retry is required

The first v2 classifier launch correctly failed closed before producing a
classification. Its preserved evidence is:

- output root:
  `D:\mtg-kernel-observation-diagnostics-v2-20260727\classification`;
- failure receipt SHA-256:
  `552edc97b059374f4996e2164ab7546e05dbc91db33af3cb50f27f786d507c56`;
- failure-receipt payload SHA-256:
  `c1238c21a3ad0a8fac196693ff036e79941b413393db8491e9cebf093dee14f8`;
- stderr SHA-256:
  `b2b1a7d72b607bd737011a545d565c19e06897e32bc2d8247ffa2659408195ea`;
- exit code: `1`;
- timeout: `false`;
- admitted classification output: none.

The rejected condition was that a candidate and generation-zero checkpoint
shared `identity_bundle_sha256`. Store authority defines that digest at the
run level and intentionally stamps the same value on every checkpoint in one
run. Candidate and generation zero must therefore share it, just as they must
share `run_sha256`.

The six payloads independently bind distinct candidate and generation-zero
model-parameter hashes and checkpoint-specific boundary/logical-state hashes.
The failure was a classifier validation defect, not checkpoint identity
collapse and not a scientific outcome. The test fixture masked it by
incorrectly manufacturing a role-specific identity-bundle digest.

## 3. Licensed correction

The only scientific-input-dependent validator change licensed here is:

1. require candidate and generation zero to have equal `run_sha256`;
2. require them to have equal run-level `identity_bundle_sha256`;
3. continue to reject equal model-parameter hashes;
4. continue to reject equal checkpoint-specific boundary-head,
   boundary-head-record, or logical-state hashes.

The fixture must use one identity-bundle digest per run, and a focused test
must reject different candidate/generation-zero identity bundles.

Because the classifier source must be committed after execution v2, the retry
must explicitly separate:

- execution producer head:
  `c1cf5f1de05b64a4cae35c61862adc725df46837`;
- clean classification implementation head, captured at launch.

Repository verification must remain enabled. The immutable v2 execution
manifest, build receipt, completion receipt, executable, and 30-file inventory
must be verified before classification and unchanged afterward.

## 4. Isolated retry output and sequence

The retry output root is frozen as:

`D:\mtg-kernel-observation-diagnostics-v2-20260727\classification-retry-v1`

It must be absent before launch. The failed `classification` directory must
not be deleted, overwritten, renamed, reused, or treated as a result.

Sequence:

1. Commit this manifest, the exact validator repair, focused tests, and
   versioned classifier wrapper.
2. From that clean commit, run the complete Python classifier/packaging test
   suite on Linux and Windows.
3. Verify the immutable v2 authorities, the prior failure receipt, and the
   absent retry root.
4. Launch the classifier once through the wrapper into the retry root.
5. Require child exit zero, canonical authoritative output, a canonical
   versioned receipt, and bit-exact preflight/postflight inventory identity.
6. Interpret the four metrics independently in both predeclared scopes only
   after every check succeeds.

The final classification commit, this manifest's SHA-256, classifier and
wrapper source hashes, command, exit code, wall time, output hashes, receipt
hashes, producer head, and classification head must be recorded.

## 5. Non-claims

This retry does not change any contrast, metric, threshold, sign rule, corpus,
checkpoint, intervention, or Store selection. It licenses no training,
promotion, global majority label, representation-bottleneck verdict,
game-strength claim, or pro-level-play claim.
