import { ArrowRight, GitBranch } from 'lucide-react';
import type { Frame } from './recording';

export function DecisionEvidence({ frame, onRoot }: { frame: Frame; onRoot: (id: string) => void }) {
  const source=frame.decisionSource;
  if (!source) return null;
  const root=source.kind==='search-root';
  return <div className="decision-evidence">
    <span className="eyebrow"><GitBranch size={13}/> RECORDED SEARCH CONTROL</span>
    <strong>{root?'A search chose this move.':'Chosen from a searched continuation.'}</strong>
    <p>{root?'This historical search selected the greatest observed rollout win fraction among moves with at least eight completed backups.':'The saved execution record identifies this move as a supported choice from the earlier search. Descendant action statistics were not retained.'}</p>
    {!root&&<button onClick={()=>onRoot(source.root)}>Inspect the source search <ArrowRight size={14}/></button>}
    <small>Root tables were retained. Individual simulations were not recorded.</small>
  </div>;
}
