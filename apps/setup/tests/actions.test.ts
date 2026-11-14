import { describe, expect, test } from 'bun:test';

import {
  ACTION_CHOICES,
  type ActionDraft,
  type ActionKind,
  UNDO_PUTS_BACK,
  actionText,
  buildAction,
  draftedAction,
  inputBlocked,
  isPicked,
  newAction,
  undoRefused,
  undoText,
  unusedKey,
} from '../src/lib/actions.ts';
import type { ActionDef, AntiCheatClass, AppSummary } from '../src/lib/bindings.ts';

function app(id: string, name: string, antiCheat: AntiCheatClass): AppSummary {
  return {
    id,
    file: {
      format: 1,
      name,
      window: {
        title: name,
        class: '',
        executable: `${name}.exe`,
        mustMatch: { title: false, class: false },
        fallback: 'none',
      },
      sizeChanges: 'often',
      antiCheat,
      values: [],
      references: [],
    },
    error: null,
    windows: [],
    running: null,
  };
}

/** A saved action of every kind and how it reads, so a kind the Rust side adds does not compile until it is here. */
const SAVED: { readonly [Kind in ActionKind]: readonly (readonly [Extract<ActionDef, { kind: Kind }>, string])[] } = {
  media: [
    [{ kind: 'media', command: 'pause', app: 'Player.exe' }, 'Pause media · Player.exe'],
    [{ kind: 'media', command: 'next', app: null }, 'Next track · whatever is playing'],
  ],
  volume: [
    [{ kind: 'volume', percent: 20, app: null }, 'Volume · system 20%'],
    [{ kind: 'volume', percent: 0, app: 'Player.exe' }, 'Volume · Player.exe 0%'],
  ],
  keys: [[{ kind: 'keys', keys: 'Ctrl+Shift+M' }, 'Press Ctrl+Shift+M']],
  open: [[{ kind: 'open', target: 'https://example.com' }, 'Open https://example.com']],
  sound: [
    [{ kind: 'sound', file: 'C:\\Sounds\\Ding.wav' }, 'Play Ding.wav'],
    [{ kind: 'sound', file: 'C:/Sounds/Ding.wav' }, 'Play Ding.wav'],
  ],
  click: [
    [
      {
        kind: 'click',
        app: 'streaming',
        button: 'right',
        delivery: 'message',
        returnFocus: false,
        at: { kind: 'spot', corner: 'topRight', x: 40, y: 12 },
      },
      'Right click · Streaming app',
    ],
  ],
};

const saved: readonly (readonly [ActionDef, string])[] = Object.values(SAVED).flat();

describe('a saved action', () => {
  test('reads in a few words', () => {
    for (const [action, text] of saved) {
      expect(actionText(action, (id) => (id === 'streaming' ? 'Streaming app' : id))).toBe(text);
    }
  });

  test('round-trips through its row', () => {
    for (const [action] of saved) {
      expect(buildAction(draftedAction(action, 3), 1)).toEqual(action);
    }
  });

  test('is a new row of its kind but for what it says', () => {
    expect(draftedAction({ kind: 'keys', keys: 'M' }, 3)).toEqual({ ...newAction(3, 'keys'), keys: 'M' });
    expect(draftedAction({ kind: 'volume', percent: 5, app: 'Player.exe' }, 0)).toEqual({
      ...newAction(0, 'volume'),
      percent: '5',
      target: 'app',
      app: 'Player.exe',
    });
  });
});

describe('the action menu', () => {
  test('offers each media command, then every other kind', () => {
    expect(ACTION_CHOICES.map((choice) => choice.label)).toEqual([
      'Pause media',
      'Play media',
      'Next track',
      'Previous track',
      'Set volume',
      'Press keys',
      'Open a program, file, or link',
      'Play a sound',
      'Click in another application',
    ]);
  });

  test('shows the one choice a row is', () => {
    const shown = (row: ActionDraft): readonly string[] =>
      ACTION_CHOICES.filter((choice) => isPicked(choice, row)).map((choice) => choice.label);
    expect(shown(newAction(0))).toEqual(['Pause media']);
    expect(shown({ ...newAction(0), command: 'previous' })).toEqual(['Previous track']);
    // A row keeps its media command while it is another kind.
    expect(shown({ ...newAction(0, 'sound'), command: 'previous' })).toEqual(['Play a sound']);
    for (const [action] of saved) {
      expect(shown(draftedAction(action, 0))).toHaveLength(1);
    }
  });
});

describe('a row says what to fill in next', () => {
  const next = (row: Partial<ActionDraft>): ActionDef | string => buildAction({ ...newAction(1), ...row }, 2);

  test('for the application a media or volume action names', () => {
    expect(next({ target: 'app', app: '  ' })).toBe('Name the application for action 2, such as Player.exe.');
    const longest = `${'a'.repeat(252)}.exe`;
    expect(next({ target: 'app', app: ` ${longest} ` })).toEqual({ kind: 'media', command: 'pause', app: longest });
    expect(next({ target: 'app', app: `a${longest}` })).toBe(
      'The application name in action 2 is too long to save. Shorten it.',
    );
    expect(next({ kind: 'volume', target: 'app', app: 'Player\t.exe' })).toBe(
      'The application name in action 2 has a tab or line break in it. Remove it.',
    );
    // A name typed before the row went back to whatever is playing is not used.
    expect(next({ target: 'default', app: 'Player.exe' })).toEqual({ kind: 'media', command: 'pause', app: null });
  });

  test('for a volume', () => {
    for (const percent of ['', '101', '-1', '2.5', 'x']) {
      expect(next({ kind: 'volume', percent })).toBe('Set the volume of action 2 to a whole number from 0 to 100.');
    }
    expect(next({ kind: 'volume', percent: ' 30 ' })).toEqual({ kind: 'volume', percent: 30, app: null });
  });

  test('for keys, in the one spelling the file keeps', () => {
    expect(next({ kind: 'keys', keys: '  ' })).toBe('Choose the keys for action 2 to press.');
    expect(next({ kind: 'keys', keys: 'Ctrl+Mute' })).toBe(
      'Spell the keys for action 2 as any of Ctrl, Alt, Shift and Win, then one key such as M, 5, F9, Space or Enter, joined by +, such as Ctrl+Shift+M.',
    );
    expect(next({ kind: 'keys', keys: ' ctrl + m ' })).toEqual({ kind: 'keys', keys: 'Ctrl+M' });
  });

  test('for what opens: a full path or a web link, as typed or copied as a path', () => {
    const opens = (text: string): ActionDef | string => next({ kind: 'open', opens: text });
    expect(opens(' "C:\\Program Files\\Timer\\Timer.exe" ')).toEqual({
      kind: 'open',
      target: 'C:\\Program Files\\Timer\\Timer.exe',
    });
    expect(opens(' HTTPS://example.com/timer ')).toEqual({ kind: 'open', target: 'HTTPS://example.com/timer' });
    expect(opens(' "" ')).toBe(
      "Enter what action 2 opens: a program's full path, such as C:\\Tools\\Timer.exe, or a web link.",
    );
    expect(opens('d:/notes.txt')).toEqual({ kind: 'open', target: 'd:/notes.txt' });
    for (const target of [
      'Timer.exe',
      'C:Timer.exe',
      'ftp://example.com',
      'search-ms:query=x',
      '\\\\server\\share\\run.bat',
      'https://',
      'https://a b',
    ]) {
      expect(opens(target)).toBe(
        'Action 2 opens a program or file by its full path, such as C:\\Tools\\Timer.exe, or a link that starts with http:// or https://.',
      );
    }
    const longest = `https://${'a'.repeat(2040)}`;
    expect(opens(longest)).toEqual({ kind: 'open', target: longest });
    expect(opens(`${longest}a`)).toBe('The path or link in action 2 is too long to save. Shorten it.');
  });

  test('for a sound: a WAV file by its full path', () => {
    const plays = (text: string): ActionDef | string => next({ kind: 'sound', sound: text });
    expect(plays('"C:\\Sounds\\Ding.WAV"')).toEqual({ kind: 'sound', file: 'C:\\Sounds\\Ding.WAV' });
    expect(plays('  ')).toBe('Enter the full path of the WAV file action 2 plays, such as C:\\Sounds\\Ding.wav.');
    for (const file of ['Ding.wav', 'C:\\Sounds\\Ding.mp3', 'https://example.com/ding.wav', '\\\\server\\ding.wav']) {
      expect(plays(file)).toBe('Action 2 plays a WAV file by its full path, such as C:\\Sounds\\Ding.wav.');
    }
    expect(plays('C:\\Sounds\tDing.wav')).toBe(
      'The path or link in action 2 has a tab or line break in it. Remove it.',
    );
  });

  test('for a click: its application, then its target', () => {
    const at = { kind: 'spot', corner: 'topLeft', x: 1, y: 2 } as const;
    expect(next({ kind: 'click', at })).toBe('Pick the application action 2 clicks in.');
    expect(next({ kind: 'click', clickApp: 'streaming' })).toBe('Set up where action 2 clicks.');
  });
});

describe('unusedKey', () => {
  test('is a key no row has, also after rows were removed', () => {
    expect(unusedKey([])).toBe(0);
    expect(unusedKey([newAction(1), newAction(2)])).toBe(3);
    expect(unusedKey([newAction(4), newAction(0)])).toBe(5);
  });
});

describe('anti-cheat', () => {
  const watched = (antiCheat: AntiCheatClass) => ({ name: 'Example App', antiCheat });
  const keys = newAction(0, 'keys');
  const click = (clickApp: string): ActionDraft => ({ ...newAction(1, 'click'), clickApp });
  const apps = [app('streaming', 'Streaming app', 'userMode'), app('notes', 'Notes', 'none')];
  const noKeys =
    "Anti-cheat can treat pressed keys as cheating, so keys are pressed only once Example App's anti-cheat is set to None, on the Applications page or when you add or edit it.";
  const noClick = (what: string): string =>
    `${what}, so AppyTappy cannot click while it runs: anti-cheat can see clicks that AppyTappy makes. A click needs the watched application and the one it clicks in both set to None, on the Applications page or when you add or edit them.`;

  test('blocks only the actions that send input', () => {
    const others = (['media', 'volume', 'open', 'sound'] as const).map((kind) => newAction(0, kind));
    expect(inputBlocked(others, watched('kernelLevel'), apps)).toEqual([]);
    expect(inputBlocked([...others, keys], watched('kernelLevel'), apps)).toEqual([noKeys]);
    expect(inputBlocked([keys, click('notes')], watched('none'), apps)).toEqual([]);
  });

  test('blocks a click by the watched application or the one it clicks in', () => {
    expect(inputBlocked([click('streaming')], watched('none'), apps)).toEqual([
      noClick("Streaming app's anti-cheat is user-mode"),
    ]);
    expect(inputBlocked([click('notes')], watched('unknown'), apps)).toEqual([
      noClick("Example App's anti-cheat is not set"),
    ]);
    // Anti-cheat can see a click however it is sent.
    for (const delivery of ['cursor', 'message'] as const) {
      expect(inputBlocked([{ ...click('streaming'), delivery }], watched('none'), apps)).toHaveLength(1);
    }
    // An application that is not saved has no setting to name, and a row without one has nothing to say yet.
    expect(inputBlocked([click('gone')], watched('none'), apps)).toEqual([]);
    expect(inputBlocked([click('')], watched('unknown'), apps)).toEqual([]);
  });

  test('says each kind of input once, keys first', () => {
    expect(inputBlocked([click('notes'), click('streaming'), keys, keys], watched('unknown'), apps)).toEqual([
      noKeys,
      noClick("Example App's anti-cheat is not set"),
    ]);
  });
});

describe('undo', () => {
  const pause: ActionDef = { kind: 'media', command: 'pause', app: null };
  const play: ActionDef = { kind: 'media', command: 'play', app: null };
  const skip: ActionDef = { kind: 'media', command: 'next', app: null };
  const volume: ActionDef = { kind: 'volume', percent: 5, app: null };
  const keys: ActionDef = { kind: 'keys', keys: 'M' };
  const open: ActionDef = { kind: 'open', target: 'https://example.com' };
  const sound: ActionDef = { kind: 'sound', file: 'C:\\Sounds\\Ding.wav' };
  const click: ActionDef = {
    kind: 'click',
    app: 'streaming',
    button: 'left',
    delivery: 'cursor',
    returnFocus: true,
    at: { kind: 'spot', corner: 'topLeft', x: 1, y: 2 },
  };

  test('says what it puts back', () => {
    expect(undoText([pause, volume, volume])).toBe(
      'Paused media plays again and each volume goes back to where it was.',
    );
    expect(undoText([play])).toBe('Started media pauses again.');
    expect(undoText([volume, play, skip, pause, keys])).toBe(
      'Paused media plays again, started media pauses again and each volume goes back to where it was.',
    );
    expect(undoText([skip, keys])).toBeNull();
    expect(UNDO_PUTS_BACK).toBe(
      'Undo puts back what it can: paused media plays again, started media pauses again and each volume goes back to where it was.',
    );
  });

  test('is refused when it can put nothing back, saying why', () => {
    expect(undoRefused([skip, pause])).toBeNull();
    expect(undoRefused([keys, volume])).toBeNull();
    expect(undoRefused([skip, { ...skip, command: 'previous' }])).toBe(
      'Turn off undo: a skipped track cannot be unskipped.',
    );
    expect(undoRefused([keys, skip])).toBe(
      'Turn off undo: a skipped track cannot be unskipped, and pressed keys cannot be unpressed.',
    );
    expect(undoRefused([click, sound, open, keys, skip])).toBe(
      'Turn off undo: a skipped track cannot be unskipped, pressed keys cannot be unpressed, what opened stays open, a played sound cannot be unplayed, and a click cannot be undone.',
    );
  });
});
