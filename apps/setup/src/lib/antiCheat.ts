// An application's anti-cheat setting, and what it keeps AppyTappy from doing: anti-cheat can see input another
// program sends, so keys are pressed and clicks made only where it is None, as the runtime also checks.

import type { AntiCheatClass, AntiCheatPick } from './bindings.ts';

/** What each anti-cheat class is called in the anti-cheat menu. */
export const ANTI_CHEAT_NAMES: Readonly<Record<AntiCheatClass, string>> = {
  none: 'None',
  userMode: 'User-mode anti-cheat',
  kernelLevel: 'Kernel-level anti-cheat',
  unknown: 'Not set',
};

/**
 * The anti-cheat menu's choices: not set first, since every application starts there, and none last,
 * so stepping through the menu with the arrow keys reaches every other choice before the one that asks.
 */
export const ANTI_CHEAT_CHOICES: readonly AntiCheatClass[] = ['unknown', 'userMode', 'kernelLevel', 'none'];

/** A saved application, as the setting bears on it. */
export interface Guarded {
  readonly name: string;
  readonly antiCheat: AntiCheatClass;
}

/** A class after `anti-cheat is`, such as `user-mode`: its menu name without the words the sentence has already. */
function inWords(antiCheat: AntiCheatClass): string {
  return ANTI_CHEAT_NAMES[antiCheat].toLowerCase().replace(/ anti-cheat$/u, '');
}

/** Why keys cannot be pressed into the application `watched`, or null when they can. */
export function keysBlocked(watched: Guarded): string | null {
  return watched.antiCheat === 'none'
    ? null
    : `Anti-cheat can treat pressed keys as cheating, so keys are pressed only once ${watched.name}'s anti-cheat is set to None, on the Applications page or when you add or edit it.`;
}

/**
 * Why a click cannot run between the application `watched` and the one it clicks in, or null when it can: both must
 * have none. `clicked` is null when the application clicked in is not saved, which leaves only the watched one to
 * say.
 */
export function clickBlocked(watched: Guarded, clicked: Guarded | null): string | null {
  const blocking = [watched, clicked].find((app) => app !== null && app.antiCheat !== 'none');
  return blocking === undefined || blocking === null
    ? null
    : `${blocking.name}'s anti-cheat is ${inWords(blocking.antiCheat)}, so AppyTappy cannot click while it runs: anti-cheat can see clicks that AppyTappy makes. A click needs the watched application and the one it clicks in both set to None, on the Applications page or when you add or edit them.`;
}

/** The class saved now, which a pick in the application editor is made over: not set for a new application. */
function savedClass(saved: AntiCheatClass | null): AntiCheatClass {
  return saved ?? 'unknown';
}

/** The class the application editor shows: the one picked there, or else the saved one `saved`. */
export function shownAntiCheat(pick: AntiCheatPick | null, saved: AntiCheatClass | null): AntiCheatClass {
  return pick?.to ?? savedClass(saved);
}

/**
 * What the application editor keeps when `choice` is picked over the saved class `saved`: null when it is that class,
 * so the save leaves the setting as it is.
 */
export function pickAntiCheat(choice: AntiCheatClass, saved: AntiCheatClass | null): AntiCheatPick | null {
  const from = savedClass(saved);
  return choice === from ? null : { from, to: choice };
}

/**
 * A pick a draft kept, as the application editor resumes it over the saved class `saved`: dropped once the class it
 * was made over is no longer saved, as after the Applications page changed it, so the editor shows the class saved
 * now and asks again before it sets None.
 */
export function resumedPick(pick: AntiCheatPick | null, saved: AntiCheatClass | null): AntiCheatPick | null {
  return pick?.from === savedClass(saved) ? pick : null;
}
