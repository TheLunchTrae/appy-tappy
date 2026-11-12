import type { CaptureOutline } from './bindings.ts';

/** `count` and `noun`, with an s when it is not one. */
function counted(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? '' : 's'}`;
}

/** What the capture outline shows, in one sentence or two. */
export function outlineSummary(outline: CaptureOutline): string {
  const watched = outline.values.filter((value) => value.watched);
  if (watched.length === 0) {
    return 'No enabled automation watches a value of this application, so the runtime copies nothing at this window size.';
  }
  const reading = watched.filter((value) => value.region !== null).length;
  if (outline.copies.length === 0 || reading === 0) {
    return 'No watched value reads at this window size, so the runtime copies nothing here.';
  }
  return (
    `At this window size the runtime copies ${counted(outline.copies.length, 'rectangle')} for ` +
    `${counted(reading, 'watched value')}. Dashed outlines are the copies, solid ones the watched values, and faint ` +
    'ones the values no enabled automation watches.'
  );
}
