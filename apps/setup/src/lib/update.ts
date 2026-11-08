// What the update notice in the nav shows: nothing until a check finds a newer release, then an offer, the download,
// the downloaded release waiting to install, the question that asks before the update closes everything, and the
// installer starting. wiki/State-machines.md maps it.

/** The update notice's state. */
export type UpdateState =
  | { readonly kind: 'none' }
  | { readonly kind: 'offered'; readonly version: string; readonly problem: string | null }
  | { readonly kind: 'downloading'; readonly version: string }
  | { readonly kind: 'ready'; readonly version: string }
  | { readonly kind: 'asking'; readonly version: string }
  | { readonly kind: 'installing'; readonly version: string };

export type UpdateEvent =
  /** The check on opening answered: a newer release's version, or null. */
  | { readonly kind: 'checked'; readonly version: string | null }
  | { readonly kind: 'update' }
  /** The download finished, with the version it downloaded, which can be newer than the offer. */
  | { readonly kind: 'downloaded'; readonly version: string }
  | { readonly kind: 'install' }
  | { readonly kind: 'keep' }
  | { readonly kind: 'confirm' }
  /** The download, or the install before its installer started, failed. */
  | { readonly kind: 'failed'; readonly problem: string };

export const NO_UPDATE: UpdateState = { kind: 'none' };

/** The notice's next state. An event that does not fit the state changes nothing. */
export function nextUpdate(state: UpdateState, event: UpdateEvent): UpdateState {
  switch (event.kind) {
    case 'checked':
      if (state.kind !== 'none' || event.version === null) {
        return state;
      }
      return { kind: 'offered', version: event.version, problem: null };
    case 'update':
      return state.kind === 'offered' ? { kind: 'downloading', version: state.version } : state;
    case 'downloaded':
      return state.kind === 'downloading' ? { kind: 'ready', version: event.version } : state;
    case 'install':
      return state.kind === 'ready' ? { kind: 'asking', version: state.version } : state;
    case 'keep':
      return state.kind === 'asking' ? { kind: 'ready', version: state.version } : state;
    case 'confirm':
      return state.kind === 'asking' ? { kind: 'installing', version: state.version } : state;
    case 'failed':
      return state.kind === 'downloading' || state.kind === 'installing'
        ? { kind: 'offered', version: state.version, problem: event.problem }
        : state;
  }
}
