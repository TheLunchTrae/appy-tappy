// How a value reads as its application's window changes size: the question each application answers, the side of
// the window each value stays at, and the fixes that draw a value again at one size.

import type { FixDef, PinDef, RegionDef, ScreenDef, SizeChangesDef, SizeDef } from './bindings.ts';

/** The answers to how often an application's window changes size, the recommended one first. */
export const SIZE_CHANGES: readonly SizeChangesDef[] = ['often', 'rarely'];

/** Each answer's name, what it means for the values, and a note that recommends it. */
export const SIZE_CHANGE_TEXT: Readonly<
  Record<SizeChangesDef, { readonly name: string; readonly description: string; readonly note: string | null }>
> = {
  often: {
    name: 'Often',
    description: "Values scale with the window. Fix a size only where a value doesn't read.",
    note: 'Recommended for windowed play',
  },
  rarely: {
    name: 'Rarely',
    description:
      'Values are read only at sizes you drew them at. Nothing is guessed, and a new size asks you to draw it.',
    note: null,
  },
};

/** The hint beside the size-change question. */
export const SIZE_CHANGES_HINT =
  'Windowed applications are often resized, and their interface grows with the window. Pick Rarely for one that ' +
  'always runs full screen at one size.';

/** The hint beside the Stays at picker. */
export const PIN_HINT =
  'The side of the window this value keeps its distance from as the window changes size. A health bar in the ' +
  'bottom left stays at the bottom left.';

/** The hint beside Fix at this size. */
export const FIX_HINT =
  "Draw the value again at the window's size now. It is read there exactly, instead of scaled from its first drawing.";

/** The hint beside the Values page's column for the window's size now. */
export const NOW_HINT =
  "What each value reads at the window's size now, scaled from where it was drawn or read at its fix. A value " +
  'that reads wrong here can be fixed at this size, unless it was first drawn at this size. Edit that one ' +
  'instead.';

/** The hint beside Remove fix. */
export const REMOVE_FIX_HINT =
  "Forget the value's drawing at this size. It then reads here the way the application's window size question " +
  'says.';

/** The hint beside Read with, in the fix flow of a number. */
export const READ_WITH_HINT =
  'Characters often look different at another size. Learn a reference at this size if the shared one misreads.';

/** The nine sides a region can stay at, row by row from the top left, as the picker lays them out. */
export const PINS: readonly PinDef[] = [
  'topLeft',
  'topCenter',
  'topRight',
  'middleLeft',
  'center',
  'middleRight',
  'bottomLeft',
  'bottomCenter',
  'bottomRight',
];

/** Each side's name, in sentence case. */
export const PIN_NAMES: Readonly<Record<PinDef, string>> = {
  topLeft: 'Top left',
  topCenter: 'Top center',
  topRight: 'Top right',
  middleLeft: 'Middle left',
  center: 'Center',
  middleRight: 'Middle right',
  bottomLeft: 'Bottom left',
  bottomCenter: 'Bottom center',
  bottomRight: 'Bottom right',
};

/** Which third of `length` a region from `start` that is `size` long has its center in: 0, 1 or 2. */
function third(start: number, size: number, length: number): 0 | 1 | 2 {
  // Three times twice the center against twice the length keeps every number whole.
  const thirds = (2 * start + size) * 3;
  if (thirds < 2 * length) {
    return 0;
  }
  return thirds < 4 * length ? 1 : 2;
}

/** The side a region drawn on a window of `window` stays at unless the user picks another: the third of the window
 * its center is in, across and down. AppyTappy guesses the same way for a value saved without one. */
export function guessPin(region: RegionDef, window: SizeDef): PinDef {
  const across = third(region.x, region.width, window.width);
  const down = third(region.y, region.height, window.height);
  return PINS[down * 3 + across] ?? 'topLeft';
}

/** The side `screen`'s region stays at: the one saved, or else the guess. */
export function pinOf(screen: ScreenDef): PinDef {
  return screen.pin ?? guessPin(screen.region, screen.windowSize);
}

/** A window size, such as `1920 × 1080`. */
export function sizeText(size: SizeDef): string {
  return `${size.width} × ${size.height}`;
}

/** Where a value is drawn and stays, with each size it is fixed at, such as `Drawn at 1920 × 1080, pinned bottom
 * left · fixed at 3440 × 1440`. */
export function regionText(screen: ScreenDef): string {
  const fixes = screen.fixes.map((fix) => ` · fixed at ${sizeText(fix.windowSize)}`).join('');
  return `Drawn at ${sizeText(screen.windowSize)}, pinned ${PIN_NAMES[pinOf(screen)].toLowerCase()}${fixes}`;
}

/** The two sizes are the same. */
export function sameSize(one: SizeDef, other: SizeDef): boolean {
  return one.width === other.width && one.height === other.height;
}

/** `screen`'s fix at `size`, if it has one. */
export function fixAt(screen: ScreenDef, size: SizeDef): FixDef | null {
  return screen.fixes.find((fix) => sameSize(fix.windowSize, size)) ?? null;
}

/** How a value reads on a window of `size`: where it was drawn, at its fix, scaled from its first drawing, or not at
 * all, since its application's window rarely changes size and it was not drawn there. */
export type Placement = 'drawn' | 'fixed' | 'scaled' | 'notDrawn';

/** How `screen` reads on a window of `size` while its application's window changes size as `sizeChanges` says. */
export function placement(screen: ScreenDef, sizeChanges: SizeChangesDef, size: SizeDef): Placement {
  if (sameSize(screen.windowSize, size)) {
    return 'drawn';
  }
  if (fixAt(screen, size) !== null) {
    return 'fixed';
  }
  return sizeChanges === 'often' ? 'scaled' : 'notDrawn';
}

/** Where a value that reads on a window of some size reads from: its first drawing, its fix there, or scaled. */
export type ReadFrom = Exclude<Placement, 'notDrawn'>;

/** What a value's cell in the Values page's column for the window's size now shows. Each but `waiting` and
 * `notDrawn` says where it reads from, since a fix at this size can only be made where it was not first drawn. */
export type NowCell =
  /** Its reading. */
  | { readonly kind: 'reading'; readonly text: string; readonly from: ReadFrom }
  /** No frame has been read yet. */
  | { readonly kind: 'waiting' }
  /** A state check whose region looks like none of its states. */
  | { readonly kind: 'noState'; readonly from: ReadFrom }
  /** It cannot be read at this size, and why. */
  | { readonly kind: 'unread'; readonly problem: string; readonly from: ReadFrom }
  /** It is read only at sizes it was drawn at, and this is not one. */
  | { readonly kind: 'notDrawn' };

/**
 * What the Values page shows for `screen` on a window of `size` while its application's window changes size as
 * `sizeChanges` says: `reading`, its reading there, the state it reads as text, or `problem`, why it has none.
 */
export function nowCell(
  screen: ScreenDef,
  sizeChanges: SizeChangesDef,
  size: SizeDef,
  reading: { readonly text: string; readonly noState: boolean } | null,
  problem: string | null,
): NowCell {
  const from = placement(screen, sizeChanges, size);
  if (from === 'notDrawn') {
    return { kind: 'notDrawn' };
  }
  if (reading !== null) {
    return reading.noState ? { kind: 'noState', from } : { kind: 'reading', text: reading.text, from };
  }
  return problem === null ? { kind: 'waiting' } : { kind: 'unread', problem, from };
}

/** Whether the cell offers Fix at this size: for a value that is not drawn here, or that reads here, wrongly or not
 * at all, from a scaled drawing or a fix, which a new fix replaces. At the size it was first drawn at, a value that
 * reads wrong is edited instead. */
export function offersFix(cell: NowCell): boolean {
  switch (cell.kind) {
    case 'waiting':
      return false;
    case 'notDrawn':
      return true;
    case 'reading':
    case 'noState':
    case 'unread':
      return cell.from !== 'drawn';
  }
}
