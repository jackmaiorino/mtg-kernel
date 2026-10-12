"""Export allowlisted fixed-perspective viewer recordings. Never execute a model."""
from __future__ import annotations
import argparse
import hashlib
import json
import struct
import subprocess
import time
import urllib.parse
import urllib.request
from pathlib import Path

SCHEMA = "mtg-kernel-viewer-recording/v1"
CAPTURE_SCHEMA = "mtg-kernel-viewer-capture/v1"
TOTAL = 1 << 64

def key(d: dict, actor: str | None = None) -> dict:
    return {"episode": d["episode_id"], "step": d["step"], "physical": d["physical_decision_id"],
            "substep": d["substep_index"], "actor": actor or d["acting_player"]}

def object_id(ref: dict) -> str:
    return f"o{ref['arena_id']}"

def references(value) -> list[str]:
    result = []
    if isinstance(value, dict):
        if "arena_id" in value:
            result.append(object_id(value))
        else:
            for child in value.values():
                result.extend(references(child))
    elif isinstance(value, list):
        for child in value:
            result.extend(references(child))
    return list(dict.fromkeys(result))

def public_ids(board: dict) -> set[str]:
    return {o['id'] for p in board['players'] for zone in ('battlefield','graveyard','hand') for o in p[zone]} | {o['id'] for o in board['exile']} | {s['source'] for s in board['stack'] if s['source']}

def digest_search(result: dict) -> str:
    stats = result["root_action_stats"]
    data = struct.pack("<Q", len(stats))
    for stat in stats:
        data += struct.pack("<IIqq", stat["flat_action_index"], stat["visits"], stat["value_sum"], stat["mean_value"])
    data += struct.pack("<IIII", result["selected_index"], result["transitions_used"], result["simulations"], result["tree_node_count"])
    return hashlib.sha256(data).hexdigest()

class Exporter:
    def __init__(self, names: dict, perspective="p0", cache: Path | None = None, assets: Path | None = None, art=False):
        self.names = names
        self.perspective = perspective
        self.cache = cache
        self.assets = assets
        self.art = art
        self.cards = {}

    def card(self, ref: dict, captured_name="", face=0) -> str:
        card_id = str(ref["card_db_id"])
        identifier = f"c{card_id}-f{face}"
        if identifier in self.cards:
            return identifier
        metadata = self.names.get(card_id, {})
        if isinstance(metadata, str):
            metadata = {"name": metadata}
        name = captured_name or metadata.get("name") or f"Card {card_id}"
        card = {"id": identifier, "name": name, "manaCost": metadata.get("mana_cost", ""),
                "type": metadata.get("type", ""), "rules": metadata.get("rules", ""),
                "image": None, "printing": None, "face": face}
        if self.art and self.cache and self.assets:
            self.cache.mkdir(parents=True, exist_ok=True)
            self.assets.mkdir(parents=True, exist_ok=True)
            slug = hashlib.sha256(name.encode()).hexdigest()[:20]
            saved = self.cache / f"{slug}.json"
            try:
                if saved.exists():
                    data = json.loads(saved.read_text(encoding="utf-8"))
                else:
                    lookup_name = name.removesuffix(' Token')
                    request = urllib.request.Request("https://api.scryfall.com/cards/named?" + urllib.parse.urlencode({"exact": lookup_name}),
                        headers={"User-Agent": "MTGKernelDecisionStudio/1.0", "Accept": "application/json"})
                    try:
                        with urllib.request.urlopen(request, timeout=20) as response:
                            data = json.load(response)
                    except urllib.error.URLError:
                        # Native curl uses the platform trust store, preserving TLS verification.
                        subprocess.run(['curl.exe' if __import__('os').name == 'nt' else 'curl','--silent','--show-error','--fail','--retry','2','--retry-delay','2','--max-time','20',request.full_url,'--output',str(saved)],check=True,capture_output=True,timeout=70)
                        data=json.loads(saved.read_text(encoding='utf-8'))
                    saved.write_text(json.dumps(data, ensure_ascii=False), encoding="utf-8")
                    time.sleep(.25)
                faces = data.get("card_faces", [])
                selected = faces[min(face, len(faces) - 1)] if faces else data
                card["manaCost"] = card["manaCost"] or selected.get("mana_cost", "")
                card["type"] = selected.get("type_line", card["type"])
                # Captured engine text takes precedence over supplemental oracle metadata.
                card["rules"] = card["rules"] or selected.get("oracle_text", "")
                card["printing"] = data["id"]
                image_url = selected.get("image_uris", data.get("image_uris", {})).get("normal")
                if image_url:
                    image_path = self.assets / f"{data['id']}-{face}.jpg"
                    if not image_path.exists():
                        if urllib.parse.urlparse(image_url).hostname != 'cards.scryfall.io':
                            raise ValueError('Unexpected image host')
                        try:
                            with urllib.request.urlopen(urllib.request.Request(image_url, headers={"User-Agent":"MTGKernelDecisionStudio/1.0"}), timeout=20) as response:
                                image_path.write_bytes(response.read(2 * 1024 * 1024))
                        except urllib.error.URLError:
                            subprocess.run(['curl.exe' if __import__('os').name == 'nt' else 'curl','--silent','--show-error','--fail','--max-time','20',image_url,'--output',str(image_path)],check=True,capture_output=True,timeout=25)
                    card["image"] = f"assets/{image_path.name}"
            except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
                print(f"Text-card fallback for {name}: {type(error).__name__}")
        self.cards[identifier] = card
        return identifier

    def obj(self, row: dict) -> dict:
        ref = row["stable"]
        traits = row.get("characteristics", {})
        return {"id": object_id(ref), "card": self.card(ref, row.get("card_name", ""), row.get("face_index", 0)),
                "owner": ref["owner"], "controller": ref["controller"], "tapped": row.get("tapped", False),
                "power": traits.get("effective_power"), "toughness": traits.get("effective_toughness"),
                "damage": row.get("damage", 0), "counters": row.get("counters", {}), "token": row.get("is_token", False)}

    def board(self, observation: dict) -> dict:
        p = observation["projection"]
        observer = observation["acting_player"]
        players = []
        for i in range(2):
            current = f"p{i}"
            hand = observation.get("own_hand", []) if observer == current == self.perspective else []
            players.append({"life": p["life_totals"][i], "handCount": p["hand_counts"][i], "libraryCount": p["library_counts"][i],
                "mana": p["mana_pools"][i], "battlefield": [self.obj(o) for o in p["battlefield"][i]],
                "graveyard": [self.obj(o) for o in p["graveyards"][i]], "hand": [self.obj(o) for o in hand]})
        exile = [self.obj(o) for o in p["exile"]]
        present = {o["id"] for pl in players for zone in ("battlefield", "graveyard", "hand") for o in pl[zone]} | {o["id"] for o in exile}
        stack = []
        for i, s in enumerate(p["stack"]):
            source = s.get("source", {})
            if isinstance(source, dict) and "arena_id" in source:
                card = self.cards[self.card(source, s.get("source_name", ""))]
                name = card["name"]
                source_ref = object_id(source)
            else:
                name = s.get("source_name") or s.get("card_name") or "Recorded spell or ability"
                source_ref = None
            stack.append({"id": f"stack-{i}", "name": name, "controller": s.get("controller", s.get("actor", p["active_player"])),
                "source": source_ref, "targets": [r for r in references(s.get("targets", [])) if r in present]})
        combat = p.get("combat", {})
        pairs = {object_id(a): [object_id(b) for b in bs if object_id(b) in present] for a, bs in combat.get("attacker_to_ordered_blockers", []) if object_id(a) in present}
        attackers = [object_id(a) for a in combat.get("ordered_attackers", []) if object_id(a) in present]
        return {"turn": p["turn"], "phase": p["phase"], "active": p["active_player"], "priority": p["priority_player"],
                "players": players, "exile": exile, "stack": stack,
                "combat": [{"attacker": a, "blockers": pairs.get(a, [])} for a in attackers]}

def terminal(value: dict | None) -> dict | None:
    if not value:
        return None
    classification = value.get("terminal_classification", value.get("classification", ""))
    if classification not in ("natural", "Natural"):
        raise ValueError("Only naturally completed games may be exported as terminal demos")
    winner = value.get("winner")
    if winner is None:
        outcome = value.get("terminal_outcome", value.get("outcome"))
        if isinstance(outcome, dict):
            winner = outcome.get("winner")
        elif outcome in ('p0_win', 'p1_win'):
            winner = outcome[:2]
    return {"kind": "Natural terminal", "winner": winner}

def export_capture(path: Path, exporter: Exporter) -> tuple[dict, list[dict], bool]:
    header, footer, frames = None, None, []
    count = 0
    with path.open("rb") as stream:
        for line in stream:
            if len(line) > 8 * 1024 * 1024 or not line.endswith(b"\n"):
                raise ValueError("Truncated or oversized capture record")
            row = json.loads(line)
            if row.get("schema") != CAPTURE_SCHEMA or footer is not None:
                raise ValueError("Invalid capture schema/order")
            if row["kind"] == "header" and header is None and not frames:
                header = row
                if row['perspective'] != exporter.perspective:
                    raise ValueError('Capture perspective differs from requested export')
                exporter.names = row["cards"]
            elif row["kind"] == "footer" and header is not None:
                footer = row
            elif row["kind"] == "decision" and header is not None:
                count += 1
                if count > 100000:
                    raise ValueError("Capture decision limit")
                before, after = exporter.board(row["before"]), exporter.board(row["after"])
                d = row["decision"]
                actor = d["acting_player"]
                present = public_ids(before) | public_ids(after)
                result = row["search"]["result"]
                if digest_search(result) != row["search"]["digest"]:
                    raise ValueError("Search digest mismatch")
                masses = row["policy"]["masses"]
                if len(masses) != len(row["actions"]) or sum(map(int, masses)) != TOTAL or any(int(n) < 0 for n in masses):
                    raise ValueError("Invalid captured policy distribution")
                featured = actor == exporter.perspective
                if featured:
                    actions = [{"index": i, "label": label, "kind": semantic["action_kind"],
                        "refs": [ref for ref in references(semantic) if ref in present]}
                        for i, (label, semantic) in enumerate(zip(row["labels"], row["actions"]))]
                    selected = row["selected"]
                    if result["selected_index"] != selected:
                        raise ValueError("Search selection differs from execution")
                    policy = {"kind":"hamilton_q64", "masses":masses, "sampled":row["policy"]["sampled"],
                        "value":row["policy"]["value"], "valueScale":"Raw model value; acting-player perspective, uncalibrated."}
                    search = {"digest":row["search"]["digest"], "simulations":result["simulations"], "transitions":result["transitions_used"],
                        "nodes":result["tree_node_count"], "tier":"T512", "stabilityHalves":False,
                        "valueScale":"Search leaf estimates use signed integers clipped to ±9,000; natural wins/losses use ±10,000. Values are from the root actor's perspective.",
                        "stats":[{"index":s["flat_action_index"],"visits":s["visits"],"valueSum":str(s["value_sum"]),"meanValue":str(s["mean_value"])} for s in result["root_action_stats"]]}
                else:
                    semantic = row["actions"][row["selected"]]
                    public_refs = [ref for ref in references(semantic) if ref in present]
                    label = semantic["action_kind"].replace("_", " ").capitalize()
                    revealed = [o for pl in after["players"] for zone in ("battlefield", "graveyard") for o in pl[zone] if o["id"] in public_refs]
                    if revealed:
                        label += ": " + ", ".join(dict.fromkeys(exporter.cards[o["card"]]["name"] for o in revealed))
                    actions = [{"index":0,"label":label,"kind":semantic["action_kind"],"refs":public_refs}]
                    selected, policy, search = 0, None, None
                if featured and row['search'].get('replay') is not None:
                    from viewer_search_export_v1 import export_replay
                    search['replay'] = export_replay(row['search']['replay'], exporter, key(d), actions, search)
                frames.append({"id":f"d{d['step']}","key":key(d),"board":before,"after":after,"actions":actions,
                    "executed":selected,"policy":policy,"search":search,"terminal":terminal(row["terminal"])})
            else:
                raise ValueError("Invalid capture order")
    if not header or not footer or footer["written"] != count or not footer["complete"]:
        raise ValueError("Capture is incomplete")
    return header, frames, bool(frames and frames[-1]["terminal"])

def export_legacy(path: Path, exporter: Exporter) -> tuple[dict, list[dict], bool]:
    from gameplay_trace_v1 import read_trace
    header, rows, _ = read_trace(path)
    frames = []
    for row in rows:
        # These snapshots belong to their actor, not a globally fixed observer.
        if row["actor"] != exporter.perspective:
            continue
        before, after = exporter.board(row["observation"]), exporter.board(row["transition"]["observation"])
        present = public_ids(before) | public_ids(after)
        nxt = row["transition"]["next"]
        frames.append({"id":f"d{row['decision']['step']}", "key":key(row["decision"], row["actor"]), "board":before,"after":after,
            "actions":[{"index":a["engine_index"],"label":a["label"],"kind":a["semantic"]["action_kind"],"refs":[r for r in references(a["semantic"]) if r in present]} for a in row["ordered_actions"]],
            "executed":row["selected_index"], "policy":{"kind":"hamilton_q64","masses":row["behavior"]["mass_numerators"],"sampled":row["selected_index"],"value":row["value"],"valueScale":"Raw model value; acting-player perspective, uncalibrated."},
            "search":None,"terminal":terminal(nxt) if nxt["kind"] == "terminal" else None})
    return header, frames, bool(frames and frames[-1]["terminal"])

def build_recording(path: Path, identifier: str, title: str, exporter: Exporter, bookmarks: list[dict]) -> dict:
    if path.stat().st_size > 512 * 1024 * 1024:
        raise ValueError("Input exceeds exporter bound")
    with path.open("rb") as stream:
        first = json.loads(stream.readline(8 * 1024 * 1024))
    native = first.get("schema") == CAPTURE_SCHEMA
    reconstructed = first.get('schema') == 'mtg-kernel-viewer-reconstruction/v1'
    if reconstructed:
        from viewer_reconstruction_export_v1 import export_reconstruction
        header, frames, complete = export_reconstruction(path, exporter)
    else:
        header, frames, complete = export_capture(path, exporter) if native else export_legacy(path, exporter)
    if not frames:
        raise ValueError("No decisions for the requested perspective")
    runtime = header.get("runtime", header.get("identity", {}).get("runtime", {}))
    identity = header.get("identity", {})
    checkpoint = (f"r1 {header['focal_checkpoint']} vs T1 {header['opponent_checkpoint']}" if reconstructed else
        identity.get("checkpoint_sha256") or identity.get("actual_model", {}).get("parameter_values_sha256", "historical-recorded-model"))
    decks = header.get("decks", ["Recorded deck", "Recorded deck"])
    version = 'mtg-kernel-viewer-recording/v3' if reconstructed else 'mtg-kernel-viewer-recording/v2' if any(f['search'] and f['search'].get('replay') for f in frames) else SCHEMA
    return {"schema":version,"id":identifier,"title":title,
        "description":("A selected qualification game: r1 Spy versus T1 Wildfire. A plain-policy opening leads to a persistent-search guide from turn 8. Search selected the self-mill and Giant reanimation choices. Reconstructed from recorded real actions; root tables and chooser identities were retained, individual simulations and policy distributions were not. This example is not representative of the refresh pilot or a playing-strength benchmark." if reconstructed else "Illustrative self-play with a frozen model. Recorded behavior is not a playing-strength benchmark." if native else "Historical actor-visible diagnostic recording."),
        "perspective":exporter.perspective,"complete":complete,"coverage":"every-step" if native or reconstructed else "decision-snapshots",
        "identity":{"source":runtime.get("engine_commit", "unknown"),"runtime":runtime.get("executable_sha256", "unknown"),
            "checkpoint":checkpoint,"cardDatabase":str(identity.get("model", {}).get("card_db_hash", "recorded registry")),
            "sourceSha256":hashlib.sha256(path.read_bytes()).hexdigest()},
        "seats":[{"name":"r1 / Search guide" if reconstructed else "Kernel / Agent 01","deck":decks[0]},{"name":"T1 / Policy" if reconstructed else "Kernel / Agent 02","deck":decks[1]}],
        "cards":exporter.cards,"frames":frames,"bookmarks":bookmarks}

def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("source", type=Path)
    ap.add_argument("output", type=Path)
    ap.add_argument("--id", required=True)
    ap.add_argument("--title", required=True)
    ap.add_argument("--perspective", choices=("p0","p1"), default="p0")
    ap.add_argument("--cards", type=Path, help="Legacy card ID-to-name mapping")
    ap.add_argument("--bookmarks", type=Path)
    ap.add_argument("--art", action="store_true")
    ap.add_argument("--cache", type=Path)
    args = ap.parse_args()
    names = json.loads(args.cards.read_text(encoding="utf-8")) if args.cards else {}
    bookmarks = json.loads(args.bookmarks.read_text(encoding="utf-8")) if args.bookmarks else []
    exporter = Exporter(names, args.perspective, args.cache, args.output.parent / "assets", args.art)
    recording = build_recording(args.source, args.id, args.title, exporter, bookmarks)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(recording, ensure_ascii=False, separators=(",",":")) + "\n", encoding="utf-8")
    print(json.dumps({"frames":len(recording["frames"]),"complete":recording["complete"],"sha256":hashlib.sha256(args.output.read_bytes()).hexdigest()}))

if __name__ == "__main__":
    main()
