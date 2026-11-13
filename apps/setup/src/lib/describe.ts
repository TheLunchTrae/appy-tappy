// How saved settings read on the Values and Applications pages.

import type { AntiCheatDef } from './bindings.ts';

/** What each anti-cheat class means for the actions AppyTappy may take. */
export const ANTI_CHEAT_NAMES: Readonly<Record<AntiCheatDef, string>> = {
  none: 'None',
  userMode: 'User-mode anti-cheat',
  kernelLevel: 'Kernel-level anti-cheat',
  unknown: 'Not set',
};

/**
 * The anti-cheat menu's choices: not set first, since every application starts there, and none last,
 * so stepping through the menu with the arrow keys reaches every other choice before the one that asks.
 */
export const ANTI_CHEAT_CHOICES: readonly AntiCheatDef[] = ['unknown', 'userMode', 'kernelLevel', 'none'];
