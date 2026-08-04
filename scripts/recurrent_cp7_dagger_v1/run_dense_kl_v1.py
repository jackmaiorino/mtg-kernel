#!/usr/bin/env python3
"""Full CP7 imitation with strong parent-KL regularization."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import random
import time
from typing import Any

os.environ.setdefault("CUBLAS_WORKSPACE_CONFIG", ":4096:8")

import torch
from torch import Tensor

import run_screen_v1 as base
import run_sparse_correction_v1 as sparse
from model_v1 import pack_rows


SCHEMA = "mtg-kernel-dense-kl-recurrent-cp7-screen/v1"
BETAS = (3.0, 10.0, 30.0, 100.0, 300.0)
EPOCHS = 8
BATCH_SIZE = 256
LOG_RATIO_BUDGET = 0.49


def _loss(
    model: Any,
    decisions: list[Any],
    beta: float,
    device: torch.device,
) -> tuple[Tensor, dict[str, float]]:
    rows = [decision.rows[0] for decision in decisions]
    packed = pack_rows(rows, device)
    candidate_logits, _ = base._candidate_logits(
        model, packed, LOG_RATIO_BUDGET
    )
    teacher = torch.tensor(
        [int(row["teacher_selected_index"]) for row in rows],
        dtype=torch.long,
        device=device,
    )
    weights = torch.tensor(
        [float(decision.episode_weight) for decision in decisions],
        dtype=torch.float32,
        device=device,
    )
    parent_logp = torch.log_softmax(packed.parent_logits, dim=1)
    candidate_logp = torch.log_softmax(candidate_logits, dim=1)
    parent_probability = torch.softmax(packed.parent_logits, dim=1)
    nll = -candidate_logp.gather(1, teacher.unsqueeze(1)).squeeze(1)
    cross_entropy = (nll * weights).sum() / weights.sum().clamp_min(1.0e-12)
    parent_kl = (parent_probability * (parent_logp - candidate_logp)).sum(dim=1)
    preservation = (parent_kl * weights).sum() / weights.sum().clamp_min(1.0e-12)
    total = cross_entropy + beta * preservation
    return total, {
        "cross_entropy": float(cross_entropy.detach()),
        "parent_kl": float(preservation.detach()),
    }


def _fit_arm(
    train: list[Any],
    selection: list[Any],
    beta: float,
    device: torch.device,
) -> tuple[Any, dict[str, Any]]:
    model = base._new_model(device)
    parameters = [parameter for parameter in model.parameters() if parameter.requires_grad]
    optimizer = torch.optim.AdamW(
        parameters, lr=base.LEARNING_RATE, weight_decay=base.WEIGHT_DECAY
    )
    rng = random.Random(base.SEED)
    history: list[dict[str, Any]] = []
    checkpoints: list[
        tuple[tuple[float, float, float], int, dict[str, Tensor], dict[str, Any]]
    ] = []
    for epoch in range(1, EPOCHS + 1):
        model.train()
        started = time.perf_counter()
        sums = {"loss": 0.0, "cross_entropy": 0.0, "parent_kl": 0.0}
        gradient_max = 0.0
        steps = 0
        for batch in sparse._batches(train, rng):
            loss, parts = _loss(model, batch, beta, device)
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            gradient = torch.nn.utils.clip_grad_norm_(parameters, base.GRADIENT_CAP)
            if not torch.isfinite(gradient):
                base._fail("non-finite dense-KL gradient")
            optimizer.step()
            sums["loss"] += float(loss.detach())
            sums["cross_entropy"] += parts["cross_entropy"]
            sums["parent_kl"] += parts["parent_kl"]
            gradient_max = max(gradient_max, float(gradient))
            steps += 1
        torch.cuda.synchronize(device)
        metrics = base._evaluate(
            model, selection, BATCH_SIZE, device, LOG_RATIO_BUDGET
        )
        gate = base._gate(metrics)
        history.append(
            {
                "epoch": epoch,
                "seconds_including_selection": time.perf_counter() - started,
                "optimizer_steps": steps,
                "maximum_preclip_gradient_norm": gradient_max,
                "mean_loss": sums["loss"] / steps,
                "mean_cross_entropy": sums["cross_entropy"] / steps,
                "mean_parent_kl": sums["parent_kl"] / steps,
                "selection": metrics,
                "gate": gate,
            }
        )
        checkpoints.append(
            (
                sparse._checkpoint_rank(metrics),
                epoch,
                {
                    name: tensor.detach().cpu().clone()
                    for name, tensor in model.state_dict().items()
                },
                metrics,
            )
        )
    selected = min(checkpoints, key=lambda row: row[0])
    model.load_state_dict(selected[2], strict=True)
    return model, {
        "beta": beta,
        "selected_epoch": selected[1],
        "selection": selected[3],
        "gate": base._gate(selected[3]),
        "training_history": history,
        "model_state_sha256": base._state_sha256(model),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument(
        "--corpus-report",
        type=Path,
        default=Path(r"D:\mtg-kernel-cp7-dagger-shadow-v1-synchronized\report.json"),
    )
    parser.add_argument(
        "--history-cache",
        type=Path,
        default=Path(r"D:\mtg-kernel-cp7-dagger-shadow-v1-synchronized\complete-history-cache.pt"),
    )
    parser.add_argument("--device", type=int, default=1)
    args = parser.parse_args()
    if args.output_dir.exists() and any(args.output_dir.iterdir()):
        base._fail("output directory must be absent or empty")
    device = base._configure(args.device)
    started = time.perf_counter()
    decisions, source, load_timings = base._load(
        args.corpus_report, args.history_cache
    )
    train, selection, heldout = base._split(decisions)
    arms = [_fit_arm(train, selection, beta, device) for beta in BETAS]
    passing = [arm for arm in arms if arm[1]["gate"]["pass"]]
    selected = (
        min(
            passing,
            key=lambda arm: (
                arm[1]["selection"]["overall"]["mean_total_variation"],
                -arm[1]["selection"]["overall"]["relative_nll_improvement"],
            ),
        )
        if passing
        else None
    )
    model_path: Path | None = None
    if selected is not None:
        model_path = args.output_dir / "model.pt"
        model_path.parent.mkdir(parents=True, exist_ok=True)
        torch.save(
            {
                "schema": SCHEMA + ".model",
                "beta": selected[1]["beta"],
                "selected_epoch": selected[1]["selected_epoch"],
                "log_ratio_budget": LOG_RATIO_BUDGET,
                "model_state_dict": {
                    name: tensor.detach().cpu()
                    for name, tensor in selected[0].state_dict().items()
                },
                "model_state_sha256": selected[1]["model_state_sha256"],
            },
            model_path,
        )
    result = {
        "schema": SCHEMA,
        "decision": "PASS" if selected is not None else "REJECT",
        "source": source,
        "load_timings": load_timings,
        "config": {
            "betas": BETAS,
            "epochs": EPOCHS,
            "batch_size": BATCH_SIZE,
            "log_ratio_budget": LOG_RATIO_BUDGET,
            "seed": base.SEED,
            "objective": "all-row CP7 CE plus all-row parent KL",
        },
        "split": {
            "train_decisions": len(train),
            "selection_decisions": len(selection),
            "reserved_revealed_residue0_decisions": len(heldout),
            "residue0_used": False,
        },
        "arms": [arm[1] for arm in arms],
        "selected_beta": selected[1]["beta"] if selected is not None else None,
        "selected_epoch": selected[1]["selected_epoch"] if selected is not None else None,
        "selected_model_state_sha256": selected[1]["model_state_sha256"] if selected is not None else None,
        "toolchain": base._toolchain(device),
        "git_commit": base._git_head(),
        "total_seconds": time.perf_counter() - started,
        "non_claims": [
            "the previously revealed residue-0 split was not evaluated",
            "selection-fit diagnostics are not playing strength",
            "a pass requires a fresh disjoint label panel",
            "terminal outcome remains the only promotion measure",
        ],
    }
    base._write_new(args.output_dir / "report.json", result)
    outputs = {"report_sha256": base._sha256(args.output_dir / "report.json")}
    if model_path is not None:
        outputs["model_file_sha256"] = base._sha256(model_path)
    base._write_new(
        args.output_dir / "manifest.json",
        {
            "schema": SCHEMA + ".manifest",
            "git_commit": base._git_head(),
            "seed": base.SEED,
            "toolchain": base._toolchain(device),
            "inputs": {
                "corpus_report_sha256": source["corpus_report_sha256"],
                "history_cache_sha256": source["history_cache_sha256"],
                "teacher_task_sha256s": source["teacher_task_sha256s"],
            },
            "outputs": outputs,
        },
    )
    print(json.dumps(result, sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
