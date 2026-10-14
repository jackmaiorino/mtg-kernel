import { ArrowRight, GitBranch } from 'lucide-react';
import type { Frame } from './recording';

export function DecisionEvidence({ frame, onRoot, fullTrace=false }: { frame: Frame; onRoot: (id: string) => void;fullTrace?:boolean }) {
  const source=frame.decisionSource;
  if (!source) return null;
  const root=source.kind==='search-root';
  return <div className="decision-evidence">
    <span className="eyebrow"><GitBranch size={13}/> RECORDED SEARCH CONTROL</span>
    <strong>{root?'A search chose this move.':'Chosen from a searched continuation.'}</strong>
    <p>{root?'This search selected the greatest observed rollout win fraction among moves with at least eight completed backups.':fullTrace?'The executed game reused a supported continuation from the captured tree. Open its source search to inspect the simulations.':'The saved execution record identifies this move as a supported choice from the earlier search. Descendant action statistics were not retained.'}</p>
    {!root&&<button onClick={()=>onRoot(source.root)}>Inspect the source search <ArrowRight size={14}/></button>}
    <small>{fullTrace?'Tree choices, rollout actions, natural results and backups were recorded.':'Root tables were retained. Individual simulations were not recorded.'}</small>
  </div>;
}
