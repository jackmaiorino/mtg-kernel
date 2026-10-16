"""Offline source/domain audit; optionally verify the original Wizards input."""

import argparse
import hashlib
import json
from pathlib import Path
import re


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rules-file", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    raw = (root / "data/rules/creature_types_v60.json").read_bytes()
    domain = json.loads(raw)
    entries = domain["types"]
    names = [entry["name"] for entry in entries]
    assert len(entries) == len(set(names)) == 324
    assert [entry["id"] for entry in entries] == list(range(324))
    assert names[-1] == "Time Lord"
    assert {name for name in names if " " in name} == {"Time Lord"}
    source = (root / "mtg-kernel/src/creature_types_v60.rs").read_text(encoding="utf-8")
    digest = hashlib.sha256(raw).hexdigest()
    assert re.search(r'DOMAIN_SHA256_V60: &str =\s*"([0-9a-f]+)"', source)[1] == digest
    generated = re.findall(r'^    (\w+) = (\d+) => "([^"]+)",$', source, re.M)
    assert [(int(index), name) for _, index, name in generated] == [
        (entry["id"], entry["name"]) for entry in entries
    ]
    native_source = (root / "mtg-kernel/src/card_def.rs").read_text(encoding="utf-8")
    native = native_source.split("pub enum Subtype {", 1)[1].split("\n}", 1)[0]
    variants = re.findall(r"^    (\w+),$", native, re.M)
    assert len(variants) == 154
    id_map = re.findall(r"^            (\d+) => Subtype::(\w+),$", native_source, re.M)
    assert id_map == [(str(index), name) for index, name in enumerate(variants)]
    mapping = re.findall(r"^        Subtype::(\w+) => (None|Some\(CreatureTypeV60::\w+\)),?$", source, re.M)
    assert [name for name, _ in mapping] == variants
    legal_variants = {variant for variant, _, _ in generated}
    mapped = [value.removeprefix("Some(CreatureTypeV60::").removesuffix(")")
              for _, value in mapping if value != "None"]
    assert set(mapped) <= legal_variants
    assert len(mapped) == 121 and len(set(mapped)) == 115
    assert sum(value == "None" for _, value in mapping) == 33
    aliases = dict(mapping)
    classification = re.findall(r"^            Subtype::(\w+) => (true|false),$", native_source, re.M)
    assert [name for name, _ in classification] == variants
    assert all((value == "true") == (aliases[name] != "None") for name, value in classification)
    assert "Subtype::from_stable_id(id)" in source
    for name in ("Bird", "Faerie", "Human", "Ninja", "Rogue", "Wizard"):
        assert aliases[name] == aliases[name + "AllCaps"]
    if args.rules_file:
        rules_raw = args.rules_file.read_bytes()
        assert len(rules_raw) == domain["rules_bytes"]
        assert hashlib.sha256(rules_raw).hexdigest() == domain["rules_sha256"]
        clause = next(line for line in rules_raw.decode("utf-8-sig").splitlines()
                      if line.startswith("205.3m "))
        assert hashlib.sha256(clause.encode("utf-8")).hexdigest() == domain["rule_utf8_sha256"]
        assert "One creature type is two words long: Time Lord." in clause
        official = clause.split("All other creature types are one word long: ", 1)[1]
        official = official.removesuffix(".").replace(", and ", ", ").split(", ")
        assert official + ["Time Lord"] == names
    print(f"PASS: 324 legal types, 154 native IDs, 33 noncreature types, 6 aliases; domain {digest}")


if __name__ == "__main__":
    main()
