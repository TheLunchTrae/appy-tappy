// Where the focus goes when what held it is gone: a question's answers, or a deleted row's buttons.

import { type RefObject, useCallback, useEffect, useRef, useState } from 'react';

/**
 * Whether the focus is free for `place` to take: nothing holds it, as after the element that held it went, or it is
 * inside `place` already. Focus the user put elsewhere is theirs to keep.
 */
export function focusIsFree(place: HTMLElement | null = null): boolean {
  const active = document.activeElement;
  return active === null || active === document.body || (place?.contains(active) ?? false);
}

/** What {@link useRowGone} gives a page. */
interface RowGone<T extends HTMLElement> {
  /** For the page's heading, which needs `tabIndex={-1}` to take the focus. */
  readonly heading: RefObject<T | null>;
  /** Tells the page that a row was deleted, once the page has read its files again. */
  readonly rowGone: () => void;
}

/**
 * Focus for a page whose rows can be deleted: once a deleted row is gone, the heading takes the focus that the row's
 * buttons held. Focus that is elsewhere by then, as in another row's question, stays where it is.
 */
export function useRowGone<T extends HTMLElement>(): RowGone<T> {
  const heading = useRef<T>(null);
  const [gone, setGone] = useState(0);
  useEffect(() => {
    // The effect runs once the page has drawn without the row, so the focus its buttons held has fallen away.
    if (gone > 0 && focusIsFree()) {
      heading.current?.focus();
    }
  }, [gone]);
  const rowGone = useCallback(() => {
    setGone((count) => count + 1);
  }, []);
  return { heading, rowGone };
}
