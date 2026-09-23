"""Read every declared native output before applying the frozen paired analysis.

Input execution metadata is prepared by the guarded launcher. This reader
does not grant launch authority or turn a statistical pass into a verdict.
"""
import argparse,hashlib,json
from pathlib import Path
import numpy as np
from g115_d3_analysis_v1 import analyze_pair

def read(p):return json.loads(Path(p).read_bytes())
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def require(condition,message):
    if not condition:raise ValueError(message)

def read_match(expected,actual,source_commit,models):
    request=actual['request'];require(sha(request['path'])==request['sha256'],'request hash differs')
    command=read(request['path']);folder=Path(actual['output_directory'])
    require(command['matches']==expected['base_command']['matches'],'match input differs from shared panel')
    require(len(command['matches'])==1,'reader requires one complete BO3 per job')
    require(not (folder/'failure.json').exists(),'native failure record exists')
    start=read(folder/'start.json');completion=read(folder/'completion.json')
    require(start['command']==command and start['git_head']==source_commit,'native start differs from execution input')
    require(completion['matches']==1 and len(completion['match_sha256'])==1,'native completion shape differs')
    path=folder/'match-000000.json';require(sha(path)==completion['match_sha256'][0],'semantic store hash differs')
    match=read(path);seat=expected['candidate_seat']
    require(match['match']==command['matches'][0] and match['models']==start['models'],'stored match/model differs')
    require([match['models'][seat]['identity']['model'],match['models'][1-seat]['identity']['model']]==models,'installed model/features differ')
    require(match['models'][seat]['identity']['checkpoint_sha256']==expected['base_command']['sources'][seat]['source']['checkpoint']['sha256'],'g115 checkpoint differs')
    require(command['cross_generation_evaluation'] is True,'mixed-generation opt-in missing')
    require(command['sources'][1-seat]['kind']=='legacy' and command['sources'][1-seat]['v3_forced_actions'] is True and command['sources'][1-seat]['v3_spell_target_reference_adapter'] is True,'frozen V3 adapters differ')
    require(match['seat_generations'][seat]=='v4' and match['seat_generations'][1-seat]=='v3','seat generation differs')
    games=match['games'];require(len(games)==completion['natural_games'] and match['decision_count']==completion['decisions'],'completion counters differ')
    require(2<=len(games)<=command['matches'][0]['config']['max_physical_games'],'invalid natural BO3 game count')
    wins=[0,0]
    for index,game in enumerate(games):
        require(game['start']['game_index']==index+1 and max(wins)<2,'game sequence continues after match completion')
        winner=game['winner'];require(winner is None or type(winner) is int and winner in (0,1),'unknown game outcome')
        if winner is not None:wins[winner]+=1
    require(match['outcome'] in ({'winner':{'winner':0}},{'winner':{'winner':1}}),'incomplete/non-natural match ending')
    winner=match['outcome']['winner']['winner'];require(wins[winner]==2 and wins[1-winner]<2,'match winner inconsistent with games')
    search_rows=[];timings=[]
    if expected['arm']=='search':
        require(command['sources'][seat]['kind']=='information_set_search_v3','candidate search wrapper missing')
        records=match['information_set_search_v3'];require(records[1-seat] is None,'search applied to opponent')
        record=records[seat];require(record['failure'] is None,'search failure retained')
        require(record['descriptor']==command['sources'][seat]['descriptor'],'search descriptor differs')
        search_rows=record['decisions'];require(bool(search_rows),'search did no work')
        for row in search_rows:
            require(row['decision']['actor']==f'p{seat}' and 0<=row['selected']<row['decision']['legal_action_count'],'search action binding invalid')
        timing=read(folder/'search-timing-000000.json');require(timing[1-seat] is None,'opponent search timing present')
        timings=timing[seat];require(len(timings)==len(search_rows),'search timing coverage differs')
        for row,timed in zip(search_rows,timings):
            require((row['game_index'],row['decision']['actor'],row['decision']['step'])==(timed['game_index'],timed['actor'],timed['step']),'timing binding differs')
    else:
        require(command['sources'][seat]['kind']=='legacy' and not command['sources'][seat]['v3_forced_actions'],'baseline route differs')
        require('information_set_search_v3' not in match,'search appeared in baseline')
    census_fields=('natural','natural_wins','natural_losses','natural_draws','expanded','depth','budget','coverage','clip_low','clip_high','forwards')
    minima=[r['census']['raw_min'] for r in search_rows if r['census']['raw_min'] is not None]
    maxima=[r['census']['raw_max'] for r in search_rows if r['census']['raw_max'] is not None]
    return dict(win=int(winner==seat),games=len(games),decisions=match['decision_count'],sha256=sha(path),
        search_decisions=len(search_rows),search_simulations=sum(r['simulations'] for r in search_rows),
        search_transitions=sum(r['transitions'] for r in search_rows),search_elapsed_ns=sum(t['elapsed_ns'] for t in timings),
        search_census={k:sum(r['census'][k] for r in search_rows) for k in census_fields},
        search_raw_min=min(minima,default=None),search_raw_max=max(maxima,default=None),
        _timings_ns=[t['elapsed_ns'] for t in timings])

def analyze(panel,execution):
    expected={j['id']:j for j in panel['jobs']}
    require(len(expected)==len(panel['jobs'])==2048,'expected panel is not complete')
    actual={j['id']:j for j in execution['jobs']}
    require(len(actual)==len(execution['jobs']) and set(actual)==set(expected),'execution omitted, duplicated or added jobs')
    rows=[];failures=[];timings=[];arrays={a:np.full((64,8,2),-1,dtype=int) for a in ('baseline','search')}
    for identifier,job in expected.items():
        try:
            result=read_match(job,actual[identifier],execution['source_commit'],execution['models'])
            coordinate=(job['cell'],job['replica'],job['candidate_seat'])
            require(arrays[job['arm']][coordinate]==-1,'duplicate panel coordinate')
            arrays[job['arm']][coordinate]=result['win'];timings.extend(result.pop('_timings_ns'))
            rows.append(dict(id=identifier,arm=job['arm'],**result))
        except (OSError,ValueError,KeyError,TypeError,IndexError) as error:
            failures.append(dict(id=identifier,error=str(error)))
    result=dict(schema='g115-d3-native-analysis/v1',complete=not failures,formal_verdict=None,
        completed_jobs=len(rows),failed_or_missing_jobs=failures,rows=rows)
    if failures:return result
    require(all((a>=0).all() for a in arrays.values()),'incomplete pairing')
    result['statistics']=analyze_pair(arrays['baseline'],arrays['search'],panel['pairs'])
    result['totals']={arm:{field:sum(r[field] for r in rows if r['arm']==arm) for field in
        ('games','decisions','search_decisions','search_simulations','search_transitions','search_elapsed_ns')} for arm in arrays}
    search=[r for r in rows if r['arm']=='search']
    result['search_census']={k:sum(r['search_census'][k] for r in search) for k in search[0]['search_census']}
    result['search_raw_range']=[min(r['search_raw_min'] for r in search if r['search_raw_min'] is not None),
        max(r['search_raw_max'] for r in search if r['search_raw_max'] is not None)]
    result['search_latency_ms']={label:float(value)/1e6 for label,value in zip(('p50','p95','p99','max'),np.percentile(timings,[50,95,99,100]))}
    return result

def main():
    p=argparse.ArgumentParser();p.add_argument('--panel',type=Path,required=True);p.add_argument('--execution',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
    result=analyze(read(a.panel),read(a.execution))
    result['inputs']={k:dict(path=str(v),sha256=sha(v)) for k,v in [('panel',a.panel),('execution',a.execution),('reader',Path(__file__))]}
    with a.output.open('x') as f:json.dump(result,f,indent=2)
    print(json.dumps(dict(complete=result['complete'],completed_jobs=result['completed_jobs'],formal_verdict=None)))
if __name__=='__main__':main()
