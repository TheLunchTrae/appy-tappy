import type { OverlayModeDef } from './bindings.ts';
// Which overlay a window shows, from the label its Rust side gave it.

/** The application and mode an overlay window's `label` names, or null for the main window. */
export function overlayOf(label: string): { readonly app: string; readonly mode: OverlayModeDef } | null {
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
