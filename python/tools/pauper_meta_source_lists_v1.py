"""Pick, per archetype, the sampled decklist closest to the archetype's average list,
and list the registry-missing cards in that exact 75 with XMage presence."""
import json, os, glob, re, collections, sys
os.environ.setdefault("MTG_KERNEL_XMAGE_CARDS", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "mage-cycle4-lead", "Mage.Sets", "src", "mage", "cards"))
ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
DECKS = os.path.join(ROOT, "docs", "research", "pauper_meta_decklists_2026-09-09")
registry = {c["name"] for c in json.load(open(os.path.join(ROOT, "data", "cards_v1.json")))["cards"]}
LINE = re.compile(r"^\s*(\d+)\s+(.+?)\s*$")

def parse(path):
    main, side = collections.Counter(), collections.Counter()
    t = main
    for raw in open(path, encoding="utf-8", errors="replace"):
        line = raw.strip()
        if not line or line.lower().startswith("sideboard"):
            t = side; continue
        m = LINE.match(line)
        if m: t[m.group(2).split(" // ")[0].strip()] += int(m.group(1))
    return main, side

# reuse java_path logic inline (avoid running the module's main)
XC = os.environ["MTG_KERNEL_XMAGE_CARDS"]
BASICS = os.path.normpath(os.path.join(XC, "..", "..", "..", "..", "Mage", "src", "main", "java", "mage", "cards", "basiclands"))
def java_path(name):
    if name in {"Plains","Island","Swamp","Mountain","Forest"}:
        return os.path.join(BASICS, name + ".java")
    words = re.sub(r"[^A-Za-z0-9 ]", " ", name).split()
    cls = "".join(w[:1].upper() + w[1:] for w in words) + ".java"
    exact = os.path.join(XC, name[0].lower(), cls)
    if os.path.exists(exact): return exact
    for h in glob.glob(os.path.join(XC, name[0].lower(), cls[:6] + "*.java")):
        if os.path.basename(h).lower() == cls.lower(): return h
    return None

by_arch = collections.defaultdict(list)
for p in sorted(glob.glob(os.path.join(DECKS, "*.txt"))):
    arch, did = os.path.basename(p)[:-4].split("__")
    by_arch[arch].append((did, *parse(p)))

out = {}
for arch, decks in sorted(by_arch.items()):
    n = len(decks)
    avg = collections.Counter()
    for _, m, s in decks:
        for k, v in m.items(): avg[k] += v / n
        for k, v in s.items(): avg["SB:" + k] += v / n
    best = None
    for did, m, s in decks:
        vec = collections.Counter(m); vec.update({"SB:" + k: v for k, v in s.items()})
        dist = sum(abs(vec[k] - avg[k]) for k in set(vec) | set(avg))
        # penalise registry-missing and xmage-absent cards lightly so ties favour implementable lists
        absent = [k for k in list(m) + list(s) if k not in registry and not java_path(k)]
        score = dist + 2 * len(absent)
        if best is None or score < best[0]: best = (score, did, m, s, absent, dist)
    score, did, m, s, absent, dist = best
    missing_main = sorted(k for k in m if k not in registry)
    missing_side = sorted(k for k in s if k not in registry)
    out[arch] = {"deck_id": did, "distance": round(dist, 1), "main_total": sum(m.values()), "side_total": sum(s.values()),
                 "missing_main": {k: m[k] for k in missing_main}, "missing_side": {k: s[k] for k in missing_side},
                 "xmage_absent": absent}
    print(f"{arch}: mtgtop8 {did} dist {dist:.1f} main {sum(m.values())} side {sum(s.values())}; missing main {len(missing_main)} side {len(missing_side)}; xmage-absent {absent}")
    print("   main:", ", ".join(f"{k} x{m[k]}" for k in missing_main))
    print("   side:", ", ".join(f"{k} x{s[k]}" for k in missing_side))
json.dump(out, open(os.path.join(ROOT, "docs", "research", "pauper_meta_source_lists_2026-09-09.json"), "w"), indent=1)
