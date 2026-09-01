# Native population refresh manifest, cycle-4 (v1)

Status: design draft under the ratified cycle-4 pre-registration
(`OX_CYCLE4_PREREG_SKETCH_V2.md`, SHA `c49bffd6`). This contract governs the
CONTROL-R and TREATMENT-RB arms' refresh chains. STATIC-RB uses no refresh
chain. Implementation lands as
`mtg-kernel/src/native_population_refresh_manifest_cycle4_v1.rs`, a versioned
sibling of the v1 module; it reinterprets nothing retroactively.

## Frame

- Schema: `mtg-kernel-population-refresh-manifest-cycle4/v1`.
- Refresh interval 128 updates; refresh indices 0..=16. Index 0 is the genesis
  binding at the exact g896 start; indices 1..=16 follow each completed
  interval. `program_update = refresh_index * 128`;
  `trainee_local_generation = 896 + program_update` (start 896, end 2944 in
  trainee-local numbering; the arm's own store counts updates 0..=2048).
- Weight total 1,000,000 units; role floors 200,000 per adjacent slot pair;
  policy cap 250,000; largest-remainder integerization with ascending-index
  tie break and the v1 one-unit repair rule.
- Panel arithmetic (versioned generalization of refresh-algorithm v1): the
  28-matchup round robin over the eight bound identities with
  `G = PANEL_GAMES_PER_MATCHUP = 256` games per matchup; for policy `i`,
  `u_i` sums terminal ranks over its `7*G = 1,792` games and
  `p_i = u_i / 1792`. MW update `r_i = w_i * exp(0.10 * p_i)`, then the v1
  deterministic projection (cap, floors) and integerization. Weight
  computation itself is builder-side; the manifest binds its inputs and
  outputs.

## Content-resolving integrity (the cycle-3 lesson, binding)

- For `refresh_index >= 1`, `payoff_panel_sha256` is mandatory and the decode
  API requires the caller to supply the panel's exact bytes; decode fails
  unless `sha256(panel_bytes)` equals the declared value. There is no
  format-only acceptance path.
- Any 64-hex value with 48 or more leading zero characters is rejected
  outright as a placeholder pattern, independent of content resolution.
- The chain validator recomputes `previous_manifest_sha256` from the prior
  manifest's canonical bytes, as in v1.
- The genesis manifest (`refresh_index 0`) carries no panel and no previous
  hash, exactly like v1's index 0.

## Slot roster and assignment rules

Roles keep the v1 order: anchor-0, anchor-1, historical-0, historical-1,
current-0, current-1, exploiter-0, exploiter-1. All non-search occupants are
`policy` or `historical-fallback` with five real store-identity hashes; the
duplicate-model-hash rejection from v1 is retained (all eight occupants are
distinct identities at every refresh).

| Slot | Occupant | Assignment rule |
|---|---|---|
| anchor-0 | promoted(2)@384, frozen | exact pinned five-hash identity |
| anchor-1 | program-v1 lineage 970002 @1536, frozen | exact pinned five-hash identity |
| historical-0 | trainee lagged snapshot | `source_generation = trainee_local_generation - 512`, uniformly: at refresh indices 0..=3 the snapshot comes from the cycle-3 lineage store (generations 384/512/640/768 exist there); from index 4 it comes from the arm's own store |
| historical-1 | program-v1 rotation | seed `[970001, 970002, 970003][refresh_index % 3]` at generation 1024, exact pinned five-hash identities per seed |
| current-0 | parent import 975002 @2048, frozen | exact pinned five-hash identity |
| current-1 | trainee latest | `source_generation = trainee_local_generation`, run sha bound at genesis to the arm's formal run |
| exploiter-0 | frozen fallback A | exact pinned five-hash identity (exploiter rebuilds stay closed) |
| exploiter-1 | frozen fallback B, distinct from A | exact pinned five-hash identity |

The exact five-hash identity table for every frozen occupant follows
(collected read-only from the source stores, each field read from its
on-disk checkpoint/sidecar/run records, promoted(2) additionally verified by
recomputed file SHA-256; no identity was selected using CP7 outcomes). The
identity fields per slot are `run_sha256`, `checkpoint_manifest_sha256`,
`checkpoint_payload_sha256`, `model_parameter_sha256`, `train_state_sha256`,
plus `source_base_seed` and `source_generation`. Manifests carry no absolute
paths; store locations resolve through a machine-local locator outside the
hashed contract.

- anchor-0, promoted(2), seed 920012, gen 384
  (store `D:\mtg-kernel-ladder-pilot-20260725\pool3\primary`):
  run `2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae`,
  manifest `4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8`,
  payload `a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99`,
  model `db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d`,
  state `fc471f85d28293d72b42dc61de628859173bd67426e251a51bfbbe86c7d586d8`.
- anchor-1, program-v1 seed 970002, gen 1536:
  run `dc171fe72549154e533e337bc39884faa76811809abc0bc573bb975cea500a42`,
  manifest `8d6219e0c5acf040de202793b6f73131a30585ce3a1fea33b73e52734e91e53b`,
  payload `dc0f3c0d6ae9b4c87745c802b0b5b71b398b378d1689e9dd040c86ad12853ba2`,
  model `429446148ee88c527c307e0d9cde545a450a9f94e5be445b683d1d9955d93e53`,
  state `041dc02e23d51180f3f564d3070a6d9673ebc51339a73736e0d246c37614e602`.
- historical-1 rotation, seeds 970001/970002/970003 at gen 1024:
  - 970001: run `3d41a6ddd18383e104563cc0c1d29011466961e54662df2aef89a64338dd0f81`,
    manifest `68df9ab80e5674e950cbf1e67cc2692d34e92c9e9b0dceb41577075cf5492b68`,
    payload `417877b68ffb217ff0d626c0ffe7abf00b3b80d55e8da272a619e22c00700113`,
    model `90d1d08cb9cf9f9b0b8016983292249b3b66667c36584d9c3d202a47fc658939`,
    state `7aa63f92188ef2a912ee9ca1c42e8b50c7f04a3b065468a5544439925b78d790`.
  - 970002: run `dc171fe72549154e533e337bc39884faa76811809abc0bc573bb975cea500a42`,
    manifest `9e55dfa9dd2802c1886cfb5a2b53e736ed0bd71307cca42c2c0d8579831dceba`,
    payload `17f25b13a6a4f76f9ca99154783f87f578497b06db263cc4f34696f70e075117`,
    model `25961c9626a41b92e5ae1ff5c68933715061a15488d244a095b958d677a12558`,
    state `385975e0062b828b15daf21fc84c9c1a229e2626b95461efde0135cd6ba5fbba`.
  - 970003: run `5816d5d3cca083e47dd4bf63245035f222e2d5edce778091a75d191cd8722e3a`,
    manifest `ac630be2ac39e6166d744d2be01fb252063dd5e93532e9be6d7f63a34a9cf7e4`,
    payload `48da0281a346c53bfe31d17828eee6f3f6df619a8132b5eabb8b94321d9d9dcd`,
    model `f7e57ef74f9f6c33edb85817c5fb0968ef44397b5c0c07a404e51f3da2fc0bf4`,
    state `9e18e7b5e053523bb9f5e22eedb4b877e23fe346535e7fec4a1f83e6055212c2`.
- current-0, parent import seed 975002, gen 2048:
  run `8d9a8287ef57651d5744d26275d2a8c0dc74cfb69cb7e1b2dd22691b5bd8a504`,
  manifest `5e1ff645091bfacdade2a3e06b47c3cd71c96ed1c9fee4dd9756b343d7c834fd`,
  payload `e4aa3172bf3962af1498028f19649a85424d0e30f226b5c1f6722160fb24a2d4`,
  model `67c5d0a2c506c0514623f3f4ea0f273b904662cbdae4f6ddc89c44e255b9a70d`,
  state `c528f15f2e354315ff757c5de61299e4297e9794ddd08b19109bf7ff1ca89a5e`.
- exploiter-0, de-novo seed 971222, gen 1024:
  run `c9bd4a75d9ac8b73951e5d681295bfb3b8d468f5e00775535b69e3cd05a963f1`,
  manifest `476594a1ad72e3180d4cf33ecb1e3034bb029cf19782595922fd8451ca5b6089`,
  payload `bcaf671f77788655e7b8b40dcbc5942dd89b10e25a3d570e4ac34464bc2c7f5b`,
  model `6b42f88ed103090e029814371e38412bb5afb4979ead8571f7d628ce24780c8d`,
  state `5098754de0f32f46c855e6db038a8fdd469f902fc0fb354da9a650cebe2b4600`.
- exploiter-1, de-novo seed 971221, gen 512:
  run `7d111d855c09858cbc8404152cdf8878af3d8d5244dc3983ac25ddb2fe566232`,
  manifest `26ac933447f12cea8c09a5a1c3ba447883325e32252148c12e0838d59804dc22`,
  payload `f123b4e04fbbc49984fa3cc72f846e0098d70abf2e05a10fa52f8f32900d4400`,
  model `10ae7b2f28a3116ef01eb38edfb83e64cffa73a3ba7a169a0520a4d05c8b729b`,
  state `8378f2306a7576d0f39c8bf154fee7945cc700214a4f51c4ffd13eca341196cf`.
- trainee lineage constants: cycle-3 run
  `f25a63d0a2968016c2d44220b02d46b642fad5c4d524cd7ed82d699dbfda83a1`, base
  seed 977002, g896 head model
  `97683d41dc35d1b0884c053b069d0b34ea5a4f600f1a1f5ea9bd9e72cf067578`.
  historical-0 at refresh indices 0..=3 binds this run at generations
  384/512/640/768 (the lagged snapshots predate the arm's own store); from
  index 4 it binds the arm's own run at `trainee_local_generation - 512`.
  The arm's run identity and base seed are bound at genesis and
  chain-carried; whether an arm continues the cycle-3 run identity or opens
  a versioned successor run is fixed by its launcher contract, not by this
  manifest schema.

Search occupants are not admitted in cycle-4 (the search lane produced no
accepted configuration); `search_authority` therefore never appears, and the
sentinel-hash path from v1 is dropped rather than carried.

## Rotation and availability

- `available_by_global_generation` semantics follow v1: a slot's source
  generation may never exceed its availability bound, and no bound may exceed
  the manifest's availability generation.
- historical-0 is the lagged trainee lineage at every index: cycle-3-store
  snapshots at indices 0..=3, the arm's own snapshots from index 4. No
  provisional import is needed.
- historical-1's rotation is fixed by `refresh_index % 3` and is therefore
  fully determined before any outcome exists.

## Process boundaries

Each refresh boundary restarts the training process with the new manifest, in
both refresh arms; STATIC-RB restarts on the same schedule without a manifest
change so the restart schedule is matched across all three arms.

## Non-goals

No promotion semantics, no CP7 contact, no reinterpretation of the v1 or
cycle-3 manifest families, no exploiter rebuild authority.
