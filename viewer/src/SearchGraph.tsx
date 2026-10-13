import { useEffect, useMemo, useRef, useState } from 'react';
import { motion, useReducedMotion } from 'motion/react';
import { Focus, Maximize2, Minus, Plus } from 'lucide-react';
import type { Recording, SearchReplay } from './recording';
import type { SearchEvent } from './search-playback';
import { graphAt, graphBounds, graphLayout, type GraphPoint, type GraphTopology, type SearchGraphState } from './search-graph';

type Camera={x:number;y:number;scale:number};
const height=390;
export function SearchGraph({replay,event,recording,playing,speed,epoch,selected,onSelect,onComplete,onInteract}:{
  replay:SearchReplay;event:SearchEvent;recording:Recording;playing:boolean;speed:number;epoch:number;
  selected:number|null;onSelect:(id:number|null)=>void;onComplete:()=>void;onInteract:()=>void;
}) {
  const graph=useMemo(()=>graphAt(replay,event),[replay,event]);
  return <SearchGraphCanvas topology={replay} graph={graph} event={event} recording={recording} playing={playing} speed={speed} epoch={epoch} selected={selected} onSelect={onSelect} onComplete={onComplete} onInteract={onInteract}/>;
}
export function SearchGraphCanvas({topology:replay,graph,event,recording,playing,speed,epoch,selected,onSelect,onComplete,onInteract,uct=false}:{
  topology:GraphTopology;graph:SearchGraphState;event:SearchEvent;recording:Recording;playing:boolean;speed:number;epoch:number;
  selected:number|null;onSelect:(id:number|null)=>void;onComplete:()=>void;onInteract:()=>void;uct?:boolean;
}) {
  const layout=useMemo(()=>graphLayout(replay),[replay]);
  const container=useRef<HTMLDivElement>(null),drag=useRef<{x:number;y:number;camera:Camera;moved:boolean}|null>(null);
  const [width,setWidth]=useState(800),[camera,setCamera]=useState<Camera|null>(null),[focus,setFocus]=useState(0);
  const reduced=useReducedMotion(),simulation=replay.simulations[event.simulation];
  const points=useMemo(()=>{
    const bounds=graphBounds([...layout.values()]),deepest=Math.max(1,...[...layout.values()].map(p=>p.depth));
    return new Map([...layout].map(([id,p])=>[id,{...p,x:(p.x-bounds.minX-55)/Math.max(1,bounds.maxX-bounds.minX-110)*Math.max(100,width-110),y:p.depth/deepest*270}]));
  },[layout,width]);
  const levelCounts=new Map<number,number>();for(const p of points.values())levelCounts.set(p.depth,(levelCounts.get(p.depth)??0)+1);
  const radius=Math.max(2.5,Math.min(8,(width-110)/Math.max(...levelCounts.values())/2.8));
  const active=new Set(graph.activeNodes),last=points.get(graph.activeNodes.at(-1)!)!;
  const evaluation=event.phase!=='select'&&graph.frontier===null?{x:last.x+32,y:last.y+100,depth:last.depth+1}:null;
  const visible=[...graph.nodes.keys()].map(id=>points.get(id)!).filter(Boolean);
  const fit=(positions:GraphPoint[]):Camera=>{const b=graphBounds(positions),scale=Math.min(1.3,(width-60)/(b.maxX-b.minX),(height-50)/(b.maxY-b.minY));return {scale,x:width/2-(b.minX+b.maxX)/2*scale,y:height/2-(b.minY+b.maxY)/2*scale};};
  const auto=fit([...visible,...evaluation?[evaluation]:[]]),view=camera??auto;
  const fullDepth=Math.max(...[...points.values()].map(p=>p.depth));
  const path=graph.activeNodes.map(id=>points.get(id)!).concat(evaluation?[evaluation]:[]);
  const moving=event.phase==='backup'?path.slice().reverse():path.slice(-2);
  useEffect(()=>{const el=container.current;if(!el)return;let previous=0;const resize=()=>{if(el.clientWidth!==previous){previous=el.clientWidth;setWidth(previous);setCamera(null);}};const observer=new ResizeObserver(resize);observer.observe(el);resize();return()=>observer.disconnect();},[]);
  useEffect(()=>{if(!graph.nodes.has(focus))setFocus(0);},[graph,focus]);
  const zoom=(factor:number)=>{onInteract();setCamera(v=>{const c=v??auto,scale=Math.max(.08,Math.min(5,c.scale*factor)),ratio=scale/c.scale;return {scale,x:width/2+(c.x-width/2)*ratio,y:height/2+(c.y-height/2)*ratio};});};
  const inspect=(id:number)=>{onInteract();onSelect(id);setFocus(id);};
  const moveFocus=(id:number,key:string)=>{
    const p=points.get(id)!,same=[...graph.nodes.keys()].filter(n=>points.get(n)?.depth===p.depth).sort((a,b)=>points.get(a)!.x-points.get(b)!.x);
    const adjacent=key==='ArrowDown'?[...graph.edges.values()].filter(e=>e.from===id).map(e=>e.to):[...graph.edges.values()].filter(e=>e.to===id).map(e=>e.from);
    const next=key==='ArrowLeft'?same[Math.max(0,same.indexOf(id)-1)]:key==='ArrowRight'?same[Math.min(same.length-1,same.indexOf(id)+1)]:adjacent[0]??id;
    setFocus(next);container.current?.querySelector<SVGGElement>(`[data-node="${next}"]`)?.focus();
  };
  const selectedNode=selected===null?null:graph.nodes.get(selected),chosen=selectedNode?replay.nodes[selectedNode.id]:null;
  return <div className="search-graph">
    <div className="search-graph-heading"><div><span className="eyebrow">EXPLORED FUTURES</span><h3>A tree that grows with the search.</h3></div><button className="search-complete-graph" onClick={()=>{setCamera(null);onComplete();}}>See complete graph</button></div>
    <div className="search-graph-metrics"><span><b>{graph.nodes.size}</b> / {replay.nodes.length} positions discovered</span><span><b>{fullDepth}</b> deepest recorded decisions</span><span><b>{graph.completed}</b> simulations backed up</span></div>
    <div className="search-graph-canvas" ref={container}>
      <svg width="100%" height={height} viewBox={`0 0 ${width} ${height}`} aria-label="Accumulated explored search graph" role="group"
        onPointerDown={e=>{if((e.target as Element).closest('[data-node]'))return;onInteract();drag.current={x:e.clientX,y:e.clientY,camera:view,moved:false};e.currentTarget.setPointerCapture(e.pointerId);}}
        onPointerMove={e=>{const d=drag.current;if(d){const dx=e.clientX-d.x,dy=e.clientY-d.y;d.moved||=Math.abs(dx)+Math.abs(dy)>3;setCamera({...d.camera,x:d.camera.x+dx,y:d.camera.y+dy});}}}
        onPointerUp={()=>{drag.current=null;}} onPointerCancel={()=>{drag.current=null;}}>
        <g transform={`translate(${view.x} ${view.y}) scale(${view.scale})`}>
          {[...graph.edges.values()].map(edge=>{const from=points.get(edge.from)!,to=points.get(edge.to)!,middle=(from.y+to.y)/2,current=graph.activeEdges.has(edge.key);return <path key={edge.key} data-edge={edge.key} className={`search-graph-edge ${current?event.phase==='backup'?'backup':'active':''}`} vectorEffect="non-scaling-stroke" d={`M ${from.x} ${from.y} C ${from.x} ${middle} ${to.x} ${middle} ${to.x} ${to.y}`}><title>{replay.nodes[edge.from].actions[edge.action].label} → node {edge.to}</title></path>;})}
          {evaluation&&<><path className="search-graph-edge evaluation" vectorEffect="non-scaling-stroke" strokeDasharray="4 4" d={`M ${last.x} ${last.y} L ${evaluation.x} ${evaluation.y}`}/><g transform={`translate(${evaluation.x} ${evaluation.y})`}><path className="search-evaluation-diamond" d="M 0 -12 L 12 0 L 0 12 L -12 0 Z"/><text className="search-node-label" y="28" textAnchor="middle">{simulation.reason==='natural'?'Result':'Evaluation only'}</text><title>This recorded evaluation did not add a persistent search node.</title></g></>}
          {[...graph.nodes.values()].map(node=>{const p=points.get(node.id)!,actor=replay.nodes[node.id].actor,frontier=node.id!==0&&!graph.explored.has(node.id),isActive=active.has(node.id),label=`Inspect node ${node.id}: ${recording.seats[actor==='p0'?0:1].name}, depth ${p.depth}, ${node.visits} visits${frontier?', evaluated frontier':''}`;return <g key={node.id} data-node={node.id} role="button" aria-label={label} aria-pressed={selected===node.id} tabIndex={focus===node.id?0:-1} transform={`translate(${p.x} ${p.y})`} className={`search-graph-node ${actor===recording.perspective?'agent':'opponent'} ${frontier?'frontier':''} ${isActive?'active':''} ${selected===node.id?'selected':''}`}
            onClick={()=>inspect(node.id)} onFocus={()=>setFocus(node.id)} onKeyDown={e=>{if(e.key.startsWith('Arrow')){e.preventDefault();e.stopPropagation();moveFocus(node.id,e.key);}if(e.key==='Enter'||e.key===' '){e.preventDefault();e.stopPropagation();inspect(node.id);}}}>
            <title>{label}{node.estimate!==null?`; first estimate ${node.estimate}`:''}</title><circle className="search-node-hit" r={Math.max(radius+2,3/view.scale)}/><circle className="search-node-dot" r={node.id===0?17:radius} vectorEffect="non-scaling-stroke"/>
            {(node.id===0||isActive||selected===node.id)&&<text className="search-node-label" y={node.id===0?-29:radius+15} textAnchor="middle">{node.id===0?'CURRENT POSITION':`#${node.id} · ${actor===recording.perspective?'Agent':'Opponent'}`}</text>}
          </g>;})}
          {playing&&!reduced&&moving.length>1&&<motion.circle key={`${event.simulation}:${event.phase}:${event.choice}:${epoch}`} r="4" className={`search-graph-pulse ${event.phase}`} initial={{cx:moving[0].x,cy:moving[0].y}} animate={{cx:moving.map(p=>p.x),cy:moving.map(p=>p.y)}} transition={{duration:(event.phase==='backup'?.72:.4)/speed,ease:'linear'}}/>}
        </g>
      </svg>
      <div className="search-graph-tools"><button aria-label="Zoom in search graph" onClick={()=>zoom(1.6)}><Plus size={15}/></button><button aria-label="Zoom out search graph" onClick={()=>zoom(1/1.6)}><Minus size={15}/></button><button aria-label="Fit discovered graph" onClick={()=>{onInteract();setCamera(null);}}><Maximize2 size={15}/></button><button aria-label="Focus current simulation path" onClick={()=>{onInteract();setCamera(fit(path));}}><Focus size={15}/></button></div>
      <span className="search-graph-pan-hint">Drag to pan · Zoom to inspect</span>
    </div>
    <div className="search-graph-legend"><span className="agent">Agent position</span>{!uct&&<span className="opponent">Opponent position</span>}<span className="frontier">{uct?'Proposed position':'Evaluated frontier'}</span><span className="route">Current route</span></div>
    <div className="search-node-picker"><label>Inspect a discovered position <select aria-label="Inspect discovered position" value={selected??''} onChange={e=>{if(e.target.value!=='')inspect(Number(e.target.value));else onSelect(null);}}><option value="">Follow current simulation</option>{[...graph.nodes.keys()].map(id=><option key={id} value={id}>Node {id} · {replay.nodes[id].actor===recording.perspective?'Agent':'Opponent'} · depth {points.get(id)?.depth}</option>)}</select></label></div>
    {selectedNode&&chosen&&<div className="search-node-detail"><strong>Node {selectedNode.id} · {recording.seats[chosen.actor==='p0'?0:1].name}</strong><span>{selectedNode.born<0?'Root position':`Discovered in simulation ${selectedNode.born+1}`} · {selectedNode.visits} backed-up visits · mean {selectedNode.visits?String(selectedNode.sum/BigInt(selectedNode.visits)):'Pending'}</span><span>{selectedNode.estimate!==null?`First leaf estimate: ${selectedNode.estimate}. `:''}{chosen.actions.length} legal continuations; {[...graph.edges.values()].filter(e=>e.from===selectedNode.id).length} recorded outgoing connections.</span></div>}
    <p className="search-graph-note">{uct?'The UCT tree stores the focal agent’s non-forced choices. Opponent and later policy moves run between nodes. Proposed branches commit only after a natural result; truncated attempts add no visits.':'Each connection is a recorded move leading to a sampled position. One move can reach several hidden-world outcomes. Amber frontier positions were evaluated; their continuations have not yet been explored.'}</p>
    {replay.schema==='mtg-kernel-search-replay/v1'&&<p className="search-graph-note">This older recording contains path nodes but no leaf identities. Only observed path connections can grow during its replay.</p>}
  </div>;
}
