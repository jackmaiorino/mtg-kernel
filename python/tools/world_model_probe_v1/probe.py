"""World-model accuracy probe (collab LANES/world-model-probe-20261020).

Can a small learned model predict a Magic game several decisions ahead from
the focal player's view? Measurement only: no policy, no reward.

    probe.py build --parts DIR --manifest M --out DIR [--full-info]
    probe.py train --data DIR --out DIR --hidden H --unroll U [--epochs E]
    probe.py eval  --data DIR --model DIR --split S --out FILE

A transition is one decision to the next decision (either player), given the
chosen action; the last decision's successor is the final (terminal) state.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import time
from pathlib import Path

import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F

C = 192  # card pool (data/cards_v1.json)
PHASES = ["Untap", "Upkeep", "Draw", "Main1", "BeginCombat", "DeclareAttackers", "DeclareBlockers",
          "CombatDamage", "EndCombat", "Main2", "End", "Cleanup"]
SEAT_FIELDS = ["life", "hand", "library", "graveyard", "creatures", "lands", "untapped_lands", "power", "toughness"]
SEAT_SCALE = [10, 3, 20, 5, 3, 3, 3, 5, 5]
BLOCKS = ["me_hand", "me_bf_untapped", "me_bf_tapped", "me_gy", "opp_bf_untapped", "opp_bf_tapped", "opp_gy",
          "exile_me", "exile_opp", "stack_me", "stack_opp"]
FULL_BLOCK = "opp_hand"  # hidden from the focal player; only in the disclosed full-information arm
# Opponent actions whose named cards are public when taken; for any other
# opponent action the model sees only its kind.
PUBLIC_OPP_KINDS = {"cast_spell", "play_land", "activate_ability", "activate_mana_ability",
                    "choose_attacker_inclusion", "choose_blocker_inclusion", "choose_target",
                    "choose_cost_target", "discard"}
# Action input: role-specific card blocks plus hashed choice tokens (include or
# decline, target player, mode, indices), so distinct choices stay distinct.
ROLES = ["source", "target", "attacker", "blocker", "other"]
N_TOKENS = 512
A_DIM = len(ROLES) * C + N_TOKENS
HORIZONS = [1, 2, 3, 5, 10]
DESCRIPTIVE_HORIZONS = [20, 40]


class Layout:
    """Raw state vector: regression dims first, then active, then phase one-hot."""

    def __init__(self, full_info: bool):
        self.full_info = full_info
        self.blocks = BLOCKS + ([FULL_BLOCK] if full_info else [])
        names, scale = [], []
        for side in ("me", "opp"):
            for f, s in zip(SEAT_FIELDS, SEAT_SCALE):
                names.append(f"{side}_{f}"); scale.append(s)
        names.append("turn"); scale.append(10)
        for b in self.blocks:
            names += [f"{b}:{c}" for c in range(C)]; scale += [1] * C
        self.reg_names = names
        self.n_reg = len(names)
        self.scale = np.array(scale, dtype=np.float32)
        self.i_active = self.n_reg
        self.i_phase = self.n_reg + 1
        self.dim = self.n_reg + 1 + len(PHASES)
        self.idx = {n: i for i, n in enumerate(names)}
        # Integer fields that cannot go negative (life can).
        self.nonneg = np.array([not n.endswith("_life") for n in names])
        self.decisive = [self.idx[n] for n in ("me_life", "opp_life", "me_creatures", "opp_creatures")]

    def block_start(self, b):
        return self.idx[f"{b}:0"]


def featurize(facts, focal, lay: Layout):
    """Sparse raw state (index -> value) from the focal player's view."""
    v = {}

    def add(i, x):
        if x:
            v[i] = v.get(i, 0) + x

    seats = facts["seats"]
    for side, p in (("me", focal), ("opp", 1 - focal)):
        for f in SEAT_FIELDS:
            add(lay.idx[f"{side}_{f}"], seats[p][f])
        for card, tapped, _pw, _tg in seats[p]["bf"]:
            add(lay.block_start(f"{side}_bf_{'tapped' if tapped else 'untapped'}") + card, 1)
        for card in seats[p]["gy"]:
            add(lay.block_start(f"{side}_gy") + card, 1)
    for card in seats[focal]["hand_cards"]:
        add(lay.block_start("me_hand") + card, 1)
    if lay.full_info:
        for card in seats[1 - focal]["hand_cards"]:
            add(lay.block_start(FULL_BLOCK) + card, 1)
    for card, owner in facts["exile"]:
        add(lay.block_start("exile_me" if owner == focal else "exile_opp") + card, 1)
    for card, controller in facts["stack"]:
        add(lay.block_start("stack_me" if controller == focal else "stack_opp") + card, 1)
    add(lay.idx["turn"], facts["turn"])
    add(lay.i_active, int(facts["active"] == focal))
    add(lay.i_phase + PHASES.index(facts["phase"]), 1)
    return v


def action_features(sem, me_actor, focal):
    """Sparse action input from the full semantic, with the focal player's
    visibility: an opponent's card in hand or library stays anonymous unless
    the action itself reveals it."""
    kind = sem["action_kind"]
    reveal = me_actor or kind in PUBLIC_OPP_KINDS
    idx = set()

    def rel(seat):
        return "me" if seat == f"p{focal}" else "opp"

    def tok(t):
        idx.add(len(ROLES) * C + int(hashlib.md5(t.encode()).hexdigest()[:8], 16) % N_TOKENS)

    def walk(v, path, role):
        if isinstance(v, dict):
            if "card_db_id" in v:
                zone = str(v.get("zone", "")).lower()
                tok(f"{path}:zone={zone}")
                tok(f"{path}:controller={rel(v.get('controller'))}")
                if not reveal and zone in ("hand", "library"):
                    tok(f"{path}:hidden")
                else:
                    idx.add(ROLES.index(role) * C + v["card_db_id"])
                    if "[" in path:  # bind a list element's identity to its position
                        tok(f"{path}:card={v['card_db_id']}")
                return
            for k, x in v.items():
                if k in ("action_kind", "actor") or k.endswith("_id"):
                    continue
                r = role if role is not None else (k.rstrip("s") if k.rstrip("s") in ROLES[:-1] else "other")
                walk(x, f"{path}.{k}" if path else k, r)
        elif isinstance(v, list):
            # Positions are kept (orders and permutations are choices); long tails share one slot.
            tok(f"{path}:len={min(len(v), 8)}")
            for i, x in enumerate(v):
                walk(x, f"{path}[{min(i, 8)}]", role)
        else:
            if isinstance(v, str) and v in ("p0", "p1"):
                v = rel(v)
            elif isinstance(v, int) and not isinstance(v, bool):
                v = max(-50, min(50, v))
            tok(f"{path}={v}")

    walk(sem, "", None)
    return sorted(idx)


def build(args):
    manifest = json.loads(Path(args.manifest).read_text())
    held = [sorted(p) for p in manifest["held_out_pairs"]["pairs"]]

    def split(g):
        if sorted([g % 9, (g // 9) % 9]) in held:
            return "unseen_test"
        if g // 648 < 4:
            return "train"
        return "validation" if (g % 81) % 2 == 0 else "test"

    lay = Layout(args.full_info)
    rows = []
    for part in sorted(Path(args.parts).glob("part-*.jsonl")):
        with part.open() as fh:
            for line in fh:
                rows.append(json.loads(line))
    rows.sort(key=lambda r: r["game"])
    assert [r["game"] for r in rows] == list(range(manifest["games"]["count"]))
    # Action vocabulary from the training split only.
    kinds = sorted({s["action"]["kind"] for r in rows if split(r["game"]) == "train" for s in r["steps"]})
    kind_ix = {k: i for i, k in enumerate(kinds)}
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=False)
    meta = dict(full_info=args.full_info, kinds=kinds, dim=lay.dim, n_reg=lay.n_reg, splits={})
    for name in ("train", "validation", "test", "unseen_test"):
        games = [r for r in rows if split(r["game"]) == name]
        s_ptr, s_idx, s_val, a_ptr, a_idx = [0], [], [], [0], []
        a_kind, a_me, legal, over, win_me, g_off, g_ids = [], [], [], [], [], [0], []
        for r in games:
            focal = r["focal_seat"]
            assert r["finished"] and r["end"]["classification"] == "Natural"
            states = [s["facts"] for s in r["steps"]] + [r["final"]]
            for i, facts in enumerate(states):
                v = featurize(facts, focal, lay)
                for k in sorted(v):
                    s_idx.append(k); s_val.append(v[k])
                s_ptr.append(len(s_idx))
                last = i == len(states) - 1
                legal.append(0 if last else r["steps"][i]["legal"])
                over.append(int(last))
                win_me.append(int(r["end"]["winner"] == focal) if last else 0)
            for s in r["steps"]:
                me = s["actor"] == focal
                a_kind.append(kind_ix.get(s["action"]["kind"], len(kinds)))
                a_me.append(int(me))
                a_idx.extend(action_features(s["action"]["semantic"], me, focal))
                a_ptr.append(len(a_idx))
            g_off.append(len(s_ptr) - 1)
            g_ids.append(r["game"])
        assert (not s_idx or max(s_idx) < lay.dim) and (not a_idx or max(a_idx) < A_DIM)
        np.savez_compressed(out / f"{name}.npz", s_ptr=np.array(s_ptr, np.int64), s_idx=np.array(s_idx, np.int32),
                            s_val=np.array(s_val, np.int16), a_ptr=np.array(a_ptr, np.int64),
                            a_idx=np.array(a_idx, np.int16), a_kind=np.array(a_kind, np.int16),
                            a_me=np.array(a_me, np.int8), legal=np.array(legal, np.int32),
                            over=np.array(over, np.int8), win_me=np.array(win_me, np.int8),
                            g_off=np.array(g_off, np.int64), g_ids=np.array(g_ids, np.int64))
        meta["splits"][name] = dict(games=len(games), states=len(s_ptr) - 1, actions=len(a_kind))
        print(name, meta["splits"][name], flush=True)
    (out / "meta.json").write_text(json.dumps(meta, indent=1))


class Split:
    """One split on the device. States are raw (unscaled) float32."""

    def __init__(self, data_dir, name, device):
        z = np.load(Path(data_dir) / f"{name}.npz")
        meta = json.loads((Path(data_dir) / "meta.json").read_text())
        self.lay = Layout(meta["full_info"])
        self.n_kinds = len(meta["kinds"]) + 1
        n = len(z["s_ptr"]) - 1
        rows = np.repeat(np.arange(n), np.diff(z["s_ptr"]))
        # float16 holds these small integers exactly and halves device memory.
        st = torch.zeros(n, self.lay.dim, dtype=torch.float16)
        st[torch.from_numpy(rows), torch.from_numpy(z["s_idx"].astype(np.int64))] = torch.from_numpy(
            z["s_val"].astype(np.float16))
        self._state = st.to(device)
        m = len(z["a_kind"])
        arows = np.repeat(np.arange(m), np.diff(z["a_ptr"]))
        ac = torch.zeros(m, A_DIM, dtype=torch.float16)
        ac[torch.from_numpy(arows), torch.from_numpy(z["a_idx"].astype(np.int64))] = 1.0
        self.a_cards_h = ac.to(device)
        self.a_kind = torch.from_numpy(z["a_kind"].astype(np.int64)).to(device)
        self.a_me = torch.from_numpy(z["a_me"].astype(np.float32)).to(device)
        self.legal = torch.from_numpy(z["legal"].astype(np.float32)).to(device)
        self.over = torch.from_numpy(z["over"].astype(np.float32)).to(device)
        self.win_me = torch.from_numpy(z["win_me"].astype(np.float32)).to(device)
        self.g_off = z["g_off"]
        self.g_ids = z["g_ids"]
        # State index i of game g pairs with action index i - g (one fewer action per game).
        game_of_state = np.repeat(np.arange(len(self.g_off) - 1), np.diff(self.g_off))
        self.game_of_state = game_of_state
        self.action_of_state = np.arange(n) - game_of_state
        self.last_state = np.repeat(self.g_off[1:] - 1, np.diff(self.g_off))

    def state(self, idx):
        return self._state[idx].float()

    def starts(self, k):
        """State indices t with t + k still inside the same game (t + k <= final)."""
        return np.nonzero(np.arange(len(self.game_of_state)) + k <= self.last_state)[0]


N_SCALAR = len(SEAT_FIELDS) * 2 + 1  # seat fields for both seats, then turn
SCALAR_RANGE = 20  # scalar change classes -20..+20 (clipped)
CARD_RANGE = 2  # card-count change classes -2..+2 (clipped)


class Net(nn.Module):
    """Predicts the most likely change of every field: a class per change value
    (exact-match metrics need the mode, not the mean)."""

    def __init__(self, lay: Layout, n_kinds, hidden):
        super().__init__()
        self.lay = lay
        self.n_card = lay.n_reg - N_SCALAR
        self.register_buffer("scale", torch.from_numpy(lay.scale))
        self.kind = nn.Embedding(n_kinds, 32)
        d_in = lay.dim + 32 + 1 + A_DIM
        self.inp = nn.Linear(d_in, hidden)
        self.blocks = nn.ModuleList([nn.Sequential(nn.LayerNorm(hidden), nn.Linear(hidden, hidden), nn.GELU(),
                                                   nn.Linear(hidden, hidden)) for _ in range(3)])
        self.out_norm = nn.LayerNorm(hidden)
        self.scalar = nn.Linear(hidden, N_SCALAR * (2 * SCALAR_RANGE + 1))
        self.card = nn.Linear(hidden, self.n_card * (2 * CARD_RANGE + 1))
        self.active = nn.Linear(hidden, 1)
        self.phase = nn.Linear(hidden, len(PHASES))
        self.over = nn.Linear(hidden, 1)
        self.win = nn.Linear(hidden, 1)
        self.legal = nn.Linear(hidden, 1)
        # Start at persistence: the no-change class wins until training moves it.
        for head, r in ((self.scalar, SCALAR_RANGE), (self.card, CARD_RANGE)):
            nn.init.zeros_(head.weight)
            bias = torch.zeros(2 * r + 1); bias[r] = 4.0
            head.bias.data.copy_(bias.repeat(head.out_features // (2 * r + 1)))

    def forward(self, state, kind, me, cards):
        lay = self.lay
        x = torch.cat([state[:, :lay.n_reg] / self.scale, state[:, lay.n_reg:], self.kind(kind), me[:, None], cards], 1)
        h = self.inp(x)
        for b in self.blocks:
            h = h + b(h)
        h = self.out_norm(h)
        return dict(scalar=self.scalar(h).view(len(h), N_SCALAR, 2 * SCALAR_RANGE + 1),
                    card=self.card(h).view(len(h), self.n_card, 2 * CARD_RANGE + 1),
                    active=self.active(h)[:, 0], phase=self.phase(h), over=self.over(h)[:, 0],
                    win=self.win(h)[:, 0], legal=self.legal(h)[:, 0])


MAX_STATE_TOKENS = 96  # observed maximum is 67 distinct (zone, card) entries per state
MAX_ACTION_TOKENS = 24  # observed maximum is 20 action features


class SetNet(nn.Module):
    """Arm B: a transformer over the state as a set of (zone, card, count)
    tokens, a scalar token (life, sizes, turn, active, phase) and action tokens
    (kind, actor, role-card and choice features). Same heads as `Net`."""

    def __init__(self, lay: Layout, n_kinds, hidden, layers=4, heads=8):
        super().__init__()
        self.lay = lay
        self.n_card = lay.n_reg - N_SCALAR
        self.register_buffer("scale", torch.from_numpy(lay.scale))
        d = hidden
        self.cls = nn.Parameter(torch.zeros(1, 1, d))
        self.scalar_in = nn.Sequential(nn.Linear(N_SCALAR + 1 + len(PHASES), d), nn.GELU(), nn.Linear(d, d))
        n_blocks = len(lay.blocks)
        self.block_emb = nn.Embedding(n_blocks, d)
        self.card_emb = nn.Embedding(C, d)
        self.count_emb = nn.Embedding(8, d)
        self.kind = nn.Embedding(n_kinds, d)
        self.actor = nn.Embedding(2, d)
        self.role_emb = nn.Embedding(len(ROLES), d)
        self.token_emb = nn.Embedding(N_TOKENS, d)
        self.type_emb = nn.Embedding(4, d)  # cls/scalar, state card, action head, action feature
        layer = nn.TransformerEncoderLayer(d, heads, 4 * d, dropout=0.0, activation="gelu", batch_first=True,
                                           norm_first=True)
        self.encoder = nn.TransformerEncoder(layer, layers)
        self.out_norm = nn.LayerNorm(d)
        self.scalar = nn.Linear(d, N_SCALAR * (2 * SCALAR_RANGE + 1))
        self.card = nn.Linear(d, self.n_card * (2 * CARD_RANGE + 1))
        self.active = nn.Linear(d, 1)
        self.phase = nn.Linear(d, len(PHASES))
        self.over = nn.Linear(d, 1)
        self.win = nn.Linear(d, 1)
        self.legal = nn.Linear(d, 1)
        for head, r in ((self.scalar, SCALAR_RANGE), (self.card, CARD_RANGE)):
            nn.init.zeros_(head.weight)
            bias = torch.zeros(2 * r + 1); bias[r] = 4.0
            head.bias.data.copy_(bias.repeat(head.out_features // (2 * r + 1)))
        self.truncated = 0  # states with more card entries than MAX_STATE_TOKENS (counted, never silent)

    @staticmethod
    def _top(mask, k):
        """Indices of the True entries (up to k, in index order) and their validity."""
        n = mask.shape[1]
        key = mask.float() * (2 * n) - torch.arange(n, device=mask.device).float()[None, :] / n
        val, ix = key.topk(min(k, n), dim=1)
        return ix, val > 0

    def forward(self, state, kind, me, cards):
        lay = self.lay
        B = len(state)
        scal = torch.cat([state[:, :N_SCALAR] / self.scale[:N_SCALAR], state[:, lay.n_reg:]], 1)
        counts = state[:, N_SCALAR:lay.n_reg]
        present = counts > 0.5
        self.truncated += int((present.sum(1) > MAX_STATE_TOKENS).sum())
        ix, valid = self._top(present, MAX_STATE_TOKENS)
        cnt = counts.gather(1, ix).round().clamp(0, 7).long()
        st = (self.block_emb(ix // C) + self.card_emb(ix % C) + self.count_emb(cnt) + self.type_emb.weight[1])
        a_ix, a_valid = self._top(cards > 0.5, MAX_ACTION_TOKENS)
        is_card = a_ix < len(ROLES) * C
        a_tok = torch.where(is_card[..., None], self.role_emb((a_ix // C).clamp(max=len(ROLES) - 1))
                            + self.card_emb(a_ix % C),
                            self.token_emb((a_ix - len(ROLES) * C).clamp(min=0))) + self.type_emb.weight[3]
        head = (self.kind(kind) + self.actor(me.long()) + self.type_emb.weight[2])[:, None]
        sc = (self.scalar_in(scal) + self.type_emb.weight[0])[:, None]
        x = torch.cat([self.cls.expand(B, 1, -1), sc, head, st, a_tok], 1)
        pad = torch.cat([torch.zeros(B, 3, dtype=torch.bool, device=x.device), ~valid, ~a_valid], 1)
        with torch.autocast("cuda", dtype=torch.bfloat16, enabled=x.is_cuda):
            h = self.encoder(x, src_key_padding_mask=pad)[:, 0]
        h = self.out_norm(h.float())
        return dict(scalar=self.scalar(h).view(B, N_SCALAR, 2 * SCALAR_RANGE + 1),
                    card=self.card(h).view(B, self.n_card, 2 * CARD_RANGE + 1),
                    active=self.active(h)[:, 0], phase=self.phase(h), over=self.over(h)[:, 0],
                    win=self.win(h)[:, 0], legal=self.legal(h)[:, 0])


def make_net(arch, lay, n_kinds, hidden):
    return SetNet(lay, n_kinds, hidden) if arch == "set" else Net(lay, n_kinds, hidden)


def assemble(lay, cur, out):
    """Most likely next raw state: mode change per field, hard active and phase."""
    delta = torch.cat([out["scalar"].argmax(2) - SCALAR_RANGE, out["card"].argmax(2) - CARD_RANGE], 1).float()
    reg = cur[:, :lay.n_reg] + delta
    nonneg = torch.from_numpy(lay.nonneg).to(reg.device)
    reg = torch.where(nonneg, reg.clamp_min(0), reg)
    active = (out["active"] > 0).float()[:, None]
    phase = F.one_hot(out["phase"].argmax(1), len(PHASES)).float()
    return torch.cat([reg, active, phase], 1)


def step_loss(lay, cur, out, true_state, true_over, true_win, true_legal, scalar_weight):
    delta = (true_state[:, :lay.n_reg] - cur[:, :lay.n_reg]).round()
    ds = (delta[:, :N_SCALAR].clamp(-SCALAR_RANGE, SCALAR_RANGE) + SCALAR_RANGE).long()
    dc = (delta[:, N_SCALAR:].clamp(-CARD_RANGE, CARD_RANGE) + CARD_RANGE).long()
    ce_s = F.cross_entropy(out["scalar"].transpose(1, 2), ds, reduction="none")
    loss = (ce_s * scalar_weight).sum(1) / scalar_weight.sum()
    loss = loss + F.cross_entropy(out["card"].transpose(1, 2), dc, reduction="none").mean(1)
    loss = loss + F.binary_cross_entropy_with_logits(out["active"], true_state[:, lay.i_active], reduction="none")
    loss = loss + F.cross_entropy(out["phase"], true_state[:, lay.i_phase:].argmax(1), reduction="none")
    loss = loss + F.binary_cross_entropy_with_logits(out["over"], true_over, reduction="none")
    loss = loss + true_over * F.binary_cross_entropy_with_logits(out["win"], true_win, reduction="none")
    loss = loss + 0.1 * (out["legal"] - torch.log1p(true_legal)) ** 2
    return loss.mean()


def train(args):
    torch.manual_seed(args.seed); np.random.seed(args.seed)
    dev = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    tr = Split(args.data, "train", dev)
    lay = tr.lay
    net = make_net(args.arch, lay, tr.n_kinds, args.hidden).to(dev)
    opt = torch.optim.AdamW(net.parameters(), lr=args.lr, weight_decay=1e-4)
    scalar_weight = torch.ones(N_SCALAR, device=dev)
    scalar_weight[[i for i in lay.decisive]] = 5.0
    starts = tr.starts(args.unroll)
    steps_per_epoch = math.ceil(len(starts) / args.batch)
    sched = torch.optim.lr_scheduler.OneCycleLR(opt, args.lr, total_steps=args.epochs * steps_per_epoch)
    log = []
    t0 = time.time()
    for epoch in range(args.epochs):
        perm = np.random.permutation(starts)
        total = 0.0
        for b in range(steps_per_epoch):
            idx = perm[b * args.batch:(b + 1) * args.batch]
            t = torch.from_numpy(idx).to(dev)
            a = torch.from_numpy(tr.action_of_state[idx]).to(dev)
            cur = tr.state(t)
            loss = 0.0
            for j in range(args.unroll):
                out = net(cur, tr.a_kind[a + j], tr.a_me[a + j], tr.a_cards_h[a + j].float())
                nxt = t + j + 1
                loss = loss + step_loss(lay, cur, out, tr.state(nxt), tr.over[nxt], tr.win_me[nxt], tr.legal[nxt],
                                        scalar_weight)
                # Later steps start from the model's own most likely state (no gradient through the feed).
                cur = assemble(lay, cur, out).detach()
            loss = loss / args.unroll
            opt.zero_grad(set_to_none=True)
            loss.backward()
            nn.utils.clip_grad_norm_(net.parameters(), 1.0)
            opt.step(); sched.step()
            total += loss.item()
        log.append(dict(epoch=epoch, loss=total / steps_per_epoch, seconds=round(time.time() - t0, 1)))
        print(json.dumps(log[-1]), flush=True)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=False)
    torch.save(net.state_dict(), out / "model.pt")
    (out / "config.json").write_text(json.dumps(dict(vars(args), log=log,
                                                     truncated_states=getattr(net, "truncated", None)), indent=1))


@torch.no_grad()
def rollout_eval(net, sp: Split, k, persistence):
    """Per-window primary correctness and per-field correctness at horizon k."""
    lay = sp.lay
    starts = sp.starts(k)
    res = dict(primary=[], game=[], fields={n: [] for n in ("me_life", "opp_life", "me_creatures", "opp_creatures",
                                                            "over", "turn", "phase", "active")}, changed=[])
    for b in range(0, len(starts), 8192):
        t = torch.from_numpy(starts[b:b + 8192]).to(sp._state.device)
        a = torch.from_numpy(sp.action_of_state[starts[b:b + 8192]]).to(sp._state.device)
        cur = sp.state(t)
        done = torch.zeros(len(t), dtype=torch.bool, device=cur.device)
        if not persistence:
            for j in range(k):
                out = net(cur, sp.a_kind[a + j], sp.a_me[a + j], sp.a_cards_h[a + j].float())
                nxt = assemble(lay, cur, out)
                cur = torch.where(done[:, None], cur, nxt)
                done = done | (out["over"] > 0)
        true = sp.state(t + k)
        true_over = sp.over[t + k] > 0
        ok = {}
        for n in ("me_life", "opp_life", "me_creatures", "opp_creatures"):
            i = lay.idx[n]
            ok[n] = torch.round(cur[:, i]) == true[:, i]
        ok["over"] = done == true_over
        ok["turn"] = torch.round(cur[:, lay.idx["turn"]]) == true[:, lay.idx["turn"]]
        ok["phase"] = cur[:, lay.i_phase:].argmax(1) == true[:, lay.i_phase:].argmax(1)
        ok["active"] = (cur[:, lay.i_active] > 0.5) == (true[:, lay.i_active] > 0.5)
        prim = ok["me_life"] & ok["opp_life"] & ok["me_creatures"] & ok["opp_creatures"] & ok["over"]
        start = sp.state(t)
        changed = (start[:, lay.decisive] != true[:, lay.decisive]).any(1) | true_over
        res["primary"].append(prim.cpu().numpy())
        res["changed"].append(changed.cpu().numpy())
        for n in res["fields"]:
            res["fields"][n].append(ok[n].cpu().numpy())
        res["game"].append(sp.game_of_state[starts[b:b + 8192]])
    cat = lambda xs: np.concatenate(xs) if xs else np.zeros(0)
    return dict(primary=cat(res["primary"]), changed=cat(res["changed"]), game=cat(res["game"]),
                fields={n: cat(v) for n, v in res["fields"].items()})


def boot_ci(values, games, reps=2000, seed=7):
    """95% interval of a window mean, resampling whole games."""
    if len(values) == 0:
        return [float("nan")] * 2
    ug, inv = np.unique(games, return_inverse=True)
    sums = np.bincount(inv, weights=values.astype(np.float64)); cnts = np.bincount(inv)
    rng = np.random.default_rng(seed)
    draws = rng.integers(0, len(ug), size=(reps, len(ug)))
    means = sums[draws].sum(1) / cnts[draws].sum(1)
    return [float(np.quantile(means, 0.025)), float(np.quantile(means, 0.975))]


def evaluate(args):
    dev = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    sp = Split(args.data, args.split, dev)
    cfg = json.loads((Path(args.model) / "config.json").read_text())
    net = make_net(cfg.get("arch", "mlp"), sp.lay, sp.n_kinds, cfg["hidden"]).to(dev)
    net.load_state_dict(torch.load(Path(args.model) / "model.pt", map_location=dev))
    net.eval()
    report = dict(split=args.split, model=str(args.model), horizons={})
    for k in HORIZONS + DESCRIPTIVE_HORIZONS:
        entry = {}
        for arm, persist in (("model", False), ("persistence", True)):
            r = rollout_eval(net, sp, k, persist)
            ch = r["changed"]
            entry[arm] = dict(windows=int(len(r["primary"])), successes=int(r["primary"].sum()),
                              primary=float(r["primary"].mean()),
                              primary_ci=boot_ci(r["primary"], r["game"]),
                              primary_when_changed=float(r["primary"][ch].mean()) if ch.any() else None,
                              changed_share=float(ch.mean()),
                              fields={n: float(v.mean()) for n, v in r["fields"].items()})
            if arm == "model":
                model_windows = r
            else:
                diff = model_windows["primary"].astype(np.float64) - r["primary"]
                entry["gap"] = dict(mean=float(diff.mean()), ci=boot_ci(diff, r["game"]))
        report["horizons"][str(k)] = entry
        m, p = entry["model"], entry["persistence"]
        print(f"k={k:>2} model {m['primary']:.3f} {m['primary_ci']} persistence {p['primary']:.3f} "
              f"gap {entry['gap']['mean']:+.3f} changed {m['changed_share']:.2f}", flush=True)
    report["truncated_states"] = getattr(net, "truncated", None)
    Path(args.out).write_text(json.dumps(report, indent=1))


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("build")
    b.add_argument("--parts", required=True); b.add_argument("--manifest", required=True)
    b.add_argument("--out", required=True); b.add_argument("--full-info", action="store_true")
    t = sub.add_parser("train")
    t.add_argument("--data", required=True); t.add_argument("--out", required=True)
    t.add_argument("--hidden", type=int, default=1024); t.add_argument("--unroll", type=int, default=5)
    t.add_argument("--epochs", type=int, default=8); t.add_argument("--batch", type=int, default=1024)
    t.add_argument("--lr", type=float, default=1e-3); t.add_argument("--seed", type=int, default=20261021)
    t.add_argument("--arch", choices=["mlp", "set"], default="mlp")
    e = sub.add_parser("eval")
    e.add_argument("--data", required=True); e.add_argument("--model", required=True)
    e.add_argument("--split", required=True, choices=["validation", "test", "unseen_test", "train"])
    e.add_argument("--out", required=True)
    args = ap.parse_args()
    dict(build=build, train=train, eval=evaluate)[args.cmd](args)


if __name__ == "__main__":
    main()
