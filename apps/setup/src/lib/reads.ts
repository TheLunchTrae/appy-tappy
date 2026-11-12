// The order of the reads of the saved lists. Reads overlap: the pages read every few seconds and after each save.

/**
 * Whether the answer of the read numbered `read` is taken, when `applied` is the number of the newest read whose
 * answer was. Reads are numbered as they start, so an answer that arrives after a newer one is dropped rather than
 * put older lists back.
 */
export function supersedes(read: number, applied: number): boolean {
  return read > applied;
}
