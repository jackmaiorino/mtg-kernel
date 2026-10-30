"""Shared W4 Step A readers and fits (collab LANES/w4-combat-20261030/DESIGN.md section 4). Measurement only."""
from __future__ import annotations

import glob
import hashlib
import json
from pathlib import Path

import numpy as np

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
    scikit-learn convention), by damped Newton steps. Returns (intercept, beta)."""
    n, k = x.shape
    z = np.hstack([np.ones((n, 1)), x])
    pen = np.full(k + 1, 1.0 / c)
    pen[0] = 0.0

    def objective(t):
        return float((w * log_loss(y, sigmoid(z @ t))).sum() + 0.5 * (pen * t * t).sum())

    theta = np.zeros(k + 1)
    current = objective(theta)
    for _ in range(100):
        p = sigmoid(z @ theta)
        grad = z.T @ (w * (p - y)) + pen * theta
        hess = (z * (w * p * (1 - p))[:, None]).T @ z + np.diag(pen) + 1e-10 * np.eye(k + 1)
        step = np.linalg.solve(hess, grad)
        scale = 1.0
        while scale > 1e-8:
            trial = theta - scale * step
            value = objective(trial)
            if value <= current:
                break
            scale /= 2
        if current - value < 1e-12 * max(1.0, abs(current)):
            theta = trial if value <= current else theta
            break
        theta, current = trial, value
    return theta[0], theta[1:]


def ranks(v):
    """Average ranks (ties share their mean rank)."""
    v = np.asarray(v, float)
    order = np.argsort(v, kind="mergesort")
    r = np.empty(len(v))
    sv = v[order]
    i = 0
    while i < len(v):
        j = i
        while j + 1 < len(v) and sv[j + 1] == sv[i]:
            j += 1
        r[order[i:j + 1]] = (i + j) / 2 + 1
        i = j + 1
    return r


def spearman(a, b):
    """Spearman correlation: Pearson correlation of average ranks."""
    ra, rb = ranks(a), ranks(b)
    return float(np.corrcoef(ra, rb)[0, 1])


def predict(model, x):
    a, b = model
    return sigmoid(a + x @ b)


def group_folds(groups, k=5):
    """Five folds of whole groups: groups ordered by SHA-256 of their key, dealt in turn."""
    keys = sorted(set(groups), key=lambda g: hashlib.sha256(str(g).encode()).hexdigest())
    fold_of = {g: i % k for i, g in enumerate(keys)}
    return np.array([fold_of[g] for g in groups])


def choose_c(x, y, w, groups, metric_w):
    """C from the grid by 5-fold grouped cross-validation, by mean held-out log loss (weighted by
    `metric_w`); ties go to the smaller C. `x` is raw: each training fold is standardized by its own
    (w-weighted) moments, which are then applied to its held-out fold. Returns (C, per-C losses,
    out-of-fold predictions at the chosen C)."""
    folds = group_folds(groups)
    losses, oof = {}, {}
    for c in C_GRID:
        pred = np.zeros(len(y))
        for f in range(5):
            tr, te = folds != f, folds == f
            xtr, xte = standardize(x[tr], w[tr], x[te])
            pred[te] = predict(fit_logistic(xtr, y[tr], w[tr], c), xte)
        oof[c] = pred
        losses[c] = float((metric_w * log_loss(y, pred)).sum() / metric_w.sum())
    best = min(C_GRID, key=lambda c: (round(losses[c], 12), c))
    return best, losses, oof[best]


def auc(y, s):
    """Mann-Whitney AUC with ties counted half."""
    y = np.asarray(y, bool)
    if y.all() or not y.any():
        return None
    r = ranks(s)
    npos, nneg = int(y.sum()), int((~y).sum())
    return float((r[y].sum() - npos * (npos + 1) / 2) / (npos * nneg))


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
