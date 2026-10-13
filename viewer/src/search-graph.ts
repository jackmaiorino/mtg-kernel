import type { Board, SearchReplay } from './recording';
import type { SearchEvent } from './search-playback';

export type GraphEdge = {from:number; to:number; action:number; key:string};
export type GraphNodeState = {id:number; born:number; visits:number; sum:bigint; board:Board; estimate:string|null};
export type SearchGraphState = {nodes:Map<number,GraphNodeState>; edges:Map<string,GraphEdge>; explored:Set<number>; activeNodes:number[]; activeEdges:Set<string>; frontier:number|null; completed:number};
export type RootChoiceState = {index:number; visits:number; evaluations:number; unidentifiedExpansions:number; children:number[]; firstEvaluation:{board:Board; value:string; simulation:number}|null};
export const edgeKey=(from:number,action:number,to:number)=>`${from}:${action}:${to}`;

// Legal choices exist before search visits them. Evaluation-only probes are
// retained as evidence, without adding invented nodes to the search tree.
export function rootChoicesAt(replay:SearchReplay,event:SearchEvent,graph:SearchGraphState):RootChoiceState[] {
  const choices=replay.nodes[0].actions.map(a=>({index:a.index,visits:0,evaluations:0,unidentifiedExpansions:0,children:[] as number[],firstEvaluation:null as RootChoiceState['firstEvaluation']}));
  for(let i=0;i<=event.simulation;i++){
    const s=replay.simulations[i],choice=choices[s.choices[0].action];
    if(i<event.simulation||event.phase!=='select'){
      choice.evaluations++;
      if(s.choices.length===1&&s.leafNode==null){
        if(s.reason==='expanded')choice.unidentifiedExpansions++;
        else if(!choice.firstEvaluation)choice.firstEvaluation={board:s.leaf,value:s.value,simulation:i};
      }
    }
    if(i<event.simulation||event.phase==='backup')choice.visits++;
  }
  for(const edge of graph.edges.values())if(edge.from===0)choices[edge.action].children.push(edge.to);
  return choices;
}

export function rootChoiceStatus(choice:RootChoiceState):string {
  if(choice.children.length)return `Expanded into ${choice.children.length} stored ${choice.children.length===1?'position':'positions'}`;
  if(choice.unidentifiedExpansions)return 'Expansion recorded · child identity unavailable';
  if(!choice.evaluations)return 'Not evaluated yet';
  return `${choice.evaluations===1?'Evaluated once':`Evaluated ${choice.evaluations} times`} · not expanded${choice.visits<choice.evaluations?' · backup pending':''}`;
}

// Only replayed arrivals reveal topology. A new leaf appears at evaluation;
// visits and sums change at backup. Seeking reconstructs the same prefix.
export function graphAt(replay:SearchReplay,event:SearchEvent):SearchGraphState {
  const nodes=new Map<number,GraphNodeState>(),edges=new Map<string,GraphEdge>(),explored=new Set<number>();
  const add=(id:number,board:Board,born:number,estimate:string|null=null)=>{
    if(!nodes.has(id))nodes.set(id,{id,born,board,estimate,visits:0,sum:0n});
  };
  add(0,replay.simulations[0].choices[0].board,-1);
  const link=(from:number,action:number,to:number)=>{const key=edgeKey(from,action,to);edges.set(key,{from,action,to,key});};
  for(let i=0;i<=event.simulation;i++){
    const s=replay.simulations[i],current=i===event.simulation;
    const limit=current&&event.phase==='select'?event.choice+1:s.choices.length;
    for(let j=0;j<limit;j++){
      const c=s.choices[j];add(c.node,c.board,i);
      if(!current||event.phase!=='select'||j<event.choice)explored.add(c.node);
      if(j){const p=s.choices[j-1];link(p.node,p.action,c.node);}
    }
    const evaluated=!current||event.phase!=='select';
    if(evaluated&&s.leafNode!==null&&s.leafNode!==undefined){
      const p=s.choices.at(-1)!;add(s.leafNode,s.leaf,i,s.value);link(p.node,p.action,s.leafNode);
    }
    if(!current||event.phase==='backup'){
      for(const c of s.choices){const n=nodes.get(c.node)!;n.visits++;n.sum+=BigInt(s.value);}
      if(s.leafNode!==null&&s.leafNode!==undefined){const n=nodes.get(s.leafNode)!;n.visits++;n.sum+=BigInt(s.value);}
    }
  }
  const s=replay.simulations[event.simulation],limit=event.phase==='select'?event.choice+1:s.choices.length;
  const activeNodes=s.choices.slice(0,limit).map(c=>c.node),activeEdges=new Set<string>();
  for(let i=1;i<limit;i++){const p=s.choices[i-1];activeEdges.add(edgeKey(p.node,p.action,s.choices[i].node));}
  const frontier=event.phase!=='select'?s.leafNode??null:null;
  if(frontier!==null){const p=s.choices.at(-1)!;activeNodes.push(frontier);activeEdges.add(edgeKey(p.node,p.action,frontier));}
  return {nodes,edges,explored,activeNodes,activeEdges,frontier,completed:event.simulation+(event.phase==='backup'?1:0)};
}

export type GraphPoint={x:number;y:number;depth:number};
export function graphLayout(replay:SearchReplay):Map<number,GraphPoint> {
  // Lay out a canonical spanning tree while retaining every recorded edge.
  // Shared positions keep one identity, including multiple incoming routes.
  const children=replay.nodes.map(n=>[...new Set(n.children.flat())].sort((a,b)=>a-b));
  const parent=new Map<number,number>(),depth=new Map([[0,0]]),queue=[0];
  for(let i=0;i<queue.length;i++)for(const child of children[queue[i]])if(!depth.has(child)){
    parent.set(child,queue[i]);depth.set(child,depth.get(queue[i])!+1);queue.push(child);
  }
  const points=new Map<number,GraphPoint>();let leaf=0;
  const place=(id:number):number=>{
    const owned=children[id].filter(c=>parent.get(c)===id);
    const xs=owned.map(place),x=xs.length?(xs[0]+xs.at(-1)!)/2:leaf++*26;
    points.set(id,{x,y:depth.get(id)!*100,depth:depth.get(id)!});return x;
  };
  place(0);return points;
}

export function graphBounds(points:GraphPoint[]) {
  const xs=points.map(p=>p.x),ys=points.map(p=>p.y);
  return {minX:Math.min(...xs)-55,maxX:Math.max(...xs)+55,minY:Math.min(...ys)-45,maxY:Math.max(...ys)+45};
}
