"""Write the Stage 4a run manifest (RUNNER.md section 5) before formal work.

python manifest_stage4a.py --variant {arrival,unordered} --out RUN-MANIFEST.json
Records code/toolchain/binary hashes, checkpoint/import/source hashes, seeds
and the exact encoder, a seed table (first selection world seed and all 16
evaluation world seeds per root), commands and environment, root hashes,
sampler/key schema, constants, placement, qualification receipt, spend
authority (none), budgets and artifact locations.
"""
import argparse
import hashlib
import json
import struct
from pathlib import Path

ROOT = Path("D:/stage4a-20261009")
VARIANTS = {
    "arrival": {"binary": "E:/pinned-binaries/92f277d9ad1bd0554a419f7f373025c429584eb8b9fdb21b076ac346ef490934/regret_census_v1.exe",
                "commit": "cc1c5d47da50265426927486f698623bad7a2d24", "frozen": "frozen",
                "corpus": {"r1": "out/corpus-r1.jsonl", "r2": "out/corpus-r2.jsonl"},
                "graveyards": "arrival order (kept)"},
    "unordered": {"binary": "E:/pinned-binaries/558838b01e326c5cb86f7c0b588c44925a670ddd468634aaa0fdf409f40ff769/regret_census_v1.exe",
                  "commit": "bb156f36 (branch claude/stage4a-gy-unordered)", "frozen": "frozen-gy",
                  "corpus": {"r1": "out/corpus-gy-r1.jsonl", "r2": "out/corpus-gy-r2.jsonl"},
                  "graveyards": "unordered (sorted by handle-masked content)"},
}
BASES = {"r1": 2026100941, "r2": 2026100942}


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def seed(model, root, purpose, arm, index, path):
    """Exact mirror of `regret_census_v1::stage4a::seeds::seed`."""
    h = hashlib.sha256()
    for part in (b"stage4a-v1", model.encode(), root.encode(), purpose.encode(), arm.encode()):
        h.update(struct.pack("<Q", len(part)) + part)
    h.update(struct.pack("<Q", 8) + struct.pack("<Q", index))
    h.update(struct.pack("<Q", len(path)) + path)
    return struct.unpack("<Q", h.digest()[:8])[0]


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--variant", choices=VARIANTS, required=True)
    ap.add_argument("--out", required=True)
    a = ap.parse_args(argv)
    v = VARIANTS[a.variant]
    frozen = ROOT / v["frozen"]
    sources = {m: json.loads((ROOT / "sources" / f"{m}-source.json").read_text()) for m in ("r1", "r2", "t1", "a48")}
    table = {}
    for m in ("r1", "r2"):
        for line in (frozen / f"roots-{m}.jsonl").read_text().splitlines():
            r = json.loads(line)
            rid = r["root_id"]
            table[rid] = {"select_world_0": seed(m, rid, "select", "", 0, b"world"),
                          "eval_worlds": [seed(m, rid, "eval-world", "", e, b"world") for e in range(16)]}
    cap = 64 * 3600
    corpus_ws = 680
    manifest = {
        "schema": "stage4a-run-manifest/v1",
        "plan": "collab LANES/spy-discovery-plan-20261009 (DESIGN.md, RUNNER.md) at collab main 6d6e5de5",
        "variant": a.variant, "graveyard_keying": v["graveyards"],
        "code": {"repo": "jackmaiorino/mtg-kernel", "pr": 189, "commit": v["commit"],
                 "toolchain": "rust 1.94.1 (rust-toolchain.toml), MSVC x86_64-pc-windows-msvc",
                 "profile": "release, lto thin, codegen-units 1, default target-cpu"},
        "binary": {"path": v["binary"], "sha256": sha(v["binary"])},
        "models": {m: {"checkpoint": s["checkpoint"], "play_import": s["play_import"],
                       "feature_transfer": s["feature_transfer"],
                       "source_sha256": sha(ROOT / "sources" / f"{m}-source.json")} for m, s in sources.items()},
        "corpus": {m: {"path": str(ROOT / p), "sha256": sha(ROOT / p), "base_seed": BASES[m], "games": 5832}
                   for m, p in v["corpus"].items()},
        "roots": {m: {"path": str(frozen / f"roots-{m}.jsonl"), "sha256": sha(frozen / f"roots-{m}.jsonl")}
                  for m in ("r1", "r2")},
        "freeze_record": {"path": str(frozen / "FREEZE.json"), "sha256": sha(frozen / "FREEZE.json")},
        "seeds": {"namespace": "stage4a-v1",
                  "encoder": "SHA-256 over u64-LE-length-prefixed (namespace, model, root_id, purpose, arm, "
                             "index as 8 LE bytes, path); first 8 digest bytes LE",
                  "purposes": ["select", "eval-world", "eval-inner", "policy", "ties", "bootstrap"],
                  "selection_world": "seed(model, root, 'select', '', i, b'world') shared by E, A, D",
                  "evaluation_world": "seed(model, root, 'eval-world', '', e, b'world'), e = 0..15, no arm",
                  "policy": "seed(model, root, 'policy', '', i, '<phase>/<role>/<0|1>') for main-line roles; "
                            "D inner uses arm 'D' with '<phase>-inner/<ordinal>/<role>/<0|1>'",
                  "ties": "E node order seed(model, root, 'ties', 'E', 0, node key); D root draw "
                          "seed(model, root, 'ties', 'D', 0, b'root-candidates')",
                  "bootstrap": "analysis: random.Random(2026100943) per model",
                  "table": table},
        "sampler": "stage4a-sampler-v1: V4 FutureChanceV3 whole-object determinization + uniform public deck prior "
                   "over the nine registered decks consistent with the opponent's public cards",
        "key_schema": "stage4a-node-v1: SHA-256 chain (parent key, edge bytes, canonical observation, sorted canonical "
                      "menu); handles to first-appearance indices, zcc relative, timestamps ranked, generations dropped; "
                      "unordered lists sorted by masked content; graveyards " + v["graveyards"],
        "constants": {"select_cap": 512000, "eval_worlds": 16, "eval_cap": 65536, "e_ucb_c2": 2.0,
                      "e_max_depth_physical": 32, "e_min_exec_backups": 8, "a_k": 4, "d_top": 2, "d_uniform": 2,
                      "d_horizon_physical": 3, "max_consecutive_rejections": 64, "win": 1, "non_win": 0, "discount": 1},
        "environment": {"OPPONENTS": "T1=<sources>/t1-source.json,A48=<sources>/a48-source.json",
                        "S4A_MODEL": "r1|r2", "decks": "0..8", "priority": "BelowNormal, EcoQoS off",
                        "CUDA_VISIBLE_DEVICES": ""},
        "placement": {"host": "DESKTOP-DJ1C40R (i7-13700K)", "cores": "0,2,4,6,8,10,12,14 (one per physical P-core)",
                      "workers": {"r1": 4, "r2": 4}, "gpu": "N/A",
                      "reason": "worker-hour cap binds: hyperthreads add 10% throughput at 2x worker-hours; "
                                "HaleysPC's slower cores would exceed the conservative admission estimate"},
        "launch": "tools/stage4a/launch_stage4a.py (host_reservation_v1 + host_slots_v1 timed) -> stage4a_queue.py",
        "qualification": {"path": str(ROOT / "out" / "QUALIFICATION.json"), "sha256": sha(ROOT / "out" / "QUALIFICATION.json")},
        "spend_authority": "none (local CPU only)",
        "budgets": {"global_worker_seconds": cap, "corpus_worker_seconds": corpus_ws,
                    "formal_max_wall_seconds_per_job": int((cap - corpus_ws) / 8)},
        "artifacts": {"rows": str(ROOT / "out"), "projected_bytes": 200 * 40_000, "limit_bytes": 64 * 2**30},
    }
    Path(a.out).write_text(json.dumps(manifest, indent=1))
    print(a.out, sha(a.out))


if __name__ == "__main__":
    main()
