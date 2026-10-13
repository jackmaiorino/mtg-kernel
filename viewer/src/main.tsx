import React, { useEffect, useMemo, useReducer, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { LayoutGroup, MotionConfig, motion, useReducedMotion } from 'motion/react';
import { ArrowDown, ArrowLeft, ArrowRight, BookOpen, Check, ChevronDown, ChevronRight, CircleHelp, Cpu, ExternalLink, Eye, Focus, GitBranch, Layers, Maximize2, Pause, Play, RotateCcw, Shield, Sparkles, Upload, X } from 'lucide-react';
import { type Board, type CardObject, type Frame, type Recording, type Seat, MAX_BYTES, comparePolicyMasses, policyFavorite, isRoutine, objectList, probability, seatIndex } from './recording';
import { initialPlayback, playbackReducer } from './playback';
import { SearchReplayView } from './SearchReplay';
import { UctSearchReplayView } from './UctSearchReplay';
import { DecisionEvidence } from './DecisionEvidence';
import './style.css';

const glossary: Record<string, string> = {
  Policy: 'The model assigns a score to each legal move. Its sampler turns those scores into a distribution and selects one move.',
  Search: 'The agent explores hypothetical continuations before choosing. Visits show how much exploration each root move received.',
  Battlefield: 'Cards in play. Lands provide mana; creatures can attack and block. A sideways card is tapped.',
  Stack: 'Spells and abilities waiting to resolve. The most recently added item resolves first.',
  Mana: 'The resources used to pay for spells and abilities. W, U, B, R, G are the five colors; C is colorless.',
  Life: 'Players normally begin at 20 life. Reaching zero is one way to lose the game.',
};
const phaseName = (p: string) => p.replace(/_/g, ' ').replace(/([a-z])([A-Z])/g, '$1 $2').replace(/\bmain1\b/i, 'First main').replace(/\bmain2\b/i, 'Second main');
const percent = (n: number) => `${(n * 100).toFixed(n < .01 && n > 0 ? 2 : 1)}%`;
const cleanLabel = (s: string) => s.replace(/\s*\[c\d+\]/g, '');
const formatHash = (s: string) => s.length > 20 ? `${s.slice(0, 12)}…${s.slice(-6)}` : s;

function Help({ term, onHelp }: { term: string; onHelp: (term: string) => void }) {
  return <button className="help-icon" onClick={() => onHelp(term)} aria-label={`Explain ${term}`}><CircleHelp size={13} /></button>;
}

function CardView({ object, recording, assetBase, active, onSelect, animate, hand = false }: {
  object: CardObject; recording: Recording; assetBase: string; active: boolean;
  onSelect: (id: string) => void; animate: boolean; hand?: boolean;
}) {
  const card = recording.cards[object.card];
  const [imageFailed, setImageFailed] = useState(false);
  const image = card.image && assetBase ? `${assetBase}${card.image}` : null;
  return <motion.button layout={animate} layoutId={object.id} initial={false}
    animate={{ rotate: object.tapped ? 8 : 0 }} transition={{ duration: animate ? .38 : 0 }}
    className={`game-card ${active ? 'card-active' : ''} ${object.tapped ? 'tapped' : ''} ${hand ? 'hand-card' : ''}`}
    data-object={object.id} onClick={() => onSelect(object.id)}
    aria-label={`${card.name}${object.tapped ? ', tapped' : ''}${object.power !== null ? `, ${object.power}/${object.toughness}` : ''}`}>
    {image && !imageFailed ? <img src={image} alt="" onError={() => setImageFailed(true)} draggable={false} /> : <span className="text-card"><span className="text-card-cost">{card.manaCost}</span><strong>{card.name}</strong><span>{card.type}</span><span className="text-card-rules">{card.rules}</span></span>}
    <span className="card-name">{card.name}</span>
    {object.power !== null && <span className="card-stats">{object.power}/{object.toughness}</span>}
    {object.damage > 0 && <span className="damage-marker">{object.damage} damage</span>}
    {Object.entries(object.counters).some(([, n]) => n > 0) && <span className="counter-marker">{Object.entries(object.counters).filter(([, n]) => n > 0).map(([k, n]) => `${n} ${k.replace(/_/g, ' ')}`).join(', ')}</span>}
    {object.tapped && <span className="tapped-label">Tapped</span>}
  </motion.button>;
}

function Connections({ board, refs, container }: { board: Board; refs: string[]; container: React.RefObject<HTMLDivElement | null> }) {
  const [lines, setLines] = useState<{ x1: number; y1: number; x2: number; y2: number; kind: string }[]>([]);
  const [size, setSize] = useState({ width: 1, height: 1 });
  useEffect(() => {
    const el = container.current;
    if (!el) return;
    const update = () => {
      const box = el.getBoundingClientRect();
      const center = (id: string) => {
        const card = [...el.querySelectorAll<HTMLElement>('[data-object]')].find(node => node.dataset.object === id);
        if (!card) return null;
        const rect = card.getBoundingClientRect();
        return { x: rect.left - box.left + rect.width / 2, y: rect.top - box.top + rect.height / 2 };
      };
      const next = [];
      for (const combat of board.combat) for (const blocker of combat.blockers) {
        const a = center(combat.attacker), b = center(blocker);
        if (a && b) next.push({ x1: a.x, y1: a.y, x2: b.x, y2: b.y, kind: 'block' });
      }
      if (refs.length > 1) {
        const first = center(refs[0]);
        if (first) for (const id of refs.slice(1)) {
          const end = center(id);
          if (end) next.push({ x1: first.x, y1: first.y, x2: end.x, y2: end.y, kind: 'target' });
        }
      }
      setLines(next); setSize({ width: box.width, height: box.height });
    };
    const observer = new ResizeObserver(update); observer.observe(el); update();
    const timer = window.setTimeout(update, 450);
    return () => { observer.disconnect(); window.clearTimeout(timer); };
  }, [board, refs, container]);
  return <svg className="connections" width={size.width} height={size.height} aria-label="Recorded combat and selected action relationships" role="img">
    <defs><marker id="target-arrow" markerWidth="6" markerHeight="6" refX="5" refY="3" orient="auto"><path d="M0,0 L6,3 L0,6" fill="currentColor" /></marker></defs>
    {lines.map((line, i) => <path key={i} className={line.kind} d={`M${line.x1},${line.y1} C${line.x1},${(line.y1 + line.y2) / 2} ${line.x2},${(line.y1 + line.y2) / 2} ${line.x2},${line.y2}`} markerEnd="url(#target-arrow)" />)}
  </svg>;
}

function App() {
  const [recording, setRecording] = useState<Recording | null>(null);
  const [assetBase, setAssetBase] = useState('');
  const [demoGames,setDemoGames]=useState<{id:string;title?:string}[]>([]);
  const [status, setStatus] = useState('Loading the studio');
  const [error, setError] = useState('');
  const [playback, dispatch] = useReducer(playbackReducer, initialPlayback);
  const [selectedAction, setSelectedAction] = useState<number | null>(null);
  const [selectedCard, setSelectedCard] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);
  const [actionPage, setActionPage] = useState(0);
  const [help, setHelp] = useState<string | null>(null);
  const [showAbout, setShowAbout] = useState(false);
  const [zone, setZone] = useState<{ name: string; objects: CardObject[] } | null>(null);
  const [view, setView] = useState<'policy' | 'search'>('policy');
  const boardRef = useRef<HTMLDivElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const workerRef = useRef<Worker | null>(null);
  const loadToken = useRef(0);
  const loadedBase = useRef('');
  const reduceMotion = useReducedMotion();
  const frame = recording?.frames[playback.index];
  const board = frame ? playback.after ? frame.after : frame.board : null;
  const effectiveAction = selectedAction ?? frame?.executed ?? 0;
  const refs = useMemo(() => frame?.actions[effectiveAction]?.refs ?? [], [frame, effectiveAction]);
  const bookmark = recording?.bookmarks.find(b => b.frame === frame?.id);
  const selectedObject = board && selectedCard ? objectList(board).find(o => o.id === selectedCard) : null;
  const animation = playback.animate && !reduceMotion;
  const modalOpen = Boolean(help || selectedObject || zone || showAbout);

  function openText(text: string, base: string) {
    setError(''); setStatus('Validating recording'); loadedBase.current = base;
    dispatch({ type: 'reset' });
    workerRef.current?.postMessage({ text, token: ++loadToken.current });
  }
  useEffect(() => {
    const abort = new AbortController();
    const worker = new Worker(new URL('./recording.worker.ts', import.meta.url), { type: 'module' });
    workerRef.current = worker;
    worker.onmessage = (event: MessageEvent<{ token: number; recording?: Recording; error?: string }>) => {
      if (event.data.token !== loadToken.current) return;
      if (event.data.error) { setError(event.data.error); setStatus(''); return; }
      const r = event.data.recording!;
      setRecording(r); setAssetBase(loadedBase.current); setStatus('');
      const params = new URLSearchParams(window.location.hash.slice(1));
      const index = params.get('game') === r.id ? r.frames.findIndex(f => f.id === params.get('decision')) : 0;
      dispatch({ type: 'reset', index: Math.max(0, index) });
    };
    worker.onerror = () => { setError('The recording worker failed. Reload and try again.'); setStatus(''); };
    const params = new URLSearchParams(window.location.hash.slice(1));
    fetch(`${import.meta.env.BASE_URL}demo/manifest.json`, { signal: abort.signal }).then(response => {
      if (!response.ok) throw new Error('Demo recording unavailable. Import a viewer recording to explore it locally.');
      return response.json();
    }).then(async (manifest: { games: { id: string; file: string; sha256: string }[] }) => {
      setDemoGames(manifest.games);
      const game = manifest.games.find(g => g.id === params.get('game')) ?? manifest.games[0];
      if (!game || !/^[-a-zA-Z0-9_/]+\.json$/.test(game.file) || game.file.split('/').includes('..')) throw new Error('Invalid demo manifest');
      const response = await fetch(`${import.meta.env.BASE_URL}demo/${game.file}`, { signal: abort.signal });
      if (!response.ok) throw new Error('Unable to load the demo recording');
      const bytes = await response.arrayBuffer();
      if (bytes.byteLength > MAX_BYTES) throw new Error('Demo exceeds the viewer size limit');
      const digest = [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))].map(n => n.toString(16).padStart(2, '0')).join('');
      if (digest !== game.sha256) throw new Error('Demo recording hash mismatch');
      if (!abort.signal.aborted && loadToken.current === 0) openText(new TextDecoder().decode(bytes), `${import.meta.env.BASE_URL}demo/`);
    }).catch(error => { if (!abort.signal.aborted && loadToken.current === 0) { setStatus(''); setError(error.message); } });
    return () => { abort.abort(); worker.terminate(); workerRef.current = null; };
  }, []);
  useEffect(() => {
    if (!recording || !frame) return;
    setSelectedAction(null); setSelectedCard(null); setShowAll(false); setActionPage(0); setZone(null);
    if (frame?.search?.algorithm === 'persistent-uct' && !frame.policy) setView('search');
    const oldParams=new URLSearchParams(window.location.hash.slice(1));
    const nextParams=new URLSearchParams({game:recording.id,decision:frame.id});
    if(oldParams.get('game')===recording.id&&oldParams.get('decision')===frame.id&&oldParams.has('searchEvent'))nextParams.set('searchEvent',oldParams.get('searchEvent')!);
    window.history.replaceState(null, '', `#${nextParams}`);
  }, [recording, frame]);
  useEffect(() => {
    if (!playback.playing || !recording || !frame || modalOpen) return;
    const wait = (isRoutine(frame) ? 260 : playback.after ? 1000 : frame.search ? 2600 : 1800) / playback.speed;
    const timer = window.setTimeout(() => dispatch({ type: 'advance', length: recording.frames.length }), wait);
    return () => window.clearTimeout(timer);
  }, [playback, recording, frame, modalOpen]);
  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { setHelp(null); setSelectedCard(null); setZone(null); setShowAbout(false); return; }
      if (event.target instanceof HTMLElement && (['INPUT', 'SELECT', 'TEXTAREA', 'BUTTON'].includes(event.target.tagName) || event.target.isContentEditable)) return;
      if (!recording) return;
      if (event.code === 'Space') { event.preventDefault(); dispatch({ type: 'toggle' }); }
      if (event.key === 'ArrowRight') { event.preventDefault(); dispatch({ type: 'seek', index: Math.min(recording.frames.length - 1, playback.index + 1) }); }
      if (event.key === 'ArrowLeft') { event.preventDefault(); dispatch({ type: 'seek', index: Math.max(0, playback.index - 1) }); }
    };
    window.addEventListener('keydown', listener); return () => window.removeEventListener('keydown', listener);
  }, [recording, playback.index]);
  useEffect(() => {
    if (!modalOpen) return;
    const previous = document.activeElement as HTMLElement | null;
    const dialog = document.querySelector<HTMLElement>('[role="dialog"]');
    dialog?.querySelector<HTMLElement>('button')?.focus();
    const trap = (event: KeyboardEvent) => {
      if (event.key !== 'Tab') return;
      const elements = [...(dialog?.querySelectorAll<HTMLElement>('button, a, input, select, textarea') ?? [])];
      const first = elements[0], last = elements.at(-1);
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
    };
    document.addEventListener('keydown', trap);
    return () => { document.removeEventListener('keydown', trap); previous?.focus(); };
  }, [modalOpen]);
  useEffect(() => {
    const listener = () => {
      if (!recording) return;
      const params = new URLSearchParams(window.location.hash.slice(1));
      const game = params.get('game');
      if (game !== recording.id && demoGames.some(candidate => candidate.id === game)) {
        window.location.reload();
        return;
      }
      const index = recording.frames.findIndex(f => f.id === params.get('decision'));
      if (game === recording.id && index >= 0) dispatch({ type: 'seek', index });
    };
    window.addEventListener('hashchange', listener); return () => window.removeEventListener('hashchange', listener);
  }, [recording, demoGames]);

  const ranked = useMemo(() => {
    if (!frame) return [];
    const stats = new Map(frame.search?.stats.map(s => [s.index, s]));
    return frame.actions.map(action => ({ action, numerator: frame.policy?.masses[action.index] ?? '0', mass: frame.policy ? probability(frame.policy.masses[action.index]) : 0, stat: stats.get(action.index) }))
      .sort((a, b) => !(view==='search' ? frame.search : frame.policy) ? Number(b.action.index===frame.executed)-Number(a.action.index===frame.executed)||a.action.index-b.action.index : view === 'search' && frame.search ? (b.stat?.visits ?? 0) - (a.stat?.visits ?? 0) || a.action.index - b.action.index : comparePolicyMasses(a.numerator, b.numerator) || a.action.index - b.action.index);
  }, [frame, view]);
  const mostLikely = frame?.policy ? policyFavorite(frame.policy.masses) : -1;
  const hasDiagnostic = view === 'policy' ? Boolean(frame?.policy) : Boolean(frame?.search);
  const visitTotal = frame?.search?.stats.reduce((n, s) => n + s.visits, 0) ?? 0;
  const featured = recording ? seatIndex(recording.perspective) : 0;
  const opponent = 1 - featured;

  const playerBar = (index: number) => board && recording && <div className={`player-bar ${board.active === (index === 0 ? 'p0' : 'p1') ? 'player-active' : ''}`}>
    <div className="avatar">{index === featured ? <Cpu size={20} /> : <Shield size={20} />}</div>
    <div className="player-name"><strong>{recording.seats[index].name}</strong><span>{recording.seats[index].deck}</span></div>
    <div className="player-resources"><span><Layers size={13} /> {board.players[index].handCount} <span className="resource-caption">in hand</span></span><span>{board.players[index].libraryCount} <span className="resource-caption">in library</span></span></div>
    <div className="life"><motion.strong key={board.players[index].life} initial={animation ? { opacity: .4, scale: .9 } : false} animate={{ opacity: 1, scale: 1 }} transition={{ duration: .25 }}>{board.players[index].life}</motion.strong><span>Life <Help term="Life" onHelp={setHelp} /></span></div>
  </div>;
  const field = (index: number) => board && recording && <div className={`field field-${index === featured ? 'featured' : 'opponent'}`}>
    <div className="zone-caption"><span>Battlefield</span><span>{board.players[index].battlefield.length} cards</span></div>
    <div className="card-row">
      {board.players[index].battlefield.map(o => <CardView key={o.id} object={o} recording={recording} assetBase={assetBase} active={refs.includes(o.id) || selectedCard === o.id || board.combat.some(c => c.attacker === o.id)} onSelect={setSelectedCard} animate={animation} />)}
      {!board.players[index].battlefield.length && <span className="empty-zone">The battlefield is waiting</span>}
    </div>
    <div className="zone-buttons"><button onClick={() => setZone({ name: `${recording.seats[index].name} · Graveyard`, objects: board.players[index].graveyard })}>Graveyard <b>{board.players[index].graveyard.length}</b></button><button onClick={() => setZone({ name: 'Exile · Public cards', objects: board.exile })}>Exile <b>{board.exile.length}</b></button><span className="mana-label">Mana <Help term="Mana" onHelp={setHelp} /> {board.players[index].mana.map((n, i) => n ? <span key={i} className={`mana mana-${i}`}>{['W','U','B','R','G','C'][i]} {n}</span> : null)}</span></div>
  </div>;

  return <MotionConfig reducedMotion="user"><div className="studio">
    <header className="topbar" inert={modalOpen}><a className="brand" href={import.meta.env.BASE_URL} aria-label="MTG Kernel Decision Studio"><span className="brand-mark"><GitBranch size={20} /></span><span>MTG <b>KERNEL</b><small>DECISION STUDIO</small></span></a><div className="topbar-center"><span className="status-dot" /> Recorded gameplay</div><nav><button className="quiet-button" onClick={() => setShowAbout(true)}><BookOpen size={15} /><span>About the agent</span></button><button className="quiet-button" onClick={() => fileRef.current?.click()}><Upload size={15} /><span>Import recording</span></button><input ref={fileRef} hidden type="file" accept=".json,application/json" onChange={async event => { const file = event.target.files?.[0]; if (!file) return; if (file.size > MAX_BYTES) { setError('Recording exceeds the 64 MiB viewer limit'); return; } openText(await file.text(), ''); event.target.value = ''; }} /></nav></header>
    <main inert={modalOpen}>
      {recording&&demoGames.length>1&&<label className="game-selector">Recorded game <select aria-label="Recorded game" value={recording.id} onChange={e=>{const game=e.target.value;if(demoGames.some(g=>g.id===game))window.location.assign(`${import.meta.env.BASE_URL}?game=${encodeURIComponent(game)}#game=${encodeURIComponent(game)}`);}}>{!demoGames.some(g=>g.id===recording.id)&&<option value={recording.id}>Imported recording</option>}{demoGames.map(g=><option key={g.id} value={g.id}>{g.title??g.id.replace(/-/g,' ')}</option>)}</select></label>}
      <div className="intro"><div><div className="eyebrow"><span className="little-line" /> MACHINE INTELLIGENCE, AT THE TABLE</div><h1>Every move is a decision.</h1><p>Watch the game. Explore the alternatives. See what the agent chose.</p></div><button className="perspective" onClick={() => setHelp('Perspective')}><Eye size={16} /> Agent’s perspective</button></div>
      {status && <div role="status" className="notice">{status}…</div>}
      {error && <div role="alert" className="notice error"><span>{error}</span><button onClick={() => setError('')} aria-label="Dismiss error"><X size={16} /></button></div>}
      {!recording && !status && <div className="empty-studio"><GitBranch size={42} /><h2>Your next decision awaits.</h2><p>Import a viewer recording to open the battlefield and decision explorer.</p><button className="primary-button" onClick={() => fileRef.current?.click()}><Upload size={16} /> Import recording</button></div>}
      {recording && frame && board && <>
        <div className="game-heading"><div><span className="eyebrow">FEATURED GAME</span><h2>{recording.title}</h2></div><div className="game-meta"><span>{recording.complete ? 'Complete game' : 'Partial recording'}</span><span>{recording.reconstruction ? 'Reconstructed from recorded actions' : recording.coverage === 'every-step' ? 'Every step recorded' : 'Decision snapshots'}</span><button onClick={() => setShowAbout(true)}>{formatHash(recording.identity.checkpoint)} <ExternalLink size={12} /></button></div></div>
        <div className="workspace">
          <section className="game-panel" aria-label="Game replay">
            <LayoutGroup id={`board-${playback.epoch}`}><div className="battlefield" ref={boardRef} key={playback.epoch}>
              {playerBar(opponent)}{field(opponent)}
              <div className="phase-strip"><span className="phase-turn">ROUND {board.turn}</span><span className="phase-label">{phaseName(board.phase)}</span><span className="phase-priority">{recording.seats[seatIndex(board.priority)].name} has priority</span><Help term="Battlefield" onHelp={setHelp} /></div>
              {board.stack.length > 0 && <div className="stack-zone"><span><Layers size={14} /> Stack <Help term="Stack" onHelp={setHelp} /></span>{board.stack.map(s => <div key={s.id} data-object={s.source ?? undefined}><ArrowDown size={12} />{s.name}</div>)}</div>}
              {field(featured)}{playerBar(featured)}
              <div className="hand-zone"><div className="zone-caption"><span>Agent’s hand <Eye size={11} /></span><span>{board.players[featured].handCount} cards</span></div><div className="card-row hand-row">{board.players[featured].hand.map(o => <CardView key={o.id} object={o} recording={recording} assetBase={assetBase} active={refs.includes(o.id) || selectedCard === o.id} onSelect={setSelectedCard} animate={animation} hand />)}{!board.players[featured].hand.length && <span className="empty-zone">{board.players[featured].handCount === 0 ? 'No cards in hand' : 'No hand snapshot at this decision'}</span>}</div></div>
              <Connections board={board} refs={refs} container={boardRef} />
              {playback.after && frame.terminal && <div className="terminal-result"><span className="eyebrow">GAME COMPLETE</span><strong>{frame.terminal.winner ? `${recording.seats[seatIndex(frame.terminal.winner)].name} wins` : 'Recorded terminal'}</strong><span>{frame.terminal.kind}</span></div>}
            </div></LayoutGroup>
            <div className="transport"><div className="transport-top"><div className="transport-buttons"><button aria-label="First decision" onClick={() => dispatch({ type: 'seek', index: 0 })}><RotateCcw size={16} /></button><button aria-label="Previous decision" disabled={playback.index === 0} onClick={() => dispatch({ type: 'seek', index: playback.index - 1 })}><ArrowLeft size={17} /></button><button className="play-button" aria-label={playback.playing ? 'Pause playback' : 'Play recording'} onClick={() => dispatch({ type: 'toggle' })}>{playback.playing ? <Pause size={18} /> : <Play size={18} />}</button><button aria-label="Next decision" disabled={playback.index === recording.frames.length - 1} onClick={() => dispatch({ type: 'seek', index: playback.index + 1 })}><ArrowRight size={17} /></button></div><span className="decision-count">Decision <b>{playback.index + 1}</b> / {recording.frames.length}</span><label className="speed-select"><span className="sr-only">Playback speed</span><select value={playback.speed} onChange={e => dispatch({ type: 'speed', speed: Number(e.target.value) })}>{[.5,1,2,4].map(s => <option key={s} value={s}>{s}×</option>)}</select></label></div><div className="timeline"><input aria-label="Decision timeline" type="range" min={0} max={recording.frames.length - 1} value={playback.index} onChange={e => dispatch({ type: 'seek', index: Number(e.target.value) })} />{recording.bookmarks.map(b => <button key={b.frame} className="timeline-mark" style={{ left: `${recording.frames.findIndex(f => f.id === b.frame) / Math.max(1, recording.frames.length - 1) * 100}%` }} aria-label={`Jump to ${b.title}`} onClick={() => dispatch({ type: 'seek', index: recording.frames.findIndex(f => f.id === b.frame) })} />)}</div><div className="transport-caption"><span>{playback.after ? 'Observed result' : 'Before the choice'}</span><span>Space to play · ← → to step</span></div></div>
          </section>
          <aside className="inspector" aria-label="Decision explorer">
            <div className="inspector-header"><span className="inspector-icon"><Focus size={17} /></span><div><span className="eyebrow">INSIDE THE DECISION</span><h2>{frame.decisionSource ? 'A search-guided choice.' : frame.search ? 'Policy meets search.' : 'A choice, made visible.'}</h2></div></div>
            <div className="decision-context"><span>{recording.seats[seatIndex(frame.key.actor)].name} · Round {board.turn}</span><strong>{cleanLabel(frame.actions[frame.executed].label)}</strong><span className="executed-label"><Check size={12} /> Recorded action</span></div>
            <DecisionEvidence frame={frame} fullTrace={Boolean(recording.frames.find(f=>f.id===frame.decisionSource?.root)?.search?.uctReplay)} onRoot={id=>{setView('search');dispatch({type:'seek',index:recording.frames.findIndex(f=>f.id===id)});}}/>
            <div className="inspector-tabs" role="tablist" aria-label="Decision diagnostics"><button role="tab" aria-selected={view === 'policy'} className={view === 'policy' ? 'active' : ''} onClick={() => setView('policy')}><Cpu size={14} /> Policy</button><button role="tab" aria-selected={view === 'search'} className={view === 'search' ? 'active' : ''} onClick={() => setView('search')}><GitBranch size={14} /> Search {frame.search && <span className="tab-dot" />}</button></div>
            <div className="diagnostic-caption"><span>{hasDiagnostic ? view === 'policy' ? 'Recorded selection probability' : 'Share of search visits' : 'Legal choices · scores unavailable'}</span><Help term={view === 'policy' ? 'Policy' : 'Search'} onHelp={setHelp} /></div>
            {view === 'policy' && !frame.policy && <div className="unavailable"><Cpu size={23} /><strong>Policy distribution not recorded</strong><p>You can inspect the legal choices and recorded action below.</p></div>}
            {view === 'search' && !frame.search && <div className="unavailable"><GitBranch size={24} /><strong>{frame.decisionSource ? 'Descendant search table not retained' : 'Search not recorded'}</strong><p>{frame.decisionSource ? 'The saved execution identifies the source search above. Only its root comparisons are available.' : 'This decision contains no recorded search table.'}</p></div>}
            <div className="action-list" role="tabpanel">{ranked.slice(showAll ? actionPage * 50 : 0, showAll ? (actionPage + 1) * 50 : 5).map(({ action, mass, stat }, rank) => {
              const share = view === 'search' ? visitTotal ? (stat?.visits ?? 0) / visitTotal : 0 : mass;
              return <button key={action.index} className={`action-option ${effectiveAction === action.index ? 'selected' : ''}`} onClick={() => setSelectedAction(action.index)}><span className="action-heading"><span className="action-rank">{String(hasDiagnostic ? rank + 1 : action.index + 1).padStart(2,'0')}</span><span className="action-name">{cleanLabel(action.label)}</span>{hasDiagnostic&&<span className="action-probability">{percent(share)}</span>}</span>{hasDiagnostic&&<span className="probability-track"><motion.span initial={false} animate={{ width: `${share * 100}%` }} transition={{ duration: reduceMotion ? 0 : .3 }} /></span>}<span className="action-tags">{action.index === mostLikely && <span>Policy favorite</span>}{action.index === frame.policy?.sampled && <span>Policy sample</span>}{action.index === frame.executed && <span className="chosen-tag"><Check size={10} /> Played</span>}{stat && <span>{frame.search?.algorithm==='persistent-uct' ? `${Number(stat.valueSum)/1000000} wins / ${stat.visits.toLocaleString()} completed rollouts` : `Search: ${stat.visits.toLocaleString()} visits · value ${(Number(stat.meanValue) / 9000).toFixed(3)}`}</span>}</span></button>;
            })}{ranked.length > 5 && <button className="show-all" onClick={() => setShowAll(!showAll)}>{showAll ? hasDiagnostic ? 'Show leading five' : 'Show five choices' : `Explore all ${ranked.length} legal choices`}<ChevronDown size={13} /></button>}{showAll && ranked.length > 50 && <div className="action-pagination"><button disabled={actionPage === 0} onClick={() => setActionPage(p => p - 1)}>Previous 50</button><span>Page {actionPage + 1} / {Math.ceil(ranked.length / 50)}</span><button disabled={(actionPage + 1) * 50 >= ranked.length} onClick={() => setActionPage(p => p + 1)}>Next 50</button></div>}</div>
            {frame.search && <div className="search-summary"><GitBranch size={15} /><div><strong>{frame.policy && frame.policy.sampled !== frame.executed ? 'Search changed the policy sample' : 'Recorded search choice'}</strong><span>{frame.search.simulations.toLocaleString()} simulations · {frame.search.transitions.toLocaleString()} transitions · {frame.search.tier}</span></div></div>}
            {(frame.policy || frame.search) && <details className="value-details"><summary>Interpret these numbers <ChevronDown size={12} /></summary><p>Selection probability describes the sampler’s behavior. It is not confidence that a move is correct.</p><p>{view === 'search' && frame.search ? frame.search.valueScale : frame.policy?.valueScale}</p>{frame.policy && <p>Position value: {frame.policy.value.toFixed(3)}. Perspective: acting player.</p>}{frame.search && <p>Search visits measure exploration. Value estimates use the captured scale; these are not calibrated win probabilities.</p>}</details>}
            <div className="inspector-note"><Eye size={13} /><span>Recorded outputs. No model runs in your browser.</span></div>
            {frame.search?.replay||frame.search?.uctReplay ? <button className="search-watch" onClick={()=>{dispatch({type:'seek',index:playback.index});document.getElementById('search-replay')?.scrollIntoView({behavior:reduceMotion?'instant':'smooth',block:'start'});}}><GitBranch size={15}/>Watch this search unfold</button> : view==='search'&&frame.search&&<p className="search-replay-missing">Search replay not recorded at this decision.</p>}
          </aside>
        </div>
        {frame.search?.replay && <SearchReplayView key={`${recording.identity.sourceSha256}:${frame.id}`} frame={frame} recording={recording} assetBase={assetBase} onInteract={()=>{if(playback.playing||playback.after)dispatch({type:'seek',index:playback.index});}}/>}
        {frame.search?.uctReplay && <UctSearchReplayView key={`${recording.identity.sourceSha256}:${frame.id}`} frame={frame} recording={recording} assetBase={assetBase} onInteract={()=>{if(playback.playing||playback.after)dispatch({type:'seek',index:playback.index});}}/>}
        <section className="guided-section" aria-label="Guided moments"><div className="section-heading"><div><span className="eyebrow">LOOK A LITTLE CLOSER</span><h2>Three moments worth exploring.</h2></div><span className="authored-label"><BookOpen size={13} /> Authored commentary</span></div><div className="moment-grid">{recording.bookmarks.map((b, i) => <button key={b.frame} className={`moment ${b.frame === frame.id ? 'moment-active' : ''}`} onClick={() => dispatch({ type: 'seek', index: recording.frames.findIndex(f => f.id === b.frame) })}><span className="moment-number">0{i + 1}</span><span className="moment-content"><strong>{b.title}</strong><span>{b.situation}</span></span><ChevronRight size={17} /></button>)}</div>{bookmark && <div className="commentary" aria-live="polite"><span><b>Alternatives</b>{bookmark.alternatives}</span><span><b>Observed consequence</b>{bookmark.consequence}</span></div>}</section>
        <details className="event-log"><summary>Every recorded decision <span>{recording.frames.length} decisions</span><ChevronDown size={14} /></summary><div className="log-content">{(() => { const result = []; let i = 0; while (i < recording.frames.length) { const start = i; if (isRoutine(recording.frames[i])) { while (i < recording.frames.length && isRoutine(recording.frames[i])) i++; result.push(<details key={start}><summary>{i - start} routine priority passes</summary>{recording.frames.slice(start, i).map((f, offset) => <button key={f.id} onClick={() => dispatch({ type: 'seek', index: start + offset })}>#{start + offset + 1} · {cleanLabel(f.actions[f.executed].label)}</button>)}</details>); } else { const f = recording.frames[i]; result.push(<button key={f.id} onClick={() => dispatch({ type: 'seek', index: start })}>#{i + 1} · Round {f.board.turn} · {cleanLabel(f.actions[f.executed].label)}</button>); i++; } } return result; })()}</div></details>
      </>}
    </main>
    <footer inert={modalOpen}><span className="brand-footer">MTG KERNEL <span>/</span> DECISION STUDIO</span><span>Illustrative gameplay · Recorded evidence · Research in progress</span><a href="https://github.com/jackmaiorino/mtg-kernel" target="_blank" rel="noreferrer">Explore the project <ExternalLink size={12} /></a></footer>
    {(help || selectedObject || zone || showAbout) && <div className="modal-shade" onClick={() => { setHelp(null); setSelectedCard(null); setZone(null); setShowAbout(false); }}><div className="modal" role="dialog" aria-modal="true" aria-label={help ?? zone?.name ?? (selectedObject ? recording?.cards[selectedObject.card].name : 'About the agent')} onClick={e => e.stopPropagation()}><button className="modal-close" onClick={() => { setHelp(null); setSelectedCard(null); setZone(null); setShowAbout(false); }} aria-label="Close details"><X size={18} /></button>{help && <><span className="eyebrow">A LITTLE CONTEXT</span><h2>{help}</h2><p>{help === 'Perspective' ? 'The public battlefield and the featured agent’s hand are visible. Unrevealed opposing cards and library order are excluded from this recording.' : glossary[help]}</p></>}{selectedObject && recording && <><span className="eyebrow">CARD DETAILS</span><h2>{recording.cards[selectedObject.card].name}</h2><p>{recording.cards[selectedObject.card].manaCost} · {recording.cards[selectedObject.card].type}</p><p className="rules-text">{recording.cards[selectedObject.card].rules || 'Rules text was not captured for this card.'}</p><p>{selectedObject.tapped ? 'Tapped' : 'Untapped'}{selectedObject.power !== null ? ` · ${selectedObject.power}/${selectedObject.toughness}` : ''}</p></>}{zone && recording && <><span className="eyebrow">PUBLIC ZONE</span><h2>{zone.name}</h2><div className="modal-cards">{zone.objects.map(o => <CardView key={o.id} object={o} recording={recording} assetBase={assetBase} active={false} onSelect={id => { setZone(null); setSelectedCard(id); }} animate={false} />)}{!zone.objects.length && <p>No cards in this zone.</p>}</div></>}{showAbout && <><span className="eyebrow">THE PROJECT</span><h2>A model learning to play Magic.</h2><p>MTG Kernel combines a deterministic rules engine with a learned policy and value model. The policy scores legal moves; search explores continuations before selecting an action.</p><p>This studio displays recordings of that process. These games illustrate behavior, not competitive strength.</p>{recording&&<p>{recording.description}</p>}{recording && <dl className="provenance"><dt>Recording</dt><dd>{recording.id}</dd><dt>Checkpoint</dt><dd>{recording.identity.checkpoint}</dd><dt>Source</dt><dd>{recording.identity.source}</dd><dt>Runtime</dt><dd>{recording.identity.runtime}</dd><dt>Coverage</dt><dd>{recording.coverage}</dd>{recording.reconstruction&&<><dt>Original search source</dt><dd>{recording.reconstruction.originalSource}</dd><dt>Original search runtime</dt><dd>{recording.reconstruction.originalRuntime}</dd><dt>Source artifact</dt><dd>{recording.reconstruction.sourceArtifactSha256}</dd><dt>Rules profile</dt><dd>{recording.reconstruction.runtimeRules}</dd></>}</dl>}</>}</div></div>}
    <div className="sr-only" aria-live="polite">{frame && `Decision ${playback.index + 1}, ${playback.after ? 'after' : 'before'} ${cleanLabel(frame.actions[frame.executed].label)}`}</div>
  </div></MotionConfig>;
}

createRoot(document.getElementById('root')!).render(<React.StrictMode><App /></React.StrictMode>);
