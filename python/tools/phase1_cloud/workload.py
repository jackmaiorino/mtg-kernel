"""Versioned, offline compatibility of a separately prepared production schedule.

Exact fixed-work proof equality remains in throughput.py. This narrower workload
class permits a bounded longer schedule of observed ten-game layouts with fresh
IDs/seeds; it never constructs or changes the future schedule.
"""
from __future__ import annotations
import copy
import hashlib
from common import encoded, require
from throughput import canonical_training_config, training_contract, preparation_workers, update_backward_execution

CLASS_SCHEMA='phase1-ten-fresh-game-workload-class/v1'
CONFIG_REQUIRED={'schema','initial_source','opponents','iterations','learning_rate',
                 'value_coefficient','output_directory'}
EPISODE_KEYS={'id','seed','registered','selected','postboard','starting_player',
              'learner_seat','max_physical_decisions','max_policy_steps'}


def digest(value):return hashlib.sha256(encoded(value)).hexdigest()


def layouts(config):
    require(CONFIG_REQUIRED<=set(config)<=CONFIG_REQUIRED|{'update_backend','collection_workers',
            'preparation_workers','update_backward_execution'},
            'unsupported training configuration fields')
    require(config['schema']=='mtg-kernel-native-expanded-training-run/v1','wrong training schema')
    require(config.get('update_backend',{'kind':'cpu'})=={'kind':'cpu'},'only qualified CPU backend supported')
    workers=config.get('collection_workers',1)
    require(type(workers) is int and 1<=workers<=1024,'invalid collector worker count')
    preparation_workers(config)
    update_backward_execution(config)
    require(config['iterations'],'empty production schedule')
    ids=set();seeds=set();result=[]
    for index,iteration in enumerate(config['iterations']):
        require(set(iteration)=={'episodes'} and len(iteration['episodes'])==10,
                'each update must collect exactly ten fresh games')
        shape=copy.deepcopy(iteration)
        for item in shape['episodes']:
            require(set(item)=={'episode','opponent'} and set(item['episode'])==EPISODE_KEYS,
                    'unsupported episode fields')
            episode=item['episode'];identifier=episode.pop('id');seed=episode.pop('seed')
            require(isinstance(identifier,str) and identifier and identifier not in ids,
                    'empty or repeated episode ID in schedule')
            require(type(seed) is int and 0<=seed<2**64 and seed not in seeds,
                    'invalid or repeated episode seed in schedule')
            ids.add(identifier);seeds.add(seed)
            require(type(episode['postboard']) is bool and
                    type(episode['learner_seat']) is int and episode['learner_seat'] in (0,1) and
                    type(episode['starting_player']) is int and episode['starting_player'] in (0,1),
                    'invalid episode seat or mode')
            for key in ('max_physical_decisions','max_policy_steps'):
                require(type(episode[key]) is int and episode[key]>0,'invalid episode resource cap')
            for key in ('registered','selected'):
                require(len(episode[key])==2,'both exact deck registrations required')
                for deck in episode[key]:
                    require(set(deck)=={'label','mainboard','sideboard'},'unsupported deck registration fields')
            opponent=item['opponent']
            require(opponent=={'kind':'current'} or
                    (set(opponent)=={'kind','id'} and opponent['kind']=='fixed' and
                     opponent['id'] in {row['id'] for row in config['opponents']}),
                    'unknown opponent or unsupported opponent-selection behavior')
        result.append(digest(shape))
    return result,ids,seeds


def compare(qualified,future,max_updates):
    require(type(max_updates) is int and max_updates>0,'explicit positive update bound required')
    observed,qualified_ids,qualified_seeds=layouts(qualified)
    requested,future_ids,future_seeds=layouts(future)
    require(len(requested)<=max_updates,'future schedule exceeds its bounded run manifest')
    # Preserve every scalar, model source and frozen opponent binding. The old
    # exact-work hash is left intact; only this separate comparison removes the
    # already-pinned future iteration array and output location.
    def invariants(config):
        value=copy.deepcopy(config);value['iterations']=[]
        return training_contract(value)
    require(invariants(qualified)==invariants(future),
            'model/import/opponent/optimizer/behavior configuration differs from qualification')
    require(qualified.get('collection_workers',1)==future.get('collection_workers',1),
            'collector worker setting differs from qualification')
    require(preparation_workers(qualified)==preparation_workers(future),
            'preparation worker setting differs from qualification')
    unseen=sorted(set(requested)-set(observed))
    require(not unseen,'future schedule contains an unqualified ordered ten-game layout: '+','.join(unseen))
    # A fresh stream cannot silently replay the benchmark's IDs or deterministic
    # episode seeds. The caller supplies the separately prepared new schedule.
    require(not (qualified_ids&future_ids) and not (qualified_seeds&future_seeds),
            'future schedule reuses qualification episode IDs or seeds')
    descriptor={'schema':CLASS_SCHEMA,'invariants_sha256':invariants(qualified),
                'collection_workers':qualified.get('collection_workers',1),
                'preparation_workers':preparation_workers(qualified),
                'games_per_update':10,'observed_layout_sha256s':sorted(set(observed)),
                'permitted_variable_fields':['episode.id','episode.seed','schedule_length','output_directory']}
    return {'schema':'phase1-production-workload-comparison/v1','compatible':True,
            'workload_class':descriptor,'workload_class_sha256':digest(descriptor),
            'future_exact_training_contract_sha256':training_contract(future),
            'future_schedule_sha256':digest(future['iterations']),
            'planned_updates':len(requested),'planned_fresh_games':10*len(requested),
            'max_updates':max_updates,'engineering_only':True,
            'limits':['Only observed ordered update layouts are supported; unseen layouts require qualification',
                      'Unique IDs/seeds and fresh native collection are required; this does not prove unseen historical seed uniqueness',
                      'A short fixed-work result does not establish constant throughput or playing strength over the longer stream']}
