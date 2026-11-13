// How saved settings read on the Values and Applications pages.

import type { AntiCheatDef, CheckDef, ReferenceDef } from './bindings.ts';
import { knownCharacters } from './reference.ts';
import type { Color } from './pixels.ts';

/** A saved check, in a few words, with its application's `references`. */
export function checkSummary(
  check: CheckDef,
  references: readonly ReferenceDef[],
): { readonly name: string; readonly colors: readonly Color[] } {
  switch (check.kind) {
    case 'number': {
      const reference = references.find((saved) => saved.id === check.reference);
      if (reference === undefined) {
        return { name: 'Number, its reference is missing', colors: [] };
      }
      return {
        name:
          `Number, read with ${reference.name}${check.whole ? ', whole only' : ''}` +
          (check.characters === undefined ? '' : `, reading ${knownCharacters([check.characters]).join('')}`),
        colors: [reference.ink],
      };
    }
    case 'barFill':
      return { name: 'Bar fill', colors: [check.filled, check.empty] };
    case 'color':
      return { name: 'Color', colors: [check.color] };
    case 'state':
      return { name: `State: ${check.states.map((state) => state.name).join(', ')}`, colors: [] };
  }
}

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
