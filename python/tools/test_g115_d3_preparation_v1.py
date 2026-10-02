import copy
import unittest
import numpy as np
from g115_d3_analysis_v1 import analyze_pair,paired_sign_flip

PAIRS=[(f'd{i}',f'd{j}') for i in range(8) for j in range(8)]
class PairedAnalysisTests(unittest.TestCase):
    def test_integer_gate_requires21_even_with_positive_lower_bound(self):
        baseline=np.zeros((64,8,2),dtype=int)
        for wins in (20,21):
            search=baseline.copy();search.flat[:wins]=1
            result=analyze_pair(baseline,search,PAIRS)
            self.assertGreater(result['bootstrap_lower_net'],0)
            self.assertEqual(result['statistical_gate_pass'],wins==21)
            self.assertIsNone(result['formal_verdict'])
    def test_identical_and_reversed_arms(self):
        rng=np.random.default_rng(20260923);base=rng.integers(0,2,(64,8,2));other=rng.integers(0,2,(64,8,2))
        equal=analyze_pair(base,base,PAIRS)
        self.assertEqual(equal['net_wins'],0);self.assertEqual(equal['paired_se_pp'],0)
        self.assertEqual(equal['bootstrap_95_pp'],[0.,0.]);self.assertFalse(equal['statistical_gate_pass'])
        a=analyze_pair(base,other,PAIRS);b=analyze_pair(other,base,PAIRS)
        self.assertEqual(a['net_wins'],-b['net_wins']);self.assertEqual(a['paired_se_pp'],b['paired_se_pp'])
        self.assertEqual(a['bootstrap_lower_net'],-b['bootstrap_upper_net'])
        self.assertAlmostEqual(a['arm_label_permutation']['two_sided_p'],b['arm_label_permutation']['two_sided_p'])
    def test_pairing_not_independent_seats(self):
        base=np.zeros((64,8,2),dtype=int);search=base.copy();search[:,0,:]=1
        result=analyze_pair(base,search,PAIRS)
        self.assertAlmostEqual(result['paired_se_pp'],100*np.sqrt(np.var(search[:,:,0],axis=1,ddof=1).sum()/8/64**2))
        self.assertEqual(result['search_only_wins'],128)
    def test_permutation_known_distribution(self):
        blocks=np.zeros((64,8),dtype=int);blocks[0,:3]=[1,1,2]
        result=paired_sign_flip(blocks)
        self.assertAlmostEqual(result['one_sided_p'],1/8);self.assertAlmostEqual(result['two_sided_p'],1/4)
    def test_missing_nonbinary_and_bad_pair_labels_rejected(self):
        a=np.zeros((64,8,2),dtype=int)
        with self.assertRaises(ValueError):analyze_pair(a[:63],a[:63],PAIRS)
        wrong=a.astype(float);wrong[0,0,0]=np.nan
        with self.assertRaises(ValueError):analyze_pair(a,wrong,PAIRS)
        with self.assertRaises(ValueError):analyze_pair(a,a,[PAIRS[0]]*64)
if __name__=='__main__':unittest.main()
