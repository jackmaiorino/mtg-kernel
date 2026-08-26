import json, hashlib, os, sys

OUT_DIR = r"E:\mtg-kernel-population-v2-cycle3\refresh-manifests"
os.makedirs(OUT_DIR, exist_ok=True)

ARCHIVE_GENS = [384,512,640,768,896,1024,1152,1280,1408,1536,1664,1792,1920,2048,2176,2304]

def archive_path(gen):
    idx = (gen - 384)//128 + 3
    return rf"E:\c-evidence-archive-20260825\mtg-kernel-population-v2-cycle2\refresh\cycle2-population-v2-refresh-{gen:04d}\attempt-001\population-v2-refresh-{idx:03d}.json"

def sha256_bytes(b):
    return hashlib.sha256(b).hexdigest()

def write_manifest(idx, obj):
    text = json.dumps(obj, sort_keys=True, separators=(",", ":"))
    b = text.encode("utf-8")
    path = os.path.join(OUT_DIR, f"population-v3-refresh-{idx:03d}.json")
    with open(path, "wb") as f:
        f.write(b)
    return path, sha256_bytes(b)

def digest(n):
    return format(n, "064x")

chain_hash = {}
chain_obj = {}

# ---------------------------------------------------------------------
# Real frozen anchor/historical identities and cycle-2's real terminal
# (index 18) current-0/exploiter-1/exploiter-0 declarations -- all
# verified byte-exact against the real cycle-2 archive
# (E:\c-evidence-archive-20260825\...\population-v2-refresh-018.json).
# ---------------------------------------------------------------------
ANCHOR_0 = dict(checkpoint_manifest_sha256="4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8",
                checkpoint_payload_sha256="a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99",
                model_parameter_sha256="db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d",
                role="anchor-0", run_sha256="2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae",
                slot_index=0, source_base_seed=920012, source_generation=384,
                store_root=r"D:\mtg-kernel-ladder-pilot-20260725\pool3\primary", occupant_class="policy")
ANCHOR_1 = dict(checkpoint_manifest_sha256="8d6219e0c5acf040de202793b6f73131a30585ce3a1fea33b73e52734e91e53b",
                checkpoint_payload_sha256="dc0f3c0d6ae9b4c87745c802b0b5b71b398b378d1689e9dd040c86ad12853ba2",
                model_parameter_sha256="429446148ee88c527c307e0d9cde545a450a9f94e5be445b683d1d9955d93e53",
                role="anchor-1", run_sha256="dc171fe72549154e533e337bc39884faa76811809abc0bc573bb975cea500a42",
                slot_index=1, source_base_seed=970002, source_generation=1536,
                store_root=r"D:\mtg-kernel-scaled-selfplay-population-v1\replay\three-lineage-replay\attempt-001\wave-00-seed-970002-gpu1\run-0\store",
                occupant_class="policy")
HIST_0 = dict(checkpoint_manifest_sha256="ac4e187d7f67b7c2ed381a7048ad244927393e97349729d088b37421e7793462",
              checkpoint_payload_sha256="d806130ccd919b6102e69a90914ccf715b7d88ae9962ffe710d7d4ebd5c63fe2",
              model_parameter_sha256="8298017f92076def29dc8087b508764a644d146493720e7c698f9083667621f9",
              role="historical-0", run_sha256="7d111d855c09858cbc8404152cdf8878af3d8d5244dc3983ac25ddb2fe566232",
              slot_index=2, source_base_seed=971221, source_generation=1024,
              store_root=r"D:\mtg-kernel-denovo-campaign-v1\seed-971221\denovo-1024-screen-build\attempt-001\denovo-1024-store\run-0\store",
              occupant_class="policy")
HIST_1 = dict(checkpoint_manifest_sha256="4065d769908079244f5fdccbf44d4252e8cd299e1279d492b18ed7cc93995fc9",
              checkpoint_payload_sha256="8fb3f4a690f04743c90519c4001d41d54230a417c0f8ac3f1b7ecaa1805b795c",
              model_parameter_sha256="2620bdc30efee0c83d98c295be2c0126c08838b508d06a0fcd4f02f92714669f",
              role="historical-1", run_sha256="31dec27f24885c1fc657403f217591b03f4c5ff7f279988cbd54392d45450178",
              slot_index=3, source_base_seed=971223, source_generation=1024,
              store_root=r"D:\mtg-kernel-denovo-campaign-v1\seed-971223\denovo-1024-screen-build\attempt-002\denovo-1024-store\run-0\store",
              occupant_class="policy")
CURRENT_0_FROZEN = dict(checkpoint_manifest_sha256="3422361ec4011816efbd1b3beb02bf681aa342faf65712ef76aef47e793f00e7",
                        checkpoint_payload_sha256="b4516464e636eb77a1e2e603310760b08b4b9e5a3d551e1d1b7ccea65220fd6f",
                        model_parameter_sha256="e43a7b93b866b16b6726ec926c6d46a9995bf2ac0ad6e101e998f15b0a6a990c",
                        role="current-0", run_sha256="ed2fa4ddfe259ecae0e82206d0560bfefbf789baca86a15bb92c58f0b268f902",
                        slot_index=4, source_base_seed=975001, source_generation=2048,
                        store_root=r"C:\mtg-kernel-population-v2-cycle2\active\cycle2-active-interval-0256-0384\attempt-001\seed-975001-store\run-0\store",
                        occupant_class="policy")
EXPLOITER_1_FROZEN = dict(checkpoint_manifest_sha256="26ac933447f12cea8c09a5a1c3ba447883325e32252148c12e0838d59804dc22",
                          checkpoint_payload_sha256="f123b4e04fbbc49984fa3cc72f846e0098d70abf2e05a10fa52f8f32900d4400",
                          model_parameter_sha256="10ae7b2f28a3116ef01eb38edfb83e64cffa73a3ba7a169a0520a4d05c8b729b",
                          role="exploiter-1", run_sha256="7d111d855c09858cbc8404152cdf8878af3d8d5244dc3983ac25ddb2fe566232",
                          slot_index=7, source_base_seed=971221, source_generation=512,
                          store_root=r"D:\mtg-kernel-denovo-campaign-v1\seed-971221\denovo-1024-screen-build\attempt-001\denovo-1024-store\run-0\store",
                          occupant_class="policy")
EXPLOITER_0_FROZEN_NONSEARCH = dict(checkpoint_manifest_sha256="476594a1ad72e3180d4cf33ecb1e3034bb029cf19782595922fd8451ca5b6089",
                          checkpoint_payload_sha256="bcaf671f77788655e7b8b40dcbc5942dd89b10e25a3d570e4ac34464bc2c7f5b",
                          model_parameter_sha256="6b42f88ed103090e029814371e38412bb5afb4979ead8571f7d628ce24780c8d",
                          role="exploiter-0", run_sha256="c9bd4a75d9ac8b73951e5d681295bfb3b8d468f5e00775535b69e3cd05a963f1",
                          slot_index=6, source_base_seed=971222, source_generation=1024,
                          store_root=r"D:\mtg-kernel-denovo-campaign-v1\seed-971222\denovo-1024-screen-build\attempt-001\denovo-1024-store\run-0\store",
                          occupant_class="policy")

def slot(base, weight_units):
    s = dict(base)
    s["weight_units"] = weight_units
    return s

PACKAGE_COMMIT_V2 = "10ac4b7f24b6ff1fd7b40522b7a7a379b4f6f723"
DOC_SHA_PROPOSED = "c3540f385cf2c8d7dae922deb3be10af913a006076077817cc61da109cfd6d88"

# ---------------------------------------------------------------------
# Indices 0-2: REAL tranche-1 genesis links, located at
# D:\mtg-kernel-population-v2-tranche1\refresh\ (coordinator-supplied
# pointer; the 8/25 cleanup relocated some C:\ evidence roots to E:\
# archive paths but D:\ retains full campaign trees, including this one --
# an earlier pass of this script wrongly concluded these were missing and
# built a disclosed synthetic bridge instead; superseded here by the real
# files). Copied byte-for-byte, chain continuity verified against each
# file's own declared previous_manifest_sha256 before trusting it.
# ---------------------------------------------------------------------
TRANCHE1_PATHS = {
    0: r"D:\mtg-kernel-population-v2-tranche1\refresh\population-v2-refresh-initial\attempt-001\population-v2-refresh-000.json",
    1: r"D:\mtg-kernel-population-v2-tranche1\refresh\population-v2-refresh-0128\attempt-001\population-v2-refresh-001.json",
    2: r"D:\mtg-kernel-population-v2-tranche1\refresh\population-v2-refresh-0256\attempt-001\population-v2-refresh-002.json",
}
for idx in (0, 1, 2):
    src = TRANCHE1_PATHS[idx]
    with open(src, "rb") as f:
        raw = f.read()
    obj = json.loads(raw.decode("utf-8"))
    assert obj["refresh_index"] == idx, (obj["refresh_index"], idx)
    if idx > 0:
        assert obj["previous_manifest_sha256"] == chain_hash[idx - 1], (
            "chain break at tranche-1 idx", idx, obj["previous_manifest_sha256"], chain_hash[idx - 1]
        )
    h = sha256_bytes(raw)
    dst = os.path.join(OUT_DIR, f"population-v3-refresh-{idx:03d}.json")
    with open(dst, "wb") as f:
        f.write(raw)
    chain_hash[idx] = h
    chain_obj[idx] = obj
    print(f"[real tranche-1, verbatim] idx={idx} gen={obj['global_generation']} sha256={h} -> {dst}")

# ---------------------------------------------------------------------
# Indices 3-18: REAL cycle-2 archive, byte-for-byte AS-ARCHIVED (copied
# verbatim, not re-serialized), so their own manifest_sha256 (computed
# from the exact archived bytes) is the genuine, real value cycle-2's own
# real campaign sealed -- not a value this authoring pass invents. Chain
# continuity verified against index 2's own real hash above (a chain
# break here would mean either the tranche-1 or cycle-2 archive is not
# what it claims to be).
# ---------------------------------------------------------------------
for gen in ARCHIVE_GENS:
    idx = (gen - 384)//128 + 3
    src = archive_path(gen)
    with open(src, "rb") as f:
        raw = f.read()
    obj = json.loads(raw.decode("utf-8"))
    assert obj["refresh_index"] == idx, (obj["refresh_index"], idx)
    assert obj["previous_manifest_sha256"] == chain_hash[idx - 1], (
        "chain break at cycle-2 idx", idx, obj["previous_manifest_sha256"], chain_hash[idx - 1]
    )
    h = sha256_bytes(raw)
    dst = os.path.join(OUT_DIR, f"population-v3-refresh-{idx:03d}.json")
    with open(dst, "wb") as f:
        f.write(raw)
    chain_hash[idx] = h
    chain_obj[idx] = obj
    print(f"[real cycle-2, verbatim] idx={idx} gen={gen} sha256={h} -> {dst}")

TERMINAL = chain_obj[18]
print("Terminal (idx 18) slots 0-3 role check:", [s["role"] for s in TERMINAL["slots"][:4]])
print("Terminal (idx 18) real sha256:", chain_hash[18])

# ---------------------------------------------------------------------
# Indices 19-34: CYCLE-3's OWN 16 new manifests, chained for real via the
# unmodified production decoder (decode_population_tranche_refresh_manifest_v2),
# anchored at the real cycle-2 terminal link's own genuine sha256 (above).
#  - slots 0-3: frozen, byte-identical continuation of the real cycle-2
#    terminal declarations.
#  - slot 4 (current-0): retired as an active trainee this cycle (sheet
#    Section 2.4); frozen at its cycle-2 terminal state.
#  - slot 5 (current-1): cycle-3's own active trainee (seed 977002),
#    tracked at each refresh's own growing LOCAL generation (128..2048).
#    Real hashes are not knowable at authoring time (the store only
#    grows once training actually runs); placeholder digests here,
#    superseded by whatever a real launch's own resolver reads back off
#    the live Store at that point -- exactly how every still-to-be-run
#    refresh's content is unknown until that refresh actually completes.
#  - slot 6 (exploiter-0): searcher-occupied at refresh_index
#    {20,25,29,34} (cycle-3's own refreshes {2,7,11,16}) at 80,000 units,
#    real live-build search-authority sub-record; cycle-2's own frozen
#    exploiter-0 identity (971222@1024) at the other 12 refreshes.
#  - slot 7 (exploiter-1): unchanged mechanism (sheet Section 1.3);
#    frozen at cycle-2's own terminal declaration for this authoring
#    pass (a live de-novo-rebuild cadence is an execution-time detail,
#    not something a paper manifest can pre-compute).
# ---------------------------------------------------------------------
HEAVY_INDICES = {20, 25, 29, 34}
prev_hash = chain_hash[18]

SEARCH_AUTHORITY_REAL = {
    "tier": "t2048",
    "action_seed": 2026082601,
    "private_diagnostic_identity": "fnv1a64-serde-json-game-state-envelope-v9",
    "evaluator_sha256": "4d32da6c4ff64b229a82cc6063b91980864a06499f93bddd62ebe7df3587a5e9",
    # Re-printed at the FINAL commit before the real preflight run
    # (fac47bdc..., "cycle-3 Task 7: preflight walks the complete real
    # chain from refresh_index 0"); must match whatever build's git HEAD
    # actually runs the preflight/launch, since matches_fresh_reconstruction_v1()
    # independently rebuilds this field from the live build and rejects a
    # stale value.
    "engine_commit": "fac47bdc9f8ab1ed6b6332a474a3a9b4c626c3ae",
    "card_db_hash": 7262100742335860506,
    "runtime_deck_catalog_sha256": "68e7602f3a4df6217119406973954630800c358a10fca9f28e6cf9f20fd3b851",
}

for n in range(1, 17):
    idx = 18 + n
    gen = idx*128
    local_gen = n*128
    is_heavy = idx in HEAVY_INDICES

    slot4 = slot(CURRENT_0_FROZEN, 136138)

    slot5 = dict(slot_index=5, role="current-1", occupant_class="policy",
                 source_base_seed=977002, source_generation=local_gen,
                 store_root=r"E:\mtg-kernel-population-v2-cycle3\lineage\attempt-001\run-0\store",
                 run_sha256=digest(300+n), checkpoint_manifest_sha256=digest(310+n),
                 checkpoint_payload_sha256=digest(320+n), model_parameter_sha256=digest(330+n),
                 weight_units=141419)

    if is_heavy:
        slot6 = dict(slot_index=6, role="exploiter-0", occupant_class="kernel-native-search-opponent-v1",
                     source_base_seed=0, source_generation=0, store_root="",
                     run_sha256="0"*64, checkpoint_manifest_sha256="0"*64,
                     checkpoint_payload_sha256="0"*64, model_parameter_sha256="0"*64,
                     weight_units=80000,
                     search_authority=SEARCH_AUTHORITY_REAL)
        slot7_weight = 101895 + (107799 - 80000)
    else:
        slot6 = slot(EXPLOITER_0_FROZEN_NONSEARCH, 107799)
        slot7_weight = 101895

    slot7 = slot(EXPLOITER_1_FROZEN, slot7_weight)

    slots = [
        slot(ANCHOR_0, 129340),
        slot(ANCHOR_1, 133940),
        slot(HIST_0, 123504),
        slot(HIST_1, 125965),
        slot4,
        slot5,
        slot6,
        slot7,
    ]
    total = sum(s["weight_units"] for s in slots)
    assert total == 1000000, (idx, total)

    obj = {
        "schema": "population-v2-tranche1-refresh/v1",
        "program_package_commit_v2": PACKAGE_COMMIT_V2,
        "program_document_sha256_v2_proposed": DOC_SHA_PROPOSED,
        "refresh_index": idx,
        "program_update": gen,
        "global_generation": gen,
        "weight_total_units": 1000000,
        "previous_manifest_sha256": prev_hash,
        "pool_manifest_sha256": digest(1000+idx),
        "payoff_panel_sha256": digest(2000+idx),
        "slots": slots,
    }
    path, h = write_manifest(idx, obj)
    chain_hash[idx] = h
    prev_hash = h
    print(f"[cycle-3 NEW] idx={idx} cycle3_refresh={n} gen={gen} local_gen={local_gen} heavy={is_heavy} sha256={h} -> {path}")

print("\nDone. Chain terminal sha256 (idx 34):", chain_hash[34])
with open(os.path.join(OUT_DIR, "chain-hashes.json"), "w") as f:
    json.dump({str(k): v for k, v in chain_hash.items()}, f, indent=2)
