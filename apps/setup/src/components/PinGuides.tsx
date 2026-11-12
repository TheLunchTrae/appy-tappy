// What a region keeps its distance from as the window changes size, drawn over the screenshot: a line from the
// region to each edge, or to the window's middle, with a mark where it lands. It takes no pointer events, so the
// region under it is drawn and dragged as before.

import type { PinDef } from '../lib/bindings.ts';
import { type Guide, pinGuides } from '../lib/pin.ts';
import type { Rect, Size } from '../lib/region.ts';

/** Half the length of the mark at a guide's window end, in shown pixels. */
const MARK = 14;
/** How far inside the picture a mark on the window's edge is drawn, in shown pixels. */
const INSET = 2;

interface Props {
  /** The screenshot's own size, which the region is measured in. */
  readonly size: Size;
  /** The size the screenshot shows at. */
  readonly shown: Size;
  readonly region: Rect;
  readonly pin: PinDef;
}

export function PinGuides({ size, shown, region, pin }: Props) {
  if (size.width <= 0 || size.height <= 0) {
    return null;
  }
  const scaleX = shown.width / size.width;
  const scaleY = shown.height / size.height;
  const { across, down } = pinGuides(region, size, pin);
  // A guide and the mark across its window end. A mark on the window's middle is dashed, as no edge is there.
  const draw = (guide: Guide, direction: 'across' | 'down', middle: boolean) => {
    const from = { x: guide.from.x * scaleX, y: guide.from.y * scaleY };
    const to = { x: guide.to.x * scaleX, y: guide.to.y * scaleY };
    // Kept inside the picture, so a mark on the window's edge shows whole.
    const x = Math.min(Math.max(from.x, INSET), shown.width - INSET);
    const y = Math.min(Math.max(from.y, INSET), shown.height - INSET);
    const mark =
      direction === 'across'
        ? { x1: x, y1: from.y - MARK, x2: x, y2: from.y + MARK }
        : { x1: from.x - MARK, y1: y, x2: from.x + MARK, y2: y };
    return (
      <g key={direction}>
        <line className="guide" x1={from.x} y1={from.y} x2={to.x} y2={to.y} />
        <line className={middle ? 'mark middle' : 'mark'} {...mark} />
      </g>
    );
  };
  return (
    <svg className="pin-guides" width={shown.width} height={shown.height} aria-hidden="true">
      {draw(across, 'across', across.from.x > 0 && across.from.x < size.width)}
      {draw(down, 'down', down.from.y > 0 && down.from.y < size.height)}
    </svg>
  );
}
