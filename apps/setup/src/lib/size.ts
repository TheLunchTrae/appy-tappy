import { type RefObject, useEffect, useState } from 'react';

import type { Size } from './region.ts';

/** The element's content size, kept current as it resizes. */
export function useSize(element: RefObject<HTMLElement | null>): Size {
  const [size, setSize] = useState<Size>({ width: 0, height: 0 });
  useEffect(() => {
    const target = element.current;
    if (target === null) {
      return undefined;
    }
    const observer = new ResizeObserver(([entry]) => {
      if (entry !== undefined) {
        const { width, height } = entry.contentRect;
        setSize((current) => (current.width === width && current.height === height ? current : { width, height }));
      }
    });
    observer.observe(target);
    return () => {
      observer.disconnect();
    };
  }, [element]);
  return size;
}
