"""Aggregate Pauper decklists per archetype, diff against the kernel registry, and
estimate per-card implementation effort from XMage sources.

Input: a directory of decklist text files named <archetype>__<deckid>.txt in MTGO
export form ("4 Lightning Bolt" lines, a blank line or "Sideboard" separating the
sideboard). Output: JSON + a markdown table.
"""
import json, os, re, sys, glob, collections

ROOT = r"C:\Users\Jack\IdeaProjects\mtg-kernel"
XMAGE_CARDS = r"C:\Users\Jack\IdeaProjects\mage-cycle4-lead\Mage.Sets\src\mage\cards"
DECK_DIR = sys.argv[1]
SHARES = json.loads(sys.argv[2]) if len(sys.argv) > 2 else {}  # archetype -> meta share percent
OUT = sys.argv[3] if len(sys.argv) > 3 else os.path.join(DECK_DIR, "meta_gap.json")

registry = {c["name"] for c in json.load(open(os.path.join(ROOT, "data", "cards_v1.json")))["cards"]}

LINE = re.compile(r"^\s*(\d+)\s+(.+?)\s*$")

def parse(path):
    main, side = collections.Counter(), collections.Counter()
    target = main
    for raw in open(path, encoding="utf-8", errors="replace"):
        line = raw.strip()
        if not line:
            target = side  # MTGO export: blank line separates sideboard
            continue
        if line.lower().startswith("sideboard"):
            target = side
            continue
        m = LINE.match(line)
        if not m:
            continue
        count, name = int(m.group(1)), m.group(2).split(" // ")[0].strip()
        target[name] += count
    return main, side

def java_path(name):
    letter = name[0].lower()
    fname = re.sub(r"[^A-Za-z0-9]", "", name.title().replace("'", "")) + ".java"
    # XMage names classes by removing punctuation and capitalising words
    words = re.sub(r"[^A-Za-z0-9 ]", " ", name).split()
    cls = "".join(w[:1].upper() + w[1:] for w in words) + ".java"
    for cand in (os.path.join(XMAGE_CARDS, letter, cls), os.path.join(XMAGE_CARDS, letter, fname)):
        if os.path.exists(cand):
            return cand
    hits = glob.glob(os.path.join(XMAGE_CARDS, letter, cls[:6] + "*.java"))
    for h in hits:
        if os.path.basename(h).lower() == cls.lower():
            return h
    return None

def java_complexity(path):
    if not path:
        return None
    src = open(path, encoding="utf-8", errors="replace").read()
    lines = src.count("\n")
    features = []
    for key in ("Trigger", "Static", "Activated", "ContinuousEffect", "Target", "Condition", "Cost", "Token", "Counter", "Kicker", "Flashback", "Cycling", "Madness", "Ninjutsu", "Landfall", "Affinity"):
        if key in src:
            features.append(key.lower())
    return {"java_lines": lines, "features": features}

archetypes = collections.defaultdict(lambda: {"decks": 0, "main": collections.Counter(), "side": collections.Counter(), "main_decks": collections.Counter(), "side_decks": collections.Counter()})
for path in sorted(glob.glob(os.path.join(DECK_DIR, "*.txt"))):
    arch = os.path.basename(path).split("__")[0]
    main, side = parse(path)
    if not main:
        continue
    a = archetypes[arch]
    a["decks"] += 1
    for n, c in main.items():
        a["main"][n] += c
        a["main_decks"][n] += 1
    for n, c in side.items():
        a["side"][n] += c
        a["side_decks"][n] += 1

report = {"archetypes": {}, "missing_cards": {}}
missing = collections.defaultdict(lambda: {"archetypes": {}, "weight": 0.0, "main_weight": 0.0})
for arch, a in archetypes.items():
    d = a["decks"]
    share = float(SHARES.get(arch, 0.0))
    rows = []
    for name, copies in a["main"].most_common():
        avg = copies / d
        pct = a["main_decks"][name] / d
        rows.append({"card": name, "avg_main": round(avg, 2), "deck_pct": round(pct, 2), "in_registry": name in registry})
        if name not in registry:
            missing[name]["archetypes"][arch] = {"avg_main": round(avg, 2), "deck_pct": round(pct, 2)}
            missing[name]["weight"] += share * avg * pct
            missing[name]["main_weight"] += share * avg * pct
    for name, copies in a["side"].most_common():
        avg = copies / d
        pct = a["side_decks"][name] / d
        if name not in registry:
            missing[name]["archetypes"].setdefault(arch, {})["avg_side"] = round(avg, 2)
            missing[name]["weight"] += 0.5 * share * avg * pct
    report["archetypes"][arch] = {"decks": d, "share": share, "cards": rows, "sideboard": [{"card": n, "avg_side": round(c / d, 2), "in_registry": n in registry} for n, c in a["side"].most_common()]}

for name, info in missing.items():
    jp = java_path(name)
    info["xmage_java"] = os.path.relpath(jp, XMAGE_CARDS) if jp else None
    info["complexity"] = java_complexity(jp)
report["missing_cards"] = dict(sorted(missing.items(), key=lambda kv: -kv[1]["weight"]))
json.dump(report, open(OUT, "w"), indent=1)

print(f"archetypes: {len(archetypes)}; missing cards: {len(missing)}")
print("| rank | card | weight | archetypes (avg main copies) | xmage lines | features |")
print("|---|---|---|---|---|---|")
for i, (name, info) in enumerate(report["missing_cards"].items(), 1):
    archs = ", ".join(f"{a} {v.get('avg_main', 0)}" for a, v in info["archetypes"].items())
    cx = info["complexity"] or {}
    print(f"| {i} | {name} | {info['weight']:.1f} | {archs} | {cx.get('java_lines','?')} | {' '.join(cx.get('features', []))} |")
    if i >= 80:
        break
