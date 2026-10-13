import { useEffect, useMemo, useRef, useState } from 'react';
import { motion, useReducedMotion } from 'motion/react';
import { Focus, Maximize2, Minus, Plus } from 'lucide-react';
import type { Recording, SearchReplay } from './recording';
import type { SearchEvent } from './search-playback';
import { graphAt, rootChoicesAt, rootChoiceStatus } from './search-graph';
import { actionsAt, actionKey, actionLayout, actionStatus, responseSummary, responseRecords, type ActionRef } from './search-actions';

const clean=(s:string)=>s.replace(/\s*\[c\d+\]/g,'');
export function SearchGraph({replay,event,recording,playing,speed,epoch,selected,selectedRoot,onSelect,onSelectRoot,onSelectEvaluation,onComplete,onInteract}:{
  replay:SearchReplay;event:SearchEvent;recording:Recording;playing:boolean;speed:number;epoch:number;
  selected:number|null;selectedRoot:number|null;onSelect:(id:number|null)=>void;onSelectRoot:(index:number)=>void;onSelectEvaluation:(ref:ActionRef)=>void;onComplete:()=>void;onInteract:()=>void;
}) {
  const graph=useMemo(()=>graphAt(replay,event),[replay,event]),model=useMemo(()=>actionsAt(replay,event,graph),[replay,event,graph]);
  const layout=useMemo(()=>actionLayout(model,graph),[model,graph]),roots=useMemo(()=>rootChoicesAt(replay,event,graph),[replay,event,graph]);
  const container=useRef<HTMLDivElement>(null),drag=useRef<{x:number;y:number;left:number;top:number}|null>(null);
  const [scale,setScale]=useState(1),[focus,setFocus]=useState(0),[inspected,setInspected]=useState<ActionRef|null>(null);
  const reduced=useReducedMotion(),simulation=replay.simulations[event.simulation];
  const actor=(node:number)=>replay.nodes[node].actor===recording.perspective?'Agent':'Opponent';
  const active=new Set(graph.activeNodes),activeActions=new Set(simulation.choices.slice(0,event.phase==='select'?event.choice+1:undefined).map(c=>actionKey(c.node,c.action)));
  const center=(id:string,zoom=scale)=>{const el=container.current,p=layout.items.get(id);if(el&&p)el.scrollTo({left:p.x*zoom-el.clientWidth/2,top:p.y*zoom-el.clientHeight/2,behavior:'instant'});};
  useEffect(()=>{setInspected(null);},[event.simulation,event.phase,event.choice]);
  useEffect(()=>{if(selectedRoot!==null)setInspected({node:0,index:selectedRoot});},[selectedRoot]);
  useEffect(()=>{const el=container.current;if(!el)return;if(event.simulation===0&&event.phase==='select'&&event.choice===0){el.scrollLeft=layout.width*scale/2-el.clientWidth/2;el.scrollTop=0;}else if(playing){const id=layout.destinations.get(graph.activeNodes.at(-1)!);if(id)center(id);}else el.scrollLeft=layout.width*scale/2-el.clientWidth/2;},[event.simulation,event.phase,event.choice,playing,layout.width,scale]);
  useEffect(()=>{if(!graph.nodes.has(focus))setFocus(0);},[graph,focus]);
  const inspectNode=(id:number)=>{onInteract();onSelect(id);setFocus(id);};
  const inspectAction=(ref:ActionRef)=>{onInteract();setInspected(ref);const evidence=model.actions.get(actionKey(ref.node,ref.index));if(ref.node===0)onSelectRoot(ref.index);else if(evidence?.firstEvaluation&&!evidence.children.length)onSelectEvaluation(ref);else onSelect(ref.node);};
  const keySelect=(e:React.KeyboardEvent,choose:()=>void)=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();e.stopPropagation();choose();}};
  const moveFocus=(id:number,key:string)=>{
    const same=[...graph.nodes.keys()].filter(n=>model.depths.get(n)===model.depths.get(id)&&layout.items.has(`n${n}`));
    const adjacent=key==='ArrowDown'?[...graph.edges.values()].filter(e=>e.from===id).map(e=>e.to):[...graph.edges.values()].filter(e=>e.to===id).map(e=>e.from);
    const next=key==='ArrowLeft'?same[Math.max(0,same.indexOf(id)-1)]:key==='ArrowRight'?same[Math.min(same.length-1,same.indexOf(id)+1)]:adjacent[0]??id;
    const destination=layout.destinations.get(next);if(destination){setFocus(next);center(destination);container.current?.querySelector<SVGGElement>(`[data-item="${destination}"]`)?.focus();}
  };
  const zoom=(factor:number)=>{onInteract();const next=Math.max(.2,Math.min(2.5,scale*factor));setScale(next);window.requestAnimationFrame(()=>center(layout.destinations.get(selected??focus)??'n0',next));};
  const fit=()=>{onInteract();const el=container.current;if(!el)return;setScale(Math.min(1,el.clientWidth/layout.width,el.clientHeight/layout.height));el.scrollTo(0,0);};
  const chosen=inspected?model.actions.get(actionKey(inspected.node,inspected.index)):null;
  const root=selectedRoot===null?null:roots[selectedRoot],selectedNode=selected===null?null:graph.nodes.get(selected);
  const path=simulation.choices.slice(0,event.phase==='select'?event.choice+1:undefined).flatMap(c=>[layout.items.get(layout.destinations.get(c.node)!)!,layout.items.get(`a${actionKey(c.node,c.action)}`)!]).filter(Boolean);
  if(event.phase!=='select'){const last=simulation.choices.at(-1)!,destination=simulation.leafNode==null?`e${actionKey(last.node,last.action)}`:layout.destinations.get(simulation.leafNode)!;const p=layout.items.get(destination);if(p)path.push(p);}
  const moving=event.phase==='backup'?path.slice().reverse():path.slice(-2);
  const curve=(from:{x:number;y:number},to:{x:number;y:number})=>`M ${from.x} ${from.y+54} C ${from.x} ${(from.y+to.y)/2} ${to.x} ${(from.y+to.y)/2} ${to.x} ${to.y-54}`;
  const connections=[...model.actions.values()].flatMap(a=>{
    const from=layout.items.get(`n${a.node}`)!,action=layout.items.get(`a${a.key}`)!;
    const links=[{id:`choice:${a.key}`,from,to:action,active:activeActions.has(a.key),title:clean(replay.nodes[a.node].actions[a.index].label)}];
    for(const destination of new Set(a.children.map(id=>layout.destinations.get(id)!))){const to=layout.items.get(destination)!,member=a.children.find(n=>layout.destinations.get(n)===destination)!;
      links.push({id:destination.startsWith('g')?`group:${a.key}`:`${a.node}:${a.index}:${member}`,from:action,to,active:activeActions.has(a.key)&&a.children.some(n=>active.has(n)&&layout.destinations.get(n)===destination),title:`${clean(replay.nodes[a.node].actions[a.index].label)} → ${destination.startsWith('g')?`${model.groups.get(a.key)!.length} sampled positions`:`position ${member}`}`});
    }
    const evaluation=layout.items.get(`e${a.key}`);if(evaluation)links.push({id:`evaluation:${a.key}`,from:action,to:evaluation,active:activeActions.has(a.key),title:'Recorded evaluation without a stored child'});return links;
  });
  return <div className="search-graph">
    <div className="search-graph-heading"><div><span className="eyebrow">EXPLORED FUTURES</span><h3>Follow the choices through each future.</h3></div><button className="search-complete-graph" onClick={onComplete}>See complete graph</button></div>
    <div className="search-graph-metrics"><span><b>{graph.nodes.size}</b> / {replay.nodes.length} positions discovered</span><span><b>{Math.max(...model.depths.values())}</b> decisions deep</span><span><b>{graph.completed}</b> simulations backed up</span></div>
    <p className="search-graph-guide">Position → action → resulting position. Scroll to follow deeper choices; select an action to compare its sampled outcomes.</p>
    <div className="search-action-canvas" ref={container}>
      <svg width={layout.width*scale} height={layout.height*scale} viewBox={`0 0 ${layout.width} ${layout.height}`} aria-label="Accumulated explored search graph" role="group"
        onPointerDown={e=>{if(e.pointerType==='touch'||(e.target as Element).closest('[role=button]'))return;onInteract();const el=container.current!;drag.current={x:e.clientX,y:e.clientY,left:el.scrollLeft,top:el.scrollTop};e.currentTarget.setPointerCapture(e.pointerId);}}
        onPointerMove={e=>{const d=drag.current,el=container.current;if(d&&el){el.scrollLeft=d.left-(e.clientX-d.x);el.scrollTop=d.top-(e.clientY-d.y);}}}
        onPointerUp={()=>{drag.current=null;}} onPointerCancel={()=>{drag.current=null;}}>
        {connections.map(c=><path key={c.id} data-edge={c.id} className={`search-graph-edge ${c.active?event.phase==='backup'?'backup':'active':''}`} d={curve(c.from,c.to)}><title>{c.title}</title></path>)}
        {[...layout.items.values()].map(item=>{
          const node=replay.nodes[item.node],side=actor(item.node),position=graph.nodes.get(item.node),a=item.index===undefined?null:model.actions.get(actionKey(item.node,item.index))!;
          const observed=model.menus.has(item.node),selectedAction=inspected?.node===item.node&&inspected.index===item.index||item.node===0&&selectedRoot===item.index;
          const isActive=item.kind==='position'?active.has(item.node):item.kind==='group'?item.members!.some(id=>active.has(id)):activeActions.has(a!.key);
          const choose=()=>item.kind==='position'?inspectNode(item.node):inspectAction(a!);
          const label=item.kind==='position'?`Inspect node ${item.node}: ${side}`:item.kind==='group'?`Inspect ${item.members!.length} sampled positions after action ${item.index} at position ${item.node}`:`Inspect ${side.toLowerCase()} action ${item.index} at position ${item.node}: ${clean(node.actions[item.index!].label)}`;
          return <g key={item.id} data-item={item.id} data-node={item.kind==='position'?item.node:undefined} data-search-action={item.kind==='action'?a!.key:undefined} data-root-choice={item.kind==='action'&&item.node===0?item.index:undefined} data-root-evaluation={item.kind==='evaluation'&&item.node===0?item.index:undefined} data-outcome-group={item.kind==='group'?a!.key:undefined} data-outcome-count={item.kind==='group'?item.members!.length:undefined}
            className={`search-readable-card ${item.kind} ${side.toLowerCase()} ${item.kind==='action'&&item.node===0?'search-graph-root-choice':''} ${isActive?'active':''} ${item.kind==='position'?selected===item.node?'selected':'':selectedAction?'selected':''}`} role="button" tabIndex={item.kind==='position'?focus===item.node?0:-1:0} aria-label={label} aria-pressed={item.kind==='position'?selected===item.node:selectedAction} transform={`translate(${item.x} ${item.y})`} onClick={choose} onFocus={()=>{if(item.kind==='position')setFocus(item.node);}} onKeyDown={e=>{if(item.kind==='position'&&e.key.startsWith('Arrow')){e.preventDefault();moveFocus(item.node,e.key);}keySelect(e,choose);}}>
            <foreignObject x="-90" y="-54" width="180" height="108"><div className="search-readable-body">
              {item.kind==='position'?<><span>POSITION {item.node} · {side.toUpperCase()}</span><strong>{item.node===0?'Current position':`${side} ${observed?'decides':'to decide'}`}</strong><small>{position!.board.phase.replace(/_/g,' ')} · life {position!.board.players.map(p=>p.life).join(' / ')}</small><small>{observed?`${position!.visits} visits · ${node.actions.length} choices`:'Evaluated before a response'}</small></>
                :item.kind==='action'?<><span>{side.toUpperCase()} · MOVE {item.index}</span><strong title={clean(node.actions[item.index!].label)}>{clean(node.actions[item.index!].label)}</strong><small>{a!.visits} visits · {actionStatus(a!)}</small></>
                :item.kind==='group'?<><span>GROUPED OUTCOMES</span><strong>{item.members!.length} sampled positions</strong><small>{[...new Set(item.members!.map(actor))].join(' / ')} next</small><small>Evaluated before a response</small></>
                :<><span>◇ EVALUATION ONLY</span><strong>Initial estimate {a!.firstEvaluation!.value}</strong><small>No stored child position</small></>}
            </div></foreignObject><title>{label}</title>
          </g>;
        })}
        {playing&&!reduced&&moving.length>1&&<motion.circle key={`${event.simulation}:${event.phase}:${event.choice}:${epoch}`} r="5" className={`search-graph-pulse ${event.phase}`} initial={{cx:moving[0].x,cy:moving[0].y}} animate={{cx:moving.map(p=>p.x),cy:moving.map(p=>p.y)}} transition={{duration:(event.phase==='backup'?.72:.4)/speed,ease:'linear'}}/>}
      </svg>
    </div>
    <div className="search-action-tools"><button aria-label="Zoom in search graph" onClick={()=>zoom(1.3)}><Plus size={15}/></button><button aria-label="Zoom out search graph" onClick={()=>zoom(1/1.3)}><Minus size={15}/></button><button aria-label="Fit discovered graph" onClick={fit}><Maximize2 size={15}/></button><button aria-label="Focus current simulation path" onClick={()=>{onInteract();setScale(1);window.requestAnimationFrame(()=>center(layout.destinations.get(graph.activeNodes.at(-1)!)!,1));}}><Focus size={15}/></button><span>Scroll or drag to follow the tree · {Math.round(scale*100)}%</span></div>
    <div className="search-graph-legend"><span className="root-choice">Action</span><span className="agent">Agent position</span><span className="opponent">Opponent position</span><span className="frontier">Grouped outcomes</span><span className="evaluation-only">Evaluation only</span></div>
    <div className="search-node-picker"><label>Inspect a discovered position <select aria-label="Inspect discovered position" value={selected??''} onChange={e=>{if(e.target.value!==''){const id=Number(e.target.value);inspectNode(id);center(layout.destinations.get(id)!);}else onSelect(null);}}><option value="">Follow current simulation</option>{[...graph.nodes.keys()].map(id=><option key={id} value={id}>Position {id} · {actor(id)} · depth {model.depths.get(id)}</option>)}</select></label></div>
    {selectedNode&&<div className="search-node-detail"><strong>Node {selectedNode.id} · {actor(selectedNode.id)}</strong><span>{selectedNode.born<0?'Root position':`First recorded in simulation ${selectedNode.born+1}`} · {selectedNode.visits} backed-up visits · mean {selectedNode.visits?String(selectedNode.sum/BigInt(selectedNode.visits)):'Pending'}</span><span>{selectedNode.estimate!==null?`First leaf estimate: ${selectedNode.estimate}. `:''}{responseSummary(selectedNode.id,replay,model)}</span></div>}
    {root&&<div className="search-node-detail"><strong>Move {root.index} · {clean(replay.nodes[0].actions[root.index].label)}</strong><span>{rootChoiceStatus(root)} · {root.visits} backed-up visits.</span><span>{root.firstEvaluation?`Initial evaluation from simulation ${root.firstEvaluation.simulation+1}: ${root.firstEvaluation.value}. The public preview shows that recorded result.`:'No initial evaluation snapshot is available at this point in the replay.'}</span></div>}
    {chosen&&<div className="search-outcome-inspector" aria-label="Sampled outcome comparison"><span className="eyebrow">{actor(chosen.node).toUpperCase()} ACTION · POSITION {chosen.node} · MOVE {chosen.index}</span><h4>{clean(replay.nodes[chosen.node].actions[chosen.index].label)}</h4><p>{actionStatus(chosen)} · {chosen.visits} backed-up visits · mean {chosen.visits?String(chosen.sum/BigInt(chosen.visits)):'Pending'}. Values use the recorded root perspective.</p>
      {!!chosen.children.length&&<div className="search-outcome-list">{chosen.children.map(id=>{const n=graph.nodes.get(id)!,responses=responseRecords(id,replay,event);return <div key={id} className="search-outcome-entry"><button data-outcome-position={id} className={selected===id?'selected':''} onClick={()=>inspectNode(id)}><strong>Position {id} · {actor(id)} next</strong><span>First reached in simulation {n.born+1} · {n.visits} visits{n.estimate!==null?` · initial estimate ${n.estimate}`:''}</span><span>{responseSummary(id,replay,model)}</span></button>{!!responses.length&&<details className="search-simulation-responses"><summary>{responses.length} recorded {responses.length===1?'response':'responses'}</summary>{responses.map((r,i)=><div key={i}>Simulation {r.simulation+1} · {actor(id)} {r.stage==='selected'?'selected':'tried'} <strong>{r.label}</strong> · {r.stage}</div>)}</details>}</div>;})}</div>}
      {!chosen.children.length&&<p>{chosen.unidentifiedExpansion?'This older replay omits the child identity.':chosen.firstEvaluation?'The recorded evaluation has no stored child.':'No resulting position is available yet.'}</p>}
    </div>}
    <p className="search-graph-note">Groups keep distinct sampled positions under one action. Opponent choices describe hypothetical responses actually selected by search. Sampled hands are excluded; unrecorded responses and unknown action details remain unavailable.</p>
    {replay.schema==='mtg-kernel-search-replay/v1'&&<p className="search-graph-note">This older recording contains path nodes but no leaf identities. Only observed path connections can grow during its replay.</p>}
  </div>;
}
