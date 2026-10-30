"""Extract per-game opening jobs from archived OH2 b512 panel match JSONs."""
import sys,json,tarfile,hashlib,glob,os
from compression import zstd
ARCH="E:/d-lane-moves-20261026/oh2-20261024/_archives"
EVAL="E:/d-lane-moves-20261026/oh2-20261024/eval"
out=open(sys.argv[1],'w')
stats={}
for f in sorted(glob.glob(f"{ARCH}/eval__r2-s*-b512-vs-*.tar.zst")):
    panel=os.path.basename(f)[len("eval__"):-len(".tar.zst")]
    rows={}
    for line in open(f"{EVAL}/{panel}/rows.jsonl"):
        r=json.loads(line); rows[r["match_sha256"]]=r
    n=g=skipped=0
    with zstd.open(f,'rb') as z, tarfile.open(fileobj=z,mode='r|') as t:
        for m in t:
            if not (m.isfile() and '/match-' in m.name and m.name.endswith('.json')): continue
            b=t.extractfile(m).read(); sha=hashlib.sha256(b).hexdigest()
            r=rows.get(sha)
            if r is None: skipped+=1; continue
            d=json.loads(b); n+=1
            regs={x["mainboard_sha256"]:x["mainboard"] for x in d["registrations"]}
            cfg=d["match"]["config"]
            for gi,game in enumerate(d["games"]):
                mbs=[regs.get(h) for h in game["mainboard_sha256"]]
                if any(x is None for x in mbs): continue
                g+=1
                out.write(json.dumps({"panel":panel,"match":m.name,"match_sha256":sha,"focal":r["seat"],
                  "own":r["own"],"other":r["other"],"repeat":r["repeat"],"deck_ids":cfg["deck_ids"],
                  "game_index":game["start"]["game_index"],"environment_seed":game["environment_seed"],
                  "starting_player":game["start"]["starting_player"],"mainboards":mbs,
                  "sideboarded":[game["mainboard_sha256"][i]!=d["registrations"][i]["mainboard_sha256"] for i in (0,1)],
                  "winner":game["winner"],"match_winner":d["outcome"]["winner"].get("winner"),
                  "max_physical_decisions":cfg["max_physical_decisions"],"max_policy_steps":cfg["max_policy_steps"]})+"\n")
    stats[panel]=(n,g,skipped,len(rows)); print(panel,stats[panel],flush=True)
