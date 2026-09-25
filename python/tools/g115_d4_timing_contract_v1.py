"""Admission for small expanded-trainer timing recipes. No execution entry point.

This is not a substantial-run allocation validator. D4 timing must separately
bind host inventory, native binary, complete model inputs, fresh output roots,
WMI transport, owned-child scheduling, process/wall bounds and output audits.
"""
import copy
import hashlib
import json
from pathlib import Path


def pinned_json(item):
    raw = Path(item["path"]).read_bytes()
    if hashlib.sha256(raw).hexdigest() != item["sha256"]:
        raise ValueError("timing input differs from SHA256 pin")
    return json.loads(raw)


def validate_recipe(config):
    if config.get("schema") != "mtg-kernel-native-expanded-training-run/v1":
        raise ValueError("only the expanded-training recipe is supported")
    updates = config.get("iterations", [])
    if not 1 <= len(updates) <= 3:
        raise ValueError("timing limited to one through three complete updates")
    if any(len(update.get("episodes", [])) != 10 for update in updates):
        raise ValueError("timing preserves ten-game synchronous batches")
    if config.get("max_non_natural_episode_fraction") != 0:
        raise ValueError("only natural-terminal collection permitted")
    loss = config.get("loss_selection", {})
    if loss != dict(kind="gae_advantage_value_v1", gamma=1.0, **{"lambda": 0.9}, entropy_coefficient=0.0):
        raise ValueError("timing loss differs from candidate recipe")
    if config.get("learning_rate") != 0.0001 or config.get("value_coefficient") != 0.5:
        raise ValueError("timing update differs from candidate recipe")
    if config.get("update_backend", {}).get("kind") != "cuda":
        raise ValueError("timing requires candidate CUDA update backend")
    seeds, ids = set(), set()
    for batch in updates:
        for item in batch["episodes"]:
            episode = item["episode"]
            if episode["postboard"] or episode["registered"] != episode["selected"]:
                raise ValueError("timing must preserve preboard recipe")
            if episode["seed"] in seeds or episode["id"] in ids:
                raise ValueError("duplicate timing episode seed or ID")
            seeds.add(episode["seed"])
            ids.add(episode["id"])
    return dict(updates=len(updates), games=10 * len(updates))


def bind_placement(template, output_directory, workers, gpu_ordinal):
    validate_recipe(template)
    if type(workers) is not int or workers not in (1, 4, 8):
        raise ValueError("timing collector count outside declared matrix")
    if type(gpu_ordinal) is not int or gpu_ordinal < 0:
        raise ValueError("invalid execution GPU ordinal")
    output = Path(output_directory)
    if not output.is_absolute() or output.exists():
        raise ValueError("fresh absolute timing output required")
    result = copy.deepcopy(template)
    result["collection_workers"] = workers
    result["update_backend"]["device_ordinal"] = gpu_ordinal
    result["output_directory"] = str(output)
    return result
