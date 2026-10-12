"""Target-reference identity audit (collab LANES/search-identity-audit-plan-20261011).

CLI:
  groups   --dump GROUPS.jsonl --out GROUPS.json [--keys KEYS.txt]
      Resolve every child's stack target references in its canonical actor-visible
      observation, classify the target descriptors, pick at most four witnesses per
      group (top two by visits, key-tied; then the highest-visited child of each of up to
      two previously unseen target-descriptor classes), and write their keys.
  witnesses --groups GROUPS.json --capture CAPTURE.jsonl --out WITNESSES.json
      For captured witnesses: raw-to-canonical target mapping, full canonical and raw
      differences between witness pairs, and the swap test (does exchanging the two
      differing target objects in one raw observation reproduce the other's canonical
      view, and does the canonicalizer's own output change under that exchange).

Analysis only: nothing here feeds search, keys or sampling.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
from pathlib import Path

# Lists the canonicalizer treats as unordered (stage4a/tree.rs UNORDERED_LISTS).
UNORDERED = ("own_hand", "known_hand_cards", "battlefield", "graveyards", "exile", "continuous_effects",
             "object_relations", "exile_play_permissions", "historical_public_sources", "cards")
HANDLE = ("arena_id", "zone_change_count", "attachments")


def sha(x) -> str:
    return hashlib.sha256(json.dumps(x, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def masked(v):
    """The canonicalizer's sort mask: handles and incarnation counters nulled."""
    if isinstance(v, dict):
        return {k: (None if k in HANDLE else masked(x)) for k, x in v.items()}
    if isinstance(v, list):
        return [masked(x) for x in v]
    return v


def occurrences(v, ident, path=""):
    """Every place an object handle `ident` appears: (path, the object dict or 'attachment')."""
    out = []
    if isinstance(v, dict):
        if v.get("arena_id") == ident:
            out.append((path, v))
        att = v.get("attachments")
        if isinstance(att, list) and ident in att:
            out.append((path + "/attachments", "attachment"))
        for k, x in v.items():
            out += occurrences(x, ident, f"{path}/{k}")
    elif isinstance(v, list):
        for i, x in enumerate(v):
            out += occurrences(x, ident, f"{path}/{i}")
    return out


def role(path: str) -> str:
    parts = path.split("/")
    if "stack" in parts:
        return "stack_target" if "targets" in parts else "stack_source"
    for r in ("battlefield", "graveyards", "exile", "own_hand", "object_relations", "historical_public_sources",
              "continuous_effects", "exile_play_permissions", "combat", "known_hand_cards", "extensions"):
        if r in parts:
            return r
    return parts[1] if len(parts) > 1 else "root"


def entry(obs, path):
    """The list element enclosing an occurrence (a battlefield entry, a stack target slot,
    a relation...): the nearest ancestor reached through a list index."""
    parts = path.split("/")[1:]
    last = max((i for i, s in enumerate(parts) if s.isdigit()), default=None)
    v = obs
    for s in parts[: (last + 1) if last is not None else len(parts)]:
        v = v[int(s)] if isinstance(v, list) else v[s]
    return v


def descriptor(obs, ident):
    """Everything the actor sees about one object, with handles removed: each
    occurrence's role and the masked content of its enclosing entry (characteristics,
    tapped state, damage, counters, attachments, zone/incarnation offset). Two objects
    with equal descriptors are visibly indistinguishable apart from their handles and
    list positions."""
    occ = occurrences(obs, ident)
    return sorted((role(p), json.dumps(masked(entry(obs, p)), sort_keys=True)) for p, _ in occ)


def stack_targets(obs):
    out = []
    for i, item in enumerate(obs["projection"]["stack"]):
        for j, t in enumerate(item.get("targets", [])):
            if isinstance(t, dict) and isinstance(t.get("object"), dict):
                out.append((i, j, t["object"]["arena_id"]))
    return out


def child_summary(c):
    obs = c["obs"]
    targets = stack_targets(obs)
    descs = []
    for i, j, ident in targets:
        d = descriptor(obs, ident)
        # Resolution: the referenced object must appear somewhere besides the target slot.
        resolved = any(r != "stack_target" for r, _ in d)
        descs.append({"stack_index": i, "target_slot": j, "canonical_id": ident, "resolved": resolved,
                      "occurrence_roles": sorted({r for r, _ in d}),
                      "descriptor_sha256": sha(d)})
    cls = sha([(x["stack_index"], x["target_slot"], x["descriptor_sha256"]) for x in descs])
    return {"key": c["key"], "visits": c["visits"], "obs_sha256": c["obs_sha256"], "menu_sha256": c["menu_sha256"],
            "created_sim": c["created_sim"], "targets": descs, "target_class": cls,
            "unresolved": any(not x["resolved"] for x in descs)}


def diff_paths(a, b, path=""):
    if a == b:
        return []
    if isinstance(a, dict) and isinstance(b, dict):
        out = []
        for k in sorted(set(a) | set(b)):
            out += diff_paths(a.get(k), b.get(k), f"{path}/{k}")
        return out
    if isinstance(a, list) and isinstance(b, list) and len(a) == len(b):
        out = []
        for i, (x, y) in enumerate(zip(a, b)):
            out += diff_paths(x, y, f"{path}/{i}")
        return out
    return [path]


def bound_index(records, key):
    indexed = {}
    for record in records:
        binding = key(record)
        if binding in indexed and indexed[binding] != record:
            raise ValueError(f"conflicting duplicate identity binding {binding}")
        indexed[binding] = record
    return indexed


def group_binding(group, default_root=None):
    root = group.get("root_id", default_root)
    if root is None:
        raise ValueError("group lacks an unambiguous root binding")
    return root, group["parent"], json.dumps(group["edge"], sort_keys=True)


def groups(dump: Path, out: Path, keys_out: Path | None):
    rows = [json.loads(l) for l in dump.read_text(encoding="utf-8").splitlines() if l.strip()]
    gs = [r for r in rows if r["kind"] == "group"]
    gs = list(bound_index(gs, group_binding).values())
    result, keys = [], []
    for g in gs:
        kids = [child_summary(c) for c in g["children"]]  # sorted by visits desc, key
        by_key = {c["key"]: c for c in g["children"]}
        classes = {}
        for k in kids:
            classes.setdefault(k["target_class"], []).append(k["key"])
        # Witnesses: top two, then up to two previously unseen classes (highest-visited, key-tied).
        top = kids[:2]
        if not top:
            raise ValueError("identity group has no children")
        seen = {k["target_class"] for k in top}
        extra = []
        for k in kids[2:]:
            if len(extra) == 2:
                break
            if k["target_class"] not in seen:
                seen.add(k["target_class"])
                extra.append(k)
        wit = top + extra
        keys += [w["key"] for w in wit]
        a = by_key[top[0]["key"]]
        b = by_key[top[1]["key"]] if len(top) == 2 else None
        pair = diff_paths(a["obs"], b["obs"]) if b else None
        # Pairwise: for each child, does it differ from the top child only in stack target ids?
        only_target = 0
        for c in g["children"][1:]:
            d = diff_paths(a["obs"], c["obs"])
            if d and all("/stack/" in p and "/targets/" in p and p.endswith("arena_id") for p in d):
                only_target += 1
        n = len(kids)
        target_children = [k for k in kids if k["targets"]]
        result.append({
            "root_id": g["root_id"], "rank": g["rank"], "parent": g["parent"], "edge": g["edge"], "parent_physical_depth": g["parent_physical_depth"],
            "children": n, "children_visits": sum(k["visits"] for k in kids),
            "children_with_object_targets": len(target_children),
            "children_without_object_targets": n - len(target_children),
            "children_with_unresolved_targets": sum(k["unresolved"] for k in kids),
            "target_descriptor_classes": len(classes),
            "class_sizes": sorted((len(v) for v in classes.values()), reverse=True),
            "top_pair": {"keys": [top[0]["key"], top[1]["key"]], "visits": [top[0]["visits"], top[1]["visits"]],
                         "full_canonical_diff_paths": pair, "menus_equal": a["edges"] == b["edges"],
                         "same_target_class": top[0]["target_class"] == top[1]["target_class"]} if b else None,
            "children_differing_from_top_only_in_target_ids": only_target,
            "witnesses": [{"key": w["key"], "visits": w["visits"], "target_class": w["target_class"],
                           "created_sim": w["created_sim"], "targets": w["targets"]} for w in wit],
            "per_child": kids})
    out.write_text(json.dumps({"schema": "identity-audit-groups-v1", "groups": result}, indent=1) + "\n")
    if keys_out:
        keys_out.write_text("".join(k + "\n" for k in keys))
    return result


def copies(obs, ident):
    """Canonical handles whose descriptor equals `ident`'s once stack-target occurrences
    are ignored (visibly identical objects), including `ident`."""
    def core(i):
        return [d for d in descriptor(obs, i) if d[0] != "stack_target"]
    want = core(ident)
    ids = set()

    def walk(v):
        if isinstance(v, dict):
            if isinstance(v.get("arena_id"), int):
                ids.add(v["arena_id"])
            for x in v.values():
                walk(x)
        elif isinstance(v, list):
            for x in v:
                walk(x)
    walk(obs)
    return sorted(i for i in ids if core(i) == want)


def candidate_key(child):
    """Offline candidate grouping (diagnostic only): every stack target slot that names
    one of several visibly identical copies is rewritten to the smallest such handle."""
    obs = copy.deepcopy(child["obs"])
    for i, j, ident in stack_targets(child["obs"]):
        obs["projection"]["stack"][i]["targets"][j]["object"]["arena_id"] = copies(child["obs"], ident)[0]
    return sha([obs, child["edges"]])


def get(v, path):
    for s in path.split("/")[1:]:
        v = v[int(s)] if isinstance(v, list) else v[s]
    return v


def witnesses(groups_path: Path, dump: Path, capture: Path, probe: Path, out: Path):
    gs = json.loads(groups_path.read_text())["groups"]
    raw_groups = [g for g in
                  (json.loads(l) for l in dump.read_text(encoding="utf-8").splitlines() if l.strip())
                  if g["kind"] == "group"]
    canon = bound_index(raw_groups, group_binding)
    roots = {g["root_id"] for g in raw_groups}
    default_root = next(iter(roots)) if len(roots) == 1 else None
    bound_index(gs, lambda g: group_binding(g, default_root))
    rows = [json.loads(l) for l in capture.read_text(encoding="utf-8").splitlines() if l.strip()]
    caps = bound_index((r for r in rows if r["kind"] == "capture"), lambda r: (r["root_id"], r["key"]))
    probe_rows = [json.loads(l) for l in probe.read_text(encoding="utf-8").splitlines() if l.strip()] if probe else rows
    probes = bound_index((r for r in rows + probe_rows if r["kind"] == "probe"),
                         lambda r: (r["root_id"], r["a"], r["b"]))
    report = []
    for g in gs:
        binding = group_binding(g, default_root)
        root = binding[0]
        kids = {c["key"]: c for c in canon[binding]["children"]}
        top = g["witnesses"][0]
        a_c = kids[top["key"]]
        ws = []
        for w in g["witnesses"]:
            c, cap = kids[w["key"]], caps.get((root, w["key"]))
            paths = diff_paths(a_c["obs"], c["obs"])
            tdesc = [{"canonical_id": t["canonical_id"],
                      "raw_id": cap["mapping"]["order"][t["canonical_id"]] if cap else None,
                      "descriptor": descriptor(c["obs"], t["canonical_id"]),
                      "visibly_identical_copies": copies(c["obs"], t["canonical_id"])}
                     for t in w["targets"]]
            # History above the node: the two exchanged handles in every captured ancestor view.
            ws.append({"key": w["key"], "visits": w["visits"], "created_sim": w["created_sim"],
                       "target_class": w["target_class"], "targets": tdesc,
                       "canonical_diff_from_top": {"paths": paths, "count": len(paths), "truncated": False},
                       "menu_equal_to_top": c["edges"] == a_c["edges"],
                       "capture": None if cap is None else {
                           "sim": cap["sim"], "step": cap["step"], "physical_decision_id": cap["physical_decision_id"],
                           "substep": cap["substep"], "physical_depth": cap["physical_depth"],
                           "mapping_order": cap["mapping"]["order"], "min_zcc": cap["mapping"]["min_zcc"],
                           "chain": cap["chain"], "raw_obs_sha256": sha(cap["raw_obs"]),
                           "raw_menu_sha256": sha(cap["raw_menu"])}})
        # The top pair: exchange evidence and visible history.
        b = g["witnesses"][1] if len(g["witnesses"]) > 1 else None
        probe_record = probes.get((root, top["key"], b["key"])) if b else None
        pr = probe_record["result"] if probe_record else None
        hist = []
        if pr and "x" in pr:
            x, y = pr["x"], pr["y"]
            for h in caps[(root, top["key"])]["chain"]:
                anc = caps.get((root, h["key"]))
                if anc is None:
                    hist.append({"key": h["key"], "captured": False})
                    continue
                ro = anc["raw_obs"]
                ex = [sorted((role(p), json.dumps(masked(entry(ro, p)), sort_keys=True)) for p, _ in occurrences(ro, i))
                      for i in (x, y)]
                i = anc["canon_menu"].index(h["edge"]) if h["edge"] in anc["canon_menu"] else None
                raw_edge = anc["raw_menu"][i] if i is not None else None
                hist.append({"key": h["key"], "captured": True, "step": anc["step"],
                             "physical_depth": anc["physical_depth"],
                             "x_y_present": [bool(ex[0]), bool(ex[1])], "x_y_visibly_identical": ex[0] == ex[1],
                             "edge_mapped_to_raw": raw_edge is not None,
                             "edge_names_x_or_y": bool(raw_edge and (occurrences(raw_edge, x) or occurrences(raw_edge, y)))})
        # Offline candidate grouping over every child of the group.
        classes = {}
        for c in kids.values():
            classes.setdefault(candidate_key(c), []).append(c)
        merged = [v for v in classes.values() if len(v) > 1]
        report.append({
            "root_id": root, "rank": g["rank"], "parent": g["parent"], "edge": g["edge"], "children": g["children"],
            "denominators": {"children": g["children"], "with_object_targets": g["children_with_object_targets"],
                             "unresolved_targets": g["children_with_unresolved_targets"],
                             "target_descriptor_classes": g["target_descriptor_classes"],
                             "class_sizes": g["class_sizes"]},
            "witnesses": ws, "top_pair_probe": pr, "top_pair_history": hist,
            "candidate_grouping": {"note": "offline diagnostic only; not calibrated values, beliefs or strength",
                                   "classes": len(classes), "merged_classes": len(merged),
                                   "children_in_merged_classes": sum(len(v) for v in merged),
                                   "visits_in_merged_classes": sum(c["visits"] for v in merged for c in v),
                                   "merged": [{"keys": [c["key"] for c in v], "visits": [c["visits"] for c in v]}
                                              for v in merged]}})
    out.write_text(json.dumps({"schema": "identity-audit-witnesses-v1", "groups": report}, indent=1) + "\n")
    return report


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    g = sub.add_parser("groups")
    g.add_argument("--dump", type=Path, required=True)
    g.add_argument("--out", type=Path, required=True)
    g.add_argument("--keys", type=Path)
    w = sub.add_parser("witnesses")
    w.add_argument("--groups", type=Path, required=True)
    w.add_argument("--dump", type=Path, required=True)
    w.add_argument("--capture", type=Path, required=True)
    w.add_argument("--probe", type=Path)
    w.add_argument("--out", type=Path, required=True)
    args = ap.parse_args(argv)
    if args.op == "witnesses":
        for r in witnesses(args.groups, args.dump, args.capture, args.probe, args.out):
            print(json.dumps({"rank": r["rank"], "denominators": r["denominators"],
                              "candidate": {k: v for k, v in r["candidate_grouping"].items() if k != "merged"},
                              "history": r["top_pair_history"]})[:2000])
            for x in r["witnesses"]:
                print("  ", x["key"][:12], x["visits"], [(t["canonical_id"], t["raw_id"], t["visibly_identical_copies"])
                                                       for t in x["targets"]],
                      x["canonical_diff_from_top"]["count"], x["canonical_diff_from_top"]["paths"][:6])
        return 0
    if args.op == "groups":
        for r in groups(args.dump, args.out, args.keys):
            print(json.dumps({k: r[k] for k in ("rank", "children", "children_with_object_targets",
                                                  "children_with_unresolved_targets", "target_descriptor_classes",
                                                  "class_sizes", "top_pair", "children_differing_from_top_only_in_target_ids")})[:1500])
            print(" witnesses", [(w["key"][:12], w["visits"], w["target_class"][:8]) for w in r["witnesses"]])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
