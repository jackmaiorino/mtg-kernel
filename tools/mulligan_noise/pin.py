"""Per-deck pinning estimate: how much of each seat's outcome the opening land count fixes."""
import json, collections, random, sys
G, H, C, OUT = sys.argv[1:5]
is_land = [c["is_land"] for c in json.load(open(C))["cards"]]
rows = []
with open(G) as g, open(H) as h:
    for gl, hl in zip(g, h):
        a, b = json.loads(gl), json.loads(hl)
        rows.append((a["match_sha256"], [d.split("/")[0] for d in a["deck_ids"]],
                     [sum(is_land[c] for c in b["hands"][s]) for s in (0, 1)], a["winner"], a["starting_player"]))
D = ["Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "Terror", "CawGates", "Faeries"]

def compute(rs):
    cell = collections.defaultdict(lambda: [0, 0])     # (deck, lands) -> [wins, n]
    for _, decks, lands, w, _s in rs:
        for s in (0, 1):
            c = cell[(decks[s], lands[s])]; c[0] += w == s; c[1] += 1
    typ, pin = {}, {}
    for d in D:
        normal = [cell[(d, l)] for l in (2, 3, 4)]
        typ[d] = sum(x[0] for x in normal) / sum(x[1] for x in normal)
        for l in range(8):
            w, n = cell[(d, l)]
            # pinned share of this hand's games: the wins it loses relative to the deck's normal hands
            pin[(d, l)] = max(0.0, (typ[d] - w / n) / typ[d]) if n >= 200 else 0.0
    seats = 2 * len(rs)
    pin_seat = sum(pin[(decks[s], lands[s])] for _, decks, lands, _w, _s in rs for s in (0, 1)) / seats
    pin_game = sum(min(1.0, pin[(decks[0], lands[0])] + pin[(decks[1], lands[1])]) for _, decks, lands, _w, _s in rs) / len(rs)
    zero = sum(lands[s] == 0 and decks[s] != "Spy" for _, decks, lands, _w, _s in rs for s in (0, 1))
    zero_games = sum(any(lands[s] == 0 and decks[s] != "Spy" for s in (0, 1)) for _, decks, lands, _w, _s in rs)
    zero_pin = sum(pin[(decks[s], 0)] for _, decks, lands, _w, _s in rs for s in (0, 1) if lands[s] == 0 and decks[s] != "Spy") / seats
    return {"pin_seat": pin_seat, "pin_game": pin_game, "inflation": 1 / (1 - pin_game) ** 2,
            "zero_land_seat_share_nonSpy": zero / sum(decks[s] != "Spy" for _, decks, _l, _w, _s in rs for s in (0, 1)),
            "games_with_a_zero_land_seat": zero_games / len(rs), "pin_seat_from_zero_land": zero_pin,
            "typ": typ, "pin": {f"{d}|{l}": pin[(d, l)] for d in D for l in range(8) if cell[(d, l)][1]},
            "cell": {f"{d}|{l}": cell[(d, l)] for d in D for l in range(8) if cell[(d, l)][1]}}

point = compute(rows)
clusters = collections.defaultdict(list)
for r in rows: clusters[r[0]].append(r)
keys = list(clusters); rng = random.Random(7)
boots = [compute([r for k in (rng.choice(keys) for _ in keys) for r in clusters[k]]) for _ in range(100)]
ci = {k: (sorted(b[k] for b in boots)[2], sorted(b[k] for b in boots)[97]) for k in
      ("pin_seat", "pin_game", "inflation", "zero_land_seat_share_nonSpy", "games_with_a_zero_land_seat", "pin_seat_from_zero_land")}
# play/draw split for 1-land non-Spy hands
pd = collections.defaultdict(lambda: [0, 0])
for _, decks, lands, w, st in rows:
    for s in (0, 1):
        if decks[s] != "Spy" and lands[s] <= 2:
            k = (lands[s], "play" if st == s else "draw"); pd[k][0] += w == s; pd[k][1] += 1
point["play_draw_low_land"] = {f"{l}|{p}": v for (l, p), v in sorted(pd.items())}
point["ci"] = ci
json.dump(point, open(OUT, "w"), indent=1)
print({k: (round(point[k], 4), [round(x, 4) for x in ci[k]]) for k in ci})
print({d: round(v, 3) for d, v in point["typ"].items()})
print({k: (v[0] / v[1], v[1]) for k, v in point["play_draw_low_land"].items()})
