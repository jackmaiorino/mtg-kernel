import type { Board, SearchReplay } from './recording';
import type { SearchEvent } from './search-playback';
import type { SearchGraphState } from './search-graph';

export type ActionRef={node:number;index:number};
export type ActionEvidence=ActionRef&{key:string;visits:number;sum:bigint;selections:number;children:number[];firstEvaluation:{board:Board;value:string;simulation:number}|null;unidentifiedExpansion:boolean};
export type ActionGraph={actions:Map<string,ActionEvidence>;menus:Set<number>;depths:Map<number,number>;groups:Map<string,number[]>};
export const actionKey=(node:number,index:number)=>`${node}:${index}`;

// Group native identities for presentation only. Similar public boards do
// not establish identical hidden worlds or identical available responses.
export function actionsAt(replay:SearchReplay,event:SearchEvent,graph:SearchGraphState):ActionGraph {
  const actions=new Map<string,ActionEvidence>(),menus=new Set([0]);
  const menu=(node:number)=>{menus.add(node);for(const a of replay.nodes[node].actions){
    const key=actionKey(node,a.index);
    if(!actions.has(key))actions.set(key,{key,node,index:a.index,visits:0,sum:0n,selections:0,children:[],firstEvaluation:null,unidentifiedExpansion:false});
  }};
  menu(0);
  for(let i=0;i<=event.simulation;i++){
    const s=replay.simulations[i],current=i===event.simulation,limit=current&&event.phase==='select'?event.choice+1:s.choices.length;
    for(let j=0;j<limit;j++){
      const c=s.choices[j];menu(c.node);const a=actions.get(actionKey(c.node,c.action))!;a.selections++;
      if(!current||event.phase==='backup'){a.visits++;a.sum+=BigInt(s.value);}
      if(j===s.choices.length-1&&(!current||event.phase!=='select')&&s.leafNode==null){
        if(s.reason==='expanded')a.unidentifiedExpansion=true;
        else if(!a.firstEvaluation)a.firstEvaluation={board:s.leaf,value:s.value,simulation:i};
      }
    }
  }
  const incoming=new Map<number,number>();
  for(const e of graph.edges.values()){
    actions.get(actionKey(e.from,e.action))?.children.push(e.to);
    incoming.set(e.to,(incoming.get(e.to)??0)+1);
  }
  const groups=new Map<string,number[]>();
  for(const a of actions.values()){
    const leaves=a.children.filter(id=>!menus.has(id)&&incoming.get(id)===1);
    if(leaves.length>1)groups.set(a.key,leaves);
  }
  const depths=new Map([[0,0]]),queue=[0];
  for(let i=0;i<queue.length;i++)for(const e of graph.edges.values())if(e.from===queue[i]&&!depths.has(e.to)){
    depths.set(e.to,depths.get(e.from)!+1);queue.push(e.to);
  }
  return {actions,menus,depths,groups};
}

export function actionStatus(a:ActionEvidence):string {
  if(a.children.length)return `${a.children.length} sampled ${a.children.length===1?'position':'positions'}`;
  if(a.unidentifiedExpansion)return 'Child identity unavailable';
  if(a.firstEvaluation)return 'Evaluation only';
  return a.selections?'Selection recorded':'Not tried yet';
}

export function responseSummary(node:number,replay:SearchReplay,model:ActionGraph):string {
  if(!model.menus.has(node))return 'Evaluated before a response';
  const selected=[...model.actions.values()].filter(a=>a.node===node&&a.selections);
  return selected.map(a=>`${replay.nodes[node].actions[a.index].label.replace(/\s*\[c\d+\]/g,'')} (${a.visits} backed up)`).join('; ')||'No response selected';
}

export function responseRecords(node:number,replay:SearchReplay,event:SearchEvent) {
  const records:{simulation:number;index:number;label:string;stage:'selected'|'tried'|'backed up'}[]=[];
  for(let i=0;i<=event.simulation;i++){
    const s=replay.simulations[i],current=i===event.simulation,limit=current&&event.phase==='select'?event.choice+1:s.choices.length;
    for(let j=0;j<limit;j++){const c=s.choices[j];if(c.node===node)records.push({simulation:i,index:c.action,label:replay.nodes[node].actions[c.action].label.replace(/\s*\[c\d+\]/g,''),stage:!current||event.phase==='backup'?'backed up':event.phase==='select'&&j===event.choice?'selected':'tried'});}
  }
  return records;
}

export type PresentationItem={id:string;kind:'position'|'action'|'group'|'evaluation';node:number;index?:number;members?:number[];x:number;y:number};
export function actionLayout(model:ActionGraph,graph:SearchGraphState) {
  const rows=new Map<number,PresentationItem[]>(),collapsed=new Set([...model.groups.values()].flat());
  const add=(row:number,item:Omit<PresentationItem,'x'|'y'>)=>{const items=rows.get(row)??[];items.push({...item,x:0,y:0});rows.set(row,items);};
  for(const node of graph.nodes.keys())if(!collapsed.has(node))add((model.depths.get(node)??0)*2,{id:`n${node}`,kind:'position',node});
  for(const a of model.actions.values()){
    const row=(model.depths.get(a.node)??0)*2+1;add(row,{id:`a${a.key}`,kind:'action',node:a.node,index:a.index});
    if(model.groups.has(a.key))add(row+1,{id:`g${a.key}`,kind:'group',node:a.node,index:a.index,members:model.groups.get(a.key)});
    if(a.firstEvaluation&&!a.children.length)add(row+1,{id:`e${a.key}`,kind:'evaluation',node:a.node,index:a.index});
  }
  const width=Math.max(800,...[...rows.values()].map(r=>r.length*196+48)),items=new Map<string,PresentationItem>();
  const anchor=(item:PresentationItem)=>{
    if(item.kind==='action')return items.get(`n${item.node}`)?.x??width/2;
    if(item.kind!=='position')return items.get(`a${actionKey(item.node,item.index!)}`)?.x??width/2;
    const parent=[...graph.edges.values()].find(e=>e.to===item.node);
    return parent?items.get(`a${actionKey(parent.from,parent.action)}`)?.x??width/2:width/2;
  };
  for(const [row,entries] of [...rows].sort((a,b)=>a[0]-b[0])){entries.sort((a,b)=>anchor(a)-anchor(b)||(a.index??-1)-(b.index??-1)||a.node-b.node);entries.forEach((item,i)=>{
    item.x=width/2+(i-(entries.length-1)/2)*196;item.y=68+row*146;items.set(item.id,item);
  });}
  const destinations=new Map<number,string>();for(const n of graph.nodes.keys())destinations.set(n,`n${n}`);
  for(const [key,members] of model.groups)for(const n of members)destinations.set(n,`g${key}`);
  return {width,height:Math.max(410,Math.max(...rows.keys())*146+180),items,destinations};
}
