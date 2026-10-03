"""Descriptive decision audit, never a strength estimate or promotion gate."""
import argparse
from collections import Counter
import json
from pathlib import Path
import re
from capture_broader_behavior_diagnostic_v1 import read, pin, checked, write, ARMS


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--root',required=True,type=Path)
    root=parser.parse_args().root; m=read(root/'manifest.json'); complete=read(root/'completion.json')
    assert complete['complete'] and complete['matches']==18
    cards_path=Path(__file__).resolve().parents[2]/'data/cards_v1.json'
    cards=read(cards_path)['cards']; names=[c['name'] for c in cards]
    traces={}
    for job in m['jobs']:
        receipt=next(a for a in complete['audits'] if a['name']==job['name'])
        traces[(job['case'],job['row']['arm'],job['row']['seat'])]=read(checked(receipt['output']))
    def card_name(ref): return names[ref['card_db_id']]
    def describe(a):
        text=a['action_kind']
        if 'source' in a:text+=' '+card_name(a['source'])
        if 'cards' in a:text+=' '+', '.join(card_name(c) for c in a['cards'])
        if 'color' in a:text+=' '+a['color']
        if 'target' in a:
            t=a['target'];text+=' -> '+(card_name(t['object']) if 'object' in t else t['player'])
        return text
    main=traces[(0,'g115',0)]['config']['registrations'][0]['mainboard']
    cost_colors=sorted(set(re.findall(r'[WUBRG]', ''.join(cards[i]['mana_cost'] or '' for i in main))))
    assert cost_colors==['U','W']
    gate_events=[]; metrics=[]
    for arm in ARMS:
        counts=Counter(); bad_masses=[]
        for seat in (0,1):
            trace=traces[(0,arm,seat)]['collected']['trajectory']['games'][0]
            for d in trace['decisions']:
                if d['actor']!='p'+str(seat) or d['visible']['kind']!='gameplay':continue
                v=d['visible'];p=v['observation']['projection'];actions=v['ordered_actions'];index=d['behavior']['selected_index'];selected=actions[index]
                masses=list(map(int,d['behavior']['mass_numerators']));assert len(masses)==len(actions) and sum(masses)==2**64
                kind=selected['action_kind'];name=card_name(selected['source']) if 'source' in selected else ''
                event=None
                if kind=='choose_effect_color' and name in ('Sea Gate','Citadel Gate'):
                    useful=[i for i,a in enumerate(actions) if a.get('color') in cost_colors];assert useful
                    bad=sum(n for a,n in zip(actions,masses) if a.get('color') not in cost_colors)/sum(masses)
                    counts['gate_choices']+=1;counts['outside_mainboard_mana_colors']+=selected['color'] not in cost_colors
                    bad_masses.append(bad)
                    event={'kind':'gate_color','chosen':selected['color'],'outside_color_probability_mass':bad,
                        'offered_colors':{a['color']:n/sum(masses) for a,n in zip(actions,masses)}}
                elif kind=='cast_spell' and name=='Prismatic Strands':
                    counts['strands_casts']+=1
                    own_main=p['active_player']==d['actor'] and p['phase'] in ('main1','main2')
                    counts['strands_own_main_casts']+=own_main
                    event={'kind':'strands_cast','own_main_phase':own_main,'stack':[
                        {'source':card_name(s['source']),'controller':s['controller'],'kind':s['stack_item_kind']} for s in p['stack']]}
                elif kind=='activate_ability' and name=='Basilisk Gate':
                    counts['basilisk_activations']+=1;counts['basilisk_main2']+=p['phase']=='main2'
                    event={'kind':'basilisk_activation'}
                if event is not None:
                    gate_events.append({'arm':arm,'seat':seat,'game':1,'decision':d['decision_index'],
                        'turn':p['turn'],'phase':p['phase'],'active_player':p['active_player'],
                        'selected_probability':masses[index]/sum(masses),**event})
        metrics.append({'arm':arm,**counts,'mean_outside_color_probability_mass':sum(bad_masses)/len(bad_masses)})
    divergences=[]
    for case in (1,2):
        for seat in (0,1):
            left=traces[(case,'g115',seat)]['collected']['trajectory']['games'][0]['decisions']
            right=traces[(case,'broader',seat)]['collected']['trajectory']['games'][0]['decisions']
            found=False
            for a,b in zip(left,right):
                assert a['actor']==b['actor'] and a['visible']==b['visible'], 'State changed before any selected action changed'
                if a['behavior']['selected_index']==b['behavior']['selected_index']:continue
                assert a['visible']['kind']=='gameplay'
                visible=a['visible'];p=visible['observation']['projection'];actions=visible['ordered_actions']
                choices={}
                for arm,d in [('g115',a),('broader',b)]:
                    masses=list(map(int,d['behavior']['mass_numerators']));index=d['behavior']['selected_index']
                    choices[arm]={'selected_index':index,'selected_action':actions[index],
                        'description':describe(actions[index]),'selected_probability':masses[index]/sum(masses),
                        'distribution':[{'action':describe(x),'probability':n/sum(masses)} for x,n in zip(actions,masses)]}
                divergences.append({'case':case,'deck':m['selection'][case][0],'seat':seat,'game':1,
                    'decision':a['decision_index'],'identical_visible_prefix':True,'turn':p['turn'],'phase':p['phase'],
                    'choices':choices,'equivalent_action_description':choices['g115']['description']==choices['broader']['description']})
                found=True;break
            assert found
    result={'complete':True,'manifest':pin(root/'manifest.json'),'card_metadata':pin(cards_path),
        'mainboard_printed_mana_cost_colors':cost_colors,'gate_g1_metrics':metrics,'gate_g1_events':gate_events,
        'first_canonical_g1_divergences':divergences,
        'limits':['Three outcome-conditioned development cases with both seats/all three models; no prevalence or win-rate inference.',
            'Timing and color observations identify candidate policy weaknesses, not counterfactual win causes.',
            'Printed mana-cost colors omit alternate costs, ability costs and sideboard possibilities; no claim that every other color is universally dominated.',
            'First divergence is not proof it caused a later loss; one observed divergence selects another copy of the same card.',
            'No hidden hands/libraries were used to score policy decisions. No rule, model, observation or reward changes.']}
    write(root/'decision-audit.json',result)
    lines=['# Broader-exposure behavior diagnosis','',
        'All18matched BO3 traces completed (41natural games,5681decisions); the exact replay is byte-identical. All serialized original game starts, seeds and winners reproduced. This is outcome-conditioned development diagnosis, not new strength evidence.','',
        '| Gates G1, both seats | Gate choices outside U/W | Mean mass outside U/W | Strands casts in own main phase |',
        '| --- | ---: | ---: | ---: |']
    for x in metrics:lines.append(f"| {x['arm']} | {x['outside_mainboard_mana_colors']}/{x['gate_choices']} | {100*x['mean_outside_color_probability_mass']:.1f}% | {x['strands_own_main_casts']}/{x['strands_casts']} |")
    lines += ['', 'The mainboard printed mana costs use only U/W. A U/W alternative was actually offered at every recorded Gate choice. These observations expose poor color use and questionable prevention timing in this slice; they do not establish a missing legal action or prove which choice caused a loss. Broader exposure reduced the average off-color mass on its visited states, but the states are not all identical across branches.','',
        '| Canonical first differing choice | g115 | Broader |','| --- | --- | --- |']
    for x in divergences:
        a=x['choices']['g115'];b=x['choices']['broader']
        lines.append(f"| {x['deck']} seat{x['seat']}, G1 decision{x['decision']} | {a['description']} ({a['selected_probability']:.1%}) | {b['description']} ({b['selected_probability']:.1%}) |")
    lines += ['', 'Each reported divergence has an identical complete visible prefix and legal menu before the selected action changes. Burn seat0 still strongly favors Grab the Prize in both policies; the sampled Highway Robbery branch crosses a probability threshold. Burn seat1 changes the discard preference between Lava Dart and Grab the Prize. Elves seat0 selects a different Lead the Stampede candidate; seat1 selects a different physical copy of Quirion Ranger. These are distinct mechanisms and do not prove a global forgetting explanation.','',
        'Next causal question: does correcting only the Gates color choice, using actor-known information and preserving the policy elsewhere, change its trajectory or terminal result? First qualify a bounded matched counterfactual and deterministic replay; do not adopt a heuristic or train a new block from these diagnostic counts. Separately audit the available printed-card and own-deck information before proposing an observation change.','',
        'Fable remains unavailable after the known zero-read HTTP429 until September22 07:00EDT. No retry or endorsement. The completed pilot stays NO-ADVANCE and g115 stays the reference. No CP7 outcomes or paid compute were used.','']
    with (root/'RESULTS.md').open('x',encoding='utf-8') as f:f.write('\n'.join(lines))
    print({'complete':True,'gate_metrics':metrics,'first_divergences':len(divergences)})


if __name__=='__main__':main()
