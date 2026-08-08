#!/usr/bin/env python3
"""Stages a de-novo store checkpoint as a checkpoint_shadow_stdio_v1
XmageCp7OutcomeDerivative "fixed native state" package.

Reads native_checkpoint_shadow_stdio_v1.rs (mtg-kernel-composed-factorial-v1-codex,
read-only) directly to determine this shape:
  <root>/
    fixed_native_state.json   -- FixedNativeStateManifestV1, byte-exact
                                  serde_json::to_vec_pretty(&manifest) + b"\n"
    checkpoint.state.f32le    -- the raw payload, byte-for-byte copy

Every digest and scalar the manifest needs (parameters_sha256, first_moments_sha256,
second_moments_sha256, model_parameter_sha256, native_state_sha256, payload_sha256,
adam_step, scorer_bias_anchor_f32_bits) is already present in the source store's own
checkpoint.json under the exact same values -- this is a field remapping, not an
independent re-derivation, and it is designed that way in the source format (the
store's own checkpoint writer and this loader both derive from the same
encode_native_train_state_payload_v1 digest set). source_result_sha256 is not
independently cross-checked by the loader (format-checked only, confirmed by
reading fixed_native_state_tests_v1's own test fixture, which uses an arbitrary
constant); this script uses the source checkpoint.json's own file SHA-256 for
traceability instead of an arbitrary value.

authority_kind is "response-exploiter-denovo-screen-v1" (registered in the
outcome-export authority allowlist by commit d1e752a on
mtg-kernel-shadow-scorer-contract-fable; an earlier, unregistered
"denovo-fixed-native-state-transfer-v1" value loaded fine but was rejected at
outcome-export time -- loading and exporting are gated separately).

Directory inventory must be EXACTLY these two files (the loader lists the
directory and requires an exact BTreeSet match), so nothing else may be placed
alongside them in the same directory.

CLI (seed-widening campaign generalization): pass --store-root, --generation,
--staging-root (and optionally --authority-kind, default
"response-exploiter-denovo-screen-v1") to stage exactly ONE (store,
generation) pair per invocation, so the campaign's checkpoint stagings (four
seeds x however many retained checkpoints each; promoted2 needs none, since
it loads through --original-store-root, never through this fixed-native
-state path) are one call each, without editing this file. Invoking with NO
arguments at all keeps staging exactly the original two already-published
pairs (denovo-256 g256, denovo-512 g512) unchanged, for backward
compatibility with the existing staging-manifest-index.json consumers.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

AUTHORITY_KIND_DEFAULT = "response-exploiter-denovo-screen-v1"

FIXED_NATIVE_STATE_SCHEMA_V1 = "mtg-kernel-xmage-fixed-native-state/v1"
FIXED_NATIVE_STATE_PAYLOAD_FILENAME_V1 = "checkpoint.state.f32le"
FIXED_NATIVE_STATE_MANIFEST_FILENAME_V1 = "fixed_native_state.json"
NON_CLAIMS = [
    "external software anchor is not professional-level evidence",
    "terminal win/loss/draw is the only playing-strength outcome",
]


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def stage(store_root: Path, generation: int, authority_kind: str,
          staging_root: Path) -> dict:
    stem = f"update-{generation:08d}"
    checkpoint_path = store_root / "checkpoints" / f"{stem}.checkpoint.json"
    state_path = store_root / "checkpoints" / f"{stem}.state.f32le"
    if not checkpoint_path.is_file() or not state_path.is_file():
        raise SystemExit(f"missing checkpoint files for {store_root} g{generation}")
    checkpoint = json.loads(checkpoint_path.read_bytes())
    train_state = checkpoint["train_state"]
    payload = checkpoint["payload"]
    sections = {section["name"]: section for section in payload["sections"]}
    if payload["byte_count"] != state_path.stat().st_size:
        raise SystemExit(f"payload byte_count mismatch for {store_root} g{generation}")

    source_result_sha256 = sha256_file(checkpoint_path)
    manifest = {
        "schema": FIXED_NATIVE_STATE_SCHEMA_V1,
        "authority_kind": authority_kind,
        "source_result_sha256": source_result_sha256,
        "payload": {
            "filename": FIXED_NATIVE_STATE_PAYLOAD_FILENAME_V1,
            "byte_count": payload["byte_count"],
            "adam_step": train_state["adam_step"],
            "scorer_bias_anchor_f32_bits": train_state["scorer_bias_anchor_f32_bits"],
            "payload_sha256": payload["sha256"],
            "parameters_sha256": sections["parameters"]["sha256"],
            "first_moments_sha256": sections["first_moments"]["sha256"],
            "second_moments_sha256": sections["second_moments"]["sha256"],
            "model_parameter_sha256": train_state["model_parameter_sha256"],
            "native_state_sha256": train_state["state_sha256"],
        },
        "non_claims": NON_CLAIMS,
    }
    if train_state["adam_step"] != generation:
        raise SystemExit(
            f"adam_step ({train_state['adam_step']}) != requested generation "
            f"({generation}) for {store_root}; refusing to stage a mismatched pair")

    staging_root.mkdir(parents=True, exist_ok=False)
    manifest_path = staging_root / FIXED_NATIVE_STATE_MANIFEST_FILENAME_V1
    payload_path = staging_root / FIXED_NATIVE_STATE_PAYLOAD_FILENAME_V1

    # Byte-exact match to Rust's serde_json::to_vec_pretty(&manifest) + b"\n":
    # 2-space indent, ": " key separator, dict insertion order == struct
    # declaration order (already built that way above), LF only (no CRLF),
    # trailing newline appended manually exactly like the Rust test fixture
    # does (to_vec_pretty itself does not add one).
    manifest_text = json.dumps(manifest, indent=2, ensure_ascii=True) + "\n"
    manifest_bytes = manifest_text.encode("ascii")
    with manifest_path.open("wb") as handle:
        handle.write(manifest_bytes)
    payload_bytes = state_path.read_bytes()
    with payload_path.open("wb") as handle:
        handle.write(payload_bytes)

    record = {
        "store_root": str(store_root),
        "generation": generation,
        "authority_kind": authority_kind,
        "staging_root": str(staging_root),
        "manifest_path": str(manifest_path),
        "manifest_sha256": sha256_file(manifest_path),
        "payload_path": str(payload_path),
        "payload_sha256": sha256_file(payload_path),
        "payload_byte_count": payload_path.stat().st_size,
        "source_checkpoint_json_sha256": source_result_sha256,
        "native_state_sha256": train_state["state_sha256"],
        "model_parameter_sha256": train_state["model_parameter_sha256"],
        "adam_step": train_state["adam_step"],
        "scorer_bias_anchor_f32_bits": train_state["scorer_bias_anchor_f32_bits"],
    }
    assert record["payload_sha256"] == payload["sha256"], "staged payload copy sha mismatch"
    return record


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Stage one de-novo checkpoint as a fixed-native-state package. "
                     "With no arguments at all, stages the original two "
                     "already-published pairs (denovo-256 g256, denovo-512 g512) "
                     "unchanged, for backward compatibility.")
    parser.add_argument("--store-root", type=Path,
                         help="Store root containing checkpoints/update-<gen>.checkpoint.json")
    parser.add_argument("--generation", type=int,
                         help="Checkpoint generation (adam_step) to stage")
    parser.add_argument("--staging-root", type=Path,
                         help="Destination directory for this pair's fixed_native_state.json "
                              "and checkpoint.state.f32le (created fresh; must not exist)")
    parser.add_argument("--authority-kind", default=AUTHORITY_KIND_DEFAULT,
                         help=f"FixedNativeStateManifestV1 authority_kind "
                              f"(default: {AUTHORITY_KIND_DEFAULT})")
    parser.add_argument("--index-path", type=Path,
                         help="Where to write this single pair's manifest-index JSON "
                              "(default: <staging-root>.index.json, sibling of staging-root)")
    return parser.parse_args(argv)


def legacy_default_targets(staging_parent: Path) -> list[tuple[Path, int, str, Path]]:
    return [
        (Path(r"D:\mtg-kernel-denovo-screen-v1\denovo-screen-build\attempt-002\denovo-store\run-0\store"),
         256, AUTHORITY_KIND_DEFAULT, staging_parent / "denovo-256"),
        (Path(r"D:\mtg-kernel-denovo-512-screen-v1\denovo-512-screen-build\attempt-002\denovo-512-store\run-0\store"),
         512, AUTHORITY_KIND_DEFAULT, staging_parent / "denovo-512"),
    ]


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv
    args = parse_args(argv)

    if args.store_root is None and args.generation is None and args.staging_root is None:
        # Legacy no-argument mode: exactly the original two already-published
        # pairs, unchanged behavior and unchanged output shape.
        staging_parent = Path(r"D:\mtg-kernel-cp7-anchor-panel-v3\denovo-fixed-state-staging")
        targets = legacy_default_targets(staging_parent)
        records = []
        for store_root, generation, authority_kind, staging_root in targets:
            record = stage(store_root, generation, authority_kind, staging_root)
            records.append(record)
            print(json.dumps(record, indent=2))
        manifest_index_path = staging_parent / "staging-manifest-index.json"
        manifest_index_path.write_text(json.dumps(records, indent=2, sort_keys=True) + "\n",
                                       encoding="utf-8", newline="\n")
        print(f"\nwritten index: {manifest_index_path}")
        return 0

    # Single-pair CLI mode (seed-widening campaign): one (store, generation)
    # per invocation. --store-root, --generation, and --staging-root are all
    # required together in this mode.
    missing = [name for name, value in (
        ("--store-root", args.store_root),
        ("--generation", args.generation),
        ("--staging-root", args.staging_root),
    ) if value is None]
    if missing:
        raise SystemExit(
            f"single-pair mode requires all of --store-root, --generation, "
            f"--staging-root (missing: {', '.join(missing)})")

    record = stage(args.store_root, args.generation, args.authority_kind, args.staging_root)
    print(json.dumps(record, indent=2))
    index_path = args.index_path
    if index_path is None:
        index_path = args.staging_root.with_name(args.staging_root.name + ".index.json")
    index_path.write_text(json.dumps([record], indent=2, sort_keys=True) + "\n",
                           encoding="utf-8", newline="\n")
    print(f"\nwritten index: {index_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
