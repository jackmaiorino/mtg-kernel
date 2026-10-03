"""Render the already frozen complete analysis; never changes selection gates."""
import argparse
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from prepare_postboard_qualification_v1 import read, write, pin


def report(root):
    result = read(root / 'analysis.json')
    assert result['complete'] and result['matches'] == 2352
    assert pin(root / 'manifest.json') == result['manifest']
    assert not (root / 'RESULTS.md').exists()
    comparisons = result['comparisons']
    keys = ['mixed-minus-preboard', 'mixed-minus-g115', 'preboard-minus-g115']
    labels = ['Postboard vs control', 'Postboard vs g115', 'Control vs g115']
    fig, axes = plt.subplots(1, 2, figsize=(11, 3.8), layout='constrained')
    for ax, metric, title in zip(axes,
            ['bo3', 'game_one'], ['BO3 win rate difference', 'Game-one score difference']):
        for i, key in enumerate(keys):
            row = comparisons[key]
            estimate = row['bo3_win_fraction_difference' if metric == 'bo3' else 'game_one_score_difference'] * 100
            lo, hi = [x * 100 for x in row[metric + '_paired_95_interval']]
            ax.plot([lo, hi], [i, i], color='#236b8e', linewidth=2)
            ax.scatter([estimate], [i], color='#236b8e', s=40, zorder=3)
        ax.axvline(0, color='#777777', linewidth=1, linestyle='--')
        ax.set_yticks(range(3), labels)
        ax.invert_yaxis()
        ax.set_ylim(2.5, -.5)
        ax.set_title(title)
        ax.set_xlabel('Percentage points; paired 95% interval')
        ax.grid(axis='x', alpha=.2)
    fig.suptitle('Fixed V3 opponent, 784 BO3 per arm | development evidence only', fontsize=12)
    figure = root / 'paired-effects.png'
    assert not figure.exists()
    fig.savefig(figure, dpi=160)
    plt.close(fig)
    lines = ['# Matched postboard development result', '',
        f"Frozen verdict: **{result['verdict']}**. All 2,352 BO3 matches completed.", '',
        '| Model | BO3 wins / 784 | Draws | Game-one score / 784 |',
        '| --- | ---: | ---: | ---: |']
    for arm in ('g115', 'preboard', 'mixed'):
        row = result['summary'][arm]
        lines.append(f"| {arm} | {row['wins']} ({100*row['wins']/784:.2f}%) | {row['draws']} | {row['game_one_score']:g} |")
    lines += ['', '| Comparison | Net BO3 wins | Win-rate difference, paired 95% interval | G1 difference, paired 95% interval |',
        '| --- | ---: | --- | --- |']
    for key, label in zip(keys, labels):
        row = comparisons[key]
        def fmt(estimate, bounds):
            return f'{estimate*100:+.2f} pp [{bounds[0]*100:+.2f}, {bounds[1]*100:+.2f}]'
        bo3 = fmt(row['bo3_win_fraction_difference'], row['bo3_paired_95_interval'])
        g1 = fmt(row['game_one_score_difference'], row['game_one_paired_95_interval'])
        lines.append(f"| {label} | {row['net_bo3_wins']:+d} | {bo3} | {g1} |")
    lines += ['', '![Paired effects](paired-effects.png)', '',
        f"Primary gate: {result['gates']['primary']}. Game-one retention: {result['gates']['retention']}. At least untouched-g115 wins: {result['gates']['at_least_reference_wins']}.", '',
        'A REPLICATE result permits another independent test, never promotion. NO-ADVANCE means this block did not satisfy the declared development criteria; it does not establish futility of postboard training.', '',
        'The 392 common seed cases retain both physical seats and all three arms. Bootstrap resampling is stratified within 49 ordered matchups. The detailed breakdown in analysis.json contains matchup, physical seat and G1 play/draw counts; each such cell has only four BO3 per arm.', '',
        '| Training arm | Completed updates | Natural games | Postboard games | Native wall seconds |',
        '| --- | ---: | ---: | ---: | ---: |']
    for arm in ('preboard', 'mixed'):
        audit = read(root / arm / 'training-audit.json')
        execution = read(root / arm / 'execution.json')
        assert audit['complete'] and execution['exit_code'] == 0 and not execution['timed_out']
        lines.append(f"| {arm} | {audit['updates']} | {audit['natural_games']} | {audit['postboard_games']} | {execution['elapsed_seconds']:.2f} |")
    lines += ['', f"Evaluation consumed {result['evaluation_worker_seconds']:.2f} summed worker seconds. Local GPU 1; no paid allocation.", '',
        '## Limits', ''] + [f'- {x}' for x in result['limitations']]
    (root / 'RESULTS.md').write_text('\n'.join(lines) + '\n', encoding='utf-8')
    write(root / 'report-pins.json', {'analysis': pin(root / 'analysis.json'), 'script': pin(__file__),
        'outputs': [pin(root / 'RESULTS.md'), pin(figure)]})
    print(root / 'RESULTS.md')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    report(parser.parse_args().root.resolve())
