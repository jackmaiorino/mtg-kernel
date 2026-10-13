import type { Frame, Recording, UctReplay } from './recording';
import { edgeKey, type SearchGraphState } from './search-graph';

export type UctEvent={simulation:number;phase:'select'|'rollout'|'evaluate'|'backup';choice:number;move?:number;after?:boolean};
export function uctEvents(r:UctReplay):UctEvent[]{
  return r.simulations.flatMap((s,simulation)=>[
    ...s.choices.map((_,choice)=>({simulation,phase:'select' as const,choice})),
    ...s.combo?s.rollout.flatMap((m,move)=>m[5]||m[6]?[{simulation,phase:'rollout' as const,choice:s.choices.length-1,move,after:false},{simulation,phase:'rollout' as const,choice:s.choices.length-1,move,after:true}]:[]):[],
    {simulation,phase:'evaluate' as const,choice:Math.max(0,s.choices.length-1)},
    {simulation,phase:'backup' as const,choice:Math.max(0,s.choices.length-1)},
  ]);
}
export function uctGraphAt(r:UctReplay,event:UctEvent):SearchGraphState{
  const nodes:SearchGraphState['nodes']=new Map(),edges:SearchGraphState['edges']=new Map(),explored=new Set<number>();
  const add=(id:number,board:number,born:number)=>{if(!nodes.has(id))nodes.set(id,{id,born,board:r.boards[board],estimate:null,visits:0,sum:0n});};
  add(0,r.nodes[0].board,-1);let completed=0;
  for(let i=0;i<=event.simulation;i++){
    const s=r.simulations[i],current=i===event.simulation,limit=current&&event.phase==='select'?event.choice+1:s.choices.length;
    for(let j=0;j<limit;j++){
      const c=s.choices[j];add(c.node,c.board,i);
      if(j){const p=s.choices[j-1],key=edgeKey(p.node,p.action,c.node);edges.set(key,{key,from:p.node,action:p.action,to:c.node});}
    }
    if(s.backedUp&&(!current||event.phase==='backup')){
      completed++;for(const c of s.choices){const n=nodes.get(c.node)!;n.visits++;n.sum+=s.reason==='win'?1000000n:0n;explored.add(c.node);}
    }
  }
  const s=r.simulations[event.simulation],limit=event.phase==='select'?event.choice+1:s.choices.length;
  const activeNodes=s.choices.slice(0,limit).map(c=>c.node),activeEdges=new Set<string>();
  for(let i=1;i<limit;i++){const p=s.choices[i-1];activeEdges.add(edgeKey(p.node,p.action,s.choices[i].node));}
  if(!activeNodes.length)activeNodes.push(0);
  return {nodes,edges,explored,activeNodes,activeEdges,frontier:null,completed};
}

export function validateUct(f:Frame,r:Recording){
  const u=f.search!.uctReplay!,sameKey=(a:Frame['key'],b:Frame['key'])=>JSON.stringify(a)===JSON.stringify(b);
  if(!sameKey(u.key,f.key)||u.treeDigest!==f.decisionSource?.treeDigest||f.decisionSource.artifactSha256!==r.identity.sourceSha256)throw new Error('UCT trace source binding differs');
  const validBoard=(id:number)=>id<u.boards.length;
  for(const b of u.boards){
    if(b.players.some(p=>p.hand.length))throw new Error('UCT sampled hands must stay hidden');
    const objects=[...b.players.flatMap(p=>[...p.battlefield,...p.graveyard]),...b.exile];
    if(new Set(objects.map(o=>o.id)).size!==objects.length||objects.some(o=>!r.cards[o.card]))throw new Error('UCT board object references differ');
  }
  const n=u.nodes,counts=n.map(v=>v.actions.map(()=>0)),wins=n.map(v=>v.actions.map(()=>0));
  for(const [id,node] of n.entries()){
    const k=node.actions.length;
    if(node.id!==id||node.actor!==r.perspective||!validBoard(node.board)||node.actions.some((a,i)=>a.index!==i)||node.edgeVisits.length!==k||node.edgeWins.length!==k||node.liveOrder.length!==k||new Set(node.liveOrder).size!==k||node.liveOrder.some(i=>i>=k))throw new Error('UCT node/action mapping differs');
    if(id===0?node.parent!==null:!node.parent||node.parent[0]>=id||node.parent[1]>=n[node.parent[0]].actions.length)throw new Error('UCT parent identity differs');
  }
  let transitions=0,completed=0,firstCombo:number|null=null;
  for(const [i,s] of u.simulations.entries()){
    if(s.ordinal!==i||s.backedUp!==['win','natural_nonwin'].includes(s.reason))throw new Error('UCT attempt/backup identity differs');
    if(s.backedUp&&(!s.terminal||s.terminal.kind!=='Natural terminal'||(s.reason==='win')!==(s.terminal.winner===r.perspective)||s.leaf===null))throw new Error('UCT terminal outcome differs');
    if(s.leaf!==null&&!validBoard(s.leaf))throw new Error('UCT leaf snapshot missing');
    if((s.backedUp&&!s.choices.length)||(s.choices.length&&s.choices[0].node!==0))throw new Error('UCT path does not start at root');
    for(const [j,c] of s.choices.entries()){
      const node=n[c.node];if(!node||c.action>=node.actions.length||!validBoard(c.board)||c.scores.length!==node.actions.length||c.scores.some(v=>v===null)||c.scores[c.action]!==c.scores.reduce<number>((best,v)=>Math.max(best,v??-Infinity),-Infinity))throw new Error('UCT selected scores or references differ');
      if(j&&JSON.stringify(node.parent)!==JSON.stringify([s.choices[j-1].node,s.choices[j-1].action]))throw new Error('UCT path connection differs');
      if(s.backedUp){counts[c.node][c.action]++;wins[c.node][c.action]+=Number(s.reason==='win');}
    }
    let stage=0;
    for(const [move,m] of s.rollout.entries()){
      if(m[0]>1||m[4]>=u.actions.length||[m[7],m[8]].some(id=>id!==null&&!validBoard(id)))throw new Error('UCT rollout reference differs');
      if(move&&m[1]!==s.rollout[move-1][1]+1)throw new Error('UCT rollout step ordering differs');
      if(m[0]===(r.perspective==='p0'?0:1)&&(m[5]&2))stage=Math.max(stage,1);
      if(stage===1&&(m[6]&2))stage=2;
      if(stage===2&&m[0]===(r.perspective==='p0'?0:1)&&(m[5]&4))stage=3;
      if(stage===3&&(m[6]&4))stage=4;
    }
    const byKey=new Map(s.rollout.map(m=>[`${m[1]}:${m[2]}:${m[3]}`,m]));
    for(const [j,c] of s.choices.entries()){
      const m=byKey.get(`${c.step}:${c.physical}:${c.substep}`);
      if(!m){if(s.reason==='truncated'&&j===s.choices.length-1&&(!s.rollout.length||c.step===s.rollout.at(-1)![1]+1))continue;throw new Error('UCT choice has no applied rollout move');}
      if(m[0]!==(r.perspective==='p0'?0:1)||u.actions[m[4]].index!==c.live)throw new Error('UCT tree/live rollout action differs');
    }
    if(s.combo!==(stage===4))throw new Error('UCT combo commentary has no ordered resolution evidence');
    if(firstCombo===null&&s.combo&&s.reason==='win')firstCombo=i;
    transitions+=s.rollout.length;completed+=Number(s.backedUp);
  }
  for(const node of n){
    if(node.visits!==node.edgeVisits.reduce((a,b)=>a+b,0)||node.edgeVisits.some((v,i)=>v!==counts[node.id][i])||node.edgeWins.some((v,i)=>v!==wins[node.id][i])||(!node.committed&&node.visits!==0))throw new Error('UCT replay does not reconcile native node statistics');
  }
  if(completed!==f.search!.simulations||transitions!==f.search!.transitions||n.filter(v=>v.committed).length!==f.search!.nodes||u.firstCombo!==firstCombo)throw new Error('UCT accounting or discovery marker differs');
  for(const [i,a] of n[0].actions.entries())if(JSON.stringify(a)!==JSON.stringify({...f.actions[n[0].liveOrder[i]],index:i}))throw new Error('UCT root semantic action mapping differs');
  for(const stat of f.search!.stats){const a=n[0].liveOrder.indexOf(stat.index);if(a<0||stat.visits!==n[0].edgeVisits[a]||BigInt(stat.valueSum)!==BigInt(n[0].edgeWins[a])*1000000n)throw new Error('UCT root live-action table differs');}
}

export async function verifyUctDigest(u:UctReplay){
  const bytes=new ArrayBuffer(u.nodes.reduce((n,v)=>n+16+8*v.actions.length,0)),view=new DataView(bytes);let offset=0;
  const put=(n:number)=>{view.setUint32(offset,n,true);offset+=4;};
  for(const n of u.nodes){[n.id,Number(n.committed),n.visits,n.actions.length].forEach(put);n.edgeVisits.forEach((v,i)=>{put(v);put(n.edgeWins[i]);});}
  const digest=[...new Uint8Array(await crypto.subtle.digest('SHA-256',bytes))].map(n=>n.toString(16).padStart(2,'0')).join('');
  if(digest!==u.tableDigest)throw new Error('UCT public table digest differs');
}
