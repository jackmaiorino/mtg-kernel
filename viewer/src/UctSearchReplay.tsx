import { useEffect, useMemo, useReducer, useRef, useState } from 'react';
import { ArrowLeft, ArrowRight, Check, ChevronsLeft, ChevronsRight, Circle, GitBranch, Pause, Play, Sparkles } from 'lucide-react';
import type { Frame, Recording } from './recording';
import { initialSearchPlayback, searchReducer } from './search-playback';
import { actualTreePath, uctEvents, uctGraphAt } from './uct-playback';
import { attemptSummaries, comboProgress, comboSteps, shortLabel, type AttemptSummary } from './uct-console';
import { SearchGraphCanvas } from './SearchGraph';
import type { GraphTopology } from './search-graph';
import { Preview } from './SearchReplay';
import './search-replay.css';
import './uct-search.css';

const outcomes={win:'Natural win',natural_nonwin:'Natural result without a focal win',nonnatural:'Non-natural termination',truncated:'Search limit reached',fault:'Simulation fault',rejected:'Sampled world rejected'};
const headlines={win:'Won this game',natural_nonwin:'Did not win this game',nonnatural:'Ended without a natural result',truncated:'Stopped at the search limit',fault:'Stopped by a simulation fault',rejected:'Sampled world rejected'};
const clean=(s:string)=>s.replace(/\s*\[c\d+\]/g,'');
const steps=['Choose moves in the tree','Play out the rest of the game','Read the result','Update the counts'];
const stepOf={select:0,rollout:1,evaluate:2,backup:3};

// One column per attempt, aligned with the event slider laid over it:
// wins are tall green ticks, combo wins gold, and the line counts wins so far.
function AttemptStrip({attempts,length,current}:{attempts:AttemptSummary[];length:number;current:number}) {
  const span=Math.max(1,length-1),total=Math.max(1,attempts.at(-1)!.wins),minimum=span/300;
  const line=attempts.map(a=>`L ${a.start} ${40-a.wins/total*30} L ${a.start+a.events} ${40-a.wins/total*30}`).join(' ');
  const here=attempts[current];
  return <svg className="uct-strip" viewBox={`0 0 ${span} 40`} preserveAspectRatio="none" aria-hidden="true">
    <rect className="uct-strip-track" x="0" y="0" width={span} height="40"/>
    <rect className="uct-strip-current" x={here.start} y="0" width={Math.max(minimum,here.events)} height="40"/>
    <path className="uct-strip-wins" d={`M 0 40 ${line} L ${span} 40 Z`}/>
    {attempts.map((a,i)=>a.win||a.reason!=='natural_nonwin'?<rect key={i} className={a.combo?'combo':a.win?'win':'stopped'} x={a.start} y={a.win?2:28} width={Math.max(minimum,a.events)} height={a.win?38:12}/>:null)}
  </svg>;
}

export function UctSearchReplayView({frame,actualFrame=frame,recording,assetBase,onInteract}:{frame:Frame;actualFrame?:Frame;recording:Recording;assetBase:string;onInteract:()=>void}) {
  const replay=frame.search!.uctReplay!,events=useMemo(()=>uctEvents(replay),[replay]);
  const attempts=useMemo(()=>attemptSummaries(replay,events),[replay,events]);
  const [state,dispatch]=useReducer(searchReducer,initialSearchPlayback,s=>{
    const p=new URLSearchParams(window.location.hash.slice(1)),event=Number(p.get('searchEvent')??0);
    return {...s,event:p.get('decision')===actualFrame.id&&p.get('game')===recording.id&&Number.isInteger(event)&&event>=0&&event<events.length?event:0};
  });
  const [showActual,setShowActual]=useState(()=>{const p=new URLSearchParams(window.location.hash.slice(1));return p.get('searchView')==='actual'||actualFrame.id!==frame.id&&!(p.get('decision')===actualFrame.id&&p.has('searchEvent'));});
  const execution=actualFrame.searchLifecycle;
  const actual=execution?.root===frame.id&&execution.node!==null?{node:execution.node,path:actualTreePath(replay,execution.node)}:undefined;
  const [previewNode,setPreviewNode]=useState<number|null>(null),[manualMove,setManualMove]=useState<number|null>(null);
  const params=new URLSearchParams(window.location.hash.slice(1)),linked=useRef(params.has('searchEvent')&&params.get('decision')===actualFrame.id&&params.get('game')===recording.id);
  const event=events[state.event],simulation=replay.simulations[event.simulation],choice=simulation.choices[event.choice];
  const graph=useMemo(()=>uctGraphAt(replay,event),[replay,event]);
  const completeGraph=useMemo(()=>uctGraphAt(replay,events.at(-1)!),[replay,events]);
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
  const counts=useMemo(()=>{
    const visits=replay.nodes[0].actions.map(()=>0),wins=visits.slice();
    for(let i=0;i<=event.simulation;i++){
      const s=replay.simulations[i];if(!s.backedUp||i===event.simulation&&event.phase!=='backup')continue;
      const a=s.choices[0]?.action;if(a!==undefined){visits[a]++;wins[a]+=Number(s.reason==='win');}
    }
    return {visits,wins};
  },[replay,event.simulation,event.phase]);
  const root=replay.nodes[0],displayedCounts=showActual?{visits:root.edgeVisits,wins:root.edgeWins}:counts;
  const order=useMemo(()=>root.actions.map((_,i)=>i).sort((a,b)=>root.edgeVisits[b]-root.edgeVisits[a]||a-b),[root]);
  const mostVisits=Math.max(1,...displayedCounts.visits);
  const actualRootAction=actual&&actual.path.length>1?replay.nodes[actual.path[1]].parent![1]:execution?.action;
  const currentRoot=showActual?actualRootAction:simulation.choices[0]?.action;
  const phaseText=event.phase==='rollout'?(event.after?'Observe the transition':'Simulate a move'):event.phase==='select'?'Choose a tree branch':event.phase==='evaluate'?outcomes[simulation.reason]:simulation.backedUp?'Back up the natural result':'No backup for this attempt';
  useEffect(()=>{if(linked.current)document.getElementById('search-replay')?.scrollIntoView({behavior:'instant',block:'start'});},[]);
  useEffect(()=>{if(!state.playing)return;const timer=window.setTimeout(()=>dispatch({type:'advance',length:events.length}),(event.phase==='rollout'?1100:650)/state.speed);return()=>window.clearTimeout(timer);},[state,event.phase,events.length]);
  useEffect(()=>{
    setPreviewNode(null);setManualMove(null);const p=new URLSearchParams(window.location.hash.slice(1));p.set('game',recording.id);p.set('decision',actualFrame.id);p.set('searchEvent',String(state.event));if(showActual)p.set('searchView','actual');else p.delete('searchView');window.history.replaceState(null,'',`#${p}`);
  },[state.event,actualFrame.id,recording.id,showActual]);
  useEffect(()=>{const listener=()=>{const p=new URLSearchParams(window.location.hash.slice(1)),n=Number(p.get('searchEvent')??0);if(p.get('game')===recording.id&&p.get('decision')===actualFrame.id&&Number.isInteger(n)&&n>=0&&n<events.length){setShowActual(p.get('searchView')==='actual');dispatch({type:'seek',event:n});}};window.addEventListener('hashchange',listener);return()=>window.removeEventListener('hashchange',listener);},[recording.id,actualFrame.id,events.length]);
  const seek=(n:number)=>{onInteract();setShowActual(false);dispatch({type:'seek',event:Math.max(0,Math.min(events.length-1,n))});};
  const interact=()=>{onInteract();if(state.playing)dispatch({type:'seek',event:state.event});};
  const start=(n:number)=>attempts[n].start;
  const firstCombo=replay.firstCombo;
  const rolloutLabels=simulation.combo&&event.phase!=='select'?simulation.rollout.flatMap((m,i)=>{
    if(event.phase==='rollout'&&(i>event.move!||i===event.move&&!event.after))return [];
    const policy=!simulation.choices.some(c=>c.step===m[1]);
    return policy?[(m[5]&2)?'Spy targets itself':null,(m[6]&2)?'Library milled':null,(m[5]&4)?'Dread Return targets Giant':null,(m[6]&4)?'Giant reanimated':null].filter((label):label is string=>label!==null):[];
  }):[];
  const completion=(s:typeof simulation)=>s.backedUp?(event.phase==='backup'?'Natural result backed up':'Natural result ready for backup'):'Attempt retained without a backup';
  const step=stepOf[event.phase],stage=comboProgress(replay,event,recording.perspective==='p0'?0:1),resolved=step>=2;
  const stepState=(i:number)=>i<step?'done':i===step?'current':'pending';
  const update=!simulation.backedUp?'Not counted. Stopped or rejected attempts leave every count unchanged.':`${step===3?'Added':'Adds'} one try${simulation.reason==='win'?' and one win':''} to each of the ${simulation.choices.length} tree ${simulation.choices.length===1?'move':'moves'} on this path.`;
  const caption=<div className="search-caption" aria-live="polite">{rollout?<><strong>{event.after?'After: ':'Move: '}{clean(action!.label)}</strong><span>{treeMove?'Selected at a UCT tree node':'Policy rollout choice'} · {recording.seats[rollout[0]].name} · step {rollout[1]}.{(rollout[6]&2)!==0&&event.after?' Self-targeted Spy resolved.':''}{(rollout[6]&4)!==0&&event.after?' Dread Return reanimated Lotleth Giant.':''}{publicBoard===null?' No snapshot was retained for this routine move; preview shows the nearest recorded tree context.':''}</span></>:event.phase==='select'?`${clean(action?.label??'Sample rejected')} · actual selection score ${choice?.scores[choice.action]??'Unavailable'}${choice?.expanded?' · proposes a new branch':''}`:`${outcomes[simulation.reason]}. ${completion(simulation)}.`}</div>;
  return <section id="search-replay" className="search-replay-section uct-console" aria-label="Recorded full UCT search replay">
    <header className="uct-head">
      <div className="uct-head-text"><span className="eyebrow"><GitBranch size={13}/>INSIDE THE SPY SEARCH</span><h2>Follow the discovery.</h2>
        <p>From this position the search plays {replay.simulations.length} hypothetical games. Each attempt picks moves in a tree, lets the frozen policies finish the game, then credits every tree move on its path if it won. Moves that win more get tried more.</p>
        <p className="search-selection-summary">Persistent UCT · {replay.simulations.length} attempts · {frame.search!.simulations} natural backups · {frame.search!.transitions.toLocaleString()} transitions · full trace recorded</p></div>
      <div className="uct-head-actions">
        {firstCombo!==null?<div className="search-discovery"><Sparkles size={18}/><div className="uct-discovery-text"><strong>First winning combo: attempt {firstCombo+1}</strong><span>Self-mill → Dread Return → Lotleth Giant → natural win. A recorded simulation of a hypothetical game.</span></div><button onClick={()=>{seek(start(firstCombo));dispatch({type:'toggle'});}}>Watch its discovery <ArrowRight size={15}/></button></div>:<p className="search-replay-missing">No completed winning combo rollout was recorded in this search.</p>}
        {actual&&<div className="actual-tree-heading"><span><strong>Actual game: decision {actualFrame.key.step+1} · node {actual.node}</strong>{!showActual&&!graph.nodes.has(actual.node)?' Not discovered yet at this point of the search.':''}</span><button onClick={()=>{interact();setPreviewNode(null);setShowActual(!showActual);}}>{showActual?'Return to search discovery':'Locate actual game in tree'}</button></div>}
      </div>
    </header>
    <div className="uct-transport">
      <div className="search-buttons">
        <button aria-label="Previous attempt" disabled={event.simulation===0} onClick={()=>seek(start(event.simulation-1))}><ChevronsLeft size={16}/></button>
        <button aria-label="Previous search event" disabled={state.event===0} onClick={()=>seek(state.event-1)}><ArrowLeft size={16}/></button>
        <button className="search-play" aria-label={state.playing?'Pause search replay':'Play search replay'} onClick={()=>{onInteract();setShowActual(false);if(state.event===events.length-1)dispatch({type:'seek',event:0});dispatch({type:'toggle'});}}>{state.playing?<Pause size={17}/>:<Play size={17}/>}</button>
        <button aria-label="Next search event" disabled={state.event===events.length-1} onClick={()=>seek(state.event+1)}><ArrowRight size={16}/></button>
        <button aria-label="Next attempt" disabled={event.simulation===replay.simulations.length-1} onClick={()=>seek(start(event.simulation+1))}><ChevronsRight size={16}/></button>
        <div className="search-phase"><span className={`search-phase-dot ${event.phase==='rollout'?'select':event.phase}`}/><strong>{showActual?'Recorded actual game route':phaseText}</strong><span>{showActual?'Completed source tree':`Attempt ${event.simulation+1} / ${replay.simulations.length}`}</span></div>
        <span className="uct-tally"><b>{showActual?attempts.at(-1)!.wins:attempts[event.simulation].wins-Number(simulation.reason==='win'&&step<2)}</b> {showActual?'wins in total':'wins so far'}</span>
        <span className="uct-event-count">Event {state.event+1} / {events.length}</span>
        <label>Speed <select aria-label="Search replay speed" value={state.speed} onChange={e=>dispatch({type:'speed',speed:Number(e.target.value)})}>{[.5,1,2,4,8].map(n=><option key={n} value={n}>{n}×</option>)}</select></label>
      </div>
      <div className="uct-timeline">
        <AttemptStrip attempts={attempts} length={events.length} current={event.simulation}/>
        {firstCombo!==null&&<span className="uct-strip-flag" style={{left:`${start(firstCombo)/Math.max(1,events.length-1)*100}%`}}><Sparkles size={11}/>First combo</span>}
        <input aria-label="Search event timeline" type="range" min="0" max={events.length-1} value={state.event} onChange={e=>seek(Number(e.target.value))}/>
      </div>
      <div className="uct-strip-key"><span className="win">Won</span><span className="combo">Won with the combo</span><span className="stopped">Stopped</span><span className="tally">Running total of wins</span><span className="drag">Drag across the strip to scrub through all {replay.simulations.length} attempts</span></div>
    </div>
    <div className={`uct-main ${state.playing?'search-running':'search-static'}`}>
      <aside className="uct-panel uct-moves" aria-label="First move options">
        <span className="eyebrow">FIRST MOVE OPTIONS</span><h3>Where the attempts went</h3>
        <p>Each attempt starts with one of these {root.actions.length} moves. Bar: tries so far. Gold line: wins. Numbers: wins / tries.</p>
        <ol className="search-root-options">{order.map(i=>{const a=root.actions[i],visits=displayedCounts.visits[i],wins=displayedCounts.wins[i],played=root.liveOrder[i]===frame.executed;
          return <li key={i}><button className={`uct-move ${currentRoot===i?'active':''}`} title={`${clean(a.label)}: ${visits} tries, ${wins} wins`} onClick={()=>{const n=replay.simulations.findIndex(s=>s.choices[0]?.action===i);if(n>=0)seek(start(n));}}>
            <span className="uct-move-fill" style={{width:`${visits/mostVisits*100}%`}}/><span className="uct-move-won" style={{width:`${wins/mostVisits*100}%`}}/>
            <strong>{shortLabel(a.label)}</strong>{played&&<span className="uct-tag">Played</span>}
            <span className="search-branch-values">{wins}/{visits}{visits?` · ${Math.round(wins/visits*100)}%`:''}</span>
          </button></li>;})}</ol>
        <p className="uct-panel-note">Listed by final number of tries. Click a move to jump to its first attempt.</p>
      </aside>
      <div className="uct-center">
        <SearchGraphCanvas compact topology={topology} graph={showActual?{...completeGraph,activeNodes:[],activeEdges:new Set()}:graph} event={{...event,phase:event.phase==='rollout'?'select':event.phase}} recording={recording} playing={!showActual&&state.playing&&!(event.phase==='backup'&&!simulation.backedUp)} speed={state.speed} epoch={state.epoch} selected={previewNode} onSelect={setPreviewNode} onInteract={interact} onComplete={()=>seek(events.length-1)} uct rolloutLabels={showActual?[]:rolloutLabels} actual={actual} actualRoute={showActual}/>
        <div className="uct-board"><div className="uct-board-head"><span className="eyebrow">{showActual?'ACTUAL GAME TREE CONTEXT':previewNode!==null?`POSITION ${previewNode}`:'THIS ATTEMPT’S HYPOTHETICAL BOARD'}</span><span>{board.players.map((p,i)=>`${recording.seats[i].name}: ${p.libraryCount} library · ${p.graveyard.length} graveyard`).join('  |  ')}</span></div><Preview board={board} recording={recording} assetBase={assetBase}/></div>
      </div>
      <aside className="uct-panel uct-attempt search-explanation" aria-label="This attempt">{showActual&&actual?<>
        <span className="eyebrow">WHY THIS ACTUAL MOVE?</span><h3>A supported tree choice</h3><strong className="uct-actual-move">{clean(actualFrame.actions[actualFrame.executed].label)}</strong>
        <p>The actual game plays the move with the best observed win rate among moves with at least eight completed rollouts. Exploration scores belong to the simulations, so none is shown here.</p>
        <p>{replay.nodes[actual.node].edgeWins[execution!.action!]} observed wins / {execution!.backups} completed rollouts · node {actual.node} · canonical move {execution!.action} · original live move {actualFrame.executed}.</p>
        <p>{execution!.mode==='reuse'?'The existing tree supplied this move. No new simulations ran at this decision.':'This search completed before the actual move was selected.'}</p>
        <div className="search-path" aria-label="Recorded actual tree route">{actual.path.map((id,i)=>{const a=i<actual.path.length-1?replay.nodes[actual.path[i+1]].parent![1]:execution!.action!;return <button key={id} onClick={()=>setPreviewNode(id)}><span>Actual route · node {id}</span><strong>{i===actual.path.length-1?clean(actualFrame.actions[actualFrame.executed].label):clean(replay.nodes[id].actions[a].label)}</strong><ArrowRight size={12}/></button>;})}</div>
        <p className="search-caption">The battlefield at the top of the page shows the actual game. This completed search supplied the recorded choice at node {actual.node}; no hypothetical board replaces the actual state.</p>
      </>:<>
        <span className="eyebrow">ATTEMPT {event.simulation+1} OF {replay.simulations.length}</span>
        <h3 className={resolved?`uct-result ${simulation.reason}`:''}>{resolved?headlines[simulation.reason]:'In progress'}{resolved&&simulation.combo&&simulation.reason==='win'?' with the combo':''}</h3>
        <ol className="uct-steps">
          <li className={stepState(0)}><strong><StepMark state={stepState(0)}/>1 · {steps[0]}</strong><p>At each tree position, take the highest-scoring move: win rate so far plus a bonus for moves tried less. Untried moves score 1.5, so each gets tried once first.</p>
            <div className="search-path" aria-label="Recorded tree path">{simulation.choices.map((c,i)=><button key={i} className={event.phase==='select'&&event.choice===i?'active':''} onClick={()=>seek(events.findIndex(e=>e.simulation===event.simulation&&e.phase==='select'&&e.choice===i))}><span>Tree node {c.node} · step {c.step}</span><strong title={clean(replay.nodes[c.node].actions[c.action].label)}>{shortLabel(replay.nodes[c.node].actions[c.action].label)}</strong></button>)}</div></li>
          <li className={stepState(1)}><strong><StepMark state={stepState(1)}/>2 · {steps[1]}</strong><p>The frozen policies play the remaining {simulation.rollout.length} moves for both players.</p>
            <ul className="uct-combo" aria-label="Combo progress in this attempt">{comboSteps.map((label,i)=><li key={label} className={i<stage?'reached':''}>{i<stage?<Check size={12}/>:<Circle size={10}/>}{label}</li>)}</ul></li>
          <li className={stepState(2)}><strong><StepMark state={stepState(2)}/>3 · {steps[2]}</strong><p>{resolved?`${outcomes[simulation.reason]}.`:'Pending until the game ends.'}</p></li>
          <li className={stepState(3)}><strong><StepMark state={stepState(3)}/>4 · {steps[3]}</strong><p>{update}</p></li>
        </ol>
        {caption}
        <details className="search-rollout-inspector"><summary>Inspect every simulated move ({simulation.rollout.length})</summary><label>Recorded move <select aria-label="Inspect rollout move" value={manualMove??''} onChange={e=>{interact();setPreviewNode(null);setManualMove(e.target.value===''?null:Number(e.target.value));}}><option value="">Follow search playback</option>{simulation.rollout.map((m,i)=><option value={i} key={i}>{i+1}. {recording.seats[m[0]].name} · {clean(replay.actions[m[4]].label)}</option>)}</select></label><p>Every applied action is preserved. Public snapshots accompany tree choices, combo events and natural outcomes. Routine actions without snapshots retain their original step and action IDs.</p></details>
        <details className="uct-howto"><summary>More on how this search works</summary><p>A simulation selects focal branches in this tree. After its expansion or depth limit, frozen policies play the remaining moves. Opponent choices and forced moves happen between tree nodes.</p><p>Only natural results add visits and wins. Draws and losses both add a non-winning backup. Limited and rejected attempts remain visible without changing the counts.</p><p>{replay.worlds}</p><p>{frame.search!.valueScale} These recorded outcomes are not a forecast of the real game.</p></details>
      </>}</aside>
    </div>
  </section>;
}

function StepMark({state}:{state:string}) {
  return <span className={`uct-step-mark ${state}`} aria-hidden="true">{state==='done'?<Check size={10}/>:null}</span>;
}
