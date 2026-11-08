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
seed namespace. Native acceptance and the successor entrant are pending.

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
`622e3b705d8e0c91284e2ebbf02ed2f58a4f9030`, starts the native reveal bridge and
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
  Owner access and permitted lookers remain; battlefield/stack reveal-on-exit
  semantics are unchanged. Both-seat/two-name permission cases and existing
  V2 hidden-return coverage assert native hand knowledge as well as projection.

Next exact default-feature libtest filters: `reveal_v4`,
`reveal_meaning_v3_`, and
`infoset_v1::face_down::v2_face_down_exile_to_hidden_zones_never_reveals_the_name`.
The last filter should use `--exact`; run all with `--test-threads=1` under
the lead's guarded native runner. These successor checks remain unexecuted.
The separate process packet `b04683d` is retained unchanged. Its opening
32-transition default is not evidence that a search produces a pick; native
qualification must resolve that process bound before claiming searched
buffered commits. No model, frozen entrant constant, or companion pin changed.
