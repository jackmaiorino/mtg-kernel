"""Forced-keep noise from OH2 r2 b512 panel games (opening hands dealt by
opening_hand_census_v1). Pure Python. Writes analysis.json."""
import json, sys, random, math, collections
from math import comb

GAMES, HANDS, CARDS, OUT = sys.argv[1:5]
cards = json.load(open(CARDS))["cards"]
is_land = [c["is_land"] for c in cards]
DECKS = ["Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "Terror", "CawGates", "Faeries"]
LANDS = {"Wildfire": 20, "Rally": 18, "Affinity": 19, "Elves": 13, "Spy": 4, "Burn": 18, "Terror": 16, "CawGates": 21, "Faeries": 18}

def hyper(L, k, N=60, n=7):
    return comb(L, k) * comb(N - L, n - k) / comb(N, n)

def bad(deck, lands):
    return deck != "Spy" and (lands <= 1 or lands >= 6)

rows = []  # one per game
with open(GAMES) as g, open(HANDS) as h:
    for i, (gl, hl) in enumerate(zip(g, h)):
        game, hand = json.loads(gl), json.loads(hl)
        assert hand["line"] == i
        decks = [d.split("/")[0] for d in game["deck_ids"]]
        lands = [sum(is_land[c] for c in hand["hands"][s]) for s in (0, 1)]
        assert all(len(x) == 7 for x in hand["hands"])
        rows.append({"cluster": game["match_sha256"], "panel": game["panel"], "decks": decks,
                     "lands": lands, "focal": game["focal"], "winner": game["winner"],
                     "start": game["starting_player"], "game_index": game["game_index"]})

n = len(rows)
res = {"games": n, "decks": {}, "hyper": {}}
# 1. Hypergeometric and observed land distributions per deck (all seats).
obs = collections.defaultdict(lambda: [0] * 8)
for r in rows:
    for s in (0, 1):
        obs[r["decks"][s]][r["lands"][s]] += 1
for d in DECKS:
    L = LANDS[d]
    tot = sum(obs[d])
    res["hyper"][d] = {
        "lands": L, "p0": hyper(L, 0), "p01": hyper(L, 0) + hyper(L, 1),
        "p67": hyper(L, 6) + hyper(L, 7),
        "observed_p01": (obs[d][0] + obs[d][1]) / tot, "observed_p67": (obs[d][6] + obs[d][7]) / tot,
        "hands": tot, "observed_by_lands": obs[d],
    }

def winrate(sel):
    w = [x for x in sel]
    return (sum(w) / len(w), len(w)) if w else (None, 0)

# 2. Seat-level: did the holder of the hand win? (both seats, all games)
by_l = collections.defaultdict(list)       # (deck, lands) -> outcomes for that seat
cls = collections.defaultdict(list)        # 'bad'/'ok' per seat -> outcomes
focal_cls = collections.defaultdict(list)
pair = collections.defaultdict(list)       # (bad_self, bad_opp) -> outcome for self, seat 0 view only once per game
for r in rows:
    for s in (0, 1):
        won = int(r["winner"] == s)
        d, l = r["decks"][s], r["lands"][s]
        by_l[(d, l)].append(won)
        b = bad(d, l)
        cls["bad" if b else "ok"].append(won)
        if s == r["focal"]:
            focal_cls["bad" if b else "ok"].append(won)
    b0, b1 = bad(r["decks"][0], r["lands"][0]), bad(r["decks"][1], r["lands"][1])
    pair[(b0, b1)].append(int(r["winner"] == 0))

res["seat_winrate"] = {k: winrate(v) for k, v in cls.items()}
res["focal_winrate"] = {k: winrate(v) for k, v in focal_cls.items()}
res["by_deck_lands"] = {f"{d}|{l}": winrate(by_l[(d, l)]) for d in DECKS for l in range(8) if by_l[(d, l)]}
one_sided = [o for (b0, b1), v in pair.items() if b0 != b1 for o in (v if b0 else [1 - x for x in v])]
res["one_sided_bad_player_winrate"] = winrate(one_sided)
res["game_classes"] = {f"{int(b0)}{int(b1)}": len(v) for (b0, b1), v in pair.items()}
p_any = sum(len(v) for (b0, b1), v in pair.items() if b0 or b1) / n
p_one = sum(len(v) for (b0, b1), v in pair.items() if b0 != b1) / n
res["share_games_any_bad"] = p_any
res["share_games_one_sided_bad"] = p_one

# 3. Cluster bootstrap by match for the key numbers.
clusters = collections.defaultdict(list)
for r in rows:
    clusters[r["cluster"]].append(r)
keys = list(clusters)
rng = random.Random(20261030)

def stats(sample_rows):
    bad_w = ok_w = bad_n = ok_n = one_n = one_w = any_n = 0
    for r in sample_rows:
        b = [bad(r["decks"][s], r["lands"][s]) for s in (0, 1)]
        any_n += b[0] or b[1]
        for s in (0, 1):
            won = r["winner"] == s
            if b[s]:
                bad_n += 1; bad_w += won
            else:
                ok_n += 1; ok_w += won
        if b[0] != b[1]:
            one_n += 1; one_w += r["winner"] == (0 if b[0] else 1)
    w = one_w / one_n
    pin = 1 - 2 * w
    f = one_n / len(sample_rows)
    return {"bad_wr": bad_w / bad_n, "ok_wr": ok_w / ok_n, "one_sided_wr": w,
            "share_any_bad": any_n / len(sample_rows), "share_one_sided": f,
            "pinned_share": f * pin, "games_inflation": 1 / (1 - f * pin) ** 2}

point = stats(rows)
boots = []
for _ in range(200):
    sample = [r for k in (rng.choice(keys) for _ in keys) for r in clusters[k]]
    boots.append(stats(sample))
res["key"] = {k: {"point": point[k],
                  "lo": sorted(b[k] for b in boots)[4], "hi": sorted(b[k] for b in boots)[195]}
              for k in point}
# 4. Expected share of seats dealt a bad seven under the hypergeometric, over the panel's deck mix.
exp_bad = sum(res["hyper"][d]["p01"] + res["hyper"][d]["p67"] for d in DECKS if d != "Spy")
res["expected_bad_seat_share_nonSpy_uniform"] = exp_bad / 8
json.dump(res, open(OUT, "w"), indent=1)
print(json.dumps(res["key"], indent=1))
print("seat", res["seat_winrate"], "focal", res["focal_winrate"], "classes", res["game_classes"])
