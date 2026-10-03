"""Read-only retained-update audit for card embeddings on qualified registrations."""
import argparse
from pathlib import Path
import struct
from prepare_postboard_qualification_v1 import BASE, REPO, read, write, pin
from run_postboard_pilot_v1 import verify_pin
from phase1_breadth_v1.catalog_v1 import canonical_zones


def run(root):
    profile=Path('E:/mtg-postboard-campaign-20260920/opponent-profile-002')
    variant=Path('E:/mtg-postboard-campaign-20260920/published-variant-qualification-001')
    manifest=read(profile/'manifest.json'); vmanifest=read(variant/'manifest.json')
    registry_path=verify_pin(vmanifest['registry']); cards=read(registry_path)['cards']
    selected=read(verify_pin(manifest['variant_selection']))
    known=read(BASE/'catalog/registrations.json')
    aliases=vmanifest['aliases_from_snapshot']
    known_main={n for r in known for n in canonical_zones(r,aliases)['mainboard']}
    known_all={n for r in known for z in canonical_zones(r,aliases).values() for n in z}
    named={c['name']:i for i,c in enumerate(cards)}
    models={}
    for label,source in [('g115',manifest['candidate']),('a48',manifest['references']['a48'])]:
        checkpoint=read(verify_pin(source['checkpoint']))
        descriptor=read(verify_pin(source['play_import']))
        initialization=read(verify_pin(descriptor['initialization']))
        initial=verify_pin(descriptor['parameters']).read_bytes()
        tensor=next(t for t in initialization['parameters'] if t['name']=='card_embedding.weight')
        assert tensor['shape']==[65537,16]
        fields={key:next(t for t in checkpoint[key] if t['name']=='card_embedding.weight')
                for key in ('parameters','first_moments','second_moments')}
        assert all(t['shape']==[65537,16] and len(t['values'])==65537*16 for t in fields.values())
        rows=[]
        for card_id,card in enumerate(cards):
            # Current actor encodings use flat_card_token_v2(id) = u32(id)+1.
            token=card_id+1; start=token*16
            original=list(struct.unpack_from('<16I',initial,tensor['byte_offset']+start*4))
            current={k:t['values'][start:start+16] for k,t in fields.items()}
            same=current['parameters']==original
            zero=all(v==0 for key in ('first_moments','second_moments') for v in current[key])
            rows.append({'card_id':card_id,'card_token':token,'name':card['name'],
                'weights_bit_identical_to_initial':same,'both_adam_moments_positive_zero':zero,
                'no_retained_embedding_update':same and zero,
                'in_seven_mainboards':card['name'] in known_main,'in_seven_registered_75s':card['name'] in known_all})
        models[label]={'source':source,'initialization':descriptor['initialization'],
            'initial_parameters':descriptor['parameters'],'adam_step':checkpoint['adam_step'],
            'rows':rows,'registry_cards_with_no_retained_embedding_update':sum(r['no_retained_embedding_update'] for r in rows)}
    per_list=[]
    for registration in selected:
        zones=registration['zones']; entry={'zoned_sha256':registration['zoned_sha256'],'models':{}}
        for label,model in models.items():
            rowmap={r['name']:r for r in model['rows']}
            entry['models'][label]={z:{'distinct_cards':len(zones[z]),
                'unchanged_distinct_cards':sum(rowmap[n]['no_retained_embedding_update'] for n in zones[z]),
                'unchanged_copies':sum(count for n,count in zones[z].items() if rowmap[n]['no_retained_embedding_update']),
                'unchanged_names':sorted(n for n in zones[z] if rowmap[n]['no_retained_embedding_update'])}
                for z in ('mainboard','sideboard')}
        per_list.append(entry)
    root.mkdir()
    result={'schema':'card-embedding-retained-update-audit/v1','script':pin(__file__),
        'registry':pin(registry_path),'variant_selection':manifest['variant_selection'],
        'training_registrations':pin(BASE/'catalog/registrations.json'),
        'card_token_source':pin(REPO/'mtg-kernel/src/rl_session.rs'),
        'checkpoint_bits_source':pin(REPO/'mtg-kernel/src/expanded_deck_training_v1.rs'),
        'models':models,'per_additional_list':per_list,
        'limits':['Exact unchanged card embedding weights plus zero retained Adam moments are evidence of no retained embedding update, not proof that a card was never observed.',
            'Shared non-embedding layers and visible semantic features can still generalize; this audit does not measure policy competence or identify the cause of losses.',
            'Seven current canonical registrations are not a full ancestral training census. Sideboard card membership does not establish in-game exposure.',
            'No model/optimizer/registry changes, engine runs, CP7 outcome selection or paid compute.']}
    write(root/'audit.json',result)
    print({'registry_no_retained_update':{k:v['registry_cards_with_no_retained_embedding_update'] for k,v in models.items()},
           'additional_mainboards':[{ 'list':r['zoned_sha256'][:12],**{k:v['mainboard'] for k,v in r['models'].items()}} for r in per_list]},flush=True)


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--root',required=True,type=Path);run(p.parse_args().root.resolve())
