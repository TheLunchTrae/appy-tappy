// An application's anti-cheat setting, and what it keeps AppyTappy from doing: anti-cheat can see input another
// program sends, so keys are pressed and clicks made only where it is None, as the runtime also checks.

import type { AntiCheatClass } from './bindings.ts';

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
    : `Anti-cheat can treat pressed keys as cheating, so keys are pressed only once ${watched.name}'s anti-cheat is set to None on the Applications page.`;
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
    : `${blocking.name}'s anti-cheat is ${inWords(blocking.antiCheat)}, so AppyTappy cannot click while it runs: anti-cheat can see clicks that AppyTappy makes. A click needs the watched application and the one it clicks in both set to None on the Applications page.`;
}

/**
 * The class the application editor shows: the one picked there, or else the saved one, which a new application
 * starts at not set.
 */
export function shownAntiCheat(picked: AntiCheatClass | null, saved: AntiCheatClass | null): AntiCheatClass {
  return picked ?? saved ?? 'unknown';
}

/**
 * What the application editor keeps of the class `choice`, picked there or kept in a draft: null when it is the
 * saved class `saved`, so the save leaves the setting as it is, and never puts back a class that the Applications
 * page set since.
 */
export function pickedAntiCheat(choice: AntiCheatClass | null, saved: AntiCheatClass | null): AntiCheatClass | null {
  return choice === shownAntiCheat(null, saved) ? null : choice;
}
