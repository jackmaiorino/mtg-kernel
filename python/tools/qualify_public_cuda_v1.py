"""Prepare and audit a CUDA gradient/update/resume probe, never a learning run."""
import argparse
import hashlib
import json
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import numpy as np
import torch
from mtg_kernel_rl.public_feature_model_v1 import PUBLIC_NAMES,encode,import_checkpoint,identity


def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()


def bits(values): return torch.from_numpy(np.asarray(values,dtype=np.uint32).view(np.float32).copy())


def native_snapshot(path):
    saved=json.loads(path.read_bytes())
    result={}
    for category in ["parameters","first_moments","second_moments"]:
        result[category]={r["name"]:bits(r["values"]).reshape(r["shape"]) for r in saved[category]}
    for layer,width in [("object",32),("state",6)]:
        name=f"{layer}_encoder.0.public_weight"
        for category,suffix in [("parameters",""),("first_moments","_first"),("second_moments","_second")]:
            result[category][name]=bits(saved["public"][layer+suffix]).reshape(64,width)
    return saved,result


def compare(actual,expected,absolute,relative):
    assert actual.shape==expected.shape
    assert torch.isfinite(actual).all() and torch.isfinite(expected).all()
    delta=(actual-expected).abs()
    assert torch.all(delta <= absolute+relative*expected.abs()), f"max delta {delta.max().item()}"
    return delta.max().item()


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("mode",choices=["prepare","analyze"])
    parser.add_argument("--root",type=Path,required=True)
    parser.add_argument("--samples",type=Path)
    parser.add_argument("--checkpoint",type=Path)
    parser.add_argument("--checkpoint-sha256")
    args=parser.parse_args()
    torch.set_num_threads(1)
    if args.mode=="prepare":
        assert sha(args.checkpoint)==args.checkpoint_sha256
        samples=json.loads(args.samples.read_bytes())["samples"]
        manifest=dict(schema="public-input-cuda-engineering/v1",gpu_ordinal=1,checkpoint=str(args.checkpoint),checkpoint_sha256=args.checkpoint_sha256,
            samples=str(args.samples),samples_sha256=sha(args.samples),model_identity=identity(),torch=torch.__version__,
            selected_indices=[(i//2)%len(s["actions"]) for i,s in enumerate(samples)],
            targets=[[-1.,0.,1.][(i//2)%3] for i in range(len(samples))],
            advantages=[[-.5,.25,.75][(i//2)%3] for i in range(len(samples))],
            gates=dict(forward=[1e-3,1e-3],gradient=[1e-4,1e-3],parameter_delta=[2e-6,.005],first_moments=[1e-5,1e-3],second_moments=[1e-7,1e-3],public_gradient_relative_l2=1e-3,public_update_relative_l2=.005),
            non_claim="Two synthetic GAE-loss updates on fixed visible fixtures; no rollout rewards, on-policy learning, candidate or strength result.")
        with (args.root/"manifest.json").open("x") as handle:json.dump(manifest,handle,indent=2)
        print("Prepared fixed CUDA engineering gates on GPU 1.")
        return
    manifest=json.loads((args.root/"manifest.json").read_bytes())
    assert manifest["model_identity"]==list(identity())
    samples_path=Path(manifest["samples"])
    assert sha(samples_path)==manifest["samples_sha256"]
    samples=json.loads(samples_path.read_bytes())["samples"]
    model,optimizer,source=import_checkpoint(manifest["checkpoint"],manifest["checkpoint_sha256"],True)
    encoded=[]
    for sample in samples:
        actions=[dict(schema_version=5,selected_index=i,stable_id=f"legal-action-v5:diagnostic-{i}",display_text=None,semantic=s) for i,s in enumerate(sample["actions"])]
        encoded.append(encode(sample["observation"],actions,True))
    native_gradients=json.loads((args.root/"gradients.json").read_bytes())
    gradients={r["name"]:bits(r["values"]).reshape(r["shape"]) for r in native_gradients["legacy"]}
    gradients["object_encoder.0.public_weight"]=bits(native_gradients["object"]).reshape(64,32)
    gradients["state_encoder.0.public_weight"]=bits(native_gradients["state"]).reshape(64,6)
    initial={name:p.detach().clone() for name,p in model.named_parameters()}
    gates=manifest["gates"]
    checks=[]
    for step in [1,2]:
        optimizer.zero_grad(set_to_none=True)
        logits=[];values=[];loss=[]
        for i,decision in enumerate(encoded):
            l,v=model(decision)
            logits.append(l);values.append(v)
            loss.append(-torch.log_softmax(l,dim=0)[manifest["selected_indices"][i]]*manifest["advantages"][i]+.5*(v-manifest["targets"][i]).square())
        torch.stack(loss).mean().backward()
        for name,p in model.named_parameters():
            if p.grad is None:p.grad=torch.zeros_like(p)
            if name=="scorer.2.bias":p.grad.zero_()
            if name=="card_embedding.weight":p.grad[0].zero_()
        if step==1:
            compare(torch.tensor(native_gradients["logits"]),torch.cat(logits).detach(),*gates["forward"])
            compare(torch.tensor(native_gradients["values"]),torch.stack(values).detach().reshape(-1),*gates["forward"])
            for name,p in model.named_parameters():
                delta=compare(gradients[name],p.grad,*gates["gradient"])
                if name in PUBLIC_NAMES:
                    assert p.grad.norm()>0 and gradients[name].norm()>0
                    assert ((gradients[name]-p.grad).norm()/p.grad.norm()).item()<=gates["public_gradient_relative_l2"],name
                checks.append(dict(step=step,tensor=name,category="gradient",max_delta=delta))
        optimizer.step()
        raw,actual=native_snapshot(args.root/("step1.json" if step==1 else "step2-uninterrupted.json"))
        assert raw["legacy_adam_step"]==source["adam_step"]+step and raw["public"]["adam_step"]==step
        for name,p in model.named_parameters():
            expected_delta=p.detach()-initial[name]
            actual_delta=actual["parameters"][name]-initial[name]
            delta=compare(actual_delta,expected_delta,*gates["parameter_delta"])
            if name in PUBLIC_NAMES:
                assert expected_delta.norm()>0 and actual_delta.norm()>0
                assert ((actual_delta-expected_delta).norm()/expected_delta.norm()).item()<=gates["public_update_relative_l2"],name
            checks.append(dict(step=step,tensor=name,category="parameter_delta",max_delta=delta))
            for category,key in [("first_moments","exp_avg"),("second_moments","exp_avg_sq")]:
                delta=compare(actual[category][name],optimizer.state[p][key],*gates[category])
                checks.append(dict(step=step,tensor=name,category=category,max_delta=delta))
        assert torch.equal(actual["parameters"]["scorer.2.bias"].view(torch.int32),initial["scorer.2.bias"].view(torch.int32))
        assert torch.equal(actual["parameters"]["card_embedding.weight"][0].view(torch.int32),initial["card_embedding.weight"][0].view(torch.int32))
    assert (args.root/"step2-uninterrupted.json").read_bytes()==(args.root/"step2-resumed.json").read_bytes()
    for filename in ["start-execution.json","resume-execution.json"]:
        receipt=json.loads((args.root/filename).read_bytes())
        assert receipt["status"]=="CUDA-ENGINEERING-EXECUTION-PASS" and receipt["manifest_sha256"]==sha(args.root/"manifest.json")
    result=dict(status="CUDA-GRADIENT-UPDATE-RESUME-ENGINEERING-PASS",gpu_ordinal=1,samples=len(samples),parameter_tensors=35,
        legacy_final_age=source["adam_step"]+2,public_final_age=2,fresh_process_resume_bit_exact=True,
        resume_sha256=sha(args.root/"step2-resumed.json"),checks=checks,non_claim=manifest["non_claim"])
    with (args.root/"qualification.json").open("x") as handle:json.dump(result,handle,indent=2)
    print(json.dumps({k:v for k,v in result.items() if k!="checks"},indent=2))

if __name__=="__main__":main()
