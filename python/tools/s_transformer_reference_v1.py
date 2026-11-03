#!/usr/bin/env python3
"""Float64 PyTorch reference and receipts for native S (mtg-kernel `s_transformer_v1`, W5 E3).

`parity --dir D --receipt R` reads a `s_transformer_v1 parity-dump` directory and recomputes, in float64 with
autograd, every batch's RT1 GAE loss terms, the per-tensor gradient and one Adam step. It compares them with the
native float32 results against E3's bars: each loss term relative error at most 1e-4, per-tensor gradient relative
L2 error at most 1e-3, and per-tensor Adam parameter-change relative L2 error at most 1e-3.

The Adam bar is read in two parts (W5 amendment A2, collab LANES/lead/w5/RULINGS-r0.md). `passed`, and the exit
status, require all four of:
- loss terms within 1e-4;
- per-tensor gradients within 1e-3;
- (a) the update one Adam step adds, before the parameter's own float32 rounding, from a warmed optimizer state (S's
  moments after a few native updates, shared by both sides), within 1e-3;
- (b) the change of the stored float32 parameters after one Adam step from a fresh optimizer (zero moments, step 1,
  as XS starts native RL), within 1e-3 after excluding the elements whose float64 reference gradient is at or below
  the declared float32 noise floor: 1e-4 of the tensor's float64 gradient RMS. That is ten times the largest
  per-tensor gradient error the parity has measured (7.9e-6 relative L2). The first step moves every parameter by
  about the learning rate in the direction of its gradient's sign, so an element within float32 noise of zero can
  step the other way than in float64.
The receipt states the floor and reports the excluded and opposite-sign counts, and keeps the literal number (the
fresh step's stored change over every element) beside the two parts. Diagnostics: the floor float32 storage alone
sets on a stored change (at lr 1e-4 a parameter near 1 holds its change to about 6e-4 of it).

`compare-logits A B --receipt R [--bar 1e-5]` compares two `logits-dump` files (for example Linux and Windows builds
on the same states): maximum absolute logit and value difference at most the bar, and the same argmax except where
either side's top two logits are within the bar.

`export-values` and `torch-logits` serve E4's S0 export: the first writes a PyTorch S state dict for
`s_transformer_v1 import-values`; the second scores tensor-dump states with the state dict in float64, for
`compare-logits --bar 1e-4` against the native checkpoint's `logits-dump`.

The model is the bignet pilot's `ObjectTransformer` (collab LANES/bignet-pilot-20261031/code/models.py) written
functionally: dropout 0, no autocast, state digest dropped, value `tanh(z / 2)`. The gauge parameters native S
defines as fixed (`policy.2.bias` and the key third of every `qkv.bias`) get a zero gradient here too.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import platform
import struct
import sys
from pathlib import Path

import numpy as np
import torch
import torch.nn.functional as F

OBJ, EDGE, ACT, REF, STATE, GROUPS = 98, 41, 195, 25, 219, 20
STATE_KEEP = STATE - 96
BARS = {"loss_relative": 1e-4, "gradient_relative_l2": 1e-3, "adam_change_relative_l2": 1e-3}
RELATIVE_BARS = {"loss_relative": 1e-4, "gradient_relative_l2": 1e-3, "adam_update_relative_l2": 1e-3}
# W5 amendment A2: the bars `passed` reads.
A2_BARS = {"loss_relative": 1e-4, "gradient_relative_l2": 1e-3, "adam_a_warmed_update_relative_l2": 1e-3,
           "adam_b_fresh_stored_change_above_noise_floor_relative_l2": 1e-3}
# The declared float32 noise floor of a reference gradient element, as a fraction of its tensor's float64 gradient RMS.
NOISE_FLOOR_FRACTION = 1e-4
# Parameters whose gradient S defines as exactly zero (softmax shift invariance); accepted by A2 item 3.
ZERO_GRADIENT_SETS = ["policy.2.bias", "the key third [d, 2d) of every blocks.<i>.qkv.bias"]
LOGIT_BAR = 1e-5
F64 = torch.float64

FIELDS = (  # name, dtype, per-row width, count key (s4a-tensor-dump record)
    ("state", np.float32, 1, "s"),
    ("object_features", np.float32, OBJ, "n"),
    ("object_card_ids", np.int32, 1, "n"),
    ("object_groups", np.int32, 1, "n"),
    ("object_node_ids", np.int32, 1, "n"),
    ("edge_features", np.float32, EDGE, "e"),
    ("edge_source", np.int32, 1, "e"),
    ("edge_target", np.int32, 1, "e"),
    ("action_features", np.float32, ACT, "a"),
    ("action_ref_features", np.float32, REF, "r"),
    ("action_ref_card_ids", np.int32, 1, "r"),
    ("action_ref_action_indices", np.int32, 1, "r"),
    ("action_ref_node_indices", np.int32, 1, "r"),
    ("logits", np.float32, 1, "a"),
    ("value", np.float32, 1, "one"),
)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def decode_records(buf: bytes) -> list[dict]:
    out, pos = [], 0
    while pos < len(buf):
        n, e, a, r, s, selected, legal, actor = np.frombuffer(buf, np.uint32, 8, pos)
        pos += 32
        counts = {"n": int(n), "e": int(e), "a": int(a), "r": int(r), "s": int(s), "one": 1}
        rec = {"selected": int(selected)}
        for name, dtype, width, key in FIELDS:
            count = counts[key] * width
            arr = np.frombuffer(buf, dtype, count, pos)
            pos += count * 4
            rec[name] = arr.reshape(-1, width) if width > 1 else arr
        if s != STATE or legal != a:
            raise ValueError(f"bad record: state {s}, legal {legal}, actions {a}")
        out.append(rec)
    return out


def collate(recs: list[dict]) -> dict:
    b = len(recs)
    no = [len(r["object_card_ids"]) for r in recs]
    na = [len(r["action_features"]) for r in recs]
    obase = np.concatenate([[0], np.cumsum(no)[:-1]]).astype(np.int64)
    abase = np.concatenate([[0], np.cumsum(na)[:-1]]).astype(np.int64)
    ne = [len(r["edge_source"]) for r in recs]
    nr = [len(r["action_ref_card_ids"]) for r in recs]
    omax, amax = max(no), max(na)
    key_mask = np.zeros((b, 1 + omax + amax), bool)
    key_mask[:, 0] = True
    for i in range(b):
        key_mask[i, 1:1 + no[i]] = True
        key_mask[i, 1 + omax:1 + omax + na[i]] = True

    def cat(key, dtype, width=None):
        parts = [np.asarray(r[key]) for r in recs]
        arr = np.concatenate(parts).astype(dtype) if parts else np.zeros(0, dtype)
        return arr.reshape(-1, width) if width else arr

    out = {
        "state": np.stack([r["state"] for r in recs]).astype(np.float64),
        "obj_feat": cat("object_features", np.float64, OBJ),
        "obj_card": cat("object_card_ids", np.int64),
        "obj_group": cat("object_groups", np.int64),
        "obj_batch": np.repeat(np.arange(b), no),
        "obj_pos": np.concatenate([np.arange(k) for k in no]),
        "edge_feat": cat("edge_features", np.float64, EDGE) if sum(ne) else np.zeros((0, EDGE)),
        "edge_src": cat("edge_source", np.int64) + np.repeat(obase, ne),
        "edge_tgt": cat("edge_target", np.int64) + np.repeat(obase, ne),
        "act_feat": cat("action_features", np.float64, ACT),
        "act_batch": np.repeat(np.arange(b), na),
        "act_pos": np.concatenate([np.arange(k) for k in na]),
        "ref_feat": cat("action_ref_features", np.float64, REF) if sum(nr) else np.zeros((0, REF)),
        "ref_card": cat("action_ref_card_ids", np.int64),
        "ref_action": cat("action_ref_action_indices", np.int64) + np.repeat(abase, nr),
        "ref_node": cat("action_ref_node_indices", np.int64) + np.repeat(obase, nr),
        "key_mask": key_mask,
    }
    t = {k: torch.from_numpy(np.ascontiguousarray(v)) for k, v in out.items()}
    t["omax"], t["amax"], t["na"] = omax, amax, na
    return t


def forward(P: dict, cfg: dict, bt: dict):
    """models.py ObjectTransformer.forward in float64; returns (per-record logits list, value logit z)."""
    d, heads = cfg["d_model"], cfg["heads"]
    dh = d // heads
    b, omax, amax = bt["state"].shape[0], bt["omax"], bt["amax"]
    l = 1 + omax + amax

    def lin(x, name):
        return x @ P[name + ".weight"].T + P[name + ".bias"]

    def mlp(x, name):
        return lin(F.gelu(lin(x, name + ".0")), name + ".2")

    card = P["card.weight"]
    obj = mlp(torch.cat([bt["obj_feat"], F.embedding(bt["obj_card"], card, padding_idx=0)], -1), "obj_in")
    obj = obj + P["group.weight"][bt["obj_group"]]
    act = mlp(bt["act_feat"], "act_in")
    if bt["ref_feat"].shape[0] > 0:
        rh = mlp(torch.cat([bt["ref_feat"], F.embedding(bt["ref_card"], card, padding_idx=0)], -1), "ref_in")
        act = act.index_add(0, bt["ref_action"], rh)
    x = torch.zeros(b, l, d, dtype=F64)
    x[:, 0] = mlp(bt["state"][:, :STATE_KEEP], "state_in")
    x[bt["obj_batch"], 1 + bt["obj_pos"]] = obj
    x[bt["act_batch"], 1 + omax + bt["act_pos"]] = act
    bias = torch.zeros(b, heads, l, l, dtype=F64)
    hh = torch.arange(heads)
    if bt["edge_feat"].shape[0] > 0:
        eb = lin(bt["edge_feat"], "edge_bias").view(-1, 2, heads)
        eg = bt["obj_batch"][bt["edge_src"]]
        s, t = 1 + bt["obj_pos"][bt["edge_src"]], 1 + bt["obj_pos"][bt["edge_tgt"]]
        bias = bias.index_put((eg[:, None], hh[None], s[:, None], t[:, None]), eb[:, 0], accumulate=True)
        bias = bias.index_put((eg[:, None], hh[None], t[:, None], s[:, None]), eb[:, 1], accumulate=True)
    if bt["ref_feat"].shape[0] > 0:
        rg = bt["act_batch"][bt["ref_action"]]
        ai = 1 + omax + bt["act_pos"][bt["ref_action"]]
        oi = 1 + bt["obj_pos"][bt["ref_node"]]
        n = rg.shape[0]
        rb = P["ref_bias"]
        bias = bias.index_put((rg[:, None], hh[None], ai[:, None], oi[:, None]), rb[0].expand(n, heads), accumulate=True)
        bias = bias.index_put((rg[:, None], hh[None], oi[:, None], ai[:, None]), rb[1].expand(n, heads), accumulate=True)
    bias = bias.masked_fill(~bt["key_mask"][:, None, None, :], float("-inf"))
    for i in range(cfg["layers"]):
        p = f"blocks.{i}"
        h1 = F.layer_norm(x, (d,), P[p + ".ln1.weight"], P[p + ".ln1.bias"], 1e-5)
        q, k, v = lin(h1, p + ".qkv").view(b, l, 3, heads, dh).permute(2, 0, 3, 1, 4)
        att = torch.softmax(q @ k.transpose(-1, -2) / math.sqrt(dh) + bias, -1) @ v
        x = x + lin(att.transpose(1, 2).reshape(b, l, d), p + ".proj")
        h2 = F.layer_norm(x, (d,), P[p + ".ln2.weight"], P[p + ".ln2.bias"], 1e-5)
        x = x + lin(F.gelu(lin(h2, p + ".ff.0")), p + ".ff.2")
    x = F.layer_norm(x, (d,), P["ln.weight"], P["ln.bias"], 1e-5)
    state_h = x[:, 0]
    act_out = x[bt["act_batch"], 1 + omax + bt["act_pos"]]
    flat = mlp(torch.cat([state_h[bt["act_batch"]], act_out], -1), "policy").squeeze(-1)
    z = mlp(state_h, "value").squeeze(-1)
    logits, start = [], 0
    for k in bt["na"]:
        logits.append(flat[start:start + k])
        start += k
    return logits, z


def gauge_mask(name: str, shape, d: int):
    if name == "policy.2.bias":
        return torch.ones(shape, dtype=torch.bool)
    if name.startswith("blocks.") and name.endswith(".qkv.bias"):
        m = torch.zeros(shape, dtype=torch.bool)
        m[d:2 * d] = True
        return m
    return None


def rel_l2(native: np.ndarray, ref: np.ndarray) -> float:
    """Relative L2 error; infinite when either side holds a non-finite value, so it can never pass a bar."""
    if not (np.all(np.isfinite(native)) and np.all(np.isfinite(ref))):
        return math.inf
    den = float(np.linalg.norm(ref))
    num = float(np.linalg.norm(native - ref))
    if den == 0.0:
        return 0.0 if num == 0.0 else math.inf
    return num / den


def parity(args) -> int:
    root = Path(args.dir)
    layout = json.loads((root / "layout.json").read_text())
    batches = json.loads((root / "batches.json").read_text())
    cfg = layout["config"]
    total = layout["total"]
    d = cfg["d_model"]
    p0 = np.frombuffer((root / "params.bin").read_bytes(), "<f4").astype(np.float64)
    native = np.frombuffer((root / "native.bin").read_bytes(), "<f4")
    records = decode_records((root / "decisions.td").read_bytes())
    c = float(np.frombuffer(struct.pack("<I", batches["value_coefficient_bits"]), "<f4")[0])
    lr = float(np.frombuffer(struct.pack("<I", batches["learning_rate_bits"]), "<f4")[0])
    tensors = layout["tensors"]
    per = 3 + 4 * total
    warm_m = np.frombuffer((root / "warm_first.bin").read_bytes(), "<f4").astype(np.float64)
    warm_v = np.frombuffer((root / "warm_second.bin").read_bytes(), "<f4").astype(np.float64)
    warm_t = int(batches["warm_adam_step"]) + 1
    if native.size != per * len(batches["batches"]):
        raise SystemExit("native.bin length does not match the batches")
    worst = {"loss": {}, "gradient": {}, "adam": {}, "fresh": {}, "fresh_far": {}, "stored": {}, "floor": {},
             "fresh_stored": {}, "fresh_floor": {}, "fresh_stored_far": {}}
    opposite_kept = 0
    fresh_excluded = 0
    fresh_near_zero = fresh_sign_flips = 0
    rows = []
    b1, b2, eps = 0.9, 0.999, 1e-8
    for index, batch in enumerate(batches["batches"]):
        nat = native[index * per:(index + 1) * per].astype(np.float64)
        nat_terms, nat_grad = nat[:3], nat[3:3 + total]
        nat_fresh = nat[3 + total:3 + 2 * total]
        nat_warm = nat[3 + 2 * total:3 + 3 * total]
        nat_warm_after = nat[3 + 3 * total:]
        P = {}
        for t in tensors:
            arr = p0[t["offset"]:t["offset"] + int(np.prod(t["shape"]))].reshape(t["shape"])
            P[t["name"]] = torch.tensor(arr, dtype=F64, requires_grad=True)
        flat_ids = [r for g in batch["groups"] for r in g["records"]]
        bt = collate([records[r] for r in flat_ids])
        logits, z = forward(P, cfg, bt)
        value = torch.tanh(0.5 * z)
        position = {r: i for i, r in enumerate(flat_ids)}
        policy_sum = torch.zeros((), dtype=F64)
        value_sum = torch.zeros((), dtype=F64)
        for g in batch["groups"]:
            adv = float(np.frombuffer(struct.pack("<I", g["advantage_bits"]), "<f4")[0])
            tgt = float(np.frombuffer(struct.pack("<I", g["value_target_bits"]), "<f4")[0])
            joint = torch.zeros((), dtype=F64)
            for r in g["records"]:
                lp = torch.log_softmax(logits[position[r]], -1)
                joint = joint + lp[records[r]["selected"]]
            policy_sum = policy_sum - joint * adv
            value_sum = value_sum + (value[position[g["records"][0]]] - tgt) ** 2
        G = len(batch["groups"])
        loss = (policy_sum + c * value_sum) / G
        loss.backward()
        ref_terms = np.array([float(policy_sum.detach()) / G, float(value_sum.detach()) / G, float(loss.detach())])
        row = {"batch": index, "decisions": len(flat_ids)}
        for k, name in enumerate(["policy_term", "value_term", "loss"]):
            err = abs(nat_terms[k] - ref_terms[k]) / abs(ref_terms[k])
            if not math.isfinite(err):
                err = math.inf
            row[name + "_relative_error"] = err
            worst["loss"][name] = max(worst["loss"].get(name, 0.0), err)
        for t in tensors:
            name, off = t["name"], t["offset"]
            size = int(np.prod(t["shape"]))
            grad = P[name].grad.detach().clone() if P[name].grad is not None else torch.zeros_like(P[name])
            mask = gauge_mask(name, grad.shape, d)
            if mask is not None:
                grad[mask] = 0.0
            g = grad.numpy().reshape(-1)
            gerr = rel_l2(nat_grad[off:off + size], g)
            worst["gradient"][name] = max(worst["gradient"].get(name, 0.0), gerr)
            base = p0[off:off + size]
            # One Adam step from the warmed state (step warm_t), float64: the Adam bar.
            m = b1 * warm_m[off:off + size] + (1 - b1) * g
            v = b2 * warm_v[off:off + size] + (1 - b2) * g * g
            delta_ref = -(lr / (1 - b1 ** warm_t)) * m / (np.sqrt(v) / math.sqrt(1 - b2 ** warm_t) + eps)
            aerr = rel_l2(nat_warm[off:off + size], delta_ref)
            worst["adam"][name] = max(worst["adam"].get(name, 0.0), aerr)
            # The stored parameters' change, with the floor float32 storage alone sets: the reference step rounded
            # to float32 (p0 + delta is stored with half an ulp of p, which at lr 1e-4 and |p| near 1 is about
            # 6e-4 of the step).
            stored = rel_l2(nat_warm_after[off:off + size] - base, delta_ref)
            floor = rel_l2((base + delta_ref).astype(np.float32).astype(np.float64) - base, delta_ref)
            worst["stored"][name] = max(worst["stored"].get(name, 0.0), stored)
            worst["floor"][name] = max(worst["floor"].get(name, 0.0), floor)
            # One Adam step from zero moments (step 1): a sign-like map of the gradient. Elements whose reference
            # gradient is at or below the noise floor are excluded from part (b).
            m = (1 - b1) * g
            v = (1 - b2) * g * g
            delta_ref = -(lr / (1 - b1)) * m / (np.sqrt(v) / math.sqrt(1 - b2) + eps)
            delta_nat = nat_fresh[off:off + size]
            worst["fresh"][name] = max(worst["fresh"].get(name, 0.0), rel_l2(delta_nat, delta_ref))
            rms = float(np.sqrt(np.mean(g * g))) if size else 0.0
            far = np.abs(g) > NOISE_FLOOR_FRACTION * rms
            fresh_near_zero += int(np.count_nonzero(~far & (g != 0)))
            fresh_excluded += int(np.count_nonzero(~far))
            opposite = np.sign(nat_grad[off:off + size]) != np.sign(g)
            fresh_sign_flips += int(np.count_nonzero(opposite))
            opposite_kept += int(np.count_nonzero(opposite & far))
            worst["fresh_far"][name] = max(worst["fresh_far"].get(name, 0.0),
                                           rel_l2(delta_nat[far], delta_ref[far]) if far.any() else 0.0)
            # The literal bar: the stored float32 parameters after the fresh step (Rust adds the f32 update to the
            # f32 parameter; numpy's float32 add rounds the same way).
            base32 = base.astype(np.float32)
            stored_fresh = (base32 + nat_fresh[off:off + size].astype(np.float32)).astype(np.float64) - base
            worst["fresh_stored"][name] = max(worst["fresh_stored"].get(name, 0.0), rel_l2(stored_fresh, delta_ref))
            worst["fresh_stored_far"][name] = max(worst["fresh_stored_far"].get(name, 0.0),
                                                  rel_l2(stored_fresh[far], delta_ref[far]) if far.any() else 0.0)
            fresh_floor = rel_l2((base + delta_ref).astype(np.float32).astype(np.float64) - base, delta_ref)
            worst["fresh_floor"][name] = max(worst["fresh_floor"].get(name, 0.0), fresh_floor)
        rows.append(row)
    maxima = {
        "loss_relative": max(worst["loss"].values()),
        "gradient_relative_l2": max(worst["gradient"].values()),
        "adam_change_relative_l2": max(worst["fresh_stored"].values()),
    }
    literal_passed = all(maxima[k] <= BARS[k] for k in BARS)
    a2 = {
        "loss_relative": maxima["loss_relative"],
        "gradient_relative_l2": maxima["gradient_relative_l2"],
        "adam_a_warmed_update_relative_l2": max(worst["adam"].values()),
        "adam_b_fresh_stored_change_above_noise_floor_relative_l2": max(worst["fresh_stored_far"].values()),
    }
    passed = all(a2[k] <= A2_BARS[k] for k in A2_BARS)
    arithmetic = {
        "loss_relative": maxima["loss_relative"],
        "gradient_relative_l2": maxima["gradient_relative_l2"],
        "adam_update_relative_l2": max(worst["adam"].values()),
    }
    arithmetic_passed = all(arithmetic[k] <= RELATIVE_BARS[k] for k in RELATIVE_BARS)
    fresh = {
        "description": "one Adam step from zero moments (step 1): the update before storage, with and without "
                       "near-zero gradient elements, and the float32 storage floor of the stored change",
        "max_relative_l2": max(worst["fresh"].values()),
        "max_float32_storage_floor": max(worst["fresh_floor"].values()),
        "per_tensor_stored_change_relative_l2": worst["fresh_stored"],
        "per_tensor_float32_storage_floor": worst["fresh_floor"],
        "max_relative_l2_excluding_near_zero_gradients": max(worst["fresh_far"].values()),
        "worst_tensor": max(worst["fresh"], key=worst["fresh"].get),
        "near_zero_gradient_elements": fresh_near_zero,
        "gradient_sign_differences": fresh_sign_flips,
        "per_tensor_relative_l2": worst["fresh"],
    }
    receipt = {
        "schema": "s-parity-receipt-v1",
        "passed": passed,
        "rule": "W5 amendment A2 (collab LANES/lead/w5/RULINGS-r0.md): the Adam bar in two parts, (a) and (b)",
        "bars": A2_BARS,
        "a2_maxima": a2,
        "noise_floor": {
            "definition": "an element is excluded from part (b) when |float64 reference gradient| is at or below "
                          "this fraction of its tensor's float64 gradient RMS",
            "fraction_of_tensor_rms": NOISE_FLOOR_FRACTION,
            "excluded_elements": fresh_excluded,
            "excluded_elements_with_nonzero_gradient": fresh_near_zero,
            "opposite_sign_elements": fresh_sign_flips,
            "opposite_sign_elements_above_floor": opposite_kept,
        },
        "literal_fresh_stored_change_relative_l2": maxima["adam_change_relative_l2"],
        "zero_gradient_parameter_sets": ZERO_GRADIENT_SETS,
        "design_bars": {"bars": BARS, "maxima": maxima, "passed": literal_passed,
                        "adam": "stored float32 parameters after one Adam step from a fresh optimizer, every element "
                                "(the literal reading, reported beside the A2 parts)"},
        "arithmetic_checks": {"bars": RELATIVE_BARS, "maxima": arithmetic, "passed": arithmetic_passed,
                              "adam": "update before storage, one step from the warmed optimizer state"},
        "maxima": maxima,
        "worst_loss_term": worst["loss"],
        "worst_gradient_tensor": max(worst["gradient"], key=worst["gradient"].get),
        "worst_adam_tensor": max(worst["fresh_stored"], key=worst["fresh_stored"].get),
        "per_tensor_gradient_relative_l2": worst["gradient"],
        "per_tensor_adam_change_relative_l2": worst["fresh_stored"],
        "per_tensor_warmed_update_relative_l2": worst["adam"],
        "adam_state": f"warmed: S's moments after {batches['warm_adam_step']} native updates, step {warm_t}",
        "fresh_step": fresh,
        "stored_parameter_change": {
            "description": "warmed step, change of the stored float32 parameters; floor = the reference step rounded "
                           "to float32",
            "max_relative_l2": max(worst["stored"].values()),
            "max_float32_floor": max(worst["floor"].values()),
            "per_tensor_relative_l2": worst["stored"],
            "per_tensor_float32_floor": worst["floor"],
        },
        "batches": len(rows),
        "decisions": sum(r["decisions"] for r in rows),
        "seed": batches["seed"],
        "files_sha256": {p.name: sha256(p) for p in sorted(root.iterdir()) if p.is_file()},
        "reference": {"torch": torch.__version__, "numpy": np.__version__, "python": platform.python_version(),
                      "dtype": "float64", "script_sha256": sha256(Path(__file__))},
        "per_batch": rows,
    }
    Path(args.receipt).write_text(json.dumps(receipt, indent=1, sort_keys=True))
    summary = {k: receipt[k] for k in ("passed", "bars", "a2_maxima", "noise_floor",
                                       "literal_fresh_stored_change_relative_l2", "worst_gradient_tensor", "batches",
                                       "decisions")}
    summary["arithmetic_checks"] = receipt["arithmetic_checks"]
    summary["fresh_step"] = {k: v for k, v in fresh.items() if not k.startswith("per_tensor")}
    summary["stored_parameter_change"] = {k: v for k, v in receipt["stored_parameter_change"].items()
                                          if not k.startswith("per_tensor")}
    print(json.dumps(summary, indent=1))
    if not passed:
        print("E3 parity bars (W5 amendment A2): NOT MET", file=sys.stderr)
    return 0 if passed else 1


def read_logits(path: Path):
    buf = path.read_bytes()
    if buf[:8] != b"SLOGIT01":
        raise SystemExit(f"{path}: not a logits dump")
    (count,) = struct.unpack_from("<Q", buf, 8)
    pos, out = 16, []
    for _ in range(count):
        (a,) = struct.unpack_from("<I", buf, pos)
        pos += 4
        logits = np.frombuffer(buf, "<f4", a, pos).astype(np.float64)
        pos += 4 * a
        (value,) = struct.unpack_from("<f", buf, pos)
        pos += 4
        out.append((logits, value))
    if pos != len(buf):
        raise SystemExit(f"{path}: trailing bytes")
    return out


def top_two_gap(logits: np.ndarray) -> float:
    if logits.size < 2:
        return math.inf
    s = np.sort(logits)
    return float(s[-1] - s[-2])


S_CONFIG = {"d_model": 96, "layers": 3, "heads": 4, "ffn": 384, "card_dim": 32, "card_vocab": 257}


def torch_logits(args) -> int:
    """Scores tensor-dump states with a PyTorch S state dict in float64 and writes a logits dump (float32 values), for
    E4's export parity against `s_transformer_v1 logits-dump model=s-checkpoint:<path> states=td:<path>`."""
    state = torch.load(args.state_dict, map_location="cpu", weights_only=True)
    P = {k: v.detach().to(F64) for k, v in state.items()}
    cfg = dict(S_CONFIG, **(json.loads(args.config) if args.config else {}))
    records = decode_records(Path(args.states).read_bytes())
    out = bytearray(b"SLOGIT01")
    out += struct.pack("<Q", len(records))
    with torch.no_grad():
        for start in range(0, len(records), 256):
            chunk = records[start:start + 256]
            logits, z = forward(P, cfg, collate(chunk))
            value = torch.tanh(0.5 * z)
            for k, lg in enumerate(logits):
                out += struct.pack("<I", lg.numel())
                out += lg.to(torch.float32).numpy().astype("<f4").tobytes()
                out += struct.pack("<f", float(value[k]))
    Path(args.out).write_bytes(bytes(out))
    print(json.dumps({"states": len(records), "sha256": sha256(Path(args.out))}))
    return 0


def compare_logits(args) -> int:
    a, b = read_logits(Path(args.a)), read_logits(Path(args.b))
    if len(a) != len(b):
        raise SystemExit("different state counts")
    max_logit = max_value = 0.0
    mismatches = ties = 0
    for (la, va), (lb, vb) in zip(a, b):
        if la.size != lb.size:
            raise SystemExit("different action counts")
        max_logit = max(max_logit, float(np.max(np.abs(la - lb))))
        max_value = max(max_value, abs(va - vb))
        if int(np.argmax(la)) != int(np.argmax(lb)):
            if min(top_two_gap(la), top_two_gap(lb)) <= args.bar:
                ties += 1
            else:
                mismatches += 1
    bar = args.bar
    identical = Path(args.a).read_bytes() == Path(args.b).read_bytes()
    passed = max_logit <= bar and max_value <= bar and mismatches == 0 and (identical or not args.require_identical)
    receipt = {
        "schema": "s-logit-parity-receipt-v1",
        "passed": passed,
        "bar": bar,
        "states": len(a),
        "max_abs_logit_difference": max_logit,
        "max_abs_value_difference": max_value,
        "argmax_mismatches": mismatches,
        "argmax_differences_within_tie_bar": ties,
        "bit_identical": identical,
        "identity_required": args.require_identical,
        "a": {"path": args.a, "sha256": sha256(Path(args.a))},
        "b": {"path": args.b, "sha256": sha256(Path(args.b))},
    }
    Path(args.receipt).write_text(json.dumps(receipt, indent=1, sort_keys=True))
    print(json.dumps(receipt, indent=1, sort_keys=True))
    return 0 if passed else 1


def export_values(args) -> int:
    """Writes a PyTorch S state dict as `values.bin` (f32 little-endian, parameter order) and `names.json`
    ([name, shape] pairs), for `s_transformer_v1 import-values`."""
    state = torch.load(args.state_dict, map_location="cpu", weights_only=True)
    out = Path(args.out_dir)
    out.mkdir(parents=True, exist_ok=True)
    names, chunks = [], []
    for name, tensor in state.items():
        names.append([name, list(tensor.shape)])
        chunks.append(tensor.detach().to(torch.float32).contiguous().numpy().astype("<f4").tobytes())
    (out / "values.bin").write_bytes(b"".join(chunks))
    (out / "names.json").write_text(json.dumps(names))
    print(json.dumps({"tensors": len(names), "values_sha256": sha256(out / "values.bin")}))
    return 0


def compare_hashes(args) -> int:
    """Compares two `resume-check` receipts (for example Linux and Windows): every after-state hash, the fresh and
    starting state hashes and the checkpoint's hash must be equal."""
    a, b = json.loads(Path(args.a).read_text()), json.loads(Path(args.b).read_text())
    keys = ("after_state_sha256", "fresh_state_sha256", "starting_state_sha256", "checkpoint_sha256", "seed", "steps",
            "split", "groups_per_batch")
    differing = [k for k in keys if a.get(k) != b.get(k)]
    receipt = {"schema": "s-hash-parity-receipt-v1", "passed": not differing, "differing": differing,
               "after_state_sha256": a.get("after_state_sha256"), "a": args.a, "b": args.b}
    Path(args.receipt).write_text(json.dumps(receipt, indent=1, sort_keys=True))
    print(json.dumps(receipt, indent=1, sort_keys=True))
    return 0 if not differing else 1


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("parity")
    p.add_argument("--dir", required=True)
    p.add_argument("--receipt", required=True)
    c = sub.add_parser("compare-logits")
    c.add_argument("a")
    c.add_argument("b")
    c.add_argument("--receipt", required=True)
    c.add_argument("--bar", type=float, default=LOGIT_BAR)
    c.add_argument("--require-identical", action="store_true")
    h = sub.add_parser("compare-hashes")
    h.add_argument("a")
    h.add_argument("b")
    h.add_argument("--receipt", required=True)
    t = sub.add_parser("torch-logits")
    t.add_argument("--state-dict", required=True)
    t.add_argument("--states", required=True)
    t.add_argument("--out", required=True)
    t.add_argument("--config", default="")
    x = sub.add_parser("export-values")
    x.add_argument("--state-dict", required=True)
    x.add_argument("--out-dir", required=True)
    args = parser.parse_args(argv)
    commands = {"parity": parity, "compare-logits": compare_logits, "export-values": export_values,
                "torch-logits": torch_logits, "compare-hashes": compare_hashes}
    return commands[args.command](args)


if __name__ == "__main__":
    sys.exit(main())
