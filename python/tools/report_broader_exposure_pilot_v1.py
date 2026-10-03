"""Render a completed broader-exposure comparison; no measurements or selection."""
import argparse
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from prepare_postboard_qualification_v1 import read,write,pin


def report(root):
    a=read(root/'analysis.json');assert a['complete'] and a['matches']==3864
    assert not (root/'RESULTS.md').exists() and not (root/'paired-effects.png').exists()
    labels=[('broader-minus-control','Broader vs control'),('broader-minus-g115','Broader vs g115'),('control-minus-g115','Control vs g115')]
    fig,axes=plt.subplots(1,2,figsize=(10.2,4.3),sharey=True,constrained_layout=True)
    for ax,(cohort,title) in zip(axes,[('additional','Eight additional lists'),('canonical','Seven familiar lists')]):
        for i,(key,_) in enumerate(labels):
            c=a['cohorts'][cohort]['comparisons'][key];mean=c['g1_score_difference']*100
            lo,hi=[x*100 for x in c['g1_paired_95']]
            ax.errorbar(mean,i,xerr=[[mean-lo],[hi-mean]],fmt='o',color='#146c8c',capsize=4)
        ax.axvline(0,color='#777777',linewidth=1);ax.set_title(title);ax.grid(axis='x',alpha=.2)
        ax.set_xlabel('G1 score difference (percentage points)')
    axes[0].set_yticks(range(3),[label for _,label in labels]);axes[0].invert_yaxis()
    fig.suptitle(f"Broader card exposure: {a['verdict']} | paired 95% intervals",fontweight='bold')
    fig.savefig(root/'paired-effects.png',dpi=170);plt.close(fig)
    lines=['# Broader card-exposure development result','',f"Frozen verdict: **{a['verdict']}**. All {a['matches']:,} BO3 matches completed, comprising {a['natural_games']:,} natural games. No model is promoted.",'',
        '| Own-list cohort | Model | G1 wins / games | G1 draws | Keep-sideboard BO3 wins |',
        '| --- | --- | ---: | ---: | ---: |']
    for cohort,title in [('additional','Additional'),('canonical','Familiar')]:
        for arm,r in a['cohorts'][cohort]['summary'].items():
            lines.append(f"| {title} | {arm} | {r['g1_win']}/{r['matches']} | {r['g1_draw']} | {r['bo3_win']}/{r['matches']} |")
    lines += ['', '| Cohort | Comparison | G1 difference, paired 95% interval |','| --- | --- | --- |']
    for cohort in ('additional','canonical'):
        for key,label in labels:
            c=a['cohorts'][cohort]['comparisons'][key];lo,hi=c['g1_paired_95']
            lines.append(f"| {cohort} | {label} | {100*c['g1_score_difference']:+.2f} pp [{100*lo:+.2f}, {100*hi:+.2f}] |")
    lines += ['', '![Paired effects](paired-effects.png)','',
        f"Additional-list net G1 wins: {a['additional_net_g1_wins']:+d}; required at least +27/896 and paired score lower bound above zero. Gates: {a['gates']}.",'',
        'REPLICATE permits independent repetition, not adoption. NO-ADVANCE means this bounded block missed its frozen criteria; it does not prove futility of broader exposure. Subgroup counts in analysis.json are descriptive, not per-archetype significance tests.','',
        '| Training branch | Updates | Natural games | Additional learner-list games | Native wall seconds |',
        '| --- | ---: | ---: | ---: | ---: |']
    for arm in ('control','broader'):
        t=read(root/arm/'training-audit.json');e=read(root/arm/'execution.json')
        lines.append(f"| {arm} | {t['updates']} | {t['natural_games']} | {t['additional_learner_games']} | {e['elapsed_seconds']:.2f} |")
    lines += ['',f"Evaluation consumed {a['evaluation_worker_seconds']:.2f} summed worker-seconds. Local GPU 1 for training; no paid allocation.",'',
        'The two branches began from untouched g115 full Adam32400 and ended at32600. The intervention replaced half the learner registrations while retaining seeds, roles, opponent assignments, terminal reward, GAE and optimizer settings. Current-policy weights subsequently diverge as part of the treatment.','',
        'This is a conditional curriculum comparison on development-exposed registrations against familiar A48 training opposition. It does not establish unseen-list, untouched-opponent, full-meta or human strength. Keep7 and Keep sideboarding omit learned openings and actual postboard swapping. Canonical retention is aggregate, not simultaneous per-archetype confirmation.','',
        'Fable review remains unavailable after the known zero-read HTTP429 until September 22 07:00 EDT; no retry or endorsement. Jack authorized bounded local continuation. No CP7 outcomes entered selection. Exact source/model/configuration and match pins are retained in the manifest and complete audits.','']
    with (root/'RESULTS.md').open('x',encoding='utf-8') as f:f.write('\n'.join(lines))
    write(root/'report-pins.json',{'script':pin(__file__),'analysis':pin(root/'analysis.json'),
          'report':pin(root/'RESULTS.md'),'figure':pin(root/'paired-effects.png')})


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--root',required=True,type=Path);report(p.parse_args().root.resolve())
