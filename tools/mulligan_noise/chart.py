import json
from math import comb
curve = json.load(open("curve.json"))
LANDS = {"Wildfire": 20, "Rally": 18, "Affinity": 19, "Elves": 13, "Spy": 4, "Burn": 18, "Terror": 16, "CawGates": 21, "Faeries": 18}
h = lambda L, k: comb(L, k) * comb(60 - L, 7 - k) / comb(60, 7)
W, H = 900, 420
o = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}" font-family="system-ui,-apple-system,Segoe UI,sans-serif">',
     """<style>
.bg{fill:#fcfcfb}.t1{fill:#0b0b0b}.t2{fill:#52514e}.grid{stroke:#e4e3df}.ref{stroke:#8f8e88}
.s1{stroke:#2a78d6;fill:#2a78d6}.s2{stroke:#eb6834;fill:#eb6834}.ring{stroke:#fcfcfb}
@media (prefers-color-scheme: dark){.bg{fill:#1a1a19}.t1{fill:#f0efec}.t2{fill:#c3c2b7}.grid{stroke:#353533}.ref{stroke:#7d7c76}
.s1{stroke:#3987e5;fill:#3987e5}.s2{stroke:#d95926;fill:#d95926}.ring{stroke:#1a1a19}}
text{font-size:12px}.h{font-size:14px;font-weight:600}
</style>""", f'<rect class="bg" width="{W}" height="{H}"/>']
# Panel A
x0, y0, pw, ph = 60, 60, 340, 270
o.append(f'<text class="t1 h" x="{x0-40}" y="24">Win rate of a seven by opening lands</text>')
o.append(f'<text class="t2" x="{x0-40}" y="42">Relative to the same deck with 2-4 lands; 8 decks, Spy excluded; 95% CI</text>')
ylo, yhi = 0.3, 1.1
Y = lambda v: y0 + ph - (v - ylo) / (yhi - ylo) * ph
X = lambda l: x0 + 20 + l * (pw - 40) / 5
for v in (0.4, 0.6, 0.8, 1.0):
    o.append(f'<line class="{"ref" if v == 1.0 else "grid"}" x1="{x0}" x2="{x0+pw}" y1="{Y(v):.1f}" y2="{Y(v):.1f}" stroke-width="1" {"stroke-dasharray=\"4 3\"" if v==1.0 else ""}/>')
    o.append(f'<text class="t2" x="{x0-8}" y="{Y(v)+4:.1f}" text-anchor="end">{int(v*100)}%</text>')
for l in range(6):
    o.append(f'<text class="t2" x="{X(l):.1f}" y="{y0+ph+18}" text-anchor="middle">{l}</text>')
o.append(f'<text class="t2" x="{x0+pw/2}" y="{y0+ph+36}" text-anchor="middle">Lands in the opening seven</text>')
for pd, cls, dx in (("play", "s1", -4), ("draw", "s2", 4)):
    pts = [(l, *curve[f"{pd}|{l}"][:2]) for l in range(6)]
    o.append(f'<polyline class="{cls}" style="fill:none" stroke-width="2" points="{" ".join(f"{X(l)+dx:.1f},{Y(v):.1f}" for l, v, _ in pts)}"/>')
    for l, v, e in pts:
        o.append(f'<line class="{cls}" x1="{X(l)+dx:.1f}" x2="{X(l)+dx:.1f}" y1="{Y(v-e):.1f}" y2="{Y(v+e):.1f}" stroke-width="1.5"/>')
        o.append(f'<circle class="{cls} ring" cx="{X(l)+dx:.1f}" cy="{Y(v):.1f}" r="4.5" stroke-width="2" style="fill:inherit"/>')
        o[-1] = f'<circle cx="{X(l)+dx:.1f}" cy="{Y(v):.1f}" r="5" class="ring" style="fill:none" stroke-width="2"/><circle class="{cls}" cx="{X(l)+dx:.1f}" cy="{Y(v):.1f}" r="4"/>'
    v0 = curve[f"{pd}|0"][0]
    o.append(f'<text class="t1" x="{X(0)+12:.1f}" y="{Y(v0)+4:.1f}">{v0*100:.0f}% on the {pd}</text>')
# legend A
o.append(f'<circle class="s1" cx="{x0+190}" cy="{y0+ph-58}" r="4"/><text class="t2" x="{x0+198}" y="{y0+ph-54}">On the play</text>')
o.append(f'<circle class="s2" cx="{x0+190}" cy="{y0+ph-40}" r="4"/><text class="t2" x="{x0+198}" y="{y0+ph-36}">On the draw</text>')
# Panel B
bx, by, bw = 600, 60, 250
o.append(f'<text class="t1 h" x="470" y="24">Odds of a 0- or 1-land seven</text>')
o.append(f'<text class="t2" x="470" y="42">Hypergeometric, 60-card deck (lands in parentheses)</text>')
decks = sorted(LANDS, key=lambda d: -(h(LANDS[d], 0) + h(LANDS[d], 1)))
row = 30
Xb = lambda p: bx + p * bw
for i, d in enumerate(decks):
    yy = by + 6 + i * row
    p0, p1 = h(LANDS[d], 0), h(LANDS[d], 1)
    o.append(f'<text class="t1" x="{bx-8}" y="{yy+14}" text-anchor="end">{d} ({LANDS[d]})</text>')
    o.append(f'<rect class="s1" x="{bx}" y="{yy}" width="{p0*bw:.1f}" height="18" rx="2"/>')
    o.append(f'<rect class="s2" x="{bx+p0*bw+2:.1f}" y="{yy}" width="{max(0,p1*bw-2):.1f}" height="18" rx="2"/>')
    o.append(f'<text class="t2" x="{Xb(p0+p1)+6:.1f}" y="{yy+14}">{(p0+p1)*100:.0f}%</text>')
ly = by + 6 + 9 * row + 8
o.append(f'<rect class="s1" x="{bx}" y="{ly}" width="10" height="10" rx="2"/><text class="t2" x="{bx+16}" y="{ly+9}">0 lands</text>')
o.append(f'<rect class="s2" x="{bx+80}" y="{ly}" width="10" height="10" rx="2"/><text class="t2" x="{bx+96}" y="{ly+9}">1 land</text>')
o.append(f'<text class="t2" x="470" y="{ly+30}">Spy runs 4 lands on purpose; its keeps hinge on combo pieces, not lands.</text>')
o.append(f'<text class="t2" x="20" y="{H-10}">Source: 177,381 OH2 r2 b512 panel games (8 seeds x 5 opponents), opening hands re-dealt from the recorded seeds.</text>')
o.append("</svg>")
open("forced_keep_noise_v1.svg", "w").write("\n".join(o))
