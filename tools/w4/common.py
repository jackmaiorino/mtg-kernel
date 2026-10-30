"""Shared W4 Step A readers and fits (collab LANES/w4-combat-20261030/DESIGN.md section 4). Measurement only."""
from __future__ import annotations

import glob
import hashlib
import json
from pathlib import Path

import numpy as np
from scipy.optimize import minimize

C_GRID = (1e-3, 1e-2, 1e-1, 1.0, 10.0, 1e2, 1e3)
BOOTSTRAP = 2000
HIDDEN = 64
ACTION_DIM = 195
OBJECT_DIM = 98


def deck(deck_id: str) -> str:
    return deck_id.split("/")[0]


def template_half(attacker_card: int, blocker_card: int, focal_attacker: int) -> str:
    """A1b include-flag split: last byte of SHA-256 of the template key, even fit, odd read."""
    key = f"a{attacker_card}:b{blocker_card}:f{focal_attacker}"
    return "read" if hashlib.sha256(key.encode("utf-8")).digest()[-1] % 2 else "fit"


class Scores:
    """The `s4a-w4 score` rows of one or more shards, with their hidden files.
    Each shard is `<stem>.jsonl` beside `<stem>.f32`."""

    def __init__(self, stems: list[Path]):
        self.decisions, self.games, self.errors = [], {}, []
        self.hidden = []
        for k, stem in enumerate(stems):
            self.hidden.append(np.memmap(str(stem) + ".f32", dtype="<f4", mode="r"))
            for line in open(str(stem) + ".jsonl", encoding="utf-8"):
                row = json.loads(line)
                row["_shard"] = k
                {"decision": self.decisions.append, "error": self.errors.append}.get(
                    row["kind"], lambda r: self.games.__setitem__(r["id"], r))(row)
        # Decisions of games whose replay failed are dropped with the game.
        self.decisions = [d for d in self.decisions if d["id"] in self.games]

    def vec(self, row: dict, offset: int, n: int) -> np.ndarray:
        return np.asarray(self.hidden[row["_shard"]][offset:offset + n], dtype=np.float64)


def load_scores(directory: Path) -> Scores:
    stems = sorted(Path(p[:-6]) for p in glob.glob(str(directory / "score-*.jsonl")))
    return Scores(stems)


def sigmoid(z):
    return 1.0 / (1.0 + np.exp(-z))


def log_loss(y, p):
    p = np.clip(p, 1e-12, 1 - 1e-12)
    return -(y * np.log(p) + (1 - y) * np.log(1 - p))


def standardize(x_fit, w_fit, *others):
    """Weighted mean and SD from the fit rows; a constant column is left centred."""
    w = w_fit / w_fit.sum()
    mu = (x_fit * w[:, None]).sum(0)
    sd = np.sqrt(((x_fit - mu) ** 2 * w[:, None]).sum(0))
    sd[sd == 0] = 1.0
    return [(x - mu) / sd for x in (x_fit, *others)]


def fit_logistic(x, y, w, c):
    """Weighted L2 logistic regression with fractional targets: minimizes
    sum_i w_i * logloss_i + |beta|^2 / (2C), the intercept unpenalized (the
    scikit-learn convention). Returns (intercept, beta)."""
    n, k = x.shape

    def f(theta):
        z = theta[0] + x @ theta[1:]
        p = sigmoid(z)
        loss = (w * log_loss(y, p)).sum() + (theta[1:] ** 2).sum() / (2 * c)
        g = w * (p - y)
        grad = np.concatenate([[g.sum()], x.T @ g + theta[1:] / c])
        return loss, grad

    res = minimize(f, np.zeros(k + 1), jac=True, method="L-BFGS-B", options={"maxiter": 2000})
    return res.x[0], res.x[1:]


def predict(model, x):
    a, b = model
    return sigmoid(a + x @ b)


def group_folds(groups, k=5):
    """Five folds of whole groups: groups ordered by SHA-256 of their key, dealt in turn."""
    keys = sorted(set(groups), key=lambda g: hashlib.sha256(str(g).encode()).hexdigest())
    fold_of = {g: i % k for i, g in enumerate(keys)}
    return np.array([fold_of[g] for g in groups])


def choose_c(x, y, w, groups, metric_w):
    """C from the grid by 5-fold grouped cross-validation, by mean held-out log loss
    (weighted by `metric_w`); ties go to the smaller C. Returns (C, per-C losses, out-of-fold
    predictions at the chosen C)."""
    folds = group_folds(groups)
    losses, oof = {}, {}
    for c in C_GRID:
        pred = np.zeros(len(y))
        for f in range(5):
            tr, te = folds != f, folds == f
            pred[te] = predict(fit_logistic(x[tr], y[tr], w[tr], c), x[te])
        oof[c] = pred
        losses[c] = float((metric_w * log_loss(y, pred)).sum() / metric_w.sum())
    best = min(C_GRID, key=lambda c: (round(losses[c], 12), c))
    return best, losses, oof[best]


def auc(y, s):
    """Mann-Whitney AUC with ties counted half."""
    y = np.asarray(y, bool)
    pos, neg = s[y], s[~y]
    if len(pos) == 0 or len(neg) == 0:
        return None
    order = np.argsort(np.concatenate([pos, neg]), kind="mergesort")
    ranks = np.empty(len(order))
    allv = np.concatenate([pos, neg])[order]
    i = 0
    while i < len(allv):
        j = i
        while j + 1 < len(allv) and allv[j + 1] == allv[i]:
            j += 1
        ranks[order[i:j + 1]] = (i + j) / 2 + 1
        i = j + 1
    return float((ranks[:len(pos)].sum() - len(pos) * (len(pos) + 1) / 2) / (len(pos) * len(neg)))


def bootstrap_groups(groups, stat, seed):
    """Percentile 95% interval of `stat(index array)` over BOOTSTRAP resamples of whole groups
    (numpy Generator(PCG64(seed)))."""
    rng = np.random.Generator(np.random.PCG64(seed))
    keys = sorted(set(groups))
    members = {g: np.flatnonzero(np.asarray(groups) == g) for g in keys}
    out = []
    for _ in range(BOOTSTRAP):
        pick = rng.integers(0, len(keys), len(keys))
        idx = np.concatenate([members[keys[i]] for i in pick])
        v = stat(idx)
        if v is not None and np.isfinite(v):
            out.append(v)
    if not out:
        return None, None, 0
    return float(np.percentile(out, 2.5)), float(np.percentile(out, 97.5)), len(out)
