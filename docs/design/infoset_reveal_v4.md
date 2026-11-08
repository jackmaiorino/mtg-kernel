# Reveal-name constraints for fair search

`export_reveal_v4` opts into `x_kernel_infoset_reveal_v4`. It inherits the
event-time visibility and recovery checks from `x_kernel_infoset_v3`, which
continues to carry the #579 recovery contract. The v1, v2 and v3 exports keep
their original refill behavior.

The exporter reduces the selected viewer's event-time `x_reveal_meaning_v3`
stream. It uses a conservative minimum per named printing. A public arrival
counts once, a reveal binds only to its current operation, and an unlinked
reveal can overlap an arrival. Departures reduce the possible surviving name
counts. Neither the live copy ledger nor an unknown hand slot selects a name.

The allocator reserves named canonical hand occupants, subtracts them from
the public refill pool and fills the remaining slots through the existing
canonical allocation. It refuses a bound beyond the hand size or available
pool. A bound that may overlap an unresolved public pin also refuses until the
allocator can represent that overlap. The selected transcript distinguishes
search and continuously public non-stack arrivals from events that might hide
an event-less move. For a public paid-cost receipt with no live reader, a
named historical object carries the receipt without linking it to a later
copy. References with a live identity consumer still require visible epoch
evidence. The digest binds a compact operation-boundary ledger
that retains the separate arrival count and present minimum for each name.
Simulated reveal and departure hooks apply the same max-overlap and survivor
arithmetic as the transcript reducer. The sampler sees deterministic rows
for the resulting minima inside each hypothetical world.

The reveal-v4 schema and canonical occupants bind the new payload digest. The
new branch has source tests that vary hidden composition and the physical copy
shown while preserving the visible stream. The bridge's
`--x-kernel-infoset-reveal-v4` profile uses the flat-action lockstep state for
canonical row mapping and the same selected-viewer journal it sends to the
trusted adapter. It advertises a separate extension and receipt identity.
The payload digest includes the public minima by name, which the adapter and
entrant can compare with the selected transcript. The `s4a-serve-reveal-v4`
search mode and its worker require the reveal-v4 schema and use a separate
seed namespace. Completed native and companion acceptance is recorded below.

## Combined candidate qualification source

The follow-up to merge `b01cf7037` adds native tests under the `reveal_v4`
filter. `reveal_v4_request_import_sample_and_worker_use_public_minima_both_seats`
exports independent three-versus-four-Island hidden hands with equal public
repeats, compares repeated versus distinct shown copies, imports the V4
envelopes, samples actual worlds and invokes the production worker request
dispatcher. Positive transitions, original menu digest, deterministic result
digest, wrong-profile refusal and invalid-path refusal are assertions.
The recovered-payment fixture also sends its R1-recovered structural receipt
through payload import and the worker. None of these native assertions has
run on this candidate yet.

`reveal_v4_original_group_map_survives_actual_step_and_export_refusal` takes
a native land action, then exercises a real R8 refusal after serialization
has removed event-time journal provenance. It checks the next menu against
the original root map in both seats and rejects a changed group identity.
The group identity itself is explicit test input, not a claim of naturally
reached buffered play. The older bridge fixture now also invokes a real R8
export refusal instead of directly clearing `root_ids`.

`python/tools/qualify_reveal_v4_process.py` provides the separate process
check. It imports the unchanged companion at
`7e3df807e8361e987a88e75804caea320b3d6e6f`, starts the native reveal bridge and
the real reveal pool/worker, and uses the companion adapter, neutral host
validator and entrant for both starting-seat mappings. It rejects zero-work
or incomplete search responses and binds payload hashes, picks and receipts.
With `--require-buffered`, it also requires a searched buffered commit, which
executes the companion's current point, observation, flat rows, support and
selected-key checks. No peer or worker is mocked in this process check.

The process command requires `--companion`, `--bridge`, `--search`,
`--catalog`, `--source`, `--source-sha256`, `--out` and `--require-buffered`.
Use a fresh guarded job with a finite timeout and `--on-overdue kill`; the
script requires `HOST_SLOTS_CLAIM`. The default bound is 128 neutral steps
per starting seat and 32 native transitions per search, with one worker.
Build `spellbench_kernel_catalog` alongside the bridge and search executable
and generate its catalog on the same candidate. The source descriptor and
binary/catalog SHA-256s are recorded in the receipt. Existing frozen entrant
constants and model sources are not rewritten. No process qualification has
been run while native capacity is reserved.

Source validation: pinned Rust 1.94.1 formatting and diff checks; Python AST
parsing; all ten unchanged companion V4 adapter/entrant tests passed under a
one-core BelowNormal guard. Those Python tests use simulated peers and are
reported only as companion regression evidence, not native V4 qualification.

## Native failure follow-up

The default-feature `f4832fe` libtest build passed. Its `reveal_v4` filter
reported four passes and four failures; the additional V3 producer filter
reported eight passes and one failure. These results do not qualify V4.
The successor based on canonical PR591 `a3adc349` addresses these findings:

- Eight transitions complete a worker budget without necessarily reaching a
  natural terminal. The opening and R1 fixtures now assert the cap and absent
  root edges. The opening fixture also calls the production pool merger and
  requires exclusion, no pick, and correct charged transitions. A separate
  constructed empty-library position requires actual draw-loss terminal
  backup, live-index edge counts, a pool pick, and deterministic replay for
  both seats. It does not substitute a synthetic result or increase the
  opening fixture's budget.
- Sequential same-name departure/return pairs retain minimum one; two
  departures followed by two public arrivals establish minimum two. Both
  histories are compared against the public reducer. The aborted Cast Down
  fixture uses registered default-feature Gurmag Angler as its vanilla target.
- A concealed exile return correctly used `RevealV3::Omit` but still called
  the native reveal helper for unauthorized observers, inserting a known-hand
  row and observation note. The materializer now captures look permission
  before the face-down marker resets and suppresses those unauthorized rows.
  Owner access and permitted lookers remain. Authorized viewers receive
  `present`, not `moved`: a concealed source does not prove a continuously
  public arrival. Two same-name concealed returns therefore preserve native
  rows for permitted viewers but establish only a V3 minimum of one.
  Battlefield/stack reveal-on-exit semantics are unchanged.
  Both-seat/two-name permission cases and existing
  V2 hidden-return coverage assert native hand knowledge as well as projection.

Next exact default-feature libtest filters: `reveal_v4`,
`reveal_meaning_v3_`, and
`infoset_v1::face_down::v2_face_down_exile_to_hidden_zones_never_reveals_the_name`.
The last filter should use `--exact`; run all with `--test-threads=1` under
the lead's guarded native runner. The later acceptance section records their execution.
The separate process packet `b04683d` is retained unchanged. Its opening
32-transition default is not evidence that a search produces a pick; native
qualification must resolve that process bound before claiming searched
buffered commits. No model, frozen entrant constant, or companion pin changed.

## Process search-to-commit evidence

The source follow-up from `b4cd794` makes incomplete pool replies diagnostic
results. They retain path, transition charges, exclusions and per-tree results,
but never count as successful searches. A successful pool reply alone also
cannot qualify a buffered commit. The harness records real point responses
from the companion's pool requests and observes the real adapter's
`commit_buffer` and `native_step` calls through `super`; it does not replace
the peer, worker, selection algorithm or any of the live commit checks.

Each completed commit records its original `_group_wire` and `_group_keys`,
root payload/step, actual live and imported row indices, every full point's
digest, point row keys and selected key. Live and imported menus may have
different row orders. For each actual committed point the harness requests
the original payload at the reconstructed imported prefix from the real pool,
checks the live/imported canonical-key bijection and translates the selected
key to its imported index. These additional `point` requests are read-only;
they do not search or consume transition units. A search qualifies the commit
only if the game, seat,
group, payload, exact path prefix, entire point key list, next picked index,
and original completion's selected key all match. A search from another path
on the same payload, or an unmapped search pick followed by a different fallback
action, is insufficient. Receipts use `reveal-v4-process-qualification/v2` and
include the matched search indices for each commit. Both starting-seat cases
must contain a genuine successful search on a committed buffered path.

The CLI keeps the same arguments and numeric bounds. `--require-buffered` is
now required, so the existing packet wrapper's invocation remains compatible.
The old `b04683d` wrapper and failed `f4832f` packet are preserved, not repinned
or launched. A new invocation must use the newly qualified default-feature
`agent_bridge_v1` and `regret_census_v1` binaries and their same-candidate
`spellbench_kernel_catalog` output. The existing approved input remains
`D:/decision-census-20261010/sources/a48-source.json`, SHA-256
`5d97f7c63857f9165db3d47c6c4a3b4b0dde01dfb13c4ed960da2171b2bda6de`, with
current companion `7e3df807e8361e987a88e75804caea320b3d6e6f`. Use explicit
`--budget 256 --max-steps 512 --require-buffered`, a new output path, and the
lead's bounded host-slot wrapper after native prerequisite checks pass.
The harness defaults remain 32/128; this source change does not silently
raise them or turn an all-incomplete attempt into acceptance.

Focused receipt checks are in
`python/tests/test_qualify_reveal_v4_process.py`, discovered by the general
Python CI shards. They cover matching
paths, same-payload wrong paths, wrong/missing keys, unmapped picks, group
identity, original-completion disagreement, incomplete replies and accounting.
These pure unit cases are separate from native evidence. The later acceptance
section records their successful guarded execution and the real process result.

## Observed process qualification (2026-11-08)

The actual process check passes for both starting seats using companion
`7e3df807e8361e987a88e75804caea320b3d6e6f`, helper source
`96478e56318b3ee220180589ba89dcf249b34ea1`, and the three same-source native
binaries built from `476f60ac02c15572f2189e15847a3ed1aa992d88`. The original
a48 input closure and explicit 256-transition/512-step bounds were retained.
Seat 0 reached the verified buffered commit in 71 steps with 29 successful
searches; seat 1 in 16 steps with 11 successful searches. Both committed paths
and full point-key arrays matched the actual live decisions.

Report SHA-256:
`408aee9c57ac8fc4b6391b32ddc1f45aa6cd2bc6e895842d57ba297c5e385e83`.
Process receipt:
`a4fba62d0a04927c511695d8f594a5f913db69d6f322412eda118f6fbf95ce8b`.
Build receipt:
`cee335799cafd5b05a69b53d537615779dcf5b4c5e502f9d1d6c8073c809bd77`.
The binaries and reports are retained in
`D:/codex-reveal543-process-476f60ac-20261108/` under `build-001` and
`process-003`. Rust 1.94.1 and MSVC 14.50.35717 were pinned; guarded desktop
cores 0-3 at BelowNormal preserved SD1 on cores 4-23, a 60 GiB free-space floor
and a 12 GiB owned-growth cap.

The first process attempt exposed the adapter's wrong canonical-key width:
native row keys have 16 hexadecimal characters, while payload/state digests
have 64. Companion 7e3 repairs only the row-key validation and its fixtures;
all 63 tests in the four directly affected Python suites passed. The failed
attempt remains in `process-001`, report hash
`0c9ed4168a7bec1111a31a5c61efcff3085533d8e54c8cfa050a4b2b038d25d0`.
A later prelaunch refusal found the helper's obsolete companion pin; 964
repairs that pin before the passing attempt. Twelve helper receipt-binding
unit tests passed before this pin-only edit.

The earlier b4cd794 native executable passed all 21 V4/V3/privacy cases.
The fresh exact-964 run passed all 28 cases: recovery 7/7, V4 9/9,
V3 11/11, and hidden face-down privacy 1/1, none failed or ignored.
Libtest receipt SHA-256:
`04092445b1bf535ea8132927f11f01e6837c0ee25ff19a418ce563948cf5e2f8`;
archived executable:
`05674ef7ba5ddebd85db5d872bbdc761f43ed8514a19c9057aad718b0d196322`.
All 12 Python receipt-binding tests also passed against integration source
31fec0c7; receipt
`b73bb2965dc6dddc3fd1975395929212d8a75e36eef89627744ff1d913858989`.
Integration source
`2ac295ddd485595c44407dd33660270ea7b20da9` merges main 87d5 cleanly and keeps
all reveal/recovery/process source files identical to 964; the only overlap
with imported main files adds its existing Eternal BO3 module declaration.
These engineering checks do not modify the frozen W1 entrant or establish
playing strength.
