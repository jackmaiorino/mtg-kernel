"""Compare the native ignored-test emitter with Python, bit for bit."""
import argparse
from hashlib import sha256
import json
from pathlib import Path

import torch
from mtg_kernel_rl import features_v6


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("emitter_log", type=Path)
    parser.add_argument("--expected-cases", type=int, default=24)
    args = parser.parse_args()
    torch.set_num_threads(1)
    raw = args.emitter_log.read_bytes()
    cases = [json.loads(line.split("NATIVE_FLAT_V3_FIXTURE=", 1)[1])
             for line in raw.decode("utf-8-sig").splitlines()
             if "NATIVE_FLAT_V3_FIXTURE=" in line]
    if len(cases) != args.expected_cases or len({c["name"] for c in cases}) != len(cases):
        raise ValueError("missing or duplicate native fixture cases")
    scalars = 0
    for case in cases:
        encoded = features_v6.encode_decision(case["observation"], case["legal_actions"])
        if len(case["expected"]) != 13:
            raise ValueError("fixture must contain all thirteen tensors")
        for key, expected in case["expected"].items():
            tensor = getattr(encoded, key[:-5] if key.endswith("_bits") else key).contiguous()
            actual = ([int(x) & 0xffffffff for x in tensor.view(torch.int32).flatten().tolist()]
                      if key.endswith("_bits") else tensor.flatten().tolist())
            if actual != expected:
                raise ValueError(f"native/Python mismatch: {case['name']} {key}")
            scalars += len(actual)
    print(json.dumps({"cases": len(cases), "tensors_per_case": 13,
                      "matched_scalars": scalars, "emitter_sha256": sha256(raw).hexdigest()}))


if __name__ == "__main__":
    main()
