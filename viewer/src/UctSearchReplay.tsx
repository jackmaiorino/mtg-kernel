import { useEffect, useMemo, useReducer, useRef, useState } from 'react';
import { ArrowLeft, ArrowRight, GitBranch, Pause, Play, Sparkles } from 'lucide-react';
import type { Frame, Recording } from './recording';
import { initialSearchPlayback, searchReducer } from './search-playback';
import { uctEvents, uctGraphAt } from './uct-playback';
import { SearchGraphCanvas } from './SearchGraph';
import type { GraphTopology } from './search-graph';
import { Preview } from './SearchReplay';
import './search-replay.css';
import './uct-search.css';

const outcomes={win:'Natural win',natural_nonwin:'Natural result without a focal win',nonnatural:'Non-natural termination',truncated:'Search limit reached',fault:'Simulation fault',rejected:'Sampled world rejected'};
const clean=(s:string)=>s.replace(/\s*\[c\d+\]/g,'');

export function UctSearchReplayView({frame,recording,assetBase,onInteract}:{frame:Frame;recording:Recording;assetBase:string;onInteract:()=>void}) {
  const replay=frame.search!.uctReplay!,events=useMemo(()=>uctEvents(replay),[replay]);
  const [state,dispatch]=useReducer(searchReducer,initialSearchPlayback,s=>{
    const p=new URLSearchParams(window.location.hash.slice(1)),event=Number(p.get('searchEvent')??0);
    return {...s,event:p.get('decision')===frame.id&&p.get('game')===recording.id&&Number.isInteger(event)&&event>=0&&event<events.length?event:0};
  });
  const [previewNode,setPreviewNode]=useState<number|null>(null),[manualMove,setManualMove]=useState<number|null>(null);
  const params=new URLSearchParams(window.location.hash.slice(1)),linked=useRef(params.has('searchEvent')&&params.get('decision')===frame.id&&params.get('game')===recording.id);
  const event=events[state.event],simulation=replay.simulations[event.simulation],choice=simulation.choices[event.choice];
  const graph=useMemo(()=>uctGraphAt(replay,event),[replay,event]);
  const topology=useMemo<GraphTopology>(()=>{
    const nodes=replay.nodes.map(n=>({actor:n.actor,actions:n.actions,children:n.actions.map(()=>[] as number[])}));
    for(const n of replay.nodes)if(n.parent)nodes[n.parent[0]].children[n.parent[1]].push(n.id);
    return {schema:replay.schema,nodes,simulations:replay.simulations.map(s=>({reason:s.backedUp?'natural':s.reason}))};
  },[replay]);
  const move=manualMove??(event.phase==='rollout'?event.move!:null),rollout=move===null?null:simulation.rollout[move];
  const action=rollout?replay.actions[rollout[4]]:choice?replay.nodes[choice.node].actions[choice.action]:null;
  const publicBoard=rollout?(event.after||manualMove!==null?rollout[8]??rollout[7]:rollout[7]):null;
  const fallback=choice?.board??replay.nodes[0].board;
  const board=previewNode!==null?graph.nodes.get(previewNode)?.board??replay.boards[fallback]:replay.boards[publicBoard??(event.phase==='evaluate'||event.phase==='backup'?simulation.leaf??fallback:fallback)];
  const treeMove=rollout&&simulation.choices.find(c=>c.step===rollout[1]&&c.physical===rollout[2]&&c.substep===rollout[3]&&c.live===action!.index);
  const currentNode=replay.nodes[choice?.node??0];
  const counts=useMemo(()=>{
    const visits=replay.nodes[0].actions.map(()=>0),wins=visits.slice();
    for(let i=0;i<=event.simulation;i++){
      const s=replay.simulations[i];if(!s.backedUp||i===event.simulation&&event.phase!=='backup')continue;
      const a=s.choices[0]?.action;if(a!==undefined){visits[a]++;wins[a]+=Number(s.reason==='win');}
    }
    return {visits,wins};
  },[replay,event.simulation,event.phase]);
  const phaseText=event.phase==='rollout'?(event.after?'Observe the transition':'Simulate a move'):event.phase==='select'?'Choose a tree branch':event.phase==='evaluate'?outcomes[simulation.reason]:simulation.backedUp?'Back up the natural result':'No backup for this attempt';
  useEffect(()=>{if(linked.current)document.getElementById('search-replay')?.scrollIntoView({behavior:'instant',block:'start'});},[]);
  useEffect(()=>{if(!state.playing)return;const timer=window.setTimeout(()=>dispatch({type:'advance',length:events.length}),(event.phase==='rollout'?1100:650)/state.speed);return()=>window.clearTimeout(timer);},[state,event.phase,events.length]);
  useEffect(()=>{
    setPreviewNode(null);setManualMove(null);const p=new URLSearchParams(window.location.hash.slice(1));p.set('game',recording.id);p.set('decision',frame.id);p.set('searchEvent',String(state.event));window.history.replaceState(null,'',`#${p}`);
  },[state.event,frame.id,recording.id]);
  useEffect(()=>{const listener=()=>{const p=new URLSearchParams(window.location.hash.slice(1)),n=Number(p.get('searchEvent')??0);if(p.get('game')===recording.id&&p.get('decision')===frame.id&&Number.isInteger(n)&&n>=0&&n<events.length)dispatch({type:'seek',event:n});};window.addEventListener('hashchange',listener);return()=>window.removeEventListener('hashchange',listener);},[recording.id,frame.id,events.length]);
  const seek=(n:number)=>{onInteract();dispatch({type:'seek',event:Math.max(0,Math.min(events.length-1,n))});};
  const interact=()=>{onInteract();if(state.playing)dispatch({type:'seek',event:state.event});};
  const start=(n:number)=>events.findIndex(e=>e.simulation===n);
  const firstCombo=replay.firstCombo;
  const rolloutLabels=simulation.combo&&event.phase!=='select'?simulation.rollout.flatMap((m,i)=>{
    if(event.phase==='rollout'&&(i>event.move!||i===event.move&&!event.after))return [];
    const policy=!simulation.choices.some(c=>c.step===m[1]);
    return policy?[(m[5]&2)?'Spy targets itself':null,(m[6]&2)?'Library milled':null,(m[5]&4)?'Dread Return targets Giant':null,(m[6]&4)?'Giant reanimated':null].filter((label):label is string=>label!==null):[];
  }):[];
  const scoreActions=currentNode.actions.map(a=>({action:a,score:choice?.scores[a.index]})).sort((a,b)=>a.action.index===choice?.action?-1:b.action.index===choice?.action?1:(b.score??-Infinity)-(a.score??-Infinity)).slice(0,5);
  const completion=(s:typeof simulation)=>s.backedUp?(event.phase==='backup'?'Natural result backed up':'Natural result ready for backup'):'Attempt retained without a backup';
  return <section id="search-replay" className="search-replay-section" aria-label="Recorded full UCT search replay">
    <div className="search-title"><div><span className="eyebrow"><GitBranch size={13}/>INSIDE THE SPY SEARCH</span><h2>Follow the discovery.</h2><p>Actual tree choices, simulated moves, and natural outcomes from a fresh game.</p><p className="search-selection-summary">Persistent UCT · {replay.simulations.length} attempts · {frame.search!.simulations} natural backups · {frame.search!.transitions.toLocaleString()} transitions</p></div><span className="search-recorded">FULL TRACE RECORDED</span></div>
    {firstCombo!==null?<div className="search-discovery"><div><Sparkles size={19}/><strong>First winning combo: attempt {firstCombo+1}</strong><span>Self-mill → Dread Return → Lotleth Giant → natural win. Recorded simulation; hypothetical game.</span></div><button onClick={()=>{seek(start(firstCombo));dispatch({type:'toggle'});}}>Watch its discovery <ArrowRight size={15}/></button></div>:<p className="search-replay-missing">No completed winning combo rollout was recorded in this search.</p>}
    <div className="search-replay-layout"><div className={`search-theater ${state.playing?'search-running':'search-static'}`}>
      <div className="search-phase"><span className={`search-phase-dot ${event.phase==='rollout'?'select':event.phase}`}/><strong>{phaseText}</strong><span>Attempt {event.simulation+1} / {replay.simulations.length}</span></div>
      <SearchGraphCanvas topology={topology} graph={graph} event={{...event,phase:event.phase==='rollout'?'select':event.phase}} recording={recording} playing={state.playing&&!(event.phase==='backup'&&!simulation.backedUp)} speed={state.speed} epoch={state.epoch} selected={previewNode} onSelect={setPreviewNode} onInteract={interact} onComplete={()=>seek(events.length-1)} uct rolloutLabels={rolloutLabels}/>
      <div className="search-branches search-root-options">{replay.nodes[0].actions.map((a,i)=><button key={i} className={`search-branch ${simulation.choices[0]?.action===i?'active':''}`} onClick={()=>{const n=replay.simulations.findIndex(s=>s.choices[0]?.action===i);if(n>=0)seek(start(n));}}><span className="search-branch-content"><span className="search-action-id">MOVE {replay.nodes[0].liveOrder[i]}{replay.nodes[0].liveOrder[i]===frame.executed?' · PLAYED':''}</span><strong>{clean(a.label)}</strong><span className="search-branch-values">{counts.visits[i]} visits · {counts.wins[i]} recorded wins</span><span className="search-visit-track"><span style={{width:`${counts.visits[i]/(graph.completed||1)*100}%`}}/></span></span></button>)}</div>
      <div className="search-path" aria-label="Recorded tree path">{simulation.choices.map((c,i)=><button key={i} className={event.phase==='select'&&event.choice===i?'active':''} onClick={()=>seek(events.findIndex(e=>e.simulation===event.simulation&&e.phase==='select'&&e.choice===i))}><span>Tree node {c.node} · step {c.step}</span><strong>{clean(replay.nodes[c.node].actions[c.action].label)}</strong><ArrowRight size={12}/></button>)}<span className="search-leaf"><span>Policy rollout</span><strong>{simulation.rollout.length} applied moves</strong></span></div>
      <Preview board={board} recording={recording} assetBase={assetBase}/>
      <div className="search-zone-counts">{board.players.map((p,i)=><span key={i}>{recording.seats[i].name}: {p.libraryCount} in library · {p.graveyard.length} in graveyard</span>)}</div>
      <div className="search-caption" aria-live="polite">{rollout?<><strong>{event.after?'After: ':'Move: '}{clean(action!.label)}</strong><span>{treeMove?'Selected at a UCT tree node':'Policy rollout choice'} · {recording.seats[rollout[0]].name} · step {rollout[1]}.{(rollout[6]&2)!==0&&event.after?' Self-targeted Spy resolved.':''}{(rollout[6]&4)!==0&&event.after?' Dread Return reanimated Lotleth Giant.':''}{publicBoard===null?' No snapshot was retained for this routine move; preview shows the nearest recorded tree context.':''}</span></>:event.phase==='select'?`${clean(action?.label??'Sample rejected')} · actual selection score ${choice?.scores[choice.action]??'Unavailable'}${choice?.expanded?' · proposes a new branch':''}`:`${outcomes[simulation.reason]}. ${completion(simulation)}.`}</div>
      <div className="search-transport"><div className="search-buttons"><button aria-label="Previous search event" disabled={state.event===0} onClick={()=>seek(state.event-1)}><ArrowLeft size={16}/></button><button className="search-play" aria-label={state.playing?'Pause search replay':'Play search replay'} onClick={()=>{onInteract();if(state.event===events.length-1)dispatch({type:'seek',event:0});dispatch({type:'toggle'});}}>{state.playing?<Pause size={17}/>:<Play size={17}/>}</button><button aria-label="Next search event" disabled={state.event===events.length-1} onClick={()=>seek(state.event+1)}><ArrowRight size={16}/></button><span>Event {state.event+1} / {events.length}</span><label>Speed <select aria-label="Search replay speed" value={state.speed} onChange={e=>dispatch({type:'speed',speed:Number(e.target.value)})}>{[.5,1,2,4].map(n=><option key={n} value={n}>{n}×</option>)}</select></label></div><input aria-label="Search event timeline" type="range" min="0" max={events.length-1} value={state.event} onChange={e=>seek(Number(e.target.value))}/><div className="search-simulation-nav"><button disabled={event.simulation===0} onClick={()=>seek(start(event.simulation-1))}>Previous attempt</button><span>Share of search visits</span><button disabled={event.simulation===replay.simulations.length-1} onClick={()=>seek(start(event.simulation+1))}>Next attempt</button></div></div>
      <details className="search-rollout-inspector"><summary>Inspect every simulated move ({simulation.rollout.length})</summary><label>Recorded move <select aria-label="Inspect rollout move" value={manualMove??''} onChange={e=>{interact();setPreviewNode(null);setManualMove(e.target.value===''?null:Number(e.target.value));}}><option value="">Follow search playback</option>{simulation.rollout.map((m,i)=><option value={i} key={i}>{i+1}. {recording.seats[m[0]].name} · {clean(replay.actions[m[4]].label)}</option>)}</select></label><p>Every applied action is preserved. Public snapshots accompany tree choices, combo events and natural outcomes. Routine actions without snapshots retain their original step and action IDs.</p></details>
    </div><aside className="search-explanation"><span className="eyebrow">WHY THIS BRANCH?</span><h3>Focal UCT selection</h3><p>The recorded score combines observed rollout results with exploration. Unvisited moves use first-play urgency 1.5. The greatest score is selected.</p><div className="search-score-list">{scoreActions.map(({action,score})=><div key={action.index} className={choice?.action===action.index?'chosen':''}><span className="search-action-id">CANONICAL MOVE {action.index}</span><strong>{clean(action.label)}</strong><dl><dt>Actual selection score</dt><dd>{score?.toPrecision(6)??'Not scored'}</dd></dl></div>)}</div><details open><summary>Tree and rollout</summary><p>A simulation selects focal branches in this tree. After its expansion or depth limit, frozen policies play the remaining moves. Opponent choices and forced moves happen between tree nodes.</p><p>Only natural results add visits and wins. Draws and losses both add a non-winning backup. Limited and rejected attempts remain visible without changing the counts.</p><p>{replay.worlds}</p><p>{frame.search!.valueScale} These recorded outcomes are not a forecast of the real game.</p></details></aside></div>
  </section>;
}
