# Net8 Action-Ingress Admission Result v1

Status: RESULT RECORDED; static and runtime admitted; diagnostic only.

## Outcome

The separately predeclared V2 retry completed successfully without training.
Its authoritative classification is:

`STATIC-AND-RUNTIME-ADMITTED`

The repaired structured action ingress distinguishes every legal action in
the fixed static and runtime corpora when the 96-float action digest is
zeroed. All three fixed parameter authorities are nevertheless more
sensitive to matched digest-sibling reassignment than to direct-sibling
reassignment on each of the three predeclared policy metrics:

| Model authority | Separate descriptive label |
|---|---|
| `raw-common-snapshot` | `RAW-INIT-DIGEST-DOMINANT` |
| `imported-mirror-g0` | `IMPORTED-DIGEST-DOMINANT` |
| `imported-diverged-g0` | `IMPORTED-DIGEST-DOMINANT` |

There is no pooled, voted, or global label.

Digest dominance at the genuinely fresh common snapshot means inherited
training is not required for this sensitivity to occur. Initialization-time
input geometry is therefore a live explanation. The result does not
establish that the digest is useful or harmful, that training amplified the
reliance, or that any model is strong at Magic.

## Authorities and execution history

### Source

- Parent observation result commit:
  `ebd13031ea14526547849272ffbd7526fa2087fd`
- Parent observation result SHA-256:
  `a728aafcba53f42b9d78f7f5db468c5fe0dc87c325168cacec94926aa9ff63f3`
- V1 admission manifest SHA-256:
  `9317e5504a72acaced0100aea889a36c50539f6ce7f46912170ca2a562fbb88f`
- Failed V1 implementation commit:
  `5d5ed8e856651e56b700915dde1844ea373407ad`
- Invalid V1 record commit:
  `395913f0c9664372e0e778056a3b90f7ede4e257`
- Invalid V1 record SHA-256:
  `f299657ab4e4e6ca9906ddc0f06b0eb538c781f53348eaa0d7dd21f5d77f8688`
- Manifest-only V2 commit:
  `4a7caca8818f24ff2b7a3d9c4f3639858fd7ca23`
- V2 manifest SHA-256:
  `3fbdd98c902db833f58dc73f1a63938983670fd1f06ebfb010f01f9ddf102945`
- Final V2 implementation commit:
  `6ba5d37f667a99ad9938f458d889e454ca8ef281`
- Branch:
  `codex/observation-diagnostics-v1`
- Worktree:
  `C:\Users\Jack\IdeaProjects\mtg-kernel-observation-diagnostics-codex`

The V2 builder mechanically accepted the exact licensed 15-path diff from
the invalid-record commit, all 23 frozen inputs, all 21 implementation-source
records, and the exact one-line parent Rust module addition. The worktree was
clean at the implementation commit before and after build, execution, and
classification.

### Invalid V1 execution

V1 is wholly `INVALID`. Its locked build passed, but the verifier rejected
the first raw payload before writing an invocation receipt because CPython
3.13.14 built-in `sum` used compensated float accumulation where the manifest
and Rust producer required an ordered positive-zero `f64` left fold.

- V1 build receipt SHA-256:
  `0cc9c7d943343b1d8babda4749257221399e3bdefaeb1366815d37bedc4060ba`
- V1 build payload SHA-256:
  `fe20757fd7f6fbdf795f99d967e6c05e34f526b079ea4c7049fa6f6978981d55`
- V1 executable SHA-256:
  `6036f3bc0fdfed2cc40931c53a073768a71702f6f839a6713c08b84899d49f82`
- V1 raw stdout SHA-256:
  `816cf78c980356cd3e1f956962122528d87d76d9fa807dbbdfa0774f7cd2c253`
- V1 raw stderr SHA-256:
  `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- V1 raw envelope payload SHA-256:
  `507a8bdef0fe13e37808074d57e20d446958d8dc78cdfb434f272874cb2bef09`

Only the raw subprocess ran. Neither imported model was invoked, no
invocation or completion receipt was written, the classifier did not run,
and no V1 metric or label was admitted or interpreted.

Before V1 was frozen, a subagent mistakenly ran one raw ignored-test
engineering smoke from a dirty source tree. It used neither official root,
the implementation changed materially afterward, and none of its values were
retained or consumed. It is non-authoritative engineering smoke, not an
execution of either manifest.

The immutable V1 evidence remained exact before and after every V2 phase:

| Root | Aggregate SHA-256 | Directories | Files | Bytes |
|---|---|---:|---:|---:|
| `D:\mtg-kernel-action-ingress-admission-v1-20260726` | `5488ffd74443833a28c44d63ebea0be27a770684caf78f0be41f57d48a248bc6` | 3 | 25 | 584131 |
| `E:\cargo-target-action-ingress-admission-v1` | `0c1680d0b4c72f4dd7e4b8b739f30fba3f0acd732e949d9fa90d08aef8312aa0` | 118 | 560 | 191861359 |

The fresh V2 raw payload also projected to the preserved V1 scientific
fields byte-for-byte after removing only the version identities and the two
new squared-norm evidence fields:

- canonical bytes:
  `400290`;
- canonical SHA-256:
  `23419136ee00d84f1cfb0b13110333705eaa2ebdd0a130f2d0dd069b89aac9af`.

That equality is packaging and invariance evidence only. It does not admit
the failed V1 metrics.

### Valid V2 packaging and receipts

- Artifact root:
  `D:\mtg-kernel-action-ingress-admission-v2-20260727`
- Dedicated Cargo target:
  `E:\cargo-target-action-ingress-admission-v2-20260727`
- Evidence label:
  `ACTION-INGRESS-ADMISSION-V2-DIAGNOSTIC-NON-EVIDENCE`
- Official Python:
  `D:\mtg-kernel-clean-venv-019f63a2\Scripts\python.exe`
- Python version:
  `3.13.14 (main, Jun 23 2026, 15:19:27) [MSC v.1944 64 bit (AMD64)]`
- Rust channel:
  `1.94.1`
- CPU only:
  `true`
- GPU visibility:
  empty

The locked release build passed:

- Linux static controls:
  `7/7`;
- Windows static controls:
  `7/7`;
- Linux V2 packaging tests:
  `29/29`;
- Windows V2 packaging tests:
  `29/29`;
- active V2 Rust controls:
  `3/3`;
- official V2 Rust probe during build:
  ignored as required;
- Cargo exit/wall time:
  `0` / `419099 ms`.

The tool and executable authorities are:

| Executable | SHA-256 |
|---|---|
| locked V2 release test executable | `80cab9e78a206c6df1dbfc2f8bf5a9cb562f4e3e903f58e4a21846f3d4f4676c` |
| `cargo.exe` | `43226f7efc5ea12b88c9156da97f8954b9af582673baadb3fb1a3ebec5d97348` |
| `rustc.exe` | `21256c9767416cbc70120e7987449c6cc5a66e3e9f843d05392ee4fd5e617261` |
| `rustup.exe` | `86478e53f769379d7f0ebfa7c9aa97cb76ca92233f79aa2cc0dbee2efaac73c7` |

The top-level receipt authorities are:

| Record | File SHA-256 | Payload SHA-256 | Status |
|---|---|---|---|
| build receipt | `2c75e7a1e6a352527609e7eaf382ab29b8d707f942fb443e77be657314481d56` | `7a6155986ebef20df121a957fe29a5c97463591ec7c78ac31767405410a53298` | accepted |
| completion receipt | `2abf3565ec168ff48a0ea9cfcc5847180695ffce8971edf63c06b667a9e8030b` | `8707e1de5c2da7a261730023e9b27128fce67a5dd37e84ff8846062f44f9553f` | `VALID` |
| classification | `e1bca92735b635399c8a9309f777836d08b5c15ca8df54b5b1d5cfc0a36dc23f` | `80d49441cb7c8f7fdf5b2a28df192e541209b1892af9bec49d8bc5c4429ac3f9` | `VALID` |
| classification receipt | `fb25357fd8854cdc771e134bede992a470247cd78d3f8c2649633d45dc72b1c3` | `35c40ac15ea334f6fa40721ebefb7432cfba18aae2ebbfa70e92d3467627b47e` | `VALID` |

Classifier stdout is byte-identical to the classification file and therefore
has SHA-256
`e1bca92735b635399c8a9309f777836d08b5c15ca8df54b5b1d5cfc0a36dc23f`.
Classifier stderr is empty with SHA-256
`e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.

The three reads ran sequentially in the predeclared order with a 120-second
timeout per model. Their combined elapsed time was `70416 ms`; classifier
wall time was `8076 ms`. All exits were zero, no invocation timed out, and
all three stderr files were empty.

| Model | Invocation receipt SHA-256 | Receipt payload SHA-256 | Envelope SHA-256 | Envelope payload SHA-256 | Payload file SHA-256 | Stdout SHA-256 | Stderr SHA-256 |
|---|---|---|---|---|---|---|---|
| raw | `95b14080f5cf7352e7a51f87578c3c5d8c0cb9a20f751dc0f175aac8db574691` | `06979b8619a4d0729b738ad1434381378d854e4ca47b132aa2406e1c8d2c4c25` | `c408d79ecc1ddcdb7cc26f33c802614dce6a71360d2158427963d16d8237212d` | `f7175f5e29cf686746e2b11a20d73c0034fbcaf316b4e471f3b6d2ead58dc9cb` | `52854017ff8c8a55160a6a9b6f7e17a3ca66e3a4bfbfe6a2861dce51b670bbd9` | `728b8ffe9702b8a30e5d07c7ffaaf9eb3805f4f6aa5907468ca0016536753880` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| mirror imported | `a98e1545be49c32707c7e5525805c84bf33139986c5fab7624a98cce5c4fb000` | `aa11ac584b0926e0f442b670ad34c52fb1db58c9cce5ff915ed432ca97f58777` | `952f0eaafea20df182238b5f541e1959cbd4b1afbd2a9bc4172275ce8f839459` | `92007ddc3c6afc8de100ea2af6078392b4eb34923d4aa0fb12646704bfc2b249` | `32edfdf1491a463a58f9a1204bb46b5697073fdddf0ea5a5e75c533037503800` | `aac80ccc8ee7429d8e768d0d5ea589d81b6e7c7e5e1b801065e0253a53e9235e` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| diverged imported | `d4501ae530ce9ef7529dd8a9fa85e721cc762b737ec97942a559762913b61d76` | `9aa18316fea241fc4ad739e72f3ef4a07aa6559d149c481369430860afe9abc2` | `9f040a60438e5fc057a8bf5b99ddeca7c419bde1943a4e7e723e679ddc5d2246` | `313d294d2b9ac69d28b6e4a3289b25e112751f9dd866c39642c0d52a9830ef6b` | `84e5d0169f0aee11ddf0121e14f67d1ec36bf08882ab246149e99772e2271947` | `a02826ef24a072fcc17f58892b7807feee34baac55645944178edfcb2c32c7b4` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |

The completed V2 artifact tree contains exactly 6 descendant directories,
43 files, and 4893205 bytes. Its canonical tree aggregate SHA-256 is
`3dd0a43f5247aa070a189454b90a253a7038c99afbd9cec2cb612a493a9c6197`.
The build and completion receipts retain the exact per-file inventories.

Both imported Stores were read-only and byte-exact before and after all
three reads:

| Store authority | Aggregate SHA-256 | Directories | Files | Bytes |
|---|---|---:|---:|---:|
| `imported-mirror-g0` | `9d4273b04c1e937b83ac8ae930dda002b371467c91bd807c6513a02abd88365e` | 4 | 905 | 2278176324 |
| `imported-diverged-g0` | `4eb510a1a7c6fc9fc73fee0fffdd399d5dcadb099aaef9f80e2b107ecd5d3b81` | 4 | 905 | 2317847644 |

## Static and runtime admission

The checked static report is `STATIC-ADMITTED`:

- report SHA-256:
  `4b4f0a6c9a28dd98b7a482fe23ad44a0b6e072619343ae01c68e9f335f99e78d`;
- payload SHA-256:
  `65f171c88260f2397614d947809b1e54f196d52a6df1c0b7d0faa57bbabf47aa`;
- combined static corpus:
  `116` cases;
- combined corpus SHA-256:
  `7a8cc702393253fdb2dfe61bcca648cbc8684cd1527e0d30a37b242baeb20a6e`;
- action model-input coverage:
  `202/202`;
- Boolean-polarity and optional-presence gaps:
  none;
- distinct-canonical raw-digest, quantized-tail, frozen-complete, and
  repaired-complete collisions:
  none;
- repaired structured aliases:
  none;
- unexpected canonical equivalences:
  none;
- unchanged frozen cases:
  `115/115` byte-identical without repair.

The runtime corpus reproduced its frozen authority:

- identity:
  `rally-mirror-splitmix64-modulo-fixed-256-post-tensorization-v1`;
- SHA-256:
  `72103ea367a662f76675a044ad4efcf4c52bf86d32630df88e5247cf79f5e5e0`;
- episodes/decisions/multi-action decisions/actions:
  `4` / `256` / `256` / `1115`;
- exact semantic-binding capture SHA-256:
  `a8210e13ef4421a8ccc8a4c5029b64a70b4ca39c0311774df96615f92d3dd5f3`;
- attacker false/true pairs:
  `33`;
- blocker false/true pairs:
  `12`.

For every model, all 1,115 repaired digest-zero 163-float pre-action-encoder
rows were pairwise distinguishable within their legal-action decision. The
model-qualified ingress stream SHA-256 values were:

| Model | Repaired-zero ingress SHA-256 |
|---|---|
| raw | `30c723da93856afb47ea291d6aa66078598e86c135db4112333f7a81e2c228ba` |
| mirror imported | `dba21a6df03baf77fab50a758e00c2b275adaf4d60cc6c10d491c9c83c884a87` |
| diverged imported | `2485d7aca41991b70b6383abb41fee0d9cc35b16f9db13d9be23375c6b384cd9` |

Every pre-transform semantic/core/reference/digest binding passed. The
slot-69 inclusion pairs were complete and differed only where predeclared;
their pooled references were bit-exact. Non-action tensors were unchanged,
digest replacement followed by the zero gate was bit-identical to ordinary
zero, every action-only intervention preserved value bits, repeated forwards
were bit-exact, and model parameters were unchanged.

This satisfies the checked-corpus ingress prerequisite for drafting a
digest-zero training arm. It does not authorize that training and does not
prove structured sufficiency outside these corpora.

## Fixed model authorities

### Raw common snapshot

- Role:
  `raw-common-snapshot`
- Initialization seed:
  `6443515232517447393`
- Named-parameter/model SHA-256:
  `36157c71b9fd736d4913e6c5722dcb9c1e4f119b7b28b108bde9d74f18862d54`
- Snapshot manifest file SHA-256:
  `d5d296f5d4ee1f7e40a6005f1e1dd328b2885f6b95f0c6968c6bf1b87351c7cc`
- Snapshot payload SHA-256:
  `79f715b11ccce80ac66cc832bfdc0c963a8a20f27f7b492fdfbb433c008a90a5`

### Imported mirror generation zero

- Role:
  `imported-mirror-g0`
- Store:
  `D:\mtg-kernel-exploiter-v3b-20260726\runs-arm1\dev0\run-0\store`
- Model SHA-256:
  `db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d`
- Run SHA-256:
  `0b46f9507caede181e745da51dabbb6c9f73d72d3eb2315f089ef248c60e2f80`
- Identity-bundle SHA-256:
  `3b3e4e2270d307e7984314b91be69f1ccad0ec171d3210e3048a7ba2eb747024`
- Checkpoint manifest/payload SHA-256:
  `fb780bfb8c5de8f88a9a1254108c7f45f7a90dba75f8ef614c8103681c7127a1`
  /
  `2a0840425ccfd09df56747d016d8fcd6b5bc19bba09b6f8cbcdc4507b7315095`
- Adam step:
  `0`
- Prior baseline output reproduced exactly:
  `92d40cc1bd5ad4d54cb65cabb66b2788e4de16306727e6efc8c92f1b37e631da`

### Imported diverged generation zero

- Role:
  `imported-diverged-g0`
- Store:
  `D:\mtg-kernel-exploiter-v3b-20260726\runs-arm2\dev0\run-0\store`
- Model SHA-256:
  `9c692503df20669686d4b5706cd5ed53989a60ca9dec3778c10312b3bddc722e`
- Run SHA-256:
  `fee86543272b4f709be46bb7f9eec820d979d264a93b606408e07c9a6871e51f`
- Identity-bundle SHA-256:
  `27c1c4798f8eb4a396e1952d055cb04122ce44d24fc8ff98118787ae0cb0985c`
- Checkpoint manifest/payload SHA-256:
  `2503dc79396fd9cf22e2771324e13b246f686de89503e497680d50091a4fbd99`
  /
  `0d818f5803a96c7ae15c0a550cc9cec99bc50bf72a996697e2d0a1f09fd41145`
- Adam step:
  `0`
- Prior baseline output reproduced exactly:
  `39d5466625461fe9eb364436255a0dec0ba75d10c1d0fcdc27b6cc582a436dfc`

Every other predeclared imported-Store provenance field also matched exactly:

| Field | Mirror generation zero | Diverged generation zero |
|---|---|---|
| segment ordinal | `0` | `0` |
| segment manifest SHA-256 | `54c1d3cc527bc339f55734a47c660b2f5078b291a9d2d4b0cdfd36eeeaa8ec5e` | `9957484508c494032526b91a3226c8b30e3e82d5a50e4070479b74e5fda4a5b5` |
| parent boundary head SHA-256 | `null` | `null` |
| boundary head SHA-256 | `659a9e4cd250cf1f38a678d3632d1ce6ae1fd6aa7d7bc02918bb6e0d4762cfd2` | `142fe85ace4c0b8e4d006b2d424c5f65604375eb0f64856395e87b783d648a13` |
| boundary head record SHA-256 | `0b45da9663aed2f56460c85693122dae267b9a7f782152023dcbe02f2fa3d64e` | `42360bb84f74a995be98f473181601baedd22ff238012fed4222d1790d11c456` |
| checkpoint sidecar SHA-256 | `a6a6c1934f388ff0e212bb15a5f43f7fd6a03dc9ec1dff91acfe762a4a72b62f` | `3c6ef5aa5fb4358014a95870060cc1cb0d80b2f38ee5fde8660167968f666ad0` |
| logical-state SHA-256 | `f46efcc86d9cc6ad2aec8bcc13e02560d1cd3bc3da166bb9a9e7054430dba18a` | `c05d303a31e300398ea40d3eca4b37b75a7cc832648fa0dda22920586a93e09b` |
| train-state SHA-256 | `0b35c448201efe92375f48a22201c432d3272a3286fae1440f6e7aa2277b9de5` | `207f2b99499ec67fcca99b332b28614771be84088696bbd4983c2053b482bd2c` |
| last-update evidence SHA-256 | `null` | `null` |

## Input scale and first-layer contribution

The repaired/full action inputs are the same for all models:

| Block | Value count | Value RMS | Mean per-row squared norm |
|---|---:|---:|---:|
| direct `[0,99)` | 110385 | `0.307286596014878` | `9.348080156950672` |
| digest `[99,195)` | 107040 | `0.5937322319121602` | `33.84172446829393` |

Using each model's own fixed `action_encoder.0.weight`, excluding bias and
the appended action-reference input:

| Model | Direct contribution RMS | Digest contribution RMS |
|---|---:|---:|
| raw | `0.23220285062385573` | `0.4533189764101576` |
| mirror imported | `0.3089029945289595` | `0.5155796046760042` |
| diverged imported | `0.3528052205810209` | `0.5458199965891755` |

These are descriptive input/contribution scales, not causal percentages.

## Functional effects and separate labels

Every row below is a mean over the same 256 multi-action decisions.
`digest - direct` is the predeclared contrast used for the model-qualified
label. The full-versus-zero row is reported separately and is not part of
that sign classifier.

| Model | Intervention/contrast | Mean Jensen-Shannon nats | Mean centered-logit RMS delta | Top-action-flip fraction |
|---|---|---:|---:|---:|
| raw | direct sibling | `0.00017275635291445153` | `0.032239345597748076` | `0.09375` |
| raw | digest sibling | `0.018391165006253502` | `0.35312742675719117` | `0.95703125` |
| raw | repaired full vs zero | `0.006951143865626616` | `0.2196245684679498` | `0.83203125` |
| raw | digest - direct | `0.01821840865333905` | `0.3208880811594431` | `0.86328125` |
| mirror imported | direct sibling | `0.024288416995426314` | `0.9613905998145641` | `0.13671875` |
| mirror imported | digest sibling | `0.32197441924561493` | `6.559408184829417` | `0.6640625` |
| mirror imported | repaired full vs zero | `0.09089719413910793` | `3.4042565460091785` | `0.171875` |
| mirror imported | digest - direct | `0.2976860022501886` | `5.598017585014853` | `0.52734375` |
| diverged imported | direct sibling | `0.029502455527074384` | `1.3528291347302235` | `0.1640625` |
| diverged imported | digest sibling | `0.348172305638869` | `6.815588805670628` | `0.6484375` |
| diverged imported | repaired full vs zero | `0.10375128556077308` | `3.727678221124678` | `0.13671875` |
| diverged imported | digest - direct | `0.3186698501117946` | `5.462759670940405` | `0.484375` |

All three contrasts are strictly positive for each model, producing exactly
the three labels stated in the outcome. Exact zero would have been neither
positive nor negative and would have routed that model to its qualified
`MIXED` label.

The output stream authorities are:

| Model | Frozen/full output SHA-256 | Repaired/full output SHA-256 | Repaired/zero output SHA-256 |
|---|---|---|---|
| raw | `a67d0eac85da932ae23651a0d90d5d5aadaf9f6b81d1e3c02c62faf16ba316ed` | `69e72e470ad2defcc84b5ae5658f3646941727ccffcbb197998593c31ce714b3` | `794b90bf38e95f96cb37d05265fa1e00f087db26b82efeb7240190fe677eb8a6` |
| mirror imported | `78fdcef7addc7e165e83c75a69b51618ed1f30d0d605c6cf9ee51a5d9a49dc0d` | `703af6b3cca3b0854fa30e35fa45196cdd0fb56d13cf6e2f342899af09571367` | `b5e499639374492ccce9e0f1940f53a56645bcc6f2db349180c7ea290b0099dc` |
| diverged imported | `747be9aed4058ec5c994b4141d540abee5d56e45a730a0d0c7697f9e149fe180` | `55a683cc4cc8444829ddc3448fcbc3e153f2549407e883c0dc1637fe04a35792` | `a52fad2a6b93e187fd583ab369f0b106cc68b7ffba15374ac001fccd446e8eae` |

The three policy metrics are correlated views of the same logits, not
independent replications. Their numerical scales also differ and must not be
pooled. The much larger imported centered-logit and Jensen-Shannon
contrasts do not by themselves establish learned amplification: the
authorities differ in all model weights, and the screen has only one raw
initialization seed. The raw top-action-flip contrast is also larger than
either imported contrast, so no single monotone cross-model ordering is
licensed.

## Interpretation

### What the result supports

1. The slot-69 combat-Boolean repair closes the two known structured aliases
   while preserving the fixed 195-float action width and all frozen inputs
   outside its declared coordinate.
2. On the checked static corpus, repaired structured action features have
   complete declared model-input coverage and no observed repaired alias or
   collision.
3. On the fixed Rally runtime corpus, the repaired direct plus pooled
   action-reference ingress remains pairwise action-distinguishing with the
   opaque action digest set to zero for all three models.
4. Digest-sibling reassignment has a larger policy effect than matched
   direct-sibling reassignment for every predeclared metric in the one fresh
   initialization and both imported trained starts.
5. Because the raw snapshot is also digest-dominant, initialization-time
   input geometry is a live explanation for the existence of digest
   dominance at this one seed and fixed corpus. Inherited training is not
   required for that occurrence.
6. A controlled three-arm training micro-rung is warranted to test whether
   the digest supplies useful signal after the structured repair or mainly
   acts as a dominant dense fingerprint.

### What the result does not support

- No causal percentage of policy reliance.
- No proof that digest sensitivity is useful, harmful, memorized, leaked, or
  learned during training.
- No claim about raw-initialization behavior beyond seed
  `6443515232517447393` and the fixed Rally corpus.
- No proof that the repaired structured representation is sufficient outside
  the checked corpora.
- No collision-absence claim outside the checked static and runtime corpora.
- No attribution to training signal or credit assignment.
- No inference about broad generalization or robustness.
- No model promotion or production encoding change.
- No game-strength, equilibrium, multi-deck, best-of-three, sideboarding,
  human-play, or pro-level-play claim.

## Route

This passing admission licenses drafting, but not executing, a separately
predeclared three-arm micro-rung:

1. repaired structured ingress with the current stable canonical digest;
2. the same repaired ingress with the action digest fixed to exact positive
   zero;
3. the same repaired ingress with one deterministic episode-specific
   permutation of the 96 digest coordinates, applied identically to every
   action row in that episode.

The arms must share frozen starts, data/curriculum, optimizer, update budget,
seed schedule, evaluation opponents, and integrity gates. Training may begin
only after a new manifest freezes those authorities, effect-size and
uncertainty thresholds, cross-seed consistency, absolute-strength and
regression gates, stop rules, artifact isolation, and precedence. No single
aggregate win rate may select an arm.
