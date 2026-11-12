// The matching pixels preview: which pixels of the screenshot shown a color setting takes, as the setup app's own
// readers decide, tinted over the screenshot while the user picks the color and moves its tolerance.

import type { Layer, MatchRule, MatchView } from './bindings.ts';
import type { Draft } from './check.ts';
import type { Color } from './pixels.ts';
import { type Rect, type Size, sameRect } from './region.ts';

/** The color settings that decide pixel by pixel: a reference's characters, a bar's two colors, a color check's. */
export type PreviewTarget = MatchRule['kind'];

/** What the preview asks the setup app: the pixels of `area` on a screenshot that `rule` takes. */
export interface PreviewRequest {
  readonly screenshot: number;
  /** In the screenshot's pixels. */
  readonly area: Rect;
  readonly rule: MatchRule;
}

/** What the preview can ask now, or why it has nothing to show. */
export type Plan =
  | { readonly kind: 'ask'; readonly request: PreviewRequest }
  | { readonly kind: 'blocked'; readonly why: string };

/** The parts of a draft the preview reads. */
export type PreviewDraft = Pick<Draft, 'ink' | 'tolerance' | 'filled' | 'empty' | 'color' | 'region'>;

const DRAW_FIRST = 'Draw the region on the screenshot to see its matching pixels.';

/**
 * What to ask for `target` of `draft` on the screenshot shown, `selected`, whose size is `size` once its pixels
 * arrived. A reference's characters are looked for on the whole screenshot, to show what else has their color; a
 * bar's and a color check's pixels in the region, which is all the check reads.
 */
export function plan(target: PreviewTarget, draft: PreviewDraft, selected: number | null, size: Size | null): Plan {
  if (selected === null) {
    return { kind: 'blocked', why: 'Matching pixels show on a screenshot. Select one, or take one.' };
  }
  const blocked = (why: string): Plan => ({ kind: 'blocked', why });
  const ask = (area: Rect, rule: MatchRule): Plan => ({ kind: 'ask', request: { screenshot: selected, area, rule } });
  switch (target) {
    case 'ink':
      if (draft.ink === null) {
        return blocked("Pick the characters' color to see its matching pixels.");
      }
      if (size === null) {
        return blocked('Waiting for the screenshot…');
      }
      return ask(
        { x: 0, y: 0, width: size.width, height: size.height },
        { kind: 'ink', ink: draft.ink, tolerance: draft.tolerance },
      );
    case 'barFill':
      if (draft.filled === null || draft.empty === null) {
        return blocked('Pick the filled and the empty color to see their matching pixels.');
      }
      if (draft.region === null) {
        return blocked(DRAW_FIRST);
      }
      return ask(draft.region, { kind: 'barFill', filled: draft.filled, empty: draft.empty });
    case 'color':
      if (draft.color === null) {
        return blocked('Pick the color to look for to see its matching pixels.');
      }
      if (draft.region === null) {
        return blocked(DRAW_FIRST);
      }
      return ask(draft.region, { kind: 'color', color: draft.color, tolerance: draft.tolerance });
  }
}

/** An answer and the request it answers. */
export interface Answered {
  readonly request: PreviewRequest;
  readonly view: MatchView;
}

/** The preview's requests and answers. */
export interface Preview {
  /** The number of the request whose answer is awaited, or null while none is. */
  readonly awaiting: number | null;
  /** The newest answer, kept while the next one is awaited. */
  readonly shown: Answered | null;
  /** Why the newest request failed. */
  readonly error: string | null;
}

/** The preview before anything is asked, and once it is turned off. */
export const NO_PREVIEW: Preview = { awaiting: null, shown: null, error: null };

export type PreviewEvent =
  /** Request `id` was made. */
  | { readonly kind: 'asked'; readonly id: number }
  | { readonly kind: 'answered'; readonly id: number; readonly request: PreviewRequest; readonly view: MatchView }
  | { readonly kind: 'failed'; readonly id: number; readonly message: string }
  /** The preview was turned off, or has nothing to ask. */
  | { readonly kind: 'cleared' };

/**
 * The preview after `event`. Only the newest request's answer or failure counts: one that carries another number,
 * or that arrives once the preview is cleared, changes nothing.
 */
export function step(preview: Preview, event: PreviewEvent): Preview {
  switch (event.kind) {
    case 'asked':
      return { ...preview, awaiting: event.id, error: null };
    case 'answered':
      return event.id === preview.awaiting
        ? { awaiting: null, shown: { request: event.request, view: event.view }, error: null }
        : preview;
    case 'failed':
      return event.id === preview.awaiting ? { awaiting: null, shown: null, error: event.message } : preview;
    case 'cleared':
      return NO_PREVIEW;
  }
}

/**
 * The answer to tint over the screenshot while `request` is the one to show, or null. An answer for the same
 * rectangle of the same screenshot stays while a new color or tolerance is asked about, so the tint does not flicker
 * under a dragged slider; one for another screenshot or rectangle would tint the wrong pixels, and goes at once.
 */
export function tinted(preview: Preview, request: PreviewRequest | null): Answered | null {
  const { shown } = preview;
  if (request === null || shown === null) {
    return null;
  }
  const same =
    shown.request.screenshot === request.screenshot &&
    shown.request.rule.kind === request.rule.kind &&
    sameRect(shown.request.area, request.area);
  return same ? shown : null;
}

/** The layers of `view`, in the order {@link tintsFor} gives their tints. */
export function layersOf(view: MatchView): readonly Layer[] {
  switch (view.kind) {
    case 'ink':
      return [view.ink];
    case 'barFill':
      return [view.filled, view.empty];
    case 'color':
      return [view.near];
  }
}

/** How a layer's pixels are tinted. */
export interface Tint {
  readonly color: Color;
  /** Whether only diagonal stripes are tinted, to tell a second layer from the first without its color. */
  readonly striped: boolean;
}

const MAGENTA: Color = { red: 255, green: 0, blue: 255 };
const GREEN: Color = { red: 0, green: 255, blue: 0 };

function apart(a: Color, b: Color): number {
  return (a.red - b.red) ** 2 + (a.green - b.green) ** 2 + (a.blue - b.blue) ** 2;
}

/** Magenta and green, the one that stands out more against `picked` first. */
function contrasting(picked: Color): readonly [Color, Color] {
  return apart(picked, MAGENTA) >= apart(picked, GREEN) ? [MAGENTA, GREEN] : [GREEN, MAGENTA];
}

/**
 * The tint of each layer of `rule`'s answer: whichever of magenta and green stands out more against the picked
 * color. A bar's empty pixels take the other one, in stripes.
 */
export function tintsFor(rule: MatchRule): readonly Tint[] {
  switch (rule.kind) {
    case 'ink':
      return [{ color: contrasting(rule.ink)[0], striped: false }];
    case 'color':
      return [{ color: contrasting(rule.color)[0], striped: false }];
    case 'barFill': {
      const [first, second] = contrasting(rule.filled);
      return [
        { color: first, striped: false },
        { color: second, striped: true },
      ];
    }
  }
}

/** How opaque a tint is, out of 255: enough to stand out, with the screenshot still showing through. */
const TINT_ALPHA = 150;
/** How opaque a stripe is; the pixels between stripes stay clear. */
const STRIPE_ALPHA = 215;
/** A stripe covers two of every four diagonals. */
const STRIPE_PERIOD = 4;

/** `color` at `alpha` as the 32-bit number that sets one pixel of RGBA bytes through a `Uint32Array`. */
function packed(color: Color, alpha: number): number {
  return new Uint32Array(new Uint8Array([color.red, color.green, color.blue, alpha]).buffer)[0] ?? 0;
}

/**
 * The RGBA pixels that tint `layers` over a `width`-wide rectangle, each with its entry in `tints`: clear where no
 * layer has a pixel. A layer with too many runs to draw is left clear.
 */
export function paint(
  layers: readonly Layer[],
  tints: readonly Tint[],
  width: number,
  height: number,
): Uint8ClampedArray<ArrayBuffer> {
  const pixels = new Uint32Array(width * height);
  layers.forEach((layer, index) => {
    const tint = tints[index];
    if (tint === undefined || layer.spans === null) {
      return;
    }
    if (!tint.striped) {
      const solid = packed(tint.color, TINT_ALPHA);
      for (const [start, end] of layer.spans) {
        pixels.fill(solid, start, end);
      }
      return;
    }
    const stripe = packed(tint.color, STRIPE_ALPHA);
    for (const [start, end] of layer.spans) {
      for (let at = start; at < end && at < pixels.length; at += 1) {
        if ((Math.floor(at / width) + (at % width)) % STRIPE_PERIOD < STRIPE_PERIOD / 2) {
          pixels[at] = stripe;
        }
      }
    }
  });
  return new Uint8ClampedArray(pixels.buffer);
}

/**
 * How many of a layer's pixels lie in `region`, when its `spans` count within `area`; both rectangles are in the
 * screenshot's pixels.
 */
export function countIn(spans: readonly (readonly [number, number])[], area: Rect, region: Rect): number {
  const left = Math.max(region.x - area.x, 0);
  const right = Math.min(region.x + region.width - area.x, area.width);
  const top = Math.max(region.y - area.y, 0);
  const bottom = Math.min(region.y + region.height - area.y, area.height);
  if (left >= right || top >= bottom) {
    return 0;
  }
  let count = 0;
  for (const [start, end] of spans) {
    const first = Math.max(Math.floor(start / area.width), top);
    const last = Math.min(Math.floor((end - 1) / area.width), bottom - 1);
    for (let row = first; row <= last; row += 1) {
      const from = Math.max(start - row * area.width, left);
      const to = Math.min(end - row * area.width, right);
      count += Math.max(to - from, 0);
    }
  }
  return count;
}

function pixelsText(count: number): string {
  return count === 1 ? '1 pixel' : `${count.toLocaleString('en-US')} pixels`;
}

/** `count` of `total` as a percentage that reads 0% and 100% only when it is none and all of them. */
export function share(count: number, total: number): string {
  if (count <= 0 || total <= 0) {
    return '0%';
  }
  if (count >= total) {
    return '100%';
  }
  const percent = (count / total) * 100;
  if (percent < 1) {
    return 'under 1%';
  }
  return percent > 99 ? 'over 99%' : `${Math.round(percent)}%`;
}

const TOO_MANY = 'Too many separate areas match to tint them all, so none is tinted.';

function area(rect: Rect): number {
  return rect.width * rect.height;
}

/** Whether `inner` lies inside `outer`. */
function holds(outer: Rect, inner: Rect): boolean {
  return (
    inner.x >= outer.x &&
    inner.y >= outer.y &&
    inner.x + inner.width <= outer.x + outer.width &&
    inner.y + inner.height <= outer.y + outer.height
  );
}

/** How a count of a region's pixels reads. */
function ofRegion(count: number, total: number): string {
  return `${count.toLocaleString('en-US')} of ${pixelsText(total)} in the region (${share(count, total)})`;
}

/**
 * What a reference's tint shows, in words: how many pixels count as its characters in `region`, the draft's region,
 * and on the rest of the screenshot. A region that reaches past the screenshot shown is left out.
 */
export function inkLines(layer: Layer, asked: Rect, region: Rect | null): readonly string[] {
  const total = area(asked);
  if (layer.spans === null) {
    return [
      `${TOO_MANY} ${layer.count.toLocaleString('en-US')} of ${pixelsText(total)} on the screenshot (${share(layer.count, total)}) count as characters. Lower the tolerance.`,
    ];
  }
  if (layer.count === 0) {
    return ['No pixel on the screenshot counts as characters. Raise the tolerance, or pick the color again.'];
  }
  if (region === null || !holds(asked, region)) {
    return [
      `${layer.count.toLocaleString('en-US')} of ${pixelsText(total)} on the screenshot (${share(layer.count, total)}) count as characters.`,
    ];
  }
  const inside = countIn(layer.spans, asked, region);
  const elsewhere = `${pixelsText(layer.count - inside)} elsewhere on the screenshot`;
  const within = area(region);
  if (inside === 0) {
    return [`No pixel in the region counts as characters; ${elsewhere} ${layer.count === 1 ? 'does' : 'do'}.`];
  }
  const counted = `${ofRegion(inside, within)} count as characters`;
  const rest = layer.count === inside ? 'none elsewhere on the screenshot' : elsewhere;
  return inside === within
    ? [`Every pixel in the region counts as characters, and ${rest}. Lower the tolerance until only the characters do.`]
    : [`${counted}, and ${rest}.`];
}

/** One line of a bar's legend: a kind of pixel, how many there are, and its tint, or null for pixels left clear. */
export interface LegendEntry {
  readonly name: string;
  readonly count: string;
  readonly tint: Tint | null;
}

/** A bar's legend: its filled, empty, and other pixels in the region. */
export function barLegend(
  filled: Layer,
  empty: Layer,
  asked: Rect,
  tints: readonly Tint[],
): { readonly entries: readonly LegendEntry[]; readonly note: string | null } {
  const total = area(asked);
  const entry = (name: string, count: number, tint: Tint | null): LegendEntry => ({
    name,
    count: `${count.toLocaleString('en-US')} (${share(count, total)})`,
    tint,
  });
  const neither = total - filled.count - empty.count;
  const entries = [
    entry('Filled', filled.count, tints[0] ?? null),
    entry('Empty', empty.count, tints[1] ?? null),
    entry('Neither, left out', neither, null),
  ];
  if (filled.count + empty.count === 0) {
    return { entries, note: 'No pixel in the region is near the filled or the empty color. Pick the colors again.' };
  }
  return { entries, note: filled.spans === null || empty.spans === null ? TOO_MANY : null };
}

/** What a color check's tint shows, in words: how many of the region's pixels are within the tolerance alone. */
export function nearLine(layer: Layer, asked: Rect): string {
  const total = area(asked);
  if (layer.count === 0) {
    return 'No pixel in the region is within the tolerance on its own.';
  }
  if (layer.count === total) {
    return 'Every pixel in the region is within the tolerance on its own.';
  }
  const counted = `${ofRegion(layer.count, total)} ${layer.count === 1 ? 'is' : 'are'} within the tolerance on ${layer.count === 1 ? 'its' : 'their'} own.`;
  return layer.spans === null ? `${TOO_MANY} ${counted}` : counted;
}
