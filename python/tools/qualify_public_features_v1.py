"""Cross-language features and Python-only g115 state/gradient qualification."""
import argparse
import hashlib
import json
from pathlib import Path
import sys
import time
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import torch
from mtg_kernel_rl import features_v7
from mtg_kernel_rl.public_cost_features_v1 import catalog, from_actor_v4
from mtg_kernel_rl.public_feature_model_v1 import PUBLIC_NAMES, bit_tensor, encode, engineering_step, identity, import_checkpoint, model

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--samples", type=Path, required=True)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--checkpoint", type=Path, required=True)
    parser.add_argument("--checkpoint-sha256", required=True)
    args = parser.parse_args()
    args.root.mkdir(parents=True, exist_ok=False)
    torch.set_num_threads(1)
    started = time.monotonic()
    samples = json.loads(args.samples.read_text())
    # Native fixture exports ordered semantics. These diagnostic wrappers carry
    # no runtime action identity; operational/forbidden fields never enter tensors.
    for sample in samples["samples"]:
        sample["actions"] = [dict(schema_version=5, selected_index=i,
            stable_id=f"legal-action-v5:diagnostic-{i}", display_text=None, semantic=semantic)
            for i, semantic in enumerate(sample["actions"])]
    native_catalog = samples["catalog"]
    python_catalog = catalog()
    for key, value in python_catalog.items():
        assert native_catalog[key] == value, key
    for sample in samples["samples"]:
        py = features_v7.encode_decision(sample["observation"], sample["actions"])
        for name, values in sample["native"].items():
            actual = getattr(py,name).reshape(-1)
            expected = torch.tensor(values, dtype=actual.dtype)
            assert torch.equal(actual,expected), name
        assert from_actor_v4(sample["observation"], py.object_card_ids.tolist()) == sample["public"]
    baseline, _, source = import_checkpoint(args.checkpoint, args.checkpoint_sha256, False)
    successor, optimizer, _ = import_checkpoint(args.checkpoint, args.checkpoint_sha256, True)
    for key in ["parameters","first_moments","second_moments"]:
        for record in source[key]:
            parameter = dict(successor.named_parameters())[record["name"]]
            actual = parameter if key == "parameters" else optimizer.state[parameter]["exp_avg" if key == "first_moments" else "exp_avg_sq"]
            assert torch.equal(actual.detach().view(torch.int32),bit_tensor(record).view(torch.int32)), (key,record["name"])
    for sample in samples["samples"]:
        with torch.no_grad():
            old = baseline(encode(sample["observation"],sample["actions"],False))
            new = successor(encode(sample["observation"],sample["actions"],True))
        assert all(torch.equal(a.view(torch.int32),b.view(torch.int32)) for a,b in zip(old,new)), "zero projection changed scores"
    # Choose the already-declared black/red shield fixture, not an outcome.
    sample = next(s for s in samples["samples"] if s["public"]["state"] == [0.,0.,1.,1.,0.,0.])
    encoded = encode(sample["observation"],sample["actions"],True)
    gradients = engineering_step(successor,optimizer,encoded)
    assert set(gradients) == PUBLIC_NAMES and all(value > 0 for value in gradients.values())
    checkpoint_path = args.root / "research-resume.pt"
    torch.save(dict(schema="public-input-research-resume/v1", identity=identity()[1], model=successor.state_dict(), optimizer=optimizer.state_dict()), checkpoint_path)
    resumed, resumed_optimizer, _ = import_checkpoint(args.checkpoint,args.checkpoint_sha256,True)
    saved = torch.load(checkpoint_path,weights_only=True)
    assert saved["identity"] == identity()[1]
    resumed.load_state_dict(saved["model"],strict=True)
    resumed_optimizer.load_state_dict(saved["optimizer"])
    engineering_step(successor,optimizer,encoded)
    engineering_step(resumed,resumed_optimizer,encoded)
    for (name,a),(other,b) in zip(successor.named_parameters(),resumed.named_parameters()):
        assert name == other and torch.equal(a.detach().view(torch.int32),b.detach().view(torch.int32))
        for key in ["step","exp_avg","exp_avg_sq"]:
            assert torch.equal(optimizer.state[a][key],resumed_optimizer.state[b][key])
        assert optimizer.state[a]["step"].item() == (2 if name in PUBLIC_NAMES else source["adam_step"]+2)
    result = dict(status="PYTHON-ENGINEERING-PASS", samples=len(samples["samples"]), registry_rows=len(python_catalog["rows"]),
        all_thirteen_base_arrays_exact=True, auxiliary_features_exact=True,
        imported_parameters_and_moments_bit_exact=True, initial_logits_and_values_bit_exact=True,
        new_projection_gradients=gradients, resumed_parameters_and_adam_exact=True,
        imported_adam_step=source["adam_step"], new_adam_initial_step=0,
        seconds=time.monotonic()-started, torch=torch.__version__, model_identity=identity(),
        source_sha256=args.checkpoint_sha256, samples_sha256=hashlib.sha256(args.samples.read_bytes()).hexdigest(),
        non_claim="No model trained on game rewards, no native successor scorer/update integration, no CUDA optimizer parity, no playing-strength claim. Two synthetic steps only verify connectivity and Python save/resume.")
    with (args.root / "qualification.json").open("x") as handle: json.dump(result,handle,indent=2)
    print(json.dumps(result,indent=2))

if __name__ == "__main__": main()
