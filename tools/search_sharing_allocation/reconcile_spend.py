"""Reconcile the owned, deleted first pod from retained receipts only."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import shutil

from cloud_lease import lease_guard


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def reconcile(root, cold):
    lease_root = root / "lease-002"
    paths = {"original_lease": lease_root / "lease/lease.json",
             "provider_absence": lease_root / "external-guard-completion.json",
             "preservation": lease_root / "preservation.json"}
    documents = {name: json.loads(path.read_text()) for name, path in paths.items()}
    original, absence, preservation = (documents[name] for name in paths)
    lease_guard.validate(original)
    pod_id = "esbt2j651ohkxh"
    if (original["name"] != "search-sharing-allocation-20261013-lease-002"
            or absence.get("pod_id") != pod_id or absence.get("provider_absent") is not True
            or preservation.get("pod_id") != pod_id or preservation.get("copied") is not True
            or not original["created_epoch"] <= preservation["epoch"] <= absence["epoch"]):
        raise ValueError("verified owned-pod absence and prior preservation required")
    cost = lease_guard.decision(original, absence["epoch"], original["rate_ceiling_usd_hour"])
    carry = math.ceil(cost["cumulative_conservative_usd"] * 100) / 100
    ledger_path = root / "CLOUD-SPEND.json"
    if ledger_path.exists():
        prior = json.loads(ledger_path.read_text())
        if prior.get("paid_pod_count") != 1 or prior.get("leases", [{}])[0].get("pod_id") != pod_id:
            raise ValueError("existing authoritative ledger contains additional allocations")
    sources = {name: {"path": str(path), "sha256": sha(path)} for name, path in paths.items()}
    ledger = {"schema": "search-sharing-allocation-cloud-spend/v1",
              "paid_pod_count": 1, "original_deadline_epoch": original["deadline_epoch"],
              "original_total_cap_usd": original["total_cap_usd"],
              "final_cumulative_conservative_usd": cost["cumulative_conservative_usd"],
              "rounded_carry_forward_usd": carry,
              "remaining_cumulative_cap_after_carry_usd": original["total_cap_usd"] - carry,
              "actual_invoice_claim": False,
              "accounting": "Original rate ceiling plus disk, recovery reserve and safety multiplier through confirmed absence; not a provider invoice.",
              "sources": sources, "cold_copy_directory": str(cold),
              "leases": [{"pod_id": pod_id, "original_lease": original,
                          "provider_absent_epoch": absence["epoch"],
                          "final_increment_conservative_usd": cost["increment_conservative_usd"],
                          "final_cumulative_conservative_usd": cost["cumulative_conservative_usd"],
                          "rounded_carry_forward_usd": carry}]}
    temporary = ledger_path.with_suffix(".json.tmp")
    temporary.write_text(json.dumps(ledger, indent=2) + "\n")
    temporary.replace(ledger_path)
    cold.mkdir(parents=True, exist_ok=True)
    copied = []
    for source in [ledger_path, paths["provider_absence"], paths["preservation"]]:
        target = cold / source.name
        shutil.copy2(source, target)
        if sha(source) != sha(target):
            raise ValueError("cold-copy hash differs: " + source.name)
        copied.append({"source": str(source), "cold": str(target), "sha256": sha(source)})
    return {"ledger": str(ledger_path), "final_conservative_usd": cost["cumulative_conservative_usd"],
            "rounded_carry_forward_usd": carry, "hash_verified_copies": copied}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--cold", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(reconcile(args.root, args.cold), indent=2))
