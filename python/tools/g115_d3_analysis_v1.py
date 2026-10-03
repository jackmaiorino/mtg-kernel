"""Frozen D3 paired statistics. Outcome provenance/validity is a separate input gate.

This module reports the numerical gate only, never a formal ADVANCE on its own.
"""
import numpy as np
from g115_d3_power_core import BootstrapGate,observed_counts

def paired_sign_flip(blocks):
    """Exact conditional seed-cluster arm-label null control, not a selection gate.

    Valid randomization inference needs arm-label exchangeability under the
    sharp null. Do not call this a distribution-free test of a weak mean null.
    """
    absolute=np.abs(blocks).ravel().astype(int)
    spread=int(absolute.sum());observed=int(blocks.sum())
    if spread==0:return dict(one_sided_p=1.,two_sided_p=1.,nonzero_clusters=0)
    spectrum=np.ones(2049,dtype=complex)
    for size in (1,2):
        count=int((absolute==size).sum())
        polynomial=np.zeros(4096);polynomial[0]=.5;polynomial[2*size]=.5
        spectrum*=np.fft.rfft(polynomial)**count
    mass=np.fft.irfft(spectrum,n=4096)[:2*spread+1]
    if mass.min() < -1e-10:raise ArithmeticError('Permutation FFT error')
    mass=np.maximum(mass,0);mass/=mass.sum();support=np.arange(-spread,spread+1)
    return dict(one_sided_p=float(mass[support>=observed].sum()),two_sided_p=float(mass[np.abs(support)>=abs(observed)].sum()),nonzero_clusters=int((absolute>0).sum()))

def analyze_pair(baseline,search,pairs):
    baseline=np.asarray(baseline);search=np.asarray(search)
    if baseline.shape!=(64,8,2) or search.shape!=baseline.shape:raise ValueError('Require complete64x8x2 paired panel')
    if not np.isin(baseline,[0,1]).all() or not np.isin(search,[0,1]).all():raise ValueError('Require binary match outcomes')
    if len(pairs)!=64 or len({tuple(p) for p in pairs})!=64:raise ValueError('Require64 distinct matchup labels')
    baseline=baseline.astype(int);search=search.astype(int);diff=search-baseline;blocks=diff.sum(axis=2)
    gate=BootstrapGate().assess(observed_counts(blocks))
    se=float(np.sqrt((blocks/2).var(axis=1,ddof=1).sum()/8/64**2)*100)
    def counts(a,b):
        n=a.size
        return dict(matches=n,baseline_wins=int(a.sum()),search_wins=int(b.sum()),baseline_rate_pp=float(a.mean()*100),search_rate_pp=float(b.mean()*100),net_wins=int((b-a).sum()),delta_pp=float((b-a).mean()*100),search_only_wins=int(((a==0)&(b==1)).sum()),baseline_only_wins=int(((a==1)&(b==0)).sum()))
    return dict(schema='g115-d3-paired-statistics/v1',**counts(baseline,search),paired_seed_blocks=512,
        paired_se_pp=se,bootstrap_lower_net=gate['lower_net'],bootstrap_upper_net=gate['upper_net'],
        bootstrap_95_pp=[100*gate['lower_net']/1024,100*gate['upper_net']/1024],
        statistical_gate_pass=bool(gate['advance']),integer_net_win_threshold=21,
        by_seat=[dict(candidate_seat=s,**counts(baseline[:,:,s],search[:,:,s])) for s in (0,1)],
        by_matchup=[dict(own=p[0],opponent=p[1],**counts(baseline[i],search[i])) for i,p in enumerate(pairs)],
        arm_label_permutation=paired_sign_flip(blocks),
        formal_verdict=None,limits='Numerical gate only. Requires verified complete paired native records, exact input/model/runtime identities, valid search boundaries and review disposition of power assumptions. No automatic at-power closure from an SE alone.')
