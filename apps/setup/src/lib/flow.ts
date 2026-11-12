// A flow opened for another flow, which shows again when the opened one ends.

/** A flow that can open the application flow for the application it needs. */
export interface TakesApplication {
  /** The application the flow starts with, or null to let the user choose. */
  readonly app: string | null;
}

/**
 * What shows when a flow opened for `opener` ends: `opener` again, starting with the application `made` when the
 * flow saved one, or as it was when the flow was canceled. Null, for the page, when no flow opened it.
 */
export function resume<Opener extends TakesApplication>(opener: Opener | null, made: string | null): Opener | null {
  if (opener === null || made === null) {
    return opener;
  }
  return { ...opener, app: made };
}
