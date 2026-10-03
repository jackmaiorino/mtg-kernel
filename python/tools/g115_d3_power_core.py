"""D3 planning math only. No game execution, weights, or selection outcomes.

Exact conditional paired-bootstrap distribution via polynomial convolution.
Each row is one matchup's eight paired-seat net-win counts in {-2,-1,0,1,2}.
Power alternatives exponentially tilt each observed empirical stratum with
one common parameter. This is an explicit planning model, not a search law.
"""
import itertools
import numpy as np

NFFT=4096
SUPPORT=np.arange(-2,3,dtype=float)

def compositions(n,k):
    if k==1:
        yield (n,)
    else:
        for i in range(n+1):
            for rest in compositions(n-i,k-1): yield (i,)+rest

class BootstrapGate:
    def __init__(self):
        self.counts=np.asarray(list(compositions(8,5)),dtype=np.int16)
        self.lookup={tuple(row):i for i,row in enumerate(self.counts)}
        coeff=np.zeros((len(self.counts),NFFT),dtype=float)
        coeff[:,:5]=self.counts/8.
        self.spectra=np.fft.rfft(coeff,axis=1)**8

    def assess(self,counts):
        counts=np.asarray(counts)
        if counts.shape!=(64,5) or not np.all(counts.sum(axis=1)==8):
            raise ValueError('Expected 64 matchup strata, eight paired seeds each')
        ids=[self.lookup[tuple(row)] for row in counts]
        mass=np.fft.irfft(np.prod(self.spectra[ids],axis=0),n=NFFT)
        if mass.min() < -1e-10 or abs(mass.sum()-1)>1e-9:
            raise ArithmeticError('FFT probability error')
        mass=np.maximum(mass[:2049],0);mass/=mass.sum()
        cumulative=np.cumsum(mass)
        lo=int(np.searchsorted(cumulative,.025))-1024
        hi=int(np.searchsorted(cumulative,.975))-1024
        net=int(counts@SUPPORT @ np.ones(64))
        # Material gain >=2pp is the integer threshold ceil(.02*1024)=21.
        return dict(net_wins=net,lower_net=lo,upper_net=hi,advance=net>=21 and lo>0)

def observed_counts(blocks):
    blocks=np.asarray(blocks)
    if blocks.shape!=(64,8) or not np.isin(blocks,SUPPORT).all():
        raise ValueError('Wrong paired panel')
    return np.stack([(blocks==x).sum(axis=1) for x in SUPPORT],axis=1)

def alternative(counts,effect_pp,discordance_tilt=0.):
    base=counts/8.
    target=2*effect_pp/100.
    def evaluate(lam):
        logits=lam*SUPPORT+discordance_tilt*np.abs(SUPPORT)
        weights=base*np.exp(logits-logits.max())
        weights/=weights.sum(axis=1,keepdims=True)
        return weights,float((weights@SUPPORT).mean())
    lo,hi=-40.,40.
    if not evaluate(lo)[1]<target<evaluate(hi)[1]:
        raise ValueError('Requested effect outside empirical support')
    for _ in range(80):
        mid=(lo+hi)/2
        if evaluate(mid)[1]<target:lo=mid
        else:hi=mid
    probs,mean=evaluate((lo+hi)/2)
    se=np.sqrt(((probs@SUPPORT**2)-(probs@SUPPORT)**2).sum()*8)/1024
    return probs,dict(effect_pp=mean*50,paired_se_pp=float(se*100),lambda_tilt=(lo+hi)/2,discordance_tilt=discordance_tilt)

def simulate(blocks,effect_pp,replicates,seed,discordance_tilt=0.):
    counts=observed_counts(blocks)
    probs,model=alternative(counts,effect_pp,discordance_tilt)
    rng=np.random.default_rng(seed)
    draws=rng.multinomial(8,probs,size=(replicates,64))
    gate=BootstrapGate()
    passed=sum(gate.assess(x)['advance'] for x in draws)
    return dict(**model,replicates=replicates,passed=passed,power=passed/replicates,seed=seed)

if __name__=='__main__':
    # Bounded arithmetic checks, not game evaluation or a power claim.
    g=BootstrapGate()
    zeros=np.zeros((64,5),dtype=int);zeros[:,2]=8
    assert g.assess(zeros)==dict(net_wins=0,lower_net=0,upper_net=0,advance=False)
    positive=zeros.copy();positive[0]=[0,0,0,0,8];positive[1]=[0,0,0,8,0]
    assert g.assess(positive)==dict(net_wins=24,lower_net=24,upper_net=24,advance=True)
    mixture=zeros.copy();mixture[0]=[0,0,4,4,0]
    result=g.assess(mixture)
    assert result==dict(net_wins=4,lower_net=1,upper_net=7,advance=False),result
    # Direct binomial convolution agrees with the FFT stratum distribution.
    from math import comb
    cdf=np.cumsum([comb(8,k)/256 for k in range(9)])
    assert result['lower_net']==int(np.searchsorted(cdf,.025))
    assert result['upper_net']==int(np.searchsorted(cdf,.975))
    print('Exact paired-bootstrap arithmetic checks passed')
