export type Playback = { index: number; playing: boolean; speed: number; epoch: number; animate: boolean; after: boolean };
export type PlaybackAction = { type: 'seek'; index: number } | { type: 'toggle' } | { type: 'speed'; speed: number } | { type: 'advance'; length: number } | { type: 'reset'; index?: number };
export const initialPlayback: Playback = { index: 0, playing: false, speed: 1, epoch: 0, animate: false, after: false };
export function playbackReducer(state: Playback, action: PlaybackAction): Playback {
  switch (action.type) {
    case 'reset': return { ...initialPlayback, index: action.index ?? 0, epoch: state.epoch + 1 };
    case 'seek': return { ...state, index: action.index, playing: false, after: false, animate: false, epoch: state.epoch + 1 };
    case 'toggle': return { ...state, playing: !state.playing, animate: false, epoch: state.epoch + 1 };
    case 'speed': return { ...state, speed: action.speed, epoch: state.epoch + 1 };
    case 'advance':
      if (!state.after) return { ...state, after: true, animate: true };
      if (state.index >= action.length - 1) return { ...state, playing: false, animate: false };
      return { ...state, index: state.index + 1, after: false, animate: true };
  }
}
