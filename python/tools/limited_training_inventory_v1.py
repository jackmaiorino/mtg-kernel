"""Audit the original DraftZero deck sizes and declared Full-only preflight."""

from collections import Counter
import csv
import hashlib
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SAMPLE = ROOT / "data/limited/draftzero_sample_v1"
REGISTRIES = (ROOT / "data/cards_v1.json", ROOT / "data/limited/fdn_v1/cards_v1.json")


def inventory() -> dict:
    definitions = {}
    registry_hashes = {}
    for path in REGISTRIES:
        data = path.read_bytes()
        registry_hashes[path.relative_to(ROOT).as_posix()] = hashlib.sha256(data).hexdigest()
        for card in json.loads(data)["cards"]:
            definitions[card["name"]] = card

    rows = []
    splits = Counter()
    all_inputs = hashlib.sha256()
    pool = SAMPLE / "pool.tsv"
    for row in csv.DictReader(pool.read_text(encoding="utf-8").splitlines(), delimiter="\t"):
        name, split = row["deck"], row["split"]
        path = SAMPLE / "decks" / f"{name}.json"
        data = path.read_bytes()
        deck = json.loads(data)
        size = sum(card["count"] for card in deck["cards"])
        if deck["name"] != name or size != int(row["cards"]):
            raise ValueError(f"DraftZero registration differs for {name}")
        missing, partial = set(), set()
        for card in deck["cards"]:
            card_name = card["name"]
            definition = definitions.get(card_name)
            if definition is None:
                missing.add(card_name)
            elif definition.get("engine_capability", "full") != "full" or definition.get("is_token", False):
                partial.add(card_name)
        eligible = not missing and not partial
        rows.append({"deck": name, "split": split, "cards": size, "sha256": hashlib.sha256(data).hexdigest(),
                     "declared_full_only": eligible, "unknown_printings": sorted(missing),
                     "non_full_printings": sorted(partial),
                     "contains_drake_hatcher": any(card["name"] == "Drake Hatcher" for card in deck["cards"])})
        splits[split, size, eligible] += 1
        all_inputs.update(path.name.encode("utf-8"))
        all_inputs.update(data)
    all_inputs.update(pool.name.encode("utf-8"))
    all_inputs.update(pool.read_bytes())
    return {"schema": "limited-training-deck-inventory/v1", "sample_sha256_in_pool_order": all_inputs.hexdigest(),
            "registry_sha256": registry_hashes, "model_coverage": "requires native actor runs",
            "counts": [{"split": split, "cards": size, "declared_full_only": eligible, "decks": count}
                       for (split, size, eligible), count in sorted(splits.items())],
            "decks": rows}


if __name__ == "__main__":
    print(json.dumps(inventory(), indent=2))
