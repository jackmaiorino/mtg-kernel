"""W5 E4: S0's frozen distillation recipe (collab LANES/lead/W5-DESIGN.md v1.3 section 6.2), shared by s0.py
(which writes it into the frozen config) and s0_train.py (which runs it). No torch import here.

Design 6.2 fixes: S's architecture, the pilot's initializer with seed 20261102, Adam at lr 1e-4, batch 256
decisions, the pilot's schedule, dropout 0.1 in distillation only, the policy loss (soft-target KL to b512's logits
on every decision), the value loss (outcome log loss on 8 decisions per game per epoch, renormalized), at most 3
epochs with early stopping on validation value.

Pinned from the W7 pilot (collab LANES/bignet-pilot-20261031; files under code/ and checks-code/):
- architecture and initializer: code/models.py ObjectTransformer (lines 128 to 154: Linear weights and embeddings
  N(0, 0.02), biases zero, LayerNorm (1, 0), ref_bias zero, via `apply(_init)` after construction) at S's size
  (code/train.py line 255; vocab 257 and dropout 0.1, models.py line 131), built after `torch.manual_seed(seed)`
  (train.py line 249). The same seed seeds the batch order (train.py line 474) and, plus 104729, the value
  subsample (checks-code/train_checks.py line 224), as the pilot's single `--seed`.
- dropout placement: attention probabilities and both residual branches (models.py lines 107 to 125).
- schedule: linear warm-up over max(1, int(0.02 x total)) steps, then cosine from 1 to 0.1 of the rate over the
  remaining steps (train.py lines 457 to 460), total = 3 epochs x steps per epoch, steps per epoch = training
  decisions // 256 (train.py line 445).
- batching: an epoch is a seeded permutation cut into pools of 64 batches, each pool ordered by record length,
  batch order shuffled (train.py lines 344 to 353); micro-batches bounded by 256 x 96^2 padded tokens squared
  (train.py lines 245 and 293 to 306), the loss the same mean however a batch is split.
- losses: the soft policy term (checks-code/train_checks.py lines 118 to 121, written here as the KL, which has the
  same gradient) and the value subsample (train_checks.py lines 40 and 145 to 155: 8 seeded decisions per training
  game per epoch, all of a shorter game; the value term divided by the batch's selected count, line 248); value plus
  policy at unit weight (line 249).
- evaluation and stopping: every max(steps per epoch // 4, 1563) steps (train.py lines 242 and 448); validation
  value = value log loss on every validation decision, averaged within each game and then over games (train.py
  lines 396 to 400 and 524); the saved state is the lowest validation value (line 533); stop after 8 evaluations
  without improvement (lines 243 and 537 to 541) or at the epoch cap.
- precision and determinism: bf16 autocast on CUDA with the residual stream and heads in fp32 (train.py line 367,
  models.py lines 167 to 201), fp32 on CPU; deterministic algorithms, no TF32 (train.py lines 411 to 418).
- Adam betas (0.9, 0.999) (train.py line 456) and PyTorch's eps 1e-8.

Where the pilot and the design differ, the design is applied:
- optimizer: the design names Adam, so no weight decay (the pilot ran AdamW with decay 0.01 on matrices,
  train.py lines 453 to 456);
- the policy target is b512's distribution, not the sampled action (the pilot's primary used the action);
- the epoch cap is 3 (the pilot's was 4 to 16 by subset).

Native S's gauge parameters (`policy.2.bias` and the key third of every `qkv.bias`; softmax ignores a shared
shift) get a zero gradient, as in native S and its float64 reference, so they stay at their initial zero.

Open (design silent, changes results): gradient-norm clipping. The pilot clipped at 1.0 (train.py line 512). The
config records the lead's choice explicitly (`grad_clip_norm`: a number or null); nothing defaults it.
"""
from __future__ import annotations

SEED = 20261102
ARCH = {"d": 96, "layers": 3, "heads": 4, "ffn": 384, "card_dim": 32, "vocab": 257, "dropout": 0.1}
LR = 1e-4
BETAS = (0.9, 0.999)
EPS = 1e-8
WEIGHT_DECAY = 0.0
BATCH = 256
EPOCHS_MAX = 3
WARMUP_FRACTION = 0.02
LR_FLOOR = 0.1
MIN_EVAL_STEPS = 1563
PATIENCE = 8
POOL = 64
TOKEN_BUDGET = 256 * 96 ** 2
VSUB_K = 8
VSUB_SEED_OFFSET = 104729
MAX_CARD_TOKEN = 192          # the b512 registry's 192 cards, token = card id + 1 (pilot code/collate.py line 7)


def recipe(grad_clip_norm) -> dict:
    if grad_clip_norm is not None:
        grad_clip_norm = float(grad_clip_norm)
        if not grad_clip_norm > 0:
            raise ValueError("grad_clip_norm is positive or None")
    return {"architecture": dict(ARCH), "init": "pilot models.py _init after torch.manual_seed(seed)", "seed": SEED,
            "optimizer": {"kind": "adam", "lr": LR, "betas": list(BETAS), "eps": EPS, "weight_decay": WEIGHT_DECAY},
            "grad_clip_norm": grad_clip_norm, "batch_decisions": BATCH, "epochs_max": EPOCHS_MAX,
            "schedule": {"kind": "pilot warmup-cosine", "warmup_fraction": WARMUP_FRACTION, "floor": LR_FLOOR},
            "evaluation": {"min_eval_steps": MIN_EVAL_STEPS, "per_epoch": 4, "patience": PATIENCE,
                           "metric": "validation value log loss, game-mean"},
            "batching": {"pool": POOL, "token_budget": TOKEN_BUDGET},
            "loss": {"policy": "KL(softmax(b512 logits) || S) on every decision, mean over the batch",
                     "value": f"outcome log loss on {VSUB_K} seeded decisions per game per epoch, divided by the "
                              "batch's selected count", "weights": [1.0, 1.0], "vsub_k": VSUB_K,
                     "vsub_seed_offset": VSUB_SEED_OFFSET},
            "dropout": ARCH["dropout"], "amp": "bf16 autocast on CUDA (heads and residual fp32); fp32 on CPU",
            "gauge_parameters": "zero gradient: policy.2.bias and the key third of every blocks.<i>.qkv.bias",
            "max_card_token": MAX_CARD_TOKEN}
