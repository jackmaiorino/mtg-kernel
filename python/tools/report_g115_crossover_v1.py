"""Render completed crossover and retained-embedding evidence."""
import argparse
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import numpy as np
from prepare_postboard_qualification_v1 import read,write,pin


def report(root):
    a=read(root/'analysis.json'); assert a['complete'] and a['new_matches']==224
    exposure_path=Path('E:/mtg-postboard-campaign-20260920/embedding-exposure-audit-001/audit.json')
    exposure=read(exposure_path); s=a['summary']['g1']
    data=np.array([[s['additional_with_g115'],s['g115_on_canonical']],
                   [s['additional_with_a48'],1-s['additional_with_g115']]])*100
    fig,ax=plt.subplots(figsize=(7.5,3.8),constrained_layout=True)
    ax.imshow(data,vmin=0,vmax=100,cmap='Blues',aspect='auto')
    ax.set_xticks([0,1],['Piloting additional lists','Piloting canonical lists'])
    ax.set_yticks([0,1],['g115','A48'])
    ax.set_title('Same list pairs and seeds, reciprocal model assignments',loc='left',fontweight='bold',fontsize=12)
    for i in range(2):
        for j in range(2):
            ax.text(j,i,f'{data[i,j]:.1f}%\n{round(data[i,j]/100*224)}/224 G1 wins',ha='center',va='center',
                    color='white' if data[i,j]>60 else '#132238',fontsize=13)
    fig.savefig(root/'crossover.png',dpi=160);plt.close(fig)
    lines=['# Reciprocal deck-assignment result','',
        'All16newjobs/224newBO3 completed, with506naturalgames and no draws. The analysis pairs them with224 immutable matches from opponent-profile-002. No model was trained or promoted.','',
        '| Model | Piloting additional lists | Piloting canonical lists |',
        '| --- | ---: | ---: |',
        f"| g115 | {data[0,0]:.1f}% (117/224) | {data[0,1]:.1f}% (167/224) |",
        f"| A48 | {data[1,0]:.1f}% (57/224) | {data[1,1]:.1f}% (107/224) |",'',
        '![Crossover](crossover.png)','',
        'Balanced across both deck assignments, g115 scores63.4% againstA48, paired95% interval60.0-66.7%. Balanced across both models, the additional lists score38.8%, interval33.9-43.8%. g115 retains a relative advantage, while the additional-list side is harder for both pilots. This does not isolate intrinsic deck quality from shared policy blind spots, model/deck interactions or engine errors.','',
        'Secondary Keep-sideboard BO3 balancedg115 score66.96%, interval63.39-70.54%; balanced additional-list score39.29%, interval34.82-43.75%. PrimaryG1 and secondaryBO3 estimates retain both physical seats and assignments in112environment-case pairs. Bootstrap10000resamples within each additional-list stratum, seed202609201809. Earlier anchors are reused explicitly; this is not an independent replication.','',
        '## Retained card-embedding audit','',
        'Both g115 andA48 have90of192registry card embedding rows bit-identical to their own fresh initialization, with both Adam moments exactly positive zero. This is a retained-update observation, not proof a card was never seen; shared layers and semantic features can generalize. Control rows for Lightning Bolt, Counterspell and Mountain changed and retain nonzero moments.','',
        '| Additional list identity | Mainboard copies whose card embedding has no retained update | Distinct such cards |',
        '| --- | ---: | ---: |']
    for row in exposure['per_additional_list']:
        x=row['models']['g115']['mainboard']; assert x==row['models']['a48']['mainboard']
        lines.append(f"| {row['zoned_sha256'][:12]} | {x['unchanged_copies']}/60 | {x['unchanged_distinct_cards']}/{x['distinct_cards']} |")
    lines += ['',
        'List44ae includes Basilisk Gate, Journey to Nowhere and Guardian of the Guildpact;44of60mainboard copies have unchanged card rows. List0997 includes Balustrade Spy, Dread Return and Lotleth Giant;37of60copies do. All exact card IDs, token=id+1mapping, names, initial payload/checkpoint/source hashes and per-row checks are recorded in the separate embedding-exposure-audit-001/audit.json.','',
        'Next: qualify a small matched card-exposure continuation from untouchedg115, replacing half the existing-list learner episodes with balanced episodes from all eight additional lists. Keep seeds, opponent policies, terminal reward, optimizer and initial full Adam state matched to an existing-list-only control. First verify natural collection, actual updates to previously unchanged card rows, exact restart/replay and cost. Only after this engineering qualification should a separate bounded200-update comparison be frozen and launched. No checkpoint promotion or broad campaign follows automatically.','',
        'The first exposure slice is explicitly preboard; exact75-bound sideboard plans and learned openings remain required later. Do not silently call Keep-sideboard games a postboard-policy test. A48 is familiar training opposition despite independent initialization. No untouched opponent, human-play, rules-parity or meta-coverage claim.','',
        f"New local work cost{a['worker_seconds_including_preflight']:.2f}summed worker-seconds including three preflight/replay matches. No paid compute. Fable remains unavailable after zero-readHTTP429 untilSeptember22 07:00EDT; no retry or endorsement. The maintainer authorized bounded local continuation. No CP7 selection.",'']
    with (root/'RESULTS.md').open('x',encoding='utf-8') as f:f.write('\n'.join(lines))
    write(root/'report-pins.json',{'script':pin(__file__),'analysis':pin(root/'analysis.json'),
          'embedding_audit':pin(exposure_path),'report':pin(root/'RESULTS.md'),'figure':pin(root/'crossover.png')})


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--root',required=True,type=Path);report(p.parse_args().root.resolve())
