import copy
import hashlib
import json
import struct
import sys
import unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'tools'))
from viewer_uct_export_v1 import replay
from viewer_reconstruction_export_v1 import export_root

def fixture():
    action=lambda index,label:{'index':index,'label':label,'kind':'pass','refs':[]}
    choice=lambda node,a,step,scores:{'node':node,'action':a,'step':step,'physical':step,'substep':0,'live':1-a if node==0 else 0,'scores':scores,'board':0,'expanded':node>0}
    move=lambda step,flags=0,resolution=0,live=0:{'actor':'p0','step':step,'physical':step,'substep':0,'action':action(live,'Recorded public choice'),'flags':flags,'resolution':resolution,'before':0,'after':0}
    simulations=[{'ordinal':0,'choices':[choice(0,0,15,[1.5,1.5]),choice(1,0,16,[1.5])],
        'rollout':[move(15,2,live=1),move(16,resolution=2),move(17,4),move(18,resolution=4)],'reason':'win','backed_up':True,'leaf':0,'terminal':{'terminal_classification':'natural','winner':'p0'}},
        {'ordinal':1,'choices':[choice(0,1,15,[1.,1.5])],'rollout':[move(15)],'reason':'natural_nonwin','backed_up':True,'leaf':0,'terminal':{'terminal_classification':'natural','winner':None}},
        {'ordinal':2,'choices':[choice(0,0,15,[1.7,1.6]),choice(2,0,16,[1.5])],'rollout':[move(15,live=1),move(16)],'reason':'truncated','backed_up':False}]
    nodes=[{'id':0,'key':'00'*32,'parent':None,'board':0,'actions':[action(0,'Cast Spy'),action(1,'Pass')],'live_order':[1,0],'committed':True,'visits':2,'n':[1,1],'wins':[1,0]},
        {'id':1,'key':'01'*32,'parent':[0,0],'board':0,'actions':[action(0,'Target yourself')],'live_order':[0],'committed':True,'visits':1,'n':[1],'wins':[1]},
        {'id':2,'key':'02'*32,'parent':[0,0],'board':0,'actions':[action(0,'Target opponent')],'live_order':[0],'committed':False,'visits':0,'n':[0],'wins':[0]}]
    digest=hashlib.sha256()
    for n in nodes[:2]:
        digest.update(bytes.fromhex(n['key']));digest.update(struct.pack('<Q',n['visits']))
        for a,b in zip(n['n'],n['wins']):digest.update(struct.pack('<QQ',a,b))
    key={'episode':1,'step':15,'physical':15,'substep':0,'actor':'p0'}
    record={'decision':{'episode_id':1,'step':15,'physical_decision_id':15,'substep_index':0,'acting_player':'p0'},'budget':128000,'transitions':7,'attempted':3,'completed_natural':2,
        'tree':{'hash':digest.hexdigest(),'nodes':2,'root':{'n':[1,1],'wins':[1,0],'frozen_choice':0}}}
    return {'complete':True,'record':record,'boards':[{'own_hand':[{'secret':'sampled card'}]}],'nodes':nodes,'simulations':simulations}, {'key':key,'executed':1,'actions':[action(0,'Pass'),action(1,'Cast Spy')]}

class VisibleExporter:
    def board(self,raw):
        if raw.get('own_hand'):raise AssertionError('Private cards were registered before redaction')
        p={'life':20,'handCount':0,'libraryCount':34,'mana':[0]*6,'battlefield':[],'graveyard':[],'hand':[]}
        return {'turn':8,'phase':'main1','active':'p0','priority':'p0','players':[copy.deepcopy(p),copy.deepcopy(p)],'exile':[],'stack':[],'combat':[]}

class UctTests(unittest.TestCase):
    def test_full_attempts_reconcile_and_private_cards_are_removed_before_metadata(self):
        raw,f=fixture();result=replay(raw,VisibleExporter(),f)
        self.assertEqual(result['firstCombo'],0)
        self.assertEqual(result['nodes'][0]['actions'][0]['index'],0)
        self.assertEqual(result['nodes'][0]['liveOrder'],[1,0])
        self.assertFalse(result['nodes'][2]['committed'])
        self.assertEqual(result['simulations'][1]['terminal']['winner'],None)
        self.assertNotIn('key',result['nodes'][0])
        self.assertNotIn('sampled card',json.dumps(result))

    def test_rejects_broken_native_hash_counts_source_and_score(self):
        for change in [lambda r:r['record']['tree'].update(hash='f'*64),lambda r:r['nodes'][0]['n'].__setitem__(0,2),
            lambda r:r['record']['decision'].update(substep_index=1),lambda r:r['simulations'][1]['choices'][0].update(scores=[2.,1.])]:
            raw,f=fixture();change(raw)
            with self.assertRaises(ValueError):replay(raw,VisibleExporter(),f)

    def test_nonnatural_attempt_stays_unbacked(self):
        raw,f=fixture();raw['simulations'][2].update(reason='nonnatural',terminal={'terminal_classification':'step_cap'})
        result=replay(raw,VisibleExporter(),f)
        self.assertEqual(result['simulations'][2]['terminal'],{'kind':'Non-natural termination','winner':None})
        self.assertFalse(result['simulations'][2]['backedUp'])

if __name__=='__main__':unittest.main()
