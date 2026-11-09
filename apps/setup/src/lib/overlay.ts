// Which overlay a window shows, from the label its Rust side gave it.

import { OVERLAY_LABELS, type OverlayModeDef } from './bindings.ts';

/** Whether `name` is an overlay mode. */
function isMode(name: string): name is OverlayModeDef {
  return Object.hasOwn(OVERLAY_LABELS, name);
}

/** The application and mode an overlay window's `label` names, or null for the main window. */
export function overlayOf(label: string): { readonly app: string; readonly mode: OverlayModeDef } | null {
  for (const [mode, prefix] of Object.entries(OVERLAY_LABELS)) {
    if (isMode(mode) && label.startsWith(prefix) && label.length > prefix.length) {
      return { app: label.slice(prefix.length), mode };
    }
  }
  return null;
}
