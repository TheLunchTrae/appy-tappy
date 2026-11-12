// What a value can do about content that moves around inside the window: a number or a bar is read wherever it is
// inside a larger reading area, and the other checks need their content to stay put. Nothing follows content; this
// is apart from the window changing size, which the value's pin covers.

import type { CheckKind } from './check.ts';

/** The hint beside This content may move. */
export const MOVING_HINT =
  'Turn this on when the value shows in different places inside the window, such as a number that shifts as it ' +
  'gains digits. AppyTappy then reads one larger area, the same every time; it does not follow the value around. ' +
  'A window that changes size is something else, which Stays at covers.';

/** What the screenshot says under it while a reading area is drawn. */
export const AREA_DRAW_HINT =
  'Drag to draw the reading area around everywhere the value can show. Drag inside it to move it, or drag a ' +
  'corner to resize it.';

/** How a value's region is drawn and read, given whether its content may move. */
export type MovingContent =
  /** The content stays put: the region is drawn around the value. */
  | { readonly kind: 'fixed' }
  /** The check reads its content wherever it is in the region, so the region is drawn as a larger reading area. */
  | { readonly kind: 'area'; readonly guidance: string; readonly limits: string }
  /** The check cannot read content that moves, and `why` says what to do instead. */
  | { readonly kind: 'unsupported'; readonly why: string };

const EVERY_PLACE = 'Make a value for each place it can show instead.';

/** How a `kind` check's region is drawn and read while its content may move, as `mayMove` says. */
export function movingContent(kind: CheckKind, mayMove: boolean): MovingContent {
  if (!mayMove) {
    return { kind: 'fixed' };
  }
  switch (kind) {
    case 'number':
      return {
        kind: 'area',
        guidance:
          'Draw the box around everywhere the number can show. AppyTappy reads the number wherever it is inside ' +
          'the box.',
        limits:
          "It reads everything in the box that has the characters' color, so keep other text and shapes of that " +
          'color out. It does not look outside the box.',
      };
    case 'barFill':
      return {
        kind: 'area',
        guidance:
          "Draw the box around everywhere the bar can show. AppyTappy counts the bar's filled and empty colors " +
          'wherever they are inside the box.',
        limits:
          'Anything else in the box that has either color counts as part of the bar, so keep it out. It does not ' +
          'look outside the box.',
      };
    case 'color':
      return {
        kind: 'unsupported',
        why: `A Color check averages its whole region, so the color has to fill the region wherever it is read. ${EVERY_PLACE}`,
      };
    case 'state':
      return {
        kind: 'unsupported',
        why: `A State check compares its region pixel by pixel with each state's screenshot, so the thing has to stay in the same place in the region. ${EVERY_PLACE}`,
      };
  }
}

/** What the region is called on the screenshot: the value's name as typed, and whether it is a reading area. */
export function regionLabel(valueName: string, moving: MovingContent): string {
  const name = valueName.trim();
  if (moving.kind !== 'area') {
    return name === '' ? 'Region' : name;
  }
  return name === '' ? 'Reading area' : `${name} reading area`;
}
