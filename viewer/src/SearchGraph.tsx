import { type CSSProperties, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { motion, useReducedMotion } from 'motion/react';
import { Focus, Maximize2, Minus, Plus } from 'lucide-react';
import type { Recording, SearchReplay } from './recording';
import type { SearchEvent } from './search-playback';
import { graphAt, graphBounds, graphLayout, type GraphNodeState, type GraphPoint, type GraphTopology, type SearchGraphState } from './search-graph';
import { shortLabel } from './uct-console';

type Camera={x:number;y:number;scale:number};
const fixedHeight=390;
export function SearchGraph({replay,event,recording,playing,speed,epoch,selected,onSelect,onComplete,onInteract}:{
  replay:SearchReplay;event:SearchEvent;recording:Recording;playing:boolean;speed:number;epoch:number;
  selected:number|null;onSelect:(id:number|null)=>void;onComplete:()=>void;onInteract:()=>void;
}) {
  const graph=useMemo(()=>graphAt(replay,event),[replay,event]);
  return <SearchGraphCanvas topology={replay} graph={graph} event={event} recording={recording} playing={playing} speed={speed} epoch={epoch} selected={selected} onSelect={onSelect} onComplete={onComplete} onInteract={onInteract}/>;
}
export function SearchGraphCanvas({topology:replay,graph,event,recording,playing,speed,epoch,selected,onSelect,onComplete,onInteract,uct=false,rolloutLabels=[],actual,actualRoute=false,compact=false,onWatch}:{
  topology:GraphTopology;graph:SearchGraphState;event:SearchEvent;recording:Recording;playing:boolean;speed:number;epoch:number;
  selected:number|null;onSelect:(id:number|null)=>void;onComplete:()=>void;onInteract:()=>void;uct?:boolean;rolloutLabels?:string[];actual?:{node:number;path:number[]};actualRoute?:boolean;compact?:boolean;onWatch?:(simulation:number)=>void;
}) {
  const layout=useMemo(()=>graphLayout(replay),[replay]);
  const container=useRef<HTMLDivElement>(null),drag=useRef<{x:number;y:number;camera:Camera;moved:boolean}|null>(null),dragged=useRef(false),calloutRef=useRef<HTMLDivElement>(null);
  const [width,setWidth]=useState(800),[measured,setMeasured]=useState(fixedHeight),[camera,setCamera]=useState<Camera|null>(null),[focus,setFocus]=useState(0),[hover,setHover]=useState<number|null>(null),[calloutHeight,setCalloutHeight]=useState(0);
  const height=compact?measured:fixedHeight;
  const reduced=useReducedMotion(),simulation=replay.simulations[event.simulation];
  const points=useMemo(()=>{
    const bounds=graphBounds([...layout.values()]),deepest=Math.max(1,...[...layout.values()].map(p=>p.depth));
    return new Map([...layout].map(([id,p])=>[id,{...p,x:(p.x-bounds.minX-55)/Math.max(1,bounds.maxX-bounds.minX-110)*Math.max(100,width-110),y:p.depth/deepest*(compact?Math.max(150,height-110):270)}]));
  },[layout,width,height,compact]);
  const levelCounts=new Map<number,number>();for(const p of points.values())levelCounts.set(p.depth,(levelCounts.get(p.depth)??0)+1);
  const radius=Math.max(2.5,Math.min(8,(width-110)/Math.max(...levelCounts.values())/2.8));
  const active=new Set(graph.activeNodes),last=points.get(graph.activeNodes.at(-1)??0)!;
  // In the UCT view, dot area grows with backed-up visits and gold marks recorded wins.
  const busiest=Math.max(1,...[...graph.nodes.values()].filter(n=>n.id!==0).map(n=>n.visits));
  const size=(n:GraphNodeState)=>uct?radius+Math.min(9,Math.sqrt(n.visits/busiest)*7):radius;
  const winShare=(n:{visits:number;sum:bigint})=>n.visits?Number(n.sum/BigInt(n.visits))/1000000:0;
  const parents=useMemo(()=>new Map([...graph.edges.values()].map(e=>[e.to,e])),[graph]),incoming=(id:number)=>parents.get(id);
  const moveInto=(id:number)=>{const e=incoming(id);return e?shortLabel(replay.nodes[e.from].actions[e.action].label):null;};
  const routeTo=(id:number)=>{const ids=[id];for(let e=incoming(id);e;e=incoming(e.from))ids.unshift(e.from);return ids;};
  const wins=(n:GraphNodeState)=>Math.round(winShare(n)*n.visits);
  const evaluation=graph.activeNodes.length&&event.phase!=='select'&&graph.frontier===null?{x:last.x+32,y:last.y+100+65*rolloutLabels.length,depth:last.depth+1}:null;
  const rolloutPoints=rolloutLabels.map((label,i)=>({x:last.x,y:last.y+65*(i+1),depth:last.depth+i+1,label}));
  const visible=[...graph.nodes.keys()].map(id=>points.get(id)!).filter(Boolean);
  const fit=(positions:GraphPoint[]):Camera=>{const b=graphBounds(positions),scale=Math.min(1.3,(width-60)/(b.maxX-b.minX),(height-50)/(b.maxY-b.minY));return {scale,x:width/2-(b.minX+b.maxX)/2*scale,y:height/2-(b.minY+b.maxY)/2*scale};};
  const auto=fit([...visible,...evaluation?[evaluation]:[],...rolloutPoints]),view=camera??auto;
  const fullDepth=Math.max(...[...points.values()].map(p=>p.depth));
  const path=graph.activeNodes.map(id=>points.get(id)!).concat(evaluation?[evaluation]:[]);
  const moving=event.phase==='backup'?path.slice().reverse():path.slice(-2);
  useEffect(()=>{const el=container.current;if(!el)return;let previous=0,tall=0;const resize=()=>{if(el.clientWidth!==previous||compact&&el.clientHeight!==tall){previous=el.clientWidth;tall=el.clientHeight;setWidth(previous);if(compact)setMeasured(Math.max(160,tall));setCamera(null);}};const observer=new ResizeObserver(resize);observer.observe(el);resize();return()=>observer.disconnect();},[compact]);
  useEffect(()=>{if(!graph.nodes.has(focus))setFocus(0);},[graph,focus]);
  useEffect(()=>{if(actualRoute&&actual)setCamera(fit(actual.path.map(id=>points.get(id)!)));else setCamera(null);},[actualRoute,actual?.node,width,height]);
  const zoom=(factor:number)=>{onInteract();setCamera(v=>{const c=v??auto,scale=Math.max(.08,Math.min(5,c.scale*factor)),ratio=scale/c.scale;return {scale,x:width/2+(c.x-width/2)*ratio,y:height/2+(c.y-height/2)*ratio};});};
  const inspect=(id:number)=>{onInteract();onSelect(id);setFocus(id);};
  // Pick the node nearest the pointer, so dense rows of small dots stay clickable.
  const nearest=(e:{clientX:number;clientY:number;currentTarget:Element})=>{
    const box=e.currentTarget.getBoundingClientRect(),x=e.clientX-box.left,y=e.clientY-box.top;let best:number|null=null,score=8;
    for(const node of graph.nodes.values()){const p=points.get(node.id)!,r=(node.id===0?17:size(node))*view.scale,d=Math.hypot(view.x+p.x*view.scale-x,view.y+p.y*view.scale-y)-r/2;if(d<score){score=d;best=node.id;}}
    return best;
  };
  // Labels keep a readable on-screen size when the camera zooms out.
  const grow=1/Math.min(1,view.scale);
  const picked=new Set(uct&&selected!==null&&graph.nodes.has(selected)?routeTo(selected):[]);
  const moveFocus=(id:number,key:string)=>{
    const p=points.get(id)!,same=[...graph.nodes.keys()].filter(n=>points.get(n)?.depth===p.depth).sort((a,b)=>points.get(a)!.x-points.get(b)!.x);
    const adjacent=key==='ArrowDown'?[...graph.edges.values()].filter(e=>e.from===id).map(e=>e.to):[...graph.edges.values()].filter(e=>e.to===id).map(e=>e.from);
    const next=key==='ArrowLeft'?same[Math.max(0,same.indexOf(id)-1)]:key==='ArrowRight'?same[Math.min(same.length-1,same.indexOf(id)+1)]:adjacent[0]??id;
    setFocus(next);container.current?.querySelector<SVGGElement>(`[data-node="${next}"]`)?.focus();
  };
  const selectedNode=selected===null?null:graph.nodes.get(selected),chosen=selectedNode?replay.nodes[selectedNode.id]:null;
  const detail=selectedNode&&chosen&&<div className="search-node-detail"><strong>Node {selectedNode.id} · {recording.seats[chosen.actor==='p0'?0:1].name}</strong><span>{selectedNode.born<0?'Root position':`Discovered in simulation ${selectedNode.born+1}`} · {selectedNode.visits} backed-up visits · mean {selectedNode.visits?String(selectedNode.sum/BigInt(selectedNode.visits)):'Pending'}</span><span>{selectedNode.estimate!==null?`First leaf estimate: ${selectedNode.estimate}. `:''}{chosen.actions.length} legal continuations; {[...graph.edges.values()].filter(e=>e.from===selectedNode.id).length} recorded outgoing connections.</span></div>;
  // In the console, details open beside the clicked dot and stay inside the canvas.
  useLayoutEffect(()=>{const h=calloutRef.current?.offsetHeight??0;if(h!==calloutHeight)setCalloutHeight(h);});
  const screen=(id:number)=>{const p=points.get(id)!;return {x:view.x+p.x*view.scale,y:view.y+p.y*view.scale};};
  // Prefer below the dot: its route back to the root runs upward and stays visible.
  const place=():CSSProperties|undefined=>{
    if(!selectedNode)return undefined;
    const w=Math.min(300,width-16),h=calloutHeight,{x,y}=screen(selectedNode.id),r=(selectedNode.id===0?17:size(selectedNode))*view.scale,centered=Math.max(8,Math.min(width-8-w,x-w/2));
    if(y+r+22+h<=height-8)return {width:w,left:centered,top:y+r+22};
    const right=x+r+12+w<=width-8,left=x-r-12-w>=8,away=screen(0).x<x;
    if(right||left)return {width:w,left:(away?right:!left)?x+r+12:x-r-12-w,top:Math.max(8,Math.min(height-8-h,y-h/2))};
    return {width:w,left:centered,top:Math.max(8,y-r-12-h)};
  };
  const callout=compact&&uct&&selectedNode&&chosen?(()=>{
    const route=routeTo(selectedNode.id).slice(1),visits=selectedNode.visits,won=wins(selectedNode),moves=chosen.actions.length;
    const tried=new Set([...graph.edges.values()].filter(e=>e.from===selectedNode.id).map(e=>e.action)).size;
    return <div ref={calloutRef} className="search-node-callout" role="dialog" aria-label="Position details" style={place()}>
      <button className="search-node-close" aria-label="Close position details" onClick={()=>onSelect(null)}>×</button>
      {selectedNode.id===0?<strong>Search root: the real game position</strong>:<ol className="search-node-route" aria-label="Choices from the search root">{route.map(id=><li key={id}>{moveInto(id)}</li>)}</ol>}
      <p className="search-node-stats"><b>{visits}</b> {visits===1?'try':'tries'} through here · <b>{won}</b> won{visits?` (${Math.round(won/visits*100)}%)`:''}</p>
      <p>{selectedNode.born<0?'Every attempt starts here':visits?`First reached in attempt ${selectedNode.born+1}`:`Proposed in attempt ${selectedNode.born+1}, not yet counted`} · {tried} of {moves} next {moves===1?'move':'moves'} tried</p>
      <div className="search-node-actions">{onWatch&&selectedNode.born>=0&&<button className="search-node-watch" onClick={()=>onWatch(selectedNode.born)}>Watch attempt {selectedNode.born+1}</button>}<span>Board below shows this position.</span></div>
    </div>;
  })():null;
  const hovered=compact&&hover!==null&&hover!==selected?graph.nodes.get(hover):undefined;
  const tooltip=hovered&&(()=>{const {x,y}=screen(hovered.id);return <div className={`search-node-hover ${y<60?'below':''}`} style={{left:Math.max(120,Math.min(width-120,x)),top:y}} aria-hidden="true">
    <strong>{hovered.id===0?'Search root':moveInto(hovered.id)}</strong><span>{hovered.visits} {hovered.visits===1?'try':'tries'} · {wins(hovered)} {wins(hovered)===1?'win':'wins'} · click for details</span></div>;})();
  const metrics=<div className="search-graph-metrics"><span><b>{graph.nodes.size}</b> / {replay.nodes.length} positions discovered</span><span><b>{fullDepth}</b> deepest recorded decisions</span><span><b>{graph.completed}</b> simulations backed up</span></div>;
  const complete=<button className="search-complete-graph" onClick={()=>{setCamera(null);onComplete();}}>See complete graph</button>;
  return <div className={`search-graph ${compact?'search-graph-compact':''}`}>
    {compact?<div className="search-graph-heading"><div><span className="eyebrow">SEARCH TREE · EXPLORED FUTURES</span>{metrics}</div>{complete}</div>
      :<><div className="search-graph-heading"><div><span className="eyebrow">EXPLORED FUTURES</span><h3>A tree that grows with the search.</h3></div>{complete}</div>{metrics}</>}
    <div className="search-graph-canvas" ref={container}>
      <svg width="100%" height={height} viewBox={`0 0 ${width} ${height}`} aria-label="Accumulated explored search graph" role="group"
        onPointerDown={e=>{dragged.current=false;if((e.target as Element).closest('[data-node]'))return;onInteract();drag.current={x:e.clientX,y:e.clientY,camera:view,moved:false};e.currentTarget.setPointerCapture(e.pointerId);}}
        onPointerMove={e=>{const d=drag.current;if(d){const dx=e.clientX-d.x,dy=e.clientY-d.y;d.moved||=Math.abs(dx)+Math.abs(dy)>3;setCamera({...d.camera,x:d.camera.x+dx,y:d.camera.y+dy});}else if(compact&&e.pointerType==='mouse'){const id=nearest(e);if(id!==hover)setHover(id);}}}
        onPointerLeave={()=>setHover(null)}
        onPointerUp={()=>{dragged.current=Boolean(drag.current?.moved);drag.current=null;}} onPointerCancel={()=>{drag.current=null;}}
        onClick={e=>{if(dragged.current)return;const id=nearest(e);if(id!==null)inspect(id);else if(compact)onSelect(null);}}>
        <g transform={`translate(${view.x} ${view.y}) scale(${view.scale})`}>
          {[...graph.edges.values()].map(edge=>{const from=points.get(edge.from)!,to=points.get(edge.to)!,middle=(from.y+to.y)/2,current=graph.activeEdges.has(edge.key),real=Boolean(actual&&actual.path.indexOf(edge.to)>0&&actual.path[actual.path.indexOf(edge.to)-1]===edge.from);return <path key={edge.key} data-edge={edge.key} className={`search-graph-edge ${current?event.phase==='backup'?'backup':'active':''} ${real?'actual-route':''} ${picked.has(edge.to)&&parents.get(edge.to)===edge?'picked':''}`} vectorEffect="non-scaling-stroke" d={`M ${from.x} ${from.y} C ${from.x} ${middle} ${to.x} ${middle} ${to.x} ${to.y}`}><title>{replay.nodes[edge.from].actions[edge.action].label} → node {edge.to}{real?' · recorded actual game route':''}</title></path>;})}
          {evaluation&&<><path className="search-graph-edge evaluation" vectorEffect="non-scaling-stroke" strokeDasharray="4 4" d={`M ${last.x} ${last.y} L ${evaluation.x} ${evaluation.y}`}/><g transform={`translate(${evaluation.x} ${evaluation.y})`}><path className="search-evaluation-diamond" d="M 0 -12 L 12 0 L 0 12 L -12 0 Z"/><text className="search-node-label" y="28" textAnchor="middle">{uct?(simulation.reason==='natural'?'Natural result':'Attempt stopped'):simulation.reason==='natural'?'Result':'Evaluation only'}</text><title>{uct?'This result or stopped attempt does not add a persistent search node.':'This recorded evaluation did not add a persistent search node.'}</title></g></>}
          {rolloutPoints.map((p,i)=>{const from=i?rolloutPoints[i-1]:last;return <g key={i} className="search-rollout-position"><path className="search-graph-edge evaluation" vectorEffect="non-scaling-stroke" strokeDasharray="3 5" d={`M ${from.x} ${from.y} L ${p.x} ${p.y}`}/><g transform={`translate(${p.x} ${p.y})`}><circle r="6" fill="#111f19" stroke="#83baa3"/><text className="search-node-label" textAnchor="middle" y="22">{p.label}</text><title>Recorded policy rollout continuation; this is not a persistent tree node.</title></g></g>;})}
          {[...graph.nodes.values()].map(node=>{const p=points.get(node.id)!,actor=replay.nodes[node.id].actor,frontier=node.id!==0&&!graph.explored.has(node.id),isActive=active.has(node.id),label=`Inspect node ${node.id}: ${recording.seats[actor==='p0'?0:1].name}, depth ${p.depth}, ${node.visits} visits${frontier?', evaluated frontier':''}`;return <g key={node.id} data-node={node.id} role="button" aria-label={label} aria-pressed={selected===node.id} tabIndex={focus===node.id?0:-1} transform={`translate(${p.x} ${p.y})`} className={`search-graph-node ${actor===recording.perspective?'agent':'opponent'} ${frontier?'frontier':''} ${isActive?'active':''} ${selected===node.id?'selected':''}`}
            onFocus={()=>setFocus(node.id)} onKeyDown={e=>{if(e.key.startsWith('Arrow')){e.preventDefault();e.stopPropagation();moveFocus(node.id,e.key);}if(e.key==='Enter'||e.key===' '){e.preventDefault();e.stopPropagation();inspect(node.id);}}}>
            {!compact&&<title>{label}{uct&&node.id!==0&&incoming(node.id)?` · reached by ${shortLabel(replay.nodes[incoming(node.id)!.from].actions[incoming(node.id)!.action].label)}`:''}{uct&&node.visits?` · ${Math.round(winShare(node)*node.visits)} wins`:''}{node.estimate!==null?`; first estimate ${node.estimate}`:''}</title>}<circle className="search-node-hit" r={Math.max(size(node)+2,3/view.scale)}/><circle className={`search-node-dot ${uct&&winShare(node)>0?'won':''}`} style={uct&&winShare(node)>0?{'--win':`${Math.round(35+65*Math.sqrt(winShare(node)))}%`} as CSSProperties:undefined} r={node.id===0?17:size(node)} vectorEffect="non-scaling-stroke"/>
            {actual?.node===node.id&&<g data-actual-node={node.id} aria-label={`Actual game choice at node ${node.id}`}><circle className="search-actual-ring" vectorEffect="non-scaling-stroke" r={node.id===0?23:Math.max(radius+5,7/view.scale)}/></g>}
          </g>;})}
          {/* Labels sit above every dot, so dense rows never cover them. */}
          <g className="search-graph-labels" aria-hidden="true">
            {[...graph.nodes.values()].filter(n=>n.id===0||active.has(n.id)||selected===n.id).map(node=>{const p=points.get(node.id)!,actor=replay.nodes[node.id].actor;return <text key={node.id} className={`search-node-label ${selected===node.id?'selected':''}`} style={{fontSize:`${9*grow}px`,strokeWidth:`${3*grow}px`}} x={p.x} y={p.y+(node.id===0?-29:size(node)+15*grow)} textAnchor="middle">{node.id===0?uct?'SEARCH ROOT':'CURRENT POSITION':uct&&incoming(node.id)?moveInto(node.id):`#${node.id} · ${actor===recording.perspective?'Agent':'Opponent'}`}</text>;})}
            {actual&&points.has(actual.node)&&graph.nodes.has(actual.node)&&<text className="search-actual-label" style={{fontSize:`${10*grow}px`,strokeWidth:`${4*grow}px`}} textAnchor="middle" x={points.get(actual.node)!.x} y={points.get(actual.node)!.y+(actual.node===0?-42*grow:radius+32)}>ACTUAL GAME · NODE {actual.node}</text>}
          </g>
          {playing&&!reduced&&moving.length>1&&<motion.circle key={`${event.simulation}:${event.phase}:${event.choice}:${epoch}`} r="4" className={`search-graph-pulse ${event.phase}`} initial={{cx:moving[0].x,cy:moving[0].y}} animate={{cx:moving.map(p=>p.x),cy:moving.map(p=>p.y)}} transition={{duration:(event.phase==='backup'?.72:.4)/speed,ease:'linear'}}/>}
        </g>
      </svg>
      <div className="search-graph-tools"><button aria-label="Zoom in search graph" onClick={()=>zoom(1.6)}><Plus size={15}/></button><button aria-label="Zoom out search graph" onClick={()=>zoom(1/1.6)}><Minus size={15}/></button><button aria-label="Fit discovered graph" onClick={()=>{onInteract();setCamera(null);}}><Maximize2 size={15}/></button><button aria-label="Focus current simulation path" onClick={()=>{onInteract();setCamera(fit(path));}}><Focus size={15}/></button></div>
      <span className="search-graph-pan-hint">Drag to pan · Zoom to inspect</span>
      {tooltip}{callout}
    </div>
    {compact?<div className="search-graph-legend"><span className="agent">Position (bigger = tried more)</span><span className="won">Led to wins (brighter = higher win rate)</span><span className="route">This attempt’s route</span>{picked.size>1&&<span className="picked">Selected route</span>}{actual&&<span className="actual">Actual game route</span>}</div>
      :<div className="search-graph-legend"><span className="agent">Agent position</span>{!uct&&<span className="opponent">Opponent position</span>}<span className="frontier">{uct?'Proposed position':'Evaluated frontier'}</span><span className="route">Simulation route</span>{actual&&<span className="actual">Actual game route</span>}</div>}
    {!compact&&rolloutLabels.length>0&&<p className="search-graph-note">Dashed continuation: observed combo events in this simulation’s policy rollout. These markers add no tree nodes or visits.</p>}
    {!compact&&<div className="search-node-picker"><label>Inspect a discovered position <select aria-label="Inspect discovered position" value={selected??''} onChange={e=>{if(e.target.value!=='')inspect(Number(e.target.value));else onSelect(null);}}><option value="">Follow current simulation</option>{[...graph.nodes.keys()].map(id=><option key={id} value={id}>Node {id} · {replay.nodes[id].actor===recording.perspective?'Agent':'Opponent'} · depth {points.get(id)?.depth}</option>)}</select></label></div>}
    {!compact&&detail}
    {!compact&&<p className="search-graph-note">{uct?'The UCT tree stores the focal agent’s non-forced choices. Opponent and later policy moves run between nodes. Proposed branches commit only after a natural result; truncated attempts add no visits.':'Each connection is a recorded move leading to a sampled position. One move can reach several hidden-world outcomes. Amber frontier positions were evaluated; their continuations have not yet been explored.'}</p>}
    {replay.schema==='mtg-kernel-search-replay/v1'&&<p className="search-graph-note">This older recording contains path nodes but no leaf identities. Only observed path connections can grow during its replay.</p>}
  </div>;
}
