"""W5 E4: the S0 distillation trainer (collab LANES/lead/W5-DESIGN.md v1.3 section 6.2), in PyTorch.

  s0_train.py --config CFG.json --out RUN_DIR [--device cuda:0|cpu] [--workers 3]

Reads the frozen config (`s0.py config`) and its store, trains S from the pilot's initializer on b512's teacher
logits and the game outcomes, and writes RUN_DIR/best.pt (the state dict at the lowest validation value, fp32),
log.jsonl and train.json (hashes of the config, store, code, initial and best parameters). Every recipe constant
and its source is in s0_recipe.py; `s0.py export` turns best.pt into the native S and its parity receipt.

Exit codes: 0 finished (epoch cap or early stop), 2 diverged (a non-finite loss).
"""
from __future__ import annotations

import argparse
import contextlib
import hashlib
import json
import math
import os
import sys
import time
from pathlib import Path

os.environ.setdefault("CUBLAS_WORKSPACE_CONFIG", ":4096:8")

import numpy as np
import torch
import torch.nn.functional as F
from torch import nn

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import s0  # noqa: E402
import s0_recipe as R  # noqa: E402

OBJ, EDGE, ACT, REF, STATE, GROUPS = 98, 41, 195, 25, 219, 20
STATE_KEEP, ACT_KEEP = STATE - 96, ACT   # S drops the state digest and keeps the action digest (models.py 20-24)


# ---------------------------------------------------------------------------------------------------------------
# The pilot's ObjectTransformer (collab LANES/bignet-pilot-20261031/code/models.py lines 107 to 207), unchanged in
# structure, parameter names and construction order so `torch.manual_seed(seed)` gives the pilot's initializer.
# Only the heads' autocast guard runs on CUDA alone (it changes nothing on the CPU).


def pad_logits(flat, b, act_batch, act_pos, amax):
    out = flat.new_full((b, amax), float("-inf"))
    out[act_batch, act_pos] = flat
    return out


class Block(nn.Module):
    def __init__(self, d, heads, ffn, dropout):
        super().__init__()
        self.heads = heads
        self.ln1 = nn.LayerNorm(d)
        self.qkv = nn.Linear(d, 3 * d)
        self.proj = nn.Linear(d, d)
        self.ln2 = nn.LayerNorm(d)
        self.ff = nn.Sequential(nn.Linear(d, ffn), nn.GELU(), nn.Linear(ffn, d))
        self.drop = nn.Dropout(dropout)
        self.p = dropout

    def forward(self, x, bias):
        b, l, d = x.shape
        q, k, v = self.qkv(self.ln1(x)).view(b, l, 3, self.heads, d // self.heads).permute(2, 0, 3, 1, 4)
        a = F.scaled_dot_product_attention(q, k, v, attn_mask=bias.to(q.dtype),
                                           dropout_p=self.p if self.training else 0.0)
        x = x + self.drop(self.proj(a.transpose(1, 2).reshape(b, l, d))).float()
        return x + self.drop(self.ff(self.ln2(x))).float()


class ObjectTransformer(nn.Module):
    """Tokens: [state, objects (padded to omax), actions (padded to amax)]."""

    def __init__(self, d=512, layers=8, heads=8, ffn=2048, card_dim=256, vocab=257, dropout=0.1):
        super().__init__()
        self.d, self.heads, self.vocab = d, heads, vocab
        self.card = nn.Embedding(vocab, card_dim, padding_idx=0)
        self.obj_in = nn.Sequential(nn.Linear(OBJ + card_dim, d), nn.GELU(), nn.Linear(d, d))
        self.group = nn.Embedding(GROUPS, d)
        self.state_in = nn.Sequential(nn.Linear(STATE_KEEP, d), nn.GELU(), nn.Linear(d, d))
        self.act_in = nn.Sequential(nn.Linear(ACT_KEEP, d), nn.GELU(), nn.Linear(d, d))
        self.ref_in = nn.Sequential(nn.Linear(REF + card_dim, d), nn.GELU(), nn.Linear(d, d))
        self.edge_bias = nn.Linear(EDGE, 2 * heads)
        self.ref_bias = nn.Parameter(torch.zeros(2, heads))
        self.blocks = nn.ModuleList(Block(d, heads, ffn, dropout) for _ in range(layers))
        self.ln = nn.LayerNorm(d)
        self.policy = nn.Sequential(nn.Linear(2 * d, d), nn.GELU(), nn.Linear(d, 1))
        self.value = nn.Sequential(nn.Linear(d, d), nn.GELU(), nn.Linear(d, 1))
        self.apply(self._init)

    @staticmethod
    def _init(m):
        if isinstance(m, nn.Linear):
            nn.init.normal_(m.weight, std=0.02)
            nn.init.zeros_(m.bias)
        elif isinstance(m, nn.Embedding):
            nn.init.normal_(m.weight, std=0.02)

    def forward(self, bt):
        b, omax, amax = bt["state"].shape[0], bt["omax"], bt["amax"]
        l = 1 + omax + amax
        heads = self.heads
        card = bt["obj_card"]
        obj = self.obj_in(torch.cat([bt["obj_feat"], self.card(card)], -1)) + self.group(bt["obj_group"])
        act = self.act_in(bt["act_feat"][:, :ACT_KEEP])
        if bt["ref_feat"].shape[0] > 0:
            rh = self.ref_in(torch.cat([bt["ref_feat"], self.card(bt["ref_card"])], -1))
            act = act.index_add(0, bt["ref_action"], rh.to(act.dtype))
        x = torch.zeros(b, l, self.d, dtype=torch.float32, device=obj.device)
        x[:, 0] = self.state_in(bt["state"][:, :STATE_KEEP]).float()
        x[bt["obj_batch"], 1 + bt["obj_pos"]] = obj.float()
        x[bt["act_batch"], 1 + omax + bt["act_pos"]] = act.float()
        bias = x.new_zeros(b, heads, l, l, dtype=torch.float32)
        hh = torch.arange(heads, device=x.device)
        if bt["edge_feat"].shape[0] > 0:
            eb = self.edge_bias(bt["edge_feat"]).float().view(-1, 2, heads)
            eg = bt["obj_batch"][bt["edge_src"]]
            s, t = 1 + bt["obj_pos"][bt["edge_src"]], 1 + bt["obj_pos"][bt["edge_tgt"]]
            bias.index_put_((eg[:, None], hh[None], s[:, None], t[:, None]), eb[:, 0], accumulate=True)
            bias.index_put_((eg[:, None], hh[None], t[:, None], s[:, None]), eb[:, 1], accumulate=True)
        if bt["ref_feat"].shape[0] > 0:
            rg = bt["act_batch"][bt["ref_action"]]
            ai = 1 + omax + bt["act_pos"][bt["ref_action"]]
            oi = 1 + bt["obj_pos"][bt["ref_node"]]
            n = rg.shape[0]
            bias.index_put_((rg[:, None], hh[None], ai[:, None], oi[:, None]),
                            self.ref_bias[0].float().expand(n, heads), accumulate=True)
            bias.index_put_((rg[:, None], hh[None], oi[:, None], ai[:, None]),
                            self.ref_bias[1].float().expand(n, heads), accumulate=True)
        bias = bias.masked_fill(~bt["key_mask"][:, None, None, :], float("-inf"))
        for blk in self.blocks:
            x = blk(x, bias)
        x = self.ln(x)
        state_h = x[:, 0]
        act_out = x[bt["act_batch"], 1 + omax + bt["act_pos"]]
        p_in = torch.cat([state_h[bt["act_batch"]], act_out], -1)
        guard = torch.autocast("cuda", enabled=False) if x.is_cuda else contextlib.nullcontext()
        with guard:  # heads in fp32
            flat = self.policy[2](self.policy[1](self.policy[0](p_in.float()))).squeeze(-1)
            value = self.value[2](self.value[1](self.value[0](state_h.float()))).squeeze(-1)
        logits = pad_logits(flat, b, bt["act_batch"], bt["act_pos"], amax)
        return logits, value.float()


def build_model(seed: int = R.SEED) -> ObjectTransformer:
    """The pilot's `build_model("S", seed)` (code/train.py lines 248 to 255)."""
    torch.manual_seed(seed)
    a = R.ARCH
    return ObjectTransformer(d=a["d"], layers=a["layers"], heads=a["heads"], ffn=a["ffn"], card_dim=a["card_dim"],
                             vocab=a["vocab"], dropout=a["dropout"])


def zero_gauge_gradients(model: ObjectTransformer) -> None:
    """Native S's fixed gauge parameters: `policy.2.bias` and the key third of every `qkv.bias`."""
    d = model.d
    if model.policy[2].bias.grad is not None:
        model.policy[2].bias.grad.zero_()
    for blk in model.blocks:
        if blk.qkv.bias.grad is not None:
            blk.qkv.bias.grad[d:2 * d].zero_()


def params_sha(model) -> str:
    h = hashlib.sha256()
    for name, p in model.state_dict().items():
        h.update(name.encode())
        h.update(p.detach().cpu().contiguous().numpy().tobytes())
    return h.hexdigest()


# ---------------------------------------------------------------------------------------------------------------
# Data (the pilot's collate, batches and micro-batches; checks-code's positioned reads)


def collate(recs: list[dict], outcomes, vmask) -> dict:
    """code/collate.py, plus the value subsample mask."""
    b = len(recs)
    no = [len(r["object_card_ids"]) for r in recs]
    na = [len(r["logits"]) for r in recs]
    obase = np.concatenate([[0], np.cumsum(no)[:-1]]).astype(np.int64)
    abase = np.concatenate([[0], np.cumsum(na)[:-1]]).astype(np.int64)
    cat = lambda key, dt: np.concatenate([r[key] for r in recs]).astype(dt, copy=False)  # noqa: E731
    ne = [len(r["edge_source"]) for r in recs]
    nr = [len(r["action_ref_node_indices"]) for r in recs]
    omax, amax = max(no), max(na)
    key_mask = np.zeros((b, 1 + omax + amax), bool)
    key_mask[:, 0] = True
    for i in range(b):
        key_mask[i, 1:1 + no[i]] = True
        key_mask[i, 1 + omax:1 + omax + na[i]] = True
    top = max([int(r["object_card_ids"].max(initial=0)) for r in recs]
              + [int(r["action_ref_card_ids"].max(initial=0)) for r in recs])
    if top > R.MAX_CARD_TOKEN:
        raise s0.S0Error(f"card token {top} above {R.MAX_CARD_TOKEN}: outside the 192-card registry")
    out = {
        "state": np.stack([r["state"] for r in recs]).astype(np.float32),
        "obj_feat": cat("object_features", np.float32).reshape(-1, OBJ),
        "obj_card": cat("object_card_ids", np.int64),
        "obj_group": cat("object_groups", np.int64),
        "obj_batch": np.repeat(np.arange(b), no),
        "obj_pos": np.concatenate([np.arange(n) for n in no]),
        "edge_feat": cat("edge_features", np.float32).reshape(-1, EDGE) if sum(ne) else np.zeros((0, EDGE), np.float32),
        "edge_src": cat("edge_source", np.int64) + np.repeat(obase, ne),
        "edge_tgt": cat("edge_target", np.int64) + np.repeat(obase, ne),
        "act_feat": cat("action_features", np.float32).reshape(-1, ACT),
        "act_batch": np.repeat(np.arange(b), na),
        "act_pos": np.concatenate([np.arange(n) for n in na]),
        "ref_feat": cat("action_ref_features", np.float32).reshape(-1, REF) if sum(nr) else np.zeros((0, REF), np.float32),
        "ref_card": cat("action_ref_card_ids", np.int64),
        "ref_action": cat("action_ref_action_indices", np.int64) + np.repeat(abase, nr),
        "ref_node": cat("action_ref_node_indices", np.int64) + np.repeat(obase, nr),
        "key_mask": key_mask,
        "selected": np.asarray([r["selected"] for r in recs], np.int64),
        "outcome": np.asarray(outcomes, np.float32),
        "teacher_logits": cat("logits", np.float32),
        "vmask": np.asarray(vmask, np.float32),
    }
    t = {k: torch.from_numpy(np.ascontiguousarray(v)) for k, v in out.items()}
    t["omax"], t["amax"] = omax, amax
    return t


def chunks(recs):
    """code/train.py lines 293 to 306."""
    out, start, mo, ma = [], 0, 0, 0
    for i, r in enumerate(recs):
        no, na = len(r["object_card_ids"]), len(r["logits"])
        o, a = max(mo, no), max(ma, na)
        if i > start and (i - start + 1) * (1 + o + a) ** 2 > R.TOKEN_BUDGET:
            out.append((start, i))
            start, o, a = i, no, na
        mo, ma = o, a
    out.append((start, len(recs)))
    return out


class Store:
    """Reads each record with one positioned read (checks-code/train_checks.py lines 60 to 91)."""

    def __init__(self, manifest_path):
        # The trainer verified the index hash once; data workers read it as is.
        self.ix, _, self.root = s0.load_index(manifest_path, verify=False)
        self._fd = {}

    def close(self):
        for fd in self._fd.values():
            os.close(fd)
        self._fd = {}

    def __del__(self):
        self.close()

    def fd(self, f):
        if f not in self._fd:
            path = self.root / str(self.ix["files"][f])
            self._fd[f] = os.open(path, os.O_RDONLY | getattr(os, "O_BINARY", 0) | getattr(os, "O_RANDOM", 0))
        return self._fd[f]

    def records(self, rows):
        ix, recs = self.ix, []
        for r in rows:
            fd = self.fd(int(ix["fid"][r]))
            n = int(ix["length"][r])
            os.lseek(fd, int(ix["offset"][r]), os.SEEK_SET)
            buf = os.read(fd, n)
            rec, end = s0.decode_record(buf, 0)
            if end != n or len(buf) != n:
                raise s0.S0Error(f"record {r}: length {end} != {n}")
            recs.append(rec)
        return recs

    def batch(self, rows, vmask=None):
        rows = np.asarray(rows)
        recs = self.records(rows)
        vm = np.ones(len(rows), np.float32) if vmask is None else vmask[rows].astype(np.float32)
        return [collate(recs[i:j], self.ix["outcome"][rows[i:j]], vm[i:j]) for i, j in chunks(recs)]


class Batches(torch.utils.data.Dataset):
    def __init__(self, manifest_path, batches, vmask=None):
        self.manifest_path, self.batches, self.vmask, self.store = manifest_path, batches, vmask, None

    def __len__(self):
        return len(self.batches)

    def __getitem__(self, i):
        if self.store is None:
            self.store = Store(self.manifest_path)
        return self.store.batch(self.batches[i], self.vmask)


def loader(manifest_path, batches, workers, vmask=None, pin=False):
    # A private generator: building the iterator never draws from torch's default generator.
    return torch.utils.data.DataLoader(Batches(manifest_path, batches, vmask), batch_size=None, shuffle=False,
                                       num_workers=workers, pin_memory=pin, persistent_workers=False,
                                       prefetch_factor=4 if workers else None,
                                       generator=torch.Generator().manual_seed(0))


def epoch_batches(rows, length, steps, rng, pool=R.POOL):
    """code/train.py lines 344 to 353."""
    perm = rows[rng.permutation(len(rows))][: steps * R.BATCH]
    out = []
    for i in range(0, len(perm), pool * R.BATCH):
        chunk = perm[i:i + pool * R.BATCH]
        chunk = chunk[np.argsort(length[chunk], kind="stable")]
        out += [chunk[j:j + R.BATCH] for j in range(0, len(chunk), R.BATCH)]
    return [out[k] for k in rng.permutation(len(out))]


def eval_batches(rows, length):
    """code/train.py lines 356 to 359."""
    order = rows[np.lexsort((rows, length[rows]))]
    return [order[i:i + R.BATCH] for i in range(0, len(order), R.BATCH)]


def vsub_mask(n_rows, rows, game_key, rng):
    """checks-code/train_checks.py lines 145 to 155: VSUB_K seeded decisions per training game."""
    u = rng.random(len(rows))
    g = game_key[rows]
    order = np.lexsort((u, g))
    gs = g[order]
    start = np.r_[0, np.nonzero(np.diff(gs))[0] + 1]
    rank = np.arange(len(gs)) - np.repeat(start, np.diff(np.r_[start, len(gs)]))
    m = np.zeros(n_rows, bool)
    m[rows[order[rank < R.VSUB_K]]] = True
    return m


def game_means(game_key, rows, x):
    """code/train.py lines 396 to 400: each game's decisions average to one number."""
    _, inv = np.unique(game_key[rows], return_inverse=True)
    return np.bincount(inv, weights=x) / np.bincount(inv)


# ---------------------------------------------------------------------------------------------------------------
# Losses and training


def to_dev(bt, dev):
    return {k: (v.to(dev, non_blocking=True) if torch.is_tensor(v) else v) for k, v in bt.items()}


def losses(model, bt, amp: bool):
    """Per decision: value log loss on the outcome, KL(teacher || S), and the sampled action's cross-entropy."""
    with torch.autocast("cuda", dtype=torch.bfloat16, enabled=amp):
        logits, z = model(bt)
    logp = torch.log_softmax(logits.float(), -1)
    b = logits.shape[0]
    tl = pad_logits(bt["teacher_logits"].float(), b, bt["act_batch"], bt["act_pos"], bt["amax"])
    logpt = torch.log_softmax(tl, -1)
    live = torch.isfinite(logpt)
    pt = torch.exp(logpt)
    kl = (pt * (logpt.masked_fill(~live, 0.0) - logp.masked_fill(~live, 0.0))).sum(-1)
    ce = -logp.gather(1, bt["selected"][:, None]).squeeze(1)
    val = F.binary_cross_entropy_with_logits(z.float(), bt["outcome"], reduction="none")
    return val, kl, ce


def evaluate(model, manifest_path, rows, ix, dev, amp, workers):
    model.eval()
    batches = eval_batches(rows, ix["length"])
    out = np.empty((3, len(rows)))
    pos = {int(r): i for i, r in enumerate(rows)}
    with torch.no_grad():
        for b, parts in zip(batches, loader(manifest_path, batches, workers, None, dev.type == "cuda")):
            acc = [[], [], []]
            for bt in parts:
                bt = to_dev(bt, dev)
                for k, x in enumerate(losses(model, bt, amp)):
                    acc[k].append(x.double().cpu().numpy())
            idx = [pos[int(r)] for r in b]
            for k in range(3):
                out[k, idx] = np.concatenate(acc[k])
    model.train()
    return out


def setup_determinism():
    """code/train.py lines 411 to 418 (the EcoQoS opt-out is a scheduling hint and is left to the launcher)."""
    torch.use_deterministic_algorithms(True)
    torch.backends.cudnn.benchmark = False
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False


def lr_factor(step: int, warm: int, total: int) -> float:
    """code/train.py lines 457 to 460 (LambdaLR's factor at scheduler step `step`)."""
    if step < warm:
        return (step + 1) / warm
    return R.LR_FLOOR + (1 - R.LR_FLOOR) * 0.5 * (1 + math.cos(math.pi * min(1.0, (step - warm) / max(1, total - warm))))


def code_hashes() -> dict:
    return {name: s0.sha256_file(HERE / name) for name in ("s0_train.py", "s0_recipe.py", "s0.py")}


def train(config_path, out, device="cpu", workers=0) -> dict:
    config = s0.read_json(config_path)
    s0.require(config.get("schema") == s0.CONFIG_SCHEMA, "not an S0 config")
    rec = config["recipe"]
    s0.require(rec == {**R.recipe(rec["grad_clip_norm"]), **({"smoke_overrides": rec["smoke_overrides"]}
                                                             if "smoke_overrides" in rec else {})},
               "the config's recipe differs from this code's frozen recipe")
    s0.require(config["smoke"] or "smoke_overrides" not in rec, "overrides are smoke only")
    manifest_path = s0.checked(config["store"]["manifest"], "store manifest")
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    s0.require(not (out / "train.json").exists(), f"{out} already holds a finished run")
    setup_determinism()
    dev = torch.device(device)
    model = build_model(rec["seed"]).to(dev)
    init_sha = params_sha(model)
    amp = dev.type == "cuda"
    ix, _, _ = s0.load_index(manifest_path)
    train_rows = np.nonzero(~ix["validation"])[0]
    val_rows = np.nonzero(ix["validation"])[0]
    s0.require(len(train_rows) >= R.BATCH and len(val_rows) > 0, "too few decisions to train")
    steps_per_epoch = len(train_rows) // R.BATCH
    overrides = rec.get("smoke_overrides") or {}
    total = R.EPOCHS_MAX * steps_per_epoch
    if overrides.get("max_steps"):
        total = min(total, int(overrides["max_steps"]))
    every = overrides.get("eval_every") or max(steps_per_epoch // 4, R.MIN_EVAL_STEPS)
    opt = torch.optim.Adam(model.parameters(), lr=R.LR, betas=R.BETAS, eps=R.EPS, weight_decay=R.WEIGHT_DECAY)
    warm = max(1, int(R.WARMUP_FRACTION * total))
    clip = rec["grad_clip_norm"]
    seed = rec["seed"]
    rng = np.random.default_rng(seed)
    vrng = np.random.default_rng(seed + R.VSUB_SEED_OFFSET)
    meta = {"config_sha256": s0.sha256_file(config_path), "store_manifest_sha256": s0.sha256_file(manifest_path),
            "train_decisions": int(len(train_rows)), "train_games": int(len(np.unique(ix["game"][train_rows]))),
            "val_decisions": int(len(val_rows)), "val_games": int(len(np.unique(ix["game"][val_rows]))),
            "steps_per_epoch": steps_per_epoch, "total_steps": total, "warmup_steps": warm, "eval_every": every,
            "init_params_sha256": init_sha, "params": sum(p.numel() for p in model.parameters()),
            "device": str(dev), "gpu": torch.cuda.get_device_name(dev) if dev.type == "cuda" else None,
            "torch": torch.__version__, "numpy": np.__version__, "code_sha256": code_hashes(), "workers": workers}
    log = open(out / "log.jsonl", "w", encoding="utf-8")
    log.write(json.dumps({"kind": "meta", **meta}) + "\n")
    best, best_step, since, step, t0 = math.inf, -1, 0, 0, time.time()
    history, status, decisions = [], "complete", 0

    def finish(code):
        end = {"kind": "end", "status": status, "steps": step, "epochs": step / steps_per_epoch,
               "best_step": best_step, "best_val_value": best, "evaluations": history,
               "sec": time.time() - t0, "train_decisions_per_second": decisions / max(1e-9, time.time() - t0),
               "final_params_sha256": params_sha(model)}
        if (out / "best.pt").exists():
            end["best_pt_sha256"] = s0.sha256_file(out / "best.pt")
        log.write(json.dumps(end) + "\n")
        log.close()
        s0.write_json(out / "train.json", {**meta, **end})
        return code

    done = False
    while not done:
        batches = epoch_batches(train_rows, ix["length"], steps_per_epoch, rng)
        vmask = vsub_mask(len(ix["game"]), train_rows, ix["game"], vrng)
        for parts in loader(manifest_path, batches, workers, vmask, dev.type == "cuda"):
            n = sum(int(bt["state"].shape[0]) for bt in parts)
            nv = sum(float(bt["vmask"].sum()) for bt in parts)
            opt.zero_grad(set_to_none=True)
            vs, ks = 0.0, 0.0
            for bt in parts:
                bt = to_dev(bt, dev)
                val, kl, _ = losses(model, bt, amp)
                vterm = (val * bt["vmask"]).sum() / max(nv, 1.0)
                (vterm + kl.sum() / n).backward()
                vs += float((val * bt["vmask"]).detach().sum())
                ks += float(kl.detach().sum())
            if not math.isfinite(vs + ks):
                status = "diverged"
                return finish(2)
            zero_gauge_gradients(model)
            if clip is not None:
                torch.nn.utils.clip_grad_norm_(model.parameters(), clip)
            for g in opt.param_groups:
                g["lr"] = R.LR * lr_factor(step, warm, total)
            opt.step()
            step += 1
            decisions += n
            if step % 50 == 0:
                log.write(json.dumps({"kind": "train", "step": step, "value": vs / max(nv, 1.0), "kl": ks / n,
                                      "lr": opt.param_groups[0]["lr"], "sec": time.time() - t0}) + "\n")
                log.flush()
            if step % every == 0:
                v, k, c = evaluate(model, manifest_path, val_rows, ix, dev, amp, workers)
                vm, km, cm = (float(game_means(ix["game"], val_rows, x).mean()) for x in (v, k, c))
                rec_eval = {"kind": "val", "step": step, "epoch": step / steps_per_epoch, "value": vm, "kl": km,
                            "policy_ce": cm, "sec": time.time() - t0}
                history.append(rec_eval)
                log.write(json.dumps(rec_eval) + "\n")
                log.flush()
                if not math.isfinite(vm + km):
                    status = "diverged"
                    return finish(2)
                if vm < best:
                    best, best_step, since = vm, step, 0
                    torch.save({k: t.detach().cpu() for k, t in model.state_dict().items()}, out / "best.pt")
                else:
                    since += 1
                    if since >= R.PATIENCE:
                        status = "stopped"
                        done = True
                        break
            if step >= total:
                done = True
                break
    return finish(0)


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--config", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--device", default="cpu")
    ap.add_argument("--workers", type=int, default=0)
    a = ap.parse_args(argv)
    try:
        return train(a.config, a.out, a.device, a.workers)
    except s0.S0Error as exc:
        print(f"refused: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
