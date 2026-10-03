#!/usr/bin/env python3
"""Offline host refold line for line (b) receipt directories.

FABLE-REVIEW-20260927 change 3 keeps the host refold check as a separate
cheap line beside the device-logit envelope. The executable pinned for the
line (b) receipts (822d0426, code 41ceac3b) predates the receipt field that
carries it (dbab4cd0), so this tool computes the line from the receipt's
own artifacts: for every distinct teacher packet in a directory written by
line_b_receipts_v1.py and every selected root, the binary32 refold of the
sampled action's log-probability (a numpy replica of the update's
selected_log_softmax over the transported collection logits: maximum by
fold, binary32 exp and sum in menu order, binary32 ln) against the
collection sampler's binary64 log-probability of the same action. The
replica may differ from the Rust refold by an ulp of binary32 exp or ln,
so the line reads the discrepancy's scale, not a bit identity.

Usage: line_b_host_refold_v1.py RECEIPT_DIRECTORY > host-refold.json
"""
import hashlib
import json
import math
import struct
import sys
from pathlib import Path

import numpy as np


def binary32(bits):
    return np.frombuffer(struct.pack("<I", bits), dtype=np.float32)[0]


def refold32(bits, selected):
    logits = [binary32(value) for value in bits]
    maximum = np.float32(-np.inf)
    for logit in logits:
        maximum = max(maximum, logit)
    total = np.float32(0.0)
    for logit in logits:
        total = np.float32(total + np.exp(np.float32(logit - maximum)))
    log_sum = np.float32(np.log(total))
    return float(np.float32(np.float32(logits[selected] - maximum) - log_sum))


def sampler64(bits, selected):
    logits = [float(binary32(value)) for value in bits]
    top = max(logits)
    total = math.fsum(math.exp(logit - top) for logit in logits)
    return (logits[selected] - top) - math.log(total)


def main(directory):
    directory = Path(directory)
    packets = {}
    for command_path in sorted(directory.glob("*.command.json")):
        command = json.loads(command_path.read_text())
        if not (command.get("line_b") or {}).get("teacher"):
            continue
        label = command_path.name[: -len(".command.json")]
        packet_path = directory / label / "line-b-teacher-packet.json"
        if not packet_path.exists():
            continue
        raw = packet_path.read_bytes()
        digest = hashlib.sha256(raw).hexdigest()
        if digest in packets:
            packets[digest]["updates"].append(label)
            continue
        packet = json.loads(raw)
        roots = []
        for game in packet["games"]:
            if game["kind"] != "root":
                continue
            pin = command["trajectories"][game["trajectory_index"]]
            trajectory = json.loads(Path(pin["path"]).read_text())
            decision = trajectory["decisions"][game["decision_index"]]
            if decision["logits"] != game["collection_logits"]:
                raise SystemExit(f"{label}: root row differs from its trajectory")
            selected = decision["selected"]
            refold = refold32(game["collection_logits"], selected)
            sampler = sampler64(game["collection_logits"], selected)
            roots.append(
                {
                    "trajectory_index": game["trajectory_index"],
                    "decision_index": game["decision_index"],
                    "width": len(game["collection_logits"]),
                    "selected": selected,
                    "host_refold_log_probability": refold,
                    "sampler_log_probability": sampler,
                    "abs_discrepancy": abs(refold - sampler),
                }
            )
        packets[digest] = {
            "packet_sha256": digest,
            "updates": [label],
            "roots": roots,
            "max_abs_discrepancy": max((root["abs_discrepancy"] for root in roots), default=None),
        }
    print(
        json.dumps(
            {
                "schema": "line-b-host-refold-line/v1",
                "claim": "offline binary32 replica of the update's host refold against the sampler's binary64 value; engineering only",
                "directory": str(directory),
                "packets": list(packets.values()),
            },
            indent=1,
        )
    )


if __name__ == "__main__":
    main(sys.argv[1])
