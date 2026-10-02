"""Read-only audit of exposed traces; no native execution or outcome selection."""
import collections
import hashlib
import json
import pathlib
import sys

previous = pathlib.Path(sys.argv[1])
out = pathlib.Path(sys.argv[2])
out.mkdir(exist_ok=False, parents=True)
cards = json.loads((pathlib.Path(__file__).resolve().parents[2] / "data/cards_v1.json").read_text(encoding="utf-8"))["cards"]
names = [c["name"] for c in cards]
strands = names.index("Prismatic Strands")
color_bits = {color: 1 << index for index, color in enumerate("WUBRG")}
rows, pins = [], []
for arm in ["g115", "control", "broader"]:
    for seat in [0, 1]:
        path = previous / "outputs" / f"case0-{arm}-seat{seat}.json"
        pins.append(dict(path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
        payload = json.loads(path.read_text(encoding="utf-8"))
        for game in payload["collected"]["trajectory"]["games"]:
            assert game["terminal"]["classification"] == "natural"
            known_colors = collections.defaultdict(set)
            for record in game["decisions"]:
                visible = record["visible"]
                if visible["kind"] != "gameplay":
                    continue
                observation = visible["observation"]
                p = observation["projection"]
                scope = (p["turn"], p["active_player"])
                actions = visible["ordered_actions"]
                selected = record["behavior"]["selected_index"]
                action = actions[selected]
                if action.get("source", {}).get("card_db_id") != strands or action["action_kind"] not in ["cast_spell", "choose_effect_color"]:
                    continue
                assert record["actor"] == f"p{seat}"
                masses = [int(value) for value in record["behavior"]["mass_numerators"]]
                assert sum(masses) == 2**64
                public_mask = 0
                for effect in p["continuous_effects"]:
                    public_mask |= effect["prevent_damage_from_color_mask"]
                def creatures(index):
                    return [dict(name=names[c["stable"]["card_db_id"]], colors=c["characteristics"]["effective_color_mask"],
                        power=c["characteristics"]["effective_power"], tapped=c["tapped"])
                        for c in p["battlefield"][index] if c["characteristics"]["type_flags"]["creature"]]
                row = dict(arm=arm, seat=seat, game=game["start"]["game_index"], decision=record["decision_index"],
                    turn=p["turn"], active_player=p["active_player"], phase=p["phase"], action=action,
                    selected_mass=masses[selected] / 2**64, prior_colors_same_turn=sorted(known_colors[scope]),
                    visible_prevention_mask=public_mask, pass_legal=any(a["action_kind"] == "pass" for a in actions),
                    pass_mass=sum(m for a,m in zip(actions,masses) if a["action_kind"] == "pass") / 2**64,
                    own_creatures=creatures(seat), opponent_creatures=creatures(1-seat),
                    stack=[dict(kind=s.get("stack_item_kind"), controller=s["controller"],
                        source_name=names[s["source"]["card_db_id"]] if s.get("source") else None) for s in p["stack"]])
                if action["action_kind"] == "choose_effect_color":
                    row["duplicate_color_this_turn"] = action["color"] in known_colors[scope]
                    row["duplicate_color_mass"] = sum(m for a,m in zip(actions,masses)
                        if a["action_kind"] == "choose_effect_color" and a["color"] in known_colors[scope]) / 2**64
                    known_colors[scope].add(action["color"])
                rows.append(row)

summary = []
for arm in ["g115", "control", "broader"]:
    selected = [r for r in rows if r["arm"] == arm]
    casts = [r for r in selected if r["action"]["action_kind"] == "cast_spell"]
    colors = [r for r in selected if r["action"]["action_kind"] == "choose_effect_color"]
    prior = [r for r in casts if r["prior_colors_same_turn"]]
    summary.append(dict(arm=arm, cast_selections=len(casts), flashback_selections=sum(r["action"]["source"]["zone"] == "Graveyard" for r in casts),
        casts_own_main=sum(r["active_player"] == f"p{r['seat']}" and r["phase"] in ["main1", "main2"] for r in casts),
        casts_with_prior_shield=len(prior), prior_shield_casts_with_empty_visible_mask=sum(r["visible_prevention_mask"] == 0 for r in prior),
        duplicate_color_choices=sum(r["duplicate_color_this_turn"] for r in colors),
        prior_shield_cast_mean_mass=sum(r["selected_mass"] for r in prior)/len(prior) if prior else None,
        all_casts_offer_pass=all(r["pass_legal"] for r in casts)))
result = dict(schema="strands-visibility-trace-audit/v1", source_pins=pins, summary=summary, decisions=rows,
    scope="All physical games from the six existing Gates/Affinity traces; selected exposed development cases, not prevalence or strength evidence.",
    inference="Prior colors are reconstructed from earlier resolved color choices within (game,turn,active_player), not read from hidden runtime state. Engine install/expiry semantics and separate live reproduction substantiate that reconstruction.",
    non_claim="Same-color prevention shields are redundant, but spell-cast triggers, tap costs and future alternatives mean this is not a proof that every flashback action is strictly dominated.")
with (out / "audit.json").open("x", encoding="utf-8") as handle:
    json.dump(result, handle, indent=2)
print(json.dumps(summary, indent=2))
