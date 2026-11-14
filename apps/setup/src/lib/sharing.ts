// Sharing an application: the name its exported file is offered under, what the import dialog says of a shared
// file, and the dialog's states. wiki/State-machines.md maps the import.

import { type AntiCheatClass, LIMITS, type OriginDef, type SharedView } from './bindings.ts';

/** The name an export of the application named `name` is saved under, such as `appytappy-example-app.zip`. */
export function exportName(name: string): string {
  const words = name.toLowerCase().match(/[a-z0-9]+/g) ?? [];
  return `appytappy-${words.length === 0 ? 'application' : words.join('-')}.zip`;
}

/**
 * The anti-cheat class the import dialog starts on for a file that says `inFile`. A file never starts it on None,
 * which lets automations press keys and click, so None is only ever the user's own pick.
 */
export function classToConfirm(inFile: AntiCheatClass): AntiCheatClass {
  return inFile === 'none' ? 'unknown' : inFile;
}

/** How a value or a reference was made, as the Values and References pages say it. */
export function originText(origin: OriginDef): string {
  switch (origin.kind) {
    case 'byHand':
      return 'Made by hand';
    case 'recording':
      return 'Made with a recording';
    case 'imported':
      // A shared file says that its maker imported it, and not from which file of theirs.
      return origin.file === '' ? 'Imported from a file' : `Imported from ${origin.file}`;
  }
}

/**
 * How a shared file's values and references were made, from the `origins` of them all, as the import dialog says
 * it: the one way when they share it, and otherwise how many were made each way.
 */
export function originsText(origins: readonly OriginDef[]): string {
  const first = origins[0];
  if (first === undefined) {
    return 'Nothing made yet';
  }
  const counts = (['byHand', 'recording', 'imported'] as const).map(
    (kind) => origins.filter((origin) => origin.kind === kind).length,
  );
  if (counts.includes(origins.length)) {
    return originText(first.kind === 'imported' ? { kind: 'imported', file: '' } : first);
  }
  const [byHand = 0, recording = 0, imported = 0] = counts;
  return [
    byHand === 0 ? null : `${byHand} made by hand`,
    recording === 0 ? null : `${recording} made with a recording`,
    imported === 0 ? null : `${imported} imported from a file`,
  ]
    .filter((part) => part !== null)
    .join(', ');
}

/** Why a picked file of `bytes` bytes is not read, or null for one small enough to read. */
export function sizeProblem(file: string, bytes: number): string | null {
  return bytes > LIMITS.maxSharedBytes
    ? `AppyTappy cannot import ${file}: it holds more than ${LIMITS.maxSharedBytes / (1024 * 1024)} MB.`
    : null;
}

/** What the user is asked about a shared file that was read. */
interface Asked {
  /** The picked file's name. */
  readonly file: string;
  /** The file's bytes, which the import sends again. */
  readonly bytes: Uint8Array;
  readonly view: SharedView;
  /** The name the application is imported under. */
  readonly name: string;
  /** The anti-cheat class the user confirms. */
  readonly antiCheat: AntiCheatClass;
}

/** The import dialog's state. */
export type ImportState =
  | { readonly kind: 'closed' }
  /** The picked file is being read. */
  | { readonly kind: 'reading'; readonly file: string }
  /** The file cannot be imported; the dialog says why. */
  | { readonly kind: 'refused'; readonly file: string; readonly problem: string }
  /** The dialog shows what the file holds; `problem` is why the last import of it failed. */
  | ({ readonly kind: 'asking'; readonly problem: string | null } & Asked)
  | ({ readonly kind: 'importing' } & Asked);

export type ImportEvent =
  /** The user picked a file; `problem` is why it is not read at all. */
  | { readonly kind: 'picked'; readonly file: string; readonly problem: string | null }
  | { readonly kind: 'read'; readonly bytes: Uint8Array; readonly view: SharedView }
  /** The read or the import failed. */
  | { readonly kind: 'failed'; readonly problem: string }
  | { readonly kind: 'named'; readonly name: string }
  | { readonly kind: 'classed'; readonly antiCheat: AntiCheatClass }
  | { readonly kind: 'import' }
  | { readonly kind: 'imported' }
  | { readonly kind: 'close' };

export const NO_IMPORT: ImportState = { kind: 'closed' };

/** Whether the dialog waits for an answer, so it cannot be closed or changed. */
export function importBusy(state: ImportState): boolean {
  return state.kind === 'reading' || state.kind === 'importing';
}

/** The import dialog's next state. An event that does not fit the state changes nothing. */
export function nextImport(state: ImportState, event: ImportEvent): ImportState {
  switch (event.kind) {
    case 'picked':
      if (state.kind !== 'closed') {
        return state;
      }
      return event.problem === null
        ? { kind: 'reading', file: event.file }
        : { kind: 'refused', file: event.file, problem: event.problem };
    case 'read':
      return state.kind === 'reading'
        ? {
            kind: 'asking',
            file: state.file,
            bytes: event.bytes,
            view: event.view,
            name: event.view.name,
            antiCheat: classToConfirm(event.view.file.antiCheat),
            problem: null,
          }
        : state;
    case 'failed':
      if (state.kind === 'reading') {
        return { kind: 'refused', file: state.file, problem: event.problem };
      }
      return state.kind === 'importing' ? { ...state, kind: 'asking', problem: event.problem } : state;
    case 'named':
      return state.kind === 'asking' ? { ...state, name: event.name } : state;
    case 'classed':
      return state.kind === 'asking' ? { ...state, antiCheat: event.antiCheat } : state;
    case 'import': {
      if (state.kind !== 'asking') {
        return state;
      }
      const { problem: _shown, ...asked } = state;
      return { ...asked, kind: 'importing' };
    }
    case 'imported':
      return state.kind === 'importing' ? NO_IMPORT : state;
    case 'close':
      return importBusy(state) ? state : NO_IMPORT;
  }
}
