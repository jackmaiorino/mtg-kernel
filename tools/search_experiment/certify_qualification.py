"""Certify immutable same-pool legacy batches after the prospective pool guard repair."""
import argparse
import copy
import json
from pathlib import Path

from launch import (allocation, allocator, checked, compatibility, digest, load,
                    publish, qualify_records, require, validate)


def certify(manifest, paths, out):
    target = load(manifest)
    validate(target, 1, "qualify")
    require(target.get("core_pool"), "certification needs an exact requested pool")
    slots = allocator(target)
    expected = allocation(target, slots.parse_cores(target["core_pool"]))
    target_identity = compatibility(target)
    enriched, provenance = [], []
    for path in paths:
        batch = load(path)
        root = Path(path).resolve().parent.parent
        dispatch_path = root / f"DISPATCH-{batch['label']}.json"
        dispatch = load(dispatch_path)
        require(dispatch.get("state") == "completed" and dispatch.get("claim_release_verified") is True,
                "legacy batch needs completed dispatch and verified release")
        require(Path(dispatch["completion"]).resolve() == Path(path).resolve(), "dispatch names another batch")
        source_asset = dispatch["manifest"]
        source = load(checked(source_asset))
        legacy = not source.get("core_pool") and not batch.get("allocation")
        if not legacy:
            require(source.get("core_pool") == target["core_pool"] and batch.get("allocation") == expected,
                    "native allocation record differs from requested pool")
        require(batch["compatible"] == compatibility(source), "legacy batch/source identity differs")
        require(source["commit"] == target["commit"] and source.get("toolchain") == target.get("toolchain"),
                "native source or toolchain differs")
        require(source["jobs"] == target["jobs"], "native jobs, seeds or limits differ")
        old_helper, new_helper = source["runtime_helper"], target["runtime_helper"]
        require(old_helper["path"] == new_helper["path"], "guard helper path differs")
        require(sum(asset == old_helper for asset in source["inputs"]) == 1
                and sum(asset == new_helper for asset in target["inputs"]) == 1,
                "guard asset must be explicitly present once")
        for asset in source["inputs"]:
            if asset != old_helper:
                checked(asset)
        adjusted = copy.deepcopy(batch["compatible"])
        adjusted.pop("allocation", None)
        hashes = adjusted["input_sha256s"]
        hashes.remove(old_helper["sha256"])
        hashes.append(new_helper["sha256"])
        adjusted["input_sha256s"] = sorted(hashes)
        without_pool = {key: value for key, value in target_identity.items() if key != "allocation"}
        require(adjusted == without_pool, "legacy scientific identity differs beyond the guard asset")
        log_path = Path(dispatch["log"])
        claims = [record for line in log_path.read_text().splitlines() if line.startswith("{")
                  for record in [json.loads(line)] if "claimed" in record]
        require(len(claims) == 1 and claims[0]["claimed"] == dispatch["claim_id"], "claim log identity differs")
        observed = sorted(slots.parse_cores(claims[0]["cores"]))
        require(observed == expected["cores"], "legacy outer claim differs from requested pool")
        require(all(row and row.get("exit_code") == 0 and row.get("affinity_core") in observed
                    and digest(row["output"]) == row["output_sha256"] for row in batch["results"]),
                "legacy native completion, affinity or output bytes differ")
        record = copy.deepcopy(batch)
        record.update(compatible=target_identity, allocation=expected)
        enriched.append(record)
        provenance.append({"batch": {"path": str(path), "sha256": digest(path)},
                           "legacy_metadata_added": legacy,
                           "original_compatible": batch["compatible"], "source_manifest": source_asset,
                           "dispatch": {"path": str(dispatch_path), "sha256": digest(dispatch_path)},
                           "claim_log": {"path": str(log_path), "sha256": digest(log_path)},
                           "observed_outer_cores": observed,
                           "topology_basis": "current supported topology on the same named host",
                           "old_guard": {"runtime_commit": source["runtime_commit"], "runtime": old_helper,
                                         "host_slots": source["host_slots"], "host_reservation": source["host_reservation"]},
                           "new_guard": {"runtime_commit": target["runtime_commit"], "runtime": new_helper,
                                         "host_slots": target["host_slots"], "host_reservation": target["host_reservation"]}})
    receipt = qualify_records(enriched, paths)
    receipt.update(certification="missing-legacy-core-pool-metadata-only",
                   target_manifest={"path": str(manifest), "sha256": digest(manifest)},
                   certification_provenance=provenance)
    publish(out, receipt)
    return receipt


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", required=True)
    parser.add_argument("--batch", action="append", required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    print(json.dumps(certify(args.manifest, args.batch, args.out)))
