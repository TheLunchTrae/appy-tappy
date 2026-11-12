// A flow opened for another flow, which shows again when the opened one ends.

/** A flow that can open the application flow for the application it needs. */
export interface TakesApplication {
  /** The application the flow starts with, or null to let the user choose. */
  readonly app: string | null;
}

/**
 * `flow` as the flow the application flow is opened for, starting with `chosen`, the application its picker shows
 * chosen, so that it shows again as the user left it.
 */
export function openerWith<Opener extends TakesApplication>(flow: Opener, chosen: string | null): Opener {
  return { ...flow, app: chosen };
}

/** How the application flow ended: it saved the application with this ID, or the user canceled it. */
export type Ended = { readonly kind: 'saved'; readonly id: string } | { readonly kind: 'canceled' };

/**
 * What the window shows once the application flow ended: nothing else than it shows now, the flow that opened
 * it, the Applications page that lists what it saved, or the page under it.
 */
export type After<Opener> =
  | { readonly kind: 'nothing' }
  | { readonly kind: 'flow'; readonly flow: Opener }
  | { readonly kind: 'applications' }
  | { readonly kind: 'page' };

/**
 * What follows the application flow opened for `opener`, or from a page when `opener` is null. `open` says the
 * flow is still the one the window shows: a save that finishes after the user left it for another changes nothing.
 * The opener shows again with the saved application chosen, or as it was left when the flow was canceled.
 */
export function afterApplication<Opener extends TakesApplication>(
  opener: Opener | null,
  ended: Ended,
  open: boolean,
): After<Opener> {
  if (!open) {
    return { kind: 'nothing' };
  }
  if (opener === null) {
    return { kind: ended.kind === 'saved' ? 'applications' : 'page' };
  }
  return { kind: 'flow', flow: ended.kind === 'saved' ? { ...opener, app: ended.id } : opener };
}
