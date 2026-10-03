"""Separately versioned public auxiliary inputs, not a replacement V4 encoder."""
import hashlib
import json
from pathlib import Path
import re

SCHEMA = "mtg-kernel-structured-public-features/v1"
COLORS = "WUBRGC"
PAIRS = [(a,b) for a in range(6) for b in range(a+1,6)]
REPO = Path(__file__).resolve().parents[2]
REGISTRY = REPO / "data/cards_v1.json"
CONTRACT = REPO / "data/public_cost_features_v1/contract.json"


def catalog():
    """Parse the printed registry independently of Rust's compiled Cost/Pip."""
    rows, names = [], []
    for card in json.loads(REGISTRY.read_text(encoding="utf-8"))["cards"]:
        cost = card["mana_cost"]
        symbols = re.findall(r"\{([^{}]+)\}", cost)
        if "".join("{"+s+"}" for s in symbols) != cost:
            raise ValueError("unsupported printed cost syntax")
        row = [0.0] * 32
        row[0], row[1] = 1.0, float(bool(cost))
        for symbol in symbols:
            if symbol.isdecimal():
                row[2] += int(symbol) / 16.0
                row[3] += int(symbol) / 16.0
            elif symbol == "X":
                row[4] += 0.25
            elif symbol in COLORS:
                row[2] += 1/16
                row[5+COLORS.index(symbol)] += 0.125
            elif len(symbol) == 3 and symbol[1:] == "/P" and symbol[0] in COLORS:
                row[2] += 1/16
                row[11+COLORS.index(symbol[0])] += 0.125
            elif len(symbol) == 3 and symbol[1] == "/" and symbol[0] in COLORS and symbol[2] in COLORS:
                pair = tuple(sorted([COLORS.index(symbol[0]),COLORS.index(symbol[2])]))
                row[2] += 1/16
                row[17+PAIRS.index(pair)] += 0.125
            else:
                raise ValueError(f"unsupported printed mana symbol: {symbol}")
        names.append(card["name"])
        rows.append(row)
    return dict(schema=SCHEMA, registry_sha256=hashlib.sha256(REGISTRY.read_bytes()).hexdigest(),
        contract_sha256=hashlib.sha256(CONTRACT.read_bytes()).hexdigest(), card_names=names, rows=rows)


def from_actor_v4(observation, object_card_tokens):
    """Use object tokens from the same validated features_v7 encoded decision."""
    contract = json.loads(CONTRACT.read_text(encoding="utf-8"))
    if type(observation["card_db_hash"]) is not int or f"{observation['card_db_hash']:016x}" != contract["card_db_hash"]:
        raise ValueError("observation card registry differs")
    values = catalog()
    state = [0.0] * 6
    for effect in observation["projection"]["continuous_effects"]:
        mask = effect["prevent_damage_from_color_mask"]
        cannot = effect["damage_cannot_be_prevented"]
        if type(mask) is not int or not 0 <= mask <= 31:
            raise ValueError("invalid public prevention color mask")
        if (mask or cannot) and not effect["global"]:
            raise ValueError("scoped prevention is outside the global feature contract")
        for bit in range(5):
            state[bit] = max(state[bit], float(bool(mask & (1 << bit))))
        state[5] = max(state[5], float(cannot))
    rows = []
    for token in object_card_tokens:
        if type(token) is not int or not 0 <= token <= len(values["rows"]):
            raise ValueError("invalid visible card token")
        rows.append(values["rows"][token-1].copy() if token else [0.0]*32)
    return dict(schema=SCHEMA, registry_sha256=values["registry_sha256"],
        contract_sha256=values["contract_sha256"], state=state, objects=rows)
