// How saved settings read on the Values and Applications pages.

import type { AntiCheat, CheckDef } from './api.ts';
import type { Color } from './pixels.ts';

/** A saved check, in a few words. */
export function checkSummary(check: CheckDef): { readonly name: string; readonly colors: readonly Color[] } {
  switch (check.kind) {
    case 'number':
      return {
        name: `Number, learned from ${check.samples.length} ${check.samples.length === 1 ? 'picture' : 'pictures'}`,
        colors: [check.ink],
      };
    case 'barFill':
      return { name: 'Bar fill', colors: [check.filled, check.empty] };
    case 'color':
      return { name: 'Color', colors: [check.color] };
    case 'reference':
      return { name: 'Picture', colors: [] };
  }
}

/** What each anti-cheat class means for the actions AppyTappy may take. */
export const ANTI_CHEAT_NAMES: Readonly<Record<AntiCheat, string>> = {
  none: 'None',
  userMode: 'User-mode anti-cheat',
  kernelLevel: 'Kernel-level anti-cheat',
  unknown: 'Not set',
};
