// Which overlay a window shows, from the label its Rust side gave it.

import type { OverlayMode } from './api.ts';

/** The application and mode an overlay window's `label` names, or null for the main window. */
export function overlayOf(label: string): { readonly app: string; readonly mode: OverlayMode } | null {
  for (const [prefix, mode] of [
    ['overlay-', 'overlay'],
    ['panel-', 'pictureInPicture'],
  ] as const) {
    if (label.startsWith(prefix) && label.length > prefix.length) {
      return { app: label.slice(prefix.length), mode };
    }
  }
  return null;
}
