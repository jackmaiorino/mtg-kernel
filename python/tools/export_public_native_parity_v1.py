"""Fixed nonzero projection probes for the native scorer, without training."""
import argparse
import hashlib
import json
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import torch
from mtg_kernel_rl.public_feature_model_v1 import encode, identity, import_checkpoint


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--samples", type=Path, required=True)
    parser.add_argument("--checkpoint", type=Path, required=True)
    parser.add_argument("--checkpoint-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        raise FileExistsError(args.output)
    torch.set_num_threads(1)
    model, _, _ = import_checkpoint(args.checkpoint, args.checkpoint_sha256, True)
    raw = args.samples.read_bytes()
    samples = json.loads(raw)["samples"]
    variants = []
    for name, use_object, use_state in [("zero",False,False),("object",True,False),("state",False,True),("both",True,True)]:
        with torch.no_grad():
            for matrix, enabled in [(model.object_encoder[0].public_weight,use_object),(model.state_encoder[0].public_weight,use_state)]:
                # Signed binary fractions, independent of outcomes or gradients.
                probe = (torch.arange(matrix.numel()).reshape(matrix.shape) % 17 - 8).float() / 64
                matrix.copy_(probe if enabled else torch.zeros_like(matrix))
            outputs = []
            for sample in samples:
                actions = [dict(schema_version=5, selected_index=i, stable_id=f"legal-action-v5:diagnostic-{i}",
                    display_text=None, semantic=semantic) for i,semantic in enumerate(sample["actions"])]
                logits,value = model(encode(sample["observation"],actions,True))
                outputs.append(dict(logits=logits.tolist(),value=value.item()))
        variants.append(dict(name=name,object=model.object_encoder[0].public_weight.detach().reshape(-1).tolist(),
            state=model.state_encoder[0].public_weight.detach().reshape(-1).tolist(),outputs=outputs))
    result = dict(schema="public-input-native-forward-qualification/v1",checkpoint=str(args.checkpoint),
        checkpoint_sha256=args.checkpoint_sha256,samples=str(args.samples),samples_sha256=hashlib.sha256(raw).hexdigest(),
        model_identity=identity(),torch=torch.__version__,absolute_tolerance=1e-3,relative_tolerance=1e-3,variants=variants,
        non_claim="Fixed projections are numerical probes, not trained or selected candidates.")
    args.output.parent.mkdir(parents=True,exist_ok=True)
    with args.output.open("x") as handle: json.dump(result,handle,indent=2)
    print(json.dumps(dict(output=str(args.output),samples=len(samples),variants=len(variants))))

if __name__ == "__main__": main()
