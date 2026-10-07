"""Render the completed, frozen opponent profile without selecting a candidate."""
import argparse
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from prepare_postboard_qualification_v1 import read, write, pin


def report(root):
    a = read(root / 'analysis.json')
    assert a['complete'] and a['matches'] == 840
    assert not (root / 'RESULTS.md').exists() and not (root / 'game-one-profile.png').exists()
    groups = [('canonical', 'Seven familiar lists'), ('additional', 'Eight additional lists'), ('all', 'All 15 lists')]
    fig, ax = plt.subplots(figsize=(9, 4.7), constrained_layout=True)
    for offset, ref, color, label in [(-.13, 'a48', '#126782', 'A48 (training opponent)'),
                                    (.13, 'v3', '#d17718', 'V3 reference')]:
        for i, (key, _) in enumerate(groups):
            row = a['cohorts'][key]['references'][ref]
            mean = row['g1_fraction']*100
            lo, hi = [x*100 for x in row['g1_paired_cluster_95']]
            ax.errorbar(mean, i+offset, xerr=[[mean-lo], [hi-mean]], fmt='o', color=color,
                        capsize=4, label=label if i == 0 else None)
            ax.annotate(f'{mean:.1f}%', (mean, i+offset), xytext=(0, 8), textcoords='offset points',
                        ha='center', fontsize=9, color=color)
    ax.set_yticks(range(3), [v for _, v in groups]); ax.invert_yaxis()
    ax.set_xlim(35, 95); ax.axvline(50, color='#999999', linestyle=':', linewidth=1)
    ax.set_xlabel('g115 game-one score (%) with paired cluster 95% intervals')
    ax.set_title('g115 performance depends on the reference opponent', loc='left', fontweight='bold')
    ax.grid(axis='x', alpha=.2); ax.legend(loc='lower right', fontsize=9)
    fig.savefig(root / 'game-one-profile.png', dpi=170); plt.close(fig)
    lines = ['# g115 opponent sensitivity result', '',
        'All 60 jobs and 840 BO3 completed, comprising 1,916 natural games. No missing cases, BO3 draws or G1 draws. g115 is unchanged; no model was promoted.', '',
        '| Own registrations | Games per reference | G1 vs A48 | G1 vs V3 | V3 minus A48, paired 95% interval |',
        '| --- | ---: | ---: | ---: | --- |']
    for key, label in groups:
        cohort = a['cohorts'][key]; refs = cohort['references']; n = cohort['matches_per_reference']
        lo, hi = cohort['v3_minus_a48_paired_95']
        lines.append(f"| {label} | {n} | {refs['a48']['g1_score']:.0f}/{n} ({100*refs['a48']['g1_fraction']:.1f}%) | {refs['v3']['g1_score']:.0f}/{n} ({100*refs['v3']['g1_fraction']:.1f}%) | {100*cohort['v3_minus_a48_g1']:+.1f} pp [{100*lo:+.1f}, {100*hi:+.1f}] |")
    lines += ['', '![Game-one profile](game-one-profile.png)', '',
        'A48 and g115 bind different fresh initializations, but A48 was a g115 training opponent. It is not held out. This is opponent sensitivity in a specified uniform list sample, not a meta-weighted or human-strength estimate.', '',
        'The lower additional-list average does not isolate a generalization failure: these lists differ in intrinsic strength, strategy and engine interactions. Per-own-list G1 counts against A48 range from 3/28 to 26/28; those small strata are descriptive. Every matchup, seat, seed and reference is preserved in audited-matches.json.', '',
        'Next causal check: run the reciprocal policy assignment on all eight additional lists, so A48 pilots the additional list and g115 the same canonical opponent list under the same seeds. Compare both assignments before attributing the gap to list strength or g115-specific competence. Do not cherry-pick only the weakest list or automatically scale training.', '',
        f"Local cost including five preflight/replay matches: {a['worker_seconds_including_preflight']:.2f} summed worker-seconds. Four BelowNormal workers, no paid allocation. All 32 V3 preflight/panel runs had zero spell-target repairs. No binaries, models or feature contracts changed.", '',
        'Primary score is G1, with Keep7 and no search. Secondary BO3 scores retain the original mainboards throughout; no learned opening or sideboarding claim. Confidence intervals resample 14 opponent/seed cases within each own-list stratum while keeping both seats/references paired, and include opponent-case variation. Sparse individual cells do not establish matchup-specific strength.', '',
        'Fable review remains unavailable after the known zero-read HTTP429 until September 22 07:00 EDT. The maintainer authorized bounded local continuation; no retry or endorsement. No CP7 outcome selection. Human play, rules parity, learned openings/sideboards, untouched opponent validation and full-field coverage remain open.', '']
    with (root / 'RESULTS.md').open('x', encoding='utf-8') as f: f.write('\n'.join(lines))
    write(root / 'report-pins.json', {'script':pin(__file__), 'analysis':pin(root/'analysis.json'),
          'report':pin(root/'RESULTS.md'), 'figure':pin(root/'game-one-profile.png')})


if __name__ == '__main__':
    p=argparse.ArgumentParser(); p.add_argument('--root', type=Path, required=True); report(p.parse_args().root.resolve())
