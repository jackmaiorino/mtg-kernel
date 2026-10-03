"""Research successor with separate public-input projections and exact warm start.

This is not yet admitted to the native trainer/checkpoint format. Its two new
matrices have their own Adam age, while all imported parameters retain theirs.
"""
from dataclasses import dataclass, replace
import hashlib
import json
from pathlib import Path
import numpy as np
import torch
from torch import nn
from torch.nn import functional as F

from . import features_v7 as base_features
from .model import KernelPolicyValueNet, ModelConfig
from .public_cost_features_v1 import CONTRACT, catalog, from_actor_v4

PUBLIC_NAMES = {"object_encoder.0.public_weight", "state_encoder.0.public_weight"}


def identity():
    descriptor = dict(schema="mtg-kernel-public-feature-model/v1",
        base_feature_contract=base_features.feature_contract_fingerprint(),
        base_feature_encoding=base_features.encoding_contract_fingerprint(),
        auxiliary_contract=catalog()["contract_sha256"], registry=catalog()["registry_sha256"],
        implementation=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        dimensions=dict(state=225, object=130),
        forward="legacy matmul plus separate zero-initialized public matmul before tanh",
        optimizer="imported per-parameter Adam age retained; two new matrices age zero")
    digest = hashlib.sha256(json.dumps(descriptor, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    return descriptor, digest


def config_fields(structured):
    values = ModelConfig().to_dict()
    values.update(model_architecture_version="native-net8-python-reference-v4/v1",
        feature_schema_version=base_features.FEATURE_SCHEMA_VERSION,
        feature_registry_version=base_features.FEATURE_REGISTRY_VERSION,
        feature_contract_digest=base_features.feature_contract_fingerprint(),
        feature_encoding_digest=base_features.encoding_contract_fingerprint())
    if structured:
        _, digest = identity()
        values.update(model_architecture_version="native-net8-public-input-projections/v1",
            feature_schema_version="actor-relative-public-inputs/v1",
            feature_registry_version="public-cost-prevention/v1", feature_contract_digest=digest,
            feature_encoding_digest=digest, state_dim=225, object_feature_dim=130)
    return values


@dataclass(frozen=True)
class ResearchConfig(ModelConfig):
    structured: bool = False

    def validate(self):
        if self.to_dict() != dict(config_fields(self.structured), structured=self.structured):
            raise ValueError("research config differs from explicit contract")


class SplitInputLinear(nn.Module):
    def __init__(self, legacy_width, output_width, offset, added_width):
        super().__init__()
        self.offset, self.added_width = offset, added_width
        self.weight = nn.Parameter(torch.zeros(output_width, legacy_width))
        self.bias = nn.Parameter(torch.zeros(output_width))
        self.public_weight = nn.Parameter(torch.zeros(output_width, added_width))

    def forward(self, value):
        legacy = torch.cat([value[..., :self.offset], value[..., self.offset+self.added_width:]], dim=-1)
        public = value[..., self.offset:self.offset+self.added_width]
        return F.linear(legacy, self.weight, self.bias) + F.linear(public, self.public_weight)


def model(structured):
    config = ResearchConfig(**config_fields(structured), structured=structured)
    result = KernelPolicyValueNet(config)
    if structured:
        result.object_encoder[0] = SplitInputLinear(114, 64, 98, 32)
        result.state_encoder[0] = SplitInputLinear(1499, 64, 219, 6)
    return result


def encode(observation, actions, structured):
    base = base_features.encode_decision(observation, actions)
    if not structured:
        return base
    auxiliary = from_actor_v4(observation, base.object_card_ids.tolist())
    fields = config_fields(True)
    schema = replace(base.schema, version=fields["feature_schema_version"],
        registry_version=fields["feature_registry_version"], contract_digest=fields["feature_contract_digest"],
        encoding_digest=fields["feature_encoding_digest"], state_dim=225, object_feature_dim=130)
    return replace(base, schema=schema,
        state=torch.cat([base.state, torch.tensor(auxiliary["state"], dtype=torch.float32)]),
        object_features=torch.cat([base.object_features, torch.tensor(auxiliary["objects"], dtype=torch.float32)], dim=-1))


def bit_tensor(record):
    bits = np.asarray(record["values"], dtype=np.uint32)
    value = torch.from_numpy(bits.view(np.float32).copy()).reshape(record["shape"])
    if not torch.isfinite(value).all():
        raise ValueError("nonfinite checkpoint tensor")
    return value


def import_checkpoint(path, expected_sha256, structured):
    raw = Path(path).read_bytes()
    if hashlib.sha256(raw).hexdigest() != expected_sha256:
        raise ValueError("checkpoint SHA differs")
    checkpoint = json.loads(raw)
    if checkpoint["feature_contract_digest"] != base_features.feature_contract_fingerprint() or checkpoint["feature_encoding_digest"] != base_features.encoding_contract_fingerprint():
        raise ValueError("checkpoint is not the V4 source contract")
    if checkpoint["card_db_hash"] != json.loads(CONTRACT.read_text())["card_db_hash"]:
        raise ValueError("checkpoint card registry differs")
    result = model(structured)
    parameters = {row["name"]: bit_tensor(row) for row in checkpoint["parameters"]}
    if len(parameters) != len(checkpoint["parameters"]):
        raise ValueError("duplicate checkpoint parameter")
    missing, unexpected = result.load_state_dict(parameters, strict=False)
    if set(missing) != (PUBLIC_NAMES if structured else set()) or unexpected:
        raise ValueError("checkpoint parameter layout mismatch")
    scalar = lambda key: np.asarray([checkpoint[key]], dtype=np.uint32).view(np.float32).item()
    optimizer = torch.optim.Adam(result.parameters(), lr=scalar("learning_rate_bits"), betas=(0.9,0.999), eps=1e-8, foreach=False, fused=False)
    first = {r["name"]: bit_tensor(r) for r in checkpoint["first_moments"]}
    second = {r["name"]: bit_tensor(r) for r in checkpoint["second_moments"]}
    if set(first) != set(parameters) or set(second) != set(parameters):
        raise ValueError("optimizer tensor layout differs")
    for name, parameter in result.named_parameters():
        if name in PUBLIC_NAMES:
            age, m, v = 0, torch.zeros_like(parameter), torch.zeros_like(parameter)
        else:
            age, m, v = checkpoint["adam_step"], first[name], second[name]
            if m.shape != parameter.shape or v.shape != parameter.shape or (v < 0).any():
                raise ValueError("invalid optimizer tensor")
        optimizer.state[parameter] = dict(step=torch.tensor(float(age)), exp_avg=m.clone(), exp_avg_sq=v.clone())
    return result, optimizer, checkpoint


def engineering_step(result, optimizer, encoded):
    """Synthetic gradient-connectivity probe, never a rollout reward or learning run."""
    optimizer.zero_grad(set_to_none=True)
    logits, value = result(encoded)
    loss = -torch.log_softmax(logits, dim=0)[0] + 0.25 * value.square()
    loss.backward()
    for name, parameter in result.named_parameters():
        if parameter.grad is None:
            parameter.grad = torch.zeros_like(parameter)
        # Preserve the native policy-logit gauge and embedding padding row.
        if name == "scorer.2.bias": parameter.grad.zero_()
        if name == "card_embedding.weight": parameter.grad[0].zero_()
    norms = {name: float(parameter.grad.abs().sum()) for name,parameter in result.named_parameters() if name in PUBLIC_NAMES}
    optimizer.step()
    return norms
